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
