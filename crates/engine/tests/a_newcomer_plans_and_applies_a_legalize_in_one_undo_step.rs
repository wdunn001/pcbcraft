//! A newcomer adds overlapping parts, plans their moves without applying,
//! applies the plan with placement.legalize, and undoes it in one step.
//!
//! Story: "As a designer I want to see what legalization will move before
//! committing, so I know which locked parts will stay put."

use pcbcraft_engine::Session;
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

/// Create two overlapping footprints and return their component ids.
fn make_overlapping(session: &mut Session) -> Result<(u64, u64), String> {
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
fn plan_command_returns_moves_without_applying() -> Result<(), String> {
    let mut s = Session::default();
    s.project.auto_sync = false;
    let (c1, c2) = make_overlapping(&mut s)?;
    // Run the plan command
    let result = run(
        &mut s,
        "layout.legalize.plan",
        json!({"scope": {"layers": ["F.Cu"]}}),
    )?;
    // Verify the plan shows moves
    let plan = &result["result"];
    let Some(moves) = plan["moves"].as_array() else {
        return Err("plan result has no moves array".to_string());
    };
    assert!(!moves.is_empty(), "plan should propose at least one move");
    // Verify the plan did not apply the moves (positions unchanged)
    let f1 = s
        .project
        .board
        .footprints
        .iter()
        .find(|f| f.component == c1)
        .ok_or("R1 footprint missing")?;
    let f2 = s
        .project
        .board
        .footprints
        .iter()
        .find(|f| f.component == c2)
        .ok_or("R2 footprint missing")?;
    assert_eq!(
        f1.position, f2.position,
        "plan should not have applied moves (positions still same)"
    );
    Ok(())
}

#[test]
fn plan_and_apply_is_one_undo_step() -> Result<(), String> {
    let mut s = Session::default();
    s.project.auto_sync = false;
    let _ = make_overlapping(&mut s)?;
    // Record positions before
    let before: Vec<(u64, f64, f64)> = s
        .project
        .board
        .footprints
        .iter()
        .map(|f| (f.id, f.position.x, f.position.y))
        .collect();
    // Plan then apply
    let plan_result = run(
        &mut s,
        "layout.legalize.plan",
        json!({"scope": {"layers": ["F.Cu"]}}),
    )?;
    let Some(moves) = plan_result["result"]["moves"].as_array() else {
        return Err("plan result has no moves array".to_string());
    };
    assert!(!moves.is_empty(), "plan should have moves");
    run(
        &mut s,
        "placement.legalize",
        json!({"scope": {"layers": ["F.Cu"]}}),
    )?;
    // One undo step restores everything
    s.undo().map_err(|e| e.to_string())?;
    // Verify positions restored
    for (f, (id, x, y)) in s.project.board.footprints.iter().zip(&before) {
        assert_eq!(f.id, *id, "footprint id changed");
        assert_eq!(f.position.x, *x, "x not restored for {}", f.id);
        assert_eq!(f.position.y, *y, "y not restored for {}", f.id);
    }
    Ok(())
}

#[test]
fn plan_shows_locked_parts_will_not_move() -> Result<(), String> {
    let mut s = Session::default();
    s.project.auto_sync = false;
    let _ = make_overlapping(&mut s)?;
    // Lock the first footprint
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
    // Plan should not propose moving the locked part
    let result = run(
        &mut s,
        "layout.legalize.plan",
        json!({"scope": {"layers": ["F.Cu"]}}),
    )?;
    let Some(moves) = result["result"]["moves"].as_array() else {
        return Err("plan result has no moves array".to_string());
    };
    for move_obj in moves {
        let Some(move_id) = move_obj["id"].as_u64() else {
            return Err("move object has no id".to_string());
        };
        assert_ne!(move_id, locked_id, "locked part should not be in plan");
    }
    Ok(())
}
