//! Small, bounded planar geometry. Units are millimetres.
pub mod impedance;
pub mod meander;
pub mod track_corner;
mod overlay;
use serde::{Deserialize, Serialize};

pub use overlay::{OffsetJoin, Ring, polygon_offset, polygon_union};
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

/// Parallel hatch segments filling a simple closed polygon. Caps output at 8192
/// segments.
pub fn hatch_fill_polygon(
    points: &[Point],
    line_interval_mm: f64,
    hatch_angle_deg: f64,
    angle_increment_deg: f64,
) -> Vec<(Point, Point)> {
    if !polygon_is_simple(points)
        || !line_interval_mm.is_finite()
        || line_interval_mm <= 0.0
        || !hatch_angle_deg.is_finite()
        || !angle_increment_deg.is_finite()
    {
        return vec![];
    }
    let mut segments = Vec::new();
    let mut angle = hatch_angle_deg;
    for _ in 0..8 {
        segments.extend(hatch_at_angle(points, line_interval_mm, angle));
        if segments.len() >= 8192 {
            return segments.into_iter().take(8192).collect();
        }
        angle += angle_increment_deg;
    }
    segments.into_iter().take(8192).collect()
}

fn hatch_at_angle(points: &[Point], spacing: f64, angle_deg: f64) -> Vec<(Point, Point)> {
    let (sin, cos) = angle_deg.to_radians().sin_cos();
    let (mut min_s, mut max_s) = (f64::INFINITY, f64::NEG_INFINITY);
    let mut min_t = f64::INFINITY;
    let mut max_t = f64::NEG_INFINITY;
    for p in points {
        let s = p.x * cos + p.y * sin;
        let t = -p.x * sin + p.y * cos;
        min_s = min_s.min(s);
        max_s = max_s.max(s);
        min_t = min_t.min(t);
        max_t = max_t.max(t);
    }
    if !min_s.is_finite() || max_s - min_s < 1e-9 {
        return vec![];
    }
    let mut out = Vec::new();
    let mut offset = min_s;
    while offset <= max_s + 1e-9 {
        let line = clip_hatch_line(points, offset, cos, sin, min_t, max_t);
        if let Some((a, b)) = line {
            out.push((a, b));
        }
        offset += spacing;
        if out.len() >= 8192 {
            break;
        }
    }
    out
}

fn clip_hatch_line(
    points: &[Point],
    offset: f64,
    cos: f64,
    sin: f64,
    min_t: f64,
    max_t: f64,
) -> Option<(Point, Point)> {
    let mut hits = Vec::new();
    for (a, b) in polygon_edges(points) {
        let sa = a.x * cos + a.y * sin - offset;
        let sb = b.x * cos + b.y * sin - offset;
        if sa.abs() < 1e-12 {
            hits.push(-a.x * sin + a.y * cos);
        }
        if sb.abs() < 1e-12 {
            hits.push(-b.x * sin + b.y * cos);
        }
        if sa * sb < 0.0 {
            let t = sa / (sa - sb);
            let px = a.x + (b.x - a.x) * t;
            let py = a.y + (b.y - a.y) * t;
            hits.push(-px * sin + py * cos);
        }
    }
    hits.sort_by(|x, y| x.total_cmp(y));
    hits.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
    if hits.len() < 2 {
        return None;
    }
    let t0 = hits.first()?.clamp(min_t, max_t);
    let t1 = hits.last()?.clamp(min_t, max_t);
    let p0 = Point::new(offset * cos - t0 * sin, offset * sin + t0 * cos);
    let p1 = Point::new(offset * cos - t1 * sin, offset * sin + t1 * cos);
    if p0.distance(p1) < 1e-9 {
        None
    } else {
        Some((p0, p1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fixed-seed generator so property runs are reproducible.
    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self) -> f64 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((self.0 >> 11) as f64) / ((1u64 << 53) as f64)
        }
        fn range(&mut self, lo: f64, hi: f64) -> f64 {
            lo + (hi - lo) * self.next()
        }
    }
    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }
    fn near_pt(a: Point, b: Point) -> bool {
        near(a.x, b.x) && near(a.y, b.y)
    }

    #[test]
    fn offset_adds_components() {
        let r = Point::new(1.5, -2.0).offset(Point::new(0.5, 7.0));
        assert_eq!(r, Point::new(2.0, 5.0));
        let mut g = Lcg(1);
        for _ in 0..200 {
            let a = Point::new(g.range(-50.0, 50.0), g.range(-50.0, 50.0));
            let b = Point::new(g.range(-50.0, 50.0), g.range(-50.0, 50.0));
            assert_eq!(a.offset(b), b.offset(a));
        }
    }

    #[test]
    fn rotate_quarter_turn_maps_x_axis_to_y_axis() {
        assert!(near_pt(
            Point::new(1.0, 0.0).rotate(90.0),
            Point::new(0.0, 1.0)
        ));
        assert!(near_pt(
            Point::new(0.0, 1.0).rotate(90.0),
            Point::new(-1.0, 0.0)
        ));
    }

    #[test]
    fn rotate_half_turn_negates() {
        assert!(near_pt(
            Point::new(3.0, 4.0).rotate(180.0),
            Point::new(-3.0, -4.0)
        ));
    }

    #[test]
    fn rotate_preserves_length_and_inverts() {
        let mut g = Lcg(2);
        for _ in 0..200 {
            let p = Point::new(g.range(-100.0, 100.0), g.range(-100.0, 100.0));
            let a = g.range(-720.0, 720.0);
            let r = p.rotate(a);
            assert!(near(
                r.distance(Point::default()),
                p.distance(Point::default())
            ));
            assert!(near_pt(r.rotate(-a), p));
        }
    }

    #[test]
    fn positive_accepts_only_finite_values_up_to_10000() {
        for v in [1e-9, 0.1, 1.0, 10_000.0] {
            assert!(positive(v), "{v}");
        }
        for v in [
            0.0,
            -0.0,
            -1.0,
            10_000.5,
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
        ] {
            assert!(!positive(v), "{v}");
        }
    }

    #[test]
    fn arc_point_at_hits_start_mid_and_end() {
        let arc = Arc {
            center: Point::new(1.0, 1.0),
            radius: 2.0,
            start_angle: 0.0,
            sweep: std::f64::consts::FRAC_PI_2,
        };
        assert!(near_pt(arc.point_at(0.0), Point::new(3.0, 1.0)));
        assert!(near_pt(arc.point_at(1.0), Point::new(1.0, 3.0)));
        let r = 2.0 * std::f64::consts::FRAC_1_SQRT_2;
        assert!(near_pt(arc.point_at(0.5), Point::new(1.0 + r, 1.0 + r)));
    }

    #[test]
    fn arc_length_is_radius_times_abs_sweep() {
        let mut arc = Arc {
            center: Point::default(),
            radius: 3.0,
            start_angle: 0.3,
            sweep: std::f64::consts::PI,
        };
        assert!(near(arc.length(), 3.0 * std::f64::consts::PI));
        arc.sweep = -std::f64::consts::FRAC_PI_2;
        assert!(near(arc.length(), 1.5 * std::f64::consts::PI));
    }

    #[test]
    fn arc_through_unit_circle_half_turn() {
        let (a, m, b) = (
            Point::new(1.0, 0.0),
            Point::new(0.0, 1.0),
            Point::new(-1.0, 0.0),
        );
        let ccw = arc_through(a, m, b);
        assert!(ccw.is_some());
        let ccw = ccw.unwrap_or(Arc {
            center: a,
            radius: 0.0,
            start_angle: 0.0,
            sweep: 0.0,
        });
        assert!(near_pt(ccw.center, Point::default()) && near(ccw.radius, 1.0));
        assert!(near(ccw.start_angle, 0.0) && near(ccw.sweep, std::f64::consts::PI));
        let cw = arc_through(b, m, a);
        assert!(cw.is_some_and(|c| near(c.sweep, -std::f64::consts::PI)));
    }

    #[test]
    fn arc_through_collinear_or_repeated_is_none() {
        let o = Point::new(0.0, 0.0);
        let x = Point::new(1.0, 1.0);
        assert!(arc_through(o, x, Point::new(2.0, 2.0)).is_none());
        assert!(arc_through(o, o, x).is_none());
        assert!(arc_through(o, o, o).is_none());
    }

    #[test]
    fn arc_through_recovers_a_random_circle() {
        let mut g = Lcg(3);
        for _ in 0..200 {
            let c = Point::new(g.range(-20.0, 20.0), g.range(-20.0, 20.0));
            let r = g.range(0.5, 30.0);
            let a0 = g.range(-3.0, 3.0);
            let s1 = g.range(0.3, 1.5);
            let s2 = s1 + g.range(0.3, 1.5);
            let on = |a: f64| Point::new(c.x + r * a.cos(), c.y + r * a.sin());
            let (p0, p1, p2) = (on(a0), on(a0 + s1), on(a0 + s2));
            let arc = arc_through(p0, p1, p2);
            assert!(arc.is_some());
            let Some(arc) = arc else { continue };
            assert!(near_pt(arc.center, c) && near(arc.radius, r), "{arc:?}");
            assert!(near(arc.sweep, s2));
            assert!(near_pt(arc.point_at(0.0), p0));
            assert!(near_pt(arc.point_at(1.0), p2));
            assert!(near_pt(arc.point_at(s1 / s2), p1));
        }
    }

    #[test]
    fn point_segment_perpendicular_and_end_cases() {
        let (a, b) = (Point::new(0.0, 0.0), Point::new(4.0, 0.0));
        assert!(near(point_segment(Point::new(2.0, 3.0), a, b), 3.0));
        assert!(near(point_segment(Point::new(-3.0, 4.0), a, b), 5.0));
        assert!(near(point_segment(Point::new(7.0, 4.0), a, b), 5.0));
        assert!(near(point_segment(Point::new(2.0, 0.0), a, b), 0.0));
        assert!(near(point_segment(Point::new(3.0, 4.0), a, a), 5.0));
    }

    #[test]
    fn point_segment_matches_dense_sampling() {
        let mut g = Lcg(4);
        for _ in 0..100 {
            let mut pt = || Point::new(g.range(-10.0, 10.0), g.range(-10.0, 10.0));
            let (p, a, b) = (pt(), pt(), pt());
            let reference = (0..=2000)
                .map(|i| {
                    let t = i as f64 / 2000.0;
                    p.distance(Point::new(a.x + t * (b.x - a.x), a.y + t * (b.y - a.y)))
                })
                .fold(f64::INFINITY, f64::min);
            let got = point_segment(p, a, b);
            assert!(got <= reference + 1e-9, "{got} > {reference}");
            assert!(reference - got < 0.02, "{got} vs {reference}");
        }
    }

    #[test]
    fn polygon_edges_close_the_loop() {
        let sq = [
            Point::new(0.0, 0.0),
            Point::new(2.0, 0.0),
            Point::new(2.0, 2.0),
            Point::new(0.0, 2.0),
        ];
        let edges: Vec<_> = polygon_edges(&sq).collect();
        assert_eq!(edges.len(), 4);
        assert_eq!(edges[0], (sq[3], sq[0]));
        assert_eq!(edges[1], (sq[0], sq[1]));
        assert_eq!(edges[3], (sq[2], sq[3]));
        assert_eq!(polygon_edges(&[]).count(), 0);
    }
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
