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
    assert!(
        paints_text(&ctx, &mut app, &sample)?,
        "the checks sheet shows the impedance finding: {sample}"
    );
    Ok(())
}
