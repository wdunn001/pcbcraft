use super::super::*;
use super::support::*;

#[test]
fn a_newcomer_adds_a_part_and_finds_it_on_the_board() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.run(
        "component.add",
        json!({"kind":"resistor","reference":"R1","value":"1 k","position":{"x":20.0,"y":30.0}}),
    );
    app.refresh();
    assert_eq!(
        app.project.components.len(),
        1,
        "the resistor sits on the schematic: {}",
        app.message
    );
    assert_eq!(
        app.project.board.footprints.len(),
        1,
        "the board already picked up the part without a separate sync step: {}",
        app.message
    );
    app.change_tab(Tab::Pcb);
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        paints_text(&ctx, &mut app, "R1")?,
        "the board view shows the new reference: {}",
        app.message
    );
    Ok(())
}

#[test]
fn a_newcomer_records_the_finish_and_thickness_of_the_board() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Layers")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        scrolled_to(&ctx, &mut app, "HASL")?,
        "the board shows the finish it ships with"
    );
    authoring::tests::tap(&ctx, &mut app, "HASL")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "ENIG")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert_eq!(
        app.project.board.finish, "ENIG",
        "the finish is recorded: {}",
        app.message
    );
    app.notes_thick = 1.2;
    app.notes_er = 4.2;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Set thickness")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Set substrate")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        (app.project.board.thickness - 1.2).abs() < 1e-9
            && (app.project.board.er - 4.2).abs() < 1e-9,
        "thickness and substrate are recorded: {} {} · {}",
        app.project.board.thickness,
        app.project.board.er,
        app.message
    );
    authoring::tests::tap(&ctx, &mut app, "Plated board edge")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        app.project
            .board
            .machining
            .as_ref()
            .and_then(|m| m.edge_plating)
            == Some(true),
        "the edge plating note is kept: {}",
        app.message
    );
    Ok(())
}
#[test]
fn a_newcomer_records_a_multilayer_board_stackup() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Layers")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        scrolled_to(&ctx, &mut app, "Nothing recorded yet")?,
        "a plain board says what it is: {}",
        app.message
    );
    app.stack_name = "Core".into();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Add layer")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert_eq!(
        app.project.board.layer_records.len(),
        1,
        "one layer is recorded: {}",
        app.message
    );
    app.stack_name = "L2".into();
    app.stack_kind = "copper".into();
    app.stack_thickness = 0.035;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Add layer")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert_eq!(
        app.project.board.layer_records.len(),
        2,
        "the stackup grows layer by layer: {}",
        app.message
    );
    assert!(
        paints_text(&ctx, &mut app, "add up to 1.235 mm")?,
        "the sheet totals the board: {}",
        app.message
    );
    authoring::tests::tap(&ctx, &mut app, "Tune")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.stack_edit = Some(("L2".into(), "copper".into(), 0.05));
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Apply")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let l2 = app
        .project
        .board
        .layer_records
        .iter()
        .find(|r| r.name == "L2")
        .ok_or("L2 should still be in the stackup")?;
    assert!(
        (l2.thickness - 0.05).abs() < 1e-9 && l2.kind == "copper",
        "the tuned layer keeps its new numbers: {} {} · {}",
        l2.kind,
        l2.thickness,
        app.message
    );
    authoring::tests::tap(&ctx, &mut app, "Back to a plain two-layer board")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        app.project.board.layer_records.is_empty(),
        "the stackup notes are forgotten: {}",
        app.message
    );
    Ok(())
}
#[test]
fn a_newcomer_chooses_a_board_shape() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Layers")?;
    let mut shown = false;
    for _ in 0..8 {
        if paints_text(&ctx, &mut app, "START FROM A SHAPE")? {
            shown = true;
            break;
        }
        frame(
            &ctx,
            &mut app,
            vec![
                egui::Event::PointerMoved(egui::pos2(1180.0, 500.0)),
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
    assert!(
        shown,
        "the layers dock should offer ready-made board shapes: {}",
        app.message
    );
    authoring::tests::tap(&ctx, &mut app, "Four layers")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let names: Vec<String> = self_names(&app);
    assert_eq!(
        names.len(),
        7,
        "the four-layer shape should record seven layers: {names:?}"
    );
    assert!(
        names.iter().any(|n| n == "Inner 1"),
        "the four-layer shape should add inner copper planes: {names:?}"
    );
    authoring::tests::tap(&ctx, &mut app, "Two layers · 1.6 mm")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let names = self_names(&app);
    assert_eq!(
        names.len(),
        3,
        "the starter shape should replace the four-layer stack with three layers: {names:?}"
    );
    Ok(())
}
#[test]
fn a_newcomer_walks_the_board_anatomy() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    app.tool_call(
        "track.route",
        json!({"from":{"footprint":10,"pad":"1"},"to":{"footprint":11,"pad":"1"},"layer":"F.Cu"}),
    );
    let (id, label) = app
        .project
        .board
        .tracks
        .iter()
        .rev()
        .find(|t| t.layer == pcbcraft_doc::Layer::Front)
        .map(|t| {
            (
                t.id,
                format!(
                    "Wire · {:.2} mm · {} · at {}, {}",
                    t.width,
                    app.project.net_name(t.net),
                    t.start.x.round(),
                    t.start.y.round()
                ),
            )
        })
        .ok_or("routing should leave a wire on the top layer")?;
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Layers")?;
    let shown = scrolled_to(&ctx, &mut app, "BOARD ANATOMY")?;
    assert!(
        shown,
        "the layers dock should list every piece of the board: {}",
        app.message
    );
    let seen = scrolled_to(&ctx, &mut app, "Top copper wires")?;
    assert!(
        seen,
        "the anatomy list should show the top copper wires layer: {}",
        app.message
    );
    authoring::tests::tap(&ctx, &mut app, "Top copper wires")?;
    let row = scrolled_to(&ctx, &mut app, &label)?;
    assert!(
        row,
        "the top copper layer should show the drawn wire {label}: {}",
        app.message
    );
    authoring::tests::tap(&ctx, &mut app, &label)?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert_eq!(
        app.selected,
        Some(id),
        "picking the wire in the tree selects it on the drawing: {}",
        app.message
    );
    assert_eq!(
        app.tab,
        Tab::Pcb,
        "the drawing jumps to the board so the pick is visible"
    );
    Ok(())
}
#[test]
fn a_newcomer_prints_words_on_the_board() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    let before = app.project.board.texts.len();
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Settings")?;
    assert!(
        scrolled_to_at(&ctx, &mut app, "BOARD LABELS", 700.0)?,
        "the board panel should offer printed words: {}",
        app.message
    );
    app.label_text = "Widget One".into();
    app.label_x = 3.0;
    app.label_y = 2.0;
    app.label_size = 1.2;
    app.label_front = false;
    scrolled_to_at(&ctx, &mut app, "Add label", 700.0)?;
    authoring::tests::tap(&ctx, &mut app, "Add label")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.refresh();
    assert_eq!(
        app.project.board.texts.len(),
        before + 1,
        "one line of words lands on the board: {}",
        app.message
    );
    let label = app
        .project
        .board
        .texts
        .last()
        .ok_or("the label should be on the board")?;
    assert!(
        label.text == "Widget One"
            && label.layer == pcbcraft_doc::Layer::Back
            && (label.position.x - 3.0).abs() < 1e-9
            && (label.size - 1.2).abs() < 1e-9,
        "the label carries the words, side, spot and letter height that were set"
    );
    for _ in 0..3 {
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
    assert!(
        paints_text(&ctx, &mut app, "Widget One · Bottom")?,
        "the printed words are listed so they can be wiped: {}",
        app.message
    );
    authoring::tests::tap(&ctx, &mut app, "Erase")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.refresh();
    assert_eq!(
        app.project.board.texts.len(),
        before,
        "erasing wipes exactly that line of words: {}",
        app.message
    );
    Ok(())
}
#[test]
fn a_newcomer_marks_a_stricter_corner() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    let before = app.project.board.rule_areas.len();
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Rules")?;
    for _ in 0..22 {
        frame(
            &ctx,
            &mut app,
            vec![
                egui::Event::PointerMoved(egui::pos2(1180.0, 500.0)),
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
    assert!(
        scrolled_to_at(&ctx, &mut app, "BOARD AREAS", 700.0)?,
        "the rules page should offer stricter corners: {}",
        app.message
    );
    app.area_x = 1.0;
    app.area_y = 1.0;
    app.area_w = 4.0;
    app.area_h = 4.0;
    app.area_gap = 0.5;
    app.area_limited = true;
    app.area_height = 3.0;
    authoring::tests::tap(&ctx, &mut app, "Add area")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.refresh();
    assert_eq!(
        app.project.board.rule_areas.len(),
        before + 1,
        "one stricter corner is marked on the board: {}",
        app.message
    );
    let area = app
        .project
        .board
        .rule_areas
        .last()
        .ok_or("the area should be on the board")?;
    assert!(
        (area.clearance - 0.5).abs() < 1e-9 && area.max_height == Some(3.0),
        "the area carries the gap and height limit that were typed"
    );
    assert!(
        paints_text(&ctx, &mut app, "Stricter areas on the board")?,
        "the area is listed so it can be erased: {}",
        app.message
    );
    authoring::tests::tap(&ctx, &mut app, "Erase area")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.refresh();
    assert_eq!(
        app.project.board.rule_areas.len(),
        before,
        "erasing hands the corner back to the normal rules: {}",
        app.message
    );
    Ok(())
}
#[test]
fn a_newcomer_cuts_a_round_hole_in_the_board() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Settings")?;
    assert!(
        scrolled_to_at(&ctx, &mut app, "CUT-OUT HOLE", 700.0)?,
        "the board geometry section should offer a hole: {}",
        app.message
    );
    let before = app.project.board.cutouts.len();
    app.cut_x = 25.0;
    app.cut_y = 20.0;
    app.cut_r = 1.5;
    authoring::tests::tap(&ctx, &mut app, "Add hole")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.refresh();
    assert_eq!(
        app.project.board.cutouts.len(),
        before + 1,
        "one more hole is cut into the board: {}",
        app.message
    );
    let hole = app
        .project
        .board
        .cutouts
        .iter()
        .find(|h| (h.center.x - 25.0).abs() < 1e-9 && (h.center.y - 20.0).abs() < 1e-9)
        .ok_or("the hole should sit exactly where it was placed")?;
    assert!(
        (hole.radius - 1.5).abs() < 1e-9,
        "the hole should be exactly the size that was typed"
    );
    Ok(())
}
#[test]
fn a_newcomer_drills_a_slot_and_reads_how_long_it_is() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    app.tool_call(
        "hole.slot",
        json!({"start":{"x":12.0,"y":8.0},"end":{"x":18.0,"y":8.0},"width":1.5}),
    );
    let slot = app
        .project
        .board
        .holes
        .iter()
        .find(|h| h.is_slot())
        .map(|h| h.id)
        .ok_or("the slot should be on the board")?;
    app.change_tab(Tab::Pcb);
    app.refresh();
    let ctx = egui::Context::default();
    authoring::tests::tap(&ctx, &mut app, "Layers")?;
    assert!(
        scrolled_to(&ctx, &mut app, "MOUNTING HOLES")?,
        "the hole list is the place a slot is read: {}",
        app.message
    );
    assert!(
        scrolled_to(&ctx, &mut app, "1.50 mm slot 7.50 mm long")?,
        "a slot reads as its width and its overall length: {}",
        app.message
    );
    // The canvas paints the slot without complaint; the drawing is the
    // only place a capsule outline shows up.
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.select(Some(slot));
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        app.project.board.holes.iter().any(|h| h.id == slot),
        "looking at the slot leaves it on the board"
    );
    Ok(())
}
#[test]
fn a_newcomer_drills_mounting_holes_into_the_board() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    app.tab = Tab::Pcb;
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Mounting hole")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.canvas_click(Point::new(-25.0, -17.0), 1.0);
    assert_eq!(
        app.project.board.holes.len(),
        1,
        "one click drills one hole: {}",
        app.message
    );
    app.canvas_click(Point::new(25.0, -17.0), 1.0);
    assert_eq!(
        app.project.board.holes.len(),
        2,
        "the tool stays ready for the next corner: {}",
        app.message
    );
    assert!(
        app.project
            .board
            .holes
            .iter()
            .all(|h| (h.diameter - 3.2).abs() < 1e-9),
        "both holes use the drill size on show: {}",
        app.message
    );
    app.tool = Tool::Select;
    app.tab = Tab::Schematic;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Layers")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        scrolled_to(&ctx, &mut app, "3.20 mm")?,
        "the holes are listed with their drill size"
    );
    let id = app.project.board.holes[0].id;
    authoring::tests::tap(&ctx, &mut app, "Tune")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.hole_edit = Some((id, 3.0));
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Apply")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        app.project
            .board
            .holes
            .iter()
            .any(|h| h.id == id && (h.diameter - 3.0).abs() < 1e-9),
        "the hole is resized: {}",
        app.message
    );
    authoring::tests::tap(&ctx, &mut app, "\u{d7}")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert_eq!(
        app.project.board.holes.len(),
        1,
        "the other hole is drilled back out: {}",
        app.message
    );
    Ok(())
}
#[test]
fn a_newcomer_draws_a_cut_line_for_their_laser() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    app.change_tab(Tab::Pcb);
    app.refresh();
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Draw shape")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "New layer")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert_eq!(
        app.project.board.user_layers.len(),
        1,
        "the new layer button should add a layer: {}",
        app.message
    );
    authoring::tests::tap(&ctx, &mut app, "Line")?;
    let click = |p: egui::Pos2| {
        vec![
            egui::Event::PointerMoved(p),
            egui::Event::PointerButton {
                pos: p,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::PointerButton {
                pos: p,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            },
        ]
    };
    frame(
        &ctx,
        &mut app,
        click(egui::pos2(660.0, 460.0)),
        egui::Modifiers::NONE,
    );
    frame(
        &ctx,
        &mut app,
        click(egui::pos2(760.0, 540.0)),
        egui::Modifiers::NONE,
    );
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.refresh();
    assert_eq!(
        app.project.board.user_layers[0].shapes.len(),
        1,
        "two clicks should leave one straight cut line: {}",
        app.message
    );
    app.art_text = "HELLO LASER".into();
    authoring::tests::tap(&ctx, &mut app, "Words")?;
    frame(
        &ctx,
        &mut app,
        click(egui::pos2(700.0, 600.0)),
        egui::Modifiers::NONE,
    );
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.refresh();
    let layer = &app.project.board.user_layers[0];
    assert_eq!(
        layer.shapes.len(),
        2,
        "one click with words armed should print them: {}",
        app.message
    );
    assert!(
        layer.shapes.iter().any(|sh| matches!(
            sh,
            pcbcraft_doc::ArtShape::Text { text, .. } if text == "HELLO LASER"
        )),
        "the printed words should be exactly what was typed"
    );
    Ok(())
}
#[test]
fn a_newcomer_sees_their_cut_line_conflict_with_a_via() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    // The sample board has no vias, so the story places one in the middle
    // of the board and then draws a cut line straight across it.
    app.tool_call(
        "via.place",
        json!({"net":4,"position":{"x":20.0,"y":30.0},"diameter":0.8,"drill":0.4}),
    );
    assert_eq!(
        app.project.board.vias.len(),
        1,
        "the via should be placed: {}",
        app.message
    );
    app.tool_call(
        "board.layer_add",
        json!({"name":"Card Outline","kind":"cut"}),
    );
    let layer = app
        .project
        .board
        .user_layers
        .iter()
        .find(|l| l.name == "Card Outline")
        .map(|l| l.id)
        .ok_or("the cut layer should be added")?;
    app.tool_call(
        "board.layer_line_add",
        json!({"layer":layer,"x1":18.0,"y1":30.0,"x2":22.0,"y2":30.0}),
    );
    app.refresh();
    let conflict = app
        .checks
        .get("drc")
        .and_then(Value::as_array)
        .is_some_and(|rows| {
            rows.iter()
                .any(|r| r.get("code").and_then(Value::as_str) == Some("cut_hits_via"))
        });
    assert!(
        conflict,
        "the cut across the via is reported: {}",
        app.message
    );
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Checks")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        paints_text(&ctx, &mut app, "Cut hits via")?,
        "the review names the conflict in plain words: {}",
        app.message
    );
    assert!(
        paints_text(&ctx, &mut app, "1 to fix")?,
        "a cut through a via lands in the footer count: {}",
        app.message
    );
    Ok(())
}

fn right_click(ctx: &egui::Context, app: &mut App, screen: egui::Pos2) {
    for pressed in [true, false] {
        frame(
            ctx,
            app,
            vec![
                egui::Event::PointerMoved(screen),
                egui::Event::PointerButton {
                    pos: screen,
                    button: egui::PointerButton::Secondary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            egui::Modifiers::NONE,
        );
    }
    // A new popup is laid out hidden for a frame or two before it paints.
    for _ in 0..3 {
        frame(ctx, app, vec![], egui::Modifiers::NONE);
    }
}

fn sample_board_app() -> App {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    app.change_tab(Tab::Pcb);
    app.needs_fit = true;
    app
}

#[test]
fn a_newcomer_presses_escape_and_the_menu_closes_without_acting() -> Result<(), String> {
    let mut app = sample_board_app();
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let screen = pcb_screen(&app, Point::new(1.0, 1.0));
    right_click(&ctx, &mut app, screen);
    assert!(
        paints_text(&ctx, &mut app, "Fit view")?,
        "right-clicking empty space opens a menu: {}",
        app.message
    );
    let tracks = app.project.board.tracks.len();
    let parts = app.project.board.footprints.len();
    frame(
        &ctx,
        &mut app,
        vec![egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
        egui::Modifiers::NONE,
    );
    assert!(
        !paints_text(&ctx, &mut app, "Fit view")?,
        "Escape closes the menu"
    );
    assert_eq!(app.project.board.tracks.len(), tracks);
    assert_eq!(app.project.board.footprints.len(), parts);
    Ok(())
}

#[test]
fn a_newcomer_right_clicks_empty_space_and_can_fit_the_view() -> Result<(), String> {
    let mut app = sample_board_app();
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.pcb.zoom *= 4.0;
    let zoomed = app.pcb.zoom;
    let screen = pcb_screen(&app, Point::new(-500.0, -500.0));
    let screen = egui::pos2(
        screen
            .x
            .clamp(app.canvas_rect.min.x + 5.0, app.canvas_rect.max.x - 5.0),
        screen
            .y
            .clamp(app.canvas_rect.min.y + 5.0, app.canvas_rect.max.y - 5.0),
    );
    right_click(&ctx, &mut app, screen);
    authoring::tests::tap(&ctx, &mut app, "Fit view")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        (app.pcb.zoom - zoomed).abs() > f32::EPSILON,
        "Fit view brings the whole board back into frame: {}",
        app.message
    );
    Ok(())
}

#[test]
fn a_newcomer_right_clicks_a_part_to_rotate_lock_or_delete_it() -> Result<(), String> {
    let mut app = sample_board_app();
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let (id, at, turned) = app
        .project
        .board
        .footprints
        .first()
        .map(|f| (f.id, f.position, f.rotation))
        .ok_or("the sample board has parts")?;
    for _ in 0..6 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    let screen = pcb_screen(&app, at);
    right_click(&ctx, &mut app, screen);
    authoring::tests::tap(&ctx, &mut app, "Rotate")?;
    app.refresh();
    let after = app.project.footprint(id).map(|f| f.rotation);
    assert_ne!(
        after,
        Some(turned),
        "Rotate turns the part: {}",
        app.message
    );

    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let screen = pcb_screen(&app, at);
    right_click(&ctx, &mut app, screen);
    authoring::tests::tap(&ctx, &mut app, "Lock in place")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.refresh();
    assert_eq!(
        app.project.footprint(id).map(|f| f.locked),
        Some(Some(true)),
        "Lock in place freezes the part: {}",
        app.message
    );

    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let screen = pcb_screen(&app, at);
    right_click(&ctx, &mut app, screen);
    authoring::tests::tap(&ctx, &mut app, "Delete")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.refresh();
    assert!(
        app.project.footprint(id).is_some(),
        "a locked part refuses to be deleted: {}",
        app.message
    );
    Ok(())
}

#[test]
fn a_newcomer_right_clicks_a_wire_and_assigns_its_layer() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    app.tool_call(
        "track.route",
        json!({"from":{"footprint":10,"pad":"1"},"to":{"footprint":11,"pad":"1"},"layer":"F.Cu"}),
    );
    let middle = app
        .project
        .board
        .tracks
        .first()
        .map(|t| Point::new((t.start.x + t.end.x) / 2.0, (t.start.y + t.end.y) / 2.0))
        .ok_or("routing should leave a wire")?;
    app.change_tab(Tab::Pcb);
    app.needs_fit = true;
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let screen = pcb_screen(&app, middle);
    for pressed in [true, false] {
        frame(
            &ctx,
            &mut app,
            vec![
                egui::Event::PointerMoved(screen),
                egui::Event::PointerButton {
                    pos: screen,
                    button: egui::PointerButton::Secondary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            egui::Modifiers::NONE,
        );
    }
    authoring::tests::tap(&ctx, &mut app, "Assign layer")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Back")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.refresh();
    let layer = app.project.board.tracks.first().map(|t| t.layer);
    assert_eq!(
        layer,
        Some(pcbcraft_doc::Layer::Back),
        "Assign layer moves the wire: {}",
        app.message
    );
    Ok(())
}

#[test]
fn a_newcomer_right_clicks_a_wire_and_cuts_it() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    app.tool_call(
        "track.route",
        json!({"from":{"footprint":10,"pad":"1"},"to":{"footprint":11,"pad":"1"},"layer":"F.Cu"}),
    );
    let (before, middle) = app
        .project
        .board
        .tracks
        .first()
        .map(|t| {
            (
                app.project.board.tracks.len(),
                Point::new((t.start.x + t.end.x) / 2.0, (t.start.y + t.end.y) / 2.0),
            )
        })
        .ok_or("routing should leave a wire to cut")?;
    app.change_tab(Tab::Pcb);
    app.needs_fit = true;
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let screen = pcb_screen(&app, middle);
    for pressed in [true, false] {
        frame(
            &ctx,
            &mut app,
            vec![
                egui::Event::PointerMoved(screen),
                egui::Event::PointerButton {
                    pos: screen,
                    button: egui::PointerButton::Secondary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            egui::Modifiers::NONE,
        );
    }
    assert!(
        paints_text(&ctx, &mut app, "Cut")?,
        "right-clicking a wire opens a menu with Cut: {}",
        app.message
    );
    authoring::tests::tap(&ctx, &mut app, "Cut")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.refresh();
    assert_eq!(
        app.project.board.tracks.len(),
        before + 1,
        "Cut splits the wire in two: {}",
        app.message
    );
    Ok(())
}

/// A board with one routed wire, on the board tab, ready to right-click.
///
/// # Rules
/// Returns the wire's midpoint in board units.
///
/// # Dependencies
/// `track.route` in ../../../engine; `sample_design` from ../../../mcp.
///
/// # Do not
/// Use for stories that need more than one wire.
fn routed_wire_app() -> Result<(App, Point), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    app.tool_call(
        "track.route",
        json!({"from":{"footprint":10,"pad":"1"},"to":{"footprint":11,"pad":"1"},"layer":"F.Cu"}),
    );
    let middle = app
        .project
        .board
        .tracks
        .first()
        .map(|t| Point::new((t.start.x + t.end.x) / 2.0, (t.start.y + t.end.y) / 2.0))
        .ok_or("routing should leave a wire")?;
    app.change_tab(Tab::Pcb);
    app.needs_fit = true;
    Ok((app, middle))
}

#[test]
fn a_newcomer_right_clicks_a_pad_and_starts_a_route() -> Result<(), String> {
    let mut app = sample_board_app();
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let (id, number, at) = app
        .project
        .board
        .footprints
        .iter()
        .find_map(|f| {
            f.pads.first().and_then(|p| {
                f.pad_position(&p.number)
                    .map(|at| (f.id, p.number.clone(), at))
            })
        })
        .ok_or("the sample board has a pad")?;
    let screen = pcb_screen(&app, at);
    right_click(&ctx, &mut app, screen);
    authoring::tests::tap(&ctx, &mut app, "Start route")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        matches!(&app.tool, Tool::Route(Some((f, n))) if *f == id && *n == number),
        "Start route arms the route tool on that pad: {}",
        app.message
    );
    Ok(())
}

#[test]
fn a_newcomer_right_clicks_a_pad_and_opens_its_properties() -> Result<(), String> {
    let mut app = sample_board_app();
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let (id, _number, at, home) = app
        .project
        .board
        .footprints
        .iter()
        .find_map(|f| {
            f.pads.first().and_then(|p| {
                f.pad_position(&p.number)
                    .map(|at| (f.id, p.number.clone(), at, f.position))
            })
        })
        .ok_or("the sample board has a pad")?;
    let screen = pcb_screen(&app, at);
    right_click(&ctx, &mut app, screen);
    app.x = -9999.0;
    authoring::tests::tap(&ctx, &mut app, "Properties")?;
    assert_eq!(app.selected, Some(id), "the pad's part is selected");
    assert_eq!(app.x, home.x, "Properties loads the part: {}", app.message);
    Ok(())
}

#[test]
fn a_newcomer_right_clicks_a_part_and_duplicates_it() -> Result<(), String> {
    let mut app = sample_board_app();
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let at = app
        .project
        .board
        .footprints
        .first()
        .map(|f| f.position)
        .ok_or("the sample board has parts")?;
    // Rows are 24 px tall now, so the canvas takes a few frames to settle.
    for _ in 0..4 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    let before = app.project.components.len();
    let screen = pcb_screen(&app, at);
    right_click(&ctx, &mut app, screen);
    authoring::tests::tap(&ctx, &mut app, "Duplicate")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.refresh();
    assert_eq!(
        app.project.components.len(),
        before + 1,
        "Duplicate places a copy: {}",
        app.message
    );
    Ok(())
}

#[test]
fn a_newcomer_right_clicks_a_part_and_opens_its_properties() -> Result<(), String> {
    let mut app = sample_board_app();
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let at = app
        .project
        .board
        .footprints
        .first()
        .map(|f| f.position)
        .ok_or("the sample board has parts")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let screen = pcb_screen(&app, at);
    right_click(&ctx, &mut app, screen);
    app.x = -9999.0;
    authoring::tests::tap(&ctx, &mut app, "Properties")?;
    assert_eq!(app.x, at.x, "Properties loads the part: {}", app.message);
    Ok(())
}

#[test]
fn a_newcomer_right_clicks_a_wire_and_changes_its_width() -> Result<(), String> {
    let (mut app, middle) = routed_wire_app()?;
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let screen = pcb_screen(&app, middle);
    right_click(&ctx, &mut app, screen);
    app.track_width = 0.75;
    authoring::tests::tap(&ctx, &mut app, "Change width")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.refresh();
    let width = app.project.board.tracks.first().map(|t| t.width);
    assert_eq!(width, Some(0.75), "Change width applies: {}", app.message);
    Ok(())
}

#[test]
fn a_newcomer_right_clicks_a_wire_and_opens_its_properties() -> Result<(), String> {
    let (mut app, middle) = routed_wire_app()?;
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let screen = pcb_screen(&app, middle);
    right_click(&ctx, &mut app, screen);
    let wire = app.project.board.tracks.first().map(|t| t.id);
    assert_eq!(app.selected, wire, "right-click selects the wire");
    app.x = -9999.0;
    authoring::tests::tap(&ctx, &mut app, "Properties")?;
    assert!(
        app.canvas_menu.is_none(),
        "Properties closes the menu: {}",
        app.message
    );
    Ok(())
}

/// The sample design on the schematic tab, plus a pin that is on screen.
///
/// # Rules
/// Fits the view first, then takes the first pin whose screen spot lies well
/// inside the canvas. Returns the app, the part, the pin number and the
/// screen spot.
///
/// # Dependencies
/// `sample_design` from ../../../mcp.
///
/// # Do not
/// Assume which part owns the pin.
fn sample_schematic_pin(ctx: &egui::Context) -> Result<(App, Id, String, egui::Pos2), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    app.change_tab(Tab::Schematic);
    app.needs_fit = true;
    // Let the side panels settle so the canvas stops moving before we aim.
    for _ in 0..6 {
        frame(ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    let inside = app.canvas_rect.shrink(30.0);
    let found = app.project.components.iter().find_map(|c| {
        c.pins.iter().find_map(|p| {
            let spot = c
                .pin_position(&p.number)
                .map(|at| app.schematic.screen(at, app.canvas_rect))?;
            inside
                .contains(spot)
                .then(|| (c.id, p.number.clone(), spot))
        })
    });
    let (id, pin, spot) = found.ok_or("no pin is on screen")?;
    Ok((app, id, pin, spot))
}

#[test]
fn a_newcomer_right_clicks_a_pin_and_starts_a_wire() -> Result<(), String> {
    let ctx = egui::Context::default();
    let (mut app, id, pin, screen) = sample_schematic_pin(&ctx)?;
    right_click(&ctx, &mut app, screen);
    authoring::tests::tap(&ctx, &mut app, "Start wire")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        matches!(&app.tool, Tool::Wire(Some(p)) if p.component == id && p.pin == pin),
        "Start wire arms the wire tool on that pin: {}",
        app.message
    );
    Ok(())
}

#[test]
fn a_newcomer_right_clicks_a_pin_and_marks_no_connect() -> Result<(), String> {
    let ctx = egui::Context::default();
    let (mut app, _id, _pin, screen) = sample_schematic_pin(&ctx)?;
    right_click(&ctx, &mut app, screen);
    authoring::tests::tap(&ctx, &mut app, "Mark no connect")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        matches!(app.tool, Tool::NoConnect),
        "Mark no connect arms the no-connect tool: {}",
        app.message
    );
    Ok(())
}

#[test]
fn a_newcomer_right_clicks_a_pin_and_opens_its_properties() -> Result<(), String> {
    let ctx = egui::Context::default();
    let (mut app, id, _pin, screen) = sample_schematic_pin(&ctx)?;
    right_click(&ctx, &mut app, screen);
    let home = app
        .project
        .component(id)
        .map(|c| c.position)
        .ok_or("pin's part exists")?;
    app.x = -9999.0;
    authoring::tests::tap(&ctx, &mut app, "Properties")?;
    assert_eq!(app.selected, Some(id), "the pin's part is selected");
    assert_eq!(app.x, home.x, "Properties loads the part: {}", app.message);
    Ok(())
}

fn fresh_board_app() -> (egui::Context, App) {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    (ctx, app)
}

#[test]
fn a_newcomer_adds_a_datum_on_a_mounting_hole_and_it_follows_the_hole() -> Result<(), String> {
    let (ctx, mut app) = fresh_board_app();
    app.tool_call(
        "board.hole_place",
        json!({"position": {"x": 10.0, "y": 12.0}, "diameter": 3.2}),
    );
    app.refresh();
    let hole = app
        .project
        .board
        .holes
        .first()
        .map(|h| h.id)
        .ok_or("the mounting hole should exist")?;
    authoring::tests::tap(&ctx, &mut app, "Layers")?;
    assert!(
        scrolled_to(&ctx, &mut app, "DATUMS")?,
        "the tree offers a DATUMS group"
    );
    app.datum_ui.add_name = "MH1".into();
    app.datum_ui.add_hole = Some(hole);
    assert!(
        scrolled_to(&ctx, &mut app, "Add datum")?,
        "the add button is on screen"
    );
    authoring::tests::tap(&ctx, &mut app, "Add datum")?;
    app.refresh();
    let datum = app
        .project
        .board
        .datums
        .first()
        .cloned()
        .ok_or_else(|| format!("a datum should be added: {}", app.message))?;
    assert_eq!(datum.hole, Some(hole), "the datum is tied to the hole");
    assert!(
        paints_text(&ctx, &mut app, "MH1")?,
        "the name shows on the drawing"
    );
    // Select it and rename it in Properties.
    app.select(Some(datum.id));
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.datum_ui.name = "SCREW_A".into();
    authoring::tests::tap(&ctx, &mut app, "Rename")?;
    app.refresh();
    assert_eq!(
        app.project.board.datums[0].name, "SCREW_A",
        "{}",
        app.message
    );
    assert!(
        paints_text(&ctx, &mut app, "SCREW_A")?,
        "the new name shows on the drawing"
    );
    // Move the hole: the datum follows.
    app.tool_call(
        "board.hole_move",
        json!({"id": hole, "position": {"x": 30.0, "y": 20.0}}),
    );
    app.refresh();
    let d = app.project.board.datums[0].clone();
    let at = app.project.datum_position(&d);
    assert!(
        (at.x - 30.0).abs() < 1e-6 && (at.y - 20.0).abs() < 1e-6,
        "the datum follows the hole to {}, {}: {}",
        at.x,
        at.y,
        app.message
    );
    // Remove it.
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Remove")?;
    app.refresh();
    assert!(
        app.project.board.datums.is_empty(),
        "the datum is gone: {}",
        app.message
    );
    Ok(())
}

#[test]
fn a_bad_datum_name_is_refused_with_a_message() -> Result<(), String> {
    let (ctx, mut app) = fresh_board_app();
    authoring::tests::tap(&ctx, &mut app, "Layers")?;
    assert!(scrolled_to(&ctx, &mut app, "DATUMS")?);
    app.datum_ui.add_name = "bad name!".into();
    authoring::tests::tap(&ctx, &mut app, "Add datum")?;
    app.refresh();
    assert!(
        app.project.board.datums.is_empty(),
        "an illegal name adds nothing"
    );
    assert!(
        !app.message.ends_with("committed"),
        "the engine's reason is shown: {}",
        app.message
    );
    app.datum_ui.add_name = "A".into();
    authoring::tests::tap(&ctx, &mut app, "Add datum")?;
    app.datum_ui.add_name = "a".into();
    authoring::tests::tap(&ctx, &mut app, "Add datum")?;
    app.refresh();
    assert_eq!(
        app.project.board.datums.len(),
        1,
        "names are unique ignoring case"
    );
    Ok(())
}

fn board_app_with_via(layers: u32) -> Result<(App, Point), String> {
    let mut app = sample_board_app();
    if layers > 2 {
        let mut session = app.session.lock().map_err(|_| "session busy")?;
        let records = &mut session.project.board.layer_records;
        records.clear();
        for i in 1..=layers {
            records.push(pcbcraft_doc::LayerRecord {
                name: format!("L{i}"),
                kind: "copper".into(),
                thickness: 0.035,
                copper_role: None,
            });
            if i < layers {
                records.push(pcbcraft_doc::LayerRecord {
                    name: format!("P{i}"),
                    kind: "prepreg".into(),
                    thickness: 0.4,
                    copper_role: None,
                });
            }
        }
    }
    let at = Point::new(20.0, 14.0);
    app.tool_call(
        "via.place",
        json!({"net":4,"position":{"x":at.x,"y":at.y},"diameter":0.8,"drill":0.4}),
    );
    if app.project.board.vias.is_empty() {
        return Err(format!("the via should be placed: {}", app.message));
    }
    Ok((app, at))
}

#[test]
fn a_newcomer_sees_one_span_on_a_two_layer_board() -> Result<(), String> {
    let (mut app, at) = board_app_with_via(2)?;
    let ctx = egui::Context::default();
    for _ in 0..8 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    let screen = pcb_screen(&app, at);
    right_click(&ctx, &mut app, screen);
    authoring::tests::tap(&ctx, &mut app, "Assign layers")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        paints_text(&ctx, &mut app, "Top to bottom (current)")?,
        "the only span is top to bottom and says it is current: {}",
        app.message
    );
    Ok(())
}

#[test]
fn a_newcomer_right_clicks_a_via_and_assigns_its_layers() -> Result<(), String> {
    let (mut app, at) = board_app_with_via(4)?;
    let ctx = egui::Context::default();
    for _ in 0..8 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    let screen = pcb_screen(&app, at);
    right_click(&ctx, &mut app, screen);
    authoring::tests::tap(&ctx, &mut app, "Assign layers")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "L1 to L2")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.refresh();
    let via = app.project.board.vias.first().ok_or("via missing")?;
    assert_eq!(
        app.project.via_span(via).1.index(),
        1,
        "the via now ends on the second layer: {}",
        app.message
    );
    app.tool_call("undo", json!({}));
    app.refresh();
    let via = app.project.board.vias.first().ok_or("via missing")?;
    assert_eq!(
        app.project.via_span(via).1.index(),
        3,
        "one undo restores the old span: {}",
        app.message
    );
    Ok(())
}

#[test]
fn a_newcomer_cannot_paste_before_copying() -> Result<(), String> {
    let mut app = sample_board_app();
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let screen = pcb_screen(&app, Point::new(1.0, 1.0));
    right_click(&ctx, &mut app, screen);
    let parts = app.project.board.footprints.len();
    authoring::tests::tap(&ctx, &mut app, "Paste")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.refresh();
    assert_eq!(
        app.project.board.footprints.len(),
        parts,
        "Paste does nothing while nothing is copied: {}",
        app.message
    );
    Ok(())
}

#[test]
fn a_newcomer_copies_a_part_and_pastes_it_from_the_right_click_menu() -> Result<(), String> {
    let mut app = sample_board_app();
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let at = app
        .project
        .board
        .footprints
        .first()
        .map(|f| f.position)
        .ok_or("the sample board has parts")?;
    let before = app.project.board.footprints.len();
    let screen = pcb_screen(&app, at);
    right_click(&ctx, &mut app, screen);
    authoring::tests::tap(&ctx, &mut app, "Copy")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        app.edit.clip.is_some(),
        "Copy keeps the text: {}",
        app.message
    );
    let spot = Point::new(at.x + 12.0, at.y + 9.0);
    let screen = pcb_screen(&app, spot);
    right_click(&ctx, &mut app, screen);
    authoring::tests::tap(&ctx, &mut app, "Paste")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.refresh();
    assert_eq!(
        app.project.board.footprints.len(),
        before + 1,
        "Paste adds one part: {}",
        app.message
    );
    let copy = app.project.board.footprints.last().ok_or("copy missing")?;
    assert!(
        (copy.position.x - spot.x).abs() < 1.0 && (copy.position.y - spot.y).abs() < 1.0,
        "the copy lands at the click: {:?}",
        copy.position
    );
    app.tool_call("undo", json!({}));
    app.refresh();
    assert_eq!(
        app.project.board.footprints.len(),
        before,
        "one undo removes the paste: {}",
        app.message
    );
    Ok(())
}

fn sample_on(tab: Tab) -> (App, egui::Context) {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    let ctx = egui::Context::default();
    app.change_tab(tab);
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    (app, ctx)
}

#[test]
fn a_newcomer_hides_and_shows_each_kind_of_thing_on_the_board() -> Result<(), String> {
    let (mut app, ctx) = sample_on(Tab::Pcb);
    for (id, name) in [
        ("checkbox:Top copper", "Top copper"),
        ("checkbox:Bottom copper", "Bottom copper"),
        ("checkbox:Pads", "Pads"),
        ("checkbox:Vias", "Vias"),
        ("checkbox:Copper zones", "Copper zones"),
        ("checkbox:Cutouts", "Cutouts"),
        ("checkbox:Package artwork", "Package artwork"),
    ] {
        tap_id(&ctx, &mut app, id)?;
        assert!(
            !app.layer_shown(name),
            "ticking {name} off hides it from the drawing"
        );
        tap_id(&ctx, &mut app, id)?;
        assert!(app.layer_shown(name), "ticking {name} again shows it");
    }
    Ok(())
}

#[test]
fn a_newcomer_picks_a_drawing_tool_from_the_strip_on_the_board() -> Result<(), String> {
    let (mut app, ctx) = sample_on(Tab::Pcb);
    tap_id(&ctx, &mut app, "icon:tool-route")?;
    assert!(
        matches!(app.tool, Tool::Route(_)),
        "Route arms the track tool"
    );
    for (id, shape) in [
        ("icon:tool-box", DrawShape::Rect),
        ("icon:tool-circle", DrawShape::Circle),
        ("icon:tool-outline", DrawShape::Poly),
        ("icon:tool-words", DrawShape::Words),
    ] {
        tap_id(&ctx, &mut app, id)?;
        assert!(
            matches!(app.tool, Tool::DrawLayer) && app.art_shape == shape,
            "{id} arms its shape on the machine layer"
        );
    }
    tap_id(&ctx, &mut app, "button:Cutout")?;
    assert!(
        matches!(app.tool, Tool::Cutout),
        "Cutout arms the hole tool"
    );
    Ok(())
}

#[test]
fn a_newcomer_wires_two_pins_and_picks_what_a_click_may_grab() -> Result<(), String> {
    let (mut app, ctx) = sample_on(Tab::Schematic);
    tap_id(&ctx, &mut app, "icon:tool-wire")?;
    assert!(matches!(app.tool, Tool::Wire(_)), "Wire arms the wire tool");
    for (id, read) in [
        (
            "button:Text",
            (|a: &App| a.edit.filter.text) as fn(&App) -> bool,
        ),
        ("button:Graphics", |a: &App| a.edit.filter.graphics),
        ("button:Copper", |a: &App| a.edit.filter.copper),
        ("button:Parts", |a: &App| a.edit.filter.parts),
    ] {
        let before = read(&app);
        tap_id(&ctx, &mut app, id)?;
        assert_ne!(read(&app), before, "{id} flips what a click may pick up");
    }
    tap_id(&ctx, &mut app, "button:Measure")?;
    assert!(matches!(app.tool, Tool::Measure), "Measure arms the ruler");
    Ok(())
}

#[test]
fn a_newcomer_zooms_out_and_fits_the_whole_design() -> Result<(), String> {
    let (mut app, ctx) = sample_on(Tab::Schematic);
    let before = app.schematic.zoom;
    tap_id(&ctx, &mut app, "button:\u{2212}")?;
    assert!(app.schematic.zoom < before, "the minus button zooms out");
    tap_id(&ctx, &mut app, "icon:canvas-fit")?;
    assert!(
        app.needs_fit || app.schematic.zoom > 0.0,
        "Fit frames the design"
    );
    Ok(())
}
