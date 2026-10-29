//! Stories for putting a part on the bottom and looking at the board from below.
use super::super::*;
use super::support::*;

fn open_practice_board() -> Result<(App, egui::Context), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    app.change_tab(Tab::Pcb);
    app.select(Some(10));
    app.refresh();
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    Ok((app, ctx))
}

fn painted_in(ctx: &egui::Context, app: &mut App, colour: egui::Color32) -> bool {
    frame(ctx, app, vec![], egui::Modifiers::NONE)
        .shapes
        .iter()
        .any(|s| match &s.shape {
            egui::Shape::Path(path) => path.fill == colour,
            egui::Shape::Mesh(_) => false,
            _ => false,
        })
}

#[test]
fn a_newcomer_flips_a_part_to_the_bottom() -> Result<(), String> {
    let (mut app, ctx) = open_practice_board()?;
    let back = app.colours().canvas.back;
    assert!(
        !painted_in(&ctx, &mut app, back),
        "on the top no pad is drawn in the bottom colour"
    );
    // Side is a Top | Bottom control in the Position section of Properties.
    tap_properties(&ctx, &mut app, "radiobutton:Bottom")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.refresh();
    let side = app
        .project
        .footprint(10)
        .map(|f| f.board_side())
        .ok_or("the practice part should still be on the board")?;
    assert_eq!(
        side,
        pcbcraft_doc::Layer::Back,
        "the part is on the bottom: {}",
        app.message
    );
    assert!(
        painted_in(&ctx, &mut app, back),
        "the flipped part draws its pads in the bottom colour"
    );
    let placement = pcbcraft_io::placement_text(&app.project)?;
    assert!(
        placement.lines().any(|l| l.contains(",B,")),
        "the placement file lists the part as bottom: {placement}"
    );
    tap_properties(&ctx, &mut app, "radiobutton:Top")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.refresh();
    assert_eq!(
        app.project.footprint(10).map(|f| f.board_side()),
        Some(pcbcraft_doc::Layer::Front),
        "the Top choice flips the part back"
    );
    Ok(())
}

#[test]
fn a_newcomer_looks_at_the_board_from_below() -> Result<(), String> {
    let (mut app, ctx) = open_practice_board()?;
    let rect = app.canvas_rect;
    let left_before = app.pcb.screen(pcbcraft_geom::Point::new(0.0, 0.0), rect).x;
    let right_before = app
        .pcb
        .screen(
            pcbcraft_geom::Point::new(app.project.board.width, 0.0),
            rect,
        )
        .x;
    assert!(
        left_before < right_before,
        "the top view runs left to right"
    );
    authoring::tests::tap(&ctx, &mut app, "From below")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(app.preferences.view_from_below, "the toggle is on");
    let rect = app.canvas_rect;
    let left = app.pcb.screen(pcbcraft_geom::Point::new(0.0, 0.0), rect).x;
    let right = app
        .pcb
        .screen(
            pcbcraft_geom::Point::new(app.project.board.width, 0.0),
            rect,
        )
        .x;
    assert!(left > right, "the board outline is mirrored left to right");
    let reference = app
        .project
        .components
        .first()
        .map(|c| c.reference.clone())
        .ok_or("the practice design has parts")?;
    let upright = frame(&ctx, &mut app, vec![], egui::Modifiers::NONE)
        .shapes
        .iter()
        .any(|s| match &s.shape {
            egui::Shape::Text(t) => t.galley.text() == reference && t.angle == 0.0,
            _ => false,
        });
    assert!(
        upright,
        "the reference text {reference} stays upright and readable"
    );
    authoring::tests::tap(&ctx, &mut app, "From below")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(!app.preferences.view_from_below, "pressing again returns");
    let back = app
        .pcb
        .screen(pcbcraft_geom::Point::new(0.0, 0.0), app.canvas_rect)
        .x;
    assert!(back < right_before, "the top view runs left to right again");
    Ok(())
}

#[test]
fn bottom_copper_draws_on_top_when_seen_from_below() {
    let order: Vec<&u8> = crate::board_side::bottom_last([1u8, 0, 1, 0].iter(), true, |v| *v == 1);
    assert_eq!(order, vec![&0, &0, &1, &1], "bottom items paint last");
    let same: Vec<&u8> = crate::board_side::bottom_last([1u8, 0, 1, 0].iter(), false, |v| *v == 1);
    assert_eq!(same, vec![&1, &0, &1, &0], "the top view keeps its order");
}

#[test]
fn a_newcomer_drops_a_part_onto_the_bottom_layer() -> Result<(), String> {
    let (mut app, ctx) = open_practice_board()?;
    app.route_layer = pcbcraft_doc::Layer::Back;
    app.run(
        "component.add",
        json!({"kind":"resistor","position":[12,12],"reference":"R90"}),
    );
    app.run("board.sync", json!({}));
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let component = app
        .project
        .components
        .iter()
        .find(|c| c.reference == "R90")
        .map(|c| c.id)
        .ok_or("the new part should exist")?;
    let footprint = app
        .project
        .board
        .footprints
        .iter()
        .find(|f| f.component == component)
        .ok_or("the new part should reach the board")?;
    assert_eq!(
        footprint.board_side(),
        pcbcraft_doc::Layer::Back,
        "it lands on the bottom with no extra click"
    );
    Ok(())
}
