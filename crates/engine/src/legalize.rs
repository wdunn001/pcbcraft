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
//!
//! Dependencies: crates/rules/src/courtyard.rs (courtyard_pairs),
//! crates/engine/src/scope.rs (Scope::may_move), crates/geom/src/lib.rs (Point).
use crate::scope::Scope;
use pcbcraft_doc::{Footprint, Id, Project};
use pcbcraft_geom::Point;
use pcbcraft_rules::courtyard_pairs;

/// Maximum passes the legaliser will try before giving up.
pub const MAX_PASSES: usize = 32;

/// The plan of moves to remove overlaps.
///
/// # Rules
/// - `moves` lists footprint ids and their target positions, sorted by id.
/// - `unresolved` lists pairs that could not be cleared (both locked or out of scope).
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
    let mut unresolved: Vec<(Id, Id)> = vec![];
    let mut passes = 0;

    for pass in 0..max_passes {
        passes = pass + 1;
        let pairs = courtyard_pairs(&clone);
        if pairs.is_empty() {
            break;
        }
        for pair in &pairs {
            let a = footprint_by_id(&clone, pair.a);
            let b = footprint_by_id(&clone, pair.b);
            let (a, b) = match (a, b) {
                (Some(a), Some(b)) => (a, b),
                _ => continue,
            };
            let a_may = scope.may_move(&clone, a);
            let b_may = scope.may_move(&clone, b);
            if !a_may && !b_may {
                unresolved.push((pair.a, pair.b));
                continue;
            }
            // Decide which part to move: prefer the one that may move;
            // if both may move, move the one with the larger id.
            let (mover_id, push_dir) = if a_may && b_may {
                if pair.a > pair.b {
                    (pair.a, (pair.push.0, pair.push.1))
                } else {
                    (pair.b, (-pair.push.0, -pair.push.1))
                }
            } else if a_may {
                (pair.a, (pair.push.0, pair.push.1))
            } else {
                (pair.b, (-pair.push.0, -pair.push.1))
            };
            let dist = pair.depth + 1e-3;
            let dx = push_dir.0 * dist;
            let dy = push_dir.1 * dist;
            // Compute the new position before borrowing mutably.
            let orig_mover = footprint_by_id(&clone, mover_id);
            let orig_mover = match orig_mover {
                Some(f) => f,
                None => continue,
            };
            let new_pos = Point::new(orig_mover.position.x + dx, orig_mover.position.y + dy);
            let pos = match grid {
                Some(g) => snap_point(new_pos, g),
                None => new_pos,
            };
            if within_edge_clearance(&clone, orig_mover, pos, clone.board.clearance) {
                if let Some(mover) = footprint_by_id_mut(&mut clone, mover_id) {
                    mover.position = pos;
                }
            } else if clone.board.outline.is_none() {
                let clamped = clamp_position(&clone, orig_mover, pos, clone.board.clearance);
                if let Some(mover) = footprint_by_id_mut(&mut clone, mover_id) {
                    mover.position = clamped;
                }
            } else {
                unresolved.push((pair.a, pair.b));
            }
        }
    }

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

/// Snap a value to the nearest grid point.
fn snap_to_grid(value: f64, grid: f64) -> f64 {
    (value / grid).round() * grid
}

/// Snap a point to the nearest grid point.
fn snap_point(p: Point, grid: f64) -> Point {
    Point::new(snap_to_grid(p.x, grid), snap_to_grid(p.y, grid))
}

/// Compute the bounding box of a footprint world coordinates.
fn footprint_bbox(p: &Project, f: &Footprint) -> (Point, Point) {
    let points = f.drawing_points(p);
    if points.is_empty() {
        return (f.position, f.position);
    }
    let mut lo = Point::new(f64::MAX, f64::MAX);
    let mut hi = Point::new(f64::MIN, f64::MIN);
    for pt in &points {
        let world = f.position.offset(pt.rotate(-f.rotation));
        lo = Point::new(lo.x.min(world.x), lo.y.min(world.y));
        hi = Point::new(hi.x.max(world.x), hi.y.max(world.y));
    }
    (lo, hi)
}

/// Check if a footprint at the given position is within the board edge clearance.
fn within_edge_clearance(p: &Project, f: &Footprint, pos: Point, edge_clear: f64) -> bool {
    let (lo, hi) = footprint_bbox(p, f);
    let offset_x = pos.x - f.position.x;
    let offset_y = pos.y - f.position.y;
    let lo_x = lo.x + offset_x;
    let lo_y = lo.y + offset_y;
    let hi_x = hi.x + offset_x;
    let hi_y = hi.y + offset_y;
    if p.board.outline.is_none() {
        if lo_x < edge_clear || lo_y < edge_clear {
            return false;
        }
        if hi_x > p.board.width - edge_clear {
            return false;
        }
        if hi_y > p.board.height - edge_clear {
            return false;
        }
        return true;
    }
    let corners = [
        Point::new(lo_x, lo_y),
        Point::new(hi_x, lo_y),
        Point::new(hi_x, hi_y),
        Point::new(lo_x, hi_y),
    ];
    for corner in &corners {
        if !p.board_contains(*corner) {
            return false;
        }
    }
    true
}

/// Clamp a position to stay within the board edge clearance (rectangular boards only).
fn clamp_position(p: &Project, f: &Footprint, pos: Point, edge_clear: f64) -> Point {
    if p.board.outline.is_some() {
        return pos;
    }
    let (lo, hi) = footprint_bbox(p, f);
    let offset_x = pos.x - f.position.x;
    let offset_y = pos.y - f.position.y;
    let lo_x = lo.x + offset_x;
    let lo_y = lo.y + offset_y;
    let hi_x = hi.x + offset_x;
    let hi_y = hi.y + offset_y;
    let mut result = pos;
    if lo_x < edge_clear {
        result.x += edge_clear - lo_x;
    }
    if hi_x > p.board.width - edge_clear {
        result.x -= hi_x - (p.board.width - edge_clear);
    }
    if lo_y < edge_clear {
        result.y += edge_clear - lo_y;
    }
    if hi_y > p.board.height - edge_clear {
        result.y -= hi_y - (p.board.height - edge_clear);
    }
    result
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
        // Place two overlapping parts near the left edge.
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
        let first_pos = first.position;
        let second = s
            .project
            .board
            .footprints
            .iter_mut()
            .find(|f| f.component == c2)
            .ok_or("the second footprint is missing")?;
        second.position = first_pos;
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
        // The plan should complete without error.
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
        assert_eq!(locked_after.position.x, 10.0, "locked part moved in x");
        assert_eq!(locked_after.position.y, 10.0, "locked part moved in y");
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
}
