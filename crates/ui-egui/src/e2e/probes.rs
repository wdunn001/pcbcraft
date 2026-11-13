//! Stories for the virtual DC probes: a probe on the schematic reads its voltage from the
//! simulator, shows it in a badge and in the Analyze table, and goes away again.
//!
//! The layout stories at the end do the same on the board: a pad or a track gets a probe and a badge.
//!
//! Context: issues #540 and #543. The divider below is a 10 V source across two 1 k resistors, so
//! the middle net reads exactly 5 V; no story here expects a number the solver did not produce.
use super::super::*;
use super::modifiers::{click, screen_of};
use super::support::*;
use egui::Modifiers;

/// What the divider story needs to find again.
struct Divider {
    /// The window and its context.
    app: App,
    ctx: egui::Context,
    /// A point on the wire of the middle net, in world millimetres.
    middle: Point,
    /// The id of the middle net.
    mid_net: Id,
}

fn pin_at(app: &App, component: Id, pin: &str) -> Result<Point, String> {
    app.project
        .pin_position(&PinRef {
            component,
            pin: pin.into(),
            unit: None,
        })
        .ok_or_else(|| format!("no spot for pin {pin}"))
}

fn part_id(app: &App, reference: &str) -> Result<Id, String> {
    app.project
        .components
        .iter()
        .find(|c| c.reference == reference)
        .map(|c| c.id)
        .ok_or_else(|| format!("no part {reference}"))
}

fn net_id(app: &App, name: &str) -> Result<Id, String> {
    app.project
        .nets
        .iter()
        .find(|n| n.name == name)
        .map(|n| n.id)
        .ok_or_else(|| format!("no net {name}"))
}

/// A blank design on the Schematic tab holding a drawn divider: V1 across R1 and R2.
fn divider() -> Result<Divider, String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tab = Tab::Schematic;
    let ctx = egui::Context::default();
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![], Modifiers::NONE);
    }
    let c = app.schematic.center;
    for name in ["GND", "VCC", "MID"] {
        app.run("net.create", json!({"name": name}));
    }
    for (reference, dx) in [("V1", -30.0), ("R1", 0.0), ("R2", 30.0)] {
        app.run(
            "component.add",
            json!({"kind": "resistor", "reference": reference, "position": {"x": c.x + dx, "y": c.y}}),
        );
    }
    let (v1, r1, r2) = (
        part_id(&app, "V1")?,
        part_id(&app, "R1")?,
        part_id(&app, "R2")?,
    );
    let pin = |component: Id, number: &str| json!({"component": component, "pin": number});
    let (v1_plus, v1_minus) = (pin_at(&app, v1, "1")?, pin_at(&app, v1, "2")?);
    let (r1_a, r1_b) = (pin_at(&app, r1, "1")?, pin_at(&app, r1, "2")?);
    let (r2_a, r2_b) = (pin_at(&app, r2, "1")?, pin_at(&app, r2, "2")?);
    let up = |p: Point, dy: f64| canvas::snap(Point::new(p.x, p.y + dy));
    let (vcc, mid, gnd) = (
        net_id(&app, "VCC")?,
        net_id(&app, "MID")?,
        net_id(&app, "GND")?,
    );
    app.run(
        "wire.place",
        json!({
            "points": [v1_plus, up(v1_plus, -12.0), up(r1_a, -12.0), r1_a],
            "pins": [pin(v1, "1"), pin(r1, "1")],
            "net": vcc
        }),
    );
    app.run(
        "wire.place",
        json!({"points": [r1_b, r2_a], "pins": [pin(r1, "2"), pin(r2, "1")], "net": mid}),
    );
    app.run(
        "wire.place",
        json!({
            "points": [v1_minus, up(v1_minus, 12.0), up(r2_b, 12.0), r2_b],
            "pins": [pin(v1, "2"), pin(r2, "2")],
            "net": gnd
        }),
    );
    if app.project.drawn_wires.len() != 3 {
        return Err(format!("the divider needs three wires: {}", app.message));
    }
    for (reference, model) in [("V1", "V"), ("R1", "R"), ("R2", "R")] {
        app.run(
            "component.field",
            json!({"reference": reference, "key": "sim.model", "value": model}),
        );
    }
    app.run(
        "component.field",
        json!({"reference": "V1", "key": "sim.value", "value": "10"}),
    );
    for (reference, value) in [("R1", "1k"), ("R2", "1k")] {
        app.run(
            "component.field",
            json!({"reference": reference, "key": "sim.value", "value": value}),
        );
    }
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![], Modifiers::NONE);
    }
    Ok(Divider {
        middle: Point::new((r1_b.x + r2_a.x) / 2.0, (r1_b.y + r2_a.y) / 2.0),
        mid_net: mid,
        app,
        ctx,
    })
}

/// Press the control whose gate id ends with `:label`, like a hand.
fn press_control(ctx: &egui::Context, app: &mut App, label: &str) -> Result<(), String> {
    for _ in 0..3 {
        frame(ctx, app, vec![egui::Event::PointerGone], Modifiers::NONE);
    }
    let suffix = format!(":{}", super::audit::fold_label(label));
    let control = super::audit::controls(ctx, app)
        .into_iter()
        .find(|c| c.enabled && c.id.as_deref().is_some_and(|id| id.ends_with(&suffix)))
        .ok_or_else(|| format!("no control named {label}"))?;
    tap_at(ctx, app, control.rect.center())
}

/// Arm the Probe tool from the tool strip and click the middle wire.
fn probe_the_middle(d: &mut Divider) -> Result<(), String> {
    tap_strip(&d.ctx, &mut d.app, "icon:tool-probe")?;
    assert!(
        matches!(d.app.tool, Tool::Probe),
        "the Probe button arms the Probe tool: {}",
        d.app.message
    );
    let at = screen_of(&d.app, d.middle);
    click(&d.ctx, &mut d.app, at, Modifiers::NONE);
    Ok(())
}

#[test]
fn a_newcomer_probes_a_net_and_reads_its_voltage() -> Result<(), String> {
    let mut d = divider()?;
    probe_the_middle(&mut d)?;
    let probe = d
        .app
        .project
        .probes
        .first()
        .ok_or_else(|| format!("the click placed no probe: {}", d.app.message))?;
    assert_eq!(
        probe.net,
        Some(d.mid_net),
        "the probe sits on the middle net"
    );
    assert_eq!(d.app.project.probes.len(), 1, "one click, one probe");
    assert!(
        paints_text(&d.ctx, &mut d.app, "P1: 5.000 V")?,
        "the badge reads the divider's middle voltage"
    );
    // The same value is in the Analyze table.
    let _tall = TallScope::enter();
    d.app.change_tab(Tab::Analyze);
    for _ in 0..3 {
        frame(&d.ctx, &mut d.app, vec![], Modifiers::NONE);
    }
    assert!(
        paints_text(&d.ctx, &mut d.app, "5.000 V")?,
        "the Analyze table shows 5.000 V"
    );
    assert!(
        paints_text(&d.ctx, &mut d.app, "MID")?,
        "the table names the point"
    );
    assert!(
        !paints_text(
            &d.ctx,
            &mut d.app,
            "Place a probe on the schematic to see its voltage here."
        )?,
        "with a probe placed the empty state is gone"
    );
    press_control(&d.ctx, &mut d.app, "Remove")?;
    assert!(
        d.app.project.probes.is_empty(),
        "Remove takes the probe out of the design: {}",
        d.app.message
    );
    assert!(
        paints_text(
            &d.ctx,
            &mut d.app,
            "Place a probe on the schematic to see its voltage here."
        )?,
        "the table says how to place one"
    );
    Ok(())
}

#[test]
fn a_newcomer_removes_a_probe_from_its_badge_and_stops_the_tool_with_escape() -> Result<(), String>
{
    let mut d = divider()?;
    probe_the_middle(&mut d)?;
    assert_eq!(d.app.project.probes.len(), 1, "the probe is placed");
    assert!(
        matches!(d.app.tool, Tool::Probe),
        "the tool stays armed for the next probe"
    );
    key(&d.ctx, &mut d.app, egui::Key::Escape);
    assert!(
        matches!(d.app.tool, Tool::Select),
        "Esc stops the Probe tool"
    );
    assert_eq!(d.app.project.probes.len(), 1, "Esc keeps the placed probe");
    press_control(&d.ctx, &mut d.app, "Remove probe P1")?;
    assert!(
        d.app.project.probes.is_empty(),
        "the cross on the badge removes the probe: {}",
        d.app.message
    );
    Ok(())
}

/// Where the layout story clicks, in board millimetres.
struct Spots {
    /// The middle of R1's second pad: its pin is on the middle net.
    pad: Point,
    /// The middle of a track on the supply net.
    track: Point,
}

/// Centre the board view on the two spots at a zoom where a click lands on one object.
fn frame_spots(d: &mut Divider, spots: &Spots) {
    d.app.pcb.center = Point::new(
        (spots.pad.x + spots.track.x) / 2.0,
        (spots.pad.y + spots.track.y) / 2.0,
    );
    d.app.pcb.zoom = 12.0;
    for _ in 0..3 {
        frame(&d.ctx, &mut d.app, vec![], Modifiers::NONE);
    }
}

/// The divider synced to the board, with a track on the supply net, shown on the Layout tab.
fn layout_divider() -> Result<(Divider, Spots), String> {
    let mut d = divider()?;
    d.app.run("board.sync", json!({}));
    let r1 = part_id(&d.app, "R1")?;
    let vcc = net_id(&d.app, "VCC")?;
    let footprint = d
        .app
        .project
        .board
        .footprints
        .iter()
        .find(|f| f.component == r1)
        .ok_or("R1 has no footprint on the board")?;
    let pad = footprint.pad_position("2").ok_or("R1 has no pad 2")?;
    if !footprint
        .pads
        .iter()
        .any(|p| p.number == "2" && p.net == Some(d.mid_net))
    {
        return Err("pad 2 of R1 is not on the middle net after the sync".into());
    }
    let (x, y) = (footprint.position.x, footprint.position.y + 6.0);
    d.app.run(
        "track.add",
        json!({"net": vcc, "start": {"x": x - 6.0, "y": y}, "end": {"x": x + 6.0, "y": y}, "width": 0.25, "layer": "F.Cu"}),
    );
    if d.app.project.board.tracks.len() != 1 {
        return Err(format!("the track was not added: {}", d.app.message));
    }
    d.app.change_tab(Tab::Pcb);
    let spots = Spots {
        pad,
        track: Point::new(x, y),
    };
    frame_spots(&mut d, &spots);
    Ok((d, spots))
}

/// Press the cross on the badge that paints `badge_text`. Every cross has the same folded name,
/// so the one on the same row as that text and to its right is the one pressed.
fn press_cross_of(ctx: &egui::Context, app: &mut App, badge_text: &str) -> Result<(), String> {
    let text = painted_text_at(ctx, app, badge_text)?
        .ok_or_else(|| format!("no badge paints {badge_text}"))?;
    let suffix = format!(":{}", super::audit::fold_label("Remove probe P1"));
    let cross = super::audit::controls(ctx, app)
        .into_iter()
        .find(|c| {
            c.enabled
                && c.id.as_deref().is_some_and(|id| id.ends_with(&suffix))
                && c.rect.center().x > text.x
                && (c.rect.center().y - text.y).abs() < 20.0
        })
        .ok_or_else(|| format!("no cross sits on the badge {badge_text}"))?;
    tap_at(ctx, app, cross.rect.center())
}

#[test]
fn a_newcomer_probes_a_pad_and_a_track_on_the_layout() -> Result<(), String> {
    let (mut d, spots) = layout_divider()?;
    let r1 = part_id(&d.app, "R1")?;
    let vcc = net_id(&d.app, "VCC")?;
    tap_strip(&d.ctx, &mut d.app, "icon:tool-probe")?;
    assert!(
        matches!(d.app.tool, Tool::Probe),
        "the Probe button of the Layout arms the Probe tool: {}",
        d.app.message
    );
    // A pad probes the pin behind it.
    let at = screen_of(&d.app, spots.pad);
    click(&d.ctx, &mut d.app, at, Modifiers::NONE);
    let first = d
        .app
        .project
        .probes
        .first()
        .ok_or_else(|| format!("the click on the pad placed no probe: {}", d.app.message))?;
    assert_eq!(
        first.pin,
        Some(PinRef {
            component: r1,
            pin: "2".into(),
            unit: None
        }),
        "the probe sits on pin 2 of R1"
    );
    assert!(
        paints_text(&d.ctx, &mut d.app, "P1: 5.000 V")?,
        "the badge on the pad reads the middle voltage"
    );
    // A track probes its net.
    let at = screen_of(&d.app, spots.track);
    click(&d.ctx, &mut d.app, at, Modifiers::NONE);
    assert_eq!(
        d.app.project.probes.len(),
        2,
        "the click on the track placed a second probe: {}",
        d.app.message
    );
    assert_eq!(
        d.app.project.probes.get(1).and_then(|p| p.net),
        Some(vcc),
        "the second probe sits on the supply net"
    );
    assert!(
        paints_text(&d.ctx, &mut d.app, "P2: 10.000 V")?,
        "the badge on the track reads the supply voltage"
    );
    // The Analyze table lists both with the same voltages.
    {
        let _tall = TallScope::enter();
        d.app.change_tab(Tab::Analyze);
        for _ in 0..3 {
            frame(&d.ctx, &mut d.app, vec![], Modifiers::NONE);
        }
        for word in ["P1", "R1.2", "5.000 V", "P2", "VCC", "10.000 V"] {
            assert!(
                paints_text(&d.ctx, &mut d.app, word)?,
                "the Analyze table shows {word}"
            );
        }
    }
    // The cross on a badge removes that probe and leaves the other.
    d.app.change_tab(Tab::Pcb);
    frame_spots(&mut d, &spots);
    press_cross_of(&d.ctx, &mut d.app, "P1: 5.000 V")?;
    assert_eq!(
        d.app.project.probes.len(),
        1,
        "the cross removes one probe: {}",
        d.app.message
    );
    assert_eq!(
        d.app.project.probes.first().and_then(|p| p.net),
        Some(vcc),
        "the supply probe is the one that stays"
    );
    assert!(
        !paints_text(&d.ctx, &mut d.app, "P1: 5.000 V")?,
        "the removed badge is gone"
    );
    assert!(
        paints_text(&d.ctx, &mut d.app, "P2: 10.000 V")?,
        "the other badge stays"
    );
    Ok(())
}
