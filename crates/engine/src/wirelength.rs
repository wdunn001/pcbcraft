//! Wirelength metrics for placement: half-perimeter and ratsnest length.
//!
//! Context: `layout.report` shows these numbers and later placers use the same
//! functions as their cost. They read pad positions only, never copper, so the
//! number moves when a part moves and not when a track is drawn.
//!
//! Local rules:
//! - Half-perimeter wirelength (HPWL) of a net is the width plus the height of
//!   the box around its pads. A net with fewer than two pads is 0.
//! - Ratsnest length is the length of the minimum spanning tree over the pads
//!   (Euclidean, Prim's method, O(n^2)); nets above `MAX_PADS` pads are
//!   refused rather than approximated.
//! - Results are plain millimetres; rounding is the caller's job.
//!
//! Dependencies: `crates/doc/src/lib.rs` (`Project`), `crates/geom/src/lib.rs`.
use pcbcraft_doc::{Id, Project};
use pcbcraft_geom::Point;

/// Most pads one net may have for the spanning tree.
pub const MAX_PADS: usize = 4096;

/// World positions of every placed pad on `net`, in footprint then pad order.
pub fn net_pad_points(project: &Project, net: Id) -> Vec<Point> {
    project
        .board
        .footprints
        .iter()
        .flat_map(|f| {
            f.pads
                .iter()
                .filter(move |p| p.net == Some(net))
                .map(move |p| f.pad_world_position(p))
        })
        .collect()
}

/// Half the perimeter of the box around `points`; 0 for fewer than two.
pub fn hpwl(points: &[Point]) -> f64 {
    if points.len() < 2 {
        return 0.0;
    }
    let (mut lo, mut hi) = (points[0], points[0]);
    for p in points {
        lo = Point::new(lo.x.min(p.x), lo.y.min(p.y));
        hi = Point::new(hi.x.max(p.x), hi.y.max(p.y));
    }
    (hi.x - lo.x) + (hi.y - lo.y)
}

/// Length of the minimum spanning tree over `points`; 0 for fewer than two.
/// Returns `None` above `MAX_PADS` points.
pub fn ratsnest_length(points: &[Point]) -> Option<f64> {
    let n = points.len();
    if n > MAX_PADS {
        return None;
    }
    if n < 2 {
        return Some(0.0);
    }
    let mut best = vec![f64::INFINITY; n];
    let mut done = vec![false; n];
    best[0] = 0.0;
    let mut total = 0.0;
    for _ in 0..n {
        let mut pick = None;
        for i in 0..n {
            if !done[i] && pick.is_none_or(|j: usize| best[i] < best[j]) {
                pick = Some(i);
            }
        }
        let Some(i) = pick else { break };
        done[i] = true;
        total += best[i];
        for j in 0..n {
            if !done[j] {
                best[j] = best[j].min(points[i].distance(points[j]));
            }
        }
    }
    Some(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pts(v: &[(f64, f64)]) -> Vec<Point> {
        v.iter().map(|(x, y)| Point::new(*x, *y)).collect()
    }

    #[test]
    fn two_pin_net_is_the_manhattan_box_and_the_straight_line() {
        let p = pts(&[(0.0, 0.0), (3.0, 4.0)]);
        assert!((hpwl(&p) - 7.0).abs() < 1e-9);
        assert!((ratsnest_length(&p).unwrap_or(-1.0) - 5.0).abs() < 1e-9);
    }

    #[test]
    fn three_pin_net_spans_with_the_two_shortest_links() {
        let p = pts(&[(0.0, 0.0), (10.0, 0.0), (10.0, 5.0)]);
        assert!((hpwl(&p) - 15.0).abs() < 1e-9);
        assert!((ratsnest_length(&p).unwrap_or(-1.0) - 15.0).abs() < 1e-9);
    }

    #[test]
    fn short_nets_are_zero_and_huge_nets_are_refused() {
        assert_eq!(hpwl(&pts(&[(1.0, 1.0)])), 0.0);
        assert_eq!(ratsnest_length(&[]), Some(0.0));
        let many = vec![Point::new(0.0, 0.0); MAX_PADS + 1];
        assert_eq!(ratsnest_length(&many), None);
    }

    /// Property: for any point set the tree is never longer than the star
    /// from the first point, never shorter than the box diagonal's largest
    /// side, and does not depend on the order of the points.
    #[test]
    fn spanning_tree_bounds_and_order_independence() {
        let mut seed: u64 = 0x9E3779B97F4A7C15;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed % 20_000) as f64 / 100.0
        };
        for round in 0..200 {
            let n = 2 + round % 9;
            let p: Vec<Point> = (0..n).map(|_| Point::new(next(), next())).collect();
            let tree = ratsnest_length(&p).unwrap_or(-1.0);
            let star: f64 = p.iter().map(|q| p[0].distance(*q)).sum();
            assert!(tree <= star + 1e-9, "tree {tree} star {star}");
            let mut rev = p.clone();
            rev.reverse();
            let other = ratsnest_length(&rev).unwrap_or(-2.0);
            assert!((tree - other).abs() < 1e-9);
            let (w, h) = (
                p.iter().map(|q| q.x).fold(f64::MIN, f64::max)
                    - p.iter().map(|q| q.x).fold(f64::MAX, f64::min),
                p.iter().map(|q| q.y).fold(f64::MIN, f64::max)
                    - p.iter().map(|q| q.y).fold(f64::MAX, f64::min),
            );
            assert!(tree + 1e-9 >= w.max(h) * 0.999 - 1e-9 || n < 2);
            assert!(hpwl(&p) >= tree * 0.0 && (hpwl(&p) - (w + h)).abs() < 1e-9);
        }
    }
}
