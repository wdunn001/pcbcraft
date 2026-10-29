//! Paste stencil and board cut-path exporters for laser galvo software.

use crate::dxf::{dxf_document, DxfWriter};
use pcbcraft_doc::{Footprint, Pad, Project};
use pcbcraft_geom::Point;
use pcbcraft_library::PadShape;
use serde::{Deserialize, Serialize};

/// One closed aperture or cut ring as millimetre vertices (first point repeats at the end).
pub type Ring = Vec<Point>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StencilSide {
    Top,
    Bottom,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PathFormat {
    Svg,
    Dxf,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Reduction {
    Percent(f64),
    Millimetres(f64),
    FromRules,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StencilFrame {
    pub margin_mm: f64,
    pub hole_diameter_mm: f64,
    pub hole_inset_mm: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StencilOptions {
    pub side: StencilSide,
    pub reduction: Reduction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame: Option<StencilFrame>,
    #[serde(default)]
    pub kerf_mm: f64,
    #[serde(default)]
    pub mirror: bool,
    pub format: PathFormat,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CutPathOptions {
    #[serde(default)]
    pub kerf_mm: f64,
    #[serde(default = "cutout_default")]
    pub cutouts: bool,
    pub format: PathFormat,
}

fn cutout_default() -> bool {
    true
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StencilSkipped {
    pub reference: String,
    pub pad: String,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StencilApertures {
    pub rings: Vec<Ring>,
    pub skipped: Vec<StencilSkipped>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CutPaths {
    pub outline: Ring,
    pub cutouts: Vec<Ring>,
    /// FR4 is not cut on a 1064 nm fiber laser; these paths suit CO2, diode cutters and stencil stock.
    pub note: String,
}

pub const CUT_PATH_FIBER_NOTE: &str =
    "FR4 is not cut on a fiber laser at 1064 nm; use these paths for CO2 or diode cutters, stencil frame stock, or placement reference only.";

pub fn validate_stencil_options(o: &StencilOptions) -> Result<(), String> {
    match o.reduction {
        Reduction::Percent(p) if !(0.0..=50.0).contains(&p) => {
            return Err("stencil reduction percent must be 0 to 50".into());
        }
        Reduction::Millimetres(m) if !(0.0..=0.5).contains(&m) => {
            return Err("stencil reduction millimetres must be 0 to 0.5".into());
        }
        _ => {}
    }
    if !(0.0..=0.5).contains(&o.kerf_mm) {
        return Err("stencil kerf must be 0 to 0.5 mm".into());
    }
    if let Some(frame) = o.frame {
        if !(5.0..=100.0).contains(&frame.margin_mm) {
            return Err("stencil frame margin must be 5 to 100 mm".into());
        }
        if !(0.1..=20.0).contains(&frame.hole_diameter_mm) {
            return Err("stencil frame hole diameter is out of range".into());
        }
        if !(0.0..=50.0).contains(&frame.hole_inset_mm) {
            return Err("stencil frame hole inset is out of range".into());
        }
    }
    Ok(())
}

pub fn validate_cut_path_options(o: &CutPathOptions) -> Result<(), String> {
    if !(0.0..=0.5).contains(&o.kerf_mm) {
        return Err("cut path kerf must be 0 to 0.5 mm".into());
    }
    Ok(())
}

fn paste_layer(side: StencilSide) -> &'static str {
    match side {
        StencilSide::Top => "F.Paste",
        StencilSide::Bottom => "B.Paste",
    }
}

fn board_bounds(p: &Project) -> (Point, Point) {
    let edge = board_edge_xy(p);
    let mut min = Point::new(f64::INFINITY, f64::INFINITY);
    let mut max = Point::new(f64::NEG_INFINITY, f64::NEG_INFINITY);
    for (x, y) in edge {
        min.x = min.x.min(x);
        min.y = min.y.min(y);
        max.x = max.x.max(x);
        max.y = max.y.max(y);
    }
    if !min.x.is_finite() {
        return (Point::default(), Point::new(p.board.width, p.board.height));
    }
    (min, max)
}

fn board_edge_xy(p: &Project) -> Vec<(f64, f64)> {
    match &p.board.outline {
        Some(o) => o.iter().map(|pt| (pt.x, pt.y)).collect(),
        None => vec![
            (0.0, 0.0),
            (p.board.width, 0.0),
            (p.board.width, p.board.height),
            (0.0, p.board.height),
        ],
    }
}

fn reference_for(p: &Project, footprint: &Footprint) -> String {
    p.component(footprint.component)
        .map(|c| c.reference.clone())
        .unwrap_or_else(|| format!("U{}", footprint.component))
}

fn paste_margin_rules(p: &Project, footprint: &Footprint) -> Option<f64> {
    p.component(footprint.component)
        .and_then(|c| c.library_footprint.as_ref())
        .and_then(|lf| lf.definition.rules.as_ref())
        .and_then(|r| r.paste_margin)
}

fn pad_on_paste_side(geometry: &pcbcraft_library::FootprintPad, side: StencilSide) -> bool {
    let layer = paste_layer(side);
    geometry.layers.iter().any(|l| l == layer)
}

fn characteristic_size(pad: &Pad, geometry: Option<&pcbcraft_library::FootprintPad>) -> f64 {
    if let Some(g) = geometry {
        g.size.x.min(g.size.y)
    } else {
        pad.diameter
    }
}

fn reduction_inset(reduction: Reduction, size: f64, rules_margin: Option<f64>) -> f64 {
    match reduction {
        Reduction::Percent(p) => size * (p / 100.0) / 2.0,
        Reduction::Millimetres(m) => m,
        Reduction::FromRules => -rules_margin.unwrap_or(0.0),
    }
}

fn shrink_pad_geometry(
    pad: &Pad,
    inset: f64,
) -> Result<Option<pcbcraft_library::FootprintPad>, String> {
    if inset <= 0.0 {
        return Ok(pad.geometry.clone());
    }
    let Some(g) = &pad.geometry else {
        let d = pad.diameter - 2.0 * inset;
        return Ok(if d > 0.0 {
            Some(pcbcraft_library::FootprintPad {
                number: pad.number.clone(),
                pad_type: "smd".into(),
                shape: "circle".into(),
                position: pad.offset,
                rotation: 0.0,
                size: Point::new(d, d),
                layers: vec!["F.Paste".into()],
                drill: None,
                roundrect_ratio: None,
                chamfer: None,
                polygon: vec![],
                raw_source: String::new(),
            })
        } else {
            None
        });
    };
    if g.pad_type != "smd" {
        return Ok(None);
    }
    match g.shape_kind() {
        PadShape::Circle => {
            let d = g.size.x.min(g.size.y) - 2.0 * inset;
            if d <= 0.0 {
                return Ok(None);
            }
            let mut h = g.clone();
            h.size = Point::new(d, d);
            Ok(Some(h))
        }
        PadShape::Oval | PadShape::Rect | PadShape::RoundRect { .. } => {
            let nx = g.size.x - 2.0 * inset;
            let ny = g.size.y - 2.0 * inset;
            if nx <= 0.0 || ny <= 0.0 {
                return Ok(None);
            }
            let mut h = g.clone();
            h.size = Point::new(nx, ny);
            Ok(Some(h))
        }
        PadShape::Chamfered(_) | PadShape::Custom { .. } | PadShape::Unsupported => Ok(None),
    }
}

fn pad_ring(footprint: &Footprint, pad: &Pad, geometry: &pcbcraft_library::FootprintPad) -> Ring {
    let mut temp = pad.clone();
    temp.geometry = Some(geometry.clone());
    temp.outline_points()
        .into_iter()
        .map(|pt| footprint.position.offset(pt.rotate(-footprint.rotation)))
        .collect()
}

fn mirror_x(points: &mut [Point], centre_x: f64) {
    for p in points {
        p.x = 2.0 * centre_x - p.x;
    }
}

pub fn stencil_apertures(p: &Project, options: &StencilOptions) -> Result<StencilApertures, String> {
    p.validate()?;
    validate_stencil_options(options)?;
    let paste = paste_layer(options.side);
    let kerf_inset = options.kerf_mm / 2.0;
    let (min, max) = board_bounds(p);
    let centre_x = (min.x + max.x) / 2.0;
    let mut rings = Vec::new();
    let mut skipped = Vec::new();
    for footprint in &p.board.footprints {
        let reference = reference_for(p, footprint);
        let rules_margin = paste_margin_rules(p, footprint);
        for pad in &footprint.pads {
            let Some(geometry) = pad.geometry.as_ref() else {
                continue;
            };
            if geometry.pad_type != "smd" || !pad_on_paste_side(geometry, options.side) {
                continue;
            }
            if !geometry.layers.iter().any(|l| l == paste) {
                continue;
            }
            let size = characteristic_size(pad, Some(geometry));
            let reduction = reduction_inset(options.reduction, size, rules_margin);
            let inset = reduction + kerf_inset;
            let shrunk = match shrink_pad_geometry(pad, inset)? {
                Some(g) => g,
                None => {
                    skipped.push(StencilSkipped {
                        reference: reference.clone(),
                        pad: pad.number.clone(),
                        reason: if matches!(
                            geometry.shape_kind(),
                            PadShape::Chamfered(_) | PadShape::Custom { .. }
                        ) {
                            "chamfered and custom pads need polygon offset".into()
                        } else {
                            "reduction exceeds pad size".into()
                        },
                    });
                    continue;
                }
            };
            let mut ring = pad_ring(footprint, pad, &shrunk);
            if options.mirror {
                mirror_x(&mut ring, centre_x);
            }
            rings.push(ring);
        }
    }
    Ok(StencilApertures { rings, skipped })
}

fn frame_geometry(
    p: &Project,
    frame: StencilFrame,
) -> Result<(Ring, Vec<Ring>), String> {
    let (min, max) = board_bounds(p);
    let x0 = min.x - frame.margin_mm;
    let y0 = min.y - frame.margin_mm;
    let x1 = max.x + frame.margin_mm;
    let y1 = max.y + frame.margin_mm;
    let outline = vec![
        Point::new(x0, y0),
        Point::new(x1, y0),
        Point::new(x1, y1),
        Point::new(x0, y1),
        Point::new(x0, y0),
    ];
    let r = frame.hole_diameter_mm / 2.0;
    let inset = frame.hole_inset_mm;
    let holes = vec![
        circle_ring(Point::new(x0 + inset, y0 + inset), r),
        circle_ring(Point::new(x1 - inset, y0 + inset), r),
        circle_ring(Point::new(x1 - inset, y1 - inset), r),
        circle_ring(Point::new(x0 + inset, y1 - inset), r),
    ];
    Ok((outline, holes))
}

fn circle_ring(center: Point, radius: f64) -> Ring {
    const SEGMENTS: usize = 32;
    let mut ring = Vec::with_capacity(SEGMENTS + 1);
    for i in 0..=SEGMENTS {
        let t = (i as f64 / SEGMENTS as f64) * std::f64::consts::TAU;
        ring.push(Point::new(
            center.x + radius * t.cos(),
            center.y + radius * t.sin(),
        ));
    }
    ring
}

fn reference_outline(p: &Project) -> Ring {
    board_edge_xy(p)
        .into_iter()
        .map(|(x, y)| Point::new(x, y))
        .chain(std::iter::once({
            let edge = board_edge_xy(p);
            edge.first()
                .map(|(x, y)| Point::new(*x, *y))
                .unwrap_or_default()
        }))
        .collect()
}

fn bounds_of_rings(rings: &[Ring]) -> (Point, Point) {
    let mut min = Point::new(f64::INFINITY, f64::INFINITY);
    let mut max = Point::new(f64::NEG_INFINITY, f64::NEG_INFINITY);
    for ring in rings {
        for p in ring {
            min.x = min.x.min(p.x);
            min.y = min.y.min(p.y);
            max.x = max.x.max(p.x);
            max.y = max.y.max(p.y);
        }
    }
    if !min.x.is_finite() {
        return (Point::default(), Point::new(1.0, 1.0));
    }
    (min, max)
}

fn mm_num(v: f64) -> String {
    let r = (v * 1000.0).round() / 1000.0;
    let r = if r == 0.0 { 0.0 } else { r };
    r.to_string()
}

fn ring_to_svg_path(ring: &Ring) -> String {
    let Some(first) = ring.first() else {
        return String::new();
    };
    let mut d = format!("M {} {}", mm_num(first.x), mm_num(first.y));
    for p in ring.iter().skip(1) {
        d.push_str(&format!(" L {} {}", mm_num(p.x), mm_num(p.y)));
    }
    if ring.len() > 2 {
        d.push_str(" Z");
    }
    d
}

fn laser_svg_document(min: Point, max: Point, body: &str) -> String {
    let width = (max.x - min.x).max(0.001);
    let height = (max.y - min.y).max(0.001);
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}mm\" height=\"{h}mm\" viewBox=\"{min_x} {min_y} {w} {h}\">\n{body}\n</svg>\n",
        w = mm_num(width),
        h = mm_num(height),
        min_x = mm_num(min.x),
        min_y = mm_num(min.y),
        body = body
    )
}

pub fn stencil_svg(p: &Project, options: &StencilOptions) -> Result<String, String> {
    let apertures = stencil_apertures(p, options)?;
    let mut groups = String::new();
    groups.push_str("<g id=\"apertures\" fill=\"none\" stroke=\"black\" stroke-width=\"0.05\">\n");
    for ring in &apertures.rings {
        groups.push_str(&format!(
            "<path d=\"{}\"/>\n",
            ring_to_svg_path(ring)
        ));
    }
    groups.push_str("</g>\n");
    let mut extra: Vec<Ring> = Vec::new();
    if let Some(frame) = options.frame {
        let (outline, holes) = frame_geometry(p, frame)?;
        groups.push_str("<g id=\"frame\" fill=\"none\" stroke=\"black\" stroke-width=\"0.05\">\n");
        groups.push_str(&format!(
            "<path d=\"{}\"/>\n",
            ring_to_svg_path(&outline)
        ));
        for hole in &holes {
            groups.push_str(&format!(
                "<path d=\"{}\"/>\n",
                ring_to_svg_path(hole)
            ));
        }
        groups.push_str("</g>\n");
        extra.push(outline);
        extra.extend(holes);
    }
    groups.push_str("<g id=\"reference\" fill=\"none\" stroke=\"#888\" stroke-width=\"0.05\">\n");
    let reference = reference_outline(p);
    groups.push_str(&format!(
        "<path d=\"{}\"/>\n",
        ring_to_svg_path(&reference)
    ));
    groups.push_str("</g>\n");
    extra.push(reference);
    let (mut min, mut max) = bounds_of_rings(&apertures.rings);
    let (emin, emax) = bounds_of_rings(&extra);
    min.x = min.x.min(emin.x);
    min.y = min.y.min(emin.y);
    max.x = max.x.max(emax.x);
    max.y = max.y.max(emax.y);
    Ok(laser_svg_document(min, max, &groups))
}

pub fn stencil_dxf(p: &Project, options: &StencilOptions) -> Result<String, String> {
    let apertures = stencil_apertures(p, options)?;
    let mut w = DxfWriter::new();
    for ring in &apertures.rings {
        write_ring_dxf(&mut w, "apertures", ring)?;
    }
    if let Some(frame) = options.frame {
        let (outline, holes) = frame_geometry(p, frame)?;
        write_ring_dxf(&mut w, "frame", &outline)?;
        for hole in holes {
            write_ring_dxf(&mut w, "frame", &hole)?;
        }
    }
    let reference = reference_outline(p);
    write_ring_dxf(&mut w, "reference", &reference)?;
    Ok(dxf_document(&w))
}

fn write_ring_dxf(w: &mut DxfWriter, layer: &str, ring: &Ring) -> Result<(), String> {
    if ring.len() < 2 {
        return Ok(());
    }
    let points: Vec<(f64, f64)> = ring
        .iter()
        .map(|p| (p.x, p.y))
        .collect();
    let closed = ring.len() > 2;
    w.polyline(layer, &points, closed)
}

pub fn cut_paths(p: &Project, options: &CutPathOptions) -> Result<CutPaths, String> {
    p.validate()?;
    validate_cut_path_options(options)?;
    let half = options.kerf_mm / 2.0;
    let edge = board_edge_xy(p);
    let (cx, cy) = edge_centroid(&edge);
    let outline: Ring = edge
        .iter()
        .map(|(x, y)| {
            let dx = x - cx;
            let dy = y - cy;
            let dist = (dx * dx + dy * dy).sqrt();
            if dist < 1e-9 {
                Point::new(*x, *y)
            } else {
                let scale = (dist + half) / dist;
                Point::new(cx + dx * scale, cy + dy * scale)
            }
        })
        .chain(edge.first().map(|(x, y)| {
            let dx = x - cx;
            let dy = y - cy;
            let dist = (dx * dx + dy * dy).sqrt();
            if dist < 1e-9 {
                Point::new(*x, *y)
            } else {
                let scale = (dist + half) / dist;
                Point::new(cx + dx * scale, cy + dy * scale)
            }
        }))
        .collect();
    let mut cutouts = Vec::new();
    if options.cutouts {
        for c in &p.board.cutouts {
            let r = c.radius - half;
            if r > 0.0 {
                cutouts.push(circle_ring(c.center, r));
            }
        }
    }
    Ok(CutPaths {
        outline,
        cutouts,
        note: CUT_PATH_FIBER_NOTE.into(),
    })
}

fn edge_centroid(edge: &[(f64, f64)]) -> (f64, f64) {
    if edge.is_empty() {
        return (0.0, 0.0);
    }
    let n = edge.len() as f64;
    let sx: f64 = edge.iter().map(|(x, _)| x).sum();
    let sy: f64 = edge.iter().map(|(_, y)| y).sum();
    (sx / n, sy / n)
}

pub fn cut_paths_svg(p: &Project, options: &CutPathOptions) -> Result<String, String> {
    let paths = cut_paths(p, options)?;
    let mut body = format!("<!-- {note} -->\n", note = paths.note);
    body.push_str("<g id=\"outline\" fill=\"none\" stroke=\"black\" stroke-width=\"0.05\">\n");
    body.push_str(&format!(
        "<path d=\"{}\"/>\n",
        ring_to_svg_path(&paths.outline)
    ));
    body.push_str("</g>\n");
    if !paths.cutouts.is_empty() {
        body.push_str("<g id=\"cutouts\" fill=\"none\" stroke=\"black\" stroke-width=\"0.05\">\n");
        for ring in &paths.cutouts {
            body.push_str(&format!(
                "<path d=\"{}\"/>\n",
                ring_to_svg_path(ring)
            ));
        }
        body.push_str("</g>\n");
    }
    let mut rings = vec![paths.outline];
    rings.extend(paths.cutouts);
    let (min, max) = bounds_of_rings(&rings);
    Ok(laser_svg_document(min, max, &body))
}

pub fn cut_paths_dxf(p: &Project, options: &CutPathOptions) -> Result<String, String> {
    let paths = cut_paths(p, options)?;
    let mut w = DxfWriter::new();
    write_ring_dxf(&mut w, "outline", &paths.outline)?;
    for ring in &paths.cutouts {
        write_ring_dxf(&mut w, "cutouts", ring)?;
    }
    Ok(dxf_document(&w))
}

pub fn stencil_export_text(p: &Project, options: &StencilOptions) -> Result<String, String> {
    match options.format {
        PathFormat::Svg => stencil_svg(p, options),
        PathFormat::Dxf => stencil_dxf(p, options),
    }
}

pub fn cut_path_export_text(p: &Project, options: &CutPathOptions) -> Result<String, String> {
    match options.format {
        PathFormat::Svg => cut_paths_svg(p, options),
        PathFormat::Dxf => cut_paths_dxf(p, options),
    }
}
