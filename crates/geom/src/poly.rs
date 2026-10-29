//! Polygon booleans and offsets for copper pours.
//!
//! All coordinates are millimetres. Every operation converts its input to a
//! fixed grid of one nanometre per unit before it runs, so a fill and the
//! Gerber written from it are bit-identical on every platform.
//!
//! A [`Piece`] is one connected area of copper: an outer ring first, then the
//! holes cut out of it. A [`Region`] is a set of pieces, so each piece is a
//! separate island. Rings come back closed: the first vertex is repeated last.
//!
//! Callers bound their own input. A boolean never fails, and [`offset`] returns
//! `None` only when the requested offset leaves the integer coordinate range.

use crate::{Point, polygon_area};
use i_overlay::core::fill_rule::FillRule;
use i_overlay::core::overlay::Overlay;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::i_float::int::angle::Angle;
use i_overlay::i_float::int::point::IntPoint;
use i_overlay::mesh::int::arc::ArcOptions;
use i_overlay::mesh::int::outline::offset::IntOutlineOffset;
use i_overlay::mesh::int::style::{IntLineJoin, IntOutlineStyle};

/// Grid units per millimetre: one nanometre per unit.
pub const UNITS_PER_MM: f64 = 1_000_000.0;

/// One closed ring of a piece, in millimetres.
pub type Ring = Vec<Point>;
/// One connected area of copper: the outer ring followed by its holes.
pub type Piece = Vec<Ring>;
/// A set of pieces, listed outer first in each piece.
pub type Region = Vec<Piece>;

fn int_point(point: Point) -> IntPoint<i64> {
    IntPoint::new(
        (point.x * UNITS_PER_MM).round() as i64,
        (point.y * UNITS_PER_MM).round() as i64,
    )
}

fn float_point(point: IntPoint<i64>) -> Point {
    Point::new(point.x as f64 / UNITS_PER_MM, point.y as f64 / UNITS_PER_MM)
}

/// Outer rings run counter-clockwise and holes run clockwise, the winding the
/// boolean engine reads back out again.
fn oriented(ring: &[Point], outer: bool) -> Vec<IntPoint<i64>> {
    let mut points: Vec<IntPoint<i64>> = ring.iter().map(|p| int_point(*p)).collect();
    if points.first() == points.last() {
        points.pop();
    }
    let ccw = polygon_area(ring) > 0.0;
    if ccw != outer {
        points.reverse();
    }
    points
}

fn to_piece(rings: &[Vec<IntPoint<i64>>]) -> Piece {
    let mut piece: Piece = Vec::with_capacity(rings.len());
    for ring in rings {
        let mut points = Vec::with_capacity(ring.len() + 1);
        points.extend(ring.iter().map(|p| float_point(*p)));
        if let Some(first) = points.first().copied()
            && points.last() != Some(&first)
        {
            points.push(first);
        }
        piece.push(points);
    }
    piece
}

fn from_region(region: &[Piece]) -> Vec<Vec<IntPoint<i64>>> {
    region
        .iter()
        .flat_map(|piece| {
            piece
                .iter()
                .enumerate()
                .map(|(index, ring)| oriented(ring, index == 0))
                .filter(|ring| ring.len() >= 3)
        })
        .collect()
}

fn boolean(subject: &[Piece], clip: &[Piece], rule: OverlayRule) -> Region {
    let subject = from_region(subject);
    let clip = from_region(clip);
    let mut overlay = Overlay::<i64>::from_subj_and_clip(&subject, &clip);
    overlay
        .overlay(rule, FillRule::NonZero)
        .iter()
        .map(|shape| to_piece(shape))
        .collect()
}

/// Every piece of both regions, merged where they touch or overlap.
pub fn union(subject: &[Piece], clip: &[Piece]) -> Region {
    boolean(subject, clip, OverlayRule::Union)
}

/// The parts of `subject` that do not fall inside `clip`.
pub fn difference(subject: &[Piece], clip: &[Piece]) -> Region {
    boolean(subject, clip, OverlayRule::Difference)
}

/// The parts of `subject` that also fall inside `clip`.
pub fn intersect(subject: &[Piece], clip: &[Piece]) -> Region {
    boolean(subject, clip, OverlayRule::Intersect)
}

/// Grow every piece by `delta` millimetres with round joins, or shrink it when
/// `delta` is negative. Holes shrink or grow the opposite way, so this is the
/// copper outline moving outward or inward as a whole. `None` means the grown
/// coordinates left the range of the integer engine.
pub fn offset(region: &[Piece], delta: f64) -> Option<Region> {
    if !delta.is_finite() {
        return None;
    }
    if delta.abs() < 1e-9 {
        return Some(union(region, &[]));
    }
    let step =
        Angle::from_radians(5.0_f64.to_radians()).unwrap_or_else(|| Angle::from_bits(1 << 26));
    let mut style = IntOutlineStyle::new((delta * UNITS_PER_MM).round() as i64);
    style.join = IntLineJoin::Round(ArcOptions {
        max_step: step,
        ..ArcOptions::default()
    });
    let mut grown: Region = Vec::new();
    for piece in region {
        let shape: Vec<Vec<IntPoint<i64>>> = piece
            .iter()
            .enumerate()
            .map(|(index, ring)| oriented(ring, index == 0))
            .filter(|ring| ring.len() >= 3)
            .collect();
        let shapes = shape.outline(&style).ok()?;
        grown.extend(shapes.iter().map(|s| to_piece(s)));
    }
    Some(union(&grown, &[]))
}

/// Signed area of one piece in square millimetres: the outer ring less its
/// holes. Negative winding is normalised away, so the result is never below 0.
pub fn piece_area(piece: &Piece) -> f64 {
    let Some((outer, holes)) = piece.split_first() else {
        return 0.0;
    };
    holes
        .iter()
        .fold(polygon_area(outer).abs(), |area, hole| {
            area - polygon_area(hole).abs()
        })
        .max(0.0)
}

/// Total copper area of a region in square millimetres.
pub fn region_area(region: &[Piece]) -> f64 {
    region.iter().map(piece_area).sum()
}

/// True when a point falls on the copper of a piece, inside the outer ring and
/// outside every hole.
pub fn piece_contains(piece: &Piece, point: Point) -> bool {
    let Some((outer, holes)) = piece.split_first() else {
        return false;
    };
    crate::point_in_polygon(point, outer)
        && !holes
            .iter()
            .any(|hole| crate::point_in_polygon(point, hole))
}

/// True when a point falls on any copper of a region.
pub fn region_contains(region: &[Piece], point: Point) -> bool {
    region.iter().any(|piece| piece_contains(piece, point))
}

/// A circle as a polygon that is never inside the true circle, so copper that
/// is knocked out around it always clears the circle itself.
pub fn circle(center: Point, radius: f64) -> Piece {
    const SIDES: usize = 32;
    circle_with(center, radius, SIDES)
}

fn circle_with(center: Point, radius: f64, sides: usize) -> Piece {
    let sides = sides.max(8);
    let grow = 1.0 / (std::f64::consts::PI / sides as f64).cos();
    let mut ring = Vec::with_capacity(sides + 1);
    for i in 0..sides {
        let angle = std::f64::consts::TAU * i as f64 / sides as f64;
        ring.push(Point::new(
            center.x + radius * grow * angle.cos(),
            center.y + radius * grow * angle.sin(),
        ));
    }
    ring.push(ring[0]);
    vec![ring]
}

/// A straight track as a capsule: the band from `start` to `end` grown by
/// `radius`, with round ends.
pub fn capsule(start: Point, end: Point, radius: f64) -> Piece {
    use std::f64::consts::{FRAC_PI_2, PI};
    const SIDES: usize = 16;
    let (dx, dy) = (end.x - start.x, end.y - start.y);
    if dx.hypot(dy) < 1e-9 || radius <= 0.0 {
        return circle(start, radius.max(0.0));
    }
    let along = dy.atan2(dx);
    let at = |center: Point, angle: f64| {
        Point::new(
            center.x + radius * angle.cos(),
            center.y + radius * angle.sin(),
        )
    };
    let mut ring = Vec::with_capacity(SIDES * 2 + 4);
    // The north side, then the cap round the far end, the south side and the
    // cap round the near end.
    ring.push(at(start, along + FRAC_PI_2));
    ring.push(at(end, along + FRAC_PI_2));
    for i in 1..SIDES {
        ring.push(at(end, along + FRAC_PI_2 - PI * i as f64 / SIDES as f64));
    }
    ring.push(at(end, along - FRAC_PI_2));
    ring.push(at(start, along - FRAC_PI_2));
    for i in 1..SIDES {
        ring.push(at(start, along - FRAC_PI_2 - PI * i as f64 / SIDES as f64));
    }
    ring.push(ring[0]);
    vec![ring]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(x: f64, y: f64, size: f64) -> Piece {
        vec![vec![
            Point::new(x, y),
            Point::new(x + size, y),
            Point::new(x + size, y + size),
            Point::new(x, y + size),
            Point::new(x, y),
        ]]
    }

    #[test]
    fn difference_leaves_a_hole_and_keeps_the_area() {
        let outer = square(0.0, 0.0, 10.0);
        let inner = square(4.0, 4.0, 2.0);
        let cut = difference(&[outer], &[inner]);
        assert_eq!(cut.len(), 1);
        let piece = &cut[0];
        assert_eq!(piece.len(), 2, "the knockout is a hole, not a second ring");
        assert!((piece_area(piece) - 96.0).abs() < 1e-6);
        assert!(piece_contains(piece, Point::new(1.0, 1.0)));
        assert!(!piece_contains(piece, Point::new(5.0, 5.0)));
    }

    #[test]
    fn union_merges_touching_pieces_and_intersect_keeps_the_overlap() {
        let a = square(0.0, 0.0, 4.0);
        let b = square(2.0, 0.0, 4.0);
        let merged = union(std::slice::from_ref(&a), std::slice::from_ref(&b));
        assert_eq!(merged.len(), 1);
        assert!((region_area(&merged) - 24.0).abs() < 1e-6);
        let shared = intersect(&[a], &[b]);
        assert!((region_area(&shared) - 8.0).abs() < 1e-6);
    }

    #[test]
    fn offset_grows_and_shrinks_by_the_requested_amount() {
        let grown = offset(&[square(0.0, 0.0, 10.0)], 1.0).unwrap_or_default();
        // 100 mm^2, plus a 1 mm band round the 40 mm perimeter, plus the
        // quarter discs at the four corners.
        assert!((region_area(&grown) - (140.0 + std::f64::consts::PI)).abs() < 0.2);
        let shrunk = offset(&[square(0.0, 0.0, 10.0)], -1.0).unwrap_or_default();
        assert!((region_area(&shrunk) - 64.0).abs() < 0.2);
    }

    #[test]
    fn circles_and_capsules_cover_their_centre_line() {
        let dot = circle(Point::new(5.0, 5.0), 1.0);
        assert!(piece_contains(&dot, Point::new(5.0, 5.5)));
        assert!(!piece_contains(&dot, Point::new(6.5, 5.0)));
        let bar = capsule(Point::new(0.0, 0.0), Point::new(10.0, 0.0), 0.5);
        assert!(piece_contains(&bar, Point::new(5.0, 0.3)));
        assert!(!piece_contains(&bar, Point::new(5.0, 0.8)));
    }
}
