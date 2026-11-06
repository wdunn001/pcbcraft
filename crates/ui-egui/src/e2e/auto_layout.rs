use super::super::*;
use super::support::*;

#[test]
fn a_newcomer_previews_and_applies_automatic_routing_for_a_net() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    app.change_tab(Tab::Pcb);

    // Find a net that has open connections
    let nets = app.project.nets.clone();
    assert!(!nets.is_empty(), "sample design should have nets");

    // Build the board with open connections
    app.tool_call("board.sync", json!({}));
    app.refresh();

    // Select nothing to show the auto layout section
    app.select(None);
    app.refresh();

    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);

    // The auto layout section should be visible when nothing is selected
    assert!(
        scrolled_to(&ctx, &mut app, "Auto layout")?,
        "the Auto layout section appears when nothing is selected: {}",
        app.message
    );

    // Select board scope
    tap_properties(&ctx, &mut app, "button:Board")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);

    // Click Preview
    tap_properties(&ctx, &mut app, "button:Preview")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);

    // Check that preview result is shown
    assert!(
        app.route_preview.is_some(),
        "preview should have run and stored results"
    );

    // Click Apply
    tap_properties(&ctx, &mut app, "button:Apply")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);

    assert!(app.route_applied, "routing should have been applied");

    Ok(())
}
