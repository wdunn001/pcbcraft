use super::super::*;
use super::support::*;
use pcbcraft_datasheet::fixture_pdf;

#[test]
fn a_browser_user_renders_page_one_of_a_local_pdf() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.browser = true;
    app.refresh();
    let ctx = egui::Context::default();
    app.datasheet
        .test_show_pdf(&ctx, fixture_pdf::one_page_text())?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        paints_text(&ctx, &mut app, "Page 1")?,
        "a local PDF should render page 1: {}",
        app.message
    );
    Ok(())
}

#[test]
fn a_browser_user_sees_a_blocked_fetch_message_with_the_link() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.browser = true;
    app.refresh();
    let ctx = egui::Context::default();
    app.datasheet
        .open_for_url(&ctx, "https://example.com/part.pdf".into());
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        paints_text(&ctx, &mut app, "does not allow a download")?,
        "blocked fetch should explain itself: {}",
        app.message
    );
    assert!(
        paints_text(&ctx, &mut app, "https://example.com/part.pdf")?,
        "the link should stay visible: {}",
        app.message
    );
    Ok(())
}
