//! The checks run after every edit. A schematic page edit cannot change a copper finding, so a
//! large board must not be poured and measured again for it (#1634, #1635).

use pcbcraft_engine::Session;
use serde_json::{Value, json};

fn showcase() -> Result<Session, String> {
    let project = pcbcraft_io::load(std::path::Path::new("../../examples/showcase.pcbcraft"))?;
    let mut s = Session::default();
    s.replace(project).map_err(|e| e.to_string())?;
    Ok(s)
}

/// The checks as a session with no memory of earlier runs reports them.
fn fresh(s: &Session) -> Result<Value, String> {
    let mut other = Session::default();
    other
        .replace(s.project.clone())
        .map_err(|e| e.to_string())?;
    let mut checks = other.checks();
    checks["revision"] = json!(0);
    Ok(checks)
}

fn without_revision(mut checks: Value) -> Value {
    checks["revision"] = json!(0);
    checks
}

#[test]
fn adding_a_page_does_not_run_the_copper_checks_again() -> Result<(), String> {
    let mut s = showcase()?;
    let first = s.checks();
    assert!(
        first["drc"].is_array(),
        "the checks carry the copper findings"
    );
    for n in 0..3 {
        s.execute(
            "schematic.new_sheet",
            json!({"name": format!("Extra {n}")}),
            None,
        )
        .map_err(|e| e.to_string())?;
        let start = pcbcraft_clock::instant_now();
        let again = s.checks();
        let took = start.elapsed().as_secs_f64() * 1000.0;
        assert!(
            took < 400.0,
            "the checks after adding a page took {took:.0} ms; they took seconds when every pour was made again"
        );
        assert_eq!(
            without_revision(again),
            fresh(&s)?,
            "the reused results equal a full run"
        );
    }
    Ok(())
}

#[test]
fn a_board_edit_still_changes_the_copper_findings() -> Result<(), String> {
    let mut s = showcase()?;
    let before = s.checks();
    let footprint = s
        .project
        .board
        .footprints
        .first()
        .ok_or("the showcase has no footprint")?
        .id;
    s.execute(
        "footprint.move",
        json!({"id": footprint, "position": {"x": 1.0, "y": 1.0}}),
        None,
    )
    .map_err(|e| e.to_string())?;
    let after = s.checks();
    assert_ne!(
        before["drc"], after["drc"],
        "moving a part onto the board edge changes the findings"
    );
    assert_eq!(without_revision(after), fresh(&s)?);
    Ok(())
}
