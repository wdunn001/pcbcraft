//! Stories for the filter chips, the kind switch and the cards of the Library
//! tab in the left aside, plus the Properties tab beside the drawing.
use super::super::*;
use super::support::*;

fn library_open() -> (egui::Context, App) {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    app.region().left_tab = "library".to_owned();
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    (ctx, app)
}

#[test]
fn a_newcomer_switches_the_library_between_symbols_footprints_and_both() -> Result<(), String> {
    let (ctx, mut app) = library_open();
    for (id, view) in [
        ("button:Footprints", library_dock::ViewKind::Footprints),
        ("button:Both", library_dock::ViewKind::Both),
        ("button:Symbols", library_dock::ViewKind::Symbols),
    ] {
        tap_id(&ctx, &mut app, id)?;
        assert_eq!(app.dock.view, view, "{id} sets what the cards show");
    }
    Ok(())
}

#[test]
fn a_newcomer_narrows_the_library_with_the_chips_and_clears_them() -> Result<(), String> {
    let (ctx, mut app) = library_open();
    let missing = press_each(
        &ctx,
        &mut app,
        &[
            "button:Show or hide the common parts that ship with the app.",
            "button:Show or hide the parts saved in this design's own library.",
            "button:Show or hide parts from the libraries installed on this comp",
            "button:Show or hide parts from online part sources. Results appear ",
            "button:Show only resistors, capacitors, inductors, diodes and simil",
            "button:Show only integrated circuits and transistors. Click again t",
            "button:Show only connectors and headers. Click again to clear.",
            "button:Show only the parts you starred. Click again to show every p",
        ],
    );
    assert!(missing.is_empty(), "chips not in the library: {missing:?}");
    assert!(
        app.dock.favourites_only,
        "the star chip keeps only starred parts"
    );
    tap_id(&ctx, &mut app, "button:More filters")?;
    Ok(())
}

#[test]
fn a_newcomer_places_a_built_in_part_from_its_card() -> Result<(), String> {
    let (ctx, mut app) = library_open();
    assert!(
        scrolled_to_left(&ctx, &mut app, "Light emitting diode")?,
        "the LED card says what the part is"
    );
    tap_id(
        &ctx,
        &mut app,
        "button:LED, Built in \u{b7} MIT OR Apache-#.#",
    )?;
    assert!(
        matches!(&app.tool, Tool::Place(kind) if kind.contains("led")),
        "pressing the LED card arms it for the sheet, got {:?}",
        app.tool
    );
    Ok(())
}

#[test]
fn a_newcomer_reads_the_properties_tab_beside_the_drawing() -> Result<(), String> {
    let (ctx, mut app) = library_open();
    tap_id(&ctx, &mut app, "tab:Properties")?;
    assert!(
        paints_text(&ctx, &mut app, "Nothing selected")?,
        "with nothing chosen the Properties tab says so"
    );
    Ok(())
}
