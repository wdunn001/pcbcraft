//! Stories for reconnecting wire ends (#457): dragging a wire's free end onto
//! a different pin or wire changes the connection and updates the net list.

use super::super::*;
use super::modifiers::{drag, sample, screen_of};
use egui::Modifiers;

#[test]
fn a_newcomer_reconnects_a_wire_end() -> Result<(), String> {
    let (mut app, ctx) = sample(Tab::Schematic);

    // Place two resistors: one will be the source, one the destination.
    let c = app.schematic.center;
    app.run(
        "component.add",
        json!({"kind": "resistor", "position": {"x": c.x - 12.0, "y": c.y}}),
    );
    let src = app
        .project
        .components
        .last()
        .ok_or("source resistor should be placed")?
        .clone();
    let src_pin = src.pin_position("1").ok_or("source pin 1 has no spot")?;

    app.run(
        "component.add",
        json!({"kind": "resistor", "position": {"x": c.x + 12.0, "y": c.y}}),
    );
    let dst = app
        .project
        .components
        .last()
        .ok_or("destination resistor should be placed")?
        .clone();
    let dst_pin = dst
        .pin_position("1")
        .ok_or("destination pin 1 has no spot")?;

    // Draw a wire from source pin 1 to a point near the destination, but not
    // actually connected.
    let near_dst = canvas::snap(Point::new(dst_pin.x - 4.0, dst_pin.y));
    app.run(
        "wire.place",
        json!({
            "points": [src_pin, near_dst],
            "pins": [{"component": src.id, "pin": "1"}],
            "net": null
        }),
    );
    let wire = app
        .project
        .drawn_wires
        .last()
        .ok_or_else(|| format!("the wire should be drawn: {}", app.message))?
        .id;

    // Verify the wire end is not connected to the destination pin.
    let wire_before = app
        .project
        .drawn_wires
        .iter()
        .find(|w| w.id == wire)
        .ok_or("wire missing")?;
    assert!(
        !wire_before
            .points
            .iter()
            .any(|p| p.distance(dst_pin) < 0.05),
        "wire end should not be on destination pin yet"
    );

    // Drag the free end of the wire onto the destination pin.
    let from = screen_of(&app, near_dst);
    let to = screen_of(&app, dst_pin);
    drag(&ctx, &mut app, from, to, Modifiers::NONE);

    // Verify the wire end is now on the destination pin.
    let wire_after = app
        .project
        .drawn_wires
        .iter()
        .find(|w| w.id == wire)
        .ok_or_else(|| format!("wire missing after drag: {}", app.message))?;
    let last = wire_after.points.last().ok_or("wire has no end point")?;
    assert!(
        last.distance(dst_pin) < 0.05,
        "wire end should be on destination pin: {}",
        app.message
    );

    // Verify both pins are now on the same net.
    let src_pin_net = app
        .project
        .components
        .iter()
        .find(|c| c.id == src.id)
        .and_then(|c| c.pins.iter().find(|p| p.number == "1"))
        .and_then(|p| p.net);
    let dst_pin_net = app
        .project
        .components
        .iter()
        .find(|c| c.id == dst.id)
        .and_then(|c| c.pins.iter().find(|p| p.number == "1"))
        .and_then(|p| p.net);
    assert!(
        src_pin_net.is_some() && src_pin_net == dst_pin_net,
        "both pins should share the same net: {}",
        app.message
    );

    Ok(())
}
