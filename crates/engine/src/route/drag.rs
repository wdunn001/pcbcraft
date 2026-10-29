//! Collision-aware drag of one end of an existing track (`track.drag`).
//!
//! Reuses `route::run`, so legality, limits and refusal wording are the route
//! rules, not a second copy. Only iteration counts bound the work.

use super::{Mode, run as route_run};
use crate::Error;
use pcbcraft_doc::{Id, Project};
use pcbcraft_geom::Point;
use serde_json::Value;

/// Drag one end of track `id` to `position`, keeping the other end fixed.
///
/// The segment is taken out and its net, width and layer are routed from the
/// fixed end to the new spot as 45-degree copper; shove mode may push other
/// nets' tracks aside. An error leaves the caller's project as it was.
///
/// # Rules
/// - `Orthogonal` is not a drag mode: it has no obstacle checks.
/// - The two ends may not meet and the new end must lie on the board.
///
/// # Dependencies
/// - `crates/engine/src/route/mod.rs` `run`: path search and shove.
///
/// # Do not
/// - Do not move the fixed end or other segments of the same net.
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
    if fixed.distance(position) < 1e-6 {
        return Err(Error::Invalid("the two ends cannot meet".into()));
    }
    let (start, end) = if drag_end {
        (fixed, position)
    } else {
        (position, fixed)
    };
    route_run(
        p,
        next,
        start,
        end,
        track.net,
        track.width,
        track.layer,
        mode,
    )
}
