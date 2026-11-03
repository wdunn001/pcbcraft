//! Stories for typed net labels and power symbols (#1171): a label takes a
//! typed name that makes or joins a net, labels of one name share a net, and a
//! Power symbol sets a supply net the checks treat as driven.

use super::super::*;
use super::modifiers::{click, sample, screen_of};
use super::support::*;
use egui::Modifiers;

/// A pin on screen that already sits on a net, as a world point.
fn pin_on_a_net(app: &App) -> Result<(Point, Id), String> {
    for comp in &app.project.components {
        for pin in &comp.pins {
            let Some(net) = pin.net else { continue };
            if let Some(at) = comp.pin_position(&pin.number)
                && app.canvas_rect.contains(screen_of(app, at))
            {
                return Ok((at, net));
            }
        }
    }
    Err("the sample has no wired pin on screen".to_owned())
}

/// Type into a field, then let the sheet settle back to its normal size so
/// screen positions read afterwards are the ones a person would click.
fn type_and_settle(ctx: &egui::Context, app: &mut App, id: &str, text: &str) -> Result<(), String> {
    type_into(ctx, app, id, text)?;
    for _ in 0..3 {
        frame(ctx, app, vec![], Modifiers::NONE);
    }
    Ok(())
}

fn nets_named(app: &App, name: &str) -> Vec<Id> {
    app.project
        .nets
        .iter()
        .filter(|n| n.name == name)
        .map(|n| n.id)
        .collect()
}

/// Two bare wires a little below the sample, each one a straight run.
fn two_bare_wires(app: &mut App) -> (Point, Point) {
    let c = app.schematic.center;
    let first = Point::new(c.x, c.y + 24.0);
    let second = Point::new(c.x, c.y + 34.0);
    for at in [first, second] {
        app.run(
            "wire.draw",
            json!({"points": [{"x": at.x - 8.0, "y": at.y}, {"x": at.x + 8.0, "y": at.y}]}),
        );
    }
    (first, second)
}

#[test]
fn a_newcomer_adds_a_net_label() -> Result<(), String> {
    let (mut app, ctx) = sample(Tab::Schematic);
    tap_strip(&ctx, &mut app, "icon:tool-label")?;
    assert!(
        on_screen(&ctx, &mut app, "textinput:Net name"),
        "arming Label shows a field for the net name"
    );
    type_and_settle(&ctx, &mut app, "textinput:Net name", "CLK")?;
    assert!(
        matches!(app.tool, Tool::SchematicLabel(_)),
        "typing a name keeps Label armed: {:?}",
        app.tool
    );
    assert_eq!(app.edit.label.name, "CLK", "the field holds the typed name");
    let (pin, net) = pin_on_a_net(&app)?;
    let at = screen_of(&app, pin);
    click(&ctx, &mut app, at, Modifiers::NONE);
    let label = app
        .project
        .labels
        .last()
        .ok_or_else(|| format!("clicking a pin should place a label: {}", app.message))?;
    assert_eq!(label.kind, "label", "the label is a plain net label");
    assert_eq!(label.net, net, "the label names the net the pin sits on");
    assert_eq!(
        app.project.net_name(net),
        "CLK",
        "the net carries the typed name"
    );
    assert!(
        paints_text(&ctx, &mut app, "CLK")?,
        "the name is written on the sheet"
    );
    // A new name on empty sheet makes a new net.
    type_and_settle(&ctx, &mut app, "textinput:Net name", "FRESH")?;
    assert!(
        nets_named(&app, "FRESH").is_empty(),
        "FRESH is not a net yet"
    );
    let blank = app.canvas_rect.left_bottom() + egui::vec2(80.0, -60.0);
    click(&ctx, &mut app, blank, Modifiers::NONE);
    assert_eq!(
        nets_named(&app, "FRESH").len(),
        1,
        "a label with a new name makes the net: {}",
        app.message
    );
    assert!(
        matches!(app.tool, Tool::SchematicLabel(_)),
        "placing goes on until Esc"
    );
    Ok(())
}

#[test]
fn a_newcomer_joins_two_wires_with_the_same_label() -> Result<(), String> {
    let (mut app, ctx) = sample(Tab::Schematic);
    let (first, second) = two_bare_wires(&mut app);
    frame(&ctx, &mut app, vec![], Modifiers::NONE);
    tap_strip(&ctx, &mut app, "icon:tool-label")?;
    type_and_settle(&ctx, &mut app, "textinput:Net name", "SIG")?;
    for at in [first, second] {
        let spot = screen_of(&app, at);
        click(&ctx, &mut app, spot, Modifiers::NONE);
    }
    let sig = nets_named(&app, "SIG");
    assert_eq!(sig.len(), 1, "one net carries the name: {}", app.message);
    let on_sig = app
        .project
        .drawn_wires
        .iter()
        .filter(|w| w.net == sig.first().copied())
        .count();
    assert_eq!(on_sig, 2, "both wires sit on the one net");
    let labels = app
        .project
        .labels
        .iter()
        .filter(|l| l.kind == "label")
        .count();
    assert_eq!(labels, 2, "each click placed a label");
    let findings = pcbcraft_rules::erc(&app.project);
    assert!(
        !findings.iter().any(|f| f.code == "dangling_label"),
        "the findings list stays clean of label complaints: {findings:?}"
    );
    Ok(())
}

#[test]
fn a_newcomer_adds_a_power_rail() -> Result<(), String> {
    let (mut app, ctx) = sample(Tab::Schematic);
    // A part with a supply input and nothing driving it yet.
    let c = app.schematic.center;
    app.run(
        "component.add",
        json!({"kind": "resistor", "reference": "U9", "position": {"x": c.x, "y": c.y + 28.0}}),
    );
    let part = app
        .project
        .components
        .last()
        .map(|p| p.id)
        .ok_or("no part")?;
    if let Ok(mut s) = app.session.lock()
        && let Some(pin) = s
            .project
            .components
            .iter_mut()
            .find(|p| p.id == part)
            .and_then(|p| p.pins.first_mut())
    {
        pin.electrical_type = "power_in".into();
    }
    app.refresh();
    frame(&ctx, &mut app, vec![], Modifiers::NONE);
    tap_strip(&ctx, &mut app, "icon:tool-power")?;
    assert!(
        on_screen(&ctx, &mut app, "textinput:Power net"),
        "arming Power shows the net field"
    );
    type_and_settle(&ctx, &mut app, "textinput:Power net", "3V3")?;
    tap_id(&ctx, &mut app, "combobox:Power style")?;
    tap_id(&ctx, &mut app, "button:Arrow")?;
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![], Modifiers::NONE);
    }
    let pin = app
        .project
        .component(part)
        .and_then(|p| p.pin_position("1"))
        .ok_or("no pin position")?;
    let at = screen_of(&app, pin);
    click(&ctx, &mut app, at, Modifiers::NONE);
    let label = app
        .project
        .labels
        .iter()
        .find(|l| l.kind == "power")
        .ok_or_else(|| format!("clicking should place a power symbol: {}", app.message))?;
    assert_eq!(label.style, "arrow", "the picked symbol style is kept");
    assert_eq!(app.project.net_name(label.net), "3V3", "the net is 3V3");
    let findings = pcbcraft_rules::erc(&app.project);
    assert!(
        !findings
            .iter()
            .any(|f| f.code == "power_not_driven" && f.objects.contains(&label.net)),
        "the power checks treat 3V3 as driven: {findings:?}"
    );
    assert!(
        matches!(app.tool, Tool::SchematicLabel(_)),
        "placing goes on until Esc"
    );
    Ok(())
}

#[test]
fn a_newcomer_is_told_when_nets_of_one_name_were_merged_on_open() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    let mut twins = app.project.clone();
    for (i, id) in [901, 902].into_iter().enumerate() {
        twins.nets.push(pcbcraft_doc::Net {
            id,
            name: "VBUS".into(),
            ..Default::default()
        });
        let y = 10.0 * i as f64;
        twins.drawn_wires.push(pcbcraft_doc::DrawnWire {
            id: id + 100,
            net: Some(id),
            points: vec![Point::new(0.0, y), Point::new(10.0, y)],
        });
    }
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let path = dir.path().join("twins.pcbcraft");
    std::fs::write(
        &path,
        serde_json::to_string(&twins).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let opened = app.tool_call("open", json!({"path": path.to_string_lossy()}));
    assert!(opened, "the design opens: {}", app.message);
    assert_eq!(nets_named(&app, "VBUS").len(), 1, "the twins were merged");
    assert!(
        app.message.contains("VBUS") && app.message.contains("Undo"),
        "the notice lists what was merged and says undo brings it back: {}",
        app.message
    );
    Ok(())
}

#[test]
fn a_newcomer_sets_a_port_direction() -> Result<(), String> {
    let (mut app, ctx) = sample(Tab::Schematic);
    // Place a port at a free spot on the sheet.
    let c = app.schematic.center;
    let spot = Point::new(c.x, c.y + 50.0);
    app.run(
        "schematic.place_port",
        json!({
            "name": "SIG",
            "net": app.project.nets.first().map(|n| n.id).ok_or("no net")?,
            "sheet": 0,
            "position": spot
        }),
    );
    frame(&ctx, &mut app, vec![], Modifiers::NONE);
    let port = app.project.ports.first().ok_or("port was not placed")?.id;
    // Click the port to select it.
    let at = screen_of(&app, spot);
    click(&ctx, &mut app, at, Modifiers::NONE);
    frame(&ctx, &mut app, vec![], Modifiers::NONE);
    assert!(
        matches!(app.tool, Tool::Select),
        "tool is Select after placing port"
    );
    assert_eq!(app.selected, Some(port), "port was selected");
    let controls = crate::e2e::audit::controls(&ctx, &app);
    let has_segment = controls
        .iter()
        .any(|c| c.id.as_deref() == Some("radiobutton:schematic.port.direction, 3 of 4"));
    let port_in_list = app.project.ports.iter().any(|p| p.id == port);
    assert!(port_in_list, "port is in project.ports");
    let is_component = app.project.component(port).is_some();
    assert!(!is_component, "port is not a component");
    assert!(has_segment, "port direction segment is on screen");
    // Tap the Output segment in the Properties panel.
    tap_properties_segment(&ctx, &mut app, "schematic.port.direction", 1, 4)?;
    frame(&ctx, &mut app, vec![], Modifiers::NONE);
    frame(&ctx, &mut app, vec![], Modifiers::NONE);
    let found = app
        .project
        .ports
        .iter()
        .find(|p| p.id == port)
        .ok_or("port vanished")?;
    assert_eq!(
        found.direction, "output",
        "the port direction is now output: {}",
        found.direction
    );
    // Undo returns to bidirectional.
    app.run("undo", json!({}));
    frame(&ctx, &mut app, vec![], Modifiers::NONE);
    let found = app
        .project
        .ports
        .iter()
        .find(|p| p.id == port)
        .ok_or("port vanished after undo")?;
    assert_eq!(
        found.direction, "bidirectional",
        "undo restored the default direction: {}",
        found.direction
    );
    Ok(())
}
