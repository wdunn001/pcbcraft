//! A project settings change recomputes only what the setting can change (#1672).
//!
//! Each test changes one group of settings on the showcase board and compares the checks the
//! session reports, which may reuse the last run, with the checks of a session that has no
//! memory of earlier runs.

use pcbcraft_engine::Session;
use serde_json::{Value, json};

fn showcase() -> Result<Session, String> {
    let project = pcbcraft_io::load(std::path::Path::new("../../examples/showcase.pcbcraft"))?;
    let mut s = Session::default();
    s.replace(project).map_err(|e| e.to_string())?;
    let _ = s.checks();
    Ok(s)
}

fn without_revision(mut checks: Value) -> Value {
    checks["revision"] = json!(0);
    checks
}

/// The checks as a session with no memory of earlier runs reports them.
fn fresh(s: &Session) -> Result<Value, String> {
    let mut other = Session::default();
    other
        .replace(s.project.clone())
        .map_err(|e| e.to_string())?;
    Ok(without_revision(other.checks()))
}

/// Run each command, then check the reported checks equal a full run. Returns how many times
/// the board checks ran from scratch over all of them.
fn run_all(s: &mut Session, commands: &[(&str, Value)]) -> Result<u64, String> {
    let before = s.board_check_runs();
    for (command, params) in commands {
        s.execute(command, params.clone(), None)
            .map_err(|e| format!("{command}: {e}"))?;
        assert_eq!(
            without_revision(s.checks()),
            fresh(s)?,
            "{command}: the reported checks equal a full run"
        );
    }
    Ok(s.board_check_runs() - before)
}

#[test]
fn title_block_edits_do_not_run_the_board_checks_again() -> Result<(), String> {
    let mut s = showcase()?;
    let runs = run_all(
        &mut s,
        &[
            ("project.set_meta", json!({"key": "title", "value": "Demo"})),
            (
                "project.set_meta",
                json!({"key": "company", "value": "Acme"}),
            ),
            ("project.set_meta", json!({"key": "paper", "value": "A3"})),
            ("project.clear_meta", json!({"key": "title"})),
        ],
    )?;
    assert_eq!(runs, 0, "a title block entry cannot change a board finding");
    Ok(())
}

#[test]
fn pin_matrix_edits_do_not_run_the_board_checks_again() -> Result<(), String> {
    let mut s = showcase()?;
    let runs = run_all(
        &mut s,
        &[
            (
                "erc.matrix_set",
                json!({"type_a": "output", "type_b": "output", "level": "warning"}),
            ),
            ("erc.matrix_reset", json!({})),
        ],
    )?;
    assert_eq!(
        runs, 0,
        "the pin matrix is read by the electrical rules only"
    );
    Ok(())
}

#[test]
fn rule_and_fab_limit_edits_give_the_full_run_result() -> Result<(), String> {
    let mut s = showcase()?;
    let runs = run_all(
        &mut s,
        &[
            ("board.rules.set", json!({"clearance": 0.4})),
            ("board.rules.set", json!({"min_space": 0.5})),
            ("board.rules.set", json!({"min_track": 0.3})),
            ("rules.preset_apply", json!({"name": "JLCPCB 1-2L"})),
            (
                "rules.severity",
                json!({"code": "clearance", "severity": "warning"}),
            ),
            (
                "checks.custom_add",
                json!({"name": "Narrow", "measure": "track width below", "limit": 0.6, "severity": "warning"}),
            ),
        ],
    )?;
    assert_eq!(runs, 6, "each rule edit runs the checks once");
    Ok(())
}

#[test]
fn net_class_edits_give_the_full_run_result() -> Result<(), String> {
    let mut s = showcase()?;
    let net = s.project.nets.first().ok_or("no net")?.id;
    run_all(
        &mut s,
        &[
            (
                "netclass.add",
                json!({"name": "Rails", "clearance": 0.5, "width": 0.4}),
            ),
            (
                "netclass.add",
                json!({"name": "Fast", "clearance": 0.15, "width": 0.2}),
            ),
        ],
    )?;
    let class = s.project.net_classes.first().ok_or("no class")?.id;
    let other = s.project.net_classes.get(1).ok_or("no second class")?.id;
    run_all(
        &mut s,
        &[
            ("netclass.set", json!({"class": class, "width": 0.5})),
            (
                "netclass.clearance",
                json!({"class": class, "clearance": 0.6}),
            ),
            ("netclass.set", json!({"class": class, "via_diameter": 0.9})),
            (
                "netclass.rise_time",
                json!({"class": class, "rise_time": 1e-9}),
            ),
            (
                "netclass.set",
                json!({"class": class, "impedance_target": 50.0}),
            ),
            ("netclass.assign", json!({"net": net, "class": class})),
            (
                "rules.class_pair_set",
                json!({"class_a": class, "class_b": other, "clearance": 0.8}),
            ),
            ("netclass.assign", json!({"net": net, "class": null})),
            ("netclass.remove", json!({"class": other})),
        ],
    )?;
    Ok(())
}

#[test]
fn board_and_stackup_edits_give_the_full_run_result() -> Result<(), String> {
    let mut s = showcase()?;
    let width = s.project.board.width;
    let height = s.project.board.height;
    run_all(
        &mut s,
        &[
            (
                "board.resize",
                json!({"width": width + 3.0, "height": height + 2.0}),
            ),
            ("board.set_stackup", json!({"er": 4.2})),
            ("board.set_stackup", json!({"thickness": 1.2})),
            ("board.set_finish", json!({"finish": "OSP"})),
            ("board.set_machining", json!({"edge_plating": true})),
            ("board.stackup_add_pair", json!({"after": "F.Cu"})),
        ],
    )?;
    Ok(())
}

#[test]
fn machine_edits_give_the_full_run_result() -> Result<(), String> {
    let mut s = showcase()?;
    let listed = s
        .execute(
            "machine.library_list",
            json!({"include_hidden": true}),
            None,
        )
        .map_err(|e| e.to_string())?;
    let id = listed
        .pointer("/result/machines/0/id")
        .and_then(Value::as_str)
        .ok_or("no library machine")?
        .to_owned();
    run_all(
        &mut s,
        &[
            ("machine.install", json!({"id": id})),
            ("machine.remove", json!({"id": id})),
        ],
    )?;
    Ok(())
}
