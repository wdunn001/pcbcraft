//! Collision-aware drag of one end of an existing track (`track.drag`).
//!
//! Reuses `route::run`, so legality, limits and refusal wording are the route
//! rules, not a second copy. Only iteration counts bound the work.

use super::{Mode, run as route_run};
use crate::Error;
use pcbcraft_doc::{Id, Project};
use pcbcraft_geom::Point;
use serde_json::Value;

/// How near two endpoints must be to count as the same point of a run.
///
/// A routed run is one track record per straight piece, and a corner is two
/// records that share a point exactly. This is a guard against float noise
/// rather than a search radius.
const JOIN: f64 = 1e-6;

/// Drag one end of track `id` to `position`, keeping the other end fixed.
///
/// Every other piece of the run that meets the point being moved comes along,
/// so a corner stays a corner. Re-routes each as 45-degree copper. Shove mode
/// may push other nets. An error leaves the project unchanged.
///
/// # Rules
/// - `Orthogonal` is not a drag mode: it has no obstacle checks.
/// - The two ends may not meet and the new end must lie on the board.
/// - A piece is carried only when it shares the point on the same net and the
///   same layer. Pieces joined through a via are on another layer, and the via
///   itself does not move, so they stay put.
/// - A carried piece whose far end is the new position has become nothing and
///   is dropped, which is what straightening a corner away means.
///
/// # Dependencies
/// - `crates/engine/src/route/mod.rs` `run`: path search and shove.
///
/// # Do not
/// - Do not move the fixed end.
/// - Do not bypass `route::run`'s DRC comparison.
pub(crate) fn run(
    p: &mut Project,
    next: &mut Id,
    id: Id,
    drag_end: bool,
    position: Point,
    mode: Mode,
) -> Result<Value, Error> {
    if mode == Mode::Orthogonal {
        return Err(Error::Invalid(
            "dragging with clearance needs mode octilinear or octilinear_shove".into(),
        ));
    }
    let (min, max) = super::bounds(p);
    if !position.valid()
        || position.x < min.x
        || position.y < min.y
        || position.x > max.x
        || position.y > max.y
    {
        return Err(Error::Invalid("track end must lie on the board".into()));
    }
    let Some(at) = p.board.tracks.iter().position(|t| t.id == id) else {
        return Err(crate::missing());
    };
    let track = p.board.tracks.remove(at);
    let fixed = if drag_end { track.start } else { track.end };
    let held = if drag_end { track.end } else { track.start };
    if fixed.distance(position) < 1e-6 {
        return Err(Error::Invalid("the two ends cannot meet".into()));
    }

    // Read the rest of the junction before any copper moves, so the search is
    // over the board as the user sees it.
    let carried: Vec<(Id, bool)> = p
        .board
        .tracks
        .iter()
        .filter(|t| t.net == track.net && t.layer == track.layer)
        .filter_map(|t| {
            if t.start.distance(held) < JOIN {
                Some((t.id, false))
            } else if t.end.distance(held) < JOIN {
                Some((t.id, true))
            } else {
                None
            }
        })
        .collect();

    let (start, end) = if drag_end {
        (fixed, position)
    } else {
        (position, fixed)
    };
    let mut out = route_run(
        p,
        next,
        start,
        end,
        track.net,
        track.width,
        track.layer,
        mode,
    )?;

    let mut moved = Vec::new();
    for (id, piece_end) in carried {
        let Some(at) = p.board.tracks.iter().position(|t| t.id == id) else {
            continue;
        };
        let piece = p.board.tracks.remove(at);
        let far = if piece_end { piece.start } else { piece.end };
        if far.distance(position) < JOIN {
            continue; // collapsed to nothing; the corner straightened out
        }
        let (s, e) = if piece_end {
            (far, position)
        } else {
            (position, far)
        };
        let laid = route_run(p, next, s, e, piece.net, piece.width, piece.layer, mode)?;
        if let Some(ids) = laid.get("ids").and_then(Value::as_array) {
            moved.extend(ids.iter().cloned());
        }
    }
    if let Some(map) = out.as_object_mut() {
        map.insert("carried".into(), Value::Array(moved));
    }
    Ok(out)
}
