//! Stories for the Schematic tool bar after the slim-down (#1171): one Add menu
//! for labels, power, buses, ports and flags, no Wire or Place part button, and
//! placing that keeps going until Esc.

use super::super::*;
use super::modifiers::{centre_of, click, drag, sample};
use super::support::*;
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
    for name in ["Label", "Power", "Bus", "Port", "No connect"] {
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
        paints_text(&ctx, &mut app, "Power:")?,
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
