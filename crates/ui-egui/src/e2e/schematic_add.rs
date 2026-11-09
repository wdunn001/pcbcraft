//! Stories for the Schematic tool bar after the slim-down (#1171): one Add menu
//! for labels, power, buses, ports and flags, no Wire or Place part button, and
//! placing that keeps going until Esc.

use super::super::*;
use super::board::right_click;
use super::modifiers::{centre_of, click, drag, sample};
use super::support::*;
use crate::authoring;
use egui::Modifiers;

/// A pin on screen that already sits on a net, as a world point.
fn pin_on_a_net(app: &App) -> Result<Point, String> {
    for comp in &app.project.components {
        for pin in &comp.pins {
            if pin.net.is_none() {
                continue;
            }
            if let Some(at) = comp.pin_position(&pin.number)
                && app.canvas_rect.contains(modifiers_screen(app, at))
            {
                return Ok(at);
            }
        }
    }
    Err("the sample has no wired pin on screen".to_owned())
}

fn modifiers_screen(app: &App, at: Point) -> egui::Pos2 {
    super::modifiers::screen_of(app, at)
}

/// Undo the last engine edit.
fn undo(app: &mut App) -> Result<(), String> {
    app.session
        .lock()
        .map_err(|_| "session is busy".to_owned())?
        .undo()
        .map_err(|e| e.to_string())?;
    app.refresh();
    Ok(())
}

#[test]
fn a_newcomer_finds_labels_and_flags_under_add() -> Result<(), String> {
    let (mut app, ctx) = sample(Tab::Schematic);
    for gone in ["icon:tool-wire", "icon:tool-place-part", "icon:tool-label"] {
        assert!(
            !on_screen(&ctx, &mut app, gone),
            "{gone} is not a button on the Schematic strip"
        );
    }
    tap_strip(&ctx, &mut app, "icon:tool-add")?;
    for name in ["Label", "Power symbol", "Bus", "Port", "No connect"] {
        assert!(
            paints_text(&ctx, &mut app, name)?,
            "the Add menu lists {name}"
        );
    }
    Ok(())
}

#[test]
fn a_newcomer_adds_net_labels_until_esc() -> Result<(), String> {
    let (mut app, ctx) = sample(Tab::Schematic);
    tap_strip(&ctx, &mut app, "icon:tool-label")?;
    assert!(
        matches!(
            app.tool,
            Tool::SchematicLabel(wire_canvas::SchematicLabelKind::Local)
        ),
        "Add then Label arms the label tool: {}",
        app.message
    );
    let pin = pin_on_a_net(&app)?;
    let before = app.project.labels.len();
    let at = modifiers_screen(&app, pin);
    click(&ctx, &mut app, at, Modifiers::NONE);
    assert_eq!(
        app.project.labels.len(),
        before + 1,
        "the first click puts a label on the pin's net: {}",
        app.message
    );
    assert!(
        matches!(app.tool, Tool::SchematicLabel(_)),
        "the label tool stays armed for the next one"
    );
    click(&ctx, &mut app, at, Modifiers::NONE);
    assert_eq!(
        app.project.labels.len(),
        before + 2,
        "a second click places a second label"
    );
    key(&ctx, &mut app, egui::Key::Escape);
    assert!(
        matches!(app.tool, Tool::Select),
        "Esc stops placing and goes back to Select"
    );
    Ok(())
}

#[test]
fn a_newcomer_sees_the_placing_hint() -> Result<(), String> {
    let (mut app, ctx) = sample(Tab::Schematic);
    tap_strip(&ctx, &mut app, "icon:tool-power")?;
    assert!(
        paints_text(&ctx, &mut app, "Press Esc to stop")?,
        "the hint says what is armed and how to stop"
    );
    assert!(
        paints_text(&ctx, &mut app, "Power symbol:")?,
        "the hint names the armed kind"
    );
    Ok(())
}

#[test]
fn a_newcomer_flags_a_pin_no_connect_and_keeps_going() -> Result<(), String> {
    let (mut app, ctx) = sample(Tab::Schematic);
    tap_strip(&ctx, &mut app, "icon:tool-noconnect")?;
    assert!(matches!(app.tool, Tool::NoConnect), "No connect is armed");
    assert!(
        paints_text(&ctx, &mut app, "Press Esc to stop")?,
        "the hint tells how to stop"
    );
    Ok(())
}

#[test]
fn a_newcomer_moves_a_part_by_dragging_it() -> Result<(), String> {
    let (mut app, ctx) = sample(Tab::Schematic);
    assert!(
        !on_screen(&ctx, &mut app, "icon:tool-move"),
        "the Schematic strip has no Move button: dragging does the job"
    );
    let c = app.schematic.center;
    app.run(
        "component.add",
        json!({"kind": "resistor", "position": Point::new(c.x, c.y + 8.0)}),
    );
    frame(&ctx, &mut app, vec![], Modifiers::NONE);
    let id = app
        .project
        .components
        .last()
        .map(|p| p.id)
        .ok_or("no part")?;
    let start = app.project.component(id).ok_or("no part")?.position;
    let from = centre_of(&app, id)?;
    drag(
        &ctx,
        &mut app,
        from,
        from + egui::vec2(60.0, 0.0),
        Modifiers::NONE,
    );
    let end = app.project.component(id).ok_or("part vanished")?.position;
    assert!(
        (end.x - start.x).abs() > 1.0,
        "dragging the part body moved it: {start:?} to {end:?} ({})",
        app.message
    );
    assert!(matches!(app.tool, Tool::Select), "no tool was needed");
    Ok(())
}

#[test]
fn a_newcomer_marks_a_pin_no_connect() -> Result<(), String> {
    let (mut app, ctx) = sample(Tab::Schematic);
    // Add a resistor to get a fresh unconnected pin.
    let c = app.schematic.center;
    app.run(
        "component.add",
        json!({ "kind": "resistor", "position": Point::new(c.x, c.y + 8.0) }),
    );
    frame(&ctx, &mut app, vec![], Modifiers::NONE);
    let id = app
        .project
        .components
        .last()
        .map(|p| p.id)
        .ok_or("no part")?;
    let part = app.project.component(id).ok_or("no part")?;
    let pin_num = part.pins.first().map(|p| &p.number).ok_or("no pin")?;
    let at = part.pin_position(pin_num).ok_or("pin has no position")?;
    let screen = super::modifiers::screen_of(&app, at);
    right_click(&ctx, &mut app, screen);
    authoring::tests::tap(&ctx, &mut app, "Mark no connect")?;
    assert_eq!(
        app.project.no_connects.len(),
        1,
        "the pin is flagged: {}",
        app.message
    );
    assert!(
        matches!(app.tool, Tool::Select),
        "the tool is still Select, not No connect: {}",
        app.message
    );
    // Toggle it off.
    let screen = super::modifiers::screen_of(&app, at);
    right_click(&ctx, &mut app, screen);
    authoring::tests::tap(&ctx, &mut app, "Mark no connect")?;
    assert!(
        app.project.no_connects.is_empty(),
        "the flag is cleared: {}",
        app.message
    );
    Ok(())
}

#[test]
fn a_newcomer_is_told_why_a_wired_pin_cannot_be_no_connect() -> Result<(), String> {
    let (mut app, ctx) = sample(Tab::Schematic);
    // Find a pin that is already on a wire.
    let pin = pin_on_a_net(&app)?;
    let screen = super::modifiers::screen_of(&app, pin);
    right_click(&ctx, &mut app, screen);
    authoring::tests::tap(&ctx, &mut app, "Mark no connect")?;
    assert!(
        app.project.no_connects.is_empty(),
        "no flag was added for a wired pin: {}",
        app.message
    );
    assert!(
        paints_text(&ctx, &mut app, "This pin is wired to something")?,
        "the connected message is shown"
    );
    Ok(())
}

/// Where on the canvas a newcomer clicks to drop a note: an open patch near the top left.
fn open_patch(app: &App) -> egui::Pos2 {
    app.canvas_rect.left_top() + egui::vec2(160.0, 140.0)
}

#[test]
fn a_newcomer_adds_text_to_the_sheet() -> Result<(), String> {
    let (mut app, ctx) = sample(Tab::Schematic);
    let before = app.project.graphics.len();
    tap_strip(&ctx, &mut app, "icon:tool-text")?;
    assert!(
        matches!(app.tool, Tool::SheetText),
        "Add then Text arms the text tool: {}",
        app.message
    );
    let at = open_patch(&app);
    click(&ctx, &mut app, at, Modifiers::NONE);
    assert!(
        on_screen(&ctx, &mut app, "textinput:Sheet text"),
        "a click on the sheet opens the note field"
    );
    type_into(&ctx, &mut app, "textinput:Sheet text", "Power stage")?;
    let note = app
        .project
        .graphics
        .iter()
        .find(|g| g.shape == "text" && g.text == "Power stage")
        .ok_or_else(|| format!("no note on the sheet: {}", app.message))?;
    assert_eq!(note.layer, "schematic", "the note belongs to the schematic");
    assert_eq!(app.project.graphics.len(), before + 1, "one note was added");
    assert!(
        matches!(app.tool, Tool::Select),
        "the tool goes back to Select after the note is placed"
    );
    assert!(
        !on_screen(&ctx, &mut app, "textinput:Sheet text"),
        "the note field closes once the note is placed"
    );
    Ok(())
}

#[test]
fn a_newcomer_drops_a_note_with_esc() -> Result<(), String> {
    let (mut app, ctx) = sample(Tab::Schematic);
    let before = app.project.graphics.len();
    tap_strip(&ctx, &mut app, "icon:tool-text")?;
    let at = open_patch(&app);
    click(&ctx, &mut app, at, Modifiers::NONE);
    frame(
        &ctx,
        &mut app,
        vec![egui::Event::Text("Never placed".to_owned())],
        Modifiers::NONE,
    );
    key(&ctx, &mut app, egui::Key::Escape);
    assert_eq!(
        app.project.graphics.len(),
        before,
        "Esc places nothing: {}",
        app.message
    );
    assert!(matches!(app.tool, Tool::Select), "Esc goes back to Select");
    assert!(
        !on_screen(&ctx, &mut app, "textinput:Sheet text"),
        "Esc closes the note field"
    );
    Ok(())
}

#[test]
fn a_newcomer_places_nothing_for_an_empty_note() -> Result<(), String> {
    let (mut app, ctx) = sample(Tab::Schematic);
    let before = app.project.graphics.len();
    tap_strip(&ctx, &mut app, "icon:tool-text")?;
    let at = open_patch(&app);
    click(&ctx, &mut app, at, Modifiers::NONE);
    key(&ctx, &mut app, egui::Key::Enter);
    assert_eq!(
        app.project.graphics.len(),
        before,
        "an empty note places nothing: {}",
        app.message
    );
    Ok(())
}

#[test]
fn a_newcomer_draws_a_rectangle_on_the_sheet() -> Result<(), String> {
    let (mut app, ctx) = sample(Tab::Schematic);
    let before = app.project.graphics.len();
    tap_strip(&ctx, &mut app, "icon:tool-add")?;
    let rect_pos = painted_text_at(&ctx, &mut app, "Rectangle")?.ok_or("Rectangle not found")?;
    click(&ctx, &mut app, rect_pos, Modifiers::NONE);
    assert!(
        matches!(
            app.tool,
            Tool::SheetShape(sheet_shape_tool::SheetShapeKind::Rect)
        ),
        "Add then Rectangle arms the rectangle tool: {}",
        app.message
    );
    let at1 = open_patch(&app);
    click(&ctx, &mut app, at1, Modifiers::NONE);
    assert!(
        app.sheet_shape_start.is_some(),
        "the first click stored the start point"
    );
    let at2 = at1 + egui::vec2(80.0, 60.0);
    click(&ctx, &mut app, at2, Modifiers::NONE);
    assert_eq!(
        app.project.graphics.len(),
        before + 1,
        "the second click placed a rectangle: {}",
        app.message
    );
    let graphic = app
        .project
        .graphics
        .last()
        .ok_or_else(|| format!("no graphic on the sheet: {}", app.message))?;
    assert_eq!(graphic.shape, "rect", "the placed graphic is a rectangle");
    assert_eq!(
        graphic.layer, "schematic",
        "the rectangle belongs to the schematic"
    );
    assert_eq!(
        graphic.points.len(),
        2,
        "the rectangle has two corner points"
    );
    assert!(
        matches!(
            app.tool,
            Tool::SheetShape(sheet_shape_tool::SheetShapeKind::Rect)
        ),
        "the rectangle tool stays armed for the next one"
    );
    key(&ctx, &mut app, egui::Key::Escape);
    assert!(
        matches!(app.tool, Tool::Select),
        "Esc stops placing and goes back to Select"
    );
    undo(&mut app)?;
    assert_eq!(
        app.project.graphics.len(),
        before,
        "one undo removes the rectangle"
    );
    Ok(())
}
