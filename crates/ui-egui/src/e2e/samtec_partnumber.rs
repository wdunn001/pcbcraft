//! Stories for Samtec connector part numbers: a newcomer types a part number in the Library dock
//! and sees its footprint, and a newcomer picks a series, sets options, and places the part on
//! the board.
use super::super::*;
use super::support::*;
use pcbcraft_library::connector_partnumber::{first_slice_specs, parse};
use pcbcraft_library::{ConnectorSeriesOptions, generate_connector_part, series_spec};

/// A Library-tab dock on a sample design, ready for a search.
fn dock() -> (egui::Context, App) {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    app.region().left_tab = "library".to_owned();
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    settle_frames(&ctx, &mut app);
    (ctx, app)
}

/// A few settled frames so the dock catches up.
fn settle_frames(ctx: &egui::Context, app: &mut App) {
    for _ in 0..6 {
        frame(ctx, app, vec![], egui::Modifiers::NONE);
    }
}

#[test]
fn a_newcomer_types_a_samtec_part_number_and_sees_its_footprint() -> Result<(), String> {
    let number = "TSW-105-07-S-D";
    let (ctx, mut app) = dock();
    type_into(&ctx, &mut app, "textinput:Keyword or part number", number)?;
    let steady = waits_until(&ctx, &mut app, |app| {
        app.dock.requests_sent > 0 && app.dock.applied == app.dock.requests_sent
    });
    assert!(steady, "the dock finished reading the search");
    settle_frames(&ctx, &mut app);
    assert!(
        paints_text(&ctx, &mut app, "TSW series, Samtec")?,
        "the typed part number finds the series it belongs to"
    );
    let specs = first_slice_specs().map_err(|e| e.to_string())?;
    parse(&specs, number).map_err(|e| format!("{number} should parse: {e:?}"))?;
    let spec = series_spec("TSW")?;
    let made = generate_connector_part(
        "TSW",
        &ConnectorSeriesOptions {
            positions_per_row: 5,
            rows: 2,
            lead_code: spec
                .lead_codes
                .first()
                .map(|c| c.code.clone())
                .unwrap_or_default(),
            stack_code: None,
            alignment_posts: false,
        },
    )
    .map_err(|e| format!("{e:?}"))?;
    assert_eq!(
        made.footprint.pads.len(),
        10,
        "the part number names ten positions, so the footprint has ten pads"
    );
    Ok(())
}

#[test]
fn a_newcomer_picks_a_connector_series_sets_options_and_places_it_on_the_board()
-> Result<(), String> {
    let (ctx, mut app) = dock();
    tap_id(
        &ctx,
        &mut app,
        "button:Show only connectors and headers. Click again to clear.",
    )?;
    let steady = waits_until(&ctx, &mut app, |app| {
        app.dock.requests_sent > 0 && app.dock.applied == app.dock.requests_sent
    });
    assert!(steady, "the dock finished reading connectors");
    settle_frames(&ctx, &mut app);
    tap_id(&ctx, &mut app, "button:TSW series, Samtec")?;
    settle_frames(&ctx, &mut app);
    assert!(
        paints_text(&ctx, &mut app, "1 row")?,
        "the form shows the rows choice the series opens with"
    );
    tap_id(&ctx, &mut app, "button:More")?;
    settle_frames(&ctx, &mut app);
    tap_id(&ctx, &mut app, "button:More")?;
    settle_frames(&ctx, &mut app);
    tap_id(&ctx, &mut app, "button:Fewer")?;
    settle_frames(&ctx, &mut app);
    tap_id(&ctx, &mut app, "checkbox:Alignment posts")?;
    settle_frames(&ctx, &mut app);
    let spec = series_spec("TSW")?;
    let options = ConnectorSeriesOptions {
        positions_per_row: 5,
        rows: 1,
        lead_code: spec
            .lead_codes
            .first()
            .map(|c| c.code.clone())
            .unwrap_or_default(),
        stack_code: None,
        alignment_posts: true,
    };
    let made = generate_connector_part("TSW", &options).map_err(|e| format!("{e:?}"))?;
    assert!(
        paints_text(&ctx, &mut app, &format!("Makes {}", made.part_number))?,
        "the form shows the number the chosen options make"
    );
    app.change_tab(Tab::Pcb);
    app.region().left_tab = "library".to_owned();
    settle_frames(&ctx, &mut app);
    let parts = app.project.components.len();
    let feet = app.project.board.footprints.len();
    tap_id(&ctx, &mut app, "button:Place on board")?;
    settle_frames(&ctx, &mut app);
    assert_eq!(app.project.components.len(), parts + 1, "{}", app.message);
    assert_eq!(app.project.board.footprints.len(), feet + 1);
    let placed = app.project.components.last().ok_or("a part was placed")?;
    assert_eq!(
        placed.value, made.part_number,
        "the placed part carries the number the form showed"
    );
    app.session
        .lock()
        .map_err(|_| "session is busy".to_owned())?
        .undo()
        .map_err(|e| e.to_string())?;
    app.refresh();
    assert_eq!(app.project.components.len(), parts);
    assert_eq!(
        app.project.board.footprints.len(),
        feet,
        "one undo removes the part and its footprint"
    );
    Ok(())
}
