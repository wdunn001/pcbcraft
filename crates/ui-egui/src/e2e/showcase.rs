use super::super::*;
use super::support::*;

/// A newcomer picks the showcase board from the Start page and lands in a
/// finished design with its sheets, parts and routed copper.
#[test]
fn a_newcomer_opens_the_showcase_example() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.home_open = true;
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        paints_text(&ctx, &mut app, "Showcase board")?,
        "the start screen offers the finished sample board"
    );
    authoring::tests::tap(&ctx, &mut app, "Showcase board")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(!app.home_open, "picking the board leaves the start screen");
    assert_eq!(app.project.components.len(), 55, "every part arrives");
    assert_eq!(
        app.project.sheets.len() + 1,
        3,
        "the root page and two named pages arrive: {}",
        app.message
    );
    authoring::tests::tap(&ctx, &mut app, "Schematic")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    for page in ["Compute", "Sensors and indicators"] {
        assert!(
            app.project.sheets.iter().any(|s| s.name == page),
            "the design names the page {page}"
        );
    }
    assert!(
        !app.project.board.tracks.is_empty(),
        "the board comes routed"
    );
    Ok(())
}

/// A newcomer on the browser demo picks the showcase board from the Start page. The browser has
/// no file system, so the board opens from the text inside the program and the page keeps
/// running.
#[test]
fn a_newcomer_opens_the_showcase_example_in_the_browser() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.browser = true;
    app.refresh();
    app.home_open = true;
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Showcase board")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        !app.home_open,
        "picking the board leaves the start screen: {}",
        app.message
    );
    assert_eq!(
        app.project.components.len(),
        55,
        "every part arrives without a file: {}",
        app.message
    );
    app.session
        .lock()
        .map_err(|_| "session unavailable".to_string())?
        .undo()
        .map_err(|e| e.to_string())?;
    app.refresh();
    assert_eq!(
        app.project.components.len(),
        0,
        "one undo returns to the empty design"
    );
    Ok(())
}
