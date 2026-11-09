use super::super::*;
use super::support::*;

#[test]
fn a_newcomer_backdrills_a_via_and_sees_it_in_the_drill_table() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    // A six-layer board: the stack-up gives the stub something to measure.
    for n in 0..6 {
        let name = match n {
            0 => "F.Cu".to_string(),
            5 => "B.Cu".to_string(),
            k => format!("In{k}.Cu"),
        };
        app.tool_call(
            "board.set_layer_record",
            json!({"name": name, "kind": "copper", "thickness": 0.035}),
        );
        if n < 5 {
            app.tool_call(
                "board.set_layer_record",
                json!({"name": format!("Prepreg{n}"), "kind": "prepreg", "thickness": 0.2}),
            );
        }
    }
    let net = app
        .project
        .nets
        .first()
        .map(|n| n.id)
        .ok_or("the practice design should have a net")?;
    app.tool_call(
        "via.place",
        json!({"net": net, "position": {"x": 12.0, "y": 12.0}, "diameter": 0.7, "drill": 0.3}),
    );
    let via = app
        .project
        .board
        .vias
        .last()
        .map(|v| v.id)
        .ok_or("the via should be on the board")?;
    app.select(Some(via));
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        scrolled_to(&ctx, &mut app, "Back-drill")?,
        "a chosen via offers a back-drill section: {}",
        app.message
    );
    authoring::tests::tap(&ctx, &mut app, "Off")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "From the bottom")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.refresh();
    let spec = app
        .project
        .board
        .vias
        .iter()
        .find(|v| v.id == via)
        .and_then(|v| v.backdrill)
        .ok_or_else(|| format!("choosing a start records a back-drill: {}", app.message))?;
    assert_eq!(spec.from, pcbcraft_doc::BackDrillFrom::Bottom);
    assert!(
        spec.diameter > 0.3,
        "the new drill is larger than the via hole"
    );
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        scrolled_to(&ctx, &mut app, "Stub left")?,
        "the pane says how much stub is left: {}",
        app.message
    );
    tap_id(&ctx, &mut app, "tab:Manufacture")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        paints_text(&ctx, &mut app, "Back-drill")?,
        "the drill table lists the back-drill: {}",
        app.message
    );
    let rows = pcbcraft_io::drill_table(&app.project)?;
    assert!(
        rows.iter()
            .any(|r| r.function == pcbcraft_io::DrillFunction::BackDrill && r.count == 1),
        "one back-drill row: {rows:?}"
    );
    Ok(())
}
