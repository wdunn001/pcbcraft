//! Stories for the fixed rows under each schematic page: Symbols, Power
//! ports, Drawing and Title block. Each row hides, locks, shows its own
//! Properties and cannot be removed; empty ones stay out of the tree.
use super::super::*;
use super::support::*;

fn start() -> (egui::Context, App) {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.change_tab(Tab::Schematic);
    app.region().left_tab = "layers".to_owned();
    app.refresh();
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    (ctx, app)
}

fn practice() -> (egui::Context, App) {
    let (ctx, mut app) = start();
    app.tool_call("sample_design", json!({}));
    app.change_tab(Tab::Schematic);
    app.region().left_tab = "layers".to_owned();
    app.refresh();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    (ctx, app)
}

/// Every text the schematic canvas paints on the current page.
fn canvas_text(app: &App) -> Vec<String> {
    let ctx = egui::Context::default();
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                Vec2::new(1440.0, 940.0),
            )),
            ..Default::default()
        },
        |root_ui| {
            egui::CentralPanel::default().show(root_ui, |ui| {
                let rect = ui.max_rect();
                app.paint_schematic(ui.painter(), rect, app.schematic, &app.project);
            });
        },
    );
    output.textures_delta.clear();
    output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
            _ => None,
        })
        .collect()
}

fn power_net(app: &App, name: &str) -> Id {
    app.project
        .nets
        .iter()
        .find(|n| n.name == name)
        .map_or(0, |n| n.id)
}

#[test]
fn a_newcomer_hides_and_locks_the_title_block() -> Result<(), String> {
    let (ctx, mut app) = practice();
    assert!(
        canvas_text(&app).iter().any(|t| t == "TITLE"),
        "the page starts with its title block"
    );
    tap_id(&ctx, &mut app, "button:Hide Title block")?;
    assert!(
        app.preferences
            .hidden_layers
            .contains(&"page:0:title".to_owned()),
        "the eye hides the title block of this page: {:?}",
        app.preferences.hidden_layers
    );
    assert!(
        !canvas_text(&app).iter().any(|t| t == "TITLE"),
        "a hidden title block is not drawn"
    );
    tap_id(&ctx, &mut app, "button:Show Title block")?;
    assert!(
        canvas_text(&app).iter().any(|t| t == "TITLE"),
        "the eye brings it back"
    );
    tap_id(&ctx, &mut app, "button:Lock Title block")?;
    assert!(
        app.tree.locked.contains("page:0:title"),
        "the lock is remembered: {:?}",
        app.tree.locked
    );
    Ok(())
}

#[test]
fn a_newcomer_cannot_pick_what_a_locked_or_hidden_row_holds() -> Result<(), String> {
    let (ctx, mut app) = practice();
    let part = app
        .project
        .components
        .first()
        .ok_or("the sample has parts")?
        .clone();
    let (lo, hi) = (
        Point::new(part.position.x - 40.0, part.position.y - 40.0),
        Point::new(part.position.x + 40.0, part.position.y + 40.0),
    );
    assert!(
        app.ids_in_rect(lo, hi).contains(&part.id),
        "a box finds the part while its row is open"
    );
    assert_eq!(
        app.pick_object(part.position, 4.0),
        Some(part.id),
        "a click finds the part while its row is open"
    );
    tap_id(&ctx, &mut app, "button:Lock Symbols")?;
    assert!(
        !app.ids_in_rect(lo, hi).contains(&part.id),
        "a box select skips a locked row"
    );
    assert_eq!(
        app.pick_object(part.position, 4.0),
        None,
        "a click skips a locked row"
    );
    tap_id(&ctx, &mut app, "button:Unlock Symbols")?;
    tap_id(&ctx, &mut app, "button:Hide Symbols")?;
    assert!(
        !app.ids_in_rect(lo, hi).contains(&part.id)
            && app.pick_object(part.position, 4.0).is_none(),
        "a hidden row cannot be picked either"
    );
    Ok(())
}

#[test]
fn a_newcomer_hides_drawing_and_power_ports_on_the_page() -> Result<(), String> {
    let (ctx, mut app) = practice();
    let gnd = power_net(&app, "GND");
    app.tool_call(
        "graphic.add",
        json!({"layer":"schematic","shape":"text","points":[{"x":20.0,"y":20.0}],"text":"NOTE ABOUT BOARD","width":3.0}),
    );
    app.tool_call(
        "label.place",
        json!({"kind":"power","net":gnd,"position":{"x":30.0,"y":30.0}}),
    );
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(canvas_text(&app).iter().any(|t| t == "NOTE ABOUT BOARD"));
    tap_id(&ctx, &mut app, "button:Hide Drawing")?;
    assert!(
        !canvas_text(&app).iter().any(|t| t == "NOTE ABOUT BOARD"),
        "hidden drawing is not drawn"
    );
    let ports_before = canvas_text(&app).iter().filter(|t| *t == "GND").count();
    tap_id(&ctx, &mut app, "button:Hide Power ports")?;
    let ports_after = canvas_text(&app).iter().filter(|t| *t == "GND").count();
    assert!(
        ports_after < ports_before,
        "hidden power ports are not drawn ({ports_before} to {ports_after})"
    );
    Ok(())
}

#[test]
fn a_newcomer_edits_the_title_block_in_properties() -> Result<(), String> {
    let (ctx, mut app) = practice();
    tap_id(&ctx, &mut app, "button:Title block, shown, unlocked")?;
    assert_eq!(
        app.tree.selected_key.as_deref(),
        Some("sec:sheet:0/layer:title"),
        "the row selects itself: {}",
        app.message
    );
    type_into(&ctx, &mut app, "textinput:Title", "Sensor board")?;
    type_into(&ctx, &mut app, "textinput:Revision", "B")?;
    type_into(&ctx, &mut app, "textinput:Company", "Quasarke")?;
    type_into(&ctx, &mut app, "textinput:Drawn by", "William")?;
    let settings = app.project.page_settings_of(0);
    assert_eq!(settings.title, "Sensor board");
    assert_eq!(settings.revision, "B");
    assert_eq!(settings.company, "Quasarke");
    assert_eq!(settings.drawn_by, "William");
    assert!(
        canvas_text(&app).iter().any(|t| t == "Sensor board"),
        "the sheet prints the new title"
    );
    app.tool_call("undo", json!({}));
    assert_eq!(
        app.project.page_settings_of(0).drawn_by,
        "",
        "an edit is one undo step"
    );
    app.tool_call("redo", json!({}));
    // Show title block off removes the box from this page and the row from the tree.
    tap_properties(&ctx, &mut app, "checkbox:Show title block")?;
    assert!(app.project.page_settings_of(0).title_hidden);
    assert!(
        !on_screen(&ctx, &mut app, "button:Title block, shown, unlocked"),
        "a page without a title block has no Title block row"
    );
    assert!(!canvas_text(&app).iter().any(|t| t == "TITLE"));
    Ok(())
}

#[test]
fn a_newcomer_sees_power_ports_listed_with_their_nets() -> Result<(), String> {
    let (ctx, mut app) = practice();
    let gnd = power_net(&app, "GND");
    app.tool_call(
        "label.place",
        json!({"kind":"power","net":gnd,"position":{"x":30.0,"y":30.0}}),
    );
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    tap_id(&ctx, &mut app, "button:Power ports, shown, unlocked")?;
    assert!(
        paints_text(&ctx, &mut app, "GND")?,
        "Properties lists the port with its net"
    );
    tap_properties(&ctx, &mut app, "button:Select all")?;
    assert_eq!(app.selection().len(), 1, "Select all picks the one port");
    let before = app
        .project
        .labels
        .iter()
        .filter(|l| l.kind == "power")
        .count();
    tap_id(&ctx, &mut app, "button:Power ports, shown, unlocked")?;
    tap_properties(&ctx, &mut app, "button:Add power port")?;
    let after = app
        .project
        .labels
        .iter()
        .filter(|l| l.kind == "power")
        .count();
    assert_eq!(
        after,
        before + 1,
        "Add power port places another: {}",
        app.message
    );
    Ok(())
}

#[test]
fn a_newcomer_never_sees_empty_categories_by_default() -> Result<(), String> {
    let (ctx, mut app) = start();
    assert!(
        !on_screen(&ctx, &mut app, "button:Power ports, shown, unlocked"),
        "an empty page lists no Power ports row"
    );
    assert!(!on_screen(
        &ctx,
        &mut app,
        "button:Drawing, shown, unlocked"
    ));
    assert!(!on_screen(
        &ctx,
        &mut app,
        "button:Symbols, shown, unlocked"
    ));
    tap_id(&ctx, &mut app, "button:Show empty")?;
    assert!(
        on_screen(&ctx, &mut app, "button:Power ports, shown, unlocked"),
        "Show empty lists every category"
    );
    tap_id(&ctx, &mut app, "button:Hide empty")?;
    assert!(!on_screen(
        &ctx,
        &mut app,
        "button:Power ports, shown, unlocked"
    ));
    Ok(())
}

#[test]
fn a_newcomer_cannot_remove_a_fixed_row_of_a_page() -> Result<(), String> {
    let (ctx, mut app) = practice();
    let before = app.project.clone();
    // The menu item is drawn disabled, so a person cannot press it; the rows stay.
    tap_id(&ctx, &mut app, "button:Title block, shown, unlocked")?;
    assert_eq!(app.project, before, "selecting a row changes nothing");
    assert!(on_screen(&ctx, &mut app, "button:Symbols, shown, unlocked"));
    Ok(())
}

/// Left edge of the right aside, read from its Properties tab.
fn aside_left(ctx: &egui::Context, app: &mut App) -> Result<f32, String> {
    interactive_rects(ctx, app)
        .into_iter()
        .filter(|(name, _)| name.starts_with("Tab") && name.contains("Properties"))
        .map(|(_, r)| r.left())
        .reduce(f32::min)
        .ok_or_else(|| "the Properties tab is not on screen".to_owned())
}

/// Controls of the aside body that come within 12 px of its left or right edge.
fn too_close_to_the_edge(ctx: &egui::Context, app: &mut App) -> Result<Vec<String>, String> {
    let left = aside_left(ctx, app)?;
    let right = 1440.0;
    let rects = interactive_rects(ctx, app);
    let tabs_bottom = rects
        .iter()
        .filter(|(name, r)| {
            name.starts_with("Tab")
                && (name.contains("Properties") || name.contains("Assistant"))
                && r.left() >= left - 0.5
        })
        .map(|(_, r)| r.bottom())
        .fold(0.0, f32::max);
    let inside: Vec<_> = rects
        .into_iter()
        .filter(|(_, r)| {
            r.left() >= left - 0.5
                && r.top() >= tabs_bottom
                && r.bottom() < 912.0
                && r.height() >= 1.0
        })
        .collect();
    if inside.len() < 3 {
        return Err(format!("only {} controls found in the aside", inside.len()));
    }
    Ok(inside
        .into_iter()
        .filter(|(_, r)| r.left() < left + 12.0 || r.right() > right - 12.0)
        .map(|(name, r)| format!("{name} at {:.0}..{:.0}", r.left() - left, right - r.right()))
        .collect())
}

#[test]
fn a_newcomer_never_sees_properties_touch_the_panel_edge() -> Result<(), String> {
    let (ctx, mut app) = practice();
    let part = app.project.components.first().ok_or("no part")?.id;
    app.select(Some(part));
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let close = too_close_to_the_edge(&ctx, &mut app)?;
    assert!(
        close.is_empty(),
        "a part's Properties reach the edge: {close:?}"
    );
    app.tree.selected_key = Some("nets".to_owned());
    app.select(None);
    app.tree.selected_key = Some("nets".to_owned());
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let close = too_close_to_the_edge(&ctx, &mut app)?;
    assert!(
        close.is_empty(),
        "a net's Properties reach the edge: {close:?}"
    );
    app.select_sheet_row(0, "title");
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let close = too_close_to_the_edge(&ctx, &mut app)?;
    assert!(
        close.is_empty(),
        "the title block's Properties reach the edge: {close:?}"
    );
    Ok(())
}
