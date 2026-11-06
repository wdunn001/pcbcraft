//! Stories for moving net labels and power symbols and renaming the net they
//! name (#1322): press one with Select and drag it, then change its net or
//! symbol in Properties. Each change is one Undo.

use super::super::*;
use super::modifiers::{click, drag, sample, screen_of};
use super::support::*;
use egui::Modifiers;
use pcbcraft_doc::PinRef;

/// Place a label with the engine on bare sheet, away from every pin, and wait
/// for the sheet to show it. Placing is setup; the story starts at the press.
fn bare_label(app: &mut App, ctx: &egui::Context, kind: &str, net: &str) -> Result<Id, String> {
    let c = app.schematic.center;
    let at = canvas::snap(Point::new(c.x + 6.0, c.y + 30.0));
    let mut params = json!({"kind": kind, "name": net, "position": {"x": at.x, "y": at.y}});
    if kind == "power" {
        params["style"] = json!("bar");
    }
    app.run("label.place", params);
    for _ in 0..3 {
        frame(ctx, app, vec![], Modifiers::NONE);
    }
    let label = app
        .project
        .labels
        .last()
        .ok_or_else(|| format!("the label should be placed: {}", app.message))?;
    if !app.canvas_rect.contains(screen_of(app, label.position)) {
        return Err("the label is off screen".to_owned());
    }
    Ok(label.id)
}

fn position_of(app: &App, id: Id) -> Result<Point, String> {
    app.project
        .labels
        .iter()
        .find(|l| l.id == id)
        .map(|l| l.position)
        .ok_or_else(|| "the label is gone".to_owned())
}

fn net_of(app: &App, id: Id) -> Result<Id, String> {
    app.project
        .labels
        .iter()
        .find(|l| l.id == id)
        .map(|l| l.net)
        .ok_or_else(|| "the label is gone".to_owned())
}

/// Drag a label by hand and check it moved, snapped, and one Undo puts it back.
fn drag_and_undo(app: &mut App, ctx: &egui::Context, id: Id, what: &str) -> Result<(), String> {
    let before = position_of(app, id)?;
    let from = screen_of(app, before);
    drag(
        ctx,
        app,
        from,
        from + egui::vec2(64.0, 40.0),
        Modifiers::NONE,
    );
    let after = position_of(app, id)?;
    assert!(
        after.x > before.x && after.y > before.y,
        "dragging the {what} right and down moves it: {before:?} to {after:?}, {}",
        app.message
    );
    assert_eq!(
        after,
        canvas::snap(after),
        "the {what} lands on the grid, not between grid points"
    );
    assert!(
        matches!(app.tool, Tool::Select),
        "moving the {what} does not leave Select"
    );
    app.tool_call("undo", json!({}));
    assert_eq!(
        position_of(app, id)?,
        before,
        "one Undo puts the {what} back where it stood"
    );
    Ok(())
}

#[test]
fn a_newcomer_moves_a_power_symbol() -> Result<(), String> {
    let (mut app, ctx) = sample(Tab::Schematic);
    let id = bare_label(&mut app, &ctx, "power", "3V3")?;
    drag_and_undo(&mut app, &ctx, id, "power symbol")
}

#[test]
fn a_newcomer_moves_a_net_label() -> Result<(), String> {
    let (mut app, ctx) = sample(Tab::Schematic);
    let id = bare_label(&mut app, &ctx, "label", "SIG")?;
    drag_and_undo(&mut app, &ctx, id, "net label")?;
    // A plain click picks the label up without moving it.
    let at = screen_of(&app, position_of(&app, id)?);
    let before = position_of(&app, id)?;
    click(&ctx, &mut app, at, Modifiers::NONE);
    assert_eq!(app.selected, Some(id), "a click selects the label");
    assert_eq!(position_of(&app, id)?, before, "a click does not move it");
    Ok(())
}

#[test]
fn a_newcomer_renames_a_label_net_in_properties() -> Result<(), String> {
    let (mut app, ctx) = sample(Tab::Schematic);
    let id = bare_label(&mut app, &ctx, "label", "SIG")?;
    let old = net_of(&app, id)?;
    let at = screen_of(&app, position_of(&app, id)?);
    click(&ctx, &mut app, at, Modifiers::NONE);
    assert_eq!(app.selected, Some(id), "clicking the label selects it");
    assert!(
        on_screen(&ctx, &mut app, "textinput:Label net"),
        "Properties shows the net name of the selected label"
    );
    // A new name makes a new net in one step.
    let nets = app.project.nets.len();
    type_into(&ctx, &mut app, "textinput:Label net", "CLK2")?;
    let now = net_of(&app, id)?;
    assert_ne!(now, old, "the label now names another net: {}", app.message);
    assert_eq!(
        app.project.net_name(now),
        "CLK2",
        "the net has the typed name"
    );
    assert_eq!(app.project.nets.len(), nets + 1, "a new name makes a net");
    app.tool_call("undo", json!({}));
    assert_eq!(net_of(&app, id)?, old, "one Undo gives the old net back");
    assert_eq!(app.project.nets.len(), nets, "and removes the new one");
    // A name already in use joins that net.
    let gnd = app
        .project
        .nets
        .iter()
        .find(|n| n.name == "GND")
        .map(|n| n.id)
        .ok_or("the sample has a GND net")?;
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![], Modifiers::NONE);
    }
    type_into(&ctx, &mut app, "textinput:Label net", "GND")?;
    assert_eq!(net_of(&app, id)?, gnd, "a name in use joins that net");
    assert_eq!(app.project.nets.len(), nets, "no extra net appears");
    app.tool_call("undo", json!({}));
    assert_eq!(net_of(&app, id)?, old, "one Undo leaves the net again");
    Ok(())
}

#[test]
fn a_newcomer_changes_the_style_of_a_power_symbol_in_properties() -> Result<(), String> {
    let (mut app, ctx) = sample(Tab::Schematic);
    let id = bare_label(&mut app, &ctx, "power", "3V3")?;
    let at = screen_of(&app, position_of(&app, id)?);
    click(&ctx, &mut app, at, Modifiers::NONE);
    assert_eq!(app.selected, Some(id), "clicking the symbol selects it");
    tap_properties_segment(&ctx, &mut app, "Symbol", 2, 4)?;
    let style = app
        .project
        .labels
        .iter()
        .find(|l| l.id == id)
        .map(|l| l.style.clone())
        .ok_or("the symbol is gone")?;
    assert_eq!(style, "arrow", "the third choice draws an arrow");
    app.tool_call("undo", json!({}));
    let back = app
        .project
        .labels
        .iter()
        .find(|l| l.id == id)
        .map(|l| l.style.clone())
        .ok_or("the symbol is gone")?;
    assert_eq!(back, "bar", "one Undo gives the old symbol back");
    Ok(())
}

#[test]
fn a_newcomer_reconnects_a_wire_end() -> Result<(), String> {
    let (mut app, ctx) = sample(Tab::Schematic);
    // Two fresh parts with free pins, side by side, plus a third below.
    let c = app.schematic.center;
    for (dx, dy) in [(-8.0, 0.0), (8.0, 0.0), (0.0, 8.0)] {
        app.run(
            "component.add",
            json!({"kind": "resistor", "position": Point::new(c.x + dx, c.y + dy)}),
        );
    }
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![], Modifiers::NONE);
    }
    let fresh: Vec<_> = app.project.components.iter().rev().take(3).collect();
    let pin_at = |i: usize| -> Result<(Id, String, Point), String> {
        let comp = fresh.get(i).ok_or("a fresh part is missing")?;
        let pin = comp.pins.first().ok_or("part has no pins")?;
        let at = comp
            .pin_position(&pin.number)
            .ok_or("pin has no position")?;
        Ok((comp.id, pin.number.clone(), at))
    };
    let (a, _, a_at) = pin_at(0)?;
    let (b, _, b_at) = pin_at(1)?;
    let (_, _, c_at) = pin_at(2)?;
    // Draw a wire from pin A to pin B by dragging.
    let start = screen_of(&app, a_at);
    let end = screen_of(&app, b_at);
    assert!(
        app.canvas_rect.contains(start) && app.canvas_rect.contains(end),
        "both pins are on screen"
    );
    drag(&ctx, &mut app, start, end, Modifiers::NONE);
    let before = app.project.drawn_wires.len();
    assert!(before > 0, "dragging pin to pin created a drawn wire");
    let wire = app
        .project
        .drawn_wires
        .last()
        .ok_or("the drawn wire is missing")?;
    let wire_id = wire.id;
    let original_start = wire.points.first().cloned().ok_or("wire has no start")?;
    let original_end = wire.points.last().cloned().ok_or("wire has no end")?;
    // Pick the end of the wire that is closer to pin B and drag it onto pin C.
    let (drag_from, drag_to, target) =
        if original_end.distance(b_at) < original_start.distance(b_at) {
            (original_end, c_at, "the end")
        } else {
            (original_start, c_at, "the start")
        };
    let from = screen_of(&app, drag_from);
    let to = screen_of(&app, c_at);
    assert!(
        app.canvas_rect.contains(from) && app.canvas_rect.contains(to),
        "the wire end and target pin are on screen"
    );
    drag(&ctx, &mut app, from, to, Modifiers::NONE);
    let wire_after = app
        .project
        .drawn_wires
        .iter()
        .find(|w| w.id == wire_id)
        .ok_or("the wire is gone after reconnect")?;
    let new_start = wire_after
        .points
        .first()
        .cloned()
        .ok_or("reconnected wire has no start")?;
    let new_end = wire_after
        .points
        .last()
        .cloned()
        .ok_or("reconnected wire has no end")?;
    // One end should now be at pin C's position (within snapping tolerance).
    let snapped_to_c = new_start.distance(c_at) < 0.5 || new_end.distance(c_at) < 0.5;
    assert!(
        snapped_to_c,
        "reconnecting the {target} moved the wire end onto pin C: {new_start:?} -> {new_end:?} (pin C at {c_at:?})"
    );
    // The net that pin C belongs to should now have the wire connected.
    let pin_c_net = app
        .project
        .pin(&PinRef {
            component: fresh[2].id,
            pin: fresh[2].pins.first().as_ref().unwrap().number.clone(),
        })
        .and_then(|p| p.net);
    if let Some(net) = pin_c_net {
        assert_eq!(wire_after.net, net, "the wire joined the net of pin C");
    }
    // Undo restores the original connection to pin B.
    app.tool_call("undo", json!({}));
    let wire_restored = app
        .project
        .drawn_wires
        .iter()
        .find(|w| w.id == wire_id)
        .ok_or("the wire is gone after undo")?;
    let restored_start = wire_restored
        .points
        .first()
        .cloned()
        .ok_or("restored wire has no start")?;
    let restored_end = wire_restored
        .points
        .last()
        .cloned()
        .ok_or("restored wire has no end")?;
    let back_at_b = restored_start.distance(b_at) < 0.5 || restored_end.distance(b_at) < 0.5;
    assert!(
        back_at_b,
        "one Undo gives the wire back its original connection to pin B"
    );
    Ok(())
}
