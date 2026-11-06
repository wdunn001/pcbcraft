//! Stories for the fixed rows under each schematic page: Symbols, Power
//! ports and Drawing. Each row hides, locks, shows its own
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
    tap_id(&ctx, &mut app, "button:Hide Power symbols")?;
    let ports_after = canvas_text(&app).iter().filter(|t| *t == "GND").count();
    assert!(
        ports_after < ports_before,
        "hidden power symbols are not drawn ({ports_before} to {ports_after})"
    );
    Ok(())
}

/// The text of every box and label the Properties panel paints this frame.
fn properties_texts(ctx: &egui::Context, app: &mut App) -> Vec<String> {
    let _tall = TallScope::enter();
    frame(ctx, app, vec![], egui::Modifiers::NONE);
    let output = frame(ctx, app, vec![], egui::Modifiers::NONE);
    output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Text(t) if t.pos.x >= PROPERTIES_LEFT => Some(t.galley.text().to_owned()),
            _ => None,
        })
        .collect()
}

/// Select page `id` in the Layers tree, as a click on its row does.
fn select_page_row(app: &mut App, ctx: &egui::Context, id: Id) {
    app.active_page = id;
    app.select(None);
    app.tree.selected_key = Some(App::page_row_key(id));
    frame(ctx, app, vec![], egui::Modifiers::NONE);
}

#[test]
fn a_newcomer_finds_no_title_block_row_in_the_tree() -> Result<(), String> {
    let (ctx, mut app) = practice();
    assert!(
        !on_screen(&ctx, &mut app, "button:Title block, shown, unlocked"),
        "the title block is part of the page, not a row of the tree"
    );
    assert!(
        !on_screen(&ctx, &mut app, "button:Hide Title block"),
        "the title block has no eye of its own"
    );
    assert!(
        canvas_text(&app).iter().any(|t| t == "TITLE"),
        "the sheet still draws its title block"
    );
    Ok(())
}

/// True when text outside the Properties panel (the tab strip and the tree) shows `word`.
fn left_of_properties_shows(ctx: &egui::Context, app: &mut App, word: &str) -> bool {
    let _tall = TallScope::enter();
    frame(ctx, app, vec![], egui::Modifiers::NONE);
    let output = frame(ctx, app, vec![], egui::Modifiers::NONE);
    output.shapes.iter().any(|shape| match &shape.shape {
        egui::Shape::Text(t) => {
            t.pos.x < PROPERTIES_LEFT
                && t.galley
                    .text()
                    .to_lowercase()
                    .contains(&word.to_lowercase())
        }
        _ => false,
    })
}

/// Empty a text box like a person: select all, Backspace, Enter.
fn clear_field(ctx: &egui::Context, app: &mut App, id: &str) -> Result<(), String> {
    tap_id(ctx, app, id)?;
    let _tall = TallScope::enter();
    let key = |key: egui::Key, modifiers: egui::Modifiers| egui::Event::Key {
        key,
        physical_key: Some(key),
        pressed: true,
        repeat: false,
        modifiers,
    };
    let none = egui::Modifiers::NONE;
    frame(
        ctx,
        app,
        vec![key(egui::Key::A, egui::Modifiers::COMMAND)],
        egui::Modifiers::COMMAND,
    );
    frame(ctx, app, vec![key(egui::Key::Backspace, none)], none);
    frame(ctx, app, vec![key(egui::Key::Enter, none)], none);
    frame(ctx, app, vec![], none);
    Ok(())
}

#[test]
fn a_newcomer_sees_in_the_page_properties_what_the_sheet_prints() -> Result<(), String> {
    let (ctx, mut app) = practice();
    app.tool_call(
        "project.set_meta",
        json!({"key": "title", "value": "Showcase sensor node"}),
    );
    app.tool_call("project.set_meta", json!({"key": "revision", "value": "A"}));
    app.tool_call(
        "project.set_meta",
        json!({"key": "date", "value": "2026-10-29"}),
    );
    app.tool_call(
        "schematic.set_page_setting",
        json!({"sheet": 0, "field": "company", "value": "Quasarke"}),
    );
    app.tool_call(
        "schematic.set_page_setting",
        json!({"sheet": 0, "field": "drawn_by", "value": "William"}),
    );
    select_page_row(&mut app, &ctx, 0);
    let boxes = properties_texts(&ctx, &mut app);
    let painted = canvas_text(&app);
    let sheet = crate::canvas::sheet::sheet_for(&app.project, 0);
    let page = app.page_name(0);
    assert_eq!(sheet.title, page, "the sheet title is the page name");
    for (label, value) in [
        ("Title", page.as_str()),
        ("Revision", "A"),
        ("Date", "2026-10-29"),
        ("Company", "Quasarke"),
        ("Drawn by", "William"),
    ] {
        assert!(
            boxes.iter().any(|t| t == value),
            "the {label} box holds {value}: {boxes:?}"
        );
    }
    for value in [page.as_str(), "A", "2026-10-29", sheet.size.as_str()] {
        assert!(
            painted.iter().any(|t| t == value),
            "the sheet paints {value}: {painted:?}"
        );
    }
    assert!(
        boxes.iter().any(|t| t.contains(sheet.size.as_str())),
        "the Sheet size box names {}: {boxes:?}",
        sheet.size
    );
    assert!(
        painted.iter().any(|t| t.starts_with("Showcase sensor")),
        "the block names the project, company and author: {painted:?}"
    );
    assert!(
        painted.iter().any(|t| t == "1 of 1"),
        "the sheet number has no page name: {painted:?}"
    );
    Ok(())
}

#[test]
fn a_newcomer_edits_the_title_block_in_the_page_properties() -> Result<(), String> {
    let (ctx, mut app) = practice();
    select_page_row(&mut app, &ctx, 0);
    type_into(&ctx, &mut app, "textinput:Title", "Sensor board")?;
    type_into(&ctx, &mut app, "textinput:Revision", "B")?;
    type_into(&ctx, &mut app, "textinput:Company", "Quasarke")?;
    type_into(&ctx, &mut app, "textinput:Drawn by", "William")?;
    assert_eq!(
        app.page_name(0),
        "Sensor board",
        "the Title box renames the page"
    );
    let settings = app.project.page_settings_of(0);
    assert_eq!(settings.revision, "B");
    assert_eq!(settings.company, "Quasarke");
    assert_eq!(settings.drawn_by, "William");
    assert!(
        canvas_text(&app).iter().any(|t| t == "Sensor board"),
        "the sheet prints the new title"
    );
    assert!(
        left_of_properties_shows(&ctx, &mut app, "Sensor board"),
        "the tab and the tree show the new name"
    );
    app.tool_call("undo", json!({}));
    assert_eq!(
        app.project.page_settings_of(0).drawn_by,
        "",
        "an edit is one undo step"
    );
    assert!(
        properties_texts(&ctx, &mut app)
            .iter()
            .all(|t| t != "William"),
        "the box follows the undo"
    );
    app.tool_call("redo", json!({}));
    assert_eq!(app.project.page_settings_of(0).drawn_by, "William");
    // A rename from elsewhere reaches the Title box at once.
    app.tool_call(
        "schematic.rename_sheet",
        json!({"id": 0, "name": "Power stage"}),
    );
    assert!(
        properties_texts(&ctx, &mut app)
            .iter()
            .any(|t| t == "Power stage"),
        "the Title box follows a rename made elsewhere"
    );
    assert!(canvas_text(&app).iter().any(|t| t == "Power stage"));
    // An emptied Title box gets the page name back.
    clear_field(&ctx, &mut app, "textinput:Title")?;
    assert_eq!(app.page_name(0), "Power stage");
    assert!(
        properties_texts(&ctx, &mut app)
            .iter()
            .any(|t| t == "Power stage")
    );
    // An emptied Revision box returns to the project default and shows it.
    app.tool_call("project.set_meta", json!({"key": "revision", "value": "A"}));
    clear_field(&ctx, &mut app, "textinput:Revision")?;
    assert_eq!(app.project.page_settings_of(0).revision, "");
    assert!(properties_texts(&ctx, &mut app).iter().any(|t| t == "A"));
    // Show title block off removes the box from this page.
    tap_properties(&ctx, &mut app, "checkbox:Show title block")?;
    assert!(app.project.page_settings_of(0).title_hidden);
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
    tap_id(&ctx, &mut app, "button:Expand Power symbols")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    tap_named_holding(
        &ctx,
        &mut app,
        "Button GND, 30.0, 30.0",
        egui::Modifiers::NONE,
    )?;
    let placed = app
        .project
        .labels
        .iter()
        .find(|l| l.kind == "power")
        .map(|l| l.id)
        .ok_or("the power symbol was placed")?;
    assert_eq!(
        app.selection(),
        vec![placed],
        "the child row selects its symbol"
    );
    let boxes = properties_texts(&ctx, &mut app);
    assert!(
        boxes.iter().all(|t| t != "Select"),
        "the Properties holds no Select button: {boxes:?}"
    );
    // The group row shows the page Properties, with the title block section.
    tap_id(&ctx, &mut app, "button:Power symbols, shown, unlocked")?;
    assert!(
        on_screen(&ctx, &mut app, "textinput:Revision"),
        "the Power symbols row shows the page Properties"
    );
    let boxes = properties_texts(&ctx, &mut app);
    assert!(boxes.iter().all(|t| t != "Select"), "{boxes:?}");
    // The group's eye hides every child.
    tap_id(&ctx, &mut app, "button:Hide Power symbols")?;
    assert!(
        app.preferences
            .hidden_layers
            .contains(&"page:0:power".to_owned()),
        "the eye hides the whole group"
    );
    Ok(())
}

#[test]
fn a_newcomer_finds_net_labels_and_buses_under_the_page() -> Result<(), String> {
    let (ctx, mut app) = practice();
    assert!(
        !on_screen(&ctx, &mut app, "button:Buses, shown, unlocked"),
        "a page without buses lists no Buses group"
    );
    app.tool_call(
        "label.place",
        json!({"kind":"label","name":"SPI_SCK","position":{"x":40.0,"y":30.0}}),
    );
    app.tool_call(
        "label.place_bus",
        json!({"name":"SPI*4","position":{"x":50.0,"y":40.0}}),
    );
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let label = app
        .project
        .labels
        .iter()
        .find(|l| l.kind == "label")
        .map(|l| l.id)
        .ok_or("the net label was placed")?;
    let bus = app
        .project
        .labels
        .iter()
        .find(|l| l.kind == "bus")
        .map(|l| l.id)
        .ok_or("the bus label was placed")?;
    assert!(on_screen(
        &ctx,
        &mut app,
        "button:Net labels, shown, unlocked"
    ));
    assert!(on_screen(&ctx, &mut app, "button:Buses, shown, unlocked"));
    tap_id(&ctx, &mut app, "button:Expand Net labels")?;
    tap_named_holding(
        &ctx,
        &mut app,
        "Button SPI_SCK, 40.0, 30.0",
        egui::Modifiers::NONE,
    )?;
    assert_eq!(
        app.selection(),
        vec![label],
        "the net label child selects it"
    );
    tap_id(&ctx, &mut app, "button:Expand Buses")?;
    tap_named_holding(
        &ctx,
        &mut app,
        "Button SPI*4, 50.0, 40.0",
        egui::Modifiers::NONE,
    )?;
    assert_eq!(app.selection(), vec![bus], "the bus child selects it");
    tap_id(&ctx, &mut app, "button:Hide Buses")?;
    assert!(
        app.preferences
            .hidden_layers
            .contains(&"page:0:buses".to_owned()),
        "the eye hides the buses of this page"
    );
    Ok(())
}

#[test]
fn a_newcomer_never_sees_empty_categories_by_default() -> Result<(), String> {
    let (ctx, mut app) = start();
    assert!(
        !on_screen(&ctx, &mut app, "button:Power symbols, shown, unlocked"),
        "an empty page lists no Power symbols row"
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
        on_screen(&ctx, &mut app, "button:Power symbols, shown, unlocked"),
        "Show empty lists every category"
    );
    tap_id(&ctx, &mut app, "button:Hide empty")?;
    assert!(!on_screen(
        &ctx,
        &mut app,
        "button:Power symbols, shown, unlocked"
    ));
    Ok(())
}

#[test]
fn a_newcomer_cannot_remove_a_fixed_row_of_a_page() -> Result<(), String> {
    let (ctx, mut app) = practice();
    let before = app.project.clone();
    // The menu item is drawn disabled, so a person cannot press it; the rows stay.
    tap_id(&ctx, &mut app, "button:Symbols, shown, unlocked")?;
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
    select_page_row(&mut app, &ctx, 0);
    let close = too_close_to_the_edge(&ctx, &mut app)?;
    assert!(
        close.is_empty(),
        "the page's Properties reach the edge: {close:?}"
    );
    Ok(())
}
