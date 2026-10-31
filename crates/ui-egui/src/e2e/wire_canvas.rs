//! Schematic wire drafting stories for issue #291.

use super::super::*;
use super::modifiers::{click, screen_of, sample};
use egui::Modifiers;

fn add_resistors(app: &mut App, y: f64) -> Result<(), String> {
    let c = app.schematic.center;
    for dx in [-12.0, 12.0] {
        app.run(
            "component.add",
            json!({"kind": "resistor", "position": {"x": c.x + dx, "y": c.y + y}}),
        );
    }
    Ok(())
}

fn pin_pos(app: &App, index: usize, pin: &str) -> Result<Point, String> {
    let mut parts: Vec<_> = app.project.components.iter().rev().collect();
    parts.sort_by(|a, b| a.position.x.total_cmp(&b.position.x));
    let comp = parts.get(index).ok_or("missing part")?;
    comp.pin_position(pin).ok_or("missing pin position".into())
}

#[test]
fn a_newcomer_draws_a_wire_with_two_bends() -> Result<(), String> {
    let (mut app, ctx) = sample(Tab::Schematic);
    add_resistors(&mut app, 8.0)?;
    let a = pin_pos(&app, 0, "1")?;
    let b = pin_pos(&app, 1, "1")?;
    let mid_x = (a.x + b.x) / 2.0;
    let bend1 = Point::new(mid_x - 4.0, a.y + 6.0);
    let bend2 = Point::new(mid_x + 4.0, a.y + 6.0);
    click(&ctx, &mut app, screen_of(&app, a), Modifiers::NONE);
    click(&ctx, &mut app, screen_of(&app, bend1), Modifiers::NONE);
    click(&ctx, &mut app, screen_of(&app, bend2), Modifiers::NONE);
    click(&ctx, &mut app, screen_of(&app, b), Modifiers::NONE);
    let wire = app
        .project
        .drawn_wires
        .last()
        .ok_or("a drawn wire should exist")?;
    assert_eq!(
        wire.points.len(),
        4,
        "pin, two bends and pin make four vertices"
    );
    assert!(
        wire.net.is_some(),
        "the wire sits on one net: {}",
        app.message
    );
    Ok(())
}

#[test]
fn a_newcomer_tees_into_a_wire_and_gets_a_junction() -> Result<(), String> {
    let (mut app, ctx) = sample(Tab::Schematic);
    add_resistors(&mut app, 0.0)?;
    let left = pin_pos(&app, 0, "1")?;
    let right = pin_pos(&app, 1, "1")?;
    let mid = Point::new((left.x + right.x) / 2.0, left.y);
    let lift = Point::new(mid.x, left.y - 6.0);
    click(&ctx, &mut app, screen_of(&app, left), Modifiers::NONE);
    click(&ctx, &mut app, screen_of(&app, lift), Modifiers::NONE);
    click(&ctx, &mut app, screen_of(&app, right), Modifiers::NONE);
    assert!(
        !app.project.drawn_wires.is_empty(),
        "the trunk is a drawn wire"
    );
    app.run(
        "component.add",
        json!({"kind": "resistor", "position": {"x": mid.x, "y": mid.y + 10.0}}),
    );
    let branch_pin = app
        .project
        .components
        .last()
        .and_then(|c| c.pin_position("1"))
        .ok_or("branch part needs a pin")?;
    let tee = Point::new((left.x + lift.x) / 2.0, left.y);
    click(&ctx, &mut app, screen_of(&app, branch_pin), Modifiers::NONE);
    click(&ctx, &mut app, screen_of(&app, tee), Modifiers::NONE);
    assert!(
        app.project.labels.iter().any(|l| l.kind == "junction" && l.position.distance(tee) < 0.05),
        "a junction dot marks the tee: {}",
        app.message
    );
    assert!(
        app.project.drawn_wires.len() >= 2,
        "the tee leaves the trunk split: {}",
        app.message
    );
    Ok(())
}

#[test]
fn a_newcomer_names_a_bus_and_breaks_out_one_member() -> Result<(), String> {
    let (mut app, ctx) = sample(Tab::Schematic);
    super::support::tap_strip(&ctx, &mut app, "icon:tool-bus")?;
    let c = app.schematic.center;
    let a = Point::new(c.x - 8.0, c.y + 20.0);
    let b = Point::new(c.x + 8.0, c.y + 20.0);
    click(&ctx, &mut app, screen_of(&app, a), Modifiers::NONE);
    click(&ctx, &mut app, screen_of(&app, b), Modifiers::NONE);
    frame(&ctx, &mut app, vec![], Modifiers::NONE);
    if let Some(prompt) = app.bus_name_prompt.as_mut() {
        prompt.text = "ADDR*4".into();
    }
    authoring::tests::tap(&ctx, &mut app, "Draw bus")?;
    frame(&ctx, &mut app, vec![], Modifiers::NONE);
    app.refresh();
    assert!(
        app.project.labels.iter().any(|l| l.kind == "bus" && l.text == "ADDR*4"),
        "the bus is named on the sheet: {}",
        app.message
    );
    super::support::tap_strip(&ctx, &mut app, "icon:tool-label")?;
    let mid = Point::new(c.x, c.y + 20.0);
    click(&ctx, &mut app, screen_of(&app, mid), Modifiers::NONE);
    assert!(
        app.project.labels.iter().any(|l| {
            l.kind == "label" && app.project.net_name(l.net).starts_with("ADDR")
        }),
        "one member net is labelled at the bus: {}",
        app.message
    );
    Ok(())
}
