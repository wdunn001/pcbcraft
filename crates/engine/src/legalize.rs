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
/// smallest moves, respecting locks and scope.
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
pub fn plan_moves(p: &Project, scope: &Scope, max_passes: usize) -> Plan {
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
            if let Some(mover) = footprint_by_id_mut(&mut clone, mover_id) {
                mover.position = Point::new(mover.position.x + dx, mover.position.y + dy);
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
        let first_id = first.id;
        let first_pos = first.position;
        let second = session
            .project
            .board
            .footprints
            .iter_mut()
            .find(|f| f.component == c2)
            .ok_or("the second footprint is missing")?;
        let second_id = second.id;
        second.position = first_pos;
        Ok((first_id, second_id))
    }

    #[test]
    fn overlap_is_removed() -> Result<(), String> {
        let mut s = Session::default();
        s.project.auto_sync = false;
        make_overlapping(&mut s)?;
        let scope = full_scope();
        let plan = plan_moves(&s.project, &scope, MAX_PASSES);
        // Apply moves to a clone.
        let mut clone = s.project.clone();
        for (fid, pos) in &plan.moves {
            if let Some(f) = clone.board.footprints.iter_mut().find(|f| f.id == *fid) {
                f.position = *pos;
            }
        }
        let remaining = courtyard_pairs(&clone);
        assert!(remaining.is_empty(), "overlaps remain");
        Ok(())
    }

    #[test]
    fn locked_footprint_does_not_move() -> Result<(), String> {
        let mut s = Session::default();
        s.project.auto_sync = false;
        let (f1, _f2) = make_overlapping(&mut s)?;
        run(
            &mut s,
            "footprint.set_locked",
            json!({"id":f1,"locked":true}),
        )?;
        let scope = full_scope();
        let plan = plan_moves(&s.project, &scope, MAX_PASSES);
        let moved_ids: Vec<Id> = plan.moves.iter().map(|(id, _)| *id).collect();
        assert!(!moved_ids.contains(&f1), "locked footprint moved: {f1}");
        Ok(())
    }

    #[test]
    fn same_input_gives_same_moves() -> Result<(), String> {
        let mut s = Session::default();
        s.project.auto_sync = false;
        make_overlapping(&mut s)?;
        let scope = full_scope();
        let plan1 = plan_moves(&s.project, &scope, MAX_PASSES);
        let plan2 = plan_moves(&s.project, &scope, MAX_PASSES);
        assert_eq!(plan1, plan2, "plans differ on same input");
        Ok(())
    }

    #[test]
    fn both_locked_gives_unresolved() -> Result<(), String> {
        let mut s = Session::default();
        s.project.auto_sync = false;
        let (f1, f2) = make_overlapping(&mut s)?;
        run(
            &mut s,
            "footprint.set_locked",
            json!({"id":f1,"locked":true}),
        )?;
        run(
            &mut s,
            "footprint.set_locked",
            json!({"id":f2,"locked":true}),
        )?;
        let scope = full_scope();
        let plan = plan_moves(&s.project, &scope, MAX_PASSES);
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
        let plan = plan_moves(&s.project, &scope, 1);
        assert_eq!(plan.passes, 1, "did not stop after one pass");
        Ok(())
    }
}
