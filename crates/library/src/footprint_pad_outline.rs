//! Pad copper outlines in footprint-local coordinates (millimetres, y down).
use crate::{FootprintPad, PadChamfer, PadShape};
use pcbcraft_geom::{Point, point_in_polygon, polygon_edges, segments_cross};

const OUTLINE_ARC_STEPS: usize = 16;

/// Closed vertex ring of the pad copper outline in footprint-local space.
///
/// # Rules
/// - Returns an open ring with the first vertex repeated to close it
/// - All pad shapes are supported: circle, oval, roundrect, chamfered, custom
///
/// # Dependencies
/// - crates/library/src/footprint_pad_outline.rs [`FootprintPad`] for pad geometry
/// - crates/library/src/footprint_pad_outline.rs [`pcbcraft_geom::Point`] for coordinate math
///
/// # Do not
/// - Assume the returned ring is convex; custom polygons may be concave
pub fn pad_outline_closed(pad: &FootprintPad) -> Vec<Point> {
    let open = pad_outline_open(pad);
    if open.is_empty() {
        return open;
    }
    let mut closed = open;
    if let Some(first) = closed.first().copied() {
        closed.push(first);
    }
    closed
}

fn pad_outline_open(pad: &FootprintPad) -> Vec<Point> {
    let local = match pad.shape_kind() {
        PadShape::Circle | PadShape::Oval => {
            rounded_rect(pad.size, pad.size.x.min(pad.size.y) / 2.0)
        }
        PadShape::RoundRect { ratio } => rounded_rect(pad.size, pad.size.x.min(pad.size.y) * ratio),
        PadShape::Rect | PadShape::Unsupported => rounded_rect(pad.size, 0.0),
        PadShape::Chamfered(chamfer) => chamfered_rect(pad.size, chamfer),
        PadShape::Custom { polygon } => polygon.to_vec(),
    };
    local
        .into_iter()
        .map(|p| pad.position.offset(p.rotate(-pad.rotation)))
        .collect()
}

fn rounded_rect(size: Point, radius: f64) -> Vec<Point> {
    let hx = size.x / 2.0;
    let hy = size.y / 2.0;
    if radius <= 1e-9 {
        return vec![
            Point::new(-hx, -hy),
            Point::new(hx, -hy),
            Point::new(hx, hy),
            Point::new(-hx, hy),
        ];
    }
    let mut points = Vec::with_capacity(68);
    for (x, y, start) in [
        (hx - radius, hy - radius, 0.0),
        (-hx + radius, hy - radius, 90.0),
        (-hx + radius, -hy + radius, 180.0),
        (hx - radius, -hy + radius, 270.0),
    ] {
        for i in 0..=OUTLINE_ARC_STEPS {
            let angle = (start + i as f64 * 90.0 / OUTLINE_ARC_STEPS as f64).to_radians();
            points.push(Point::new(
                x + radius * angle.cos(),
                y + radius * angle.sin(),
            ));
        }
    }
    points
}

fn chamfered_rect(size: Point, chamfer: PadChamfer) -> Vec<Point> {
    let hx = size.x / 2.0;
    let hy = size.y / 2.0;
    let cut = size.x.min(size.y) * chamfer.ratio;
    let c = chamfer.corners;
    let mut points = Vec::with_capacity(8);
    let mut corner = |cut_it: bool, square: Point, before: Point, after: Point| {
        if cut_it {
            points.extend([before, after]);
        } else {
            points.push(square);
        }
    };
    corner(
        c.bottom_right,
        Point::new(hx, hy),
        Point::new(hx, hy - cut),
        Point::new(hx - cut, hy),
    );
    corner(
        c.bottom_left,
        Point::new(-hx, hy),
        Point::new(-hx + cut, hy),
        Point::new(-hx, hy - cut),
    );
    corner(
        c.top_left,
        Point::new(-hx, -hy),
        Point::new(-hx, -hy + cut),
        Point::new(-hx + cut, -hy),
    );
    corner(
        c.top_right,
        Point::new(hx, -hy),
        Point::new(hx - cut, -hy),
        Point::new(hx, -hy + cut),
    );
    points.dedup_by(|a, b| a.distance(*b) < 1e-12);
    #[allow(clippy::collapsible_if)]
    if points.len() > 1 {
        if let (Some(first), Some(last)) = (points.first(), points.last()) {
            if first.distance(*last) < 1e-12 {
                points.pop();
            }
        }
    }
    points
}

/// True when two pad outlines share interior area.
///
/// # Rules
/// - Uses polygon intersection tests from pcbcraft-geom
/// - Handles all pad shapes via pad_outline_closed
///
/// # Dependencies
/// - crates/library/src/footprint_pad_outline.rs [`pad_outline_closed`] for outline extraction
/// - crates/library/src/footprint_pad_outline.rs [`pcbcraft_geom::point_in_polygon`] for containment tests
///
/// # Do not
/// - Use this for clearance checks; it detects overlap, not proximity
pub fn pad_outlines_overlap(a: &FootprintPad, b: &FootprintPad) -> bool {
    let a = pad_outline_closed(a);
    let b = pad_outline_closed(b);
    polygons_overlap(&a, &b)
}

fn open_ring(closed: &[Point]) -> Vec<Point> {
    if closed.len() < 2 {
        return closed.to_vec();
    }
    let mut ring = closed.to_vec();
    #[allow(clippy::collapsible_if)]
    if ring.len() > 1 {
        if let (Some(first), Some(last)) = (ring.first(), ring.last()) {
            if first.distance(*last) < 1e-9 {
                ring.pop();
            }
        }
    }
    ring
}

fn polygons_overlap(a: &[Point], b: &[Point]) -> bool {
    let a_ring = open_ring(a);
    let b_ring = open_ring(b);
    if a_ring.len() < 3 || b_ring.len() < 3 {
        return false;
    }
    if a_ring.iter().any(|p| point_in_polygon(*p, &b_ring)) {
        return true;
    }
    if b_ring.iter().any(|p| point_in_polygon(*p, &a_ring)) {
        return true;
    }
    for (a1, a2) in polygon_edges(&a_ring) {
        for (b1, b2) in polygon_edges(&b_ring) {
            if segments_cross(a1, a2, b1, b2) {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ChamferCorners, FootprintPad, PadChamfer};
    use pcbcraft_geom::Point;

    fn smd_rect(at: Point, size: Point) -> FootprintPad {
        FootprintPad {
            number: "1".into(),
            pad_type: "smd".into(),
            shape: "rect".into(),
            position: at,
            rotation: 0.0,
            size,
            layers: vec!["F.Cu".into()],
            drill: None,
            roundrect_ratio: None,
            chamfer: None,
            polygon: vec![],
            raw_source: String::new(),
        }
    }

    #[test]
    fn pad_outline_closed_repeats_first_vertex_on_rect_pad() {
        let pad = smd_rect(Point::new(10.0, 20.0), Point::new(2.0, 4.0));
        let outline = pad_outline_closed(&pad);
        assert_eq!(outline.len(), 5);
        assert_eq!(outline.first(), outline.last());
        assert_eq!(outline[0], Point::new(9.0, 18.0));
        assert_eq!(outline[1], Point::new(11.0, 18.0));
        assert_eq!(outline[2], Point::new(11.0, 22.0));
        assert_eq!(outline[3], Point::new(9.0, 22.0));
    }

    #[test]
    fn pad_outlines_overlap_detects_shared_interior_not_mere_touch() {
        let left = smd_rect(Point::new(-1.0, 0.0), Point::new(2.0, 2.0));
        let right = smd_rect(Point::new(1.0, 0.0), Point::new(2.0, 2.0));
        assert!(
            pad_outlines_overlap(&left, &right),
            "1 mm overlap on 2 mm pads"
        );
        let gap = smd_rect(Point::new(2.01, 0.0), Point::new(2.0, 2.0));
        assert!(
            !pad_outlines_overlap(&left, &gap),
            "separated pads must not overlap"
        );
    }

    #[test]
    fn chamfered_pad_outline_applies_corner_cuts() {
        let pad = FootprintPad {
            number: "1".into(),
            pad_type: "smd".into(),
            shape: "chamfered".into(),
            position: Point::new(0.0, 0.0),
            rotation: 0.0,
            size: Point::new(2.0, 2.0),
            layers: vec!["F.Cu".into()],
            drill: None,
            roundrect_ratio: None,
            chamfer: Some(PadChamfer {
                ratio: 0.25,
                corners: ChamferCorners {
                    top_right: true,
                    ..Default::default()
                },
            }),
            polygon: vec![],
            raw_source: String::new(),
        };
        let open_len = pad_outline_closed(&pad).len().saturating_sub(1);
        assert_eq!(open_len, 5, "one chamfered corner adds a vertex");
    }
}
