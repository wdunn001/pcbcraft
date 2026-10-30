//! Snap targets and helpers for aligning a 3D model to a footprint in the library editor.
//!
//! Coordinates are footprint-local millimetres, y down, the same frame as pad positions.

use crate::{FootprintDefinition, Graphic};
use pcbcraft_geom::Point;

/// Default distance within which a drag point snaps to a target, in millimetres.
pub const SNAP_RADIUS_MM: f64 = 0.35;

/// One snap target on the footprint plane.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SnapTargetKind {
    Origin,
    PadCentre,
    CourtyardCorner,
    CourtyardMidpoint,
    BodyCorner,
}

/// A point on the footprint that model edits may snap to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SnapTarget {
    pub kind: SnapTargetKind,
    pub position: Point,
}

fn is_courtyard(layer: &str) -> bool {
    matches!(layer, "F.CrtYd" | "B.CrtYd" | "pcbcraft.courtyard")
}

fn is_body_outline(layer: &str) -> bool {
    layer.ends_with(".Fab") || layer.ends_with(".SilkS")
}

fn bounds_of(points: &[Point]) -> Option<(Point, Point)> {
    let first = *points.first()?;
    let mut lo = first;
    let mut hi = first;
    for p in points {
        lo = Point::new(lo.x.min(p.x), lo.y.min(p.y));
        hi = Point::new(hi.x.max(p.x), hi.y.max(p.y));
    }
    Some((lo, hi))
}

fn rect_snap_points(
    lo: Point,
    hi: Point,
    kind_corner: SnapTargetKind,
    kind_mid: SnapTargetKind,
) -> Vec<SnapTarget> {
    let mx = (lo.x + hi.x) / 2.0;
    let my = (lo.y + hi.y) / 2.0;
    vec![
        SnapTarget {
            kind: kind_corner,
            position: lo,
        },
        SnapTarget {
            kind: kind_corner,
            position: Point::new(hi.x, lo.y),
        },
        SnapTarget {
            kind: kind_corner,
            position: hi,
        },
        SnapTarget {
            kind: kind_corner,
            position: Point::new(lo.x, hi.y),
        },
        SnapTarget {
            kind: kind_mid,
            position: Point::new(mx, lo.y),
        },
        SnapTarget {
            kind: kind_mid,
            position: Point::new(mx, hi.y),
        },
        SnapTarget {
            kind: kind_mid,
            position: Point::new(lo.x, my),
        },
        SnapTarget {
            kind: kind_mid,
            position: Point::new(hi.x, my),
        },
    ]
}

/// Every snap target for a footprint definition.
pub fn footprint_snap_targets(def: &FootprintDefinition) -> Vec<SnapTarget> {
    let mut out = vec![SnapTarget {
        kind: SnapTargetKind::Origin,
        position: Point::default(),
    }];
    for pad in &def.pads {
        out.push(SnapTarget {
            kind: SnapTargetKind::PadCentre,
            position: pad.position,
        });
    }
    let mut courtyard_pts = Vec::new();
    let mut body_pts = Vec::new();
    for g in &def.graphics {
        let layer = match g {
            Graphic::Line { layer, .. }
            | Graphic::Rectangle { layer, .. }
            | Graphic::Circle { layer, .. }
            | Graphic::Arc { layer, .. }
            | Graphic::Polyline { layer, .. }
            | Graphic::Text { layer, .. } => layer.as_str(),
        };
        let pts = g.path_points();
        if is_courtyard(layer) {
            courtyard_pts.extend(pts);
        } else if is_body_outline(layer) {
            body_pts.extend(pts);
        }
    }
    if let Some((lo, hi)) = bounds_of(&courtyard_pts) {
        out.extend(rect_snap_points(
            lo,
            hi,
            SnapTargetKind::CourtyardCorner,
            SnapTargetKind::CourtyardMidpoint,
        ));
    }
    if let Some((lo, hi)) = bounds_of(&body_pts) {
        out.extend(rect_snap_points(
            lo,
            hi,
            SnapTargetKind::BodyCorner,
            SnapTargetKind::CourtyardMidpoint,
        ));
    }
    out
}

/// Nearest snap target within `radius`, if any.
pub fn snap_point(
    point: Point,
    targets: &[SnapTarget],
    radius: f64,
) -> Option<(Point, SnapTargetKind)> {
    let mut best: Option<(f64, Point, SnapTargetKind)> = None;
    for t in targets {
        let d = point.distance(t.position);
        if d > radius {
            continue;
        }
        if best.as_ref().is_none_or(|(bd, _, _)| d < *bd) {
            best = Some((d, t.position, t.kind));
        }
    }
    best.map(|(_, p, k)| (p, k))
}

/// Pad centres in footprint coordinates.
pub fn pad_centres(def: &FootprintDefinition) -> Vec<Point> {
    def.pads.iter().map(|p| p.position).collect()
}

/// Centroid of all pads, or the origin when there are none.
pub fn pad_centroid(def: &FootprintDefinition) -> Point {
    if def.pads.is_empty() {
        return Point::default();
    }
    let n = def.pads.len() as f64;
    Point::new(
        def.pads.iter().map(|p| p.position.x).sum::<f64>() / n,
        def.pads.iter().map(|p| p.position.y).sum::<f64>() / n,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FootprintPad;

    fn pad(n: &str, x: f64, y: f64) -> FootprintPad {
        FootprintPad {
            number: n.into(),
            pad_type: "smd".into(),
            shape: "rect".into(),
            position: Point::new(x, y),
            rotation: 0.0,
            size: Point::new(1.0, 1.0),
            layers: vec!["F.Cu".into()],
            drill: None,
            roundrect_ratio: None,
            chamfer: None,
            polygon: vec![],
            raw_source: String::new(),
        }
    }

    fn empty_footprint() -> FootprintDefinition {
        FootprintDefinition {
            name: "t".into(),
            description: String::new(),
            keywords: String::new(),
            pads: vec![],
            graphics: vec![],
            model: None,
            rules: None,
            raw_source: String::new(),
            unsupported: vec![],
        }
    }

    #[test]
    fn snap_targets_include_pads_and_origin() {
        let mut def = empty_footprint();
        def.pads = vec![pad("1", -1.0, 0.0), pad("2", 1.0, 0.0)];
        let targets = footprint_snap_targets(&def);
        assert!(targets.iter().any(|t| t.kind == SnapTargetKind::Origin));
        assert_eq!(
            targets
                .iter()
                .filter(|t| t.kind == SnapTargetKind::PadCentre)
                .count(),
            2
        );
    }

    #[test]
    fn snap_point_picks_the_nearest_target() {
        let mut def = empty_footprint();
        def.pads = vec![pad("1", 0.0, 0.0)];
        let targets = footprint_snap_targets(&def);
        assert_eq!(
            snap_point(Point::new(0.1, 0.05), &targets, 0.35).map(|(p, k)| (k, p.x, p.y)),
            Some((SnapTargetKind::PadCentre, 0.0, 0.0))
        );
    }
}
