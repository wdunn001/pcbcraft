//! Story: a newcomer places a generated header and sees it line up with the board grid.
//!
//! Generate a TSW-011-07-L-S part (11-position, 2.54mm pitch single-row header),
//! validate the footprint, and confirm the pads land on 2.54mm grid coordinates.

use pcbcraft_library::{ConnectorSeriesOptions, generate_connector_part};

#[test]
fn a_newcomer_places_a_generated_header_and_sees_it_line_up_with_the_board_grid()
-> Result<(), String> {
    // Generate a TSW-011-07-L-S: 11 positions, single row, 2.54mm pitch, straight leads
    let part = generate_connector_part(
        "TSW",
        &ConnectorSeriesOptions {
            positions_per_row: 11,
            rows: 1,
            lead_code: "07".into(),
            stack_code: None,
            alignment_posts: false,
        },
    )
    .map_err(|e| format!("generate_connector_part failed: {e:?}"))?;

    // The part number should match the expected format
    assert!(
        part.part_number.starts_with("TSW-011-"),
        "expected part number to start with TSW-011-, got: {}",
        part.part_number
    );

    // Validate the footprint
    part.footprint
        .validate()
        .map_err(|e| format!("footprint validate failed: {e}"))?;

    // Confirm the description includes the verification note
    assert!(
        part.footprint.description.contains("verify against"),
        "description should contain 'verify against' note"
    );

    // Check that signal pads are on 2.54mm grid coordinates (within tolerance)
    let grid = 2.54;
    let tolerance = 0.01;
    let signal_pads: Vec<_> = part
        .footprint
        .pads
        .iter()
        .filter(|p| p.number.chars().all(|c| c.is_ascii_digit()))
        .collect();

    assert_eq!(
        signal_pads.len(),
        11,
        "expected 11 signal pads, got {}",
        signal_pads.len()
    );

    for pad in &signal_pads {
        let x_on_grid = (pad.position.x / grid).round() * grid;
        let y_on_grid = (pad.position.y / grid).round() * grid;
        assert!(
            (pad.position.x - x_on_grid).abs() < tolerance,
            "pad {} at x={:.4} is not on 2.54mm grid (expected x={:.4})",
            pad.number,
            pad.position.x,
            x_on_grid
        );
        assert!(
            (pad.position.y - y_on_grid).abs() < tolerance,
            "pad {} at y={:.4} is not on 2.54mm grid (expected y={:.4})",
            pad.number,
            pad.position.y,
            y_on_grid
        );
    }

    // Confirm the body fits within the courtyard
    assert!(
        pcbcraft_library::body_fits_courtyard(&part.parametric, &part.footprint),
        "parametric body should fit within the courtyard"
    );

    Ok(())
}
