//! Stories for the generated Samtec connector series (TSW, SSW, BTH):
//! searching the library and generating a footprint with the right pads. The
//! 3D body at z=0 is covered by crates/library/tests/connector_series_generate.rs.
use super::super::*;
use super::support::*;

#[test]
fn a_newcomer_searches_for_a_header_series_and_generates_a_footprint_with_its_pads()
-> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared.clone(), Preferences::default(), None, None);
    app.refresh();
    app.region().left_tab = "library".to_owned();
    app.tool_call("sample_design", json!({}));
    let ctx = egui::Context::default();
    let none = egui::Modifiers::NONE;
    frame(&ctx, &mut app, vec![], none);
    app.change_tab(Tab::Schematic);
    frame(&ctx, &mut app, vec![], none);

    // Search for the TSW connector series.
    tap_id(&ctx, &mut app, "textinput:Keyword or part number")?;
    for letter in "TSW".chars() {
        frame(&ctx, &mut app, vec![egui::Event::Text(letter.into())], none);
    }
    frame(&ctx, &mut app, vec![], none);

    // Wait for the library list to return the generated TSW part.
    assert!(
        waits_for_text(&ctx, &mut app, "TSW")?,
        "the generated TSW connector series should appear in the search results"
    );

    // Generate the footprint for a 4-position single-row TSW header.
    app.tool_call(
        "library.footprint.generate",
        json!({
            "family": "header_single",
            "name": "TSW-104-07-L-S",
            "description": "4-position single-row TSW header",
            "keywords": "connector header tsw",
            "pitch": 2.54,
            "pin_count": 4,
            "body_x": 10.16,
            "body_y": 3.0,
            "body_z": 3.0
        }),
    );

    frame(&ctx, &mut app, vec![], none);

    // Verify the generated footprint.
    let session = shared.lock().map_err(|_| "session")?;

    // The footprint should have the correct number of pads (4 signal pads).
    let footprint = session
        .project
        .library
        .footprints
        .iter()
        .find(|f| f.name.starts_with("TSW"))
        .ok_or("the TSW footprint should exist in the library")?;

    let signal_pads: usize = footprint
        .pads
        .iter()
        .filter(|p| p.number.chars().all(|c| c.is_ascii_digit()))
        .count();
    assert_eq!(signal_pads, 4, "the TSW header should have 4 signal pads");

    Ok(())
}
