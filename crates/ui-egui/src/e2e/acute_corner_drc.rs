//! Stories for the corner checks in Checks: an acute corner between joined tracks and an arc
//! tighter than the net class allows (issue #918).
//!
//! A newcomer draws wires, presses Run checks in the Checks sheet and reads the finding in
//! plain words. Rounding the corner makes the finding go away.
//!
//! # Dependencies
//! - crates/rules/src/acute.rs
//! - crates/rules/src/track_radius.rs
//! - crates/engine/src/lib.rs
use super::super::*;
use super::support::*;

/// A board with one net and two joined tracks that meet at 45 degrees, plus the open Checks sheet.
fn sharp_corner() -> Result<(egui::Context, App, [u64; 2]), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.run("board.resize", json!({"width":100.0,"height":80.0}));
    app.run("net.create", json!({"name":"SIG"}));
    let net = app.project.nets.last().ok_or("the net was made")?.id;
    let mut ids = [0u64; 2];
    for (slot, (from, to)) in [((30.0, 30.0), (40.0, 30.0)), ((40.0, 30.0), (35.0, 35.0))]
        .into_iter()
        .enumerate()
    {
        app.run(
            "track.add",
            json!({"net":net,"start":{"x":from.0,"y":from.1},"end":{"x":to.0,"y":to.1},"width":0.25}),
        );
        ids[slot] = app
            .project
            .board
            .tracks
            .last()
            .map(|t| t.id)
            .ok_or_else(|| format!("the track was added: {}", app.message))?;
    }
    app.change_tab(Tab::Pcb);
    let ctx = egui::Context::default();
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    authoring::tests::open_page(&ctx, &mut app, "Checks")?;
    Ok((ctx, app, ids))
}

fn run_checks(ctx: &egui::Context, app: &mut App) -> Result<(), String> {
    tap_id(ctx, app, "button:Run checks")?;
    for _ in 0..3 {
        frame(ctx, app, vec![], egui::Modifiers::NONE);
    }
    Ok(())
}

fn codes(app: &App) -> Vec<String> {
    app.checks
        .get("drc")
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .filter_map(|f| f.get("code").and_then(Value::as_str))
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn a_newcomer_sees_a_warning_on_a_sharp_corner_and_fixes_it_by_rounding() -> Result<(), String> {
    let (ctx, mut app, ids) = sharp_corner()?;
    run_checks(&ctx, &mut app)?;
    assert!(
        codes(&app).iter().any(|c| c == "acute_angle"),
        "a 45 degree join is reported: {:?}",
        codes(&app)
    );
    assert!(
        paints_text(&ctx, &mut app, "easier to etch")?,
        "the row says in plain words why the corner is a problem: {}",
        app.message
    );
    assert!(
        app.tool_call("track.round_corners", json!({"ids": ids, "radius": 1.0}),),
        "rounding the corner works: {}",
        app.message
    );
    run_checks(&ctx, &mut app)?;
    assert!(
        !codes(&app).iter().any(|c| c == "acute_angle"),
        "the warning is gone once the corner is rounded: {:?}",
        codes(&app)
    );
    assert!(
        !paints_text(&ctx, &mut app, "easier to etch")?,
        "the row no longer shows"
    );
    Ok(())
}

#[test]
fn a_newcomer_is_told_when_an_arc_is_tighter_than_the_net_class_allows() -> Result<(), String> {
    let (ctx, mut app, ids) = sharp_corner()?;
    let net = app.project.nets.last().ok_or("the net exists")?.id;
    let class = app.run_and_keep(
        "netclass.add",
        json!({"name":"USB","clearance":0.2,"width":0.25,"nets":[net]}),
    );
    let class = class["id"].as_u64().ok_or("the class has an id")?;
    assert!(app.tool_call(
        "track.set_arc",
        json!({"id": ids[0], "mid": {"x": 35.0, "y": 30.5}}),
    ));
    app.run(
        "netclass.radius_rules",
        json!({"class": class, "min_radius": 30.0}),
    );
    run_checks(&ctx, &mut app)?;
    assert!(
        codes(&app).iter().any(|c| c == "track_radius"),
        "the tight arc is reported: {:?}",
        codes(&app)
    );
    assert!(
        paints_text(&ctx, &mut app, "arc radius")?,
        "the row names the limit: {}",
        app.message
    );
    app.run(
        "netclass.radius_rules",
        json!({"class": class, "min_radius": null}),
    );
    run_checks(&ctx, &mut app)?;
    assert!(
        !codes(&app).iter().any(|c| c == "track_radius"),
        "clearing the limit clears the finding"
    );
    Ok(())
}
