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
        PadShape::RoundRect { ratio } => {
            rounded_rect(pad.size, pad.size.x.min(pad.size.y) * ratio)
        }
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
    if a_ring
        .iter()
        .any(|p| point_in_polygon(*p, &b_ring))
    {
        return true;
    }
    if b_ring
        .iter()
        .any(|p| point_in_polygon(*p, &a_ring))
    {
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
