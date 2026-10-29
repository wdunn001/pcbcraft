//! Pad copper outlines in footprint-local coordinates (millimetres, y down).
use crate::{FootprintPad, PadChamfer, PadShape};
use pcbcraft_geom::{Point, point_in_polygon, polygon_edges, segments_cross};

const OUTLINE_ARC_STEPS: usize = 16;

/// Closed vertex ring of the pad copper outline in footprint-local space.
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
    use pcbcraft_geom::point_in_polygon;

    fn make_pad(x: f64, y: f64, size: f64) -> FootprintPad {
        FootprintPad {
            number: "1".to_string(),
            pad_type: "thru_hole".to_string(),
            shape: "circle".to_string(),
            position: Point::new(x, y),
            rotation: 0.0,
            size: Point::new(size, size),
            layers: vec![],
            drill: None,
            roundrect_ratio: None,
            chamfer: None,
            polygon: vec![],
            raw_source: String::new(),
        }
    }

    #[test]
    fn outline_closed_is_closed_ring() {
        let pad = make_pad(0.0, 0.0, 1.0);
        let ring = pad_outline_closed(&pad);
        // Oracle: ring must have at least 4 points and be closed
        assert!(ring.len() >= 4, "outline should have at least 4 points");
        let first = ring.first().copied().unwrap();
        let last = ring.last().copied().unwrap();
        assert!(
            first.distance(last) < 1e-9,
            "outline should be closed (first == last point)"
        );
    }

    #[test]
    fn outline_closed_center_is_inside() {
        let pad = make_pad(0.0, 0.0, 2.0);
        let ring = pad_outline_closed(&pad);
        let open = open_ring(&ring);
        // Oracle: pad center must be inside its own outline
        let center = Point::new(0.0, 0.0);
        assert!(
            point_in_polygon(center, &open),
            "pad center should be inside its outline"
        );
    }

    #[test]
    fn outline_closed_outside_point_is_outside() {
        let pad = make_pad(0.0, 0.0, 2.0);
        let ring = pad_outline_closed(&pad);
        let open = open_ring(&ring);
        // Oracle: a point well outside should not be inside
        let outside = Point::new(10.0, 10.0);
        assert!(
            !point_in_polygon(outside, &open),
            "point far from pad should be outside outline"
        );
    }

    #[test]
    fn outlines_overlap_true_when_concentric() {
        let a = make_pad(0.0, 0.0, 2.0);
        let b = make_pad(0.0, 0.0, 1.0);
        // Oracle: concentric pads of different sizes must overlap
        assert!(
            pad_outlines_overlap(&a, &b),
            "concentric pads should overlap"
        );
    }

    #[test]
    fn outlines_overlap_true_when_partially_overlapping() {
        let a = make_pad(0.0, 0.0, 2.0);
        let b = make_pad(1.0, 0.0, 2.0);
        // Oracle: pads with centers 1mm apart and size 2mm overlap
        assert!(
            pad_outlines_overlap(&a, &b),
            "partially overlapping pads should detect overlap"
        );
    }

    #[test]
    fn outlines_overlap_false_when_far_apart() {
        let a = make_pad(0.0, 0.0, 1.0);
        let b = make_pad(10.0, 0.0, 1.0);
        // Oracle: pads 10mm apart with 1mm size do not overlap
        assert!(
            !pad_outlines_overlap(&a, &b),
            "far apart pads should not overlap"
        );
    }

    #[test]
    fn outlines_overlap_symmetric() {
        let a = make_pad(0.0, 0.0, 2.0);
        let b = make_pad(1.0, 0.0, 2.0);
        // Oracle: overlap is symmetric
        assert_eq!(
            pad_outlines_overlap(&a, &b),
            pad_outlines_overlap(&b, &a),
            "overlap should be symmetric"
        );
    }
}
