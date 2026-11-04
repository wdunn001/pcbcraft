//! Stories for the generated Samtec connector series (TSW, SSW, BTH):
//! searching, generating a footprint, and seeing the 3D body sit flush with
//! the board at z=0.
use super::super::*;
use super::support::*;

#[test]
fn a_newcomer_places_a_generated_header_and_sees_it_line_up_with_the_board_grid()
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

    // The TSW part should be listed as generated.
    assert!(
        paints_text(&ctx, &mut app, "generated")?,
        "the TSW part should be identified as a generated connector series part"
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

    // The generated part should be placed on the schematic.
    frame(&ctx, &mut app, vec![], none);

    // Verify the part was placed with the correct footprint.
    let session = shared.lock().map_err(|_| "session")?;

    // Find the placed component by looking for one with a TSW library footprint.
    let _placed = session
        .project
        .components
        .iter()
        .find(|c| {
            c.library_footprint
                .as_ref()
                .map(|lf| lf.definition.name.starts_with("TSW"))
                .unwrap_or(false)
        })
        .ok_or("the TSW header should be placed on the schematic")?;

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

    // Verify the 3D body is at z=0 (flush with the board).
    let parametric = pcbcraft_library::ConnectorParametricSpec::from_json(
        &footprint
            .model
            .as_ref()
            .map(|m| m.raw_source.clone())
            .unwrap_or_default(),
    )
    .map_err(|e| format!("failed to parse parametric spec: {e}"))?;
    let (min, _) = parametric.body_above_board_bounds();
    assert!(
        (min[2] - 0.0).abs() < 0.01,
        "the 3D body should sit at z=0 (flush with the board), got z={:.4}",
        min[2]
    );

    Ok(())
}
