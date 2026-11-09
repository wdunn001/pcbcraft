//! Stories for the Samtec series cards in the Library dock: they show before any search, a card
//! opens a form bounded by the series limits, and Place puts the generated part in the project.
use super::super::*;
use super::support::*;
use pcbcraft_library::connector_catalog::ConnectorCatalog;
use pcbcraft_library::{ConnectorSeriesOptions, generate_connector_part, series_spec};

const CONNECTOR_CHIP: &str = "button:Show only connectors and headers. Click again to clear.";

fn dock() -> (egui::Context, App) {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    app.region().left_tab = "library".to_owned();
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    settle(&ctx, &mut app);
    (ctx, app)
}

fn settle(ctx: &egui::Context, app: &mut App) {
    for _ in 0..6 {
        frame(ctx, app, vec![], egui::Modifiers::NONE);
    }
}

fn open_card(ctx: &egui::Context, app: &mut App, series: &str) -> Result<(), String> {
    tap_id(ctx, app, CONNECTOR_CHIP)?;
    let steady = waits_until(ctx, app, |app| {
        app.dock.requests_sent > 0 && app.dock.applied == app.dock.requests_sent
    });
    assert!(steady, "the dock finished reading connectors");
    settle(ctx, app);
    tap_id(ctx, app, &format!("button:{series} series, Samtec"))?;
    settle(ctx, app);
    Ok(())
}

#[test]
fn a_newcomer_turns_on_connectors_and_sees_the_samtec_series() -> Result<(), String> {
    let (ctx, mut app) = dock();
    tap_id(&ctx, &mut app, CONNECTOR_CHIP)?;
    settle(&ctx, &mut app);
    assert!(
        paints_text(&ctx, &mut app, "Samtec connector series")?,
        "the group heading is read from the strings"
    );
    let catalog = ConnectorCatalog::embedded()?;
    assert!(!catalog.series.is_empty());
    for series in &catalog.series {
        let name = format!("{} series, Samtec", series.prefix);
        assert!(
            paints_text(&ctx, &mut app, &name)?,
            "one card for the {} series",
            series.prefix
        );
    }
    Ok(())
}

#[test]
fn a_newcomer_types_samtec_and_finds_the_series() -> Result<(), String> {
    let (ctx, mut app) = dock();
    type_into(&ctx, &mut app, "textinput:Keyword or part number", "samtec")?;
    settle(&ctx, &mut app);
    for series in &ConnectorCatalog::embedded()?.series {
        let name = format!("{} series, Samtec", series.prefix);
        assert!(paints_text(&ctx, &mut app, &name)?, "{name} matches samtec");
    }
    Ok(())
}

#[test]
fn a_newcomer_places_a_samtec_pin_header_from_its_card() -> Result<(), String> {
    let (ctx, mut app) = dock();
    open_card(&ctx, &mut app, "TSW")?;
    assert!(
        paints_text(&ctx, &mut app, "Positions per row")?,
        "the card opens a form with the pin count"
    );
    assert!(
        paints_text(&ctx, &mut app, "Rows")?,
        "and the rows the series allows"
    );
    tap_id(&ctx, &mut app, "button:# rows")?;
    tap_id(&ctx, &mut app, "button:More")?;
    settle(&ctx, &mut app);
    let spec = series_spec("TSW")?;
    let options = ConnectorSeriesOptions {
        positions_per_row: 5,
        rows: 2,
        lead_code: spec
            .lead_codes
            .first()
            .map(|c| c.code.clone())
            .unwrap_or_default(),
        stack_code: None,
        alignment_posts: false,
    };
    let made = generate_connector_part("TSW", &options).map_err(|e| format!("{e:?}"))?;
    assert!(
        paints_text(&ctx, &mut app, &format!("Makes {}", made.part_number))?,
        "the form shows the generated part number"
    );
    let before = app.project.components.len();
    tap_id(&ctx, &mut app, "button:Place on schematic")?;
    settle(&ctx, &mut app);
    assert_eq!(app.project.components.len(), before + 1, "{}", app.message);
    let placed = app.project.components.last().ok_or("a part was placed")?;
    assert_eq!(placed.value, made.part_number);
    let footprint = app
        .project
        .library
        .footprints
        .iter()
        .find(|f| f.name == made.footprint.name)
        .ok_or("the generated footprint joined the project library")?;
    assert_eq!(
        footprint.pads.len(),
        made.footprint.pads.len(),
        "same pad count as the generator"
    );
    for (got, want) in footprint.pads.iter().zip(&made.footprint.pads) {
        assert_eq!(got.number, want.number, "same pad numbers");
        assert!(
            (got.position.x - want.position.x).abs() < 1e-6
                && (got.position.y - want.position.y).abs() < 1e-6,
            "pad {} sits where the generator put it",
            want.number
        );
    }
    assert!(
        placed.library_footprint.is_some(),
        "the part carries the footprint so the Layout can show it"
    );
    app.session
        .lock()
        .map_err(|_| "session is busy".to_owned())?
        .undo()
        .map_err(|e| e.to_string())?;
    app.refresh();
    assert_eq!(
        app.project.components.len(),
        before,
        "one undo undoes it all"
    );
    assert!(
        !app.project
            .library
            .footprints
            .iter()
            .any(|f| f.name == made.footprint.name),
        "the footprint goes with it"
    );
    Ok(())
}

#[test]
fn a_newcomer_asks_for_too_many_pins_and_reads_the_limit() -> Result<(), String> {
    let (ctx, mut app) = dock();
    open_card(&ctx, &mut app, "TSW")?;
    let max = series_spec("TSW")?.limits.positions_per_row_max;
    let line = format!("This series makes at most {max} positions per row.");
    assert!(
        !paints_text(&ctx, &mut app, &line)?,
        "no limit line while the count is inside the limits"
    );
    for _ in 0..max {
        if paints_text(&ctx, &mut app, &line)? {
            break;
        }
        tap_id(&ctx, &mut app, "button:More")?;
        settle(&ctx, &mut app);
    }
    assert!(
        paints_text(&ctx, &mut app, &line)?,
        "the form says the limit in a plain line"
    );
    assert_eq!(
        app.dock.series.positions_for_test(),
        max,
        "the count stops at the limit"
    );
    Ok(())
}

#[test]
fn a_newcomer_places_a_samtec_footprint_on_the_layout_in_one_undo_step() -> Result<(), String> {
    let (ctx, mut app) = dock();
    app.change_tab(Tab::Pcb);
    app.region().left_tab = "library".to_owned();
    settle(&ctx, &mut app);
    open_card(&ctx, &mut app, "TSW")?;
    let parts = app.project.components.len();
    let feet = app.project.board.footprints.len();
    tap_id(&ctx, &mut app, "button:Place on board")?;
    settle(&ctx, &mut app);
    assert_eq!(app.project.components.len(), parts + 1, "{}", app.message);
    assert_eq!(app.project.board.footprints.len(), feet + 1);
    app.session
        .lock()
        .map_err(|_| "session is busy".to_owned())?
        .undo()
        .map_err(|e| e.to_string())?;
    app.refresh();
    assert_eq!(app.project.components.len(), parts);
    assert_eq!(app.project.board.footprints.len(), feet);
    Ok(())
}
