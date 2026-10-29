//! Placed part bodies for the mechanical exports. Every placed part becomes
//! one box: its footprint extent (the courtyard when the footprint draws one,
//! otherwise the pad extent) extruded by the part height. The STEP and VRML
//! writers share this list so the two files agree.
//!
//! A part with a 3D model reference gets the same box for now. The model
//! offset is not applied here until the offset unit is settled for exports.

use crate::board_mesh::graphic_layer;
use pcbcraft_doc::{Footprint, Project};
use pcbcraft_geom::Point;

/// Body height in millimetres when a part states none.
pub const DEFAULT_BODY_HEIGHT_MM: f64 = 1.5;
/// Largest body height accepted from a component height field.
const MAX_BODY_HEIGHT_MM: f64 = 100.0;
/// Smallest body height accepted from a component height field.
const MIN_BODY_HEIGHT_MM: f64 = 0.05;

/// One part body: a counter-clockwise ring on the board plane between two
/// heights, where z=0 is the bottom face of the board.
#[derive(Clone, Debug, PartialEq)]
pub struct PartBox {
    pub name: String,
    pub ring: Vec<Point>,
    pub z0: f64,
    pub z1: f64,
}

/// True when the part sits on the bottom side: it has surface-mount pads and
/// none of them touch the top layer.
pub fn is_bottom_side(fp: &Footprint) -> bool {
    fp.pads.iter().any(|pad| {
        pad.geometry.as_ref().is_some_and(|g| {
            g.pad_type == "smd"
                && !g.layers.is_empty()
                && !g.layers.iter().any(|l| l.starts_with("F."))
        })
    })
}

fn body_height(text: Option<&str>) -> f64 {
    let Some(text) = text else {
        return DEFAULT_BODY_HEIGHT_MM;
    };
    let trimmed = text.trim();
    let number = trimmed.strip_suffix("mm").unwrap_or(trimmed).trim();
    match number.parse::<f64>() {
        Ok(v) if v.is_finite() && (MIN_BODY_HEIGHT_MM..=MAX_BODY_HEIGHT_MM).contains(&v) => v,
        _ => DEFAULT_BODY_HEIGHT_MM,
    }
}

fn is_courtyard(layer: &str) -> bool {
    matches!(layer, "F.CrtYd" | "B.CrtYd" | "pcbcraft.courtyard")
}

/// Footprint-local extent as (low, high) corners.
fn local_extent(project: &Project, fp: &Footprint) -> Option<(Point, Point)> {
    let mut points: Vec<Point> = project
        .component(fp.component)
        .and_then(|c| c.library_footprint.as_ref())
        .map(|l| {
            l.definition
                .graphics
                .iter()
                .filter(|g| graphic_layer(g).is_some_and(|(layer, _)| is_courtyard(layer)))
                .flat_map(|g| g.path_points())
                .collect()
        })
        .unwrap_or_default();
    if points.is_empty() {
        points = fp.pads.iter().flat_map(|p| p.outline_points()).collect();
    }
    let first = points.first().copied()?;
    let mut lo = first;
    let mut hi = first;
    for p in &points {
        lo = Point::new(lo.x.min(p.x), lo.y.min(p.y));
        hi = Point::new(hi.x.max(p.x), hi.y.max(p.y));
    }
    ((hi.x - lo.x) > 1e-6 && (hi.y - lo.y) > 1e-6).then_some((lo, hi))
}

/// One box per placed part that has an extent. Top parts stand on the top
/// face; bottom parts hang below the bottom face and are mirrored in x.
pub fn part_boxes(project: &Project) -> Vec<PartBox> {
    let thickness = project.board.thickness;
    let mut boxes = Vec::new();
    for fp in &project.board.footprints {
        let Some((lo, hi)) = local_extent(project, fp) else {
            continue;
        };
        let component = project.component(fp.component);
        let height = body_height(component.and_then(|c| c.field("height")));
        let bottom = is_bottom_side(fp);
        let corners = [
            Point::new(lo.x, lo.y),
            Point::new(hi.x, lo.y),
            Point::new(hi.x, hi.y),
            Point::new(lo.x, hi.y),
        ];
        let mut ring: Vec<Point> = corners
            .iter()
            .map(|c| {
                let local = if bottom { Point::new(-c.x, c.y) } else { *c };
                fp.position.offset(local.rotate(-fp.rotation))
            })
            .collect();
        if bottom {
            // Mirroring flips the winding; keep the ring counter-clockwise.
            ring.reverse();
        }
        let (z0, z1) = if bottom {
            (-height, 0.0)
        } else {
            (thickness, thickness + height)
        };
        let name = component
            .map(|c| c.reference.clone())
            .unwrap_or_else(|| format!("footprint {}", fp.id));
        boxes.push(PartBox { name, ring, z0, z1 });
    }
    boxes
}
