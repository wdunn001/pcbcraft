//! Stories for the library browser: dragging a part onto the drawing and the
//! card's right-click menu. Each starts at the Library tab of the left aside.

use super::super::*;
use super::support::*;

/// A new app on the sample design with the Library tab showing.
fn library_open() -> Result<(egui::Context, App), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.region().left_tab = "library".to_owned();
    app.tool_call("sample_design", json!({}));
    let ctx = egui::Context::default();
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    Ok((ctx, app))
}

/// Where the last copy of `label` was painted, if it is on screen.
fn painted_at(output: &egui::FullOutput, label: &str) -> Option<egui::Pos2> {
    let mut found = None;
    for shape in &output.shapes {
        if let egui::Shape::Text(text) = &shape.shape
            && text.galley.text() == label
        {
            found = Some(text.pos + text.galley.size() / 2.0);
        }
    }
    found
}

fn button(pos: egui::Pos2, button: egui::PointerButton, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button,
        pressed,
        modifiers: egui::Modifiers::NONE,
    }
}

/// Press on a card, pull it onto the drawing and let go there.
fn drag_card_to(
    ctx: &egui::Context,
    app: &mut App,
    label: &str,
    to: egui::Pos2,
) -> Result<(), String> {
    let output = frame(ctx, app, vec![], egui::Modifiers::NONE);
    let from = painted_at(&output, label).ok_or_else(|| format!("no card called {label}"))?;
    let primary = egui::PointerButton::Primary;
    frame(
        ctx,
        app,
        vec![egui::Event::PointerMoved(from)],
        egui::Modifiers::NONE,
    );
    frame(
        ctx,
        app,
        vec![button(from, primary, true)],
        egui::Modifiers::NONE,
    );
    for step in 1..=6 {
        let at = from + (to - from) * (step as f32 / 6.0);
        frame(
            ctx,
            app,
            vec![egui::Event::PointerMoved(at)],
            egui::Modifiers::NONE,
        );
    }
    frame(
        ctx,
        app,
        vec![egui::Event::PointerMoved(to), button(to, primary, false)],
        egui::Modifiers::NONE,
    );
    frame(ctx, app, vec![], egui::Modifiers::NONE);
    Ok(())
}

#[test]
fn a_newcomer_drags_a_part_from_the_library_onto_the_schematic() -> Result<(), String> {
    let (ctx, mut app) = library_open()?;
    assert_eq!(app.tab, Tab::Schematic, "the sample opens on the schematic");
    let before = app.project.components.len();
    let drop = egui::pos2(900.0, 500.0);
    drag_card_to(&ctx, &mut app, "Resistor", drop)?;
    assert_eq!(
        app.project.components.len(),
        before + 1,
        "letting go over the schematic adds the part: {}",
        app.message
    );
    let placed = app
        .project
        .components
        .last()
        .ok_or("the new part should be last")?;
    let at = canvas::snap(app.schematic.world(drop, app.canvas_rect));
    assert!(
        (placed.position.x - at.x).abs() < 1e-6 && (placed.position.y - at.y).abs() < 1e-6,
        "the symbol lands where the pointer let go: {:?} vs {:?}",
        placed.position,
        at
    );
    assert!(
        app.project
            .board
            .footprints
            .iter()
            .all(|f| f.component != placed.id),
        "dropping on the schematic does not put anything on the board"
    );
    Ok(())
}

#[test]
fn a_newcomer_drags_a_part_from_the_library_onto_the_board() -> Result<(), String> {
    let (ctx, mut app) = library_open()?;
    app.change_tab(Tab::Pcb);
    app.region().left_tab = "library".to_owned();
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    assert_eq!(
        app.dock.view,
        library_dock::ViewKind::Footprints,
        "the cards show footprints on the Layout"
    );
    let parts = app.project.components.len();
    let pads = app.project.board.footprints.len();
    let drop = egui::pos2(900.0, 500.0);
    drag_card_to(&ctx, &mut app, "Resistor", drop)?;
    assert_eq!(
        app.project.board.footprints.len(),
        pads + 1,
        "letting go over the board places the footprint: {}",
        app.message
    );
    assert_eq!(
        app.project.components.len(),
        parts + 1,
        "the part joins the schematic as well"
    );
    let footprint = app
        .project
        .board
        .footprints
        .last()
        .ok_or("the new footprint should be last")?;
    let at = canvas::snap(app.pcb.world(drop, app.canvas_rect));
    assert!(
        (footprint.position.x - at.x).abs() < 1e-6 && (footprint.position.y - at.y).abs() < 1e-6,
        "the footprint lands where the pointer let go: {:?} vs {:?}",
        footprint.position,
        at
    );
    let part = app
        .project
        .components
        .iter()
        .find(|c| c.id == footprint.component)
        .ok_or("the footprint belongs to a part")?;
    assert_eq!(
        part.position.x, 0.0,
        "the schematic symbol waits unplaced at the left edge, not at the board spot"
    );
    Ok(())
}

#[test]
fn a_newcomer_places_a_part_from_its_right_click_menu() -> Result<(), String> {
    let (ctx, mut app) = library_open()?;
    let before = app.project.components.len();
    let output = frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let card = painted_at(&output, "Resistor").ok_or("no Resistor card")?;
    let secondary = egui::PointerButton::Secondary;
    frame(
        &ctx,
        &mut app,
        vec![egui::Event::PointerMoved(card)],
        egui::Modifiers::NONE,
    );
    frame(
        &ctx,
        &mut app,
        vec![
            button(card, secondary, true),
            button(card, secondary, false),
        ],
        egui::Modifiers::NONE,
    );
    assert!(
        paints_text(&ctx, &mut app, "Place on schematic")?,
        "a right click on a card opens its menu"
    );
    assert!(
        paints_text(&ctx, &mut app, "Show source and licence")?,
        "the menu can say where the part comes from"
    );
    authoring::tests::tap(&ctx, &mut app, "Place on schematic")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert_eq!(
        app.project.components.len(),
        before + 1,
        "the menu entry places the part: {}",
        app.message
    );
    Ok(())
}
