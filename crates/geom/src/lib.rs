//! Small, bounded planar geometry. Units are millimetres.
pub mod impedance;
pub mod meander;
use serde::{Deserialize, Serialize};

pub mod hull;
pub mod poly;

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
    pub fn valid(self) -> bool {
        self.x.is_finite()
            && self.y.is_finite()
            && self.x.abs() <= 100_000.0
            && self.y.abs() <= 100_000.0
    }
    pub fn distance(self, other: Self) -> f64 {
        (self.x - other.x).hypot(self.y - other.y)
    }
    pub fn offset(self, other: Self) -> Self {
        Self::new(self.x + other.x, self.y + other.y)
    }
    pub fn rotate(self, degrees: f64) -> Self {
        let (s, c) = degrees.to_radians().sin_cos();
        Self::new(self.x * c - self.y * s, self.x * s + self.y * c)
    }
}

pub fn positive(value: f64) -> bool {
    value.is_finite() && value > 0.0 && value <= 10_000.0
}
/// A circular arc: centre, radius, the angle of the start point and a signed
/// sweep in radians to the end point (positive turns anticlockwise).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Arc {
    pub center: Point,
    pub radius: f64,
    pub start_angle: f64,
    pub sweep: f64,
}

impl Arc {
    /// The point at fraction `t` of the sweep, 0 at the start and 1 at the end.
    pub fn point_at(&self, t: f64) -> Point {
        let angle = self.start_angle + self.sweep * t;
        Point::new(
            self.center.x + self.radius * angle.cos(),
            self.center.y + self.radius * angle.sin(),
        )
    }
    /// Arc length in millimetres.
    pub fn length(&self) -> f64 {
        self.radius * self.sweep.abs()
    }
}

/// The unique circular arc through `start`, `mid` and `end`, or none when the
/// three points are collinear or repeated.
pub fn arc_through(start: Point, mid: Point, end: Point) -> Option<Arc> {
    let d =
        2.0 * (start.x * (mid.y - end.y) + mid.x * (end.y - start.y) + end.x * (start.y - mid.y));
    if !d.is_finite() || d.abs() < 1e-9 {
        return None;
    }
    let norm2 = |p: Point| p.x * p.x + p.y * p.y;
    let center = Point::new(
        (norm2(start) * (mid.y - end.y)
            + norm2(mid) * (end.y - start.y)
            + norm2(end) * (start.y - mid.y))
            / d,
        (norm2(start) * (end.x - mid.x)
            + norm2(mid) * (start.x - end.x)
            + norm2(end) * (mid.x - start.x))
            / d,
    );
    let radius = center.distance(start);
    if !center.valid() || !radius.is_finite() || radius <= 0.0 {
        return None;
    }
    let angle = |p: Point| (p.y - center.y).atan2(p.x - center.x);
    let wrap = |a: f64| {
        let mut a = a % (2.0 * std::f64::consts::PI);
        if a < 0.0 {
            a += 2.0 * std::f64::consts::PI;
        }
        a
    };
    let (a0, am, a1) = (angle(start), angle(mid), angle(end));
    let ccw = wrap(a1 - a0);
    let mid_ccw = wrap(am - a0);
    let sweep = if mid_ccw <= ccw {
        ccw
    } else {
        ccw - 2.0 * std::f64::consts::PI
    };
    if sweep.abs() < 1e-9 {
        return None;
    }
    Some(Arc {
        center,
        radius,
        start_angle: a0,
        sweep,
    })
}

pub fn point_segment(p: Point, a: Point, b: Point) -> f64 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let length = dx * dx + dy * dy;
    if length < 1e-20 {
        return p.distance(a);
    }
    let t = (((p.x - a.x) * dx + (p.y - a.y) * dy) / length).clamp(0.0, 1.0);
    p.distance(Point::new(a.x + t * dx, a.y + t * dy))
}

fn cross(a: Point, b: Point, c: Point) -> f64 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}

pub fn segment_distance(a: Point, b: Point, c: Point, d: Point) -> f64 {
    let ab_c = cross(a, b, c);
    let ab_d = cross(a, b, d);
    let cd_a = cross(c, d, a);
    let cd_b = cross(c, d, b);
    if ab_c * ab_d < 0.0 && cd_a * cd_b < 0.0 {
        return 0.0;
    }
    point_segment(a, c, d)
        .min(point_segment(b, c, d))
        .min(point_segment(c, a, b))
        .min(point_segment(d, a, b))
}

/// True when the open segments AB and CD cross at a single interior point.
pub fn segments_cross(a: Point, b: Point, c: Point, d: Point) -> bool {
    cross(a, b, c) * cross(a, b, d) < 0.0 && cross(c, d, a) * cross(c, d, b) < 0.0
}

/// Shoelace area of an open vertex ring; positive for counterclockwise in y-up axes.
pub fn polygon_area(points: &[Point]) -> f64 {
    let Some(last) = points.last().copied() else {
        return 0.0;
    };
    let mut previous = last;
    let mut twice = 0.0;
    for &current in points {
        twice += previous.x * current.y - current.x * previous.y;
        previous = current;
    }
    twice / 2.0
}

/// Even-odd point containment for an open or closed vertex ring.
pub fn point_in_polygon(point: Point, points: &[Point]) -> bool {
    let Some(mut previous) = points.last().copied() else {
        return false;
    };
    let mut inside = false;
    for &current in points {
        if (current.y > point.y) != (previous.y > point.y)
            && point.x
                < (previous.x - current.x) * (point.y - current.y) / (previous.y - current.y)
                    + current.x
        {
            inside = !inside;
        }
        previous = current;
    }
    inside
}

/// Edges of an open vertex ring, including the closing edge.
pub fn polygon_edges(points: &[Point]) -> impl Iterator<Item = (Point, Point)> + '_ {
    let last = points.last().copied();
    last.into_iter()
        .chain(points.iter().copied())
        .zip(points.iter().copied())
}

/// Minimum distance from a point to the boundary of an open vertex ring.
pub fn polygon_boundary_distance(point: Point, points: &[Point]) -> f64 {
    polygon_edges(points)
        .map(|(a, b)| point_segment(point, a, b))
        .fold(f64::INFINITY, f64::min)
}

/// A simple polygon has at least three distinct vertices, non-zero area, and no
/// edge touching any non-adjacent edge.
pub fn polygon_is_simple(points: &[Point]) -> bool {
    let n = points.len();
    if n < 3 || polygon_area(points).abs() < 1e-9 {
        return false;
    }
    let edges: Vec<(Point, Point)> = polygon_edges(points).collect();
    if edges.iter().any(|(a, b)| a.distance(*b) < 1e-9) {
        return false;
    }
    for (i, (a, b)) in edges.iter().enumerate() {
        for (j, (c, d)) in edges.iter().enumerate().skip(i + 1) {
            // Adjacent edges share one vertex; they must not fold back over each other.
            let folds = |shared: Point, first: Point, second: Point| {
                cross(shared, first, second).abs() < 1e-12
                    && (first.x - shared.x) * (second.x - shared.x)
                        + (first.y - shared.y) * (second.y - shared.y)
                        > 0.0
            };
            if j == i + 1 {
                if folds(*b, *a, *d) {
                    return false;
                }
                continue;
            }
            if i == 0 && j == n - 1 {
                if folds(*a, *b, *c) {
                    return false;
                }
                continue;
            }
            if segment_distance(*a, *b, *c, *d) < 1e-9 {
                return false;
            }
        }
    }
    true
}

/// Ear-clipping triangulation of a simple polygon given as an open vertex ring.
/// Returns vertex index triples; an empty result means the ring is degenerate.
pub fn triangulate(points: &[Point]) -> Vec<[usize; 3]> {
    let mut remaining: Vec<usize> = (0..points.len()).collect();
    let orientation = polygon_area(points).signum();
    let at = |i: usize| points.get(i).copied().unwrap_or_default();
    let mut triangles = Vec::with_capacity(points.len().saturating_sub(2));
    let mut guard = points.len() * points.len() + 8;
    while remaining.len() > 3 && guard > 0 {
        guard -= 1;
        let n = remaining.len();
        let mut clipped = false;
        // First insist that no other vertex touches the ear, which keeps
        // collinear vertices out of the diagonal; fall back to the strict
        // test only when every ear is blocked that way.
        for strict in [false, true] {
            if clipped {
                break;
            }
            for k in 0..n {
                let (Some(&ia), Some(&ib), Some(&ic)) = (
                    remaining.get((k + n - 1) % n),
                    remaining.get(k),
                    remaining.get((k + 1) % n),
                ) else {
                    return vec![];
                };
                let (a, b, c) = (at(ia), at(ib), at(ic));
                let turn = (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
                if turn * orientation <= 1e-15 {
                    continue;
                }
                let contains_other = remaining.iter().any(|&other| {
                    if other == ia || other == ib || other == ic {
                        return false;
                    }
                    let p = at(other);
                    let d1 = cross(a, b, p) * orientation;
                    let d2 = cross(b, c, p) * orientation;
                    let d3 = cross(c, a, p) * orientation;
                    if strict {
                        d1 > 0.0 && d2 > 0.0 && d3 > 0.0
                    } else {
                        d1 >= -1e-12 && d2 >= -1e-12 && d3 >= -1e-12 && p != a && p != b && p != c
                    }
                });
                if contains_other {
                    continue;
                }
                triangles.push([ia, ib, ic]);
                remaining.remove(k);
                clipped = true;
                break;
            }
        }
        if !clipped {
            return vec![];
        }
    }
    if let [a, b, c] = remaining[..] {
        triangles.push([a, b, c]);
    }
    triangles
}

#[cfg(test)]
mod tests {
    use super::*;
    fn l_shape() -> Vec<Point> {
        vec![
            Point::new(0.0, 0.0),
            Point::new(2.0, 0.0),
            Point::new(2.0, 1.0),
            Point::new(1.0, 1.0),
            Point::new(1.0, 2.0),
            Point::new(0.0, 2.0),
        ]
    }
    #[test]
    fn concave_polygons_are_simple_and_triangulate_to_their_area() {
        let l = l_shape();
        assert!(polygon_is_simple(&l));
        assert!((polygon_area(&l).abs() - 3.0).abs() < 1e-12);
        let triangles = triangulate(&l);
        assert_eq!(triangles.len(), 4);
        let area: f64 = triangles
            .iter()
            .map(|t| {
                polygon_area(&[
                    l.get(t[0]).copied().unwrap_or_default(),
                    l.get(t[1]).copied().unwrap_or_default(),
                    l.get(t[2]).copied().unwrap_or_default(),
                ])
                .abs()
            })
            .sum();
        assert!((area - 3.0).abs() < 1e-12);
        assert!(point_in_polygon(Point::new(0.5, 1.5), &l));
        assert!(!point_in_polygon(Point::new(1.5, 1.5), &l));
        assert!((polygon_boundary_distance(Point::new(0.5, 0.5), &l) - 0.5).abs() < 1e-12);
    }
    #[test]
    fn bow_ties_spikes_and_slivers_are_not_simple() {
        let bow_tie = [
            Point::new(0.0, 0.0),
            Point::new(1.0, 1.0),
            Point::new(1.0, 0.0),
            Point::new(0.0, 1.0),
        ];
        assert!(!polygon_is_simple(&bow_tie));
        assert!(segments_cross(
            bow_tie[0], bow_tie[1], bow_tie[2], bow_tie[3]
        ));
        let line = [
            Point::new(0.0, 0.0),
            Point::new(1.0, 0.0),
            Point::new(2.0, 0.0),
        ];
        assert!(!polygon_is_simple(&line));
        let spike = [
            Point::new(0.0, 0.0),
            Point::new(2.0, 0.0),
            Point::new(1.0, 0.0),
            Point::new(1.0, 1.0),
        ];
        assert!(!polygon_is_simple(&spike));
        let repeated = [
            Point::new(0.0, 0.0),
            Point::new(1.0, 0.0),
            Point::new(1.0, 0.0),
            Point::new(0.0, 1.0),
        ];
        assert!(!polygon_is_simple(&repeated));
    }
    #[test]
    fn crossing_and_collinear() {
        assert_eq!(
            segment_distance(
                Point::new(0.0, 0.0),
                Point::new(5.0, 5.0),
                Point::new(0.0, 5.0),
                Point::new(5.0, 0.0)
            ),
            0.0
        );
        assert_eq!(
            segment_distance(
                Point::new(0.0, 0.0),
                Point::new(1.0, 0.0),
                Point::new(3.0, 0.0),
                Point::new(4.0, 0.0)
            ),
            2.0
        );
    }
}
