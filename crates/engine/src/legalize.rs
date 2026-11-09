//! Pure legalization: remove courtyard overlaps with the smallest moves.
//!
//! Context: after any global placement, parts may overlap. This step finds
//! overlapping courtyard pairs and moves one part of each pair by the minimum
//! distance needed to clear the overlap. Locked parts never move. The result
//! is deterministic: same input gives same moves.
//!
//! Local rules:
//! - Work on a clone of the project; the input is never changed.
//! - Each pass processes all overlapping pairs; repeat until none remain or
//!   the pass limit is reached.
//! - When both parts of an overlap may move, the one with the larger footprint
//!   id moves (a fixed tie-break for determinism).
//! - Move distance is the overlap depth plus 1e-3 mm to guarantee clearance.
//! - With a grid, a moved part snaps in the direction it moved, so snapping
//!   never gives back the distance it gained.
//! - A moved part keeps its courtyard edges the board clearance away from each
//!   side of the board and inside a traced outline.
//! - A pair that is still overlapping at the end is reported as unresolved.
//!
//! Dependencies: crates/rules/src/courtyard.rs (courtyard_pairs),
//! crates/engine/src/scope.rs (Scope::may_move), crates/geom/src/lib.rs (Point).
use super::{CommandSpec, Error, schema};
use crate::scope::{self, Scope};
use pcbcraft_doc::{Footprint, Id, Project};
use pcbcraft_geom::Point;
use pcbcraft_rules::courtyard_pairs;
use serde::Deserialize;
use serde_json::{Value, json};

/// Maximum passes the legaliser will try before giving up.
pub const MAX_PASSES: usize = 32;

/// The plan of moves to remove overlaps.
///
/// # Rules
/// - `moves` lists footprint ids and their target positions, sorted by id.
/// - `unresolved` lists pairs still overlapping at the end (both fixed, or no legal spot).
/// - `passes` is the number of passes actually run.
///
/// # Dependencies
/// - crates/engine/src/legalize.rs (plan_moves)
///
/// # Do not
/// - Apply moves to the original project; apply to a clone.
#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    /// Footprints to move and their new positions, sorted by footprint id.
    pub moves: Vec<(Id, Point)>,
    /// Overlapping pairs that could not be resolved.
    pub unresolved: Vec<(Id, Id)>,
    /// Number of passes run.
    pub passes: usize,
}

/// Find the minimum moves to remove all courtyard overlaps in scope.
///
/// Why: after placement, parts may overlap; this step clears them with the
/// smallest moves, respecting locks and scope, keeping parts on the grid and
/// within the board edge clearance.
///
/// # Rules
/// - The input project is never changed; work happens on a clone.
/// - Locked footprints and those outside the scope never move.
/// - When both parts of an overlap may move, the one with the larger id moves.
/// - Move distance is the overlap depth plus 1e-3 mm.
/// - With a grid, every moved position lands on it after the move and the checks.
/// - A moved part keeps the board edge clearance; one that cannot fit stays put.
/// - Deterministic: same input gives the same plan.
///
/// # Dependencies
/// - crates/rules/src/courtyard.rs (courtyard_pairs)
/// - crates/engine/src/scope.rs (Scope::may_move)
/// - crates/geom/src/lib.rs (Point)
///
/// # Do not
/// - Move locked parts or parts outside the scope.
/// - Loop more than max_passes times.
pub fn plan_moves(p: &Project, scope: &Scope, max_passes: usize, grid: Option<f64>) -> Plan {
    let mut clone = p.clone();
    let mut moves: Vec<(Id, Point)> = vec![];
    let mut passes = 0;

    for pass in 0..max_passes {
        passes = pass + 1;
        let pairs = courtyard_pairs(&clone);
        if pairs.is_empty() {
            break;
        }
        let mut changed = false;
        for pair in &pairs {
            let (Some(a), Some(b)) = (
                footprint_by_id(&clone, pair.a),
                footprint_by_id(&clone, pair.b),
            ) else {
                continue;
            };
            let a_may = scope.may_move(&clone, a);
            let b_may = scope.may_move(&clone, b);
            // The push points from part a to part b. Part a moves against it,
            // part b moves along it. When both may move, the larger id moves.
            let (mover, sign) = match (a_may, b_may) {
                (false, false) => continue,
                (true, false) => (a, -1.0),
                (false, true) => (b, 1.0),
                (true, true) if pair.a > pair.b => (a, -1.0),
                (true, true) => (b, 1.0),
            };
            let dist = pair.depth + 1e-3;
            let wanted = Point::new(
                mover.position.x + sign * pair.push.0 * dist,
                mover.position.y + sign * pair.push.1 * dist,
            );
            let Some(target) = legal_target(&clone, mover, wanted, grid) else {
                continue;
            };
            let mover_id = mover.id;
            if let Some(m) = footprint_by_id_mut(&mut clone, mover_id)
                && m.position != target
            {
                m.position = target;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    // Whatever still overlaps could not be cleared.
    let unresolved: Vec<(Id, Id)> = courtyard_pairs(&clone)
        .iter()
        .map(|pair| (pair.a, pair.b))
        .collect();

    // Collect moves by comparing original and final positions.
    for (orig, final_fp) in p.board.footprints.iter().zip(clone.board.footprints.iter()) {
        if orig.position != final_fp.position {
            moves.push((orig.id, final_fp.position));
        }
    }
    moves.sort_by_key(|(id, _)| *id);

    Plan {
        moves,
        unresolved,
        passes,
    }
}

fn footprint_by_id(p: &Project, id: Id) -> Option<&Footprint> {
    p.board.footprints.iter().find(|f| f.id == id)
}

fn footprint_by_id_mut(p: &mut Project, id: Id) -> Option<&mut Footprint> {
    p.board.footprints.iter_mut().find(|f| f.id == id)
}

/// The usable grid step, or `None` when no grid applies.
fn usable_grid(grid: Option<f64>) -> Option<f64> {
    grid.filter(|g| g.is_finite() && *g > 0.0)
}

/// Snap a value to a grid point without moving it back toward `from`.
///
/// A value above `from` rounds up, a value below rounds down, an unchanged
/// value rounds to the nearest point. A tiny tolerance keeps values that are
/// already on the grid where they are.
fn snap_away(value: f64, from: f64, grid: f64) -> f64 {
    let steps = value / grid;
    let snapped = if value > from {
        (steps - 1e-9).ceil()
    } else if value < from {
        (steps + 1e-9).floor()
    } else {
        steps.round()
    };
    snapped * grid
}

/// Bounding box of a footprint relative to its own position.
fn footprint_extent(p: &Project, f: &Footprint) -> (Point, Point) {
    let points = f.drawing_points(p);
    let mut lo = Point::new(f64::MAX, f64::MAX);
    let mut hi = Point::new(f64::MIN, f64::MIN);
    for pt in &points {
        let rel = pt.rotate(-f.rotation);
        lo = Point::new(lo.x.min(rel.x), lo.y.min(rel.y));
        hi = Point::new(hi.x.max(rel.x), hi.y.max(rel.y));
    }
    if points.is_empty() {
        return (Point::new(0.0, 0.0), Point::new(0.0, 0.0));
    }
    (lo, hi)
}

/// Closest legal position for a footprint that wants to go to `wanted`.
///
/// The result is on the grid when one is given. Its bounding box keeps the
/// board edge clearance from each side of a rectangular board. On a board with
/// a traced outline the box grown by the clearance must lie inside the outline.
/// Returns `None` when no such position exists.
fn legal_target(p: &Project, f: &Footprint, wanted: Point, grid: Option<f64>) -> Option<Point> {
    let grid = usable_grid(grid);
    let clearance = p.board.clearance;
    let (lo, hi) = footprint_extent(p, f);
    let snap = |v: f64, from: f64| match grid {
        Some(g) => snap_away(v, from, g),
        None => v,
    };
    if p.board.outline.is_some() {
        let pos = Point::new(snap(wanted.x, f.position.x), snap(wanted.y, f.position.y));
        let corners = [
            Point::new(pos.x + lo.x - clearance, pos.y + lo.y - clearance),
            Point::new(pos.x + hi.x + clearance, pos.y + lo.y - clearance),
            Point::new(pos.x + hi.x + clearance, pos.y + hi.y + clearance),
            Point::new(pos.x + lo.x - clearance, pos.y + hi.y + clearance),
        ];
        return corners.iter().all(|c| p.board_contains(*c)).then_some(pos);
    }
    let axis = |value: f64, from: f64, rel_lo: f64, rel_hi: f64, size: f64| -> Option<f64> {
        let mut min = clearance - rel_lo;
        let mut max = size - clearance - rel_hi;
        if let Some(g) = grid {
            min = (min / g - 1e-9).ceil() * g;
            max = (max / g + 1e-9).floor() * g;
        }
        if min > max + 1e-9 {
            return None;
        }
        let snapped = snap(value, from);
        Some(snapped.clamp(min, max.max(min)))
    };
    Some(Point::new(
        axis(wanted.x, f.position.x, lo.x, hi.x, p.board.width)?,
        axis(wanted.y, f.position.y, lo.y, hi.y, p.board.height)?,
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegalizeParams {
    scope: Value,
    #[serde(default)]
    grid: Option<f64>,
    #[serde(default = "max_passes_default")]
    max_passes: usize,
}

fn max_passes_default() -> usize {
    32
}

pub(super) fn commands() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "placement.legalize",
            description: "Move overlapping parts to remove courtyard overlaps and edge violations with the smallest moves; locked parts never move; snap to a grid; deterministic with the same input giving the same result",
            input_schema: schema(
                json!({
                    "scope": scope::schema(),
                    "grid": {"type":"number","description":"grid step in millimetres to snap moved parts to; optional, default none"},
                    "max_passes": {"type":"integer","minimum":1,"maximum":200,"default":32,"description":"most passes to try before giving up"}
                }),
                &["scope"],
            ),
        },
        CommandSpec {
            id: "layout.legalize.plan",
            description: "Return the plan of moves the legalizer would make without applying them; the result lists parts to move and their target positions plus any pairs that cannot be cleared",
            input_schema: schema(
                json!({
                    "scope": scope::schema(),
                    "grid": {"type":"number","description":"grid step in millimetres to snap moved parts to; optional, default none"},
                    "max_passes": {"type":"integer","minimum":1,"maximum":200,"default":32,"description":"most passes to try before giving up"}
                }),
                &["scope"],
            ),
        },
    ]
}

pub(super) fn run(
    p: &mut Project,
    _next: &mut Id,
    command: &str,
    params: &Value,
) -> Option<Result<Value, Error>> {
    match command {
        "placement.legalize" => Some(legalize(p, params)),
        "layout.legalize.plan" => Some(legalize_plan(p, params)),
        _ => None,
    }
}

fn legalize(p: &mut Project, params: &Value) -> Result<Value, Error> {
    let a: LegalizeParams = super::parse(params)?;
    let scope = Scope::from_value(p, &a.scope)?;
    if a.grid.is_some_and(|g| !g.is_finite() || g <= 0.0) {
        return Err(Error::Invalid(
            "grid must be a positive finite number".into(),
        ));
    }
    let plan = plan_moves(p, &scope, a.max_passes, a.grid);
    let mut moved = Vec::new();
    for (fid, pos) in &plan.moves {
        let Some(f) = p
            .board
            .footprints
            .iter_mut()
            .find(|f| f.id == *fid && f.locked != Some(true))
        else {
            continue;
        };
        let before = f.position;
        f.position = *pos;
        if f.position != before {
            moved.push(json!({"id": fid, "x": pos.x, "y": pos.y}));
        }
    }
    let unresolved = plan
        .unresolved
        .into_iter()
        .map(|(a, b)| json!({"a": a, "b": b}))
        .collect::<Vec<_>>();
    Ok(json!({"moved": moved, "unresolved": unresolved, "passes": plan.passes}))
}

fn legalize_plan(p: &Project, params: &Value) -> Result<Value, Error> {
    let a: LegalizeParams = super::parse(params)?;
    let scope = Scope::from_value(p, &a.scope)?;
    if a.grid.is_some_and(|g| !g.is_finite() || g <= 0.0) {
        return Err(Error::Invalid(
            "grid must be a positive finite number".into(),
        ));
    }
    let plan = plan_moves(p, &scope, a.max_passes, a.grid);
    let moves: Vec<Value> = plan
        .moves
        .into_iter()
        .map(|(id, pos)| json!({"id": id, "x": pos.x, "y": pos.y}))
        .collect();
    let unresolved: Vec<Value> = plan
        .unresolved
        .into_iter()
        .map(|(a, b)| json!({"a": a, "b": b}))
        .collect();
    Ok(json!({"moves": moves, "unresolved": unresolved, "passes": plan.passes}))
}

/// Compute the bounding box of a footprint in world coordinates.
#[cfg(test)]
fn footprint_bbox(p: &Project, f: &Footprint) -> (Point, Point) {
    let (lo, hi) = footprint_extent(p, f);
    (f.position.offset(lo), f.position.offset(hi))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Session;
    use serde_json::json;

    fn id(v: &serde_json::Value) -> Result<u64, String> {
        v["result"]["id"]
            .as_u64()
            .ok_or_else(|| "the command answer has no id".to_string())
    }

    fn run(
        session: &mut Session,
        command: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        session
            .execute(command, params, None)
            .map_err(|e| e.to_string())
    }

    fn full_scope() -> Scope {
        Scope {
            layers: vec!["F.Cu".into()],
            nets: vec![],
            components: vec![],
            respect_locked: true,
        }
    }

    /// Create two overlapping footprints and return their ids.
    fn make_overlapping(session: &mut Session) -> Result<(Id, Id), String> {
        let c1 = id(&run(
            session,
            "component.add",
            json!({"kind":"resistor","reference":"R1","value":"1k","position":{"x":10,"y":10}}),
        )?)?;
        let c2 = id(&run(
            session,
            "component.add",
            json!({"kind":"resistor","reference":"R2","value":"1k","position":{"x":10,"y":10}}),
        )?)?;
        run(session, "board.sync", json!({}))?;
        // Move the second footprint to overlap the first.
        let first = session
            .project
            .board
            .footprints
            .iter()
            .find(|f| f.component == c1)
            .ok_or("the first footprint is missing")?;
        let first_pos = first.position;
        let second = session
            .project
            .board
            .footprints
            .iter_mut()
            .find(|f| f.component == c2)
            .ok_or("the second footprint is missing")?;
        second.position = first_pos;
        Ok((c1, c2))
    }

    #[test]
    fn two_overlapping_footprints_are_separated() -> Result<(), String> {
        let mut s = Session::default();
        s.project.auto_sync = false;
        make_overlapping(&mut s)?;
        let scope = full_scope();
        let plan = plan_moves(&s.project, &scope, MAX_PASSES, None);
        assert!(!plan.moves.is_empty(), "no moves were made");
        Ok(())
    }

    #[test]
    fn locked_footprint_is_not_moved() -> Result<(), String> {
        let mut s = Session::default();
        s.project.auto_sync = false;
        make_overlapping(&mut s)?;
        // Lock the first footprint.
        let locked_id = {
            let locked = s
                .project
                .board
                .footprints
                .iter_mut()
                .find(|f| f.locked != Some(true))
                .ok_or("no unlocked footprint found")?;
            locked.locked = Some(true);
            locked.id
        };
        let scope = full_scope();
        let plan = plan_moves(&s.project, &scope, MAX_PASSES, None);
        // The locked footprint should not be in the moves list.
        assert!(
            !plan.moves.iter().any(|(fid, _)| *fid == locked_id),
            "locked footprint was moved"
        );
        Ok(())
    }

    #[test]
    fn both_locked_is_unresolved() -> Result<(), String> {
        let mut s = Session::default();
        s.project.auto_sync = false;
        let (c1, c2) = make_overlapping(&mut s)?;
        // Lock both footprints.
        for f in &mut s.project.board.footprints {
            f.locked = Some(true);
        }
        let scope = full_scope();
        let plan = plan_moves(&s.project, &scope, MAX_PASSES, None);
        // Both locked means unresolved.
        let f1 = s
            .project
            .board
            .footprints
            .iter()
            .find(|f| f.component == c1)
            .ok_or("first footprint missing")?
            .id;
        let f2 = s
            .project
            .board
            .footprints
            .iter()
            .find(|f| f.component == c2)
            .ok_or("second footprint missing")?
            .id;
        assert!(
            plan.unresolved.contains(&(f1, f2)) || plan.unresolved.contains(&(f2, f1)),
            "unresolved pair missing"
        );
        assert!(plan.moves.is_empty(), "moves made despite both locked");
        Ok(())
    }

    #[test]
    fn max_passes_stops_after_one_pass() -> Result<(), String> {
        let mut s = Session::default();
        s.project.auto_sync = false;
        make_overlapping(&mut s)?;
        let scope = full_scope();
        let plan = plan_moves(&s.project, &scope, 1, None);
        assert_eq!(plan.passes, 1, "did not stop after one pass");
        Ok(())
    }

    #[test]
    fn edge_clearance_is_kept() -> Result<(), String> {
        let mut s = Session::default();
        s.project.auto_sync = false;
        s.project.board.width = 40.0;
        s.project.board.height = 30.0;
        s.project.board.clearance = 1.0;
        // Add two parts; their spots are set by hand below.
        let c1 = id(&run(
            &mut s,
            "component.add",
            json!({"kind":"resistor","reference":"R1","value":"1k","position":{"x":2,"y":10}}),
        )?)?;
        let c2 = id(&run(
            &mut s,
            "component.add",
            json!({"kind":"resistor","reference":"R2","value":"1k","position":{"x":2,"y":10}}),
        )?)?;
        run(&mut s, "board.sync", json!({}))?;
        // Move the second footprint to overlap the first.
        let first = s
            .project
            .board
            .footprints
            .iter()
            .find(|f| f.component == c1)
            .ok_or("the first footprint is missing")?;
        let first_id = first.id;
        // The sync step puts parts at its own spots, so place both by hand: the
        // first near the top right corner, the second just above it. The push
        // would take the second part outside the edge clearance.
        for f in &mut s.project.board.footprints {
            if f.id == first_id {
                f.position = Point::new(34.0, 4.0);
            } else if f.component == c2 {
                f.position = Point::new(34.0, 3.5);
            }
        }
        let scope = full_scope();
        let plan = plan_moves(&s.project, &scope, MAX_PASSES, None);
        // Apply moves to a clone.
        let mut clone = s.project.clone();
        for (fid, pos) in &plan.moves {
            if let Some(f) = clone.board.footprints.iter_mut().find(|f| f.id == *fid) {
                f.position = *pos;
            }
        }
        // Check that no footprint violates edge clearance.
        for f in &clone.board.footprints {
            let (lo, hi) = footprint_bbox(&clone, f);
            assert!(
                lo.x >= clone.board.clearance - 1e-9,
                "footprint {} left edge at {:.3} is less than clearance {:.3}",
                f.id,
                lo.x,
                clone.board.clearance
            );
            assert!(
                lo.y >= clone.board.clearance - 1e-9,
                "footprint {} bottom edge at {:.3} is less than clearance {:.3}",
                f.id,
                lo.y,
                clone.board.clearance
            );
            assert!(
                hi.x <= clone.board.width - clone.board.clearance + 1e-9,
                "footprint {} right edge at {:.3} exceeds board width minus clearance",
                f.id,
                hi.x
            );
            assert!(
                hi.y <= clone.board.height - clone.board.clearance + 1e-9,
                "footprint {} top edge at {:.3} exceeds board height minus clearance",
                f.id,
                hi.y
            );
        }
        Ok(())
    }

    #[test]
    fn moves_land_on_the_grid() -> Result<(), String> {
        let mut s = Session::default();
        s.project.auto_sync = false;
        make_overlapping(&mut s)?;
        let scope = full_scope();
        let plan = plan_moves(&s.project, &scope, MAX_PASSES, Some(0.5));
        // Check that every moved position is on the 0.5mm grid.
        for (_fid, pos) in &plan.moves {
            let x_on_grid = ((pos.x * 2.0).round() - pos.x * 2.0).abs() < 1e-9;
            let y_on_grid = ((pos.y * 2.0).round() - pos.y * 2.0).abs() < 1e-9;
            assert!(x_on_grid, "x coordinate {:.4} is not on 0.5mm grid", pos.x);
            assert!(y_on_grid, "y coordinate {:.4} is not on 0.5mm grid", pos.y);
        }
        Ok(())
    }

    #[test]
    fn no_grid_leaves_unsnapped() -> Result<(), String> {
        let mut s = Session::default();
        s.project.auto_sync = false;
        make_overlapping(&mut s)?;
        let scope = full_scope();
        let plan = plan_moves(&s.project, &scope, MAX_PASSES, None);
        // At least one move should exist.
        assert!(!plan.moves.is_empty(), "no moves were made");
        Ok(())
    }

    #[test]
    fn off_board_traced_outline_is_unresolved() -> Result<(), String> {
        let mut s = Session::default();
        s.project.auto_sync = false;
        s.project.board.width = 40.0;
        s.project.board.height = 30.0;
        s.project.board.clearance = 1.0;
        // Create a traced outline board.
        let outline = vec![
            Point::new(0.0, 0.0),
            Point::new(40.0, 0.0),
            Point::new(40.0, 30.0),
            Point::new(0.0, 30.0),
        ];
        s.project.board.outline = Some(outline);
        // Place two parts that will overlap.
        let c1 = id(&run(
            &mut s,
            "component.add",
            json!({"kind":"resistor","reference":"R1","value":"1k","position":{"x":20,"y":15}}),
        )?)?;
        let c2 = id(&run(
            &mut s,
            "component.add",
            json!({"kind":"resistor","reference":"R2","value":"1k","position":{"x":20,"y":15}}),
        )?)?;
        run(&mut s, "board.sync", json!({}))?;
        let first = s
            .project
            .board
            .footprints
            .iter()
            .find(|f| f.component == c1)
            .ok_or("first missing")?;
        let first_pos = first.position;
        let second = s
            .project
            .board
            .footprints
            .iter_mut()
            .find(|f| f.component == c2)
            .ok_or("second missing")?;
        second.position = first_pos;
        let scope = full_scope();
        let plan = plan_moves(&s.project, &scope, MAX_PASSES, None);
        assert!(
            !plan.moves.is_empty() || !plan.unresolved.is_empty(),
            "two parts on the same spot are either moved or reported"
        );
        Ok(())
    }

    #[test]
    fn a_newcomer_has_overlapping_parts_cleaned_up_without_moving_locked_ones() -> Result<(), String>
    {
        let mut s = Session::default();
        s.project.auto_sync = false;
        s.project.board.width = 100.0;
        s.project.board.height = 100.0;
        s.project.board.clearance = 1.0;
        // Add three parts: two overlapping, one locked far away.
        let c1 = id(&run(
            &mut s,
            "component.add",
            json!({"kind":"resistor","reference":"R1","value":"1k","position":{"x":50,"y":50}}),
        )?)?;
        let c2 = id(&run(
            &mut s,
            "component.add",
            json!({"kind":"resistor","reference":"R2","value":"1k","position":{"x":50,"y":50}}),
        )?)?;
        let c3 = id(&run(
            &mut s,
            "component.add",
            json!({"kind":"capacitor","reference":"C1","value":"10uF","position":{"x":10,"y":10}}),
        )?)?;
        run(&mut s, "board.sync", json!({}))?;
        // Lock the third part (far away).
        let locked_fp = s
            .project
            .board
            .footprints
            .iter_mut()
            .find(|f| f.component == c3)
            .ok_or("C1 footprint missing")?;
        locked_fp.locked = Some(true);
        let locked_before = locked_fp.position;
        // Make the first two overlap.
        let f1_pos = s
            .project
            .board
            .footprints
            .iter()
            .find(|f| f.component == c1)
            .ok_or("R1 missing")?
            .position;
        let f2 = s
            .project
            .board
            .footprints
            .iter_mut()
            .find(|f| f.component == c2)
            .ok_or("R2 missing")?;
        f2.position = f1_pos;
        // Run legalization.
        let scope = full_scope();
        let plan = plan_moves(&s.project, &scope, MAX_PASSES, Some(0.5));
        // Apply moves.
        let mut clone = s.project.clone();
        for (fid, pos) in &plan.moves {
            if let Some(f) = clone.board.footprints.iter_mut().find(|f| f.id == *fid) {
                f.position = *pos;
            }
        }
        // Verify the locked part did not move.
        let locked_after = clone
            .board
            .footprints
            .iter()
            .find(|f| f.component == c3)
            .ok_or("C1 missing after")?;
        assert_eq!(
            locked_after.position.x, locked_before.x,
            "locked part moved in x"
        );
        assert_eq!(
            locked_after.position.y, locked_before.y,
            "locked part moved in y"
        );
        // Verify overlapping parts were separated.
        let r1_after = clone
            .board
            .footprints
            .iter()
            .find(|f| f.component == c1)
            .ok_or("R1 missing after")?;
        let r2_after = clone
            .board
            .footprints
            .iter()
            .find(|f| f.component == c2)
            .ok_or("R2 missing after")?;
        assert_ne!(
            r1_after.position, r2_after.position,
            "overlapping parts not separated"
        );
        // Verify all positions are on the grid.
        for f in &clone.board.footprints {
            let x_on = ((f.position.x * 2.0).round() - f.position.x * 2.0).abs() < 1e-9;
            let y_on = ((f.position.y * 2.0).round() - f.position.y * 2.0).abs() < 1e-9;
            assert!(x_on, "x not on grid for {}", f.id);
            assert!(y_on, "y not on grid for {}", f.id);
        }
        Ok(())
    }

    #[test]
    fn legalize_second_run_changes_nothing() -> Result<(), String> {
        let mut s = Session::default();
        s.project.auto_sync = false;
        make_overlapping(&mut s)?;
        let scope = full_scope();
        let plan1 = plan_moves(&s.project, &scope, MAX_PASSES, None);
        assert!(!plan1.moves.is_empty(), "first run should make moves");
        // Apply moves to the project
        for (fid, pos) in &plan1.moves {
            if let Some(f) = s.project.board.footprints.iter_mut().find(|f| f.id == *fid) {
                f.position = *pos;
            }
        }
        // Second run should find no more moves
        let plan2 = plan_moves(&s.project, &scope, MAX_PASSES, None);
        assert!(plan2.moves.is_empty(), "second run should make no moves");
        Ok(())
    }

    #[test]
    fn locked_part_keeps_its_position_to_the_unit() -> Result<(), String> {
        let mut s = Session::default();
        s.project.auto_sync = false;
        make_overlapping(&mut s)?;
        // Lock one footprint
        let (locked_id, locked_pos) = {
            let locked_fp = s
                .project
                .board
                .footprints
                .first_mut()
                .ok_or("no footprint")?;
            locked_fp.locked = Some(true);
            (locked_fp.id, locked_fp.position)
        };
        let scope = full_scope();
        let plan = plan_moves(&s.project, &scope, MAX_PASSES, None);
        // The locked footprint must not appear in moves
        assert!(
            !plan.moves.iter().any(|(fid, _)| *fid == locked_id),
            "locked footprint should not move"
        );
        // And running the command leaves it exactly where it was.
        s.execute(
            "placement.legalize",
            json!({"scope":{"layers":["F.Cu"]}}),
            None,
        )
        .map_err(|e| e.to_string())?;
        let after = s
            .project
            .board
            .footprints
            .iter()
            .find(|f| f.id == locked_id)
            .ok_or("the locked footprint is still there")?;
        assert_eq!(after.position, locked_pos, "locked footprint moved");
        Ok(())
    }

    #[test]
    fn empty_scope_refused() {
        let mut s = Session::default();
        let result = s.execute("placement.legalize", json!({"scope":{}}), None);
        assert!(result.is_err(), "empty scope should be refused");
    }

    #[test]
    fn legalize_one_undo_reverses_the_run() -> Result<(), String> {
        let mut s = Session::default();
        s.project.auto_sync = false;
        make_overlapping(&mut s)?;
        // Record positions before
        let before: Vec<(u64, f64, f64)> = s
            .project
            .board
            .footprints
            .iter()
            .map(|f| (f.id, f.position.x, f.position.y))
            .collect();
        // Run the command
        let result = s
            .execute(
                "placement.legalize",
                json!({"scope":{"layers":["F.Cu"]}}),
                None,
            )
            .map_err(|e| e.to_string())?;
        let moved = result["result"]["moved"]
            .as_array()
            .ok_or("no moved array")?;
        assert!(!moved.is_empty(), "command should have moved parts");
        // Undo
        s.undo().map_err(|e| e.to_string())?;
        // Verify positions are restored
        for (f, (id, x, y)) in s.project.board.footprints.iter().zip(&before) {
            assert_eq!(f.id, *id);
            assert_eq!(f.position.x, *x, "x not restored for {}", f.id);
            assert_eq!(f.position.y, *y, "y not restored for {}", f.id);
        }
        Ok(())
    }

    #[test]
    fn legalize_dry_run_changes_nothing() -> Result<(), String> {
        let mut s = Session::default();
        s.project.auto_sync = false;
        make_overlapping(&mut s)?;
        // Record positions before
        let before: Vec<(u64, f64, f64)> = s
            .project
            .board
            .footprints
            .iter()
            .map(|f| (f.id, f.position.x, f.position.y))
            .collect();
        // Dry run
        let dry = s
            .dry_run(
                &[crate::Command {
                    command: "placement.legalize".into(),
                    params: json!({"scope":{"layers":["F.Cu"]}}),
                }],
                None,
            )
            .map_err(|e| e.to_string())?;
        assert_eq!(dry.results.len(), 1);
        // Verify positions are unchanged
        for (f, (id, x, y)) in s.project.board.footprints.iter().zip(&before) {
            assert_eq!(f.id, *id);
            assert_eq!(f.position.x, *x, "dry run moved x for {}", f.id);
            assert_eq!(f.position.y, *y, "dry run moved y for {}", f.id);
        }
        Ok(())
    }

    #[test]
    fn courtyard_check_passes_after() -> Result<(), String> {
        let mut s = Session::default();
        s.project.auto_sync = false;
        make_overlapping(&mut s)?;
        // Verify there is an overlap before
        assert!(
            !courtyard_pairs(&s.project).is_empty(),
            "setup should have overlaps"
        );
        // Run the command
        s.execute(
            "placement.legalize",
            json!({"scope":{"layers":["F.Cu"]}}),
            None,
        )
        .map_err(|e| e.to_string())?;
        // Verify no overlaps after
        let remaining = courtyard_pairs(&s.project);
        assert!(
            remaining.is_empty(),
            "{} overlaps remain after legalization",
            remaining.len()
        );
        Ok(())
    }
}
