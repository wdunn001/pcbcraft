//! Stories for the schematic page switcher: one page on the canvas at a time,
//! a way to add and name pages, and a dot on the pages a connection reaches.

use super::super::*;
use super::board::right_click;
use super::support::*;

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
    assert!(!labels.iter().any(|t| t.contains("R_PAGE")), "{labels:?}");
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
    type_into(&ctx, &mut app, "textinput:Name", "Power")?;
    assert_eq!(
        page_names(&app),
        ["Power"],
        "the Name field renames: {}",
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
