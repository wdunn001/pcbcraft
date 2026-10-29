//! Octilinear routing: walkaround of octagonal hulls, then a bounded shove.
//!
//! Hulls come from `pcbcraft_rules::obstacles`, the same copper list the checks
//! use, grown by clearance plus half the track width. Limits are iteration
//! counts only (never time), so identical input gives identical copper.
mod shove;
mod walk;

use super::{PadRef, pad};
use crate::Error;
use pcbcraft_doc::{Id, Layer, Project};
use pcbcraft_geom::Point;
use pcbcraft_geom::hull::octagon;
use pcbcraft_rules::{Obstacle, drc, obstacles};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use walk::{Failure, Hull, Space, find_path};

use walk::WALK_ITERATIONS;

/// Extra room added to every hull so routed copper never sits exactly on the limit.
pub(crate) const MARGIN: f64 = 1e-4;

#[derive(Deserialize, Clone, Copy, Default, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Mode {
    /// One bend, right angles, no obstacle checks (the original behaviour).
    #[default]
    Orthogonal,
    /// 45-degree walkaround of every pad, track and cutout; nothing is moved.
    Octilinear,
    /// 45-degree walkaround of pads and cutouts, then push other nets' tracks aside.
    OctilinearShove,
}

#[derive(Deserialize, Clone)]
#[serde(untagged)]
pub(crate) enum Endpoint {
    Pad(PadRef),
    Point(Point),
}

/// Resolve both ends and the net being routed. Pads supply their net; a bare
/// point needs `net` in the command.
pub(crate) fn resolve(
    p: &Project,
    from: &Endpoint,
    to: &Endpoint,
    net: Option<Id>,
    layer: Layer,
) -> Result<(Point, Point, Id), Error> {
    let mismatch =
        || Error::Invalid("route requires distinct pads on the same net and a valid width".into());
    let mut nets = vec![];
    let mut points = vec![];
    for e in [from, to] {
        match e {
            Endpoint::Pad(r) => {
                let (point, n) = pad(p, r, layer)?;
                points.push(point);
                nets.push(n);
            }
            Endpoint::Point(point) => {
                if !point.valid()
                    || point.x < 0.0
                    || point.y < 0.0
                    || point.x > p.board.width
                    || point.y > p.board.height
                {
                    return Err(Error::Invalid("route point must lie on the board".into()));
                }
                points.push(*point);
            }
        }
    }
    nets.extend(net);
    let Some(first) = nets.first().copied() else {
        return Err(Error::Invalid(
            "routing between points needs a net; add \"net\" to the command".into(),
        ));
    };
    if nets.iter().any(|n| *n != first) {
        return Err(mismatch());
    }
    if !p.nets.iter().any(|n| n.id == first) {
        return Err(Error::Invalid("unknown net".into()));
    }
    match (points.first(), points.get(1)) {
        (Some(a), Some(b)) => Ok((*a, *b, first)),
        _ => Err(mismatch()),
    }
}

/// Clearance a track of `net` must keep from `o`, as the checks demand it.
pub(crate) fn required(p: &Project, layer: Layer, net: Id, o: &Obstacle) -> f64 {
    p.required_clearance_at(net, o.net, o.at, o.at, layer, layer)
        .max(o.clearance.unwrap_or(0.0))
}

/// Bounding box of the board: its outline when one is traced, else the rectangle.
pub(crate) fn bounds(p: &Project) -> (Point, Point) {
    match &p.board.outline {
        Some(poly) if !poly.is_empty() => {
            let min_x = poly.iter().map(|q| q.x).fold(f64::INFINITY, f64::min);
            let min_y = poly.iter().map(|q| q.y).fold(f64::INFINITY, f64::min);
            let max_x = poly.iter().map(|q| q.x).fold(f64::NEG_INFINITY, f64::max);
            let max_y = poly.iter().map(|q| q.y).fold(f64::NEG_INFINITY, f64::max);
            (Point::new(min_x, min_y), Point::new(max_x, max_y))
        }
        _ => (
            Point::new(0.0, 0.0),
            Point::new(p.board.width, p.board.height),
        ),
    }
}

/// Hulls of every fixed copper item and cutout that a track of `net` and
/// `width` must avoid: everything except movable straight tracks.
pub(crate) fn solid_hulls(p: &Project, layer: Layer, net: Id, width: f64) -> Vec<Hull> {
    let clearance = p.board.clearance;
    let mut hulls: Vec<Hull> = obstacles(p, layer)
        .into_iter()
        .filter(|o| !o.is_track && (o.net != net || o.net == 0))
        .map(|o| Hull {
            pts: octagon(
                &o.points,
                required(p, layer, net, &o) + width / 2.0 + o.radius + MARGIN,
            ),
            net: o.net,
        })
        .collect();
    for c in &p.board.cutouts {
        hulls.push(Hull {
            pts: octagon(&[c.center], clearance + width / 2.0 + c.radius + MARGIN),
            net: 0,
        });
    }
    hulls
}

fn track_hulls(p: &Project, layer: Layer, net: Id, width: f64) -> Vec<Hull> {
    obstacles(p, layer)
        .into_iter()
        .filter(|o| o.is_track && o.net != net)
        .map(|o| Hull {
            pts: octagon(
                &o.points,
                required(p, layer, net, &o) + width / 2.0 + o.radius + MARGIN,
            ),
            net: o.net,
        })
        .collect()
}

fn errors(p: &Project) -> BTreeSet<(&'static str, Vec<Id>)> {
    drc(p)
        .into_iter()
        .filter(|f| f.severity == "error")
        .map(|f| {
            let mut objects = f.objects;
            objects.sort_unstable();
            (f.code, objects)
        })
        .collect()
}

/// Route one net on one layer. On any refusal the project is left as it was by
/// the caller (`Session::batch` only commits a candidate that returns `Ok`).
#[allow(clippy::too_many_arguments)]
pub(crate) fn run(
    p: &mut Project,
    next: &mut Id,
    start: Point,
    end: Point,
    net: Id,
    width: f64,
    layer: Layer,
    mode: Mode,
) -> Result<Value, Error> {
    let before = errors(p);
    let lo = p.board.clearance + width / 2.0;
    let (min, max) = bounds(p);
    let mut hulls = solid_hulls(p, layer, net, width);
    if mode == Mode::Octilinear {
        hulls.extend(track_hulls(p, layer, net, width));
    }
    let space = Space {
        hulls,
        lo: Point::new(min.x + lo, min.y + lo),
        hi: Point::new(max.x - lo, max.y - lo),
    };
    let name = p.net_name(net);
    let path = find_path(start, end, &space).map_err(|f| match f {
        Failure::OutsideBoard => Error::Invalid(format!(
            "an end of the route is closer to the board edge than clearance plus half the track width ({lo:.3} mm); move the part or narrow the track"
        )),
        Failure::TooClose(other) => Error::Invalid(format!(
            "an end of the route is already too close to net {}; fix that first",
            p.net_name(other)
        )),
        Failure::NoPath => Error::Invalid(format!(
            "no clear 45-degree path for net {name} on {}: pads, cutouts or fixed copper leave no room within {WALK_ITERATIONS} detours; move a part or route in two steps",
            layer.name()
        )),
    })?;
    let new_line = shove::Line {
        ids: vec![],
        net,
        width,
        pts: path,
        rank: 0,
    };
    let lines = if mode == Mode::OctilinearShove {
        shove::push(p, layer, net, width, &new_line.pts)?
    } else {
        shove::Outcome {
            lines: vec![new_line],
            iterations: 0,
        }
    };
    let mut ids = vec![];
    let mut pushed = vec![];
    let mut touched = vec![];
    for (k, line) in lines.lines.iter().enumerate() {
        let used = shove::apply(p, next, line, layer)?;
        touched.extend(used.iter().copied());
        if k == 0 {
            ids = used;
        } else {
            pushed.push(json!({"net":line.net,"ids":used,"rank":line.rank}));
        }
    }
    let after = errors(p);
    if let Some((code, objects)) = after
        .difference(&before)
        .find(|(_, objects)| objects.iter().any(|id| touched.contains(id)))
    {
        return Err(Error::Invalid(format!(
            "the route would break the {code} check on object(s) {objects:?}; nothing was changed"
        )));
    }
    Ok(
        json!({"ids":ids,"net":net,"mode":mode_name(mode),"pushed":pushed,"shove_steps":lines.iterations}),
    )
}

fn mode_name(mode: Mode) -> &'static str {
    match mode {
        Mode::Orthogonal => "orthogonal",
        Mode::Octilinear => "octilinear",
        Mode::OctilinearShove => "octilinear_shove",
    }
}

#[cfg(test)]
mod tests;
