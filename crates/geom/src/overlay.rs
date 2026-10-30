//! Boolean union and offset for simple polygon rings (millimetres).
use crate::Point;
use i_float::float::compatible::FloatPointCompatible;
use i_overlay::core::fill_rule::FillRule;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::float::single::SingleFloatOverlay;
use i_overlay::mesh::float::outline::offset::OutlineOffset;
use i_overlay::mesh::float::style::{LineJoin, OutlineStyle};

/// Closed vertex ring in board space (first point may repeat at end).
pub type Ring = Vec<Point>;

#[derive(Clone, Copy, Debug, PartialEq)]
/// Corner style when a ring is grown or shrunk.
pub enum OffsetJoin {
    /// Corners are rounded with a circular arc, so growing a ring keeps a constant distance from
    /// the original.
    Round,
}

const SCALE: f64 = 1_000_000.0;

impl FloatPointCompatible for Point {
    type Scalar = f64;

    fn from_xy(x: f64, y: f64) -> Self {
        Point::new(x, y)
    }

    fn x(&self) -> f64 {
        self.x
    }

    fn y(&self) -> f64 {
        self.y
    }
}

fn open_ring(ring: &Ring) -> Vec<Point> {
    let mut pts: Vec<Point> = ring.iter().copied().filter(|p| p.valid()).collect();
    if pts.len() >= 2
        && pts
            .first()
            .zip(pts.last())
            .is_some_and(|(a, b)| a.distance(*b) < 1e-9)
    {
        pts.pop();
    }
    pts
}

fn ensure_ccw(points: &mut [Point]) {
    if crate::polygon_area(points) < 0.0 {
        points.reverse();
    }
}

fn ring_to_shape(ring: &Ring) -> Option<Vec<Vec<Point>>> {
    let mut outer = open_ring(ring);
    if outer.len() < 3 {
        return None;
    }
    ensure_ccw(&mut outer);
    Some(vec![outer])
}

fn rings_from_shapes(shapes: &[Vec<Vec<Point>>]) -> Vec<Ring> {
    let mut out = Vec::new();
    for shape in shapes {
        for contour in shape {
            if contour.len() < 3 {
                continue;
            }
            let mut ring = contour.clone();
            if let Some(first) = ring.first().copied() {
                ring.push(first);
            }
            out.push(ring);
        }
    }
    out
}

/// Union of filled regions bounded by the given rings.
pub fn polygon_union(polys: &[Ring]) -> Vec<Ring> {
    let mut acc: Vec<Vec<Vec<Point>>> = vec![];
    for ring in polys {
        let Some(shape) = ring_to_shape(ring) else {
            continue;
        };
        if acc.is_empty() {
            acc = vec![shape];
        } else {
            acc = acc.overlay(&shape, OverlayRule::Union, FillRule::EvenOdd);
        }
    }
    rings_from_shapes(&acc)
}

/// Expand (positive delta) or shrink (negative delta) polygon rings.
pub fn polygon_offset(polys: &[Ring], delta_mm: f64, join: OffsetJoin) -> Vec<Ring> {
    if polys.is_empty() || delta_mm.abs() < 1e-12 {
        return polys.to_vec();
    }
    let mut shapes: Vec<Vec<Vec<Point>>> = vec![];
    for ring in polys {
        if let Some(shape) = ring_to_shape(ring) {
            shapes.push(shape);
        }
    }
    if shapes.is_empty() {
        return vec![];
    }
    let round = match join {
        OffsetJoin::Round => {
            let r = delta_mm.abs().min(0.5);
            LineJoin::Round(r)
        }
    };
    let style = OutlineStyle::new(delta_mm).line_join(round);
    let result = shapes
        .outline_fixed_scale(&style, SCALE)
        .unwrap_or_default();
    rings_from_shapes(&result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(half: f64) -> Ring {
        vec![
            Point::new(-half, -half),
            Point::new(half, -half),
            Point::new(half, half),
            Point::new(-half, half),
            Point::new(-half, -half),
        ]
    }

    #[test]
    fn polygon_union_merges_overlapping_pads_into_one_ring() {
        let a = vec![
            Point::new(0.0, 0.0),
            Point::new(2.0, 0.0),
            Point::new(2.0, 2.0),
            Point::new(0.0, 2.0),
            Point::new(0.0, 0.0),
        ];
        let b = vec![
            Point::new(1.0, 1.0),
            Point::new(3.0, 1.0),
            Point::new(3.0, 3.0),
            Point::new(1.0, 3.0),
            Point::new(1.0, 1.0),
        ];
        let merged = polygon_union(&[a, b]);
        assert_eq!(merged.len(), 1);
        assert!(crate::polygon_area(&open_ring(&merged[0])).abs() > 6.9);
    }

    #[test]
    fn polygon_offset_grows_a_square_by_the_delta_with_round_corners() {
        let s = square(1.0);
        let grown = polygon_offset(&[s], 0.2, OffsetJoin::Round);
        assert_eq!(grown.len(), 1);
        let area = crate::polygon_area(&open_ring(&grown[0])).abs();
        assert!(area > 2.0 * 2.0);
        assert!(area < 2.4 * 2.4);
    }

    #[test]
    fn polygon_offset_is_bit_identical_on_repeat() {
        let s = square(2.0);
        let once = polygon_offset(std::slice::from_ref(&s), 0.15, OffsetJoin::Round);
        let twice = polygon_offset(&[s], 0.15, OffsetJoin::Round);
        assert_eq!(once, twice);
    }
}
