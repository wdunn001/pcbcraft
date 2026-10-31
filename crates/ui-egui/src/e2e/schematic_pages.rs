//! Stories for the schematic page switcher: one page on the canvas at a time,
//! a way to add and name pages, and a dot on the pages a connection reaches.

use super::super::*;
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
    authoring::tests::tap(&ctx, &mut app, "Add page")?;
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

#[test]
fn a_newcomer_renames_and_removes_a_page() -> Result<(), String> {
    let mut app = start();
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Add page")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Rename page")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        paints_text(&ctx, &mut app, "Use name")?,
        "rename opens a name box with a button to confirm"
    );
    let id = app.project.sheets[0].id;
    app.tool_call("schematic.rename_sheet", json!({"id": id, "name": "Power"}));
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert_eq!(app.project.sheets[0].name, "Power");
    authoring::tests::tap(&ctx, &mut app, "Cancel")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Remove page")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(app.project.sheets.is_empty(), "the page is gone");
    assert_eq!(app.schematic_page(), 0, "the canvas falls back to Main");
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
