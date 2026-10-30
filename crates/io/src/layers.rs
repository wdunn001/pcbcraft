//! Shared fabrication layer geometry for raster export, Gerber, stencil, and UV artwork.
//!
//! Collects all geometry for a single fabrication layer into a `FabLayer` value
//! that can be rasterised or plotted. Used by the Gerber, stencil, silk, and
//! mask exporters to avoid duplicating geometry collection logic.
//!
//! # Rules
//! - Geometry is collected in board coordinates (millimetres).
//! - Pads are included only for footprints placed on the board.
//! - Vias are included on the layers they connect through.
//!
//! # Dependencies
//! - crates/doc/src/lib.rs ArtShape, BoardText, CopperZone, Fiducial, Footprint, Id, Layer, Project, Track, Via
//! - crates/geom/src/lib.rs Point
//! - crates/library/src/lib.rs Graphic
use pcbcraft_doc::{
    ArtShape, BoardText, CopperZone, Fiducial, Footprint, Id, Layer, Project, Track, Via,
};
use pcbcraft_geom::Point;
use pcbcraft_library::Graphic;

const DEFAULT_MASK_MARGIN_MM: f64 = 0.05;

/// A fabrication layer that can be rasterised or plotted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FabLayer {
    CopperTop,
    CopperBottom,
    MaskTop,
    MaskBottom,
    LegendTop,
    LegendBottom,
    PasteTop,
    PasteBottom,
    Profile,
    Drill,
    User(Id),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShapePolarity {
    Positive,
    Negative,
}

/// One drawable fabrication primitive in board millimetres after the fab transform.
#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    Polygon(Vec<Point>, ShapePolarity),
    Disk(Point, f64, ShapePolarity),
    Stroke(Vec<Point>, f64, bool, ShapePolarity),
}

/// Geometry collected for one layer, plus reasons anything was skipped.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LayerShapes {
    pub shapes: Vec<Shape>,
    pub skipped: Vec<String>,
}

/// Board coordinates to fabrication space: drill origin subtracted, y axis up.
pub fn fabrication_point(p: &Project, board: Point) -> Point {
    let x = board.x - p.board.drill_origin.x;
    let y = p.board.height - (board.y - p.board.drill_origin.y);
    Point::new(x, y)
}

fn footprint_rules(f: &Footprint, project: &Project) -> pcbcraft_library::FootprintRules {
    project
        .component(f.component)
        .and_then(|c| c.library_footprint.as_ref())
        .and_then(|lf| lf.definition.rules)
        .unwrap_or_default()
}

fn pad_on_copper_layer(pad: &pcbcraft_doc::Pad, top: bool) -> bool {
    match &pad.geometry {
        None => true,
        Some(g) => {
            let through = g.drill.is_some() || g.pad_type.starts_with("thru_hole");
            if through {
                return true;
            }
            if g.layers.is_empty() {
                return top;
            }
            let want = if top { "F." } else { "B." };
            g.layers.iter().any(|l| l.starts_with(want))
        }
    }
}

fn pad_smd(pad: &pcbcraft_doc::Pad) -> bool {
    pad.geometry
        .as_ref()
        .is_some_and(|g| g.drill.is_none() && !g.pad_type.starts_with("thru_hole"))
}

fn grow_outline(points: &[Point], margin: f64) -> Vec<Point> {
    if points.len() < 3 || margin.abs() < 1e-12 {
        return points.to_vec();
    }
    let mut cx = 0.0;
    let mut cy = 0.0;
    let n = points.len().saturating_sub(1).max(1);
    for p in points.iter().take(n) {
        cx += p.x;
        cy += p.y;
    }
    cx /= n as f64;
    cy /= n as f64;
    let center = Point::new(cx, cy);
    points
        .iter()
        .map(|p| {
            let dx = p.x - center.x;
            let dy = p.y - center.y;
            let d = dx.hypot(dy);
            if d < 1e-9 {
                *p
            } else {
                let scale = (d + margin) / d;
                Point::new(center.x + dx * scale, center.y + dy * scale)
            }
        })
        .collect()
}

fn shrink_outline(points: &[Point], margin: f64) -> Vec<Point> {
    if points.len() < 3 || margin.abs() < 1e-12 {
        return points.to_vec();
    }
    let mut cx = 0.0;
    let mut cy = 0.0;
    let n = points.len().saturating_sub(1).max(1);
    for p in points.iter().take(n) {
        cx += p.x;
        cy += p.y;
    }
    cx /= n as f64;
    cy /= n as f64;
    let center = Point::new(cx, cy);
    points
        .iter()
        .map(|p| {
            let dx = p.x - center.x;
            let dy = p.y - center.y;
            let d = dx.hypot(dy);
            if d < 1e-9 {
                *p
            } else {
                let scale = ((d - margin) / d).max(0.0);
                Point::new(center.x + dx * scale, center.y + dy * scale)
            }
        })
        .collect()
}

fn footprint_pad_outline(f: &Footprint, pad: &pcbcraft_doc::Pad) -> Vec<Point> {
    pad.outline_points()
        .into_iter()
        .map(|local| f.position.offset(local.rotate(-f.rotation)))
        .collect()
}

fn push_zone(shapes: &mut Vec<Shape>, zone: &CopperZone, p: &Project) {
    if !zone.filled {
        return;
    }
    let outline: Vec<Point> = zone
        .outline
        .iter()
        .map(|pt| fabrication_point(p, *pt))
        .collect();
    shapes.push(Shape::Polygon(outline, ShapePolarity::Positive));
    for cut in &zone.cutouts {
        let ring: Vec<Point> = cut.iter().map(|pt| fabrication_point(p, *pt)).collect();
        shapes.push(Shape::Polygon(ring, ShapePolarity::Negative));
    }
}

fn push_track(shapes: &mut Vec<Shape>, track: &Track, p: &Project) {
    if let Some(arc) = track.curve() {
        let steps = 32;
        let mut points = Vec::with_capacity(steps + 1);
        for i in 0..=steps {
            let t = i as f64 / steps as f64;
            points.push(fabrication_point(p, arc.point_at(t)));
        }
        shapes.push(Shape::Stroke(
            points,
            track.width,
            false,
            ShapePolarity::Positive,
        ));
    } else {
        shapes.push(Shape::Stroke(
            vec![
                fabrication_point(p, track.start),
                fabrication_point(p, track.end),
            ],
            track.width,
            false,
            ShapePolarity::Positive,
        ));
    }
}

fn push_via(shapes: &mut Vec<Shape>, via: &Via, p: &Project) {
    shapes.push(Shape::Disk(
        fabrication_point(p, via.position),
        via.diameter / 2.0,
        ShapePolarity::Positive,
    ));
}

fn push_fiducial(shapes: &mut Vec<Shape>, fid: &Fiducial, p: &Project) {
    shapes.push(Shape::Disk(
        fabrication_point(p, fid.position),
        fid.diameter / 2.0,
        ShapePolarity::Positive,
    ));
}

fn silk_layer(top: bool) -> &'static str {
    if top { "F.SilkS" } else { "B.SilkS" }
}

fn push_graphic(shapes: &mut Vec<Shape>, g: &Graphic, origin: Point, rotation: f64, p: &Project) {
    let map = |pt: Point| fabrication_point(p, origin.offset(pt.rotate(-rotation)));
    match g {
        Graphic::Line {
            start, end, width, ..
        } => {
            shapes.push(Shape::Stroke(
                vec![map(*start), map(*end)],
                *width,
                false,
                ShapePolarity::Positive,
            ));
        }
        Graphic::Polyline { points, width, .. } => {
            if points.len() >= 2 {
                shapes.push(Shape::Stroke(
                    points.iter().map(|pt| map(*pt)).collect(),
                    *width,
                    false,
                    ShapePolarity::Positive,
                ));
            }
        }
        Graphic::Rectangle {
            start,
            end,
            width,
            fill,
            ..
        } => {
            let a = map(*start);
            let b = map(*end);
            let ring = vec![
                Point::new(a.x.min(b.x), a.y.min(b.y)),
                Point::new(a.x.max(b.x), a.y.min(b.y)),
                Point::new(a.x.max(b.x), a.y.max(b.y)),
                Point::new(a.x.min(b.x), a.y.max(b.y)),
                Point::new(a.x.min(b.x), a.y.min(b.y)),
            ];
            if fill == "solid" {
                shapes.push(Shape::Polygon(ring, ShapePolarity::Positive));
            } else {
                shapes.push(Shape::Stroke(ring, *width, true, ShapePolarity::Positive));
            }
        }
        Graphic::Circle {
            center,
            radius,
            width,
            fill,
            ..
        } => {
            if fill == "solid" {
                shapes.push(Shape::Disk(map(*center), *radius, ShapePolarity::Positive));
            } else {
                let steps = 48;
                let mut ring = Vec::with_capacity(steps + 1);
                for i in 0..=steps {
                    let angle = i as f64 * std::f64::consts::TAU / steps as f64;
                    ring.push(map(
                        center.offset(Point::new(radius * angle.cos(), radius * angle.sin()))
                    ));
                }
                shapes.push(Shape::Stroke(ring, *width, true, ShapePolarity::Positive));
            }
        }
        Graphic::Arc {
            start,
            mid,
            end,
            width,
            ..
        } => {
            if let Some(arc) = pcbcraft_geom::arc_through(*start, *mid, *end) {
                let steps = 24;
                let mut points = Vec::with_capacity(steps + 1);
                for i in 0..=steps {
                    points.push(map(arc.point_at(i as f64 / steps as f64)));
                }
                shapes.push(Shape::Stroke(
                    points,
                    *width,
                    false,
                    ShapePolarity::Positive,
                ));
            }
        }
        Graphic::Text { .. } => {}
    }
}

fn push_board_text(
    _shapes: &mut Vec<Shape>,
    _text: &BoardText,
    _p: &Project,
    skipped: &mut Vec<String>,
) {
    skipped.push("board text needs the stroke font".into());
}

pub(crate) fn profile_edge(p: &Project) -> Vec<Point> {
    match &p.board.outline {
        Some(o) => o.iter().map(|pt| fabrication_point(p, *pt)).collect(),
        None => {
            let w = p.board.width;
            let h = p.board.height;
            vec![
                fabrication_point(p, Point::new(0.0, 0.0)),
                fabrication_point(p, Point::new(w, 0.0)),
                fabrication_point(p, Point::new(w, h)),
                fabrication_point(p, Point::new(0.0, h)),
                fabrication_point(p, Point::new(0.0, 0.0)),
            ]
        }
    }
}

fn push_drill_hole(shapes: &mut Vec<Shape>, pos: Point, diameter: f64, mark: f64, p: &Project) {
    let r = if mark > 0.0 {
        mark / 2.0
    } else {
        diameter / 2.0
    };
    shapes.push(Shape::Disk(
        fabrication_point(p, pos),
        r.max(0.05),
        ShapePolarity::Positive,
    ));
}

fn footprint_bounds(p: &Project, f: &Footprint) -> (Point, Point) {
    let points = f.drawing_points(p);
    let min = points
        .iter()
        .fold(Point::new(f64::INFINITY, f64::INFINITY), |bounds, pt| {
            Point::new(bounds.x.min(pt.x), bounds.y.min(pt.y))
        });
    let max = points.iter().fold(
        Point::new(f64::NEG_INFINITY, f64::NEG_INFINITY),
        |bounds, pt| Point::new(bounds.x.max(pt.x), bounds.y.max(pt.y)),
    );
    if min.valid() && max.valid() {
        (min, max)
    } else {
        (Point::new(-3.4, -1.4), Point::new(3.4, 1.4))
    }
}

fn push_reference_marker(
    _shapes: &mut Vec<Shape>,
    _origin: Point,
    _rotation: f64,
    _label_y: f64,
    _label_x: f64,
    _p: &Project,
    skipped: &mut Vec<String>,
) {
    skipped.push("reference labels need the stroke font".into());
}

fn push_pad_clearance_zones(shapes: &mut Vec<Shape>, p: &Project, profile_pad_clearance_mm: f64) {
    for f in &p.board.footprints {
        let rules = footprint_rules(f, p);
        let footprint_clearance = rules.clearance.unwrap_or(0.0);
        for pad in &f.pads {
            let margin = profile_pad_clearance_mm
                .max(footprint_clearance)
                .max(p.board.clearance);
            let outline = footprint_pad_outline(f, pad);
            let grown = grow_outline(&outline, margin);
            if grown.len() < 3 {
                continue;
            }
            let fab: Vec<Point> = grown.iter().map(|pt| fabrication_point(p, *pt)).collect();
            shapes.push(Shape::Polygon(fab, ShapePolarity::Negative));
        }
    }
}

/// Legend strokes for one board side, with pad keep-out zones applied as negative
/// polygons and skipped strings listed when text cannot be stroked yet.
pub fn legend_shapes(
    p: &Project,
    side: Layer,
    profile_pad_clearance_mm: f64,
    include_values: bool,
) -> Result<LayerShapes, String> {
    let fab_layer = if side == Layer::Back {
        FabLayer::LegendBottom
    } else {
        FabLayer::LegendTop
    };
    let mut out = layer_shapes(p, fab_layer)?;
    let top = side != Layer::Back;
    for f in &p.board.footprints {
        if let Some(c) = p.component(f.component) {
            let (min, max) = footprint_bounds(p, f);
            let label_x = (min.x + max.x) / 2.0;
            push_reference_marker(
                &mut out.shapes,
                f.position,
                f.rotation,
                min.y - 1.2,
                label_x,
                p,
                &mut out.skipped,
            );
            if include_values {
                push_reference_marker(
                    &mut out.shapes,
                    f.position,
                    f.rotation,
                    max.y + 1.2,
                    label_x,
                    p,
                    &mut out.skipped,
                );
                let _ = c;
            }
        }
    }
    let _ = top;
    push_pad_clearance_zones(&mut out.shapes, p, profile_pad_clearance_mm);
    Ok(out)
}

/// Collect drawable shapes for one fabrication layer.
pub fn layer_shapes(p: &Project, layer: FabLayer) -> Result<LayerShapes, String> {
    p.validate()?;
    let mut out = LayerShapes::default();
    match layer {
        FabLayer::CopperTop | FabLayer::CopperBottom => {
            let top = matches!(layer, FabLayer::CopperTop);
            let copper = if top { Layer::Front } else { Layer::Back };
            for z in p.board.zones.iter().filter(|z| z.layer == copper) {
                push_zone(&mut out.shapes, z, p);
            }
            for t in p.board.tracks.iter().filter(|t| t.layer == copper) {
                push_track(&mut out.shapes, t, p);
            }
            for v in &p.board.vias {
                push_via(&mut out.shapes, v, p);
            }
            for f in &p.board.footprints {
                for pad in &f.pads {
                    if !pad_on_copper_layer(pad, top) {
                        continue;
                    }
                    let outline = footprint_pad_outline(f, pad);
                    let fab: Vec<Point> =
                        outline.iter().map(|pt| fabrication_point(p, *pt)).collect();
                    out.shapes
                        .push(Shape::Polygon(fab, ShapePolarity::Positive));
                }
            }
            for fid in &p.board.fiducials {
                push_fiducial(&mut out.shapes, fid, p);
            }
        }
        FabLayer::MaskTop | FabLayer::MaskBottom => {
            let top = matches!(layer, FabLayer::MaskTop);
            for f in &p.board.footprints {
                let margin = footprint_rules(f, p)
                    .mask_margin
                    .unwrap_or(DEFAULT_MASK_MARGIN_MM);
                for pad in &f.pads {
                    if !pad_on_copper_layer(pad, top) {
                        continue;
                    }
                    let outline = footprint_pad_outline(f, pad);
                    let grown = grow_outline(&outline, margin);
                    let fab: Vec<Point> =
                        grown.iter().map(|pt| fabrication_point(p, *pt)).collect();
                    out.shapes
                        .push(Shape::Polygon(fab, ShapePolarity::Positive));
                }
            }
        }
        FabLayer::PasteTop | FabLayer::PasteBottom => {
            let top = matches!(layer, FabLayer::PasteTop);
            for f in &p.board.footprints {
                let margin = footprint_rules(f, p).paste_margin.unwrap_or(0.0);
                for pad in &f.pads {
                    if !pad_smd(pad) || !pad_on_copper_layer(pad, top) {
                        continue;
                    }
                    let outline = footprint_pad_outline(f, pad);
                    let shrunk = shrink_outline(&outline, margin);
                    if shrunk.len() < 3 {
                        continue;
                    }
                    let fab: Vec<Point> =
                        shrunk.iter().map(|pt| fabrication_point(p, *pt)).collect();
                    out.shapes
                        .push(Shape::Polygon(fab, ShapePolarity::Positive));
                }
            }
        }
        FabLayer::LegendTop | FabLayer::LegendBottom => {
            let top = matches!(layer, FabLayer::LegendTop);
            let layer_name = silk_layer(top);
            for f in &p.board.footprints {
                if let Some(library) = p
                    .component(f.component)
                    .and_then(|c| c.library_footprint.as_ref())
                {
                    for graphic in &library.definition.graphics {
                        if graphic.layer() == layer_name {
                            push_graphic(&mut out.shapes, graphic, f.position, f.rotation, p);
                        }
                    }
                }
            }
            for text in p
                .board
                .texts
                .iter()
                .filter(|t| t.layer == if top { Layer::Front } else { Layer::Back })
            {
                push_board_text(&mut out.shapes, text, p, &mut out.skipped);
            }
        }
        FabLayer::Profile => {
            out.shapes.push(Shape::Stroke(
                profile_edge(p),
                0.1,
                true,
                ShapePolarity::Positive,
            ));
        }
        FabLayer::Drill => {
            let mark = 0.0;
            for v in &p.board.vias {
                if v.drill > 0.0 {
                    push_drill_hole(&mut out.shapes, v.position, v.drill, mark, p);
                }
            }
            for h in &p.board.holes {
                push_drill_hole(&mut out.shapes, h.position, h.diameter, mark, p);
            }
            for f in &p.board.footprints {
                for pad in &f.pads {
                    let Some(g) = pad.geometry.as_ref() else {
                        continue;
                    };
                    let Some(drill) = g.drill.as_ref() else {
                        continue;
                    };
                    let d = drill.size.x.min(drill.size.y);
                    if d <= 0.0 {
                        continue;
                    }
                    let local = pad.offset.offset(drill.offset.rotate(-g.rotation));
                    let world = f.position.offset(local.rotate(-f.rotation));
                    push_drill_hole(&mut out.shapes, world, d, mark, p);
                }
            }
        }
        FabLayer::User(id) => {
            let Some(art) = p.board.user_layers.iter().find(|l| l.id == id) else {
                return Err(format!("no user layer with id {id}"));
            };
            for shape in &art.shapes {
                push_art_shape(&mut out.shapes, shape, p);
            }
        }
    }
    Ok(out)
}

fn push_art_shape(shapes: &mut Vec<Shape>, shape: &ArtShape, p: &Project) {
    match shape {
        ArtShape::Line { start, end } => {
            shapes.push(Shape::Stroke(
                vec![fabrication_point(p, *start), fabrication_point(p, *end)],
                0.1,
                false,
                ShapePolarity::Positive,
            ));
        }
        ArtShape::Poly { points } => {
            if points.len() >= 2 {
                let mut ring: Vec<Point> =
                    points.iter().map(|pt| fabrication_point(p, *pt)).collect();
                if let (Some(first), Some(last)) = (ring.first(), ring.last())
                    && first.distance(*last) > 1e-9
                {
                    ring.push(*first);
                }
                shapes.push(Shape::Stroke(ring, 0.1, true, ShapePolarity::Positive));
            }
        }
        ArtShape::Rect {
            x,
            y,
            width,
            height,
            rotate,
        } => {
            let cx = *x + *width / 2.0;
            let cy = *y + *height / 2.0;
            let corners = [
                Point::new(*x, *y),
                Point::new(*x + *width, *y),
                Point::new(*x + *width, *y + *height),
                Point::new(*x, *y + *height),
                Point::new(*x, *y),
            ];
            let ring: Vec<Point> = corners
                .iter()
                .map(|pt| {
                    let local = pt.offset(Point::new(-cx, -cy)).rotate(-rotate);
                    fabrication_point(p, Point::new(cx, cy).offset(local))
                })
                .collect();
            shapes.push(Shape::Stroke(ring, 0.1, true, ShapePolarity::Positive));
        }
        ArtShape::Circle { center, radius } => {
            shapes.push(Shape::Disk(
                fabrication_point(p, *center),
                *radius,
                ShapePolarity::Positive,
            ));
        }
        ArtShape::Text { .. } => {}
    }
}

trait GraphicLayer {
    fn layer(&self) -> &str;
}

impl GraphicLayer for Graphic {
    fn layer(&self) -> &str {
        match self {
            Graphic::Line { layer, .. }
            | Graphic::Polyline { layer, .. }
            | Graphic::Rectangle { layer, .. }
            | Graphic::Circle { layer, .. }
            | Graphic::Arc { layer, .. }
            | Graphic::Text { layer, .. } => layer,
        }
    }
}

/// Count flashes (pads/vias) and region fills (zones) on a copper side for Gerber parity checks.
pub fn copper_shape_counts(p: &Project, top: bool) -> Result<(usize, usize), String> {
    p.validate()?;
    let copper = if top { Layer::Front } else { Layer::Back };
    let mut flashes = 0usize;
    let mut regions = 0usize;
    for z in p
        .board
        .zones
        .iter()
        .filter(|z| z.layer == copper && z.filled)
    {
        let _ = z;
        regions += 1;
    }
    for v in &p.board.vias {
        let _ = v;
        flashes += 1;
    }
    for f in &p.board.footprints {
        for pad in &f.pads {
            if pad_on_copper_layer(pad, top) {
                flashes += 1;
            }
        }
    }
    Ok((flashes, regions))
}

pub type GerberCopperCounts = ((usize, usize), (usize, usize));

pub fn gerber_copper_counts(p: &Project) -> Result<GerberCopperCounts, String> {
    let files = crate::gerber_files(p)?;
    let front = files.get("pcb-F.Cu.gbr").ok_or("missing front copper")?;
    let back = files.get("pcb-B.Cu.gbr").ok_or("missing back copper")?;
    Ok((count_gerber_ops(front), count_gerber_ops(back)))
}

fn count_gerber_ops(text: &str) -> (usize, usize) {
    let flashes = text.matches("D03*").count();
    let regions = text.matches("G36*").count();
    (flashes, regions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pcbcraft_doc::Project;

    #[test]
    fn legend_without_a_stroke_font_lists_skipped_texts() -> Result<(), String> {
        let mut p = Project::default();
        p.board.texts.push(BoardText {
            id: 1,
            text: "note".into(),
            position: Point::new(1.0, 1.0),
            size: 1.0,
            layer: Layer::Front,
            rotation: 0.0,
            component: None,
        });
        p.validate()?;
        let out = layer_shapes(&p, FabLayer::LegendTop)?;
        assert!(out.shapes.is_empty());
        assert_eq!(out.skipped.len(), 1);
        assert!(out.skipped[0].contains("stroke font"));
        Ok(())
    }

    #[test]
    fn layer_shapes_agree_with_gerber_flash_and_region_counts() -> Result<(), String> {
        let mut project = Project::default();
        project.nets.push(pcbcraft_doc::Net {
            id: 8,
            name: "GND".into(),
            ..Default::default()
        });
        project.board.zones.push(CopperZone {
            id: 5,
            net: 8,
            layer: Layer::Front,
            outline: vec![
                Point::new(5.0, 5.0),
                Point::new(45.0, 5.0),
                Point::new(45.0, 25.0),
                Point::new(5.0, 25.0),
                Point::new(5.0, 5.0),
            ],
            cutouts: vec![],
            filled: true,
            kind: None,
            pour: None,
        });
        project.board.vias.push(Via {
            id: 4,
            net: 8,
            position: Point::new(30.0, 10.0),
            diameter: 0.6,
            drill: 0.3,
            from_layer: None,
            to_layer: None,
            covering: None,
            covering_scope: None,
        });
        let pad = pcbcraft_doc::Pad {
            number: "1".into(),
            offset: Point::default(),
            diameter: 1.6,
            net: None,
            castellated: None,
            geometry: Some(pcbcraft_library::FootprintPad {
                number: "1".into(),
                pad_type: "thru_hole".into(),
                shape: "circle".into(),
                position: Point::default(),
                rotation: 0.0,
                size: Point::new(1.6, 1.6),
                layers: vec!["*.Cu".into()],
                drill: Some(pcbcraft_library::Drill {
                    size: Point::new(0.8, 0.8),
                    offset: Point::default(),
                }),
                roundrect_ratio: None,
                raw_source: String::new(),
                chamfer: None,
                polygon: Vec::new(),
            }),
        };
        let embedded_pad = pad.geometry.as_ref().ok_or("pad")?.clone();
        let mut pad2 = pad.clone();
        pad2.number = "2".into();
        pad2.offset = Point::new(2.0, 0.0);
        if let Some(g) = pad2.geometry.as_mut() {
            g.number = "2".into();
            g.position = Point::new(2.0, 0.0);
        }
        let mut embedded_pad2 = embedded_pad.clone();
        embedded_pad2.number = "2".into();
        embedded_pad2.position = Point::new(2.0, 0.0);
        project.board.footprints.push(Footprint {
            id: 2,
            component: 1,
            position: Point::new(0.0, 0.0),
            rotation: 0.0,
            locked: None,
            side: None,
            pads: vec![pad, pad2],
        });
        project.components.push(pcbcraft_doc::Component {
            sheet: 0,
            id: 1,
            reference: "R1".into(),
            value: "1k".into(),
            kind: "resistor".into(),
            position: Point::default(),
            rotation: 0.0,
            pins: vec![
                pcbcraft_doc::Pin {
                    number: "1".into(),
                    name: String::new(),
                    offset: Point::new(-1.0, 0.0),
                    net: None,
                    electrical_type: "passive".into(),
                    direction: Some(180.0),
                    unit: 1,
                },
                pcbcraft_doc::Pin {
                    number: "2".into(),
                    name: String::new(),
                    offset: Point::new(1.0, 0.0),
                    net: None,
                    electrical_type: "passive".into(),
                    direction: Some(0.0),
                    unit: 1,
                },
            ],
            units: vec![],
            library_symbol: None,
            library_footprint: Some(pcbcraft_doc::LibraryFootprint {
                library: "ThroughHole".into(),
                definition: pcbcraft_library::FootprintDefinition {
                    name: "PAD-1.6".into(),
                    description: String::new(),
                    keywords: String::new(),
                    pads: vec![embedded_pad, embedded_pad2],
                    graphics: vec![],
                    model: None,
                    rules: None,
                    raw_source: String::new(),
                    unsupported: vec![],
                },
            }),
            fields: vec![],
        });
        project.validate()?;
        let shape_front = copper_shape_counts(&project, true)?;
        let gerber = gerber_copper_counts(&project)?;
        assert_eq!(shape_front, gerber.0, "front copper counts");
        let shape_back = copper_shape_counts(&project, false)?;
        assert_eq!(shape_back, gerber.1, "back copper counts");
        Ok(())
    }
}
