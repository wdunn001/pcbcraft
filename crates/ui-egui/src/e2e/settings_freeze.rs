//! Stories for changing Project settings on a big design: every setting must leave the
//! window answering (#1672, follow up of #1634). The fixture is the showcase board.
//!
//! Each step runs the engine command a control of the dialog runs, then draws two frames
//! with the dialog open on the matching tab.

use super::super::*;
use super::support::*;
use crate::project_settings as ps;

fn showcase() -> Result<(egui::Context, App), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.home_open = true;
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Showcase board")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    Ok((ctx, app))
}

fn ms(start: &pcbcraft_clock::Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

/// The most one step may take, in milliseconds.
const STEP_BUDGET_MS: f64 = 6000.0;

/// One measured step: the setting, the command time and the slowest next frame.
struct Row {
    setting: String,
    command: f64,
    frame: f64,
    refused: bool,
    /// Board checks that ran on the window thread during the step.
    on_frame: u64,
    /// Whether the footer said Checking when the command returned.
    checking: bool,
    /// Time from the command until the new findings were on screen, in milliseconds. Before
    /// this fix the window was blocked for all of it.
    settled: f64,
    /// Whether the findings on screen after the run equal a full run.
    exact: bool,
}

fn runs(app: &App) -> u64 {
    app.session
        .lock()
        .map(|s| s.board_check_runs())
        .unwrap_or_default()
}

/// The findings and airwires of a session with no memory of earlier runs.
fn full_run(app: &App) -> Value {
    let mut other = pcbcraft_engine::Session::default();
    if other.replace(app.project.clone()).is_err() {
        return Value::Null;
    }
    let checks = other.checks();
    json!([checks["drc"], checks["airwires"]])
}

fn step(
    ctx: &egui::Context,
    app: &mut App,
    rows: &mut Vec<Row>,
    setting: &str,
    command: &str,
    params: Value,
) {
    let before = app.revision;
    let runs_before = runs(app);
    let start = pcbcraft_clock::instant_now();
    app.settings_run(command, params);
    let command_ms = ms(&start);
    let checking = app.checking();
    let mut slowest = 0.0_f64;
    for _ in 0..2 {
        let start = pcbcraft_clock::instant_now();
        frame(ctx, app, vec![], egui::Modifiers::NONE);
        slowest = slowest.max(ms(&start));
    }
    let on_frame = runs(app) - runs_before;
    app.finish_board_check();
    let settled = ms(&start);
    let exact = json!([app.checks["drc"], app.checks["airwires"]]) == full_run(app);
    rows.push(Row {
        setting: setting.to_owned(),
        command: command_ms,
        frame: slowest,
        refused: app.revision == before,
        on_frame,
        checking,
        settled,
        exact,
    });
}

fn open_tab(ctx: &egui::Context, app: &mut App, tab: usize) {
    app.open_project_settings(tab);
    frame(ctx, app, vec![], egui::Modifiers::NONE);
}

/// Every command the dialog can run, once, with a value that differs from the current one.
fn measure() -> Result<Vec<Row>, String> {
    let (ctx, mut app) = showcase()?;
    let mut rows = Vec::new();
    let r = &mut rows;
    let width = app.project.board.width;
    let height = app.project.board.height;
    open_tab(&ctx, &mut app, ps::TAB_BOARD);
    step(
        &ctx,
        &mut app,
        r,
        "board width",
        "board.resize",
        json!({"width": width + 1.0, "height": height}),
    );
    step(
        &ctx,
        &mut app,
        r,
        "board height",
        "board.resize",
        json!({"width": width + 1.0, "height": height + 1.0}),
    );
    step(
        &ctx,
        &mut app,
        r,
        "surface finish",
        "board.set_finish",
        json!({"finish": "ENIG"}),
    );
    step(
        &ctx,
        &mut app,
        r,
        "dielectric constant",
        "board.set_stackup",
        json!({"er": 4.3}),
    );
    step(
        &ctx,
        &mut app,
        r,
        "edge plating",
        "board.set_machining",
        json!({"edge_plating": true}),
    );
    step(
        &ctx,
        &mut app,
        r,
        "board thickness",
        "board.set_stackup",
        json!({"thickness": 1.2}),
    );
    step(
        &ctx,
        &mut app,
        r,
        "layer record add",
        "board.set_layer_record",
        json!({"name": "Core 9", "kind": "core", "thickness": 0.2}),
    );
    step(
        &ctx,
        &mut app,
        r,
        "layer record dk",
        "board.set_layer_record",
        json!({"name": "Core 9", "kind": "core", "thickness": 0.2, "dk": 4.1}),
    );
    step(
        &ctx,
        &mut app,
        r,
        "layer record remove",
        "board.remove_layer_record",
        json!({"name": "Core 9"}),
    );
    step(
        &ctx,
        &mut app,
        r,
        "add copper pair",
        "board.stackup_add_pair",
        json!({"after": "F.Cu"}),
    );
    open_tab(&ctx, &mut app, ps::TAB_RULES);
    step(
        &ctx,
        &mut app,
        r,
        "default clearance",
        "board.rules.set",
        json!({"clearance": 0.2}),
    );
    step(
        &ctx,
        &mut app,
        r,
        "fab min track",
        "board.rules.set",
        json!({"min_track": 0.12}),
    );
    step(
        &ctx,
        &mut app,
        r,
        "fab min drill",
        "board.rules.set",
        json!({"min_drill": 0.25}),
    );
    step(
        &ctx,
        &mut app,
        r,
        "fab silk line",
        "board.rules.set",
        json!({"silk_line": 0.18}),
    );
    step(
        &ctx,
        &mut app,
        r,
        "fab preset",
        "rules.preset_apply",
        json!({"name": "JLCPCB 1-2L"}),
    );
    step(
        &ctx,
        &mut app,
        r,
        "severity clearance",
        "rules.severity",
        json!({"code": "clearance", "severity": "warning"}),
    );
    step(
        &ctx,
        &mut app,
        r,
        "severity unrouted",
        "rules.severity",
        json!({"code": "unrouted", "severity": "off"}),
    );
    step(
        &ctx,
        &mut app,
        r,
        "custom rule add",
        "checks.custom_add",
        json!({"name": "Narrow", "measure": "track_width", "limit": 0.2, "severity": "warning"}),
    );
    open_tab(&ctx, &mut app, ps::TAB_CLASSES);
    step(
        &ctx,
        &mut app,
        r,
        "net class add",
        "netclass.add",
        json!({"name": "Power", "clearance": 0.2, "width": 0.4}),
    );
    step(
        &ctx,
        &mut app,
        r,
        "net class add 2",
        "netclass.add",
        json!({"name": "Signal", "clearance": 0.15, "width": 0.2}),
    );
    let class = app
        .project
        .net_classes
        .first()
        .map(|c| c.id)
        .ok_or("no class")?;
    let other = app
        .project
        .net_classes
        .get(1)
        .map(|c| c.id)
        .ok_or("no second class")?;
    let net = app.project.nets.first().map(|n| n.id).ok_or("no net")?;
    step(
        &ctx,
        &mut app,
        r,
        "net class name",
        "netclass.set",
        json!({"class": class, "name": "Rails"}),
    );
    step(
        &ctx,
        &mut app,
        r,
        "net class clearance",
        "netclass.clearance",
        json!({"class": class, "clearance": 0.25}),
    );
    step(
        &ctx,
        &mut app,
        r,
        "net class width",
        "netclass.set",
        json!({"class": class, "width": 0.5}),
    );
    step(
        &ctx,
        &mut app,
        r,
        "net class via diameter",
        "netclass.set",
        json!({"class": class, "via_diameter": 0.9}),
    );
    step(
        &ctx,
        &mut app,
        r,
        "net class via drill",
        "netclass.set",
        json!({"class": class, "via_drill": 0.45}),
    );
    step(
        &ctx,
        &mut app,
        r,
        "net class pair width",
        "netclass.set",
        json!({"class": class, "pair_width": 0.25}),
    );
    step(
        &ctx,
        &mut app,
        r,
        "net class pair gap",
        "netclass.set",
        json!({"class": class, "pair_gap": 0.25}),
    );
    step(
        &ctx,
        &mut app,
        r,
        "net class rise time",
        "netclass.rise_time",
        json!({"class": class, "rise_time": 1e-9}),
    );
    step(
        &ctx,
        &mut app,
        r,
        "net class impedance",
        "netclass.set",
        json!({"class": class, "impedance_target": 50.0}),
    );
    step(
        &ctx,
        &mut app,
        r,
        "net class tolerance",
        "netclass.set",
        json!({"class": class, "impedance_tolerance_pct": 10.0}),
    );
    step(
        &ctx,
        &mut app,
        r,
        "net class assign net",
        "netclass.assign",
        json!({"net": net, "class": class}),
    );
    step(
        &ctx,
        &mut app,
        r,
        "net class release net",
        "netclass.assign",
        json!({"net": net, "class": null}),
    );
    step(
        &ctx,
        &mut app,
        r,
        "class pair gap",
        "rules.class_pair_set",
        json!({"class_a": class, "class_b": other, "clearance": 0.3}),
    );
    step(
        &ctx,
        &mut app,
        r,
        "class pair remove",
        "rules.class_pair_remove",
        json!({"class_a": class, "class_b": other}),
    );
    step(
        &ctx,
        &mut app,
        r,
        "net class remove",
        "netclass.remove",
        json!({"class": other}),
    );
    open_tab(&ctx, &mut app, ps::TAB_ELECTRICAL);
    step(
        &ctx,
        &mut app,
        r,
        "pin matrix cell",
        "erc.matrix_set",
        json!({"type_a": "output", "type_b": "output", "level": "warning"}),
    );
    step(
        &ctx,
        &mut app,
        r,
        "pin matrix reset",
        "erc.matrix_reset",
        json!({}),
    );
    open_tab(&ctx, &mut app, ps::TAB_MACHINES);
    app.machine_lib_refresh();
    let machine = app
        .libws
        .machines
        .cards
        .iter()
        .filter_map(|c| c.get("id").and_then(Value::as_str))
        .next()
        .map(str::to_owned)
        .ok_or("no library machine")?;
    step(
        &ctx,
        &mut app,
        r,
        "machine install",
        "machine.install",
        json!({"id": machine}),
    );
    let mut profile = app
        .project
        .machine_profiles
        .first()
        .and_then(|p| serde_json::to_value(p).ok())
        .ok_or("no profile")?;
    if let Some(map) = profile.as_object_mut() {
        map.insert("notes".into(), json!("measured"));
    }
    step(
        &ctx,
        &mut app,
        r,
        "machine notes",
        "machine.set",
        json!({"profile": profile}),
    );
    step(
        &ctx,
        &mut app,
        r,
        "machine remove",
        "machine.remove",
        json!({"id": machine}),
    );
    open_tab(&ctx, &mut app, ps::TAB_TITLE);
    step(
        &ctx,
        &mut app,
        r,
        "title",
        "project.set_meta",
        json!({"key": "title", "value": "Showcase"}),
    );
    step(
        &ctx,
        &mut app,
        r,
        "company",
        "project.set_meta",
        json!({"key": "company", "value": "Acme"}),
    );
    step(
        &ctx,
        &mut app,
        r,
        "paper",
        "project.set_meta",
        json!({"key": "paper", "value": "A3"}),
    );
    step(
        &ctx,
        &mut app,
        r,
        "clear title",
        "project.clear_meta",
        json!({"key": "title"}),
    );
    Ok(rows)
}

#[test]
fn every_project_setting_answers_within_a_frame_budget() -> Result<(), String> {
    let rows = measure()?;
    let mut slow = Vec::new();
    println!("setting | command ms | frame ms");
    for row in &rows {
        println!(
            "TABLE {} | {:.0} | {:.0} | settled {:.0} | on frame {} | checking {} | exact {}{}",
            row.setting,
            row.command,
            row.frame,
            row.settled,
            row.on_frame,
            row.checking,
            row.exact,
            if row.refused { " (refused)" } else { "" }
        );
        if row.command > STEP_BUDGET_MS || row.frame > STEP_BUDGET_MS {
            slow.push(format!("{} took too long", row.setting));
        }
        if row.on_frame > 0 {
            slow.push(format!("{} ran the board checks on the frame", row.setting));
        }
        if !row.exact {
            slow.push(format!(
                "{} shows findings that differ from a full run",
                row.setting
            ));
        }
    }
    assert!(slow.is_empty(), "{slow:?}");
    Ok(())
}

#[test]
fn a_title_block_edit_does_not_start_the_board_checks() -> Result<(), String> {
    let (ctx, mut app) = showcase()?;
    open_tab(&ctx, &mut app, ps::TAB_TITLE);
    let mut rows = Vec::new();
    step(
        &ctx,
        &mut app,
        &mut rows,
        "title",
        "project.set_meta",
        json!({"key": "title", "value": "Demo"}),
    );
    let row = rows.first().ok_or("no step ran")?;
    assert!(!row.checking, "nothing to wait for");
    assert!(row.exact);
    Ok(())
}

#[test]
fn a_design_rule_edit_keeps_the_old_findings_and_says_checking() -> Result<(), String> {
    let (ctx, mut app) = showcase()?;
    open_tab(&ctx, &mut app, ps::TAB_RULES);
    let known = app.checks["drc"]
        .as_array()
        .map(Vec::len)
        .ok_or("no findings")?;
    app.settings_run("board.rules.set", json!({"clearance": 0.9}));
    assert!(app.checking(), "the footer says Checking");
    let kept = app.checks["drc"]
        .as_array()
        .map(Vec::len)
        .ok_or("findings were dropped")?;
    assert_eq!(kept, known, "the old findings stay visible meanwhile");
    let output = frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let says = output.shapes.iter().any(|shape| {
        matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text().contains("Checking"))
    });
    assert!(says, "the footer paints Checking");
    app.finish_board_check();
    assert!(!app.checking());
    assert!(
        app.checks["drc"].as_array().map(Vec::len) != Some(known),
        "a wider clearance changes the findings"
    );
    Ok(())
}
