//! Arcs on the board outline: validation, bounds and flattening.
//!
//! The outline stays a list of vertices in `Board.outline`. `Board.outline_arcs`
//! holds one optional arc midpoint per segment, the same midpoint form tracks
//! use. Everything here is pure geometry on those two lists; no project state.
//! Rules: an empty arc list means every segment is straight; arcs sweep under
//! 180 degrees; the flattened ring must be simple.
//!
//! # Dependencies
//! - crates/geom/src/lib.rs#arc_through
//! - crates/geom/src/lib.rs#polygon_is_simple

use pcbcraft_geom::{Arc, Point, arc_through, polygon_is_simple};
use std::f64::consts::{FRAC_PI_2, PI};

/// Shallowest bulge an outline arc may have, in millimetres. Below this the
/// arc is a straight segment in all but name.
pub const MIN_ARC_SAGITTA: f64 = 0.001;
/// Chord error used when the flattened ring is only checked, in millimetres.
pub const VALIDATION_CHORD_ERROR: f64 = 0.02;
/// Most chords one arc is cut into.
const MAX_CHORDS: usize = 64;

/// The arc of segment `i` (vertex `i` to vertex `i + 1`, the last closing to
/// vertex 0), or none when the segment is straight or has no midpoint.
///
/// # Rules
/// - `arcs` may be shorter than `points`; missing entries are straight.
pub fn segment_arc(points: &[Point], arcs: &[Option<Point>], i: usize) -> Option<Arc> {
    let mid = (*arcs.get(i)?)?;
    let a = *points.get(i)?;
    let b = *points.get((i + 1) % points.len())?;
    arc_through(a, mid, b)
}

/// Height of an arc's bulge above its chord, in millimetres.
pub fn arc_sagitta(arc: &Arc) -> f64 {
    arc.radius * (1.0 - (arc.sweep.abs() / 2.0).cos())
}

/// Chords needed so no chord strays more than `max_error` from `arc`.
fn chord_count(arc: &Arc, max_error: f64) -> usize {
    let e = max_error.max(1e-6);
    if e >= arc.radius {
        return 1;
    }
    let step = 2.0 * (1.0 - e / arc.radius).acos();
    if !step.is_finite() || step <= 0.0 {
        return MAX_CHORDS;
    }
    ((arc.sweep.abs() / step).ceil() as usize).clamp(1, MAX_CHORDS)
}

/// The outline as a plain polygon: every vertex, plus points along each arc so
/// no chord strays more than `max_error` millimetres from the true edge.
///
/// # Rules
/// - The first vertex is not repeated at the end.
/// - Arc endpoints are the exact vertices.
///
/// # Do not
/// - Do not read the arcs as chords; use this for any polygon-only reader.
pub fn flatten_outline(points: &[Point], arcs: &[Option<Point>], max_error: f64) -> Vec<Point> {
    let mut ring = Vec::with_capacity(points.len());
    for (i, &p) in points.iter().enumerate() {
        ring.push(p);
        if let Some(arc) = segment_arc(points, arcs, i) {
            let n = chord_count(&arc, max_error);
            for k in 1..n {
                ring.push(arc.point_at(k as f64 / n as f64));
            }
        }
    }
    ring
}

/// Smallest and largest coordinates of the outline, counting arc bulges that
/// reach past the vertices.
pub fn outline_extent(points: &[Point], arcs: &[Option<Point>]) -> Option<(Point, Point)> {
    let first = *points.first()?;
    let (mut lo, mut hi) = (first, first);
    let mut take = |p: Point| {
        lo = Point::new(lo.x.min(p.x), lo.y.min(p.y));
        hi = Point::new(hi.x.max(p.x), hi.y.max(p.y));
    };
    for &p in points {
        take(p);
    }
    for i in 0..points.len() {
        let Some(arc) = segment_arc(points, arcs, i) else {
            continue;
        };
        for quarter in 0..4 {
            let angle = quarter as f64 * FRAC_PI_2;
            let t = angle_fraction(&arc, angle);
            if (0.0..=1.0).contains(&t) {
                take(arc.point_at(t));
            }
        }
    }
    Some((lo, hi))
}

/// Fraction of the sweep at which the arc passes `angle`, outside 0..=1 when
/// it never does.
fn angle_fraction(arc: &Arc, angle: f64) -> f64 {
    let turn = 2.0 * PI;
    let mut d = (angle - arc.start_angle) % turn;
    if arc.sweep >= 0.0 {
        if d < 0.0 {
            d += turn;
        }
    } else if d > 0.0 {
        d -= turn;
    }
    d / arc.sweep
}

/// Check the arc list against the vertices.
///
/// # Rules
/// - An empty list is fine; otherwise one entry per vertex.
/// - Each midpoint is finite and gives a real arc: bulge of at least
///   `MIN_ARC_SAGITTA`, sweep under 180 degrees.
/// - The flattened ring does not cross or touch itself.
pub fn validate_outline_arcs(points: &[Point], arcs: &[Option<Point>]) -> Result<(), String> {
    if arcs.is_empty() {
        return Ok(());
    }
    if arcs.len() != points.len() {
        return Err("an outline needs one arc entry per segment".into());
    }
    for (i, mid) in arcs.iter().enumerate() {
        let Some(mid) = mid else { continue };
        if !mid.valid() {
            return Err(format!("outline arc {i} midpoint is out of range"));
        }
        let Some(arc) = segment_arc(points, arcs, i) else {
            return Err(format!("outline arc {i} is not a real arc"));
        };
        if arc_sagitta(&arc) < MIN_ARC_SAGITTA {
            return Err(format!("outline arc {i} is too shallow; keep it straight"));
        }
        if arc.sweep.abs() >= PI - 1e-6 {
            return Err(format!(
                "outline arc {i} turns 180 degrees or more; split it in two"
            ));
        }
    }
    if !polygon_is_simple(&flatten_outline(points, arcs, VALIDATION_CHORD_ERROR)) {
        return Err("the outline crosses itself".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square() -> Vec<Point> {
        vec![
            Point::new(0.0, 0.0),
            Point::new(20.0, 0.0),
            Point::new(20.0, 20.0),
            Point::new(0.0, 20.0),
        ]
    }

    /// Arc midpoint of a quarter circle round the corner at (20, 0).
    fn bulged() -> Vec<Option<Point>> {
        vec![Some(Point::new(10.0, -2.0)), None, None, None]
    }

    #[test]
    fn edge_ring_chords_stay_within_the_error() -> Result<(), String> {
        let pts = square();
        let arcs = bulged();
        let arc = segment_arc(&pts, &arcs, 0).ok_or("no arc")?;
        let ring = flatten_outline(&pts, &arcs, 0.01);
        assert!(ring.len() > pts.len());
        for pair in ring.windows(2).take(ring.len() - pts.len()) {
            let mid = Point::new((pair[0].x + pair[1].x) / 2.0, (pair[0].y + pair[1].y) / 2.0);
            let off = (mid.distance(arc.center) - arc.radius).abs();
            assert!(off <= 0.0101, "chord strays {off} from the arc");
        }
        Ok(())
    }

    #[test]
    fn outline_bounds_include_arc_bulges() {
        let (lo, hi) = outline_extent(&square(), &bulged()).unwrap_or_default();
        assert!(
            lo.y < -1.99,
            "the bulge reaches below the vertices: {}",
            lo.y
        );
        assert!((hi.x - 20.0).abs() < 1e-9);
    }

    #[test]
    fn self_intersecting_arc_outline_is_refused() {
        let mut arcs = bulged();
        arcs[0] = Some(Point::new(10.0, 25.0));
        assert!(validate_outline_arcs(&square(), &arcs).is_err());
        assert!(validate_outline_arcs(&square(), &bulged()).is_ok());
    }

    #[test]
    fn shallow_straight_and_half_turn_arcs_are_refused() {
        let flat = vec![Some(Point::new(10.0, 0.0)), None, None, None];
        assert!(validate_outline_arcs(&square(), &flat).is_err());
        let hair = vec![Some(Point::new(10.0, -0.0001)), None, None, None];
        assert!(validate_outline_arcs(&square(), &hair).is_err());
        let half = vec![Some(Point::new(10.0, -10.0)), None, None, None];
        assert!(validate_outline_arcs(&square(), &half).is_err());
        assert!(validate_outline_arcs(&square(), &[None]).is_err());
    }
}
