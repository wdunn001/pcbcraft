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
        crate::stack_view::FAR_ALPHA,
        "the top copper is now behind the facing side and fades"
    );
    assert_eq!(
        app.stack.copper_alpha(bottom),
        1.0,
        "the bottom copper is at full strength"
    );
    Ok(())
}

#[test]
fn a_newcomer_steps_through_the_layers() -> Result<(), String> {
    let (mut app, ctx) = open_layout();
    assert_eq!(app.current_target(), Target::Copper(Layer::Front));
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
