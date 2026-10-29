//! Shove: push other nets' tracks aside by re-walking them around the pusher.
//!
//! The new route has the top rank. A pushed line gets its pusher's rank minus
//! one, so a line only ever pushes lines of lower rank and nothing pushes back.
//! Pad-anchored and free ends never move. Only iteration counts bound the work.
use super::walk::{Hull, Space, Turn, acute, length, optimize, path_ok, walkaround};
use super::{MARGIN, bounds, solid_hulls};
use crate::{Error, allocate};
use pcbcraft_doc::{Id, Layer, Project, Track};
use pcbcraft_geom::hull::octagon;
use pcbcraft_geom::{Point, segment_distance};
use std::collections::VecDeque;

/// Most tracks one command may push.
pub const SHOVE_ITERATIONS: usize = 250;

/// A chain of same-net segments with fixed end points, moved as one unit.
#[derive(Clone, Debug)]
pub struct Line {
    pub ids: Vec<Id>,
    pub net: Id,
    pub width: f64,
    pub pts: Vec<Point>,
    pub rank: i32,
}

fn near(a: Point, b: Point) -> bool {
    a.distance(b) < 1e-6
}

/// Chains of tracks on `layer`. A chain continues through a joint only where
/// exactly two same-width tracks of one net meet and no pad sits.
pub fn lines(p: &Project, layer: Layer) -> Vec<Line> {
    let mut tracks: Vec<&Track> = p
        .board
        .tracks
        .iter()
        .filter(|t| t.layer == layer && t.arc.is_none())
        .collect();
    tracks.sort_by_key(|t| t.id);
    let pads: Vec<Point> = p
        .board
        .footprints
        .iter()
        .flat_map(|f| f.pads.iter().filter_map(|pad| f.pad_position(&pad.number)))
        .collect();
    let joint = |at: Point, t: &Track| -> Option<&Track> {
        let here: Vec<&&Track> = tracks
            .iter()
            .filter(|u| near(u.start, at) || near(u.end, at))
            .collect();
        if here.len() != 2 || pads.iter().any(|q| near(*q, at)) {
            return None;
        }
        here.into_iter()
            .copied()
            .find(|u| u.id != t.id && u.net == t.net && (u.width - t.width).abs() < 1e-9)
    };
    let mut visited: Vec<Id> = vec![];
    let mut result = vec![];
    for t in &tracks {
        if visited.contains(&t.id) {
            continue;
        }
        visited.push(t.id);
        let mut ids = VecDeque::from([t.id]);
        let mut pts = VecDeque::from([t.start, t.end]);
        for forward in [true, false] {
            for _ in 0..tracks.len() {
                let (Some(end), Some(id)) = (
                    if forward { pts.back() } else { pts.front() }.copied(),
                    if forward { ids.back() } else { ids.front() }.copied(),
                ) else {
                    break;
                };
                let Some(current) = tracks.iter().find(|c| c.id == id) else {
                    break;
                };
                let Some(u) = joint(end, current) else { break };
                if visited.contains(&u.id) {
                    break;
                }
                visited.push(u.id);
                let far = if near(u.start, end) { u.end } else { u.start };
                if forward {
                    ids.push_back(u.id);
                    pts.push_back(far);
                } else {
                    ids.push_front(u.id);
                    pts.push_front(far);
                }
            }
        }
        result.push(Line {
            ids: ids.into(),
            net: t.net,
            width: t.width,
            pts: pts.into(),
            rank: i32::MIN,
        });
    }
    result
}

fn mid(a: Point, b: Point) -> Point {
    Point::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0)
}

/// Clearance the checks demand between two segments of different nets.
fn pair_clearance(
    p: &Project,
    layer: Layer,
    a: &Line,
    sa: &[Point],
    b: &Line,
    sb: &[Point],
) -> f64 {
    p.required_clearance_at(
        a.net,
        b.net,
        mid(sa[0], sa[1]),
        mid(sb[0], sb[1]),
        layer,
        layer,
    )
}

fn collides(p: &Project, layer: Layer, a: &Line, b: &Line) -> bool {
    a.pts.windows(2).any(|s| {
        b.pts.windows(2).any(|t| {
            segment_distance(s[0], s[1], t[0], t[1]) - a.width / 2.0 - b.width / 2.0 + 1e-6
                < pair_clearance(p, layer, a, s, b, t)
        })
    })
}

fn rewalk(p: &Project, layer: Layer, victim: &Line, pushers: &[Line]) -> Option<Vec<Point>> {
    let mut hulls = solid_hulls(p, layer, victim.net, victim.width);
    for l in pushers.iter().filter(|l| l.net != victim.net) {
        for s in l.pts.windows(2) {
            let required = p.required_clearance_at(
                victim.net,
                l.net,
                mid(victim.pts[0], victim.pts[1]),
                mid(s[0], s[1]),
                layer,
                layer,
            );
            hulls.push(Hull {
                pts: octagon(
                    &[s[0], s[1]],
                    required + victim.width / 2.0 + l.width / 2.0 + MARGIN,
                ),
                net: l.net,
            });
        }
    }
    let lo = p.board.clearance + victim.width / 2.0;
    let (min, max) = bounds(p);
    let space = Space {
        hulls,
        lo: Point::new(min.x + lo, min.y + lo),
        hi: Point::new(max.x - lo, max.y - lo),
    };
    let before = acute(&victim.pts);
    let mut best: Option<Vec<Point>> = None;
    for turn in [Turn::Shortest, Turn::Ccw, Turn::Cw] {
        let Some(walked) = walkaround(&victim.pts, &space, turn) else {
            continue;
        };
        let path = optimize(walked, &space);
        if path.len() >= 2
            && acute(&path) <= before
            && path_ok(&path, &space)
            && best
                .as_ref()
                .is_none_or(|b| length(&path) < length(b) - 1e-9)
        {
            best = Some(path);
        }
    }
    best
}

pub struct Outcome {
    /// The new route first, then every pushed line.
    pub lines: Vec<Line>,
    pub iterations: usize,
}

/// Push every other-net track the new `route` collides with, and whatever those push.
pub fn push(
    p: &Project,
    layer: Layer,
    net: Id,
    width: f64,
    route: &[Point],
) -> Result<Outcome, Error> {
    let mut processed = vec![Line {
        ids: vec![],
        net,
        width,
        pts: route.to_vec(),
        rank: 0,
    }];
    let mut pool: Vec<Line> = lines(p, layer)
        .into_iter()
        .filter(|l| l.net != net)
        .collect();
    let mut queue = VecDeque::from([0usize]);
    let mut iterations = 0;
    while let Some(pi) = queue.pop_front() {
        let mut k = 0;
        while k < pool.len() {
            let hit = match (processed.get(pi), pool.get(k)) {
                (Some(a), Some(b)) => a.net != b.net && collides(p, layer, a, b),
                _ => false,
            };
            if !hit {
                k += 1;
                continue;
            }
            iterations += 1;
            let pusher_rank = processed.get(pi).map_or(0, |l| l.rank);
            let mut v = pool.remove(k);
            let name = p.net_name(v.net);
            if iterations > SHOVE_ITERATIONS {
                return Err(Error::Invalid(format!(
                    "pushing the track of net {name} needs more than {SHOVE_ITERATIONS} shove steps; move a part or route in two steps"
                )));
            }
            if pusher_rank <= v.rank {
                return Err(Error::Invalid(format!(
                    "the track of net {name} would have to push back; route it first or move it"
                )));
            }
            let Some(pts) = rewalk(p, layer, &v, &processed) else {
                return Err(Error::Invalid(format!(
                    "cannot push the track of net {name} out of the way: it would hit a pad, the board edge or another track, or its end is too close; move it or route around it"
                )));
            };
            v.pts = pts;
            v.rank = pusher_rank - 1;
            processed.push(v);
            queue.push_back(processed.len() - 1);
        }
    }
    Ok(Outcome {
        lines: processed,
        iterations,
    })
}

/// Write a line into the project. Existing ids are reused in order, extra
/// segments get new ids and unused ids are deleted. Returns the ids now used.
pub fn apply(p: &mut Project, next: &mut Id, line: &Line, layer: Layer) -> Result<Vec<Id>, Error> {
    let segments: Vec<(Point, Point)> = line.pts.windows(2).map(|w| (w[0], w[1])).collect();
    let mut used = vec![];
    for (k, (start, end)) in segments.iter().enumerate() {
        match line.ids.get(k) {
            Some(id) => {
                if let Some(t) = p.board.tracks.iter_mut().find(|t| t.id == *id) {
                    t.start = *start;
                    t.end = *end;
                }
                used.push(*id);
            }
            None => {
                let id = allocate(next)?;
                p.board.tracks.push(Track {
                    id,
                    net: line.net,
                    start: *start,
                    end: *end,
                    width: line.width,
                    layer,
                    arc: None,,
                    preserved: pcbcraft_doc::PreservedSource::default(),
                });
                used.push(id);
            }
        }
    }
    let dropped: Vec<Id> = line.ids.iter().skip(segments.len()).copied().collect();
    p.board.tracks.retain(|t| !dropped.contains(&t.id));
    Ok(used)
}
