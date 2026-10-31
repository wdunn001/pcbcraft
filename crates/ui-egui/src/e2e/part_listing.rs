//! Story for the one part search of the Library tab: a single box lists parts
//! from this project, an installed library, the built-in parts and the open
//! online library, each card naming its source. Online answers come from the
//! recordings next to the library crate's tests; no story reaches a host.
use super::super::*;
use super::support::*;

/// Top edge of the first line of text on screen that reads exactly `needle`.
fn top_of(ctx: &egui::Context, app: &mut App, needle: &str) -> Option<f32> {
    let output = frame(ctx, app, vec![], egui::Modifiers::NONE);
    output.shapes.iter().find_map(|shape| match &shape.shape {
        egui::Shape::Text(text) if text.galley.text() == needle => Some(text.pos.y),
        _ => None,
    })
}

#[test]
fn a_newcomer_searches_once_and_sees_parts_from_every_source() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.region().left_tab = "library".to_owned();
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.change_tab(Tab::Schematic);

    // A part saved in this project, an installed library folder, and the open library.
    let text = std::fs::read_to_string(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../library/tests/fixtures/device.kicad_sym"),
    )
    .map_err(|e| e.to_string())?;
    let mut mine = pcbcraft_library::read_symbols(&text).map_err(|e| e.to_string())?;
    mine.truncate(1);
    for s in &mut mine {
        s.name = "Project resistor".into();
    }
    app.project.library.symbols = mine;
    let folder = std::env::temp_dir().join(format!("pcbcraft-listing-{}", std::process::id()));
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    std::fs::write(folder.join("Device.kicad_sym"), &text).map_err(|e| e.to_string())?;
    app.dock.roots = Some(vec![folder]);
    use_recorded_library(&mut app, "one-part-listing")?;

    // One search, typed once.
    app.part_search = "resistor".into();
    assert!(
        waits_for_text(&ctx, &mut app, "R_Small")?,
        "the open library answers: {}",
        app.message
    );
    for (what, badge) in [
        ("this project", "From this project"),
        ("an installed library", "Device"),
        ("the built-in parts", "Built in"),
        ("the open library", "Open library"),
    ] {
        assert!(
            paints_text(&ctx, &mut app, badge)?,
            "a card from {what} names its source ({badge})"
        );
    }
    assert!(paints_text(&ctx, &mut app, "Project resistor")?);

    // The part named exactly like the search comes before the looser matches.
    let exact = top_of(&ctx, &mut app, "Resistor").ok_or("no Resistor card")?;
    let loose = top_of(&ctx, &mut app, "R_Small").ok_or("no R_Small card")?;
    assert!(exact < loose, "exact name first: {exact} vs {loose}");
    Ok(())
}
