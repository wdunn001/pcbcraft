//! Stories for the in-app datasheet text layer.

use super::support::*;
use pcbcraft_datasheet::fixtures;

#[test]
fn a_newcomer_searches_a_datasheet_and_copies_text_in_order() -> Result<(), String> {
    let f = fixtures::all()
        .into_iter()
        .find(|f| f.name == "text-one-page.pdf")
        .ok_or("fixture pdf")?;
    let ctx = egui::Context::default();
    let mut app = fresh();
    app.datasheet_pane
        .open_fixture(&ctx, f.bytes)
        .map_err(|e| e.to_string())?;
    app.datasheet_pane.search_in_ui("LM317");
    assert_eq!(
        app.datasheet_pane.match_count(),
        1,
        "search should find the part name on the page"
    );
    app.datasheet_pane.select_all_on_page();
    let copied = app.datasheet_pane.copied_selection();
    assert!(
        copied.contains("LM317") && copied.contains("TO-220"),
        "copy should keep reading order across the title and body: {copied}"
    );
    Ok(())
}

#[test]
fn a_newcomer_finds_matches_on_every_page() -> Result<(), String> {
    let f = fixtures::all()
        .into_iter()
        .find(|f| f.name == "two-pages.pdf")
        .ok_or("fixture pdf")?;
    let ctx = egui::Context::default();
    let mut app = fresh();
    app.datasheet_pane
        .open_fixture(&ctx, f.bytes)
        .map_err(|e| e.to_string())?;
    app.datasheet_pane.search_in_ui("Absolute");
    assert_eq!(
        app.datasheet_pane.match_count(),
        1,
        "search should reach the second page"
    );
    Ok(())
}

#[test]
fn a_newcomer_sees_when_a_page_has_no_text_layer() -> Result<(), String> {
    let f = fixtures::all()
        .into_iter()
        .find(|f| f.name == "ink-only.pdf")
        .ok_or("fixture pdf")?;
    let ctx = egui::Context::default();
    let mut app = fresh();
    app.datasheet_pane
        .open_fixture(&ctx, f.bytes)
        .map_err(|e| e.to_string())?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        app.datasheet_pane.page_shows_no_text_message(),
        "ink without glyphs should explain that there is no text layer"
    );
    assert!(
        paints_text(
            &ctx,
            &mut app,
            &pcbcraft_text::lookup("datasheet.pane.no_text", &[])
        )?,
        "the plain sentence should appear on screen"
    );
    Ok(())
}

#[test]
fn a_newcomer_opens_a_local_datasheet_from_properties() -> Result<(), String> {
    let f = fixtures::all()
        .into_iter()
        .find(|f| f.name == "text-one-page.pdf")
        .ok_or("fixture pdf")?;
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let path = dir.path().join("part.pdf");
    std::fs::write(&path, &f.bytes).map_err(|e| e.to_string())?;
    let url = format!("file:///{}", path.display().to_string().replace('\\', "/"));
    let mut app = fresh();
    app.tool_call("sample_design", json!({}));
    let resistor = app
        .project
        .components
        .iter()
        .find(|c| c.reference.starts_with('R'))
        .map(|c| c.id)
        .ok_or("the practice design carries a resistor")?;
    app.tool_call(
        "component.field",
        json!({"id": resistor, "key": "Datasheet", "value": url}),
    );
    app.select(Some(resistor));
    let ctx = egui::Context::default();
    settled(&ctx, &mut app);
    assert!(
        scrolled_to(&ctx, &mut app, "View datasheet")?,
        "the datasheet action stays visible"
    );
    let out = frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let at = centre_of(&out, "View datasheet", f32::MAX).ok_or("view datasheet button")?;
    let out = frame(
        &ctx,
        &mut app,
        vec![egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }],
        egui::Modifiers::NONE,
    );
    let opened_browser = out
        .platform_output
        .commands
        .iter()
        .any(|c| matches!(c, egui::OutputCommand::OpenUrl(_)));
    assert!(
        !opened_browser,
        "a local file should open in the app, not the browser"
    );
    assert!(
        app.datasheet_pane.is_open(),
        "the reader pane should be open"
    );
    Ok(())
}
