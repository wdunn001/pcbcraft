//! Impedance-target findings in the checks sheet.

use super::super::*;
use super::support::*;

#[test]
fn a_newcomer_sees_a_finding_when_the_usb_pair_impedance_misses_the_target() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.run("project.set_auto_sync", json!({"auto_sync": false}));
    for (reference, y) in [("R1", 20.0), ("R2", 34.0)] {
        let x = if reference == "R1" { 10.0 } else { 50.0 };
        app.run(
            "component.add",
            json!({"kind":"resistor","reference":reference,"value":"90 ohm","position":{"x":x,"y":y}}),
        );
    }
    for pin in ["1", "2"] {
        app.run(
            "wire.connect",
            json!({"from":{"component":1,"pin":pin},"to":{"component":2,"pin":pin}}),
        );
    }
    app.run("board.sync", json!({}));
    let nets: Vec<Id> = app
        .project
        .components
        .first()
        .ok_or("parts exist")?
        .pins
        .iter()
        .filter_map(|p| p.net)
        .collect();
    let (net_p, net_n) = (
        *nets.first().ok_or("positive net")?,
        *nets.get(1).ok_or("negative net")?,
    );
    app.run(
        "board.set_diffpair",
        json!({"name":"USB","net_p":net_p,"net_n":net_n,"gap":0.15}),
    );
    app.run(
        "net.set_impedance",
        json!({"id":net_p,"target":90.0,"tolerance_pct":10.0}),
    );
    let width = 0.25;
    for (net, y) in [(net_p, 30.0), (net_n, 30.0 + width + 0.15)] {
        app.run(
            "track.add",
            json!({"net":net,"start":{"x":12.0,"y":y},"end":{"x":48.0,"y":y},"width":width,"layer":"F.Cu"}),
        );
    }
    app.refresh();
    assert!(
        pcbcraft_rules::drc(&app.project)
            .iter()
            .any(|f| f.code == "impedance_target"),
        "tight coupling misses a 90 ohm USB target: {}",
        app.message
    );
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::open_page(&ctx, &mut app, "Checks")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let sample = pcbcraft_rules::drc(&app.project)
        .into_iter()
        .find(|f| f.code == "impedance_target")
        .ok_or("impedance finding exists")?
        .message;
    // The sheet is short and lists the louder findings first: scroll it to the row.
    assert!(
        scrolled_to_at(&ctx, &mut app, &sample, 700.0)?,
        "the checks sheet shows the impedance finding: {sample}"
    );
    Ok(())
}

/// Issue #1021: the Results tab gains a Signal integrity area that lists only
/// the signal integrity findings, separate from the board clearance checks.
#[test]
fn a_newcomer_lists_only_the_signal_integrity_findings() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.run("project.set_auto_sync", json!({"auto_sync": false}));
    for (reference, y) in [("R1", 20.0), ("R2", 34.0)] {
        let x = if reference == "R1" { 10.0 } else { 50.0 };
        app.run(
            "component.add",
            json!({"kind":"resistor","reference":reference,"value":"90 ohm","position":{"x":x,"y":y}}),
        );
    }
    for pin in ["1", "2"] {
        app.run(
            "wire.connect",
            json!({"from":{"component":1,"pin":pin},"to":{"component":2,"pin":pin}}),
        );
    }
    app.run("board.sync", json!({}));
    let nets: Vec<Id> = app
        .project
        .components
        .first()
        .ok_or("parts exist")?
        .pins
        .iter()
        .filter_map(|p| p.net)
        .collect();
    let (net_p, net_n) = (
        *nets.first().ok_or("positive net")?,
        *nets.get(1).ok_or("negative net")?,
    );
    app.run(
        "board.set_diffpair",
        json!({"name":"USB","net_p":net_p,"net_n":net_n,"gap":0.15}),
    );
    app.run(
        "net.set_impedance",
        json!({"id":net_p,"target":90.0,"tolerance_pct":10.0}),
    );
    let width = 0.25;
    for (net, y) in [(net_p, 30.0), (net_n, 30.0 + width + 0.15)] {
        app.run(
            "track.add",
            json!({"net":net,"start":{"x":12.0,"y":y},"end":{"x":48.0,"y":y},"width":width,"layer":"F.Cu"}),
        );
    }
    // A separate clearance violation on two foreign tracks 0.05 mm apart.
    let other = app.project.nets.first().ok_or("a net exists")?.id;
    app.run(
        "track.add",
        json!({"net":other,"start":{"x":5.0,"y":5.0},"end":{"x":10.0,"y":5.0},"width":0.2,"layer":"F.Cu"}),
    );
    app.run(
        "track.add",
        json!({"net":other,"start":{"x":5.0,"y":5.05},"end":{"x":10.0,"y":5.05},"width":0.2,"layer":"F.Cu"}),
    );
    app.refresh();
    assert!(
        pcbcraft_rules::drc(&app.project)
            .iter()
            .any(|f| f.code == "impedance_target"),
        "tight coupling misses a 90 ohm USB target: {}",
        app.message
    );
    let has_clearance = pcbcraft_rules::drc(&app.project)
        .iter()
        .any(|f| f.code == "clearance");
    assert!(
        has_clearance,
        "the close tracks make a clearance finding: {}",
        app.message
    );
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    // Open the Rules and checks dialog from the Edit menu.
    authoring::tests::tap(&ctx, &mut app, "Edit")?;
    authoring::tests::tap(&ctx, &mut app, "Rules and checks…")?;
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    assert!(app.rules_dialog.open, "the dialog is open");
    // Run the checks.
    tap_id(&ctx, &mut app, "button:Run checks")?;
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    // Pick the Signal integrity area; the list then holds only its findings.
    tap_dialog_segment(&ctx, &mut app, "All areas", 3, 5)?;
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    assert!(
        paints_text(&ctx, &mut app, "Impedance target")?,
        "the signal integrity area shows the impedance finding: {}",
        app.message
    );
    // The clearance finding is not an SI finding; its own sentence must not appear.
    let clearance_text = pcbcraft_rules::drc(&app.project)
        .iter()
        .find(|f| f.code == "clearance")
        .map(|f| f.message.clone())
        .ok_or("the board has a clearance finding")?;
    assert!(
        !paints_text(&ctx, &mut app, &clearance_text)?,
        "the clearance finding is not in the signal integrity area: {clearance_text}"
    );
    Ok(())
}
