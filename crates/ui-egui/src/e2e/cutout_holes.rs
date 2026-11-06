//! A newcomer opens the showcase board and can select, edit and delete its
//! cut-out and mounting holes from the Layers tree, and undo the deletes.

use super::super::*;
use super::support::*;

fn showcase_layout() -> Result<(egui::Context, App), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.home_open = true;
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Showcase board")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.change_tab(Tab::Pcb);
    app.region().left_tab = "layers".to_owned();
    app.tree.expanded.insert("edge".to_owned());
    app.tree.expanded.insert("drills".to_owned());
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    Ok((ctx, app))
}

/// Scroll the tree to the row of object `id`, then clear the selection so a
/// click on the row is what selects it.
fn scroll_to_row(ctx: &egui::Context, app: &mut App, id: Id) {
    app.select(Some(id));
    for _ in 0..4 {
        frame(ctx, app, vec![], egui::Modifiers::NONE);
    }
    app.select(None);
    frame(ctx, app, vec![], egui::Modifiers::NONE);
}

/// Corners of the traced outline, not counting a repeated first point.
fn corner_count(app: &App) -> usize {
    app.project.board.outline.as_ref().map_or(0, |p| {
        p.len() - usize::from(p.first() == p.last() && p.len() > 1)
    })
}

/// The colour a row label was painted in, if the label is in the left aside.
fn label_colour(ctx: &egui::Context, app: &mut App, label: &str) -> Option<egui::Color32> {
    let output = frame(ctx, app, vec![], egui::Modifiers::NONE);
    output.shapes.iter().find_map(|shape| match &shape.shape {
        egui::Shape::Text(t) if t.galley.text() == label && t.pos.x < 320.0 => {
            t.galley.job.sections.first().map(|s| s.format.color)
        }
        _ => None,
    })
}

/// Deleting the cut-out from the tree removes it from the board and the tree,
/// and undo brings it back.
#[test]
fn a_newcomer_deletes_the_showcase_cutout() -> Result<(), String> {
    let (ctx, mut app) = showcase_layout()?;
    delete_cutout(&ctx, &mut app)
}

fn delete_cutout(ctx: &egui::Context, app: &mut App) -> Result<(), String> {
    let id = app.project.board.cutouts.first().ok_or("no cut-out")?.id;
    let label = t!("tree.cutout");
    scroll_to_row(ctx, app, id);
    tap_left(ctx, app, &label)?;
    assert_eq!(app.selection(), vec![id], "the row selects the cut-out");
    key(ctx, app, egui::Key::Delete);
    frame(ctx, app, vec![], egui::Modifiers::NONE);
    assert!(app.project.board.cutouts.is_empty(), "the cut-out is gone");
    assert!(app.selection().is_empty(), "nothing stale stays selected");
    assert!(!paints_text(ctx, app, &label)?, "the tree row is gone too");
    key(ctx, app, egui::Key::Z);
    app.tool_call("undo", json!({}));
    frame(ctx, app, vec![], egui::Modifiers::NONE);
    assert_eq!(app.project.board.cutouts.len(), 1, "undo brings it back");
    Ok(())
}

/// Mounting hole rows read as editable, select, delete, and leave the other
/// holes selectable.
#[test]
fn a_newcomer_deletes_a_showcase_mounting_hole() -> Result<(), String> {
    let (ctx, mut app) = showcase_layout()?;
    delete_hole(&ctx, &mut app)
}

fn delete_hole(ctx: &egui::Context, app: &mut App) -> Result<(), String> {
    let label = t!("tree.mounting_hole");
    let before = app.project.board.holes.len();
    assert_eq!(before, 4, "the showcase has four mounting holes");
    let first_id = app.project.board.holes.first().ok_or("no hole")?.id;
    scroll_to_row(ctx, app, first_id);
    let hole = label_colour(ctx, app, &label).ok_or("no hole row")?;
    let edge = label_colour(ctx, app, &t!("tree.cutout")).ok_or("no cut-out row")?;
    assert_eq!(hole, edge, "an editable hole row is not greyed");
    assert_eq!(
        hole,
        app.colours().ink,
        "the hole row is painted in full ink"
    );
    tap_left(ctx, app, &label)?;
    let first = app.selection();
    assert_eq!(first.len(), 1, "the row selects one hole");
    assert!(
        app.project.board.holes.iter().any(|h| h.id == first[0]),
        "the selection is a hole"
    );
    key(ctx, app, egui::Key::Delete);
    frame(ctx, app, vec![], egui::Modifiers::NONE);
    assert_eq!(
        app.project.board.holes.len(),
        before - 1,
        "one hole is gone"
    );
    assert!(app.selection().is_empty(), "nothing stale stays selected");
    tap_left(ctx, app, &label)?;
    let next = app.selection();
    assert_eq!(next.len(), 1, "another hole row still selects");
    assert!(next[0] != first[0], "it is a different hole");
    app.tool_call("undo", json!({}));
    frame(ctx, app, vec![], egui::Modifiers::NONE);
    assert_eq!(app.project.board.holes.len(), before, "undo brings it back");
    Ok(())
}

/// The outline row names the shape, shows its size, is not greyed and picks
/// the board outline when clicked.
#[test]
fn a_newcomer_selects_the_board_outline_row() -> Result<(), String> {
    let (ctx, mut app) = showcase_layout()?;
    let id = app.project.board.cutouts.first().ok_or("no cut-out")?.id;
    scroll_to_row(&ctx, &mut app, id);
    let corners = corner_count(&app);
    let label = t!("tree.edge.free_form", n = corners);
    let row = label_colour(&ctx, &mut app, &label).ok_or("no outline row")?;
    let hole = label_colour(&ctx, &mut app, &t!("tree.cutout")).ok_or("no cut-out row")?;
    assert_eq!(row, hole, "the outline row is not greyed");
    tap_left(&ctx, &mut app, &label)?;
    assert!(
        app.tree
            .selected_key
            .as_deref()
            .is_some_and(|k| k.ends_with("/edge")),
        "the click picks the board outline: {:?}",
        app.tree.selected_key
    );
    Ok(())
}

/// A traced free-form outline keeps the outline row, the cut-out and the
/// holes working: select, delete and undo.
#[test]
fn a_newcomer_edits_objects_on_an_l_shaped_board() -> Result<(), String> {
    let (ctx, mut app) = showcase_layout()?;
    assert!(
        app.tool_call(
            "board.set_shape",
            json!({"kind":"l_shape","width":72.0,"height":60.0,"notch_width":10.0,"notch_height":10.0}),
        ),
        "the L shape is accepted: {}",
        app.message
    );
    app.refresh();
    let corners = corner_count(&app);
    assert_eq!(corners, 6, "an L has six corners");
    let id = app.project.board.cutouts.first().ok_or("no cut-out")?.id;
    scroll_to_row(&ctx, &mut app, id);
    assert!(
        paints_text(&ctx, &mut app, &t!("tree.edge.free_form", n = 6))?,
        "the outline row names the shape: {}",
        app.message
    );
    delete_cutout(&ctx, &mut app)?;
    delete_hole(&ctx, &mut app)
}

/// The cut-out and a hole picked on the drawing delete with the Delete key.
#[test]
fn a_newcomer_deletes_picked_objects_from_the_drawing() -> Result<(), String> {
    let (ctx, mut app) = showcase_layout()?;
    let cut = app
        .project
        .board
        .cutouts
        .first()
        .ok_or("no cut-out")?
        .clone();
    app.canvas_click(Point::new(cut.center.x + cut.radius, cut.center.y), 10.0);
    assert_eq!(app.selection(), vec![cut.id], "the ring picks the cut-out");
    key(&ctx, &mut app, egui::Key::Delete);
    assert!(app.project.board.cutouts.is_empty(), "the cut-out is gone");
    assert!(app.selection().is_empty(), "nothing stale stays selected");
    let hole = app.project.board.holes.first().ok_or("no hole")?.clone();
    app.canvas_click(hole.position, 10.0);
    assert_eq!(app.selection(), vec![hole.id], "the click picks the hole");
    key(&ctx, &mut app, egui::Key::Delete);
    assert_eq!(app.project.board.holes.len(), 3, "the hole is gone");
    assert!(app.selection().is_empty(), "nothing stale stays selected");
    Ok(())
}
