use super::super::*;
use super::support::*;

fn fresh() -> App {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app
}

#[test]
fn a_newcomer_visits_settings_and_comes_back_to_the_same_zoom() -> Result<(), String> {
    let mut app = fresh();
    let ctx = egui::Context::default();
    app.change_tab(Tab::Pcb);
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.pcb.zoom = 33.0;
    for tab in [Tab::Settings, Tab::Rules, Tab::Checks, Tab::Pcb] {
        app.change_tab(tab);
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    assert_eq!(app.pcb.zoom, 33.0, "the board stays as the user left it");
    Ok(())
}

#[test]
fn a_newcomer_reads_settings_with_room_around_the_text() -> Result<(), String> {
    let mut app = fresh();
    let ctx = egui::Context::default();
    for tab in [Tab::Settings, Tab::Rules, Tab::Checks] {
        app.change_tab(tab);
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        let output = frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        let left = app.page_left;
        let mut tight = Vec::new();
        for shape in &output.shapes {
            if let egui::Shape::Text(text) = &shape.shape
                && text.pos.x > left
                && text.pos.x < left + 16.0
                && text.pos.y > 80.0
            {
                tight.push(text.galley.text().to_owned());
            }
        }
        assert!(tight.is_empty(), "{tab:?} text hugs the edge: {tight:?}");
    }
    Ok(())
}

#[test]
fn a_newcomer_finds_appearance_on_the_settings_page() -> Result<(), String> {
    let mut app = fresh();
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.change_tab(Tab::Settings);
    assert!(
        paints_text(&ctx, &mut app, "APPEARANCE")?,
        "appearance lives on the Settings page"
    );
    app.change_tab(Tab::Pcb);
    assert!(
        !paints_text(&ctx, &mut app, "APPEARANCE")?,
        "the inspector no longer shows appearance"
    );
    Ok(())
}

/// The Japanese font the installer ships next to the program; the test
/// reads the same file from the repository that the native loader reads
/// from beside the executable and the browser build fetches over HTTP.
const JP_FONT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../assets/fonts/NotoSansJP-Regular.otf"
);

#[test]
fn a_newcomer_picks_japanese_and_sees_no_empty_boxes() -> Result<(), String> {
    let mut app = fresh();
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let bytes = std::fs::read(JP_FONT).unwrap_or_default();
    assert!(!bytes.is_empty(), "{JP_FONT} must ship in the repository");
    crate::fonts::add_fallback(&ctx, "NotoSansJP-Regular", bytes);
    app.set_language(&ctx, "ja");
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    // Test-only Japanese words a designer meets in this app: board,
    // design, wiring. The translation file arrives in another issue.
    let japanese = "基板設計配線";
    let galley = ctx.fonts_mut(|fonts| {
        fonts.layout_no_wrap(
            japanese.to_string(),
            egui::FontId::proportional(14.0),
            egui::Color32::WHITE,
        )
    });
    let mut boxes = Vec::new();
    for row in &galley.rows {
        for glyph in &row.glyphs {
            // A real Japanese glyph fills a full 14-point em; an empty
            // box from the Latin font is far narrower.
            if (glyph.chr as u32) >= 0x2e80 && glyph.advance_width < 13.0 {
                boxes.push(format!(
                    "{} drew only {:.1} points wide",
                    glyph.chr, glyph.advance_width
                ));
            }
        }
    }
    assert!(
        boxes.is_empty(),
        "Japanese text still shows empty boxes: {boxes:?}"
    );
    Ok(())
}

#[test]
fn a_missing_font_file_keeps_the_app_usable_and_says_so() -> Result<(), String> {
    let mut app = fresh();
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    // The test binary runs with no fonts folder next to it, so the
    // Japanese file really is missing for this app.
    app.set_language(&ctx, "ja");
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        app.message.contains("font file") && app.message.contains("NotoSansJP-Regular.otf"),
        "the status line should say which font file is missing: {}",
        app.message
    );
    app.change_tab(Tab::Settings);
    assert!(
        paints_text(&ctx, &mut app, "APPEARANCE")?,
        "the app stays usable and keeps painting text"
    );
    Ok(())
}
