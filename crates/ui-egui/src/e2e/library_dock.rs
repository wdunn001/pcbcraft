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
    for (slot, view) in [
        (1, library_dock::ViewKind::Footprints),
        (2, library_dock::ViewKind::Both),
        (0, library_dock::ViewKind::Symbols),
    ] {
        tap_library_view(&ctx, &mut app, slot)?;
        assert_eq!(app.dock.view, view, "slot {slot} sets what the cards show");
    }
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
    // Below 640 points the whole dock steps aside (reflow), so the narrow case is the
    // default 320 point aside: the switch must stay inside it at the smallest windows
    // that still draw it.
    for width in [640.0_f32, 1000.0] {
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
