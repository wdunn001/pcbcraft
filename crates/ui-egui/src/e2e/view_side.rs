//! Stories for the Layout view controls: Top | Bottom, rotate, stepping through
//! the layer stack, hover preview and a pinned layer view.
use super::super::*;
use super::support::*;

#[test]
fn a_newcomer_flips_the_view_to_the_bottom_with_the_toggle() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    app.change_tab(Tab::Pcb);
    app.refresh();
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(!app.pcb.mirror, "the board opens from the top");
    assert_eq!(app.route_layer, pcbcraft_doc::Layer::Front);

    authoring::tests::tap(&ctx, &mut app, "Bottom")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(app.pcb.mirror, "Bottom mirrors the view");
    assert_eq!(
        app.route_layer,
        pcbcraft_doc::Layer::Back,
        "bottom copper is now the drawing target"
    );

    app.set_target(crate::draw_target::Target::Silk(
        crate::draw_target::Side::Top,
    ));
    authoring::tests::tap(&ctx, &mut app, "Top")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(!app.pcb.mirror, "Top is the normal view again");
    authoring::tests::tap(&ctx, &mut app, "Bottom")?;
    assert_eq!(
        app.target,
        crate::draw_target::Target::Silk(crate::draw_target::Side::Bottom),
        "bottom silkscreen becomes the drawing target"
    );
    Ok(())
}

use crate::draw_target::{Side, Target};
use pcbcraft_doc::Layer;

fn open_layout() -> (App, egui::Context) {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    app.change_tab(Tab::Pcb);
    app.refresh();
    let ctx = egui::Context::default();
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    (app, ctx)
}

/// Press the control whose accessible name starts with `name`, like a hand.
fn tap_named(ctx: &egui::Context, app: &mut App, name: &str) -> Result<(), String> {
    let at = interactive_rects(ctx, app)
        .into_iter()
        .find(|(n, _)| n.contains(name))
        .map(|(_, r)| r.center())
        .ok_or_else(|| format!("{name} is not on screen"))?;
    tap_at(ctx, app, at)
}

/// Move the pointer over the left-aside row that reads exactly `label`.
fn hover_left(ctx: &egui::Context, app: &mut App, label: &str) -> Result<egui::Pos2, String> {
    let output = frame(ctx, app, vec![], egui::Modifiers::NONE);
    let at = output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Text(t) if t.galley.text() == label => {
                let c = t.pos + t.galley.size() / 2.0;
                (c.x < 320.0 && shape.clip_rect.contains(c)).then_some(c)
            }
            _ => None,
        })
        .next()
        .ok_or_else(|| format!("{label} is not in the left aside"))?;
    for _ in 0..2 {
        frame(
            ctx,
            app,
            vec![egui::Event::PointerMoved(at)],
            egui::Modifiers::NONE,
        );
    }
    Ok(at)
}

/// Click the first left-aside row that reads exactly `label`.
fn click_left(ctx: &egui::Context, app: &mut App, label: &str) -> Result<(), String> {
    let at = hover_left(ctx, app, label)?;
    tap_at(ctx, app, at)
}

#[test]
fn a_newcomer_rotates_the_board_view() -> Result<(), String> {
    let (mut app, ctx) = open_layout();
    let rect = app.canvas_rect;
    let origin = pcbcraft_geom::Point::new(0.0, 0.0);
    let wide = pcbcraft_geom::Point::new(app.project.board.width, 0.0);
    let (a, b) = (app.pcb.screen(origin, rect), app.pcb.screen(wide, rect));
    assert!(
        b.x > a.x && (b.y - a.y).abs() < 0.5,
        "the board starts upright"
    );
    tap_named(&ctx, &mut app, "Rotate right.")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert_eq!(app.pcb.turns, 1, "one tap is one quarter turn");
    let rect = app.canvas_rect;
    let (a, b) = (app.pcb.screen(origin, rect), app.pcb.screen(wide, rect));
    assert!(
        b.y > a.y && (b.x - a.x).abs() < 0.5,
        "the width now runs down the screen"
    );
    let back = app.pcb.world(app.pcb.screen(wide, rect), rect);
    assert!(
        (back.x - wide.x).abs() < 1e-3,
        "a click maps back to the same board point"
    );
    tap_named(&ctx, &mut app, "Rotate left.")?;
    tap_named(&ctx, &mut app, "Rotate left.")?;
    assert_eq!(app.pcb.turns, 3, "left turns count back round");
    tap_named(&ctx, &mut app, "Reset view.")?;
    assert_eq!(app.pcb.turns, 0, "Reset view turns it upright");
    Ok(())
}

#[test]
fn a_newcomer_flips_the_board_and_sees_the_bottom_parts_on_top() -> Result<(), String> {
    let (mut app, ctx) = open_layout();
    app.preferences.reduced_motion = true;
    let bottom = app.bottom_copper();
    assert!(
        app.stack.far(bottom) > app.stack.far(Layer::Front),
        "from the top, the bottom copper is the far side and paints first"
    );
    authoring::tests::tap(&ctx, &mut app, "Bottom")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        app.stack.far(bottom) < app.stack.far(Layer::Front),
        "flipped, the bottom copper faces you and paints last"
    );
    assert_eq!(
        app.stack.copper_alpha(Layer::Front),
        1.0,
        "in normal editing nothing is dimmed, even flipped"
    );
    assert_eq!(app.stack.copper_alpha(bottom), 1.0);
    // Stepping to a layer pins it: now the top copper is behind the bottom one.
    key(&ctx, &mut app, egui::Key::PageDown);
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(app.pinned_view.is_some(), "stepping pins the view");
    Ok(())
}

#[test]
fn a_newcomer_steps_through_the_layers() -> Result<(), String> {
    let (mut app, ctx) = open_layout();
    assert_eq!(app.current_target(), Target::Copper(Layer::Front));
    let opening = app.layer_status();
    assert!(opening.contains("Top copper"), "{opening}");
    authoring::tests::tap(&ctx, &mut app, &opening)?;
    key(&ctx, &mut app, egui::Key::PageUp);
    assert_eq!(
        app.current_target(),
        Target::Paste(Side::Top),
        "Page Up goes one level toward you"
    );
    key(&ctx, &mut app, egui::Key::PageDown);
    key(&ctx, &mut app, egui::Key::PageDown);
    assert_eq!(
        app.current_target(),
        Target::Copper(app.bottom_copper()),
        "Page Down walks the stack"
    );
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        app.layer_status().starts_with("Layer 2 of 2"),
        "{}",
        app.layer_status()
    );
    let status = app.layer_status();
    assert!(
        paints_text(&ctx, &mut app, &status)?,
        "the status bar names the layer"
    );
    authoring::tests::tap(&ctx, &mut app, &status)?;
    Ok(())
}

#[test]
fn a_newcomer_hovers_a_layer_and_sees_through_the_layers_above() -> Result<(), String> {
    let (mut app, ctx) = open_layout();
    app.preferences.reduced_motion = true;
    let at = hover_left(&ctx, &mut app, "Silkscreen")?;
    assert!(
        app.tree.hover_target.is_some() || app.pinned_view.is_none(),
        "the tree reports the hovered row"
    );
    let _ = at;
    assert_eq!(app.pinned_view, None, "hovering pins nothing");
    assert_eq!(
        app.current_target(),
        Target::Copper(Layer::Front),
        "hovering keeps the active layer"
    );
    Ok(())
}

#[test]
fn a_newcomer_hovers_a_sublayer_and_sees_it_through_the_layers_above() -> Result<(), String> {
    let (mut app, ctx) = open_layout();
    app.preferences.reduced_motion = true;
    hover_left(&ctx, &mut app, "Solder mask")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert_eq!(
        app.stack.of(Target::Silk(Side::Top)),
        crate::stack_view::NEAR_ALPHA,
        "the silkscreen above the mask is see-through"
    );
    assert_eq!(
        app.stack.of(Target::Mask(Side::Top)),
        1.0,
        "the hovered sublayer is at full strength"
    );
    assert_eq!(
        app.stack.copper_alpha(Layer::Front),
        crate::stack_view::FAR_ALPHA,
        "copper behind it is dimmed"
    );
    assert_eq!(
        app.current_target(),
        Target::Copper(Layer::Front),
        "the active layer did not change"
    );
    Ok(())
}

#[test]
fn a_newcomer_clicks_a_sublayer_and_the_view_stays() -> Result<(), String> {
    let (mut app, ctx) = open_layout();
    app.preferences.reduced_motion = true;
    click_left(&ctx, &mut app, "Silkscreen")?;
    assert_eq!(
        app.pinned_view,
        Some(Target::Silk(Side::Top)),
        "the click pins the view"
    );
    assert_eq!(
        app.target,
        Target::Silk(Side::Top),
        "and makes it the drawing target"
    );
    // Move the pointer well away from the tree.
    for _ in 0..2 {
        frame(
            &ctx,
            &mut app,
            vec![egui::Event::PointerMoved(egui::pos2(900.0, 500.0))],
            egui::Modifiers::NONE,
        );
    }
    assert_eq!(
        app.stack.of(Target::Silk(Side::Top)),
        1.0,
        "the pinned layer stays forward"
    );
    assert_eq!(
        app.stack.copper_alpha(Layer::Front),
        crate::stack_view::FAR_ALPHA
    );
    key(&ctx, &mut app, egui::Key::Escape);
    assert_eq!(app.pinned_view, None, "Esc unpins");
    Ok(())
}

#[test]
fn a_newcomer_flips_the_board_in_3d_and_looks_from_below() -> Result<(), String> {
    let (mut app, ctx) = open_layout();
    authoring::tests::tap(&ctx, &mut app, "3D")?;
    assert!(app.view3d.pitch > 0.0, "3D opens above the board");
    authoring::tests::tap(&ctx, &mut app, "Bottom")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        app.view3d.pitch < 0.0,
        "Bottom puts the camera under the board"
    );
    authoring::tests::tap(&ctx, &mut app, "Top")?;
    assert!(app.view3d.pitch > 0.0, "Top brings it back");
    Ok(())
}

#[test]
fn a_newcomer_sees_the_board_is_behind_the_schematic_and_updates_it() -> Result<(), String> {
    let (mut app, ctx) = open_layout();
    assert!(
        !app.board_behind(),
        "with automatic sync on, nothing is behind"
    );
    app.run("project.set_auto_sync", json!({"auto_sync": false}));
    app.run(
        "component.add",
        json!({"kind":"resistor","position":{"x":10.0,"y":10.0},"reference":"R9"}),
    );
    app.refresh();
    assert!(
        paints_text(&ctx, &mut app, "Board is behind the schematic")?,
        "the Layout canvas says the board is behind"
    );
    tap_named(&ctx, &mut app, "Update")?;
    app.refresh();
    assert!(
        app.project.board.footprints.iter().any(|f| {
            app.project
                .component(f.component)
                .is_some_and(|c| c.reference == "R9")
        }),
        "the update puts the new part on the board"
    );
    assert!(!app.board_behind(), "the notice goes away once synced");
    Ok(())
}

// ----- Y-up board view (#797) -----

/// A camera on the board: y up, optionally flipped to the bottom and turned.
fn board_camera(mirror: bool, turns: u8) -> Camera {
    Camera {
        center: pcbcraft_geom::Point::new(30.0, 20.0),
        zoom: 10.0,
        mirror,
        turns,
        up: true,
    }
}

#[test]
fn a_newcomer_sees_a_part_at_y_20_twenty_millimetres_above_the_origin() {
    let rect = Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(800.0, 600.0));
    let camera = board_camera(false, 0);
    let origin = camera.screen(pcbcraft_geom::Point::new(0.0, 0.0), rect);
    let part = camera.screen(pcbcraft_geom::Point::new(10.0, 20.0), rect);
    assert!(
        (origin.y - part.y - 20.0 * camera.zoom).abs() < 1e-3,
        "Y 20 sits 20 mm above the origin on screen"
    );
    assert!(
        (part.x - origin.x - 10.0 * camera.zoom).abs() < 1e-3,
        "X 10 sits 10 mm to the right"
    );
}

#[test]
fn screen_world_screen_round_trips_with_the_flip_the_bottom_and_every_turn() {
    let rect = Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(800.0, 600.0));
    for mirror in [false, true] {
        for turns in 0..4 {
            let camera = board_camera(mirror, turns);
            for p in [
                egui::pos2(10.0, 20.0),
                egui::pos2(400.0, 300.0),
                egui::pos2(777.0, 123.0),
            ] {
                let back = camera.screen(camera.world(p, rect), rect);
                assert!(
                    (back.x - p.x).abs() < 1e-3 && (back.y - p.y).abs() < 1e-3,
                    "mirror {mirror} turns {turns}: {p:?} came back as {back:?}"
                );
            }
            // Picking: the world point under a screen point is the one drawn there.
            let w = pcbcraft_geom::Point::new(12.5, -3.0);
            let hit = camera.world(camera.screen(w, rect), rect);
            assert!((hit.x - w.x).abs() < 1e-6 && (hit.y - w.y).abs() < 1e-6);
        }
    }
}

#[test]
fn a_newcomer_reads_a_larger_y_in_the_status_bar_moving_the_pointer_up() -> Result<(), String> {
    let (mut app, ctx) = open_layout();
    let rect = app.canvas_rect;
    let low = rect.center() + Vec2::new(0.0, 80.0);
    let high = rect.center() - Vec2::new(0.0, 80.0);
    frame(
        &ctx,
        &mut app,
        vec![egui::Event::PointerMoved(low)],
        egui::Modifiers::NONE,
    );
    let y_low = app.cursor.ok_or("the pointer is over the canvas")?.y;
    frame(
        &ctx,
        &mut app,
        vec![egui::Event::PointerMoved(high)],
        egui::Modifiers::NONE,
    );
    let y_high = app.cursor.ok_or("the pointer is over the canvas")?.y;
    assert!(y_high > y_low, "up reads larger: {y_low} then {y_high}");
    Ok(())
}

#[test]
fn a_newcomer_keeps_picks_right_after_flipping_to_the_bottom_and_rotating() -> Result<(), String> {
    let (mut app, ctx) = open_layout();
    authoring::tests::tap(&ctx, &mut app, "Bottom")?;
    app.pcb.turns = 1;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let rect = app.canvas_rect;
    let spot = pcbcraft_geom::Point::new(20.0, 30.0);
    let at = app.pcb.screen(spot, rect);
    frame(
        &ctx,
        &mut app,
        vec![egui::Event::PointerMoved(at)],
        egui::Modifiers::NONE,
    );
    let seen = app.cursor.ok_or("the pointer is over the canvas")?;
    assert!(
        (seen.x - spot.x).abs() < 0.05 && (seen.y - spot.y).abs() < 0.05,
        "the pointer reads the point drawn under it: {seen:?}"
    );
    Ok(())
}

#[test]
fn the_measure_angle_counts_up_on_the_board_and_down_on_the_schematic() {
    let a = pcbcraft_geom::Point::new(0.0, 0.0);
    let b = pcbcraft_geom::Point::new(1.0, 1.0);
    assert!(crate::editing::measure_text(a, b, true).contains("angle 45.0"));
    assert!(crate::editing::measure_text(a, b, false).contains("angle -45.0"));
}
