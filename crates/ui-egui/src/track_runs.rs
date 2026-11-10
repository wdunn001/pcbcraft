//! Join track segments that meet end to end into one path each, for drawing.
//!
//! A routed run is stored as one track record per straight piece, so a bend is
//! two records that share a point. Stroked on its own, a segment has flat ends,
//! and two flat ends meeting at an angle leave a notch on the outside of the
//! corner. Copper that is continuous is then drawn with a bite out of it at
//! every turn.
//!
//! Handing the whole run to the tessellator as one path fixes it: the join at
//! an interior vertex is mitered, so the corner is solid. Drawing a round cap
//! on each segment instead would work on an opaque layer and show a dark dot
//! at every joint on a dimmed one, because the cap and the segment would both
//! be blended over the same pixels.
//!
//! # Dependencies
//! - crates/ui-egui/src/canvas.rs (the only caller, when painting copper)
//!
//! # Do not
//! - Join ends that merely sit close. Only an exact match counts.
use pcbcraft_geom::Point;
use std::collections::HashMap;

/// A point as an exact hash key, which is what joining ends asks for.
///
/// Negative zero and zero are the same place and must land on the same key.
type Key = (u64, u64);

fn key(p: Point) -> Key {
    let flat = |v: f64| if v == 0.0 { 0.0 } else { v };
    (flat(p.x).to_bits(), flat(p.y).to_bits())
}

/// Chain `segs` into as few paths as possible, each a list of points.
///
/// # Rules
/// - Two segments join only where their ends are exactly equal.
/// - A point where three or more ends meet closes a run. A T is not a bend,
///   and mitering through one would draw copper that is not there.
/// - Every segment lands in exactly one path, so nothing is drawn twice and
///   a translucent layer blends once.
pub(crate) fn assemble(segs: &[(Point, Point)]) -> Vec<Vec<Point>> {
    let mut ends: HashMap<Key, Vec<usize>> = HashMap::new();
    for (i, s) in segs.iter().enumerate() {
        ends.entry(key(s.0)).or_default().push(i);
        ends.entry(key(s.1)).or_default().push(i);
    }
    let mut used = vec![false; segs.len()];
    let mut runs: Vec<Vec<Point>> = Vec::new();

    // A run starts at a free end or a junction. What is left over after that
    // is a closed loop, which can start anywhere.
    let heads = (0..segs.len())
        .flat_map(|i| [(i, segs[i].0), (i, segs[i].1)])
        .filter(|(_, p)| ends.get(&key(*p)).map_or(0, Vec::len) != 2)
        .collect::<Vec<_>>();
    for (i, from) in heads
        .into_iter()
        .chain((0..segs.len()).map(|i| (i, segs[i].0)))
    {
        if used[i] {
            continue;
        }
        runs.push(walk(segs, &ends, &mut used, i, from));
    }
    runs
}

/// Follow the chain from segment `first`, entering it at `from`.
fn walk(
    segs: &[(Point, Point)],
    ends: &HashMap<Key, Vec<usize>>,
    used: &mut [bool],
    first: usize,
    from: Point,
) -> Vec<Point> {
    let mut path = vec![from];
    let (mut here, mut at) = (first, from);
    loop {
        used[here] = true;
        let (a, b) = segs[here];
        let far = if key(a) == key(at) { b } else { a };
        path.push(far);
        let Some(touching) = ends.get(&key(far)) else {
            break;
        };
        // Only a plain bend carries on. Anything else ends the run.
        if touching.len() != 2 {
            break;
        }
        match touching.iter().copied().find(|&j| j != here) {
            Some(next) if !used[next] => {
                here = next;
                at = far;
            }
            _ => break,
        }
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pt(x: f64, y: f64) -> Point {
        Point::new(x, y)
    }

    #[test]
    fn a_bend_becomes_one_path_so_the_corner_can_be_mitered() {
        // The shape that showed the notch: two records sharing a point.
        let runs = assemble(&[
            (pt(0.0, 0.0), pt(10.0, 0.0)),
            (pt(10.0, 0.0), pt(10.0, 10.0)),
        ]);
        assert_eq!(runs.len(), 1, "one bend is one path: {runs:?}");
        assert_eq!(
            runs[0],
            vec![pt(0.0, 0.0), pt(10.0, 0.0), pt(10.0, 10.0)],
            "the shared point appears once, in the middle"
        );
    }

    #[test]
    fn a_run_assembles_whatever_order_the_records_arrive_in() {
        let runs = assemble(&[
            (pt(10.0, 10.0), pt(20.0, 10.0)),
            (pt(0.0, 0.0), pt(10.0, 0.0)),
            (pt(10.0, 0.0), pt(10.0, 10.0)),
        ]);
        assert_eq!(
            runs.len(),
            1,
            "order of records does not split it: {runs:?}"
        );
        assert_eq!(runs[0].len(), 4, "four points for three pieces");
    }

    #[test]
    fn a_near_miss_is_not_joined() {
        // 40 micrometres apart. Joining this would draw copper that the
        // checks never agreed was connected.
        let runs = assemble(&[
            (pt(0.0, 0.0), pt(10.0, 0.0)),
            (pt(10.00004, 0.0), pt(10.00004, 10.0)),
        ]);
        assert_eq!(runs.len(), 2, "only an exact end joins: {runs:?}");
    }

    #[test]
    fn a_junction_ends_the_run_rather_than_mitering_through_it() {
        // Three ends at one point. Carrying on through would miter a corner
        // across the branch and paint copper that is not there.
        let runs = assemble(&[
            (pt(0.0, 0.0), pt(10.0, 0.0)),
            (pt(10.0, 0.0), pt(20.0, 0.0)),
            (pt(10.0, 0.0), pt(10.0, 10.0)),
        ]);
        assert_eq!(runs.len(), 3, "a T is three runs: {runs:?}");
        assert!(
            runs.iter().all(|r| r.len() == 2),
            "none of them runs through the junction: {runs:?}"
        );
    }

    #[test]
    fn every_segment_is_drawn_exactly_once() {
        // A loop has no free end, so it has to be picked up by the sweep that
        // follows. Drawing a segment twice would double-blend a dimmed layer.
        let segs = [
            (pt(0.0, 0.0), pt(10.0, 0.0)),
            (pt(10.0, 0.0), pt(10.0, 10.0)),
            (pt(10.0, 10.0), pt(0.0, 10.0)),
            (pt(0.0, 10.0), pt(0.0, 0.0)),
        ];
        let runs = assemble(&segs);
        let drawn: usize = runs.iter().map(|r| r.len() - 1).sum();
        assert_eq!(drawn, segs.len(), "each piece once: {runs:?}");
    }

    #[test]
    fn separate_runs_stay_separate() {
        let runs = assemble(&[
            (pt(0.0, 0.0), pt(10.0, 0.0)),
            (pt(50.0, 50.0), pt(60.0, 50.0)),
        ]);
        assert_eq!(runs.len(), 2);
    }
}
