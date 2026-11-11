//! Stories for the schematic page switcher: one page on the canvas at a time,
//! a way to add and name pages, and a dot on the pages a connection reaches.

use super::super::*;
use super::board::right_click;
use super::support::*;
use crate::schematic_port;

fn start() -> App {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tab = Tab::Schematic;
    app
}

fn add_part(app: &mut App, reference: &str, x: f64) {
    app.tool_call(
        "component.add",
        json!({"kind":"resistor","reference":reference,"position":{"x":x,"y":10.0}}),
    );
}

/// Text the schematic canvas itself paints on the current page (the side
/// docks also list every part, so they cannot be used to tell pages apart).
fn canvas_labels(app: &App) -> Vec<String> {
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
                let view = app.sheet_view();
                app.paint_schematic(ui.painter(), rect, app.schematic, &view);
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

#[test]
fn a_newcomer_sees_one_schematic_page_at_a_time() -> Result<(), String> {
    let mut app = start();
    add_part(&mut app, "R_ROOT", 10.0);
    add_part(&mut app, "R_PAGE", 40.0);
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "+")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert_eq!(app.project.sheets.len(), 1, "one click adds one page");
    let page = app.project.sheets[0].id;
    let part = app
        .project
        .components
        .iter()
        .find(|c| c.reference == "R_PAGE")
        .ok_or("second part missing")?
        .id;
    app.tool_call(
        "schematic.move_component",
        json!({"id": part, "sheet": page}),
    );
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert_eq!(app.schematic_page(), page, "a new page opens straight away");
    let labels = canvas_labels(&app);
    assert!(
        labels.iter().any(|t| t.contains("R_PAGE")),
        "the new page shows its own part: {labels:?}"
    );
    assert!(
        !labels.iter().any(|t| t.contains("R_ROOT")),
        "the new page hides the Main page's part: {labels:?}"
    );
    authoring::tests::tap(&ctx, &mut app, "Main")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert_eq!(app.schematic_page(), 0, "the Main tab goes back");
    let labels = canvas_labels(&app);
    assert!(labels.iter().any(|t| t.contains("R_ROOT")), "{labels:?}");
    assert!(
        labels.iter().any(|t| t == "Page 1"),
        "Main shows every page as one part: {labels:?}"
    );
    assert!(
        !labels.iter().any(|t| t.contains("R_PAGE")),
        "Main hides the items of a closed page: {labels:?}"
    );
    Ok(())
}

/// Height in points of the text `label` painted on the sheet at `zoom`, or None when it is not painted.
fn painted_text_height(app: &mut App, label: &str, zoom: f32) -> Option<f32> {
    app.schematic.zoom = zoom;
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
    output.shapes.iter().find_map(|shape| match &shape.shape {
        egui::Shape::Text(text) if text.galley.text() == label => Some(text.galley.size().y),
        _ => None,
    })
}

#[test]
fn a_newcomer_zooms_in_and_the_part_text_grows_with_the_drawing() -> Result<(), String> {
    let mut app = start();
    add_part(&mut app, "R_ZOOM", 20.0);
    let near = painted_text_height(&mut app, "R_ZOOM", 12.0)
        .ok_or("the reference is not painted at 12")?;
    let mid =
        painted_text_height(&mut app, "R_ZOOM", 6.0).ok_or("the reference is not painted at 6")?;
    assert!(
        near > mid * 1.5,
        "the reference is larger when zoomed in: {near} at 12 px/mm and {mid} at 6 px/mm"
    );
    let far = painted_text_height(&mut app, "R_ZOOM", 0.2)
        .ok_or("the reference is not painted far out")?;
    assert!(
        far > 0.0 && far <= mid,
        "far out it is still painted, no larger than before: {far}"
    );
    Ok(())
}

#[test]
fn a_newcomer_places_a_part_on_the_second_page_and_it_stays_there() -> Result<(), String> {
    let mut app = start();
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "+")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let page = app.project.sheets.first().ok_or("no page was added")?.id;
    assert_eq!(
        app.schematic_page(),
        page,
        "the new page is the one on screen"
    );
    // Pick the resistor tool and click the drawing, as a person does.
    app.tool = Tool::Place("resistor".into());
    let at = app
        .schematic
        .screen(Point::new(40.0, 10.0), app.canvas_rect);
    tap_at(&ctx, &mut app, at)?;
    let part = app.project.components.first().ok_or("nothing was placed")?;
    assert_eq!(
        part.sheet, page,
        "the part belongs to the page it was placed on"
    );
    let reference = part.reference.clone();
    let labels = canvas_labels(&app);
    assert!(
        labels.iter().any(|t| t.contains(&reference)),
        "the page shows the part it just got: {labels:?}"
    );
    authoring::tests::tap(&ctx, &mut app, "Main")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let labels = canvas_labels(&app);
    assert!(
        labels.iter().any(|t| t == "Page 1") && !labels.iter().any(|t| t.contains(&reference)),
        "Main shows the other page as one part: {labels:?}"
    );
    Ok(())
}

/// Where the last copy of `label` is painted (the topmost one), after a quiet frame.
fn text_at(ctx: &egui::Context, app: &mut App, label: &str) -> Option<egui::Pos2> {
    frame(ctx, app, vec![], egui::Modifiers::NONE);
    let output = frame(ctx, app, vec![], egui::Modifiers::NONE);
    let mut found = None;
    for shape in &output.shapes {
        if let egui::Shape::Text(text) = &shape.shape
            && text.galley.text() == label
        {
            found = Some(text.pos + text.galley.size() / 2.0);
        }
    }
    found
}

/// Rest the pointer on `pos` for a few frames, as a hand does before reading.
fn hover_at(ctx: &egui::Context, app: &mut App, pos: egui::Pos2) {
    for _ in 0..3 {
        frame(
            ctx,
            app,
            vec![egui::Event::PointerMoved(pos)],
            egui::Modifiers::NONE,
        );
    }
}

/// Type `text` over what the focused field holds (select all, then type).
fn type_over(ctx: &egui::Context, app: &mut App, text: &str) {
    frame(
        ctx,
        app,
        vec![egui::Event::Key {
            key: egui::Key::A,
            physical_key: Some(egui::Key::A),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::COMMAND,
        }],
        egui::Modifiers::COMMAND,
    );
    frame(
        ctx,
        app,
        vec![egui::Event::Text(text.to_owned())],
        egui::Modifiers::NONE,
    );
}

fn page_names(app: &App) -> Vec<String> {
    app.project.sheets.iter().map(|s| s.name.clone()).collect()
}

#[test]
fn a_newcomer_adds_a_page_with_the_plus_tab() -> Result<(), String> {
    let mut app = start();
    let ctx = egui::Context::default();
    assert!(
        paints_text(&ctx, &mut app, "Main")?,
        "the Main tab is there"
    );
    assert!(
        paints_text(&ctx, &mut app, "+")?,
        "the strip ends in a plus tab"
    );
    for gone in ["Add page", "Rename page", "Remove page"] {
        assert!(
            !paints_text(&ctx, &mut app, gone)?,
            "the strip has no {gone} button"
        );
    }
    authoring::tests::tap(&ctx, &mut app, "+")?;
    assert_eq!(page_names(&app), ["Page 1"], "one click adds one page");
    assert_eq!(
        app.schematic_page(),
        app.project.sheets[0].id,
        "the new page opens"
    );
    assert!(paints_text(&ctx, &mut app, "Page 1")?, "it gets a tab");
    Ok(())
}

#[test]
fn a_newcomer_renames_and_removes_a_page_in_properties() -> Result<(), String> {
    let mut app = start();
    add_part(&mut app, "R_KEEP", 10.0);
    let ctx = egui::Context::default();
    authoring::tests::tap(&ctx, &mut app, "+")?;
    let page = app.project.sheets[0].id;
    let part = app.project.components[0].id;
    app.tool_call(
        "schematic.move_component",
        json!({"id": part, "sheet": page}),
    );
    assert!(
        paints_text(&ctx, &mut app, "Schematic page")?,
        "the selected page shows its card in Properties"
    );
    type_into(&ctx, &mut app, "textinput:Title", "Power")?;
    assert_eq!(
        page_names(&app),
        ["Power"],
        "the Title field renames: {}",
        app.message
    );
    assert!(paints_text(&ctx, &mut app, "Power")?, "the tab follows");
    authoring::tests::tap(&ctx, &mut app, "Remove page")?;
    assert!(
        app.project.sheets.is_empty(),
        "the page is gone: {}",
        app.message
    );
    assert_eq!(app.project.components[0].sheet, 0, "its part moved to Main");
    assert_eq!(app.schematic_page(), 0, "the canvas falls back to Main");
    app.tool_call("undo", json!({}));
    assert_eq!(page_names(&app), ["Power"], "undo brings the page back");
    Ok(())
}

#[test]
fn a_newcomer_previews_page_properties_by_hovering_a_tree_row() -> Result<(), String> {
    let mut app = start();
    app.tool_call("schematic.new_sheet", json!({"name":"Power"}));
    let ctx = egui::Context::default();
    let row = text_at(&ctx, &mut app, "POWER").ok_or("the Power row is not in the tree")?;
    assert!(
        !paints_text(&ctx, &mut app, "Schematic page, preview")?,
        "nothing previews until the pointer rests on a row"
    );
    hover_at(&ctx, &mut app, row);
    assert!(
        paints_text(&ctx, &mut app, "Schematic page, preview")?,
        "resting on the row previews the page"
    );
    assert_eq!(app.tree.selected_key, None, "hovering selects nothing");
    assert_eq!(app.schematic_page(), 0, "hovering opens nothing");
    for _ in 0..3 {
        frame(
            &ctx,
            &mut app,
            vec![egui::Event::PointerGone],
            egui::Modifiers::NONE,
        );
    }
    assert!(
        !paints_text(&ctx, &mut app, "Schematic page, preview")?,
        "moving away clears the preview"
    );
    Ok(())
}

#[test]
fn a_newcomer_right_clicks_a_page_row_and_a_page_tab() -> Result<(), String> {
    let mut app = start();
    app.tool_call("schematic.new_sheet", json!({"name":"Power"}));
    let ctx = egui::Context::default();
    let tab = text_at(&ctx, &mut app, "Power").ok_or("the Power tab is not on screen")?;
    right_click(&ctx, &mut app, tab);
    for item in ["Add page", "Rename", "Remove page"] {
        assert!(
            paints_text(&ctx, &mut app, item)?,
            "the tab menu offers {item}"
        );
    }
    authoring::tests::tap(&ctx, &mut app, "Add page")?;
    assert_eq!(
        page_names(&app),
        ["Power", "Page 2"],
        "Add page from the tab menu"
    );
    let row = text_at(&ctx, &mut app, "POWER").ok_or("the Power row is not in the tree")?;
    right_click(&ctx, &mut app, row);
    for item in ["Add page", "Rename", "Remove page"] {
        assert!(
            paints_text(&ctx, &mut app, item)?,
            "the row menu offers {item}"
        );
    }
    authoring::tests::tap(&ctx, &mut app, "Remove page")?;
    assert_eq!(
        page_names(&app),
        ["Page 2"],
        "Remove page from the row menu"
    );
    Ok(())
}

#[test]
fn a_newcomer_presses_f2_to_rename_a_page() -> Result<(), String> {
    let mut app = start();
    app.tool_call("schematic.new_sheet", json!({"name":"Page 2"}));
    let page = app.project.sheets[0].id;
    let ctx = egui::Context::default();
    // In the Layers tree: F2 turns the row into a field; Enter renames.
    press_on(&ctx, &mut app, &App::page_row_key(page), egui::Key::F2);
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(app.tree.rename.is_some(), "F2 starts a rename on the row");
    type_over(&ctx, &mut app, "Power");
    key(&ctx, &mut app, egui::Key::Enter);
    assert_eq!(
        page_names(&app),
        ["Power"],
        "Enter commits: {}",
        app.message
    );
    assert!(app.tree.rename.is_none(), "the editor closes");
    // Escape cancels.
    press_on(&ctx, &mut app, &App::page_row_key(page), egui::Key::F2);
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    type_over(&ctx, &mut app, "Nope");
    key(&ctx, &mut app, egui::Key::Escape);
    assert_eq!(page_names(&app), ["Power"], "Escape keeps the old name");
    // On the page tab: select it, F2, type, Enter.
    authoring::tests::tap(&ctx, &mut app, "Power")?;
    key(&ctx, &mut app, egui::Key::F2);
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    type_over(&ctx, &mut app, "Audio");
    key(&ctx, &mut app, egui::Key::Enter);
    assert_eq!(
        page_names(&app),
        ["Audio"],
        "F2 on the tab renames: {}",
        app.message
    );
    app.tool_call("undo", json!({}));
    assert_eq!(
        page_names(&app),
        ["Power"],
        "one undo takes the rename back"
    );
    Ok(())
}

#[test]
fn a_newcomer_sees_which_pages_a_highlighted_connection_reaches() -> Result<(), String> {
    let mut app = start();
    app.tool_call("net.create", json!({"name":"VCC"}));
    let net = app.project.nets[0].id;
    app.tool_call("schematic.new_sheet", json!({"name":"Power"}));
    let page = app.project.sheets[0].id;
    app.tool_call(
        "schematic.place_port",
        json!({"name":"VCC","net":net,"position":{"x":20.0,"y":20.0},"sheet":page}),
    );
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        !paints_text(&ctx, &mut app, "\u{25cf} Power")?,
        "no dot while no connection is highlighted"
    );
    app.highlight_net = Some(net);
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        paints_text(&ctx, &mut app, "\u{25cf} Power")?,
        "the page with the VCC port gets a dot: {}",
        app.message
    );
    authoring::tests::tap(&ctx, &mut app, "\u{25cf} Power")?;
    assert_eq!(app.schematic_page(), page);
    assert_eq!(
        app.highlight_net,
        Some(net),
        "switching pages keeps the connection highlighted"
    );
    Ok(())
}

#[test]
fn a_newcomer_adds_a_second_page_and_carries_a_net_across() -> Result<(), String> {
    let mut app = start();
    app.tool_call("net.create", json!({"name":"SDA"}));
    let ctx = egui::Context::default();
    super::support::tap_strip(&ctx, &mut app, "icon:tool-port")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        app.port_pick_name == "SDA",
        "the port picker follows the new connection: {}",
        app.port_pick_name
    );
    app.canvas_click(Point::new(12.0, 12.0), app.schematic.zoom);
    authoring::tests::tap(&ctx, &mut app, "+")?;
    let child = app.project.sheets[0].id;
    app.tool_call(
        "component.add",
        json!({"kind":"resistor","reference":"R_CHILD","position":{"x":40.0,"y":40.0}}),
    );
    // component.add always lands on the Main page; move the part to the new page.
    let part = app
        .project
        .components
        .iter()
        .find(|c| c.reference == "R_CHILD")
        .ok_or("the child part was not added")?
        .id;
    app.tool_call(
        "schematic.move_component",
        json!({"id": part, "sheet": child}),
    );
    super::support::tap_strip(&ctx, &mut app, "icon:tool-port")?;
    app.canvas_click(Point::new(45.0, 45.0), app.schematic.zoom);
    assert_eq!(
        app.project.nets.iter().filter(|n| n.name == "SDA").count(),
        1,
        "both ports share one connection"
    );
    assert_eq!(
        app.project.ports.iter().filter(|p| p.name == "SDA").count(),
        2,
        "each page gets its own port: {}",
        app.message
    );
    authoring::tests::tap(&ctx, &mut app, "Main")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let labels = canvas_labels(&app);
    assert!(
        !labels.iter().any(|t| t.contains("R_CHILD")),
        "Main hides the child page part: {labels:?}"
    );
    assert!(
        labels.iter().any(|t| t.contains("[SDA]")),
        "Main shows the port placed here: {labels:?}"
    );
    assert!(
        labels.iter().any(|t| t == "Page 1") && labels.iter().any(|t| t == "SDA"),
        "Main shows the page as a part with a pin for its SDA net: {labels:?}"
    );
    // Press the Page 1 tab: the copy of the label nearest the top of the window.
    let output = frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let tab = output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.text() == "Page 1" => {
                Some(text.pos + text.galley.size() / 2.0)
            }
            _ => None,
        })
        .min_by(|a, b| a.y.total_cmp(&b.y))
        .ok_or("the Page 1 tab is not on screen")?;
    tap_at(&ctx, &mut app, tab)?;
    let labels = canvas_labels(&app);
    assert!(
        labels.iter().any(|t| t.contains("R_CHILD")),
        "the child page shows its part: {labels:?}"
    );
    assert_eq!(app.schematic_page(), child);
    Ok(())
}

#[test]
fn a_newcomer_opens_a_child_page_from_its_box() -> Result<(), String> {
    let mut app = start();
    let ctx = egui::Context::default();
    authoring::tests::tap(&ctx, &mut app, "+")?;
    let child = app.project.sheets[0].id;
    // A page only sits in a box on another page once it is placed there as a sheet symbol.
    let entry = Point::new(25.0, 35.0);
    app.tool_call(
        "schematic.set_sheet_parent",
        json!({"id": child, "parent": 0, "entry": {"x": entry.x, "y": entry.y}}),
    );
    authoring::tests::tap(&ctx, &mut app, "Main")?;
    assert_eq!(app.schematic_page(), 0, "the parent page is showing");
    let inside = Point::new(
        entry.x + schematic_port::CHILD_BOX_W / 2.0,
        entry.y + schematic_port::CHILD_BOX_H / 2.0,
    );
    let labels = canvas_labels(&app);
    assert!(
        labels.iter().any(|t| t.contains("Page 1")),
        "the child box shows the page name on Main: {labels:?}"
    );
    app.schematic_double_click(
        inside,
        app.schematic.zoom,
        crate::tool_modifiers::Held::default(),
    );
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert_eq!(
        app.schematic_page(),
        child,
        "double-clicking the box opens the child page"
    );
    assert!(
        paints_text(&ctx, &mut app, "Page 1")?,
        "the child tab stays selected"
    );
    Ok(())
}

#[test]
fn a_newcomer_adds_a_page_and_page_one_gets_no_extra_outline() -> Result<(), String> {
    let mut app = start();
    add_part(&mut app, "R_ROOT", 10.0);
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let before = canvas_labels(&app);
    authoring::tests::tap(&ctx, &mut app, "+")?;
    assert_eq!(page_names(&app), ["Page 1"], "one click adds one page");
    authoring::tests::tap(&ctx, &mut app, "Main")?;
    assert_eq!(app.schematic_page(), 0, "the Main tab goes back");
    let after = canvas_labels(&app);
    assert!(
        !after.iter().any(|t| t.contains("Page 1")),
        "page 1 shows no outline named after the new page: {after:?}"
    );
    assert_eq!(
        before.len(),
        after.len(),
        "adding a page draws nothing on the Main page: {before:?} then {after:?}"
    );
    assert!(
        app.project
            .sheets
            .iter()
            .all(|s| s.parent.is_none() && s.entry.is_none()),
        "a plain new page is not placed on any other page"
    );
    Ok(())
}

#[test]
fn a_newcomer_clicks_where_a_page_one_part_sits_on_page_two() -> Result<(), String> {
    let mut app = start();
    add_part(&mut app, "R_ROOT", 40.0);
    app.tool_call("net.create", json!({"name":"SDA"}));
    let net = app.project.nets[0].id;
    app.tool_call(
        "label.place",
        json!({"kind":"label","net":net,"position":{"x":70.0,"y":30.0},"rotation":0.0}),
    );
    assert_eq!(app.project.labels.len(), 1, "the net label is placed");
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let part = app
        .project
        .components
        .iter()
        .find(|c| c.reference == "R_ROOT")
        .ok_or("the part is missing")?
        .id;
    let at = app
        .schematic
        .screen(Point::new(40.0, 10.0), app.canvas_rect);
    assert!(
        app.canvas_rect.contains(at),
        "the part sits inside the drawing area"
    );
    tap_at(&ctx, &mut app, at)?;
    assert_eq!(
        app.selected,
        Some(part),
        "clicking the part on its own page selects it"
    );
    authoring::tests::tap(&ctx, &mut app, "+")?;
    assert_eq!(app.schematic_page(), app.project.sheets[0].id);
    assert_eq!(
        app.selected, None,
        "a new page starts with nothing selected"
    );
    tap_at(&ctx, &mut app, at)?;
    assert_eq!(
        app.selected, None,
        "clicking where the part of page 1 sits selects nothing on page 2"
    );
    hover_at(&ctx, &mut app, at);
    let world = app.schematic.world(at, app.canvas_rect);
    assert!(
        matches!(
            app.hit_at(world, app.schematic.zoom),
            canvas::gestures::Hit::Empty
        ),
        "resting the pointer there on page 2 finds nothing to act on"
    );
    let label_world = Point::new(70.0, 30.0);
    assert!(
        matches!(
            app.hit_at(label_world, app.schematic.zoom),
            canvas::gestures::Hit::Empty
        ),
        "the net label of page 1 cannot be picked on page 2"
    );
    authoring::tests::tap(&ctx, &mut app, "Main")?;
    assert!(
        matches!(
            app.hit_at(label_world, app.schematic.zoom),
            canvas::gestures::Hit::NetLabel(_)
        ),
        "the same net label is found again on page 1"
    );
    Ok(())
}

#[test]
fn a_newcomer_renames_page_one_and_every_place_shows_the_same_name() -> Result<(), String> {
    let mut app = start();
    let ctx = egui::Context::default();
    authoring::tests::tap(&ctx, &mut app, "+")?;
    authoring::tests::tap(&ctx, &mut app, "Main")?;
    assert_eq!(app.schematic_page(), 0, "page 1 is showing");
    type_into(&ctx, &mut app, "textinput:Title", "Amp")?;
    assert_eq!(
        app.project.root_sheet_name.as_deref(),
        Some("Amp"),
        "the Title field renames page 1: {}",
        app.message
    );
    assert!(
        canvas_labels(&app).iter().any(|t| t == "1 of 2")
            && canvas_labels(&app).iter().any(|t| t == "Amp"),
        "the title block shows the number and the page name as its title: {:?}",
        canvas_labels(&app)
    );
    assert!(
        paints_text(&ctx, &mut app, "Amp")?,
        "the tab strip shows the new name"
    );
    assert!(
        !paints_text(&ctx, &mut app, "Main")?,
        "the old name is gone from the tab strip"
    );
    assert!(
        paints_text(&ctx, &mut app, "AMP")?,
        "the Layers tree shows the same name"
    );
    assert!(
        !paints_text(&ctx, &mut app, "SHEET 1")?,
        "the Layers tree no longer calls page 1 a second name"
    );
    Ok(())
}

#[test]
fn a_newcomer_reaches_a_new_page_from_the_tab_strip_and_the_layers_tree() -> Result<(), String> {
    let mut app = start();
    let ctx = egui::Context::default();
    authoring::tests::tap(&ctx, &mut app, "+")?;
    let page = app.project.sheets[0].id;
    authoring::tests::tap(&ctx, &mut app, "Main")?;
    assert_eq!(app.schematic_page(), 0);
    authoring::tests::tap(&ctx, &mut app, "PAGE 1")?;
    assert_eq!(
        app.schematic_page(),
        page,
        "the Layers tree row opens the page"
    );
    authoring::tests::tap(&ctx, &mut app, "Main")?;
    authoring::tests::tap(&ctx, &mut app, "Page 1")?;
    assert_eq!(app.schematic_page(), page, "the tab opens the page");
    Ok(())
}

#[test]
fn a_newcomer_clicks_a_part_on_page_two_and_the_tree_goes_to_that_page_part() -> Result<(), String>
{
    let mut app = start();
    app.region().left_tab = "layers".to_owned();
    add_part(&mut app, "R1", 10.0);
    add_part(&mut app, "R2", 60.0);
    let ids: Vec<(Id, f64)> = app
        .project
        .components
        .iter()
        .map(|c| (c.id, c.position.x))
        .collect();
    assert_eq!(ids.len(), 2, "two parts are placed");
    let on_main = ids
        .iter()
        .find(|(_, x)| *x < 30.0)
        .ok_or("the Main part is missing")?
        .0;
    let on_page = ids
        .iter()
        .find(|(_, x)| *x > 30.0)
        .ok_or("the page part is missing")?
        .0;
    app.tool_call(
        "components.update",
        json!({"updates": [{"id": on_main, "value": "1k"}, {"id": on_page, "value": "22k"}]}),
    );
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "+")?;
    let page = app.project.sheets[0].id;
    app.tool_call(
        "schematic.move_component",
        json!({"id": on_page, "sheet": page}),
    );
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert_eq!(app.schematic_page(), page);
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let at = app
        .schematic
        .screen(Point::new(60.0, 10.0), app.canvas_rect);
    tap_at(&ctx, &mut app, at)?;
    assert_eq!(
        app.selected,
        Some(on_page),
        "the click selects the part of this page"
    );
    assert_eq!(app.value, "22k", "Properties shows the part clicked");
    assert_eq!(app.x, 60.0, "Properties shows the position of that part");
    let symbols = format!("sec:sheet:{page}/layer:symbols");
    let row = format!("{symbols}/comp:{on_page}");
    assert_eq!(
        app.tree.cursor.as_deref(),
        Some(row.as_str()),
        "the tree cursor sits on the part under its own page"
    );
    assert!(
        app.tree.expanded.contains(&format!("sec:sheet:{page}")),
        "the page row opens: {:?}",
        app.tree.expanded
    );
    assert!(
        app.tree.expanded.contains(&symbols),
        "the Symbols row of that page opens: {:?}",
        app.tree.expanded
    );
    Ok(())
}

/// A design with a Main part and a page that owns R_CHILD and a port on SDA; the view is Main.
fn main_with_a_page() -> Result<(egui::Context, App, Id), String> {
    let mut app = start();
    app.tool_call("net.create", json!({"name":"SDA"}));
    add_part(&mut app, "R_ROOT", 10.0);
    let ctx = egui::Context::default();
    authoring::tests::tap(&ctx, &mut app, "+")?;
    let child = app.project.sheets.first().ok_or("no page was added")?.id;
    app.tool_call(
        "component.add",
        json!({"kind":"resistor","reference":"R_CHILD","position":{"x":40.0,"y":40.0}}),
    );
    let part = app
        .project
        .components
        .iter()
        .find(|c| c.reference == "R_CHILD")
        .ok_or("the child part was not added")?
        .id;
    app.tool_call(
        "schematic.move_component",
        json!({"id": part, "sheet": child}),
    );
    let net = app
        .project
        .nets
        .iter()
        .find(|n| n.name == "SDA")
        .ok_or("the net is missing")?
        .id;
    app.tool_call(
        "schematic.place_port",
        json!({"name":"SDA","net":net,"sheet":child,"position":{"x":45.0,"y":45.0}}),
    );
    authoring::tests::tap(&ctx, &mut app, "Main")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    Ok((ctx, app, child))
}

/// Centre the view on `at` so a control drawn there is on screen.
fn look_at(ctx: &egui::Context, app: &mut App, at: Point) {
    app.schematic.zoom = 6.0;
    app.schematic.center = at;
    frame(ctx, app, vec![], egui::Modifiers::NONE);
}

#[test]
fn a_newcomer_opens_one_page_part_on_main_with_its_toggle() -> Result<(), String> {
    let (ctx, mut app, child) = main_with_a_page()?;
    let labels = canvas_labels(&app);
    assert!(
        !labels.iter().any(|t| t.contains("R_CHILD")),
        "a closed page hides its parts: {labels:?}"
    );
    assert!(
        labels.iter().any(|t| t.contains("R_ROOT")),
        "Main keeps its own parts: {labels:?}"
    );
    let part = crate::page_part::PART_BASE + child;
    let at = app
        .sheet_view()
        .component(part)
        .ok_or("the page part is missing")?
        .position;
    look_at(&ctx, &mut app, at);
    tap_named_holding(
        &ctx,
        &mut app,
        "Button Show items of Page 1",
        egui::Modifiers::NONE,
    )?;
    let labels = canvas_labels(&app);
    assert!(
        labels.iter().any(|t| t.contains("R_CHILD")),
        "opening the page shows its parts: {labels:?}"
    );
    assert!(
        !labels.iter().any(|t| t == "SDA"),
        "the page part is gone once the page is open: {labels:?}"
    );
    let origin = app
        .sheet_view()
        .component(
            app.project
                .components
                .iter()
                .find(|c| c.reference == "R_CHILD")
                .ok_or("the child part is missing")?
                .id,
        )
        .ok_or("the placed child part is missing")?
        .position;
    look_at(&ctx, &mut app, origin);
    tap_named_holding(
        &ctx,
        &mut app,
        "Button Hide items of Page 1",
        egui::Modifiers::NONE,
    )?;
    let labels = canvas_labels(&app);
    assert!(
        !labels.iter().any(|t| t.contains("R_CHILD")),
        "closing the page folds it into one part again: {labels:?}"
    );
    Ok(())
}

#[test]
fn a_newcomer_opens_every_page_with_the_main_toolbar_toggle() -> Result<(), String> {
    let (ctx, mut app, _) = main_with_a_page()?;
    tap_named_holding(
        &ctx,
        &mut app,
        "CheckBox Open all pages",
        egui::Modifiers::NONE,
    )?;
    let labels = canvas_labels(&app);
    assert!(
        labels.iter().any(|t| t.contains("R_CHILD")),
        "the toolbar toggle opens every page: {labels:?}"
    );
    tap_named_holding(
        &ctx,
        &mut app,
        "CheckBox Open all pages",
        egui::Modifiers::NONE,
    )?;
    let labels = canvas_labels(&app);
    assert!(
        !labels.iter().any(|t| t.contains("R_CHILD")),
        "turning it off folds the pages again: {labels:?}"
    );
    Ok(())
}

#[test]
fn a_newcomer_wires_a_main_pin_to_a_page_part_pin_and_joins_the_net() -> Result<(), String> {
    let (_ctx, mut app, child) = main_with_a_page()?;
    let net = app
        .project
        .nets
        .iter()
        .find(|n| n.name == "SDA")
        .ok_or("the net is missing")?
        .id;
    let root = app
        .project
        .components
        .iter()
        .find(|c| c.reference == "R_ROOT")
        .ok_or("the Main part is missing")?
        .id;
    let part = crate::page_part::PART_BASE + child;
    let (from, to) = {
        let view = app.sheet_view();
        let pin = view
            .component(part)
            .and_then(|c| c.pins.first())
            .ok_or("the page part has no SDA pin")?;
        assert_eq!(pin.net, Some(net), "the pin carries the SDA net");
        let to = view
            .pin_position(&PinRef {
                component: part,
                pin: pin.number.clone(),
                unit: None,
            })
            .ok_or("the part pin has no place")?;
        let from = view
            .pin_position(&PinRef {
                component: root,
                pin: "1".into(),
                unit: None,
            })
            .ok_or("the Main pin has no place")?;
        (from, to)
    };
    let zoom = app.schematic.zoom;
    app.canvas_click(from, zoom);
    app.canvas_click(to, zoom);
    let joined = app
        .project
        .component(root)
        .and_then(|c| c.pins.iter().find(|p| p.number == "1"))
        .and_then(|p| p.net);
    assert_eq!(
        joined,
        Some(net),
        "the Main pin joins the net of the page part pin: {}",
        app.message
    );
    Ok(())
}
