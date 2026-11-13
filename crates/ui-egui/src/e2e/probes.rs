//! Stories for the virtual DC probes: a probe on the schematic reads its voltage from the
//! simulator, shows it in a badge and in the Analyze table, and goes away again.
//!
//! The layout stories at the end do the same on the board: a pad or a track gets a probe and a badge.
//!
//! Context: issues #540 and #543. The divider below is a 10 V source across two 1 k resistors, so
//! the middle net reads exactly 5 V; no story here expects a number the solver did not produce.
use super::super::bottom_sheet::SeverityFilter;
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
    // The view fits the design on the first frames after a tab change; frame it after that.
    for _ in 0..3 {
        frame(&d.ctx, &mut d.app, vec![], Modifiers::NONE);
    }
    d.app.pcb.center = Point::new(
        (spots.pad.x + spots.track.x) / 2.0,
        (spots.pad.y + spots.track.y) / 2.0,
    );
    d.app.pcb.zoom = 12.0;
    frame(&d.ctx, &mut d.app, vec![], Modifiers::NONE);
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

/// Type `text` into the expected minimum or maximum box of probe P1 in the Analyze table.
fn type_expected(d: &mut Divider, which: &str, text: &str) -> Result<(), String> {
    let name = format!("Expected {which} of P1");
    let id = format!("textinput:{}", super::audit::fold_label(&name));
    type_into(&d.ctx, &mut d.app, &id, text)
}

/// Empty one expected box the way a hand does: click in, select all, delete, press Enter.
fn clear_expected(d: &mut Divider, which: &str) -> Result<(), String> {
    let name = format!("Expected {which} of P1");
    let id = format!("textinput:{}", super::audit::fold_label(&name));
    tap_id(&d.ctx, &mut d.app, &id)?;
    let _tall = TallScope::enter();
    frame(
        &d.ctx,
        &mut d.app,
        vec![egui::Event::Key {
            key: egui::Key::A,
            physical_key: Some(egui::Key::A),
            pressed: true,
            repeat: false,
            modifiers: Modifiers::COMMAND,
        }],
        Modifiers::COMMAND,
    );
    key(&d.ctx, &mut d.app, egui::Key::Backspace);
    key(&d.ctx, &mut d.app, egui::Key::Enter);
    frame(&d.ctx, &mut d.app, vec![], Modifiers::NONE);
    Ok(())
}

/// True when the Analyze page, on a screen tall enough for it, paints `needle`.
fn table_shows(d: &mut Divider, needle: &str) -> Result<bool, String> {
    let _tall = TallScope::enter();
    for _ in 0..3 {
        frame(&d.ctx, &mut d.app, vec![], Modifiers::NONE);
    }
    paints_text(&d.ctx, &mut d.app, needle)
}

/// The range the first probe holds, as saved in the design.
fn saved_range(d: &Divider) -> Option<(Option<f64>, Option<f64>)> {
    d.app
        .project
        .probes
        .first()
        .and_then(|p| p.expect)
        .map(|e| (e.min, e.max))
}

/// The message of the probe finding in the findings list, when there is one.
fn probe_finding(d: &mut Divider) -> Option<String> {
    d.app
        .findings()
        .visible(SeverityFilter::All)
        .find(|r| r.code == "probe_out_of_range")
        .map(|r| r.message.clone())
}

/// Go to a tab and let the view settle.
fn go_to(d: &mut Divider, tab: Tab) {
    d.app.change_tab(tab);
    for _ in 0..3 {
        frame(&d.ctx, &mut d.app, vec![], Modifiers::NONE);
    }
}

#[test]
fn a_newcomer_checks_a_probe_against_the_voltage_it_should_read() -> Result<(), String> {
    let mut d = divider()?;
    probe_the_middle(&mut d)?;
    assert!(
        paints_text(&d.ctx, &mut d.app, "P1: 5.000 V")?,
        "the new probe reads the middle voltage"
    );
    assert!(
        !paints_text(&d.ctx, &mut d.app, "P1: 5.000 V ok")?,
        "with no range set the badge gives no verdict"
    );
    // The Analyze table has two empty boxes and no verdict yet.
    go_to(&mut d, Tab::Analyze);
    assert!(
        table_shows(&mut d, "No limit")?,
        "an empty expected box says it has no limit"
    );
    assert!(
        table_shows(&mut d, "Not checked")?,
        "with no range the status says not checked"
    );
    assert_eq!(saved_range(&d), None, "no range is saved yet");
    // A range around 5 V: the divider passes.
    type_expected(&mut d, "minimum", "4.9")?;
    assert_eq!(
        saved_range(&d),
        Some((Some(4.9), None)),
        "the minimum alone is saved: {}",
        d.app.message
    );
    type_expected(&mut d, "maximum", "5.1")?;
    assert_eq!(
        saved_range(&d),
        Some((Some(4.9), Some(5.1))),
        "both bounds are saved: {}",
        d.app.message
    );
    assert!(
        table_shows(&mut d, "In range")?,
        "5 V is inside 4.9 to 5.1 V and the table says so"
    );
    assert_eq!(
        probe_finding(&mut d),
        None,
        "a passing probe raises no finding"
    );
    go_to(&mut d, Tab::Schematic);
    assert!(
        paints_text(&d.ctx, &mut d.app, "P1: 5.000 V ok")?,
        "the badge says ok in words"
    );
    // A range that 5 V is outside of: the probe fails and the design says so.
    go_to(&mut d, Tab::Analyze);
    type_expected(&mut d, "maximum", "5.4")?;
    type_expected(&mut d, "minimum", "5.2")?;
    assert_eq!(
        saved_range(&d),
        Some((Some(5.2), Some(5.4))),
        "the new range is saved: {}",
        d.app.message
    );
    assert!(
        table_shows(&mut d, "Out of range")?,
        "5 V is outside 5.2 to 5.4 V and the table says so"
    );
    let finding = "Probe P1 reads 5 V, outside the expected 5.2 to 5.4 V";
    assert_eq!(
        probe_finding(&mut d).as_deref(),
        Some(finding),
        "the findings list names the probe, the reading and the range"
    );
    go_to(&mut d, Tab::Schematic);
    assert!(
        paints_text(&d.ctx, &mut d.app, "P1: 5.000 V out of range 5.2 to 5.4 V")?,
        "the badge says out of range in words"
    );
    key(&d.ctx, &mut d.app, egui::Key::Escape);
    authoring::tests::open_page(&d.ctx, &mut d.app, "Checks")?;
    frame(&d.ctx, &mut d.app, vec![], Modifiers::NONE);
    assert!(
        d.app.region().bottom_open,
        "View > Checks opens the Findings sheet: {}",
        d.app.message
    );
    assert_eq!(
        probe_finding(&mut d).as_deref(),
        Some(finding),
        "the sheet's findings list holds the finding"
    );
    let shown = paints_text(&d.ctx, &mut d.app, finding)?;
    let on_screen: String = {
        let output = frame(&d.ctx, &mut d.app, vec![], Modifiers::NONE);
        output
            .shapes
            .iter()
            .filter_map(|s| match &s.shape {
                egui::Shape::Text(t) => Some(t.galley.text().to_owned()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(" | ")
    };
    assert!(
        shown,
        "the Findings sheet paints the finding text (tab {:?}, filter {:?}, checks {}): {on_screen}",
        d.app.tab, d.app.sheet.filter, d.app.checks["drc"]
    );
    d.app.region().bottom_open = false;
    // A refused entry changes nothing and says why.
    go_to(&mut d, Tab::Analyze);
    type_expected(&mut d, "minimum", "9999")?;
    assert_eq!(
        saved_range(&d),
        Some((Some(5.2), Some(5.4))),
        "a number out of bounds changes nothing"
    );
    assert!(
        table_shows(
            &mut d,
            "Enter a voltage from -1000 to 1000 V, or leave the box empty."
        )?,
        "the table says what to type instead"
    );
    // Back to a range 5 V is inside: the finding goes away.
    type_expected(&mut d, "minimum", "4.9")?;
    type_expected(&mut d, "maximum", "5.1")?;
    assert_eq!(
        saved_range(&d),
        Some((Some(4.9), Some(5.1))),
        "the range is back: {}",
        d.app.message
    );
    assert!(
        table_shows(&mut d, "In range")?,
        "the table passes the probe again"
    );
    assert_eq!(
        probe_finding(&mut d),
        None,
        "the finding goes when the probe passes"
    );
    // An empty box drops a bound, and with both empty the range is gone.
    clear_expected(&mut d, "maximum")?;
    assert_eq!(
        saved_range(&d),
        Some((Some(4.9), None)),
        "an empty box drops its bound"
    );
    clear_expected(&mut d, "minimum")?;
    assert_eq!(saved_range(&d), None, "no bounds, no range");
    assert!(
        table_shows(&mut d, "Not checked")?,
        "with the range gone the status is not checked"
    );
    Ok(())
}
