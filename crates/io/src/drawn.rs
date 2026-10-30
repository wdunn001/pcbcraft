//! Per-layer machine plots for the drawn layers: cut and perforation layers
//! as mill paths, silk and mask layers as print layers, and one DXF of every
//! layer. Everything here works in board millimetres.
//!
//! Generates DXF files for each drawn layer (cut, perforation, silk, mask)
//! that can be sent directly to a CNC machine or printer. Each layer is
//! exported as a separate DXF file with appropriate layer names and
//! line weights.
//!
//! # Rules
//! - Coordinates are in board millimetres.
//! - Line weights are set to reasonable defaults for each layer type.
//! - Output is DXF R12 format for maximum compatibility.
//!
//! # Dependencies
//! - crates/io/src/lib.rs DxfWriter
//! - crates/doc/src/lib.rs ArtKind, ArtLayer, ArtPath, Id, Project
//! - crates/geom/src/lib.rs Point

use super::DxfWriter;
use pcbcraft_doc::{ArtKind, ArtLayer, ArtPath, Id, Project};
use pcbcraft_geom::Point;
use std::collections::{BTreeMap, BTreeSet};

/// Stroke width of a silk plot, in millimetres: a common screen minimum.
const SILK_APERTURE_MM: f64 = 0.15;
/// Stroke width of a mask plot, in millimetres.
const MASK_APERTURE_MM: f64 = 0.20;
/// Stroke width of a reference drawing, in millimetres.
const OTHER_APERTURE_MM: f64 = 0.10;
/// Plotter units per millimetre in the pen path file, so a plot is 1:1.
const HPGL_UNITS_PER_MM: f64 = 40.0;
/// Most files one board may plot.
const MAX_PLOT_FILES: usize = 96;
/// Most dots one dotted layer may carry.
const MAX_PLOT_DOTS: usize = 20_000;
/// Margin around a layer drawing, in millimetres.
const PAGE_MARGIN_MM: f64 = 1.0;

/// The plotted files for every drawn layer, keyed by file name, plus the
/// figures left out because a machine cannot follow them.
#[derive(Clone, Debug, Default)]
pub struct DrawnLayerPlots {
    /// One entry per file, ready to write beside the other outputs.
    pub files: BTreeMap<String, String>,
    /// Figures that no file carries, each named for the person reading the
    /// job result.
    pub skipped: Vec<String>,
}
impl DrawnLayerPlots {
    /// One sentence naming what was left out, or nothing when every figure
    /// was plotted.
    pub fn skipped_note(&self) -> Option<String> {
        if self.skipped.is_empty() {
            None
        } else {
            Some(format!(
                "{} figure(s) skipped: {}",
                self.skipped.len(),
                self.skipped.join("; ")
            ))
        }
    }
}

/// Reduce a layer name to a safe file stem: ASCII letters and digits kept,
/// every other run of bytes turned into one dash, case lowered. An empty
/// result falls back to the layer id so a file name always exists and can
/// never leave the output folder.
fn file_stem(name: &str, id: Id) -> String {
    let mut out = String::new();
    let mut dash = false;
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        return format!("layer-{id}");
    }
    out.truncate(48);
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        format!("layer-{id}")
    } else {
        out
    }
}

/// A file stem for every drawn layer that has figures, unique across the
/// board. The first layer keeps the plain stem; a later layer whose name
/// slugs to the same stem gets its id appended.
fn stems(p: &Project) -> BTreeMap<Id, String> {
    let mut taken: BTreeSet<String> = BTreeSet::new();
    let mut out = BTreeMap::new();
    for layer in &p.board.user_layers {
        if layer.shapes.is_empty() {
            continue;
        }
        let mut stem = file_stem(&layer.name, layer.id);
        if taken.contains(&stem) {
            stem = format!("{stem}-{}", layer.id);
        }
        taken.insert(stem.clone());
        out.insert(layer.id, stem);
    }
    out
}

/// Gerber coordinates: millimetres scaled to 4.6 format, so one unit is a
/// nanometre.
fn gerber_units(value: f64) -> i64 {
    (value * 1_000_000.0).round() as i64
}

/// The per-figure note for a skipped figure.
fn skipped_note(layer: &ArtLayer, shape: &pcbcraft_doc::ArtShape) -> String {
    match shape {
        pcbcraft_doc::ArtShape::Text { text, .. } => {
            let short: String = text.chars().take(24).collect();
            format!("text \"{short}\" on layer {}", layer.name)
        }
        _ => format!("figure on layer {}", layer.name),
    }
}

/// One Gerber file for a silk, mask or reference layer. Every drawn layer is
/// top side, so silk is a legend and a mask or reference layer is written as
/// a positive other layer rather than a negative soldermask image.
pub(crate) fn gerber_layer_text(layer: &ArtLayer) -> Result<String, String> {
    let aperture = match layer.kind {
        ArtKind::Silk => SILK_APERTURE_MM,
        ArtKind::Mask => MASK_APERTURE_MM,
        _ => OTHER_APERTURE_MM,
    };
    let mut out = String::new();
    out.push_str(&format!(
        "G04 {} drawn layer \"{}\", top side, generated by PCBCraft *\n",
        layer.kind.label(),
        layer.name
    ));
    match layer.kind {
        ArtKind::Silk => out.push_str("%TF.FileFunction,Legend,Top*%\n"),
        _ => {
            let safe: String = layer
                .name
                .chars()
                .map(|c| {
                    if c.is_ascii_graphic() && c != '*' {
                        c
                    } else {
                        '_'
                    }
                })
                .take(48)
                .collect();
            out.push_str(&format!("%TF.FileFunction,Other,{safe}*%\n"));
            out.push_str("%TF.FilePolarity,Positive*%\n");
        }
    }
    out.push_str("%FSLAX46Y46*%\n");
    out.push_str("%MOMM*%\n");
    out.push_str(&format!("%ADD10C,{aperture:.6}*%\n"));
    out.push_str("D10*\nG01*\n");
    let dotted = layer.dotted || layer.kind == ArtKind::Perforation;
    if dotted {
        out.push_str(&format!("%ADD11C,{:.6}*%\n", layer.dot_size));
    }
    let mut figures = 0usize;
    for shape in &layer.shapes {
        if matches!(shape, pcbcraft_doc::ArtShape::Text { .. }) {
            continue;
        }
        figures += 1;
        if figures > MAX_PLOT_DOTS {
            return Err("a drawn layer holds too many figures to plot".into());
        }
        if let Some((center, radius)) = shape.circle()
            && !dotted
        {
            out.push_str(&format!(
                "X{}Y{}D02*\nG75*\nG03*\nX{}Y{}I{}J0D01*\nG01*\n",
                gerber_units(center.x - radius),
                gerber_units(center.y),
                gerber_units(center.x - radius),
                gerber_units(center.y),
                gerber_units(radius)
            ));
            continue;
        }
        for path in shape.paths() {
            if dotted {
                for dot in path.dots(layer.dot_pitch) {
                    out.push_str(&format!(
                        "D11*\nX{}Y{}D03*\n",
                        gerber_units(dot.x),
                        gerber_units(dot.y)
                    ));
                }
                continue;
            }
            if path.points.len() < 2 {
                continue;
            }
            let first = path.points[0];
            out.push_str(&format!(
                "D10*\nX{}Y{}D02*\n",
                gerber_units(first.x),
                gerber_units(first.y)
            ));
            for point in path.points.iter().skip(1) {
                out.push_str(&format!(
                    "X{}Y{}D01*\n",
                    gerber_units(point.x),
                    gerber_units(point.y)
                ));
            }
            if path.closed {
                out.push_str(&format!(
                    "X{}Y{}D01*\n",
                    gerber_units(first.x),
                    gerber_units(first.y)
                ));
            }
        }
    }
    out.push_str("M02*\n");
    Ok(out)
}

/// One DXF R12 file for a layer, in millimetres. Circles stay CIRCLE
/// entities, polygons stay LWPOLYLINE, and text stays a TEXT entity so laser
/// and printer software can convert it itself.
fn dxf_layer_text(layer: &ArtLayer, name: &str) -> Result<String, String> {
    let mut w = DxfWriter::new();
    write_dxf_layer(layer, name, &mut w)?;
    Ok(w.finish())
}

/// Write one layer's figures into a shared DXF drawing.
pub(crate) fn write_dxf_layer(
    layer: &ArtLayer,
    name: &str,
    w: &mut DxfWriter,
) -> Result<(), String> {
    let dotted = layer.dotted || layer.kind == ArtKind::Perforation;
    for shape in &layer.shapes {
        if let pcbcraft_doc::ArtShape::Text {
            at,
            text,
            size,
            rotate,
        } = shape
        {
            if !dotted {
                w.text(name, at.x, at.y, *size, *rotate, text)?;
            }
            continue;
        }
        if let Some((center, radius)) = shape.circle()
            && !dotted
        {
            w.circle(name, center.x, center.y, radius)?;
            continue;
        }
        for path in shape.paths() {
            if dotted {
                for dot in path.dots(layer.dot_pitch) {
                    w.circle(name, dot.x, dot.y, layer.dot_size / 2.0)?;
                }
                continue;
            }
            if path.points.len() < 2 {
                continue;
            }
            let points: Vec<(f64, f64)> = path.points.iter().map(|q| (q.x, q.y)).collect();
            w.polyline(name, &points, path.closed)?;
        }
    }
    Ok(())
}

/// How far a layer's drawing reaches, in millimetres.
fn layer_bounds(layer: &ArtLayer) -> Option<(f64, f64, f64, f64)> {
    let mut bounds: Option<(f64, f64, f64, f64)> = None;
    let mut add = |x: f64, y: f64| {
        bounds = Some(match bounds {
            None => (x, y, x, y),
            Some((x0, y0, x1, y1)) => (x0.min(x), y0.min(y), x1.max(x), y1.max(y)),
        });
    };
    for shape in &layer.shapes {
        if let Some((center, radius)) = shape.circle() {
            add(center.x - radius, center.y - radius);
            add(center.x + radius, center.y + radius);
            continue;
        }
        for path in shape.paths() {
            for point in &path.points {
                add(point.x, point.y);
            }
        }
        if let pcbcraft_doc::ArtShape::Text { at, size, .. } = shape {
            add(at.x, at.y);
            add(at.x + size, at.y + size);
        }
    }
    bounds
}

/// One millimetre SVG for a laser or printer. The page starts at the board
/// origin, cut lines are blue and perforations red, and every stroke is a
/// centreline at real scale.
fn svg_layer_text(layer: &ArtLayer) -> Result<String, String> {
    let Some((min_x, min_y, max_x, max_y)) = layer_bounds(layer) else {
        return Err("a drawn layer needs at least one figure to plot".into());
    };
    let width = (max_x.max(min_x) + PAGE_MARGIN_MM).max(1.0);
    let height = (max_y.max(min_y) + PAGE_MARGIN_MM).max(1.0);
    let (stroke, closed_fill) = match layer.kind {
        ArtKind::Cut => ("#0000ff", "none"),
        ArtKind::Perforation => ("#ff0000", "none"),
        ArtKind::Silk => ("#000000", "none"),
        ArtKind::Mask => ("#00a000", "none"),
        ArtKind::Other => ("#808080", "none"),
    };
    let width_mm = format!("{width:.3}mm");
    let height_mm = format!("{height:.3}mm");
    let mut s = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!-- {} drawn layer \"{}\", top side, millimetres, generated by PCBCraft -->\n<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width_mm}\" height=\"{height_mm}\" viewBox=\"0 0 {width:.3} {height:.3}\">\n",
        layer.kind.label(),
        xml_escape(&layer.name)
    );
    let style = format!("fill=\"{closed_fill}\" stroke=\"{stroke}\" stroke-width=\"0.1\"");
    let flip = |y: f64| height - y;
    let dotted = layer.dotted || layer.kind == ArtKind::Perforation;
    for shape in &layer.shapes {
        if let pcbcraft_doc::ArtShape::Text { at, text, size, .. } = shape {
            if !dotted {
                s.push_str(&format!(
                    "<text x=\"{:.3}mm\" y=\"{:.3}mm\" font-size=\"{:.3}mm\" stroke=\"none\" fill=\"{stroke}\">{}</text>\n",
                    at.x,
                    flip(at.y),
                    size,
                    xml_escape(text)
                ));
            }
            continue;
        }
        if let Some((center, radius)) = shape.circle()
            && !dotted
        {
            s.push_str(&format!(
                "<circle cx=\"{:.3}mm\" cy=\"{:.3}mm\" r=\"{:.3}mm\" {style}/>\n",
                center.x,
                flip(center.y),
                radius
            ));
            continue;
        }
        for path in shape.paths() {
            if dotted {
                for dot in path.dots(layer.dot_pitch) {
                    s.push_str(&format!(
                        "<circle cx=\"{:.3}mm\" cy=\"{:.3}mm\" r=\"{:.3}mm\" fill=\"{stroke}\" stroke=\"{stroke}\"/>\n",
                        dot.x,
                        flip(dot.y),
                        layer.dot_size / 2.0
                    ));
                }
                continue;
            }
            if path.points.len() < 2 {
                continue;
            }
            let points: Vec<String> = path
                .points
                .iter()
                .map(|q| format!("{:.3},{:.3}", q.x, flip(q.y)))
                .collect();
            s.push_str(&format!(
                "<{} points=\"{}\" {style}/>\n",
                if path.closed { "polygon" } else { "polyline" },
                points.join(" ")
            ));
        }
    }
    s.push_str("</svg>\n");
    Ok(s)
}

/// Escape the five characters an SVG text node cannot carry raw.
fn xml_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
    out
}

/// A real-scale pen path file for a mill, 40 plotter units per millimetre,
/// never scaled to a page. Dotted layers become pen dots at their pitch.
fn hpgl_layer_text(layer: &ArtLayer, stem: &str) -> Result<String, String> {
    let unit = |v: f64| (v * HPGL_UNITS_PER_MM).round() as i64;
    let mut s = format!(
        "IN;SP1;CO \"{} drawn layer {}, top side, 40 units per mm\";\n",
        layer.kind.label(),
        stem
    );
    let dotted = layer.dotted || layer.kind == ArtKind::Perforation;
    for shape in &layer.shapes {
        if matches!(shape, pcbcraft_doc::ArtShape::Text { .. }) {
            continue;
        }
        if let Some((center, radius)) = shape.circle()
            && !dotted
        {
            let ring = ArtPath {
                points: (0..64)
                    .map(|i| {
                        let a = std::f64::consts::TAU * i as f64 / 64.0;
                        Point::new(center.x + radius * a.cos(), center.y + radius * a.sin())
                    })
                    .collect(),
                closed: true,
            };
            s.push_str(&hpgl_path(&ring, &unit));
            continue;
        }
        for path in shape.paths() {
            if dotted {
                for dot in path.dots(layer.dot_pitch) {
                    s.push_str(&format!(
                        "PU{},{};PD{},{};\n",
                        unit(dot.x),
                        unit(dot.y),
                        unit(dot.x),
                        unit(dot.y)
                    ));
                }
                continue;
            }
            s.push_str(&hpgl_path(&path, &unit));
        }
    }
    s.push_str("SP0;\n");
    Ok(s)
}

/// One polyline as pen up, then pen down through its points.
fn hpgl_path(path: &ArtPath, unit: &impl Fn(f64) -> i64) -> String {
    if path.points.len() < 2 {
        return String::new();
    }
    let mut out = String::new();
    let mut ring = path.points.clone();
    if path.closed
        && let Some(first) = ring.first().copied()
    {
        ring.push(first);
    }
    out.push_str(&format!("PU{},{};\n", unit(ring[0].x), unit(ring[0].y)));
    out.push_str("PD");
    for (i, point) in ring.iter().skip(1).enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&format!("{},{}", unit(point.x), unit(point.y)));
    }
    out.push_str(";\n");
    out
}

/// Plot every drawn layer that has figures: a Gerber for silk and mask, a
/// DXF for every layer, and a real-scale SVG and pen path for cut and
/// perforation layers. Layers are top side, and the on-screen hidden flag
/// does not change machine output, so a hidden cut line still gets its file.
pub fn drawn_layer_plots(p: &Project) -> Result<DrawnLayerPlots, String> {
    p.validate()?;
    let stems = stems(p);
    let mut plots = DrawnLayerPlots::default();
    let mut add = |name: String, text: String| -> Result<(), String> {
        if plots.files.len() >= MAX_PLOT_FILES {
            return Err("a board plots more drawn layers than the file limit".into());
        }
        plots.files.insert(name, text);
        Ok(())
    };
    for layer in &p.board.user_layers {
        let Some(stem) = stems.get(&layer.id).cloned() else {
            continue;
        };
        for shape in &layer.shapes {
            if matches!(shape, pcbcraft_doc::ArtShape::Text { .. }) {
                plots.skipped.push(skipped_note(layer, shape));
            }
        }
        match layer.kind {
            ArtKind::Cut | ArtKind::Perforation => {
                add(format!("{stem}.dxf"), dxf_layer_text(layer, &stem)?)?;
                add(format!("{stem}.svg"), svg_layer_text(layer)?)?;
                add(format!("{stem}.hpg"), hpgl_layer_text(layer, &stem)?)?;
            }
            ArtKind::Silk | ArtKind::Mask => {
                add(format!("{stem}.gbr"), gerber_layer_text(layer)?)?;
                add(format!("{stem}.dxf"), dxf_layer_text(layer, &stem)?)?;
            }
            ArtKind::Other => {
                add(format!("{stem}.dxf"), dxf_layer_text(layer, &stem)?)?;
            }
        }
    }
    Ok(plots)
}

/// The drawn layers that a Gerber export carries: silk and mask prints and
/// reference drawings, one file each.
pub(crate) fn gerber_layer_files(p: &Project) -> Result<BTreeMap<String, String>, String> {
    let stems = stems(p);
    let mut files = BTreeMap::new();
    for layer in &p.board.user_layers {
        if !matches!(layer.kind, ArtKind::Silk | ArtKind::Mask | ArtKind::Other) {
            continue;
        }
        let Some(stem) = stems.get(&layer.id) else {
            continue;
        };
        if files.len() >= MAX_PLOT_FILES {
            return Err("a board plots more drawn layers than the file limit".into());
        }
        files.insert(format!("{stem}.gbr"), gerber_layer_text(layer)?);
    }
    Ok(files)
}

/// Write every drawn layer's figures into a DXF drawing, one DXF layer per
/// drawn layer named after the file stem.
pub(crate) fn write_dxf_layers(p: &Project, w: &mut DxfWriter) -> Result<(), String> {
    let stems = stems(p);
    for layer in &p.board.user_layers {
        let Some(stem) = stems.get(&layer.id) else {
            continue;
        };
        write_dxf_layer(layer, stem, w)?;
    }
    Ok(())
}
