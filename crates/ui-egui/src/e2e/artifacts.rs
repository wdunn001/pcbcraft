use super::super::*;
use super::support::*;

#[test]
fn a_newcomer_prints_a_drawing_of_the_board() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    let folder = std::env::temp_dir().join(format!("pcbcraftdraw-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    app.make_dir = folder.to_string_lossy().into_owned();
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    for _ in 0..6 {
        frame(
            &ctx,
            &mut app,
            vec![
                egui::Event::PointerMoved(egui::pos2(1300.0, 500.0)),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Line,
                    delta: egui::vec2(0.0, -4.0),
                    phase: egui::TouchPhase::Move,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            egui::Modifiers::NONE,
        );
    }
    authoring::tests::tap(&ctx, &mut app, "Settings")?;
    scrolled_to_at(&ctx, &mut app, "Printable drawing (PDF)", 700.0)?;
    authoring::tests::tap(&ctx, &mut app, "Printable drawing (PDF)")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        folder.join("board.pdf").is_file(),
        "one click leaves a printable drawing on disk: {}",
        app.message
    );
    authoring::tests::tap(&ctx, &mut app, "Web drawing (SVG)")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        folder.join("board.svg").is_file(),
        "the web drawing lands next to the printable one: {}",
        app.message
    );
    assert!(
        app.message.contains("Web drawing saved"),
        "the app says where the drawing went: {}",
        app.message
    );
    let _ = std::fs::remove_dir_all(&folder);
    Ok(())
}
#[test]
fn a_newcomer_pours_a_block_of_copper() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    let before = app.project.board.zones.len();
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Settings")?;
    assert!(
        scrolled_to_at(&ctx, &mut app, "Pour copper", 700.0)?,
        "the board geometry section should offer a copper pour: {}",
        app.message
    );
    app.pour_x = 2.0;
    app.pour_y = 2.0;
    app.pour_w = 6.0;
    app.pour_h = 4.0;
    authoring::tests::tap(&ctx, &mut app, "Pour copper")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.refresh();
    assert_eq!(
        app.project.board.zones.len(),
        before + 1,
        "one rectangle of copper is poured: {}",
        app.message
    );
    let zone = app
        .project
        .board
        .zones
        .last()
        .ok_or("the pour should be on the board")?;
    assert_eq!(
        zone.net, app.pour_net,
        "the pour went to the connection picked in the box"
    );
    assert!(
        zone.outline
            .iter()
            .all(|p| p.x >= 2.0 && p.x <= 8.0 && p.y >= 2.0 && p.y <= 6.0)
            && zone.outline.iter().any(|p| (p.x - 8.0).abs() < 1e-9),
        "the copper covers exactly the rectangle that was typed"
    );
    Ok(())
}
#[test]
fn a_newcomer_fills_a_copper_area_with_vias() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    let net = app
        .project
        .nets
        .first()
        .map(|n| n.id)
        .ok_or("the practice design should have a net")?;
    app.tool_call(
        "zone.place",
        json!({"net": net, "layer":"F.Cu","outline":[
            {"x":2.0,"y":2.0},{"x":8.0,"y":2.0},{"x":8.0,"y":6.0},{"x":2.0,"y":6.0},{"x":2.0,"y":2.0}
        ]}),
    );
    let zone_id = app
        .project
        .board
        .zones
        .last()
        .map(|z| z.id)
        .ok_or("the poured area should be on the board")?;
    let vias_before = app.project.board.vias.len();
    app.select(Some(zone_id));
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        paints_text(&ctx, &mut app, "Fill with vias")?,
        "a chosen copper area offers to be stitched: {}",
        app.message
    );
    authoring::tests::tap(&ctx, &mut app, "Fill with vias")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.refresh();
    let stitched = app.project.board.vias.len() - vias_before;
    assert!(
        stitched >= 4,
        "a grid of connecting holes lands inside the area: {}",
        app.message
    );
    assert!(
        app.project
            .board
            .vias
            .iter()
            .rev()
            .take(stitched)
            .all(|v| v.net == net),
        "every new hole belongs to the area's connection"
    );
    Ok(())
}
#[test]
fn a_newcomer_writes_a_drill_map() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    let folder = std::env::temp_dir().join(format!("pcbcraft-drill-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    app.make_dir = folder.to_string_lossy().into_owned();
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Settings")?;
    assert!(
        scrolled_to_at(&ctx, &mut app, "Drill map (CSV)", 700.0)?,
        "the make files should include a drill map: {}",
        app.message
    );
    authoring::tests::tap(&ctx, &mut app, "Drill map (CSV)")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let map = folder.join("board-drill-tools.csv");
    let text = std::fs::read_to_string(&map)
        .map_err(|e| format!("the drill map should land in the make folder: {e}"))?;
    assert!(
        !text.is_empty(),
        "the drill map counts the holes on the board"
    );
    let _ = std::fs::remove_dir_all(&folder);
    Ok(())
}
#[test]
fn a_newcomer_gets_a_3d_model_of_the_board() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    let folder = std::env::temp_dir().join(format!("pcbcraft3d-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    app.make_dir = folder.to_string_lossy().into_owned();
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    for _ in 0..6 {
        frame(
            &ctx,
            &mut app,
            vec![
                egui::Event::PointerMoved(egui::pos2(1300.0, 500.0)),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Line,
                    delta: egui::vec2(0.0, -4.0),
                    phase: egui::TouchPhase::Move,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            egui::Modifiers::NONE,
        );
    }
    authoring::tests::tap(&ctx, &mut app, "Settings")?;
    assert!(
        scrolled_to_at(&ctx, &mut app, "BOARD DRAWINGS", 700.0)?,
        "the factory folder section should also offer a 3D look: {}",
        app.message
    );
    authoring::tests::tap(&ctx, &mut app, "3D model (Step)")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        folder.join("board.step").is_file(),
        "one click leaves a Step model on disk: {}",
        app.message
    );
    authoring::tests::tap(&ctx, &mut app, "3D viewer file")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        folder.join("board.wrl").is_file(),
        "the viewer file lands next to the model: {}",
        app.message
    );
    assert!(
        app.message.contains("3D viewer file saved"),
        "the app says where the file went: {}",
        app.message
    );
    let _ = std::fs::remove_dir_all(&folder);
    Ok(())
}
#[test]
fn a_newcomer_plans_a_panel_of_board_copies() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    for _ in 0..8 {
        frame(
            &ctx,
            &mut app,
            vec![
                egui::Event::PointerMoved(egui::pos2(1300.0, 500.0)),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Line,
                    delta: egui::vec2(0.0, -4.0),
                    phase: egui::TouchPhase::Move,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            egui::Modifiers::NONE,
        );
    }
    authoring::tests::tap(&ctx, &mut app, "Settings")?;
    assert!(
        scrolled_to_at(&ctx, &mut app, "PANEL PLAN", 700.0)?,
        "the factory folder section should also plan panels: {}",
        app.message
    );
    let width = app.project.board.width;
    authoring::tests::tap(&ctx, &mut app, "Preview panel")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        paints_text(&ctx, &mut app, "4 copies")?,
        "a two by two preview counts four boards: {}",
        app.message
    );
    assert!(
        paints_text(&ctx, &mut app, "panel")?,
        "the panel's finished size is spelled out: {}",
        app.message
    );
    app.refresh();
    assert_eq!(
        app.project.board.width, width,
        "planning a panel never touches the design"
    );
    Ok(())
}

#[test]
fn a_newcomer_opens_their_factory_zip_and_hides_a_layer() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    let directory = std::env::temp_dir().join(format!("pcbcraft-fab-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    app.make_dir = directory.to_string_lossy().into_owned();
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Settings")?;
    authoring::tests::tap(&ctx, &mut app, "Copper & drill files")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Plot files")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.plot_viewer.folder_path = directory.to_string_lossy().into_owned();
    authoring::tests::tap(&ctx, &mut app, "Load")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        paints_text(&ctx, &mut app, "pcb-F.Cu.gbr")?,
        "front copper should appear in the layer list: {}",
        app.message
    );
    assert!(
        paints_text(&ctx, &mut app, "pcb-B.Cu.gbr")?,
        "back copper should appear in the layer list: {}",
        app.message
    );
    let visible_before = app.plot_viewer.visible_layer_count();
    assert!(visible_before >= 2, "at least two layers should show");
    authoring::tests::tap(&ctx, &mut app, "pcb-F.Cu.gbr")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        paints_text(&ctx, &mut app, "pcb-F.Cu.gbr (hidden)")?,
        "turning a layer off should mark it hidden in the list"
    );
    assert!(
        app.plot_viewer.visible_layer_count() < visible_before,
        "one fewer layer should be visible on the canvas"
    );
    let _ = std::fs::remove_dir_all(&directory);
    Ok(())
}
