//! Stories for the shape of each copper layer on the Layout tab.
//!
//! Context: the board is a free-form shape and each copper layer may have a
//! shape of its own. These stories drive the tree, the context menu, the
//! Properties page and the canvas handles the way a person would.
use super::super::*;
use super::board::right_click;
use super::support::*;

fn undo(app: &mut App) -> Result<(), String> {
    app.session
        .lock()
        .map_err(|_| "session is busy".to_owned())?
        .undo()
        .map_err(|e| e.to_string())?;
    app.refresh();
    Ok(())
}

/// An empty board of 72 by 60 millimetres on the Layout tab.
fn empty_board() -> Result<(egui::Context, App), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("new_design", json!({}));
    app.run(
        "board.set_rectangle",
        json!({"width": 72.0, "height": 60.0}),
    );
    app.change_tab(Tab::Pcb);
    app.needs_fit = true;
    app.tool = Tool::Select;
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    Ok((ctx, app))
}

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

fn painted_texts(ctx: &egui::Context, app: &mut App) -> Vec<(String, egui::Pos2)> {
    let output = frame(ctx, app, vec![], egui::Modifiers::NONE);
    output
        .shapes
        .iter()
        .filter_map(|s| match &s.shape {
            egui::Shape::Text(t) => Some((t.galley.text().to_owned(), t.pos)),
            _ => None,
        })
        .collect()
}

fn l_shape() -> serde_json::Value {
    json!([
        {"x":0.0,"y":0.0},{"x":40.0,"y":0.0},{"x":40.0,"y":10.0},
        {"x":10.0,"y":10.0},{"x":10.0,"y":30.0},{"x":0.0,"y":30.0}
    ])
}

fn u_shape() -> serde_json::Value {
    json!([
        {"x":0.0,"y":0.0},{"x":10.0,"y":0.0},{"x":10.0,"y":20.0},{"x":30.0,"y":20.0},
        {"x":30.0,"y":0.0},{"x":40.0,"y":0.0},{"x":40.0,"y":30.0},{"x":0.0,"y":30.0}
    ])
}

fn drag(ctx: &egui::Context, app: &mut App, from: Point, to: Point) {
    let (a, b) = (pcb_screen(app, from), pcb_screen(app, to));
    super::modifiers::drag(ctx, app, a, b, egui::Modifiers::NONE);
}

#[test]
fn a_newcomer_reads_the_board_shape_and_no_size_in_the_tree() -> Result<(), String> {
    let (ctx, mut app) = empty_board()?;
    assert!(
        paints_text(&ctx, &mut app, "Board shape")?,
        "the top row is the board shape: {}",
        app.message
    );
    assert!(
        paints_text(&ctx, &mut app, "Rectangle, 4 corners")?,
        "the row says what the shape is"
    );
    let tree_edge = left_aside();
    let sized: Vec<String> = painted_texts(&ctx, &mut app)
        .into_iter()
        .filter(|(text, at)| at.x < tree_edge && text.contains(" x "))
        .map(|(text, _)| text)
        .collect();
    assert!(
        sized.is_empty(),
        "no tree row carries a width times height: {sized:?}"
    );
    app.run("board.set_outline", json!({"points": l_shape()}));
    assert!(
        paints_text(&ctx, &mut app, "Free-form, 6 corners")?,
        "an L shaped board reads as free-form with its corners"
    );
    Ok(())
}

#[test]
fn a_newcomer_gives_the_top_layer_its_own_shape_and_edits_only_that() -> Result<(), String> {
    let (ctx, mut app) = empty_board()?;
    let board_before = app.project.board.outline.clone();
    let row = text_at(&ctx, &mut app, "Layer 1 \u{b7} Top").ok_or("the top layer row")?;
    right_click(&ctx, &mut app, row);
    authoring::tests::tap(&ctx, &mut app, "Give this layer its own shape")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let own = app
        .project
        .board
        .layer_shapes
        .get("F.Cu")
        .ok_or_else(|| format!("F.Cu holds its own shape: {}", app.message))?;
    assert_eq!(
        Some(&own.outline),
        board_before.as_ref(),
        "the new shape starts as a copy of the board outline"
    );
    assert!(
        paints_text(&ctx, &mut app, "Own shape")?,
        "the layer row carries the Own shape tag"
    );
    assert!(
        paints_text(&ctx, &mut app, "Editing the own shape of F.Cu")?,
        "the status line says which shape the handles change: {}",
        app.shape_status()
    );
    // Drag the far corner of the shape; the handles edit F.Cu only.
    app.outline_ui.clear_pick();
    drag(
        &ctx,
        &mut app,
        Point::new(72.0, 0.0),
        Point::new(60.0, 10.0),
    );
    let moved = app
        .project
        .board
        .layer_shapes
        .get("F.Cu")
        .map(|s| s.outline.clone())
        .ok_or("F.Cu keeps its shape")?;
    assert!(
        moved.contains(&Point::new(60.0, 10.0)),
        "the corner moved on F.Cu: {moved:?} {} screen {:?} canvas {:?} zoom {} seg {:?}",
        app.message,
        pcb_screen(&app, Point::new(72.0, 0.0)),
        app.canvas_rect,
        app.pcb.zoom,
        app.outline_ui.segment
    );
    assert_eq!(
        app.project.board.outline, board_before,
        "the board outline is untouched"
    );
    undo(&mut app)?;
    assert!(
        app.project
            .board
            .layer_shapes
            .get("F.Cu")
            .is_some_and(|s| s.outline.contains(&Point::new(72.0, 0.0))),
        "undo puts the corner back"
    );
    // Follow the board shape again.
    let row = text_at(&ctx, &mut app, "Layer 1 \u{b7} Top").ok_or("the top layer row")?;
    right_click(&ctx, &mut app, row);
    authoring::tests::tap(&ctx, &mut app, "Follow the board shape")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        app.project.board.layer_shapes.is_empty(),
        "the layer follows the board again: {}",
        app.message
    );
    assert!(
        !paints_text(&ctx, &mut app, "Own shape")?,
        "the tag goes away with the shape"
    );
    undo(&mut app)?;
    assert!(
        app.project.board.layer_shapes.contains_key("F.Cu"),
        "undo brings the own shape back"
    );
    undo(&mut app)?;
    undo(&mut app)?;
    assert!(
        app.project.board.layer_shapes.is_empty(),
        "undoing the earlier steps leaves the layer on the board shape"
    );
    Ok(())
}

#[test]
fn a_newcomer_edits_the_board_shape_when_the_layer_follows_it() -> Result<(), String> {
    let (ctx, mut app) = empty_board()?;
    open_tree_row(&ctx, &mut app, "edge");
    assert!(
        paints_text(
            &ctx,
            &mut app,
            "Editing the board shape, used by every layer that follows it"
        )?,
        "the status line names the board shape: {}",
        app.shape_status()
    );
    drag(
        &ctx,
        &mut app,
        Point::new(72.0, 0.0),
        Point::new(60.0, 10.0),
    );
    assert!(
        app.project
            .board
            .outline
            .as_ref()
            .is_some_and(|o| o.contains(&Point::new(60.0, 10.0))),
        "the board outline moved: {}",
        app.message
    );
    assert!(app.project.board.layer_shapes.is_empty());
    // A segment is picked: add a corner, then take it away.
    assert!(app.try_pick_outline(Point::new(30.0, 5.0), 20.0));
    let before = app.project.board.outline.as_ref().map_or(0, Vec::len);
    authoring::tests::tap(&ctx, &mut app, "Add a corner")?;
    let added = app.project.board.outline.as_ref().map_or(0, Vec::len);
    assert_eq!(added, before + 1, "a corner was added: {}", app.message);
    assert!(app.try_pick_outline(Point::new(45.0, 7.5), 20.0));
    authoring::tests::tap(&ctx, &mut app, "Remove this corner")?;
    let removed = app.project.board.outline.as_ref().map_or(0, Vec::len);
    assert_eq!(removed, before, "the corner was removed: {}", app.message);
    Ok(())
}

#[test]
fn a_newcomer_sees_corner_counts_of_an_l_and_a_u_shaped_layer() -> Result<(), String> {
    let (ctx, mut app) = empty_board()?;
    app.run(
        "board.layer_shape_set",
        json!({"layer": "F.Cu", "points": l_shape()}),
    );
    app.run(
        "board.layer_shape_set",
        json!({"layer": "B.Cu", "points": u_shape()}),
    );
    open_tree_row(&ctx, &mut app, "side:top");
    assert!(
        paints_text(&ctx, &mut app, "Free-form, 6 corners")?,
        "the top layer shows its six corners: {}",
        app.message
    );
    open_tree_row(&ctx, &mut app, "side:bottom");
    assert!(
        paints_text(&ctx, &mut app, "Free-form, 8 corners")?,
        "the bottom layer shows its eight corners: {}",
        app.message
    );
    // The two shapes overlap and both are on the plan; the active one is solid.
    let plan = app.shape_paint_plan();
    assert_eq!(plan.len(), 2, "both own shapes are drawn: {plan:?}");
    assert_eq!(
        plan.iter()
            .filter(|p| p.active)
            .map(|p| p.layer.as_str())
            .collect::<Vec<_>>(),
        vec!["F.Cu"],
        "only the active layer is solid"
    );
    Ok(())
}

#[test]
fn a_newcomer_hides_a_layer_and_its_shape_goes_with_it() -> Result<(), String> {
    let (ctx, mut app) = empty_board()?;
    app.run(
        "board.layer_shape_set",
        json!({"layer": "F.Cu", "points": l_shape()}),
    );
    app.run(
        "board.layer_shape_set",
        json!({"layer": "B.Cu", "points": u_shape()}),
    );
    assert_eq!(app.shape_paint_plan().len(), 2);
    tap_id(&ctx, &mut app, "button:Hide Layer # \u{b7} Bottom")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let plan = app.shape_paint_plan();
    assert_eq!(
        plan.iter().map(|p| p.layer.as_str()).collect::<Vec<_>>(),
        vec!["F.Cu"],
        "the hidden layer's shape is not drawn"
    );
    Ok(())
}

#[test]
fn a_newcomer_finds_a_sub_layer_follows_its_copper_layer() -> Result<(), String> {
    let (ctx, mut app) = empty_board()?;
    open_tree_row(&ctx, &mut app, "side:top/silk");
    assert!(
        paints_text(&ctx, &mut app, "Follows the shape of F.Cu")?,
        "the silkscreen says it follows F.Cu: {}",
        app.message
    );
    let names: Vec<String> = interactive_rects(&ctx, &mut app)
        .into_iter()
        .map(|(n, _)| n)
        .collect();
    assert!(
        !names.iter().any(|n| n.contains("own shape")
            || n.contains("Follow the board shape")
            || n.contains("Start from a rectangle")),
        "a sub-layer offers no shape action: {names:?}"
    );
    Ok(())
}

#[test]
fn a_newcomer_finds_no_board_width_or_height_field_on_the_layout_tab() -> Result<(), String> {
    let (ctx, mut app) = empty_board()?;
    for key in ["root", "edge", "side:top", "side:bottom"] {
        open_tree_row(&ctx, &mut app, key);
        let names: Vec<String> = interactive_rects(&ctx, &mut app)
            .into_iter()
            .map(|(n, _)| n.to_lowercase())
            .collect();
        assert!(
            !names
                .iter()
                .any(|n| n.contains("board width") || n.contains("board height")),
            "{key} has no board width or height control: {names:?}"
        );
    }
    open_tree_row(&ctx, &mut app, "root");
    assert!(
        paints_text(&ctx, &mut app, "Reaches")?,
        "the board properties say how far the board reaches"
    );
    assert!(
        paints_text(&ctx, &mut app, "72 x 60 mm")?,
        "the reach is a read-only sentence"
    );
    Ok(())
}

#[test]
fn a_newcomer_starts_from_a_rectangle_in_a_small_window() -> Result<(), String> {
    let (ctx, mut app) = empty_board()?;
    open_tree_row(&ctx, &mut app, "root");
    tap_id(&ctx, &mut app, "button:Start from a rectangle")?;
    assert!(
        paints_text(&ctx, &mut app, "Type the size of the rectangle")?,
        "the window opens: {}",
        app.message
    );
    type_into(&ctx, &mut app, "spinbutton:Rectangle width", "80")?;
    type_into(&ctx, &mut app, "spinbutton:Rectangle height", "50")?;
    tap_id(&ctx, &mut app, "button:Make rectangle")?;
    let (w, h) = app.project.board.extent_size();
    assert!(
        (w - 80.0).abs() < 1e-6 && (h - 50.0).abs() < 1e-6,
        "the board is now 80 by 50: {w} {h} {}",
        app.message
    );
    tap_id(&ctx, &mut app, "button:Replace from SVG or DXF")?;
    assert!(app.edit.dialog.open, "the import window opens");
    Ok(())
}
