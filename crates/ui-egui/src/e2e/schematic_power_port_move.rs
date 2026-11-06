//! Stories for moving power and ground ports (#1754): the wire that
//! touches a port follows it, the net keeps its connection, a wire can be
//! drawn to the port again, and a port sitting on a pin drags like any other.

use super::super::*;
use super::modifiers::{click, drag, sample, screen_of};
use super::support::*;
use egui::Modifiers;

struct Rig {
    port: Id,
    wire: Id,
    pin: Point,
    other_pin: Point,
}

/// A resistor with pin 1 wired straight up to a power port named `net`.
fn rig(app: &mut App, ctx: &egui::Context, net: &str, style: &str) -> Result<Rig, String> {
    let c = app.schematic.center;
    app.run(
        "component.add",
        json!({"kind": "resistor", "position": {"x": c.x, "y": c.y + 14.0}}),
    );
    let part = app
        .project
        .components
        .last()
        .ok_or("the resistor should be placed")?
        .clone();
    let pin = part.pin_position("1").ok_or("pin 1 has no spot")?;
    let other_pin = part.pin_position("2").ok_or("pin 2 has no spot")?;
    let at = canvas::snap(Point::new(pin.x, pin.y - 10.0));
    app.run(
        "label.place",
        json!({"kind": "power", "name": net, "style": style, "position": at}),
    );
    let port = app
        .project
        .labels
        .last()
        .ok_or("the port should exist")?
        .clone();
    app.run(
        "wire.place",
        json!({
            "points": [pin, port.position],
            "pins": [{"component": part.id, "pin": "1"}],
            "net": port.net
        }),
    );
    for _ in 0..3 {
        frame(ctx, app, vec![], Modifiers::NONE);
    }
    let wire = app
        .project
        .drawn_wires
        .last()
        .ok_or_else(|| format!("the wire should be drawn: {}", app.message))?
        .id;
    Ok(Rig {
        port: port.id,
        wire,
        pin,
        other_pin,
    })
}

fn port(app: &App, id: Id) -> Result<pcbcraft_doc::Label, String> {
    app.project
        .labels
        .iter()
        .find(|l| l.id == id)
        .cloned()
        .ok_or_else(|| "the port is gone".to_owned())
}

fn wire_points(app: &App, id: Id) -> Result<Vec<Point>, String> {
    app.project
        .drawn_wires
        .iter()
        .find(|w| w.id == id)
        .map(|w| w.points.clone())
        .ok_or_else(|| "the wire is gone".to_owned())
}

fn square(points: &[Point]) -> bool {
    points
        .windows(2)
        .all(|s| (s[0].x - s[1].x).abs() < 1e-6 || (s[0].y - s[1].y).abs() < 1e-6)
}

fn pin_net(app: &App, pin: &str) -> Option<Id> {
    app.project
        .components
        .last()
        .and_then(|c| c.pins.iter().find(|p| p.number == pin))
        .and_then(|p| p.net)
}

#[test]
fn a_newcomer_moves_a_power_port_and_its_wire_follows() -> Result<(), String> {
    let (mut app, ctx) = sample(Tab::Schematic);
    let r = rig(&mut app, &ctx, "3V3", "bar")?;
    let before = port(&app, r.port)?;
    let was = wire_points(&app, r.wire)?;
    let from = screen_of(&app, before.position);
    drag(
        &ctx,
        &mut app,
        from,
        from + egui::vec2(64.0, -24.0),
        Modifiers::NONE,
    );
    let after = port(&app, r.port)?;
    assert_ne!(
        after.position, before.position,
        "the port moved: {}",
        app.message
    );
    let now = wire_points(&app, r.wire)?;
    assert_eq!(
        now.first().copied(),
        Some(r.pin),
        "the wire stays on its pin"
    );
    assert_eq!(
        now.last().copied(),
        Some(after.position),
        "the wire end follows the port"
    );
    assert!(
        square(&now),
        "the wire stays square, no slanted line: {now:?}"
    );
    let wire_net = app
        .project
        .drawn_wires
        .iter()
        .find(|w| w.id == r.wire)
        .and_then(|w| w.net);
    assert_eq!(wire_net, Some(after.net), "the wire keeps the port net");
    assert_eq!(pin_net(&app, "1"), Some(after.net), "the pin keeps the net");
    app.tool_call("undo", json!({}));
    assert_eq!(
        port(&app, r.port)?.position,
        before.position,
        "one Undo puts the port back"
    );
    assert_eq!(
        wire_points(&app, r.wire)?,
        was,
        "the same Undo puts the wire back"
    );
    Ok(())
}

#[test]
fn a_newcomer_draws_a_wire_to_a_moved_port_and_the_net_rejoins() -> Result<(), String> {
    let (mut app, ctx) = sample(Tab::Schematic);
    let r = rig(&mut app, &ctx, "VBUS", "bar")?;
    let before = port(&app, r.port)?;
    let from = screen_of(&app, before.position);
    drag(
        &ctx,
        &mut app,
        from,
        from + egui::vec2(80.0, 0.0),
        Modifiers::NONE,
    );
    let moved = port(&app, r.port)?;
    assert_ne!(
        moved.position, before.position,
        "the port moved: {}",
        app.message
    );
    assert_eq!(pin_net(&app, "2"), None, "pin 2 starts with no net");
    let wires = app.project.drawn_wires.len();
    app.tool = Tool::Wire(None);
    let pin_two = screen_of(&app, r.other_pin);
    click(&ctx, &mut app, pin_two, Modifiers::NONE);
    let to_port = screen_of(&app, moved.position);
    click(&ctx, &mut app, to_port, Modifiers::NONE);
    assert_eq!(
        app.project.drawn_wires.len(),
        wires + 1,
        "a wire is drawn to the port: {}",
        app.message
    );
    assert_eq!(
        pin_net(&app, "2"),
        Some(moved.net),
        "pin 2 joined the port net"
    );
    let last = app.project.drawn_wires.last().ok_or("no wire")?;
    assert_eq!(
        last.points.last().copied(),
        Some(moved.position),
        "the wire ends on the port"
    );
    Ok(())
}

#[test]
fn a_newcomer_moves_a_ground_port_that_sits_on_a_pin() -> Result<(), String> {
    let (mut app, ctx) = sample(Tab::Schematic);
    let c = app.schematic.center;
    app.run(
        "component.add",
        json!({"kind": "resistor", "position": {"x": c.x, "y": c.y + 14.0}}),
    );
    let part = app.project.components.last().ok_or("no part")?.clone();
    let pin = part.pin_position("2").ok_or("no pin spot")?;
    app.run(
        "label.place",
        json!({"kind": "power", "name": "GND", "style": "ground", "position": pin}),
    );
    let id = app.project.labels.last().ok_or("no port")?.id;
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![], Modifiers::NONE);
    }
    let before = port(&app, id)?;
    let pin_was = pin_net(&app, "2");
    let from = screen_of(&app, before.position);
    drag(
        &ctx,
        &mut app,
        from,
        from + egui::vec2(48.0, 48.0),
        Modifiers::NONE,
    );
    let after = port(&app, id)?;
    assert!(
        matches!(app.tool, Tool::Select),
        "a drag on the port does not start a wire"
    );
    assert_ne!(
        after.position, before.position,
        "the ground port moved: {}",
        app.message
    );
    assert_eq!(after.net, before.net, "the port keeps its net");
    assert_eq!(pin_net(&app, "2"), pin_was, "the pin keeps its net");
    app.tool_call("undo", json!({}));
    assert_eq!(
        port(&app, id)?.position,
        before.position,
        "one Undo puts it back"
    );
    Ok(())
}

#[test]
fn a_newcomer_picks_up_a_power_port_by_its_symbol_body() -> Result<(), String> {
    let (mut app, ctx) = sample(Tab::Schematic);
    let r = rig(&mut app, &ctx, "3V3", "bar")?;
    let before = port(&app, r.port)?;
    // The stem of a supply symbol stands 5 mm above its anchor.
    let body = screen_of(&app, Point::new(before.position.x, before.position.y - 4.0));
    drag(
        &ctx,
        &mut app,
        body,
        body + egui::vec2(64.0, 0.0),
        Modifiers::NONE,
    );
    assert_ne!(
        port(&app, r.port)?.position,
        before.position,
        "dragging the symbol body moves the port: {}",
        app.message
    );
    Ok(())
}

#[test]
fn a_newcomer_drops_a_power_port_on_a_pin_and_it_connects() -> Result<(), String> {
    let (mut app, ctx) = sample(Tab::Schematic);
    let r = rig(&mut app, &ctx, "3V3", "bar")?;
    let before = port(&app, r.port)?;
    assert_eq!(pin_net(&app, "2"), None, "pin 2 starts with no net");
    // Selecting the port opens Properties, which moves the canvas; aim after.
    app.select(Some(r.port));
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![], Modifiers::NONE);
    }
    let from = screen_of(&app, before.position);
    let onto = screen_of(&app, r.other_pin);
    drag(&ctx, &mut app, from, onto, Modifiers::NONE);
    let after = port(&app, r.port)?;
    assert_eq!(
        after.position, r.other_pin,
        "the port lands on the pin: {}",
        app.message
    );
    assert_eq!(
        pin_net(&app, "2"),
        Some(after.net),
        "the pin joined the port net"
    );
    assert_eq!(
        pin_net(&app, "1"),
        Some(after.net),
        "the first pin keeps the net"
    );
    Ok(())
}
