//! Stories for the filter chips, the kind switch and the cards of the Library
//! tab in the left aside, plus the Properties tab beside the drawing.
use super::super::*;
use super::audit;
use super::support::*;

fn frame_sized(
    ctx: &egui::Context,
    app: &mut App,
    size: egui::Vec2,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    ctx.enable_accesskit();
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, size)),
            events,
            ..Default::default()
        },
        |ui| app.show(ui),
    );
    audit::remember_labels(&output);
    output.textures_delta.clear();
    output
}

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

fn library_view_strip(ctx: &egui::Context, app: &App) -> Result<Rect, String> {
    let control = audit::controls(ctx, app)
        .into_iter()
        .find(|c| {
            c.id.as_deref()
                .is_some_and(|id| id.starts_with("radiobutton:Show"))
        })
        .ok_or("library view switch is not on screen")?;
    Ok(control.rect)
}

fn tap_library_view(ctx: &egui::Context, app: &mut App, slot: usize) -> Result<(), String> {
    let rect = library_view_strip(ctx, app)?;
    let third = rect.width() / 3.0;
    let x = rect.min.x + third * (slot as f32 + 0.5);
    tap_at(ctx, app, egui::pos2(x, rect.center().y))
}

#[test]
fn a_newcomer_switches_the_library_between_symbols_footprints_and_both() -> Result<(), String> {
    let (ctx, mut app) = library_open();

    // The segmented control is on screen with all three options.
    let strip = library_view_strip(&ctx, &app)?;
    assert!(
        strip.width() > 100.0,
        "the view switch is full width, got {:.0}",
        strip.width()
    );

    // The footprints icon is not in the filter chips row.
    let chips = audit::controls(&ctx, &app);
    let footprints_icon = chips.iter().find(|c| {
        c.id.as_deref()
            .is_some_and(|id| id.starts_with("icon:footprint"))
    });
    assert!(
        footprints_icon.is_none(),
        "the footprints icon should not be in the filter chips row"
    );

    // Click each option and assert the view changes.
    for (slot, view) in [
        (1, library_dock::ViewKind::Footprints),
        (2, library_dock::ViewKind::Both),
        (0, library_dock::ViewKind::Symbols),
    ] {
        tap_library_view(&ctx, &mut app, slot)?;
        assert_eq!(app.dock.view, view, "slot {slot} sets what the cards show");
    }

    // AccessKit: the group is one radio group named "Show".
    let controls = audit::controls(&ctx, &app);
    let radios = controls
        .iter()
        .filter(|c| {
            c.id.as_deref()
                .is_some_and(|id| id.starts_with("radiobutton:Show"))
        })
        .collect::<Vec<_>>();
    assert_eq!(radios.len(), 1, "the view switch is one radio group");
    Ok(())
}

#[test]
fn a_newcomer_uses_the_arrow_keys_on_the_filter() -> Result<(), String> {
    let (ctx, mut app) = library_open();
    // Tab moves the keys onto the strip; it changes nothing by itself.
    ctx.memory_mut(|m| m.request_focus(egui::Id::new("library-view")));
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    frame(
        &ctx,
        &mut app,
        vec![egui::Event::Key {
            key: egui::Key::ArrowRight,
            physical_key: Some(egui::Key::ArrowRight),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
        egui::Modifiers::NONE,
    );
    assert_eq!(
        app.dock.view,
        library_dock::ViewKind::Footprints,
        "Right should move to Footprints"
    );
    frame(
        &ctx,
        &mut app,
        vec![egui::Event::Key {
            key: egui::Key::Home,
            physical_key: Some(egui::Key::Home),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
        egui::Modifiers::NONE,
    );
    assert_eq!(
        app.dock.view,
        library_dock::ViewKind::Symbols,
        "Home should jump back to Symbols"
    );
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

#[test]
fn the_library_view_switch_fits_a_narrow_aside() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    app.region().left_tab = "library".to_owned();
    let ctx = egui::Context::default();
    // Below `aside::is_narrow` the whole dock steps aside (reflow), so the narrow case is
    // the smallest aside: the switch must stay inside it at the smallest windows that
    // still draw it.
    for width in [800.0_f32, 1000.0] {
        // The first passes install the fonts; the dock draws on a later one.
        let mut output = frame_sized(&ctx, &mut app, egui::Vec2::new(width, 640.0), vec![]);
        for _ in 0..3 {
            output = frame_sized(&ctx, &mut app, egui::Vec2::new(width, 640.0), vec![]);
        }
        let strip = library_view_strip(&ctx, &app)?;
        assert!(
            strip.max.x <= 321.0,
            "view switch should not spill out of the aside in a {width} px window, got {:.0}",
            strip.max.x
        );
        let _ = output;
    }
    Ok(())
}

/// Frames so the dock settles after a toggle or a typed word.
fn settle(ctx: &egui::Context, app: &mut App) {
    for _ in 0..6 {
        frame(ctx, app, vec![], egui::Modifiers::NONE);
    }
}

const CONNECTOR_CHIP: &str = "button:Show only connectors and headers. Click again to clear.";

#[test]
fn a_newcomer_sees_the_samtec_series_at_once_and_with_connectors_on() -> Result<(), String> {
    let (ctx, mut app) = library_open();
    settle(&ctx, &mut app);
    assert!(
        paints_text(&ctx, &mut app, "Samtec connector series")?,
        "with no kind switch and no search the dock already lists the catalog series"
    );
    tap_id(&ctx, &mut app, CONNECTOR_CHIP)?;
    settle(&ctx, &mut app);
    assert!(
        paints_text(&ctx, &mut app, "Samtec connector series")?,
        "the heading names the maker once Connectors is on"
    );
    for series in [
        "TSW series, Samtec",
        "SSW series, Samtec",
        "BTH series, Samtec",
    ] {
        assert!(
            paints_text(&ctx, &mut app, series)?,
            "the {series} row is listed from the catalog"
        );
    }
    assert!(
        paints_text(&ctx, &mut app, "1 to 80 positions")?,
        "the range of positions comes from the series table"
    );
    Ok(())
}

#[test]
fn a_newcomer_types_samtec_and_finds_the_series() -> Result<(), String> {
    let (ctx, mut app) = library_open();
    settle(&ctx, &mut app);
    type_into(&ctx, &mut app, "textinput:Keyword or part number", "samtec")?;
    settle(&ctx, &mut app);
    for series in [
        "TSW series, Samtec",
        "SSW series, Samtec",
        "BTH series, Samtec",
    ] {
        assert!(
            paints_text(&ctx, &mut app, series)?,
            "{series} matches the word samtec"
        );
    }
    Ok(())
}

#[test]
fn a_newcomer_who_turns_on_resistors_sees_no_connector_series() -> Result<(), String> {
    let (ctx, mut app) = library_open();
    settle(&ctx, &mut app);
    tap_id(
        &ctx,
        &mut app,
        "button:Show only resistors, capacitors, inductors, diodes and simil",
    )?;
    settle(&ctx, &mut app);
    assert!(
        !paints_text(&ctx, &mut app, "Samtec connector series")?,
        "another kind switch keeps the connector series out"
    );
    Ok(())
}

/// Turn on Connectors, wait for the dock's read to finish so rows stop moving, then tap a series
/// row by its accessible name and check that it opens.
fn series_open(row_id: &str) -> Result<(egui::Context, App), String> {
    let (ctx, mut app) = library_open();
    settle(&ctx, &mut app);
    tap_id(&ctx, &mut app, CONNECTOR_CHIP)?;
    let steady = waits_until(&ctx, &mut app, |app| {
        app.dock.requests_sent > 0 && app.dock.applied == app.dock.requests_sent
    });
    assert!(steady, "the dock finished reading connectors");
    settle(&ctx, &mut app);
    tap_id(&ctx, &mut app, row_id)?;
    settle(&ctx, &mut app);
    assert!(
        app.dock.series.open_for_test().is_some(),
        "tapping {row_id} opens its choices"
    );
    Ok((ctx, app))
}

#[test]
fn a_newcomer_places_a_samtec_header_from_its_series_card() -> Result<(), String> {
    let (ctx, mut app) = series_open("button:TSW series, Samtec")?;
    assert!(
        paints_text(&ctx, &mut app, "Positions per row")?,
        "the series opens its choices"
    );
    tap_id(&ctx, &mut app, "button:# rows")?;
    tap_id(&ctx, &mut app, "button:More")?;
    settle(&ctx, &mut app);
    assert!(
        paints_text(&ctx, &mut app, "Makes TSW-005-07-G-D")?,
        "two rows of five make the part number the series table gives"
    );
    let before = app.project.components.len();
    tap_id(&ctx, &mut app, "button:Place on schematic")?;
    settle(&ctx, &mut app);
    assert_eq!(
        app.project.components.len(),
        before + 1,
        "one part is placed: {}",
        app.message
    );
    let placed = app.project.components.last().ok_or("a part was placed")?;
    assert_eq!(
        placed.value, "TSW-005-07-G-D",
        "the part carries the generated number"
    );
    assert!(
        app.project
            .library
            .footprints
            .iter()
            .any(|f| f.name.starts_with("TSW")),
        "its footprint joined the project library"
    );
    assert_eq!(
        app.project
            .library
            .footprints
            .iter()
            .find(|f| f.name.starts_with("TSW"))
            .map(|f| f.pads.len()),
        Some(10),
        "two rows of five make ten pads"
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
        "one undo takes the whole placement back"
    );
    assert!(
        !app.project
            .library
            .footprints
            .iter()
            .any(|f| f.name.starts_with("TSW")),
        "and the imported footprint goes with it"
    );
    Ok(())
}

#[test]
fn a_newcomer_picks_a_stack_height_for_a_board_to_board_series() -> Result<(), String> {
    let (ctx, mut app) = series_open("button:BTH series, Samtec")?;
    assert!(
        paints_text(&ctx, &mut app, "Stack height")?,
        "a board to board series asks for a stack code"
    );
    assert!(
        !paints_text(&ctx, &mut app, "Lead")?,
        "and not for a lead code, which that series does not have"
    );
    Ok(())
}

#[test]
fn a_newcomer_opens_the_socket_strip_series_and_chooses_a_lead_code() -> Result<(), String> {
    let (ctx, mut app) = series_open("button:SSW series, Samtec")?;
    assert!(
        paints_text(&ctx, &mut app, "Lead")?,
        "a socket strip series asks for a lead code"
    );
    tap_id(&ctx, &mut app, "button:More")?;
    settle(&ctx, &mut app);
    assert!(
        paints_text(&ctx, &mut app, "Makes SSW-005-07-G-S")?,
        "one more position than the starting four makes the part number the series table gives"
    );
    Ok(())
}
