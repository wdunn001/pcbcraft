use super::super::*;
use crate::test_support::{frame, paints_text, scrolled_to, scrolled_to_at, self_names, tap};
use serde_json::json;

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
    tap(&ctx, &mut app, "Layers")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        scrolled_to(&ctx, &mut app, "HASL")?,
        "the board shows the finish it ships with"
    );
    tap(&ctx, &mut app, "HASL")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    tap(&ctx, &mut app, "ENIG")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert_eq!(
        app.project.board.finish, "ENIG",
        "the finish is recorded: {}",
        app.message
    );
    app.notes_thick = 1.2;
    app.notes_er = 4.2;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    tap(&ctx, &mut app, "Set thickness")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    tap(&ctx, &mut app, "Set substrate")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        (app.project.board.thickness - 1.2).abs() < 1e-9
            && (app.project.board.er - 4.2).abs() < 1e-9,
        "thickness and substrate are recorded: {} {} · {}",
        app.project.board.thickness,
        app.project.board.er,
        app.message
    );
    tap(&ctx, &mut app, "Plated board edge")?;
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
    tap(&ctx, &mut app, "Layers")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        scrolled_to(&ctx, &mut app, "Nothing recorded yet")?,
        "a plain board says what it is: {}",
        app.message
    );
    app.stack_name = "Core".into();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    tap(&ctx, &mut app, "Add layer")?;
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
    tap(&ctx, &mut app, "Add layer")?;
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
    tap(&ctx, &mut app, "Tune")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.stack_edit = Some(("L2".into(), "copper".into(), 0.05));
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    tap(&ctx, &mut app, "Apply")?;
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
    tap(&ctx, &mut app, "Back to a plain two-layer board")?;
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
    tap(&ctx, &mut app, "Layers")?;
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
                egui::Event::PointerMoved(egui::pos2(150.0, 500.0)),
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
    tap(&ctx, &mut app, "Four layers")?;
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
    tap(&ctx, &mut app, "Two layers · 1.6 mm")?;
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
    tap(&ctx, &mut app, "Layers")?;
    let mut shown = false;
    for _ in 0..14 {
        if paints_text(&ctx, &mut app, "BOARD ANATOMY")? {
            shown = true;
            break;
        }
        frame(
            &ctx,
            &mut app,
            vec![
                egui::Event::PointerMoved(egui::pos2(150.0, 500.0)),
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
        "the layers dock should list every piece of the board: {}",
        app.message
    );
    let mut seen = false;
    for _ in 0..12 {
        if paints_text(&ctx, &mut app, "Top copper wires")? {
            seen = true;
            break;
        }
        frame(
            &ctx,
            &mut app,
            vec![
                egui::Event::PointerMoved(egui::pos2(150.0, 500.0)),
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
        seen,
        "the anatomy list should show the top copper wires layer: {}",
        app.message
    );
    tap(&ctx, &mut app, "Top copper wires")?;
    let mut row = false;
    for _ in 0..18 {
        if paints_text(&ctx, &mut app, &label)? {
            row = true;
            break;
        }
        frame(
            &ctx,
            &mut app,
            vec![
                egui::Event::PointerMoved(egui::pos2(150.0, 500.0)),
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
        row,
        "the top copper layer should show the drawn wire {label}: {}",
        app.message
    );
    tap(&ctx, &mut app, &label)?;
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
    tap(&ctx, &mut app, "Settings")?;
    assert!(
        scrolled_to_at(&ctx, &mut app, "BOARD LABELS", 300.0)?,
        "the board panel should offer printed words: {}",
        app.message
    );
    app.label_text = "Widget One".into();
    app.label_x = 3.0;
    app.label_y = 2.0;
    app.label_size = 1.2;
    app.label_front = false;
    scrolled_to_at(&ctx, &mut app, "Add label", 300.0)?;
    tap(&ctx, &mut app, "Add label")?;
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
    tap(&ctx, &mut app, "Erase")?;
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
    tap(&ctx, &mut app, "Rules")?;
    for _ in 0..22 {
        frame(
            &ctx,
            &mut app,
            vec![
                egui::Event::PointerMoved(egui::pos2(150.0, 500.0)),
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
        scrolled_to_at(&ctx, &mut app, "BOARD AREAS", 300.0)?,
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
    tap(&ctx, &mut app, "Add area")?;
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
    tap(&ctx, &mut app, "Erase area")?;
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
    tap(&ctx, &mut app, "Settings")?;
    assert!(
        paints_text(&ctx, &mut app, "CUT-OUT HOLE")?,
        "the board geometry section should offer a hole: {}",
        app.message
    );
    let before = app.project.board.cutouts.len();
    app.cut_x = 25.0;
    app.cut_y = 20.0;
    app.cut_r = 1.5;
    tap(&ctx, &mut app, "Add hole")?;
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
fn a_newcomer_drills_mounting_holes_into_the_board() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    app.tab = Tab::Pcb;
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    tap(&ctx, &mut app, "Mounting hole")?;
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
    tap(&ctx, &mut app, "Layers")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        scrolled_to(&ctx, &mut app, "3.20 mm")?,
        "the holes are listed with their drill size"
    );
    let id = app.project.board.holes[0].id;
    tap(&ctx, &mut app, "Tune")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.hole_edit = Some((id, 3.0));
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    tap(&ctx, &mut app, "Apply")?;
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
    tap(&ctx, &mut app, "\u{d7}")?;
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
    tap(&ctx, &mut app, "Draw shape")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    tap(&ctx, &mut app, "New layer")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert_eq!(
        app.project.board.user_layers.len(),
        1,
        "the new layer button should add a layer: {}",
        app.message
    );
    tap(&ctx, &mut app, "Line")?;
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
    tap(&ctx, &mut app, "Words")?;
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
