//! Stories for the unified Layers tree in the left aside (#756).
use super::super::*;
use super::board::right_click;
use super::support::*;

fn pcb_app() -> (egui::Context, App) {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    app.change_tab(Tab::Pcb);
    app.refresh();
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    (ctx, app)
}

#[test]
fn a_newcomer_finds_anything_through_one_tree() -> Result<(), String> {
    let (ctx, mut app) = pcb_app();
    assert!(
        paints_text(&ctx, &mut app, "Layer 1 · Top")?,
        "the tree opens on the top layer: {}",
        app.message
    );
    assert!(
        paints_text(&ctx, &mut app, "Copper")?,
        "the copper sublayer is a row of the tree: {}",
        app.message
    );
    // Search a net, select it, see it highlighted.
    let net = app.project.nets[0].clone();
    app.tree.query = net.name.clone();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    press_on(
        &ctx,
        &mut app,
        &format!("nets/net:{}", net.id),
        egui::Key::Enter,
    );
    assert_eq!(
        app.highlight_net,
        Some(net.id),
        "selecting a net row lights the net on the drawing: {}",
        app.message
    );
    app.tree.query.clear();
    // Focus a layer: the others dim.
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    press_on(&ctx, &mut app, "side:top/copper", egui::Key::F);
    assert_eq!(
        app.focus_layer.as_deref(),
        Some("copper:front"),
        "F focuses the layer under the cursor: {}",
        app.message
    );
    assert!(
        paints_text(&ctx, &mut app, "FOCUSED ON Top copper")?,
        "the drawing says what is focused: {}",
        app.message
    );
    key(&ctx, &mut app, egui::Key::F);
    assert_eq!(app.focus_layer, None, "F again brings everything back");
    // The board outline row opens the board settings in Properties.
    press_on(&ctx, &mut app, "root", egui::Key::Enter);
    assert!(
        paints_text(&ctx, &mut app, "Board settings")?,
        "the board row opens board settings: {}",
        app.message
    );
    Ok(())
}

#[test]
fn a_newcomer_hides_and_locks_a_layer_from_the_tree() -> Result<(), String> {
    let (ctx, mut app) = pcb_app();
    press_on(&ctx, &mut app, "side:top/copper", egui::Key::Space);
    assert!(
        app.preferences
            .hidden_layers
            .iter()
            .any(|h| h == "Top copper"),
        "Space hides the layer: {}",
        app.message
    );
    key(&ctx, &mut app, egui::Key::Space);
    assert!(
        !app.preferences
            .hidden_layers
            .iter()
            .any(|h| h == "Top copper"),
        "Space shows it again"
    );
    app.project.board.tracks.push(pcbcraft_doc::Track {
        id: 9_999,
        net: app.project.nets[0].id,
        start: Point::new(10.0, 10.0),
        end: Point::new(20.0, 10.0),
        width: 0.25,
        layer: pcbcraft_doc::Layer::Front,
        arc: None,
    });
    let spot = Point::new(15.0, 10.0);
    assert_eq!(app.pick_object(spot, 10.0), Some(9_999));
    key(&ctx, &mut app, egui::Key::L);
    assert!(app.tree.locked.contains("copper:front"));
    assert_eq!(
        app.pick_object(spot, 10.0),
        None,
        "a locked layer cannot be picked on the drawing"
    );
    key(&ctx, &mut app, egui::Key::L);
    assert_eq!(app.pick_object(spot, 10.0), Some(9_999));
    Ok(())
}

#[test]
fn selecting_on_the_canvas_reveals_the_row_in_the_tree() -> Result<(), String> {
    let (ctx, mut app) = pcb_app();
    app.tree.expanded.remove("components");
    let id = app.project.board.footprints[0].id;
    app.select(Some(id));
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        app.tree.expanded.contains("components"),
        "the Parts group opens for a part picked on the drawing"
    );
    assert_eq!(
        app.tree.cursor.as_deref(),
        Some(format!("components/fp:{id}").as_str())
    );
    let reference = app
        .project
        .component(app.project.board.footprints[0].component)
        .map(|c| c.reference.clone())
        .ok_or("no part")?;
    assert!(
        paints_text(&ctx, &mut app, &reference)?,
        "the part's row is on screen: {}",
        app.message
    );
    Ok(())
}

#[test]
fn a_board_of_two_thousand_parts_keeps_the_tree_interactive() -> Result<(), String> {
    let (ctx, mut app) = pcb_app();
    let template = app.project.board.footprints[0].clone();
    for i in 0..2000u64 {
        let mut f = template.clone();
        f.id = 1_000_000 + i;
        app.project.board.footprints.push(f);
    }
    app.revision = app.revision.wrapping_add(1);
    app.tree.expanded.insert("components".to_owned());
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let start = std::time::Instant::now();
    for _ in 0..5 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    let each = start.elapsed() / 5;
    assert!(
        each.as_millis() < 400,
        "a frame with 2,000 parts took {each:?}"
    );
    Ok(())
}

/// Where the painted text `label` sits on the current frame, if anywhere.
fn text_at(ctx: &egui::Context, app: &mut App, label: &str) -> Option<egui::Pos2> {
    let output = frame(ctx, app, vec![], egui::Modifiers::NONE);
    let mut found = None;
    for shape in &output.shapes {
        if let egui::Shape::Text(text) = &shape.shape
            && text.galley.text() == label
        {
            found.get_or_insert(text.pos + text.galley.size() / 2.0);
        }
    }
    found
}

#[test]
fn a_newcomer_hides_a_layer_from_its_right_click_menu() -> Result<(), String> {
    let (ctx, mut app) = pcb_app();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let row = text_at(&ctx, &mut app, "Copper").ok_or("the Copper row is not on screen")?;
    right_click(&ctx, &mut app, row);
    authoring::tests::tap(&ctx, &mut app, "Hide  Space")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        app.preferences
            .hidden_layers
            .iter()
            .any(|h| h == "Top copper"),
        "the menu hides the layer on the drawing: {}",
        app.message
    );
    Ok(())
}

/// The track and graphic counts a drawing story compares before and after.
fn drawn(app: &App) -> (usize, usize, usize) {
    (
        app.project.board.tracks.len(),
        app.project.board.zones.len(),
        app.project.graphics.len(),
    )
}

#[test]
fn a_newcomer_draws_silkscreen_by_picking_the_silkscreen_layer() -> Result<(), String> {
    use crate::draw_target::{Side, Target};
    let (ctx, mut app) = pcb_app();
    press_on(&ctx, &mut app, "side:top/silk", egui::Key::Enter);
    assert_eq!(
        app.current_target(),
        Target::Silk(Side::Top),
        "clicking Silkscreen makes it the drawing target: {}",
        app.message
    );
    assert!(
        paints_text(&ctx, &mut app, "Layer 1 of 2 · Top silkscreen")?,
        "the status bar names the target"
    );
    // Line, Box and Circle on the silkscreen make silkscreen graphics.
    for (icon, shape) in [
        ("tool-line", "line"),
        ("tool-box", "rect"),
        ("tool-circle", "circle"),
    ] {
        authoring::tests::tap_icon(&ctx, &mut app, icon)?;
        app.canvas_click(Point::new(10.0, 10.0), 10.0);
        app.canvas_click(Point::new(20.0, 14.0), 10.0);
        let made = app
            .project
            .graphics
            .iter()
            .any(|g| g.shape == shape && g.sublayer == "F.SilkS");
        assert!(
            made,
            "{icon} draws a {shape} on the silkscreen: {}",
            app.message
        );
    }
    // The same Line on Copper is a track.
    press_on(&ctx, &mut app, "side:top/copper", egui::Key::Enter);
    assert_eq!(
        app.current_target(),
        Target::Copper(pcbcraft_doc::Layer::Front)
    );
    app.highlight_net = app.project.nets.first().map(|n| n.id);
    tap_strip(&ctx, &mut app, "icon:tool-line")?;
    let before = drawn(&app);
    app.canvas_click(Point::new(30.0, 10.0), 10.0);
    app.canvas_click(Point::new(40.0, 10.0), 10.0);
    assert_eq!(
        drawn(&app).0,
        before.0 + 1,
        "Line on Copper is a track: {}",
        app.message
    );
    // Text on the silkscreen is board text on the top side.
    app.set_target(Target::Silk(Side::Top));
    tap_strip(&ctx, &mut app, "icon:tool-words")?;
    app.art_text = "REV A".into();
    app.canvas_click(Point::new(12.0, 20.0), 10.0);
    assert!(
        app.project
            .board
            .texts
            .iter()
            .any(|t| t.text == "REV A" && t.layer == pcbcraft_doc::Layer::Front),
        "Text prints on the top silkscreen: {}",
        app.message
    );
    // The Layer field changes the target and the tree follows.
    app.set_target(Target::Silk(Side::Bottom));
    assert!(
        paints_text(&ctx, &mut app, "Bottom silkscreen")?,
        "the Layer field shows the target picked elsewhere"
    );
    Ok(())
}

#[test]
fn a_newcomer_draws_the_board_edge_from_the_tree() -> Result<(), String> {
    use crate::draw_target::Target;
    let (ctx, mut app) = pcb_app();
    press_on(&ctx, &mut app, "edge", egui::Key::Enter);
    assert_eq!(
        app.current_target(),
        Target::Edge,
        "clicking Board edge makes it the target: {}",
        app.message
    );
    tap_strip(&ctx, &mut app, "icon:tool-box")?;
    app.canvas_click(Point::new(1.0, 1.0), 10.0);
    app.canvas_click(Point::new(59.0, 39.0), 10.0);
    let outline = app
        .project
        .board
        .outline
        .as_ref()
        .ok_or_else(|| format!("Box on the board edge sets the outline: {}", app.message))?;
    assert_eq!(outline.len(), 4, "a box has four corners");
    assert!((app.project.board.width - 58.0).abs() < 0.01);
    assert!((app.project.board.height - 38.0).abs() < 0.01);
    // Line on the board edge chains corners until Enter closes the outline.
    tap_strip(&ctx, &mut app, "icon:tool-line")?;
    for (x, y) in [(1.0, 1.0), (59.0, 1.0), (59.0, 39.0), (1.0, 39.0)] {
        app.canvas_click(Point::new(x, y), 10.0);
    }
    key(&ctx, &mut app, egui::Key::Enter);
    assert_eq!(
        app.project.board.outline.as_ref().map(Vec::len),
        Some(4),
        "Line on the edge chains corners into an outline after Enter: {}",
        app.message
    );
    Ok(())
}

#[test]
fn a_newcomer_adds_an_inner_copper_pair_from_the_tree() -> Result<(), String> {
    let (ctx, mut app) = pcb_app();
    assert_eq!(app.project.copper_layer_count(), 2);
    let row =
        text_at(&ctx, &mut app, "Layer 1 · Top").ok_or("the top layer row is not on screen")?;
    right_click(&ctx, &mut app, row);
    authoring::tests::tap(&ctx, &mut app, "Add copper layer pair below")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert_eq!(
        app.project.copper_layer_count(),
        4,
        "the menu adds two inner layers: {}",
        app.message
    );
    assert!(
        paints_text(&ctx, &mut app, "Layer 4 · Bottom")?,
        "the bottom layer is now layer 4"
    );
    assert!(
        paints_text(&ctx, &mut app, "Layer 2 · In1.Cu")?,
        "the new inner layers are rows of the tree"
    );
    // Rename the first inner layer.
    app.tree.rename = Some((
        crate::layers_tree::RenameFor::Layer("In1.Cu".into()),
        "GND".into(),
    ));
    key(&ctx, &mut app, egui::Key::Enter);
    assert!(
        app.project
            .board
            .layer_records
            .iter()
            .any(|r| r.name == "GND"),
        "Rename changes the layer name: {}",
        app.message
    );
    // One undo takes the rename back, a second takes the pair back.
    app.tool_call("undo", json!({}));
    app.tool_call("undo", json!({}));
    assert_eq!(app.project.copper_layer_count(), 2, "undo removes the pair");
    Ok(())
}

/// Save the board as a project file when `PCBCRAFT_SAMPLE_DIR` names a
/// folder, so screenshots can start from the state a story built.
fn keep_sample(app: &mut App, name: &str) {
    if let Ok(dir) = std::env::var("PCBCRAFT_SAMPLE_DIR") {
        let path = std::path::Path::new(&dir).join(format!("{name}.pcbcraft"));
        app.tool_call("save", json!({"path": path.to_string_lossy()}));
    }
}

#[test]
fn a_four_layer_board_lists_every_layer_in_the_tree() -> Result<(), String> {
    let (ctx, mut app) = pcb_app();
    app.run("board.stackup_add_pair", json!({"after": "F.Cu"}));
    let net = app.project.nets.first().map(|n| n.id).ok_or("no net")?;
    app.run(
        "track.add",
        json!({"net": net, "start": {"x": 5.0, "y": 5.0}, "end": {"x": 20.0, "y": 5.0}, "width": 0.25, "layer": "In1.Cu"}),
    );
    app.tree.expanded.insert("inner:1".into());
    for label in [
        "Layer 1 · Top",
        "Layer 2 · In1.Cu",
        "Layer 3 · In2.Cu",
        "Layer 4 · Bottom",
    ] {
        assert!(
            paints_text(&ctx, &mut app, label)?,
            "the tree lists {label}: {}",
            app.message
        );
    }
    press_on(&ctx, &mut app, "inner:1/copper", egui::Key::Enter);
    assert_eq!(
        app.current_target(),
        crate::draw_target::Target::Copper(pcbcraft_doc::Layer::from_index(1)?),
        "an inner copper row is a drawing target"
    );
    keep_sample(&mut app, "four-layer");
    Ok(())
}

#[test]
fn a_silkscreen_line_drawn_on_the_layout_is_kept_for_the_capture() -> Result<(), String> {
    let (ctx, mut app) = pcb_app();
    press_on(&ctx, &mut app, "side:top/silk", egui::Key::Enter);
    tap_strip(&ctx, &mut app, "icon:tool-line")?;
    app.canvas_click(Point::new(8.0, 4.0), 10.0);
    app.canvas_click(Point::new(30.0, 4.0), 10.0);
    assert!(
        app.project.graphics.iter().any(|g| g.sublayer == "F.SilkS"),
        "{}",
        app.message
    );
    keep_sample(&mut app, "silk-line");
    Ok(())
}
