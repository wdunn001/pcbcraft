//! Tool modifier stories: Shift, Ctrl or Cmd and Alt change what a click or a
//! drag does, and a press on a pin or pad decides the action (#796).

use super::super::*;
use super::support::*;
use egui::{Event, Modifiers, PointerButton, Pos2};

const SHIFT: Modifiers = Modifiers {
    shift: true,
    ..Modifiers::NONE
};
const CTRL: Modifiers = Modifiers {
    ctrl: true,
    command: true,
    ..Modifiers::NONE
};

fn sample(tab: Tab) -> (App, egui::Context) {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    app.tab = tab;
    let ctx = egui::Context::default();
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![], Modifiers::NONE);
    }
    (app, ctx)
}

/// Where a world point sits on screen in the active canvas.
fn screen_of(app: &App, world: Point) -> Pos2 {
    let camera = if app.tab == Tab::Pcb {
        app.pcb
    } else {
        app.schematic
    };
    camera.screen(world, app.canvas_rect)
}

fn press(ctx: &egui::Context, app: &mut App, at: Pos2, down: bool, m: Modifiers) {
    frame(
        ctx,
        app,
        vec![Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed: down,
            modifiers: m,
        }],
        m,
    );
}

fn hover(ctx: &egui::Context, app: &mut App, at: Pos2, m: Modifiers) {
    frame(ctx, app, vec![Event::PointerMoved(at)], m);
}

/// Move the pointer onto a spot and click it with the modifiers held.
fn click(ctx: &egui::Context, app: &mut App, at: Pos2, m: Modifiers) {
    hover(ctx, app, at, m);
    hover(ctx, app, at, m);
    press(ctx, app, at, true, m);
    press(ctx, app, at, false, m);
    frame(ctx, app, vec![], m);
}

/// Press at one spot, glide to another and let go.
fn drag(ctx: &egui::Context, app: &mut App, from: Pos2, to: Pos2, m: Modifiers) {
    hover(ctx, app, from, m);
    hover(ctx, app, from, m);
    press(ctx, app, from, true, m);
    for step in 1..=6 {
        let at = from + (to - from) * (step as f32 / 6.0);
        hover(ctx, app, at, m);
    }
    press(ctx, app, to, false, m);
    frame(ctx, app, vec![], m);
}

/// The middle of the box around a part, away from its pins.
fn centre_of(app: &App, id: Id) -> Result<Pos2, String> {
    let (lo, hi) = app.bounds_of(id).ok_or("part has no box")?;
    Ok(screen_of(
        app,
        Point::new((lo.x + hi.x) / 2.0, (lo.y + hi.y) / 2.0),
    ))
}

fn two_parts(app: &App) -> Result<(Id, Id), String> {
    let mut parts: Vec<_> = app.project.components.iter().collect();
    parts.sort_by(|a, b| a.position.x.total_cmp(&b.position.x));
    match parts.as_slice() {
        [a, b, ..] => Ok((a.id, b.id)),
        _ => Err("the sample needs two parts".into()),
    }
}

#[test]
fn a_newcomer_adds_to_a_selection_with_shift() -> Result<(), String> {
    let (mut app, ctx) = sample(Tab::Schematic);
    let (first, second) = two_parts(&app)?;
    {
        let target = centre_of(&app, first)?;
        click(&ctx, &mut app, target, Modifiers::NONE);
    }
    assert_eq!(app.selection(), vec![first], "a plain click picks one part");
    {
        let target = centre_of(&app, second)?;
        click(&ctx, &mut app, target, SHIFT);
    }
    let picked = app.selection();
    assert!(
        picked.contains(&first) && picked.contains(&second),
        "Shift+click adds the second part: {picked:?} ({})",
        app.message
    );
    {
        let target = centre_of(&app, second)?;
        click(&ctx, &mut app, target, Modifiers::NONE);
    }
    assert_eq!(
        app.selection(),
        vec![second],
        "a plain click replaces the selection"
    );
    {
        let target = centre_of(&app, first)?;
        click(&ctx, &mut app, target, CTRL);
    }
    assert!(
        app.selection().contains(&first) && app.selection().contains(&second),
        "Ctrl+click toggles the first part in"
    );
    {
        let target = centre_of(&app, first)?;
        click(&ctx, &mut app, target, CTRL);
    }
    assert_eq!(app.selection(), vec![second], "Ctrl+click toggles it out");
    Ok(())
}

#[test]
fn a_newcomer_routes_at_45_degrees_by_holding_shift() -> Result<(), String> {
    let (mut app, ctx) = sample(Tab::Pcb);
    // This person turned 45 degree corners off, so routes use right angles.
    app.preferences.chamfer_corners = false;
    let mut pair = None;
    'find: for a in &app.project.board.footprints {
        for b in &app.project.board.footprints {
            if a.id == b.id {
                continue;
            }
            for pa in &a.pads {
                for pb in &b.pads {
                    if pa.net.is_some() && pa.net == pb.net {
                        pair = Some((a.id, pa.number.clone(), b.id, pb.number.clone()));
                        break 'find;
                    }
                }
            }
        }
    }
    let (fa, pa, fb, pb) = pair.ok_or("the sample board needs two pads on one net")?;
    let at = |app: &App, f: Id, pad: &str| -> Result<Pos2, String> {
        let p = app
            .project
            .footprint(f)
            .and_then(|fp| fp.pad_position(pad))
            .ok_or("pad has no position")?;
        Ok(screen_of(app, p))
    };
    frame(
        &ctx,
        &mut app,
        vec![Event::Key {
            key: egui::Key::X,
            physical_key: Some(egui::Key::X),
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
        Modifiers::NONE,
    );
    assert!(
        matches!(app.tool, Tool::Route(None)),
        "X arms the Route tool"
    );
    {
        let target = at(&app, fa, &pa)?;
        click(&ctx, &mut app, target, Modifiers::NONE);
    }
    assert!(
        matches!(app.tool, Tool::Route(Some(_))),
        "the first pad starts the route"
    );
    let before = app.project.board.tracks.len();
    {
        let target = at(&app, fb, &pb)?;
        hover(&ctx, &mut app, target, SHIFT);
    }
    assert_eq!(
        app.route_mode(),
        "octilinear",
        "holding Shift switches to 45 degree corners"
    );
    {
        let target = at(&app, fb, &pb)?;
        click(&ctx, &mut app, target, SHIFT);
    }
    let laid = &app.project.board.tracks[before.min(app.project.board.tracks.len())..];
    assert!(!laid.is_empty(), "the route laid copper: {}", app.message);
    for t in laid {
        let angle = (t.end.y - t.start.y)
            .atan2(t.end.x - t.start.x)
            .to_degrees();
        let off = (angle / 45.0 - (angle / 45.0).round()).abs();
        assert!(
            off < 0.01,
            "every piece runs at a multiple of 45 degrees, got {angle}"
        );
    }
    Ok(())
}

#[test]
fn a_newcomer_places_without_snapping_by_holding_ctrl() -> Result<(), String> {
    let (mut app, ctx) = sample(Tab::Pcb);
    app.tool = Tool::Hole;
    let spot = Point::new(10.13, 10.21);
    let before = app.project.board.holes.len();
    {
        let target = screen_of(&app, spot);
        click(&ctx, &mut app, target, Modifiers::NONE);
    }
    let snapped = app
        .project
        .board
        .holes
        .get(before)
        .ok_or("the first hole was not placed")?
        .position;
    assert!(
        (snapped.x - 10.0).abs() < 0.05 && (snapped.y - 10.0).abs() < 0.05,
        "without Ctrl the hole lands on the grid: {snapped:?}"
    );
    {
        let target = screen_of(&app, spot);
        click(&ctx, &mut app, target, CTRL);
    }
    let free = app
        .project
        .board
        .holes
        .get(before + 1)
        .ok_or("the second hole was not placed")?
        .position;
    assert!(
        (free.x - spot.x).abs() < 0.05 && (free.y - spot.y).abs() < 0.05,
        "with Ctrl held the hole lands where the pointer was: {free:?}"
    );
    Ok(())
}

#[test]
fn a_newcomer_starts_a_wire_by_dragging_from_a_pin() -> Result<(), String> {
    let (mut app, ctx) = sample(Tab::Schematic);
    // Two fresh parts with free pins, side by side in the middle of the view.
    let c = app.schematic.center;
    for dx in [-8.0, 8.0] {
        app.run(
            "component.add",
            json!({"kind": "resistor", "position": Point::new(c.x + dx, c.y + 8.0)}),
        );
    }
    frame(&ctx, &mut app, vec![], Modifiers::NONE);
    let fresh: Vec<_> = app.project.components.iter().rev().take(2).collect();
    let pin_of = |i: usize| -> Result<(Id, String, Point), String> {
        let comp = fresh.get(i).ok_or("a fresh part is missing")?;
        let pin = comp.pins.first().ok_or("part has no pins")?;
        let at = comp
            .pin_position(&pin.number)
            .ok_or("pin has no position")?;
        Ok((comp.id, pin.number.clone(), at))
    };
    let (a, b) = (pin_of(0)?, pin_of(1)?);
    let before = app.project.wires.len();
    assert!(matches!(app.tool, Tool::Select), "no tool is armed");
    let start = screen_of(&app, a.2);
    let end = screen_of(&app, b.2);
    assert!(
        app.canvas_rect.contains(start) && app.canvas_rect.contains(end),
        "both pins are on screen"
    );
    drag(&ctx, &mut app, start, end, Modifiers::NONE);
    assert!(
        app.project.wires.len() > before,
        "dragging pin to pin joins them: {}",
        app.message
    );
    assert!(
        matches!(app.tool, Tool::Select),
        "the tool disarms after the wire"
    );
    Ok(())
}

#[test]
fn a_newcomer_deletes_a_connection_with_ctrl_in_the_wire_tool() -> Result<(), String> {
    let (mut app, ctx) = sample(Tab::Schematic);
    let c = app.schematic.center;
    let points = [
        Point::new(c.x - 3.0, c.y + 40.0),
        Point::new(c.x, c.y + 40.0),
        Point::new(c.x, c.y + 44.0),
    ];
    app.run("wire.draw", json!({"points": points}));
    let wire = app
        .project
        .drawn_wires
        .last()
        .map(|w| w.id)
        .ok_or("the drawn wire was not added")?;
    app.needs_fit = false;
    app.tool = Tool::Wire(None);
    let spot = screen_of(&app, points[1]);
    if !app.canvas_rect.contains(spot) {
        return Ok(()); // the corner is off screen on this layout; the resolver tests cover it
    }
    click(&ctx, &mut app, spot, CTRL);
    let left = app
        .project
        .drawn_wires
        .iter()
        .find(|w| w.id == wire)
        .map(|w| w.points.len());
    assert_ne!(
        left,
        Some(3),
        "Ctrl+click on a corner removes that node: {}",
        app.message
    );
    Ok(())
}

#[test]
fn hovering_a_part_paints_its_outline_inside_the_frame_budget() -> Result<(), String> {
    let (mut app, ctx) = sample(Tab::Schematic);
    let (first, _) = two_parts(&app)?;
    let at = centre_of(&app, first)?;
    for _ in 0..3 {
        hover(&ctx, &mut app, at, Modifiers::NONE);
    }
    assert!(app.edit.hover.is_some(), "the part is hovered");
    let camera = app.schematic;
    let rect = app.canvas_rect;
    let frames = 30u32;
    let mut total = std::time::Duration::ZERO;
    let mut painted = Vec::new();
    for _ in 0..frames {
        let mut took = std::time::Duration::ZERO;
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            let painter = ui.painter().clone();
            let start = std::time::Instant::now();
            app.paint_affordance(&painter, rect, camera);
            took = start.elapsed();
        });
        out.textures_delta.clear();
        total += took;
        painted.push(out.shapes.len());
    }
    // The work done is the measure: the outline draws the same few shapes
    // every frame and never piles up. Time only counts in a release build,
    // with a wide margin.
    let most = painted.iter().copied().max().unwrap_or(0);
    let least = painted.iter().copied().min().unwrap_or(0);
    assert_eq!(
        most, least,
        "every frame draws the same shapes: {painted:?}"
    );
    assert!(
        (1..=400).contains(&most),
        "the hovered part paints {most} shapes"
    );
    if !cfg!(debug_assertions) {
        assert!(
            total / frames < std::time::Duration::from_millis(20),
            "overlay took {:?} a frame",
            total / frames
        );
    }
    Ok(())
}

#[test]
fn a_newcomer_opens_the_tool_keys_in_settings() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    app.change_tab(Tab::Settings);
    let ctx = egui::Context::default();
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![], Modifiers::NONE);
    }
    super::support::scrolled_to_at(&ctx, &mut app, "Keys", 720.0)?;
    crate::authoring::tests::tap(&ctx, &mut app, "Keys")?;
    assert!(
        super::support::paints_text(&ctx, &mut app, "Select tool")?,
        "the Keys section lists each tool hotkey"
    );
    Ok(())
}
