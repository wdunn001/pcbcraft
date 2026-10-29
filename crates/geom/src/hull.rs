//! Octagonal hulls whose edges run only at 0, 45 and 90 degrees.
//!
//! A hull is the tightest octagon, in the eight fixed directions, that contains
//! a point set grown by a margin. Any path that follows a hull boundary is
//! therefore a legal 45-degree path, and every corner on it is 135 degrees.
use crate::Point;

const S: f64 = std::f64::consts::FRAC_1_SQRT_2;
const DIAGONALS: [(f64, f64); 4] = [(S, S), (-S, S), (-S, -S), (S, -S)];

/// Counter-clockwise octagon around `points` grown by `inflate` millimetres.
/// Returns an empty list for empty, non-finite or negative input.
pub fn octagon(points: &[Point], inflate: f64) -> Vec<Point> {
    if points.is_empty()
        || !inflate.is_finite()
        || inflate < 0.0
        || points.iter().any(|p| !p.x.is_finite() || !p.y.is_finite())
    {
        return vec![];
    }
    let (mut min_x, mut min_y) = (f64::INFINITY, f64::INFINITY);
    let (mut max_x, mut max_y) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    for p in points {
        min_x = min_x.min(p.x);
        min_y = min_y.min(p.y);
        max_x = max_x.max(p.x);
        max_y = max_y.max(p.y);
    }
    let mut poly = vec![
        Point::new(min_x - inflate, min_y - inflate),
        Point::new(max_x + inflate, min_y - inflate),
        Point::new(max_x + inflate, max_y + inflate),
        Point::new(min_x - inflate, max_y + inflate),
    ];
    for (ux, uy) in DIAGONALS {
        let limit = points
            .iter()
            .map(|p| p.x * ux + p.y * uy)
            .fold(f64::NEG_INFINITY, f64::max)
            + inflate;
        poly = clip(&poly, ux, uy, limit);
    }
    let mut out: Vec<Point> = Vec::with_capacity(poly.len());
    for p in poly {
        if out.last().is_none_or(|q| q.distance(p) > 1e-9) {
            out.push(p);
        }
    }
    while out.len() > 1
        && let (Some(first), Some(last)) = (out.first().copied(), out.last().copied())
        && first.distance(last) <= 1e-9
    {
        out.pop();
    }
    if out.len() < 3 { vec![] } else { out }
}

fn clip(poly: &[Point], ux: f64, uy: f64, limit: f64) -> Vec<Point> {
    let mut out = Vec::with_capacity(poly.len() + 1);
    for (i, a) in poly.iter().enumerate() {
        let Some(b) = poly.get((i + 1) % poly.len()) else {
            continue;
        };
        let da = a.x * ux + a.y * uy - limit;
        let db = b.x * ux + b.y * uy - limit;
        if da <= 0.0 {
            out.push(*a);
        }
        if (da < 0.0 && db > 0.0) || (da > 0.0 && db < 0.0) {
            let t = da / (da - db);
            out.push(Point::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t));
        }
    }
    out
}

/// Signed distance from `p` to the edge `a`-`b`; positive on the inside of a
/// counter-clockwise polygon.
fn inside_distance(a: Point, b: Point, p: Point) -> f64 {
    let (ex, ey) = (b.x - a.x, b.y - a.y);
    let length = ex.hypot(ey);
    if length < 1e-12 {
        return f64::INFINITY;
    }
    (ex * (p.y - a.y) - ey * (p.x - a.x)) / length
}

/// Parameter range `(t0, t1)` of the segment `a`-`b` that lies inside the convex
/// counter-clockwise polygon shrunk by `shrink`. `None` when that range is empty.
pub fn clip_segment(poly: &[Point], a: Point, b: Point, shrink: f64) -> Option<(f64, f64)> {
    if poly.len() < 3 {
        return None;
    }
    let (mut t0, mut t1) = (0.0_f64, 1.0_f64);
    for (i, p) in poly.iter().enumerate() {
        let q = poly.get((i + 1) % poly.len())?;
        if p.distance(*q) < 1e-12 {
            continue;
        }
        let f0 = inside_distance(*p, *q, a);
        let f1 = inside_distance(*p, *q, b);
        let d = f1 - f0;
        let num = f0 - shrink;
        if d.abs() < 1e-15 {
            if num < 0.0 {
                return None;
            }
            continue;
        }
        let t = -num / d;
        if d > 0.0 {
            t0 = t0.max(t);
        } else {
            t1 = t1.min(t);
        }
        if t1 - t0 <= 1e-9 {
            return None;
        }
    }
    Some((t0, t1))
}

/// True when `p` lies deeper than `shrink` inside the convex counter-clockwise polygon.
pub fn strictly_inside(poly: &[Point], p: Point, shrink: f64) -> bool {
    poly.len() >= 3
        && poly.iter().enumerate().all(|(i, a)| {
            poly.get((i + 1) % poly.len())
                .is_some_and(|b| inside_distance(*a, *b, p) > shrink)
        })
}

/// Distance travelled counter-clockwise along the polygon boundary from its
/// first vertex to the boundary point nearest `p`, and the full perimeter.
pub fn boundary_position(poly: &[Point], p: Point) -> (f64, f64) {
    let mut best = (f64::INFINITY, 0.0);
    let mut walked = 0.0;
    for (i, a) in poly.iter().enumerate() {
        let Some(b) = poly.get((i + 1) % poly.len()) else {
            continue;
        };
        let d = crate::point_segment(p, *a, *b);
        if d < best.0 {
            best = (d, walked + a.distance(p).min(a.distance(*b)));
        }
        walked += a.distance(*b);
    }
    (best.1, walked)
}

/// Hull vertices met when walking from boundary position `from` to `to`
/// (both as returned by `boundary_position`), counter-clockwise or clockwise.
pub fn boundary_vertices(poly: &[Point], from: f64, to: f64, ccw: bool) -> Vec<Point> {
    let mut positions = Vec::with_capacity(poly.len());
    let mut walked = 0.0;
    for (i, a) in poly.iter().enumerate() {
        positions.push((walked, *a));
        let Some(b) = poly.get((i + 1) % poly.len()) else {
            continue;
        };
        walked += a.distance(*b);
    }
    let total = walked;
    if total <= 0.0 {
        return vec![];
    }
    let span = if ccw {
        (to - from).rem_euclid(total)
    } else {
        (from - to).rem_euclid(total)
    };
    let mut found: Vec<(f64, Point)> = positions
        .into_iter()
        .filter_map(|(s, v)| {
            let along = if ccw {
                (s - from).rem_euclid(total)
            } else {
                (from - s).rem_euclid(total)
            };
            (along > 1e-9 && along < span - 1e-9).then_some((along, v))
        })
        .collect();
    found.sort_by(|a, b| a.0.total_cmp(&b.0));
    found.into_iter().map(|(_, v)| v).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn octagon_around_a_point_has_only_legal_edges() {
        let hull = octagon(&[Point::new(5.0, 5.0)], 1.0);
        assert_eq!(hull.len(), 8);
        for (i, a) in hull.iter().enumerate() {
            let b = hull[(i + 1) % hull.len()];
            let angle = (b.y - a.y).atan2(b.x - a.x).to_degrees();
            let snapped = (angle / 45.0).round() * 45.0;
            assert!((angle - snapped).abs() < 1e-6, "edge at {angle}");
        }
        assert!(strictly_inside(&hull, Point::new(5.0, 5.0), 0.0));
        assert!(!strictly_inside(&hull, Point::new(6.5, 5.0), 0.0));
    }

    #[test]
    fn octagon_contains_the_grown_circle() {
        let hull = octagon(&[Point::new(0.0, 0.0)], 2.0);
        for k in 0..360 {
            let a = (k as f64).to_radians();
            let p = Point::new(1.999 * a.cos(), 1.999 * a.sin());
            assert!(strictly_inside(&hull, p, 0.0));
        }
    }

    #[test]
    fn clipping_a_crossing_segment_finds_entry_and_exit() {
        let hull = octagon(&[Point::new(0.0, 0.0)], 1.0);
        let hit = clip_segment(&hull, Point::new(-3.0, 0.0), Point::new(3.0, 0.0), 0.0);
        let (t0, t1) = hit.unwrap_or((1.0, 0.0));
        assert!((t0 - 2.0 / 6.0).abs() < 1e-9 && (t1 - 4.0 / 6.0).abs() < 1e-9);
        assert!(clip_segment(&hull, Point::new(-3.0, 1.0), Point::new(3.0, 1.0), 1e-6).is_none());
    }
}
