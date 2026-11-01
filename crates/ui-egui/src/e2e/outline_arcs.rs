use super::super::*;
use super::layers::pcb_app;
use super::support::*;

#[test]
fn a_newcomer_rounds_a_board_corner() -> Result<(), String> {
    let (ctx, mut app) = pcb_app();
    // The practice board has a cutout, and an outline may not cut through it,
    // so the traced outline is the board's own rectangle.
    let (w, h) = (app.project.board.width, app.project.board.height);
    app.run(
        "board.set_outline",
        json!({"points": [
            {"x":0.0,"y":0.0},
            {"x":w,"y":0.0},
            {"x":w,"y":h},
            {"x":0.0,"y":h}
        ]}),
    );
    app.tool = Tool::Select;
    assert!(
        app.try_pick_outline(Point::new(w, h / 2.0), 15.0),
        "clicking the right edge selects it for editing: {}",
        app.message
    );
    // A bow needs a radius a little over half the edge it spans.
    app.outline_ui.radius = h * 0.75;
    app.outline_ui.convex = true;
    assert!(
        paints_text(&ctx, &mut app, "Board edge")?,
        "the inspector names the board edge section"
    );
    authoring::tests::tap(&ctx, &mut app, "Round edge")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        app.project
            .board
            .outline_arcs
            .get(1)
            .and_then(|m| *m)
            .is_some(),
        "the corner is rounded: {}",
        app.message
    );
    assert!(
        app.project.board.width > w,
        "an outward bow widens the board"
    );
    let bow = app
        .project
        .board
        .outline_arcs
        .get(1)
        .and_then(|m| *m)
        .ok_or("the bowed edge keeps its midpoint")?;
    assert!(
        app.try_pick_outline(bow, 15.0),
        "the edge stays selectable after the bend"
    );
    authoring::tests::tap(&ctx, &mut app, "Straighten edge")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        app.project.board.outline_arcs.is_empty()
            || app
                .project
                .board
                .outline_arcs
                .get(1)
                .and_then(|m| *m)
                .is_none(),
        "straighten returns the edge to a line: {}",
        app.message
    );
    Ok(())
}
