use super::super::*;
use super::support::*;

#[test]
fn a_newcomer_rounds_a_board_corner() -> Result<(), String> {
    let (ctx, mut app) = pcb_app();
    app.run(
        "board.set_outline",
        json!({"points": [
            {"x":0.0,"y":0.0},
            {"x":40.0,"y":0.0},
            {"x":40.0,"y":30.0},
            {"x":0.0,"y":30.0}
        ]}),
    );
    app.tool = Tool::Select;
    assert!(
        app.try_pick_outline(Point::new(40.0, 15.0), 15.0),
        "clicking the right edge selects it for editing"
    );
    app.outline_ui.radius = 20.0;
    app.outline_ui.convex = true;
    assert!(
        paints_text(&ctx, &mut app, "Board edge")?,
        "the inspector names the board edge section"
    );
    authoring::tests::tap(&ctx, &mut app, "Round edge")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        app.project.board.outline_arcs.get(1).and_then(|m| *m).is_some(),
        "the corner is rounded: {}",
        app.message
    );
    assert!(
        app.project.board.width > 40.0,
        "an outward bow widens the board"
    );
    assert!(
        app.try_pick_outline(Point::new(40.0, 15.0), 15.0),
        "the edge stays selectable after the bend"
    );
    authoring::tests::tap(&ctx, &mut app, "Straighten edge")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        app.project.board.outline_arcs.is_empty()
            || app.project.board.outline_arcs.get(1).and_then(|m| *m).is_none(),
        "straighten returns the edge to a line: {}",
        app.message
    );
    Ok(())
}
