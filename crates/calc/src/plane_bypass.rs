//! Suggested route for a track that crosses gaps in its reference plane:
//! `plane_bypass` keeps the track over solid copper by going around
//! rectangular gaps, or says plainly why no such route exists.
//!
//! Context: return-path work (#1858). The return path check flags a track
//! that runs over a gap in the plane below or above it; this function is
//! the fix the finding can offer. It is pure, bounded and deterministic:
//! the same inputs give the same points on every machine. No screen uses
//! it yet.
//!
//! Local rules:
//! - Gaps are axis-aligned rectangles in millimetres, and so is the board
//!   outline the route has to stay inside. Round, slanted or free-form
//!   gaps are not handled here; a caller can pass the bounding box of one,
//!   which is a safe but longer detour.
//! - A track that does not cross the inside of any gap is returned
//!   unchanged, even if it runs close to a gap edge.
//! - A track that crosses a gap is routed around it. The route is the
//!   shortest path through the corners of every gap grown by `clearance`
//!   on all sides, so the detour never touches a gap and keeps the
//!   clearance from it. Running along the grown edge is allowed.
//! - On a tie between two equally short routes the one with the smaller
//!   y (and then the smaller x) wins, so the answer repeats exactly.
//! - Both end points must be inside the board outline and at least the
//!   clearance away from every gap. A track that starts or ends over a
//!   gap cannot be fixed by moving its middle, so it is refused.
//! - The route is refused when no path exists inside the outline, or when
//!   the detour adds more than `max_extra` millimetres of length.
//!
//! # Dependencies
//! - crates/calc/src/lib.rs (`Error`, `num`, `finite`)
//!
//! # Do not
//! - Read the layer stackup or the design; the caller turns plane gaps
//!   into rectangles and applies the route.
//! - Add a screen or a fix command here; those have their own issues.

use crate::{Error, finite, num};

/// Most gaps one call accepts.
pub const MAX_GAPS: usize = 64;

/// Distances closer than this, in millimetres, count as touching.
const EPS: f64 = 1.0e-9;

/// A point in millimetres.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pt {
    /// Horizontal position, millimetres.
    pub x: f64,
    /// Vertical position, millimetres.
    pub y: f64,
}

/// An axis-aligned rectangle in millimetres.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    /// Left edge, millimetres.
    pub x_min: f64,
    /// Bottom edge, millimetres.
    pub y_min: f64,
    /// Right edge, millimetres.
    pub x_max: f64,
    /// Top edge, millimetres.
    pub y_max: f64,
}

/// Input for `plane_bypass`.
#[derive(Debug, Clone, PartialEq)]
pub struct BypassInput {
    /// Where the track starts.
    pub start: Pt,
    /// Where the track ends.
    pub end: Pt,
    /// Gaps in the reference plane, at most `MAX_GAPS`.
    pub gaps: Vec<Rect>,
    /// The board outline the route must stay inside.
    pub bounds: Rect,
    /// Margin a detour keeps from every gap edge, 0 to 10 millimetres.
    pub clearance: f64,
    /// Most extra length a detour may add, 0 to 1000 millimetres.
    pub max_extra: f64,
}

/// Output of `plane_bypass`.
#[derive(Debug, Clone, PartialEq)]
pub struct BypassOutput {
    /// The route from start to end, start and end included.
    pub points: Vec<Pt>,
    /// Length of the route in millimetres.
    pub length: f64,
    /// Length of the straight line from start to end in millimetres.
    pub direct_length: f64,
    /// How much longer the route is than the straight line.
    pub extra: f64,
    /// True when the route differs from the straight line.
    pub rerouted: bool,
}

fn dist(a: Pt, b: Pt) -> f64 {
    (a.x - b.x).hypot(a.y - b.y)
}

fn check_point(name: &str, p: Pt) -> Result<(), Error> {
    num(&format!("{name} x"), p.x, -10_000.0, 10_000.0)?;
    num(&format!("{name} y"), p.y, -10_000.0, 10_000.0)?;
    Ok(())
}

fn check_rect(name: &str, r: &Rect) -> Result<(), Error> {
    num(&format!("{name} x_min"), r.x_min, -10_000.0, 10_000.0)?;
    num(&format!("{name} y_min"), r.y_min, -10_000.0, 10_000.0)?;
    num(&format!("{name} x_max"), r.x_max, -10_000.0, 10_000.0)?;
    num(&format!("{name} y_max"), r.y_max, -10_000.0, 10_000.0)?;
    if r.x_max - r.x_min <= EPS || r.y_max - r.y_min <= EPS {
        return Err(Error::Invalid(format!(
            "{name} must have a positive width and height"
        )));
    }
    Ok(())
}

fn grow(r: &Rect, by: f64) -> Rect {
    Rect {
        x_min: r.x_min - by,
        y_min: r.y_min - by,
        x_max: r.x_max + by,
        y_max: r.y_max + by,
    }
}

fn strictly_inside(p: Pt, r: &Rect) -> bool {
    p.x > r.x_min + EPS && p.x < r.x_max - EPS && p.y > r.y_min + EPS && p.y < r.y_max - EPS
}

fn inside_or_on(p: Pt, r: &Rect) -> bool {
    p.x >= r.x_min - EPS && p.x <= r.x_max + EPS && p.y >= r.y_min - EPS && p.y <= r.y_max + EPS
}

/// Clip the segment `p` to `q` against the closed rectangle and return the
/// parameter interval that lies inside it, if any.
fn clip(p: Pt, q: Pt, r: &Rect) -> Option<(f64, f64)> {
    let (dx, dy) = (q.x - p.x, q.y - p.y);
    let (mut t0, mut t1) = (0.0_f64, 1.0_f64);
    let sides = [
        (-dx, p.x - r.x_min),
        (dx, r.x_max - p.x),
        (-dy, p.y - r.y_min),
        (dy, r.y_max - p.y),
    ];
    for (pk, qk) in sides {
        if pk == 0.0 {
            if qk < 0.0 {
                return None;
            }
        } else {
            let t = qk / pk;
            if pk < 0.0 {
                if t > t1 {
                    return None;
                }
                t0 = t0.max(t);
            } else {
                if t < t0 {
                    return None;
                }
                t1 = t1.min(t);
            }
        }
    }
    Some((t0, t1))
}

/// True when the segment passes through the inside of the rectangle.
/// Touching a corner or running along an edge is not passing through.
fn hits_interior(p: Pt, q: Pt, r: &Rect) -> bool {
    let Some((t0, t1)) = clip(p, q, r) else {
        return false;
    };
    if t1 - t0 <= 1.0e-12 {
        return false;
    }
    let t = (t0 + t1) / 2.0;
    let mid = Pt {
        x: p.x + (q.x - p.x) * t,
        y: p.y + (q.y - p.y) * t,
    };
    strictly_inside(mid, r)
}

fn hits_any(p: Pt, q: Pt, rects: &[Rect]) -> bool {
    rects.iter().any(|r| hits_interior(p, q, r))
}

/// Propose a route over solid reference copper for a track.
///
/// Returns the straight track when no gap crosses it. Otherwise returns
/// the shortest route around the gaps that stays inside the outline, or
/// an error that says why there is none.
pub fn plane_bypass(input: &BypassInput) -> Result<BypassOutput, Error> {
    check_point("start", input.start)?;
    check_point("end", input.end)?;
    check_rect("bounds", &input.bounds)?;
    let clearance = num("clearance", input.clearance, 0.0, 10.0)?;
    let max_extra = num("max_extra", input.max_extra, 0.0, 1000.0)?;
    if input.gaps.len() > MAX_GAPS {
        return Err(Error::Invalid(format!(
            "at most {MAX_GAPS} gaps are accepted, got {}",
            input.gaps.len()
        )));
    }
    for (i, gap) in input.gaps.iter().enumerate() {
        check_rect(&format!("gap {}", i + 1), gap)?;
    }
    for (name, p) in [("start", input.start), ("end", input.end)] {
        if !inside_or_on(p, &input.bounds) {
            return Err(Error::Invalid(format!(
                "the {name} point is outside the board outline"
            )));
        }
        if input.gaps.iter().any(|g| strictly_inside(p, g)) {
            return Err(Error::Invalid(format!(
                "the {name} point is over a plane gap, so moving the middle of the track cannot fix it"
            )));
        }
    }
    let direct_length = finite("direct_length", dist(input.start, input.end))?;
    let unchanged = BypassOutput {
        points: vec![input.start, input.end],
        length: direct_length,
        direct_length,
        extra: 0.0,
        rerouted: false,
    };
    if direct_length <= EPS || !hits_any(input.start, input.end, &input.gaps) {
        return Ok(unchanged);
    }

    let grown: Vec<Rect> = input.gaps.iter().map(|g| grow(g, clearance)).collect();
    for (name, p) in [("start", input.start), ("end", input.end)] {
        if grown.iter().any(|g| strictly_inside(p, g)) {
            return Err(Error::Invalid(format!(
                "the {name} point is closer to a plane gap than the {clearance} mm clearance"
            )));
        }
    }

    // Nodes: start, end, then the four corners of every grown gap that sit
    // inside the outline and are not buried in another grown gap.
    let mut nodes: Vec<Pt> = vec![input.start, input.end];
    for g in &grown {
        let corners = [
            Pt {
                x: g.x_min,
                y: g.y_min,
            },
            Pt {
                x: g.x_max,
                y: g.y_min,
            },
            Pt {
                x: g.x_max,
                y: g.y_max,
            },
            Pt {
                x: g.x_min,
                y: g.y_max,
            },
        ];
        for c in corners {
            if inside_or_on(c, &input.bounds) && !grown.iter().any(|o| strictly_inside(c, o)) {
                nodes.push(c);
            }
        }
    }

    // Shortest path over the nodes. Ties keep the earlier node, and the
    // nodes were added gap by gap, bottom corners first, so the answer is
    // the same on every run.
    let n = nodes.len();
    let mut best = vec![f64::INFINITY; n];
    let mut from: Vec<Option<usize>> = vec![None; n];
    let mut done = vec![false; n];
    best[0] = 0.0;
    for _ in 0..n {
        let mut pick: Option<usize> = None;
        for i in 0..n {
            if !done[i] && best[i].is_finite() && pick.is_none_or(|p| best[i] < best[p] - 1.0e-12) {
                pick = Some(i);
            }
        }
        let Some(u) = pick else { break };
        done[u] = true;
        if u == 1 {
            break;
        }
        for v in 0..n {
            if done[v] || hits_any(nodes[u], nodes[v], &grown) {
                continue;
            }
            let candidate = best[u] + dist(nodes[u], nodes[v]);
            if candidate < best[v] - 1.0e-12 {
                best[v] = candidate;
                from[v] = Some(u);
            }
        }
    }
    if !best[1].is_finite() {
        return Err(Error::Invalid(
            "no route over solid copper exists inside the board outline".to_string(),
        ));
    }
    let mut order = vec![1usize];
    let mut at = 1usize;
    while let Some(prev) = from[at] {
        order.push(prev);
        at = prev;
    }
    order.reverse();
    let points: Vec<Pt> = order.iter().map(|&i| nodes[i]).collect();
    let length = finite("length", best[1])?;
    let extra = (length - direct_length).max(0.0);
    if extra > max_extra {
        return Err(Error::Invalid(format!(
            "the detour adds {extra:.3} mm, more than the limit of {max_extra:.3} mm"
        )));
    }
    Ok(BypassOutput {
        points,
        length,
        direct_length,
        extra,
        rerouted: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    type T = Result<(), String>;

    fn rect(x_min: f64, y_min: f64, x_max: f64, y_max: f64) -> Rect {
        Rect {
            x_min,
            y_min,
            x_max,
            y_max,
        }
    }

    fn pt(x: f64, y: f64) -> Pt {
        Pt { x, y }
    }

    fn board() -> Rect {
        rect(-20.0, -20.0, 30.0, 20.0)
    }

    fn input(gaps: Vec<Rect>) -> BypassInput {
        BypassInput {
            start: pt(0.0, 0.0),
            end: pt(10.0, 0.0),
            gaps,
            bounds: board(),
            clearance: 0.5,
            max_extra: 5.0,
        }
    }

    /// Sample a segment finely and report whether any sample sits strictly
    /// inside the rectangle. This is a separate check from `hits_interior`.
    fn samples_inside(p: Pt, q: Pt, r: &Rect) -> bool {
        (0..=2000).any(|k| {
            let t = f64::from(k) / 2000.0;
            let s = pt(p.x + (q.x - p.x) * t, p.y + (q.y - p.y) * t);
            s.x > r.x_min + 1.0e-6
                && s.x < r.x_max - 1.0e-6
                && s.y > r.y_min + 1.0e-6
                && s.y < r.y_max - 1.0e-6
        })
    }

    #[test]
    fn a_newcomer_reads_a_suggested_route_over_a_plane_gap() -> T {
        // A gap 2 mm wide and 2 mm tall sits in the middle of a 10 mm run
        // along y = 0. Grown by 0.5 mm it spans x 3.5 to 6.5 and y -1.5 to
        // 1.5. The shortest way round goes to a lower corner (3.5, -1.5),
        // along the grown edge to (6.5, -1.5), then to the end. The two
        // slanted legs are each sqrt(3.5^2 + 1.5^2) = sqrt(14.5) long and
        // the flat part is 3 mm, so the route is 2 * sqrt(14.5) + 3.
        let out =
            plane_bypass(&input(vec![rect(4.0, -1.0, 6.0, 1.0)])).map_err(|e| e.to_string())?;
        let leg = 14.5_f64.sqrt();
        assert!(out.rerouted);
        assert_eq!(
            out.points,
            vec![pt(0.0, 0.0), pt(3.5, -1.5), pt(6.5, -1.5), pt(10.0, 0.0)]
        );
        assert!(
            (out.length - (2.0 * leg + 3.0)).abs() < 1.0e-9,
            "length {}",
            out.length
        );
        assert!((out.direct_length - 10.0).abs() < 1.0e-12);
        assert!((out.extra - (2.0 * leg + 3.0 - 10.0)).abs() < 1.0e-9);
        Ok(())
    }

    #[test]
    fn a_track_that_never_crosses_a_gap_comes_back_unchanged() -> T {
        let away =
            plane_bypass(&input(vec![rect(4.0, 2.0, 6.0, 4.0)])).map_err(|e| e.to_string())?;
        assert!(!away.rerouted);
        assert_eq!(away.points, vec![pt(0.0, 0.0), pt(10.0, 0.0)]);
        assert_eq!(away.extra, 0.0);
        // A gap whose edge touches the track from above is not a crossing.
        let touching =
            plane_bypass(&input(vec![rect(4.0, 0.0, 6.0, 2.0)])).map_err(|e| e.to_string())?;
        assert!(!touching.rerouted);
        // No gaps at all.
        let none = plane_bypass(&input(vec![])).map_err(|e| e.to_string())?;
        assert!(!none.rerouted);
        assert!((none.length - 10.0).abs() < 1.0e-12);
        Ok(())
    }

    #[test]
    fn a_detour_longer_than_the_limit_is_refused_with_the_reason() -> T {
        let mut tall = input(vec![rect(4.0, -9.0, 6.0, 9.0)]);
        tall.max_extra = 2.0;
        match plane_bypass(&tall) {
            Err(Error::Invalid(m)) => {
                assert!(m.contains("limit of 2.000 mm"), "{m}");
                assert!(m.contains("adds"), "{m}");
            }
            other => return Err(format!("expected a refusal, got {other:?}")),
        }
        // The same gap with a generous limit is routed.
        let mut roomy = tall.clone();
        roomy.max_extra = 100.0;
        let out = plane_bypass(&roomy).map_err(|e| e.to_string())?;
        assert!(out.rerouted);
        Ok(())
    }

    #[test]
    fn a_gap_that_fills_the_board_width_has_no_route() -> T {
        // The gap spans the whole board height, so every corner of the
        // grown gap is outside the outline and there is no way round.
        let mut wall = input(vec![rect(4.0, -25.0, 6.0, 25.0)]);
        wall.max_extra = 1000.0;
        match plane_bypass(&wall) {
            Err(Error::Invalid(m)) => assert!(m.contains("no route"), "{m}"),
            other => return Err(format!("expected no route, got {other:?}")),
        }
        Ok(())
    }

    #[test]
    fn an_end_point_over_a_gap_or_outside_the_board_is_refused() -> T {
        let mut over = input(vec![rect(-1.0, -1.0, 1.0, 1.0)]);
        over.start = pt(0.0, 0.0);
        match plane_bypass(&over) {
            Err(Error::Invalid(m)) => assert!(m.contains("start point is over a plane gap"), "{m}"),
            other => return Err(format!("expected a refusal, got {other:?}")),
        }
        let mut out_of_board = input(vec![]);
        out_of_board.end = pt(50.0, 0.0);
        match plane_bypass(&out_of_board) {
            Err(Error::Invalid(m)) => {
                assert!(m.contains("end point is outside the board outline"), "{m}")
            }
            other => return Err(format!("expected a refusal, got {other:?}")),
        }
        // A track that must detour but starts closer than the clearance.
        let mut close = input(vec![rect(0.3, -1.0, 6.0, 1.0)]);
        close.start = pt(0.0, 0.0);
        match plane_bypass(&close) {
            Err(Error::Invalid(m)) => assert!(m.contains("closer to a plane gap"), "{m}"),
            other => return Err(format!("expected a refusal, got {other:?}")),
        }
        Ok(())
    }

    #[test]
    fn bad_numbers_are_refused_with_the_range() -> T {
        let mut bad = input(vec![]);
        bad.clearance = 11.0;
        assert!(matches!(plane_bypass(&bad), Err(Error::OutOfRange { .. })));
        let mut flat = input(vec![rect(4.0, 1.0, 6.0, 1.0)]);
        flat.clearance = 0.0;
        match plane_bypass(&flat) {
            Err(Error::Invalid(m)) => assert!(m.contains("positive width and height"), "{m}"),
            other => return Err(format!("expected a refusal, got {other:?}")),
        }
        let many = input(
            (0..=MAX_GAPS)
                .map(|i| rect(f64::from(i as u32), 5.0, f64::from(i as u32) + 0.5, 6.0))
                .collect(),
        );
        assert!(matches!(plane_bypass(&many), Err(Error::Invalid(_))));
        Ok(())
    }

    #[test]
    fn the_same_inputs_give_the_same_route_and_a_tie_goes_low() -> T {
        let gaps = vec![rect(4.0, -1.0, 6.0, 1.0)];
        let a = plane_bypass(&input(gaps.clone())).map_err(|e| e.to_string())?;
        let b = plane_bypass(&input(gaps)).map_err(|e| e.to_string())?;
        assert_eq!(a, b);
        // The gap is centred on the track, so the route over the top is
        // exactly as long as the route under it. The lower one wins.
        assert!(a.points.iter().skip(1).take(2).all(|p| p.y < 0.0));
        Ok(())
    }

    #[test]
    fn two_gaps_in_a_row_are_both_avoided() -> T {
        let gaps = vec![rect(2.0, -1.0, 3.0, 1.0), rect(6.0, -0.5, 7.0, 2.0)];
        let mut two = input(gaps.clone());
        two.max_extra = 20.0;
        let out = plane_bypass(&two).map_err(|e| e.to_string())?;
        assert!(out.rerouted);
        assert!(out.extra > 0.0);
        for pair in out.points.windows(2) {
            for g in &gaps {
                assert!(
                    !samples_inside(pair[0], pair[1], g),
                    "{pair:?} crosses {g:?}"
                );
            }
        }
        Ok(())
    }

    #[test]
    fn a_route_never_crosses_a_gap_or_leaves_the_outline() -> T {
        // A fixed grid of gap positions and sizes, so the run is repeatable.
        let mut routed = 0_u32;
        for ix in 0..6 {
            for iy in 0..6 {
                for w in [0.5_f64, 1.5, 3.0] {
                    for h in [0.5_f64, 2.0, 6.0] {
                        let x0 = 1.0 + f64::from(ix) * 1.3;
                        let y0 = -3.0 + f64::from(iy) * 1.1;
                        let gap = rect(x0, y0, x0 + w, y0 + h);
                        let mut case = input(vec![gap]);
                        case.max_extra = 100.0;
                        // Skip cases the function refuses on its own terms.
                        let Ok(out) = plane_bypass(&case) else {
                            continue;
                        };
                        for p in &out.points {
                            assert!(inside_or_on(*p, &case.bounds), "{p:?} left the outline");
                        }
                        for pair in out.points.windows(2) {
                            assert!(
                                !samples_inside(pair[0], pair[1], &gap),
                                "{pair:?} crosses {gap:?}"
                            );
                        }
                        assert!(out.length + 1.0e-9 >= out.direct_length);
                        assert!((out.length - out.direct_length - out.extra).abs() < 1.0e-9);
                        if out.rerouted {
                            routed += 1;
                        }
                    }
                }
            }
        }
        assert!(
            routed > 20,
            "the grid should force many detours, got {routed}"
        );
        Ok(())
    }
}
