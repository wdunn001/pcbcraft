//! RS-274X (Gerber X2 attributes) writers for one board side: copper flashes
//! and track draws, solder mask openings, stencil paste openings, and
//! silkscreen strokes. Zones, drills and the board outline live in
//! `gerber_files`.
//!
//! Circles use the standard C aperture. Axis-aligned rect and oval pads use R
//! and O. Every other pad (rotated rect/oval, roundrect, chamfered and custom
//! polygon pads) is flashed through an aperture macro with one outline
//! primitive (code 4) holding the pad's board-space outline relative to the
//! flash point. Chamfered and custom outlines are exact; roundrect corners are
//! chord approximated within `Pad::outline_error`.
//!
//! The copper writer states coordinates in millimetres y-up about the board
//! height. The mask, paste and silkscreen writers match the fabrication package
//! from `gerber_files`: y stays down from the top-left corner, and a bottom
//! side file mirrors x about the board origin.
use crate::board_mesh::{graphic_layer, map_ring, pad_sides, thin_polyline};
use pcbcraft_doc::{Footprint, Layer, Pad, Project};
use pcbcraft_geom::Point;
use pcbcraft_library::PadShape;
use std::collections::BTreeMap;

/// Solder mask opening growth in millimetres when the footprint rules state no
/// `mask_margin`. Fab houses quote an opening this much larger than the pad.
pub const DEFAULT_MASK_MARGIN: f64 = 0.05;
/// Stencil paste opening growth in millimetres when the footprint rules state
/// no `paste_margin`. Zero keeps the opening equal to the copper; a negative
/// margin shrinks it, as the library rule documents.
pub const DEFAULT_PASTE_MARGIN: f64 = 0.0;
/// Stroke width in millimetres for board art silk figures, matching the 3D
/// preview because the art layer records no width of its own.
const ART_SILK_WIDTH: f64 = 0.15;
/// Upper bound on the operations one written file may carry.
const MAX_FILE_OPS: usize = 50_000;
/// Upper bound on distinct apertures in one written file.
const MAX_FILE_APERTURES: usize = 64;

/// Millimetres to 4.6 fixed-point coordinate integers.
fn coordinate(value: f64) -> i64 {
    (value * 1_000_000.0).round() as i64
}
fn decimal(value: f64) -> String {
    let text = format!("{value:.6}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" || text.is_empty() {
        "0".into()
    } else {
        text.into()
    }
}

enum Aperture {
    Standard(String),
    Outline(Vec<Point>),
}
impl Aperture {
    fn key(&self) -> String {
        match self {
            Self::Standard(body) => body.clone(),
            Self::Outline(points) => points
                .iter()
                .map(|p| format!("{},{}", decimal(p.x), decimal(p.y)))
                .collect::<Vec<_>>()
                .join(","),
        }
    }
}

/// Gerber copper image of one layer. Board coordinates are y-down from the
/// top-left board corner; Gerber is y-up, so y is mirrored about the board height.
pub fn gerber_copper(p: &Project, layer: Layer) -> Result<String, String> {
    p.validate()?;
    let height = p.board.height;
    let to_gerber = |point: Point| Point::new(point.x, height - point.y);
    let mut codes: BTreeMap<String, usize> = BTreeMap::new();
    let mut definitions = String::new();
    let mut define = |aperture: Aperture| -> usize {
        let key = aperture.key();
        if let Some(code) = codes.get(&key) {
            return *code;
        }
        let code = 10 + codes.len();
        match aperture {
            Aperture::Standard(body) => definitions.push_str(&format!("%ADD{code}{body}*%\n")),
            Aperture::Outline(points) => {
                // Outline primitive: exposure, vertex count after the start
                // point, the closed vertex list, then rotation.
                let mut ring = points.clone();
                if let Some(first) = points.first().copied() {
                    ring.push(first);
                }
                definitions.push_str(&format!(
                    "%AMPAD{code}*\n4,1,{},{},0*%\n%ADD{code}PAD{code}*%\n",
                    ring.len().saturating_sub(1),
                    ring.iter()
                        .map(|v| format!("{},{}", decimal(v.x), decimal(v.y)))
                        .collect::<Vec<_>>()
                        .join(",")
                ));
            }
        }
        codes.insert(key, code);
        code
    };
    let mut body = String::new();
    let mut current: Option<usize> = None;
    let mut select = |body: &mut String, code: usize| {
        if current != Some(code) {
            body.push_str(&format!("D{code}*\n"));
            current = Some(code);
        }
    };
    for t in p.board.tracks.iter().filter(|t| t.layer == layer) {
        let code = define(Aperture::Standard(format!("C,{}", decimal(t.width))));
        select(&mut body, code);
        let (a, b) = (to_gerber(t.start), to_gerber(t.end));
        body.push_str(&format!(
            "X{}Y{}D02*\nX{}Y{}D01*\n",
            coordinate(a.x),
            coordinate(a.y),
            coordinate(b.x),
            coordinate(b.y)
        ));
    }
    for f in &p.board.footprints {
        for pad in &f.pads {
            let plated = pad
                .geometry
                .as_ref()
                .is_some_and(|g| g.pad_type == "thru_hole");
            if layer == Layer::Back && !plated {
                continue;
            }
            let Some(center) = f.pad_position(&pad.number) else {
                continue;
            };
            let rotation = f.rotation + pad.geometry.as_ref().map(|g| g.rotation).unwrap_or(0.0);
            let quarter = rotation.rem_euclid(90.0);
            let axis_aligned = quarter < 1e-9 || 90.0 - quarter < 1e-9;
            let swapped = (rotation.rem_euclid(180.0) - 90.0).abs() < 1e-9;
            let aperture = match pad.geometry.as_ref().map(|g| (g.shape_kind(), g.size)) {
                None => Aperture::Standard(format!("C,{}", decimal(pad.diameter))),
                Some((PadShape::Circle, size)) => {
                    Aperture::Standard(format!("C,{}", decimal(size.x)))
                }
                Some((kind @ (PadShape::Rect | PadShape::Oval), size)) if axis_aligned => {
                    let (w, h) = if swapped {
                        (size.y, size.x)
                    } else {
                        (size.x, size.y)
                    };
                    let letter = if kind == PadShape::Rect { "R" } else { "O" };
                    Aperture::Standard(format!("{letter},{}X{}", decimal(w), decimal(h)))
                }
                Some((PadShape::Unsupported, _)) => {
                    return Err(format!(
                        "pad {} has a shape the Gerber writer does not support",
                        pad.number
                    ));
                }
                Some(_) => {
                    let mut outline = pad.outline_points();
                    outline.pop();
                    let origin = to_gerber(center);
                    Aperture::Outline(
                        outline
                            .into_iter()
                            .map(|v| {
                                let board = to_gerber(f.position.offset(v.rotate(-f.rotation)));
                                Point::new(board.x - origin.x, board.y - origin.y)
                            })
                            .collect(),
                    )
                }
            };
            let code = define(aperture);
            select(&mut body, code);
            let at = to_gerber(center);
            body.push_str(&format!("X{}Y{}D03*\n", coordinate(at.x), coordinate(at.y)));
        }
    }
    let idx = layer.index();
    let index = idx + 1;
    let side = match idx {
        0 => "Top",
        1 => "Bot",
        _ => "Inr",
    };
    Ok(format!(
        "%TF.GenerationSoftware,Tangency Labs,PCBCraft*%\n%TF.FileFunction,Copper,L{index},{side}*%\n%TF.FilePolarity,Positive*%\n%FSLAX46Y46*%\n%MOMM*%\n%LPD*%\nG01*\n{definitions}{body}M02*\n"
    ))
}

/// Aperture shape for a mask or paste opening, in micrometres. Integer values
/// keep aperture identity exact, so two equal openings share one definition.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Opening {
    Circle(i64),
    Rect(i64, i64),
    Obround(i64, i64),
    Outline(Vec<(i64, i64)>),
}

/// One written Gerber file: X2 attributes, aperture definitions and operations.
struct Sheet {
    scale: f64,
    apertures: BTreeMap<(String, Opening), usize>,
    definitions: String,
    body: String,
    current: Option<usize>,
    ops: usize,
}

impl Sheet {
    fn new(scale: f64) -> Self {
        Self {
            scale,
            apertures: BTreeMap::new(),
            definitions: String::new(),
            body: String::new(),
            current: None,
            ops: 0,
        }
    }
    /// Scale a millimetre value into the file unit and print it without
    /// trailing zeros.
    fn value(&self, millimetres: f64) -> String {
        decimal(millimetres * self.scale)
    }
    /// Define the aperture, or reuse the code an equal aperture already holds.
    fn define(&mut self, opening: Opening, function: &'static str) -> Result<usize, String> {
        let key = (function.to_owned(), opening);
        if let Some(code) = self.apertures.get(&key) {
            return Ok(*code);
        }
        if self.apertures.len() >= MAX_FILE_APERTURES {
            return Err("a fabrication layer needs more than 64 apertures".into());
        }
        let code = 10 + self.apertures.len();
        let mut definition = format!("%TA.AperFunction,{function}*%\n");
        match &key.1 {
            Opening::Circle(diameter) => definition.push_str(&format!(
                "%ADD{code}C,{}*%\n",
                self.value(*diameter as f64 / 1_000.0)
            )),
            Opening::Rect(width, height) => definition.push_str(&format!(
                "%ADD{code}R,{}X{}*%\n",
                self.value(*width as f64 / 1_000.0),
                self.value(*height as f64 / 1_000.0)
            )),
            Opening::Obround(width, height) => definition.push_str(&format!(
                "%ADD{code}O,{}X{}*%\n",
                self.value(*width as f64 / 1_000.0),
                self.value(*height as f64 / 1_000.0)
            )),
            Opening::Outline(points) => {
                // Closed outline through the pad's vertices, then no rotation.
                let mut ring = points.clone();
                if let Some(first) = points.first().copied() {
                    ring.push(first);
                }
                definition.push_str(&format!(
                    "%AMPAD{code}*\n4,1,{},{},0*%\n%ADD{code}PAD{code}*%\n",
                    ring.len().saturating_sub(1),
                    ring.iter()
                        .map(|(x, y)| format!(
                            "{},{}",
                            self.value(*x as f64 / 1_000.0),
                            self.value(*y as f64 / 1_000.0)
                        ))
                        .collect::<Vec<_>>()
                        .join(",")
                ));
            }
        }
        definition.push_str("%TD*%\n");
        self.definitions.push_str(&definition);
        self.apertures.insert(key, code);
        Ok(code)
    }
    fn select(&mut self, code: usize) {
        if self.current != Some(code) {
            self.body.push_str(&format!("D{code}*\n"));
            self.current = Some(code);
        }
    }
    /// Fixed-point coordinate pair: 4.6 integers in the file unit.
    fn point(&self, at: Point) -> (i64, i64) {
        (coordinate(at.x * self.scale), coordinate(at.y * self.scale))
    }
    fn flash(&mut self, code: usize, at: Point) -> Result<(), String> {
        self.select(code);
        self.ops += 1;
        self.check_ops()?;
        let (x, y) = self.point(at);
        self.body.push_str(&format!("X{x}Y{y}D03*\n"));
        Ok(())
    }
    fn stroke(&mut self, code: usize, points: &[Point], closed: bool) -> Result<(), String> {
        if points.len() < 2 {
            return Ok(());
        }
        self.select(code);
        self.ops = self.ops.saturating_add(points.len());
        self.check_ops()?;
        for (index, point) in points.iter().enumerate() {
            let (x, y) = self.point(*point);
            self.body
                .push_str(&format!("X{x}Y{y}D0{}*\n", if index == 0 { 2 } else { 1 }));
        }
        if closed && let Some(first) = points.first() {
            let (x, y) = self.point(*first);
            self.body.push_str(&format!("X{x}Y{y}D01*\n"));
        }
        Ok(())
    }
    fn check_ops(&self) -> Result<(), String> {
        if self.ops > MAX_FILE_OPS {
            return Err("a fabrication layer exceeds the object limit".into());
        }
        Ok(())
    }
    /// Header plus aperture definitions plus operations. `notes` become G04
    /// comments after the format block so a reader can see what was left out.
    fn finish(
        &self,
        file_function: &str,
        polarity: &str,
        comment: &str,
        notes: &[String],
    ) -> String {
        let unit = if (self.scale - 1.0).abs() > f64::EPSILON {
            "MOIN"
        } else {
            "MOMM"
        };
        let mut out = format!(
            "G04 {comment} generated by PCBCraft *\n\
             %TF.GenerationSoftware,Tangency Labs,PCBCraft*%\n\
             %TF.FileFunction,{file_function}*%\n\
             %TF.FilePolarity,{polarity}*%\n\
             %TF.SameCoordinates,PCBCraftFabrication*%\n\
             %FSLAX46Y46*%\n\
             %{unit}*%\n"
        );
        for note in notes {
            out.push_str(&format!("G04 {note} *\n"));
        }
        out.push_str("%LPD*%\nG01*\n");
        out.push_str(&self.definitions);
        out.push_str(&self.body);
        out.push_str("M02*\n");
        out
    }
}

/// Fabrication files write inches for an inch project, like `gerber_files`.
fn file_scale(p: &Project) -> f64 {
    if p.unit == "inch" { 1.0 / 25.4 } else { 1.0 }
}

/// A bottom side file mirrors x about the board origin.
fn mirror(at: Point, layer: Layer) -> Point {
    match layer {
        Layer::Back => Point::new(-at.x, at.y),
        // Only the outer sides carry mask, paste and silk; an inner layer
        // never reaches here, and the top side is the harmless default.
        _ => at,
    }
}

/// Millimetres to whole micrometres, the unit the aperture keys use.
fn microns(millimetres: f64) -> i64 {
    (millimetres * 1_000.0).round() as i64
}

/// Whether a footprint's library rules state a usable mask or paste margin.
fn footprint_margin(p: &Project, f: &Footprint, mask: bool) -> Option<f64> {
    let rules = p
        .component(f.component)?
        .library_footprint
        .as_ref()?
        .definition
        .rules
        .as_ref()?;
    let value = if mask {
        rules.mask_margin?
    } else {
        rules.paste_margin?
    };
    if value.is_finite() && value.abs() <= 1.0 {
        Some(value)
    } else {
        None
    }
}

/// Whether a pad carries copper on the requested side.
fn pad_on_side(fp: &Footprint, pad: &Pad, layer: Layer) -> bool {
    let (front, back, _) = pad_sides(fp, pad);
    match layer {
        Layer::Back => back,
        // Only the outer sides carry mask, paste and silk; an inner layer
        // never reaches here, and the top side is the harmless default.
        _ => front,
    }
}

/// Whether a pad is surface mounted, and so takes paste. Pads with a drill and
/// legacy pads that carry only a diameter are through-hole and are skipped.
fn pad_is_smd(pad: &Pad) -> bool {
    match &pad.geometry {
        None => false,
        Some(g) => {
            g.drill.is_none()
                && !g.pad_type.starts_with("thru_hole")
                && !g.pad_type.starts_with("np_thru_hole")
        }
    }
}

/// Opening for one pad: the pad shape grown by `margin` on every side. A
/// negative margin shrinks it. Circle, axis-aligned rect and oval openings are
/// exact. Other outlines grow their bounding box by the margin, which matches
/// the rule within the margin at every edge for a plain rectangle and never
/// leaves an opening smaller than the pad itself.
fn pad_opening(f: &Footprint, pad: &Pad, margin: f64, mirror_x: bool) -> Result<Opening, String> {
    let Some(g) = &pad.geometry else {
        let diameter = (pad.diameter + 2.0 * margin).max(0.001);
        return Ok(Opening::Circle(microns(diameter)));
    };
    let rotation = f.rotation + g.rotation;
    let quarter = rotation.rem_euclid(90.0);
    let axis_aligned = quarter < 1e-9 || 90.0 - quarter < 1e-9;
    let swapped = (rotation.rem_euclid(180.0) - 90.0).abs() < 1e-9;
    match (g.shape_kind(), g.size) {
        (PadShape::Circle, size) => {
            Ok(Opening::Circle(microns((size.x + 2.0 * margin).max(0.001))))
        }
        (kind @ (PadShape::Rect | PadShape::Oval), size) if axis_aligned => {
            let (w, h) = if swapped {
                (size.y, size.x)
            } else {
                (size.x, size.y)
            };
            let (w, h) = (
                microns((w + 2.0 * margin).max(0.001)),
                microns((h + 2.0 * margin).max(0.001)),
            );
            if kind == PadShape::Rect {
                Ok(Opening::Rect(w, h))
            } else {
                Ok(Opening::Obround(w, h))
            }
        }
        (PadShape::Unsupported, _) => Err(format!(
            "pad {} has a shape the Gerber writer does not support",
            pad.number
        )),
        _ => {
            let mut outline = pad.outline_points();
            outline.pop();
            if outline.is_empty() {
                return Err(format!("pad {} has no outline to open", pad.number));
            }
            let grown = crate::board_mesh::grow(&outline, margin);
            // Outline points are footprint local, so the flash point is
            // removed before the vertices become aperture geometry.
            let points = grown
                .into_iter()
                .map(|v| {
                    let v = v
                        .offset(Point::new(-pad.offset.x, -pad.offset.y))
                        .rotate(-f.rotation);
                    let x = if mirror_x { -v.x } else { v.x };
                    (microns(x), microns(v.y))
                })
                .collect();
            Ok(Opening::Outline(points))
        }
    }
}

/// Solder mask file for one side: one opening per pad on that side and per via,
/// each grown by the footprint's `mask_margin` rule, or by
/// `DEFAULT_MASK_MARGIN` when unset. Pads without a library footprint, and all
/// vias, use the default because no footprint rule reaches them.
pub fn gerber_mask(p: &Project, layer: Layer) -> Result<String, String> {
    p.validate()?;
    let mirror_x = layer == Layer::Back;
    let mut sheet = Sheet::new(file_scale(p));
    for f in &p.board.footprints {
        let margin = footprint_margin(p, f, true).unwrap_or(DEFAULT_MASK_MARGIN);
        for pad in &f.pads {
            if !pad_on_side(f, pad, layer) {
                continue;
            }
            let Some(at) = f.pad_position(&pad.number) else {
                continue;
            };
            let opening = pad_opening(f, pad, margin, mirror_x)?;
            let code = sheet.define(opening, "SMDPad,CuDef")?;
            sheet.flash(code, mirror(at, layer))?;
        }
    }
    for v in &p.board.vias {
        let diameter = (v.diameter + 2.0 * DEFAULT_MASK_MARGIN).max(0.001);
        let code = sheet.define(Opening::Circle(microns(diameter)), "ViaPad,CuDef")?;
        sheet.flash(code, mirror(v.position, layer))?;
    }
    let (function, comment) = match layer {
        Layer::Back => ("Soldermask,Bot", "bottom solder mask"),
        // Only the outer sides carry mask, paste and silk; an inner layer
        // never reaches here, and the top side is the harmless default.
        _ => ("Soldermask,Top", "top solder mask"),
    };
    Ok(sheet.finish(function, "Negative", comment, &[]))
}

/// Stencil paste file for one side: one opening per surface-mounted pad on that
/// side, grown by the footprint's `paste_margin` rule or by
/// `DEFAULT_PASTE_MARGIN` when unset. A negative margin shrinks the opening, as
/// the library rule documents. Through-hole pads get no paste.
pub fn gerber_paste(p: &Project, layer: Layer) -> Result<String, String> {
    p.validate()?;
    let mirror_x = layer == Layer::Back;
    let mut sheet = Sheet::new(file_scale(p));
    for f in &p.board.footprints {
        let margin = footprint_margin(p, f, false).unwrap_or(DEFAULT_PASTE_MARGIN);
        for pad in &f.pads {
            if !pad_on_side(f, pad, layer) || !pad_is_smd(pad) {
                continue;
            }
            let Some(at) = f.pad_position(&pad.number) else {
                continue;
            };
            let opening = pad_opening(f, pad, margin, mirror_x)?;
            let code = sheet.define(opening, "SMDPad,CuDef")?;
            sheet.flash(code, mirror(at, layer))?;
        }
    }
    let (function, comment) = match layer {
        Layer::Back => ("Paste,Bot", "bottom paste stencil"),
        // Only the outer sides carry mask, paste and silk; an inner layer
        // never reaches here, and the top side is the harmless default.
        _ => ("Paste,Top", "top paste stencil"),
    };
    Ok(sheet.finish(function, "Positive", comment, &[]))
}

/// Silk opening boxes on one side, in board coordinates: every pad and via mask
/// opening grown by its margin, used to keep silk off exposed metal.
fn silk_openings(p: &Project, layer: Layer) -> Vec<(Point, Point)> {
    let mut boxes = Vec::new();
    for f in &p.board.footprints {
        let margin = footprint_margin(p, f, true).unwrap_or(DEFAULT_MASK_MARGIN);
        for pad in &f.pads {
            if !pad_on_side(f, pad, layer) {
                continue;
            }
            let outline: Vec<Point> = pad
                .outline_points()
                .into_iter()
                .map(|v| f.position.offset(v.rotate(-f.rotation)))
                .collect();
            let mut frame = bounds(&outline);
            frame.0 = Point::new(frame.0.x - margin, frame.0.y - margin);
            frame.1 = Point::new(frame.1.x + margin, frame.1.y + margin);
            boxes.push(frame);
        }
    }
    for v in &p.board.vias {
        let reach = v.diameter / 2.0 + DEFAULT_MASK_MARGIN;
        boxes.push((
            Point::new(v.position.x - reach, v.position.y - reach),
            Point::new(v.position.x + reach, v.position.y + reach),
        ));
    }
    boxes
}

/// Bounding box of a point list; the origin when the list is empty.
fn bounds(points: &[Point]) -> (Point, Point) {
    let mut low = Point::new(f64::MAX, f64::MAX);
    let mut high = Point::new(f64::MIN, f64::MIN);
    for point in points {
        low.x = low.x.min(point.x);
        low.y = low.y.min(point.y);
        high.x = high.x.max(point.x);
        high.y = high.y.max(point.y);
    }
    if points.is_empty() {
        (Point::default(), Point::default())
    } else {
        (low, high)
    }
}

fn boxes_overlap(a: &(Point, Point), b: &(Point, Point)) -> bool {
    a.0.x <= b.1.x && b.0.x <= a.1.x && a.0.y <= b.1.y && b.0.y <= a.1.y
}

/// A silk stroke in board coordinates: two ends and a width in millimetres.
struct Stroke {
    start: Point,
    end: Point,
    width: f64,
}

/// Close a ring so it strokes back to its first vertex.
fn closed(mut points: Vec<Point>) -> Vec<Point> {
    if let Some(first) = points.first().copied() {
        points.push(first);
    }
    points
}

/// Silkscreen file for one side: footprint silk graphics, board text anchors
/// that this writer cannot stroke, and the board's own drawn silk art. Strokes
/// whose bounding box meets a mask opening bounding box are dropped, which is a
/// simple conservative test: a stroke that merely shares a box corner with a
/// pad is also left out, and no stroke is clipped to a partial length. Curves
/// are stroked as short chords taken from the footprint's display path, so no
/// G75 arc commands are written. Text is not stroked; a G04 note in the file
/// records how many text items were left out because no stroke font ships with
/// this writer. Board art layers and board graphics carry no side of their own,
/// so they print on the top file only.
pub fn gerber_silk(p: &Project, layer: Layer) -> Result<String, String> {
    p.validate()?;
    let mut strokes: Vec<Stroke> = Vec::new();
    let mut dropped_text = 0_usize;
    for f in &p.board.footprints {
        let Some(library) = p
            .component(f.component)
            .and_then(|c| c.library_footprint.as_ref())
        else {
            continue;
        };
        for graphic in &library.definition.graphics {
            let Some((name, width)) = graphic_layer(graphic) else {
                dropped_text += 1;
                continue;
            };
            let on_top = name.starts_with('F');
            if !name.contains("Silk") || on_top != (layer == Layer::Front) {
                continue;
            }
            let path = thin_polyline(&map_ring(f, &graphic.path_points()), 96);
            for pair in path.windows(2) {
                strokes.push(Stroke {
                    start: pair[0],
                    end: pair[1],
                    width: width.clamp(0.05, 2.0),
                });
            }
        }
    }
    for text in &p.board.texts {
        if text.layer == layer {
            dropped_text += 1;
        }
    }
    if layer == Layer::Front {
        // Board art layers carry no side of their own, so they print on top.
        for art in p
            .board
            .user_layers
            .iter()
            .filter(|l| l.visible && l.kind == pcbcraft_doc::ArtKind::Silk)
        {
            for shape in &art.shapes {
                let path: Vec<Point> = match shape {
                    pcbcraft_doc::ArtShape::Line { start, end } => vec![*start, *end],
                    pcbcraft_doc::ArtShape::Rect {
                        x,
                        y,
                        width,
                        height,
                        rotate,
                    } => closed(
                        crate::board_mesh::rect_ring(0.0, 0.0, *width, *height)
                            .into_iter()
                            .map(|corner| {
                                Point::new(x + width / 2.0, y + height / 2.0)
                                    .offset(corner.rotate(*rotate))
                            })
                            .collect(),
                    ),
                    pcbcraft_doc::ArtShape::Circle { center, radius } => {
                        closed(crate::board_mesh::circle_ring(*center, *radius))
                    }
                    pcbcraft_doc::ArtShape::Poly { points } => closed(points.clone()),
                    pcbcraft_doc::ArtShape::Text { .. } => {
                        dropped_text += 1;
                        Vec::new()
                    }
                };
                for pair in path.windows(2) {
                    strokes.push(Stroke {
                        start: pair[0],
                        end: pair[1],
                        width: ART_SILK_WIDTH,
                    });
                }
            }
        }
    }
    // Board graphics print on the top file unless they name a sublayer.
    for graphic in p.graphics.iter().filter(|g| {
        g.layer == "board"
            && match g.sublayer.as_str() {
                "" | "F.SilkS" => layer == Layer::Front,
                "B.SilkS" => layer == Layer::Back,
                _ => false,
            }
    }) {
        match graphic.outline() {
            Some(path) => {
                let path = if graphic.shape == "line" {
                    path
                } else {
                    closed(path)
                };
                for pair in path.windows(2) {
                    strokes.push(Stroke {
                        start: pair[0],
                        end: pair[1],
                        width: graphic.width.clamp(0.05, 2.0),
                    });
                }
            }
            None => dropped_text += 1,
        }
    }
    let openings = silk_openings(p, layer);
    let mut sheet = Sheet::new(file_scale(p));
    for stroke in &strokes {
        if !stroke.start.x.is_finite()
            || !stroke.start.y.is_finite()
            || !stroke.end.x.is_finite()
            || !stroke.end.y.is_finite()
        {
            continue;
        }
        let half = stroke.width / 2.0;
        let frame = (
            Point::new(
                stroke.start.x.min(stroke.end.x) - half,
                stroke.start.y.min(stroke.end.y) - half,
            ),
            Point::new(
                stroke.start.x.max(stroke.end.x) + half,
                stroke.start.y.max(stroke.end.y) + half,
            ),
        );
        if openings
            .iter()
            .any(|opening| boxes_overlap(&frame, opening))
        {
            continue;
        }
        let code = sheet.define(Opening::Circle(microns(stroke.width.max(0.05))), "Legend")?;
        sheet.stroke(
            code,
            &[mirror(stroke.start, layer), mirror(stroke.end, layer)],
            false,
        )?;
    }
    let notes = if dropped_text > 0 {
        vec![format!(
            "{dropped_text} text items were left out because this writer has no stroke font"
        )]
    } else {
        Vec::new()
    };
    let (function, comment) = match layer {
        Layer::Back => ("Legend,Bot", "bottom silkscreen"),
        // Only the outer sides carry mask, paste and silk; an inner layer
        // never reaches here, and the top side is the harmless default.
        _ => ("Legend,Top", "top silkscreen"),
    };
    Ok(sheet.finish(function, "Positive", comment, &notes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pcbcraft_doc::{Component, LibraryFootprint, Net, Pin, Via};
    use pcbcraft_library::{
        Drill, FootprintDefinition, FootprintPad, FootprintRules, Graphic as LibraryGraphic,
    };

    /// Mask margin the fixture footprint states in its rules, in millimetres.
    const MASK_RULE: f64 = 0.1;
    /// Paste margin the fixture footprint states in its rules: negative shrinks.
    const PASTE_RULE: f64 = -0.02;

    /// One through-hole pad: a circle with a drill, copper on both sides.
    fn through_pad() -> FootprintPad {
        FootprintPad {
            number: "1".into(),
            pad_type: "thru_hole".into(),
            shape: "circle".into(),
            position: Point::default(),
            rotation: 0.0,
            size: Point::new(1.6, 1.6),
            layers: vec!["*.Cu".into(), "*.Mask".into()],
            drill: Some(Drill {
                size: Point::new(0.8, 0.8),
                offset: Point::default(),
            }),
            roundrect_ratio: None,
            raw_source: String::new(),
            chamfer: None,
            polygon: Vec::new(),
        }
    }

    /// One surface-mounted rect pad, 1.2 by 0.8 millimetres.
    fn smd_pad(number: &str, at: Point) -> FootprintPad {
        FootprintPad {
            number: number.into(),
            pad_type: "smd".into(),
            shape: "rect".into(),
            position: at,
            rotation: 0.0,
            size: Point::new(1.2, 0.8),
            layers: vec!["F.Cu".into(), "F.Paste".into(), "F.Mask".into()],
            drill: None,
            roundrect_ratio: None,
            raw_source: String::new(),
            chamfer: None,
            polygon: Vec::new(),
        }
    }

    /// The board pad that places one embedded pad at a position.
    fn placed(source: &FootprintPad) -> Pad {
        Pad {
            number: source.number.clone(),
            offset: source.position,
            diameter: source.size.x.min(source.size.y),
            net: None,
            castellated: None,
            geometry: Some(source.clone()),
        }
    }

    fn pins(numbers: &[&str]) -> Vec<Pin> {
        numbers
            .iter()
            .map(|number| Pin {
                number: (*number).into(),
                name: String::new(),
                offset: Point::default(),
                net: None,
                electrical_type: "passive".into(),
                direction: Some(0.0),
                unit: 1,
            })
            .collect()
    }

    fn component(
        id: u64,
        reference: &str,
        position: Point,
        pads: Vec<FootprintPad>,
        graphics: Vec<LibraryGraphic>,
        rules: Option<FootprintRules>,
    ) -> Component {
        let numbers: Vec<&str> = pads.iter().map(|p| p.number.as_str()).collect();
        Component {
            sheet: 0,
            id,
            reference: reference.into(),
            value: "1k".into(),
            kind: "resistor".into(),
            position,
            rotation: 0.0,
            pins: pins(&numbers),
            units: vec![],
            library_symbol: None,
            library_footprint: Some(LibraryFootprint {
                library: "Test".into(),
                definition: FootprintDefinition {
                    name: format!("{reference}-PKG"),
                    description: String::new(),
                    keywords: String::new(),
                    pads: pads.clone(),
                    graphics,
                    model: None,
                    rules,
                    raw_source: String::new(),
                    unsupported: vec![],
                },
            }),
            fields: vec![],
        }
    }

    /// Project with a front SMD part, a through-hole part, a via, front and back
    /// silk lines, one silk line inside a pad opening, a board label and a board
    /// graphic.
    fn fixture() -> Result<Project, String> {
        let mut project = Project::default();
        project.nets.push(Net {
            id: 8,
            name: "N1".into(),
            ..Default::default()
        });
        let front_pads = vec![
            smd_pad("1", Point::new(-1.0, 0.0)),
            smd_pad("2", Point::new(1.0, 0.0)),
        ];
        let silk = vec![
            LibraryGraphic::Line {
                start: Point::new(-2.0, 1.0),
                end: Point::new(2.0, 1.0),
                width: 0.12,
                layer: "F.SilkS".into(),
            },
            LibraryGraphic::Line {
                start: Point::new(-2.0, -1.0),
                end: Point::new(2.0, -1.0),
                width: 0.12,
                layer: "B.SilkS".into(),
            },
            // Crosses the pad 1 mask opening, so silk must drop it.
            LibraryGraphic::Line {
                start: Point::new(-1.2, -0.1),
                end: Point::new(-0.8, 0.1),
                width: 0.12,
                layer: "F.SilkS".into(),
            },
            LibraryGraphic::Text {
                position: Point::new(0.0, 0.0),
                angle: 0.0,
                text: "R1".into(),
                size: Point::new(1.0, 1.0),
                layer: "F.SilkS".into(),
            },
        ];
        let rules = FootprintRules {
            clearance: None,
            mask_margin: Some(MASK_RULE),
            paste_margin: Some(PASTE_RULE),
        };
        let mut front = component(
            1,
            "R1",
            Point::new(10.0, 10.0),
            front_pads.clone(),
            silk,
            Some(rules),
        );
        let mut through = component(
            3,
            "J1",
            Point::new(20.0, 20.0),
            vec![through_pad()],
            vec![],
            None,
        );
        project.board.footprints.push(Footprint {
            id: 2,
            component: 1,
            position: front.position,
            rotation: 0.0,
            locked: None,
            side: None,
            pads: front_pads.iter().map(placed).collect(),
        });
        project.board.footprints.push(Footprint {
            id: 4,
            component: 3,
            position: through.position,
            rotation: 0.0,
            locked: None,
            side: None,
            pads: vec![placed(&through_pad())],
        });
        front.id = 1;
        through.id = 3;
        project.components.push(front);
        project.components.push(through);
        project.board.vias.push(Via {
            id: 5,
            net: 8,
            position: Point::new(30.0, 10.0),
            diameter: 0.6,
            drill: 0.3,
            from_layer: None,
            to_layer: None,
            covering: None,
        });
        project.board.texts.push(pcbcraft_doc::BoardText {
            id: 6,
            text: "R1".into(),
            position: Point::new(10.0, 8.0),
            size: 1.0,
            layer: Layer::Front,
            rotation: 0.0,
            component: None,
        });
        project.graphics.push(pcbcraft_doc::Graphic {
            id: 7,
            layer: "board".into(),
            shape: "line".into(),
            points: vec![Point::new(5.0, 5.0), Point::new(6.0, 5.0)],
            width: 0.15,
            text: String::new(),
            sublayer: String::new(),
        });
        project.validate()?;
        Ok(project)
    }

    /// Read-back of a written file: X2 file attributes, aperture templates,
    /// flashes and draw points, enough to check counts and sizes.
    struct Parsed {
        attributes: Vec<String>,
        apertures: BTreeMap<usize, String>,
        flashes: Vec<(usize, Point)>,
        draws: Vec<Point>,
    }

    /// Small Gerber reader used by the tests. It understands the constructs the
    /// mask, paste and silk writers emit and refuses anything else, so a test
    /// fails when a file carries a command this reader cannot place.
    fn gerber_parser(text: &str) -> Result<Parsed, String> {
        let mut parsed = Parsed {
            attributes: Vec::new(),
            apertures: BTreeMap::new(),
            flashes: Vec::new(),
            draws: Vec::new(),
        };
        let mut current: Option<usize> = None;
        let mut macro_body = false;
        for line in text.lines() {
            if macro_body {
                if line.ends_with("*%") {
                    macro_body = false;
                }
                continue;
            }
            if let Some(body) = line.strip_prefix("%TF.").and_then(|l| l.strip_suffix("*%")) {
                parsed.attributes.push(body.to_owned());
            } else if line.starts_with("%AM") {
                macro_body = !line.ends_with("*%");
            } else if let Some(body) = line.strip_prefix("%ADD").and_then(|l| l.strip_suffix("*%"))
            {
                let split = body
                    .find(|c: char| !c.is_ascii_digit())
                    .ok_or("aperture without a template")?;
                let (code, template) = body.split_at(split);
                let code: usize = code.parse().map_err(|e| format!("{e}"))?;
                parsed.apertures.insert(code, template.to_owned());
            } else if line.starts_with('%')
                || line.starts_with("G04")
                || matches!(line, "M02*" | "G01*" | "G36*" | "G37*")
            {
                continue;
            } else if let Some(body) = line.strip_suffix('*') {
                if let Some(code) = body.strip_prefix('D') {
                    let code: usize = code.parse().map_err(|e| format!("{e}"))?;
                    current = Some(code);
                    continue;
                }
                let body = body.strip_prefix('X').ok_or("unknown Gerber command")?;
                let (xs, rest) = body.split_once('Y').ok_or("coordinate without Y")?;
                let (ys, op) = rest.split_once('D').ok_or("coordinate without D")?;
                let at = Point::new(
                    xs.parse::<f64>().map_err(|e| format!("{e}"))? / 1e6,
                    ys.parse::<f64>().map_err(|e| format!("{e}"))? / 1e6,
                );
                match op {
                    "03" => parsed
                        .flashes
                        .push((current.ok_or("flash without aperture")?, at)),
                    "01" | "02" => parsed.draws.push(at),
                    other => return Err(format!("unsupported operation D{other}")),
                }
            } else if !line.starts_with("G01") {
                return Err(format!("unsupported Gerber line {line}"));
            }
        }
        for attribute in ["FileFunction", "FilePolarity", "SameCoordinates"] {
            if !parsed
                .attributes
                .iter()
                .any(|a| a.starts_with(&format!("{attribute},")))
            {
                return Err(format!("missing X2 attribute {attribute}"));
            }
        }
        Ok(parsed)
    }

    fn aperture(parsed: &Parsed, code: usize) -> Result<&str, String> {
        parsed
            .apertures
            .get(&code)
            .map(String::as_str)
            .ok_or_else(|| format!("no aperture D{code}"))
    }

    #[test]
    fn mask_opens_every_pad_and_via_by_the_rule_margin() -> Result<(), String> {
        let project = fixture()?;
        let front = gerber_parser(&gerber_mask(&project, Layer::Front)?)?;
        assert_eq!(
            front.flashes.len(),
            4,
            "two SMD pads, one through pad, one via"
        );
        let templates: Vec<&str> = front
            .flashes
            .iter()
            .map(|(code, _)| aperture(&front, *code))
            .collect::<Result<_, _>>()?;
        assert!(
            templates.contains(&"R,1.4X1"),
            "the SMD pad opens by the 0.1 mm rule margin: {templates:?}"
        );
        assert!(
            templates.contains(&"C,1.7"),
            "the through pad falls back to the 0.05 mm default: {templates:?}"
        );
        assert!(
            templates.contains(&"C,0.7"),
            "the via opens by the default margin: {templates:?}"
        );
        let back = gerber_parser(&gerber_mask(&project, Layer::Back)?)?;
        assert_eq!(back.flashes.len(), 2, "the through pad and the via");
        assert!(
            back.flashes.iter().any(|(_, at)| at.x == -30.0),
            "the back file mirrors x: {:?}",
            back.flashes
        );
        Ok(())
    }

    #[test]
    fn paste_opens_surface_pads_only_and_shrinks_by_the_rule_margin() -> Result<(), String> {
        let project = fixture()?;
        let front = gerber_parser(&gerber_paste(&project, Layer::Front)?)?;
        assert_eq!(front.flashes.len(), 2, "the two front SMD pads");
        assert!(
            front.flashes.iter().all(|(_, at)| at.y != 20.0),
            "the through-hole pad takes no paste: {:?}",
            front.flashes
        );
        let templates: Vec<&str> = front
            .flashes
            .iter()
            .map(|(code, _)| aperture(&front, *code))
            .collect::<Result<_, _>>()?;
        assert!(
            templates.contains(&"R,1.16X0.76"),
            "a -0.02 mm rule margin shrinks the 1.2 by 0.8 pad: {templates:?}"
        );
        let back = gerber_parser(&gerber_paste(&project, Layer::Back)?)?;
        assert!(
            back.flashes.is_empty(),
            "nothing surface mounted on the back"
        );
        Ok(())
    }

    #[test]
    fn silk_keeps_each_line_on_its_side_and_off_exposed_pads() -> Result<(), String> {
        let project = fixture()?;
        let front = gerber_parser(&gerber_silk(&project, Layer::Front)?)?;
        assert!(
            front.draws.contains(&Point::new(8.0, 11.0)),
            "the front silk line starts here: {:?}",
            front.draws
        );
        assert!(
            front.draws.contains(&Point::new(5.0, 5.0)),
            "the board graphic line is silk: {:?}",
            front.draws
        );
        assert!(
            !front.draws.iter().any(|p| p.x == 8.8 && p.y == 9.9),
            "silk inside a mask opening is dropped: {:?}",
            front.draws
        );
        assert!(
            !front.draws.contains(&Point::new(-8.0, 9.0)),
            "the back line stays off the front file"
        );
        let back = gerber_parser(&gerber_silk(&project, Layer::Back)?)?;
        assert!(
            back.draws.contains(&Point::new(-8.0, 9.0)),
            "the back silk line is mirrored: {:?}",
            back.draws
        );
        assert!(
            !back.draws.contains(&Point::new(8.0, 11.0)),
            "the front line stays off the back file"
        );
        let text = gerber_silk(&project, Layer::Front)?;
        assert!(
            text.contains("text items were left out"),
            "the file records the skipped text"
        );
        Ok(())
    }

    #[test]
    fn both_sides_carry_x2_attributes_and_end_cleanly() -> Result<(), String> {
        let project = fixture()?;
        for layer in [Layer::Front, Layer::Back] {
            for (name, text) in [
                ("mask", gerber_mask(&project, layer)?),
                ("paste", gerber_paste(&project, layer)?),
                ("silk", gerber_silk(&project, layer)?),
            ] {
                assert!(text.ends_with("M02*\n"), "{name} ends with M02");
                gerber_parser(&text)?;
            }
        }
        let front = gerber_mask(&project, Layer::Front)?;
        assert!(front.contains("%TF.FileFunction,Soldermask,Top*%"));
        assert!(front.contains("%TF.FilePolarity,Negative*%"));
        assert!(front.contains("%TA.AperFunction,SMDPad,CuDef*%"));
        let paste = gerber_paste(&project, Layer::Back)?;
        assert!(paste.contains("%TF.FileFunction,Paste,Bot*%"));
        assert!(paste.contains("%TF.FilePolarity,Positive*%"));
        let silk = gerber_silk(&project, Layer::Back)?;
        assert!(silk.contains("%TF.FileFunction,Legend,Bot*%"));
        assert!(silk.contains("%TF.FilePolarity,Positive*%"));
        assert!(silk.contains("%TA.AperFunction,Legend*%"));
        Ok(())
    }

    #[test]
    fn a_back_surface_pad_takes_mask_and_paste_on_the_back_only() -> Result<(), String> {
        let mut pad = smd_pad("1", Point::new(0.0, 0.0));
        pad.layers = vec!["B.Cu".into(), "B.Paste".into(), "B.Mask".into()];
        let placed = placed(&pad);
        assert!(pad_is_smd(&placed), "a pad without a drill takes paste");
        let footprint = Footprint {
            id: 1,
            component: 1,
            position: Point::new(0.0, 0.0),
            rotation: 0.0,
            locked: None,
            side: None,
            pads: vec![],
        };
        assert!(!pad_on_side(&footprint, &placed, Layer::Front));
        assert!(pad_on_side(&footprint, &placed, Layer::Back));
        let opening = pad_opening(&footprint, &placed, MASK_RULE, true)?;
        assert_eq!(opening, Opening::Rect(1_400, 1_000));
        Ok(())
    }
}
