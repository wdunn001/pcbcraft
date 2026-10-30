//! Stories for the minimap: peek from the Map button or a held key, jump to a
//! region, pin it, move and resize it, and peek at another schematic sheet.
use super::super::*;
use super::support::*;
use crate::minimap::{MapFrame, build_overview, view_box};

fn open(tab: Tab) -> (App, egui::Context) {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    app.change_tab(tab);
    app.refresh();
    let ctx = egui::Context::default();
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    (app, ctx)
}

fn rect_of(app: &App, id: &str) -> Result<Rect, String> {
    app.icon_rects
        .get(id)
        .copied()
        .ok_or_else(|| format!("{id} is not on screen"))
}

fn press_at(ctx: &egui::Context, app: &mut App, pos: egui::Pos2, pressed: bool) {
    frame(
        ctx,
        app,
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            },
        ],
        egui::Modifiers::NONE,
    );
}

/// Press at `from`, drag to `to` over a few frames and release.
fn drag(ctx: &egui::Context, app: &mut App, from: egui::Pos2, to: egui::Pos2) {
    frame(
        ctx,
        app,
        vec![egui::Event::PointerMoved(from)],
        egui::Modifiers::NONE,
    );
    press_at(ctx, app, from, true);
    for step in 1..=4 {
        let at = from + (to - from) * (step as f32 / 4.0);
        frame(
            ctx,
            app,
            vec![egui::Event::PointerMoved(at)],
            egui::Modifiers::NONE,
        );
    }
    press_at(ctx, app, to, false);
}

fn key(key: egui::Key, pressed: bool) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }
}

fn settle(ctx: &egui::Context, app: &mut App, frames: usize) {
    for _ in 0..frames {
        frame(ctx, app, vec![], egui::Modifiers::NONE);
    }
}

/// Move the pointer well away from the map and the card.
fn pointer_away(ctx: &egui::Context, app: &mut App) {
    let at = app.canvas_rect.left_top() + Vec2::new(60.0, 60.0);
    frame(
        ctx,
        app,
        vec![egui::Event::PointerMoved(at)],
        egui::Modifiers::NONE,
    );
}

fn map_frame(app: &App) -> Result<MapFrame, String> {
    let o = build_overview(&app.project, app.tab == Tab::Pcb, app.schematic_page());
    Ok(MapFrame {
        bounds: o.bounds,
        rect: rect_of(app, "minimap-surface")?,
    })
}

#[test]
fn a_newcomer_peeks_at_the_whole_board_and_jumps_to_a_region() -> Result<(), String> {
    let (mut app, ctx) = open(Tab::Pcb);
    app.preferences.reduced_motion = true;
    assert!(!app.minimap_visible(), "nothing else is on screen at rest");
    authoring::tests::tap_icon(&ctx, &mut app, "canvas-map")?;
    settle(&ctx, &mut app, 2);
    assert!(app.minimap_visible(), "the Map button opens the peek");
    let f = map_frame(&app)?;
    let canvas = app.canvas_rect.size();
    let a = f.to_map(pcbcraft_geom::Point::new(10.0, 8.0));
    let b = f.to_map(pcbcraft_geom::Point::new(30.0, 20.0));
    drag(&ctx, &mut app, a, b);
    settle(&ctx, &mut app, 3);
    let (lo, hi) = view_box(app.pcb, canvas);
    assert!(
        lo.x <= 10.5 && hi.x >= 29.5 && lo.y <= 8.5 && hi.y >= 19.5,
        "the canvas shows the dragged region: {lo:?} {hi:?}"
    );
    assert!(
        (hi.x - lo.x) < 80.0,
        "and it zoomed in to it, not out: {}",
        hi.x - lo.x
    );
    assert!(
        !app.minimap_visible(),
        "releasing goes and puts the map away"
    );

    // Esc leaves the view as it was.
    let before = (app.pcb.center, app.pcb.zoom);
    authoring::tests::tap_icon(&ctx, &mut app, "canvas-map")?;
    settle(&ctx, &mut app, 2);
    let f = map_frame(&app)?;
    let from = f.to_map(pcbcraft_geom::Point::new(40.0, 25.0));
    let to = f.to_map(pcbcraft_geom::Point::new(55.0, 32.0));
    frame(
        &ctx,
        &mut app,
        vec![egui::Event::PointerMoved(from)],
        egui::Modifiers::NONE,
    );
    press_at(&ctx, &mut app, from, true);
    frame(
        &ctx,
        &mut app,
        vec![egui::Event::PointerMoved(to)],
        egui::Modifiers::NONE,
    );
    frame(
        &ctx,
        &mut app,
        vec![
            egui::Event::PointerMoved(to + Vec2::new(2.0, 2.0)),
            key(egui::Key::Escape, true),
        ],
        egui::Modifiers::NONE,
    );
    press_at(&ctx, &mut app, to, false);
    settle(&ctx, &mut app, 3);
    assert_eq!(
        before,
        (app.pcb.center, app.pcb.zoom),
        "Esc cancels the jump"
    );

    // A click on a point keeps the zoom and centres there.
    authoring::tests::tap_icon(&ctx, &mut app, "canvas-map")?;
    settle(&ctx, &mut app, 2);
    let f = map_frame(&app)?;
    let zoom = app.pcb.zoom;
    tap_at(
        &ctx,
        &mut app,
        f.to_map(pcbcraft_geom::Point::new(50.0, 30.0)),
    )?;
    settle(&ctx, &mut app, 3);
    assert_eq!(app.pcb.zoom, zoom);
    assert!((app.pcb.center.x - 50.0).abs() < 1.5 && (app.pcb.center.y - 30.0).abs() < 1.5);
    Ok(())
}

#[test]
fn a_newcomer_glides_to_the_region_unless_motion_is_reduced() -> Result<(), String> {
    let (mut app, ctx) = open(Tab::Pcb);
    app.preferences.reduced_motion = false;
    authoring::tests::tap_icon(&ctx, &mut app, "canvas-map")?;
    settle(&ctx, &mut app, 2);
    let f = map_frame(&app)?;
    let at = f.to_map(pcbcraft_geom::Point::new(60.0, 35.0));
    tap_at(&ctx, &mut app, at)?;
    assert!((app.pcb.center.x - 60.0).abs() > 1.0, "a glide takes time");
    settle(&ctx, &mut app, 90);
    assert!(
        (app.pcb.center.x - 60.0).abs() < 1.5,
        "it arrives: {:?}",
        app.pcb.center
    );
    Ok(())
}

#[test]
fn a_newcomer_holds_a_key_to_peek_and_chooses_a_region_with_the_arrows() -> Result<(), String> {
    let (mut app, ctx) = open(Tab::Pcb);
    app.preferences.reduced_motion = true;
    pointer_away(&ctx, &mut app);
    frame(
        &ctx,
        &mut app,
        vec![key(egui::Key::G, true)],
        egui::Modifiers::NONE,
    );
    settle(&ctx, &mut app, 2);
    assert!(
        app.minimap_visible(),
        "holding the key shows the map with no click"
    );
    let before = app.pcb.center;
    frame(
        &ctx,
        &mut app,
        vec![key(egui::Key::ArrowRight, true)],
        egui::Modifiers::NONE,
    );
    frame(
        &ctx,
        &mut app,
        vec![key(egui::Key::ArrowRight, false)],
        egui::Modifiers::NONE,
    );
    assert!(app.minimap.region.is_some(), "the arrows move a region box");
    assert!(
        app.minimap.announced.contains("Going to the region"),
        "the target is announced: {}",
        app.minimap.announced
    );
    frame(
        &ctx,
        &mut app,
        vec![key(egui::Key::Enter, true)],
        egui::Modifiers::NONE,
    );
    settle(&ctx, &mut app, 3);
    assert!(app.pcb.center.x > before.x, "Enter goes to the region");
    // Releasing the key puts the map away.
    frame(
        &ctx,
        &mut app,
        vec![key(egui::Key::G, false)],
        egui::Modifiers::NONE,
    );
    settle(&ctx, &mut app, 2);
    assert!(!app.minimap_visible());

    // Esc drops the box and leaves the view alone.
    let at = (app.pcb.center, app.pcb.zoom);
    frame(
        &ctx,
        &mut app,
        vec![key(egui::Key::G, true)],
        egui::Modifiers::NONE,
    );
    frame(
        &ctx,
        &mut app,
        vec![key(egui::Key::ArrowDown, true)],
        egui::Modifiers::NONE,
    );
    frame(
        &ctx,
        &mut app,
        vec![key(egui::Key::Escape, true)],
        egui::Modifiers::NONE,
    );
    settle(&ctx, &mut app, 2);
    assert!(app.minimap.region.is_none());
    assert_eq!(at, (app.pcb.center, app.pcb.zoom));
    Ok(())
}

#[test]
fn a_newcomer_pins_the_minimap() -> Result<(), String> {
    let (mut app, ctx) = open(Tab::Pcb);
    app.preferences.reduced_motion = true;
    authoring::tests::tap_icon(&ctx, &mut app, "canvas-map")?;
    settle(&ctx, &mut app, 2);
    assert!(!app.map_prefs().pinned);
    authoring::tests::tap_icon(&ctx, &mut app, "minimap-pin")?;
    settle(&ctx, &mut app, 2);
    assert!(app.map_prefs().pinned, "the pin icon toggles it on");
    assert!(
        app.preferences
            .minimap
            .get("layout")
            .is_some_and(|p| p.pinned),
        "and it is remembered for this tab"
    );
    pointer_away(&ctx, &mut app);
    settle(&ctx, &mut app, 3);
    assert!(
        app.minimap_visible(),
        "a pinned map stays with the pointer away"
    );
    assert!(app.minimap.chrome < 0.01, "no frame or controls while away");
    assert_eq!(
        app.map_prefs().opacity,
        0.55,
        "faded to the default opacity"
    );

    // Pointer over: the chrome comes in.
    let over = rect_of(&app, "minimap-surface")?.center();
    frame(
        &ctx,
        &mut app,
        vec![egui::Event::PointerMoved(over)],
        egui::Modifiers::NONE,
    );
    assert!(app.minimap.chrome > 0.99, "instant with Reduce motion");

    // Opacity slider popup.
    authoring::tests::tap_icon(&ctx, &mut app, "minimap-opacity")?;
    settle(&ctx, &mut app, 2);
    assert!(app.minimap.opacity_open);
    let slider = rect_of(&app, "minimap-opacity-slider")?;
    tap_at(&ctx, &mut app, slider.left_center() + Vec2::new(2.0, 0.0))?;
    assert!(
        app.map_prefs().opacity <= 0.2,
        "the slider lowers it: {}",
        app.map_prefs().opacity
    );
    frame(
        &ctx,
        &mut app,
        vec![key(egui::Key::Escape, true)],
        egui::Modifiers::NONE,
    );
    settle(&ctx, &mut app, 2);
    assert!(!app.minimap.opacity_open, "Esc closes the slider");
    assert!(app.minimap_visible(), "and leaves the map pinned");
    authoring::tests::tap_icon(&ctx, &mut app, "minimap-opacity")?;
    settle(&ctx, &mut app, 2);
    let away = app.canvas_rect.left_top() + Vec2::new(40.0, 120.0);
    tap_at(&ctx, &mut app, away)?;
    assert!(!app.minimap.opacity_open, "a click away closes the slider");

    // Resize from the bottom-right grip keeps the board's shape.
    let before = rect_of(&app, "minimap-surface")?;
    let grip = rect_of(&app, "minimap-resize")?.center();
    drag(&ctx, &mut app, grip, grip + Vec2::new(60.0, 0.0));
    settle(&ctx, &mut app, 2);
    let after = rect_of(&app, "minimap-surface")?;
    assert!(after.width() > before.width() + 20.0, "the map grew");
    assert!(
        (after.width() / after.height() - before.width() / before.height()).abs() < 0.02,
        "the board's shape is kept"
    );

    // The chips switch layers off and on.
    authoring::tests::tap_icon(&ctx, &mut app, "minimap-chip-findings")?;
    assert_eq!(app.minimap.show_findings, Some(false));
    authoring::tests::tap_icon(&ctx, &mut app, "minimap-chip-selection")?;
    authoring::tests::tap_icon(&ctx, &mut app, "minimap-chip-net")?;
    assert_eq!(app.minimap.show_net, Some(false));

    // Another tab has its own look; this one is still pinned afterwards.
    app.change_tab(Tab::Schematic);
    settle(&ctx, &mut app, 3);
    assert!(!app.map_prefs().pinned, "the schematic map is its own");
    app.change_tab(Tab::Pcb);
    settle(&ctx, &mut app, 3);
    assert!(app.map_prefs().pinned);
    assert!(app.map_prefs().width > 300.0, "size is remembered too");

    // Hide puts it away and unpins.
    frame(
        &ctx,
        &mut app,
        vec![egui::Event::PointerMoved(over)],
        egui::Modifiers::NONE,
    );
    authoring::tests::tap_icon(&ctx, &mut app, "minimap-hide")?;
    settle(&ctx, &mut app, 2);
    assert!(!app.map_prefs().pinned);
    Ok(())
}

#[test]
fn a_newcomer_moves_the_pinned_minimap() -> Result<(), String> {
    let (mut app, ctx) = open(Tab::Pcb);
    app.preferences.reduced_motion = true;
    authoring::tests::tap_icon(&ctx, &mut app, "canvas-map")?;
    authoring::tests::tap_icon(&ctx, &mut app, "minimap-pin")?;
    let over = rect_of(&app, "minimap-surface")?.center();
    frame(
        &ctx,
        &mut app,
        vec![egui::Event::PointerMoved(over)],
        egui::Modifiers::NONE,
    );
    settle(&ctx, &mut app, 2);
    let strip = rect_of(&app, "minimap-move")?;
    assert!(strip.width() > 40.0, "the title strip is a visible grip");
    let before = rect_of(&app, "minimap-surface")?;
    drag(
        &ctx,
        &mut app,
        strip.left_center() + Vec2::new(20.0, 0.0),
        strip.left_center() + Vec2::new(-100.0, -60.0),
    );
    settle(&ctx, &mut app, 2);
    let after = rect_of(&app, "minimap-surface")?;
    assert!(after.left() < before.left() - 50.0, "it moved left");
    assert!(after.top() < before.top() - 30.0, "and up");
    assert!(
        app.map_prefs().origin.is_some(),
        "the position is saved with the tab"
    );

    // Dragged far past the edge, it stays inside the canvas.
    let strip = rect_of(&app, "minimap-move")?;
    let corner = app.canvas_rect.left_top() + Vec2::new(-500.0, -500.0);
    drag(
        &ctx,
        &mut app,
        strip.left_center() + Vec2::new(20.0, 0.0),
        corner,
    );
    settle(&ctx, &mut app, 2);
    let edge = rect_of(&app, "minimap-surface")?;
    assert!(edge.left() >= app.canvas_rect.left() && edge.top() >= app.canvas_rect.top());
    let o = app.map_prefs().origin.ok_or("no saved position")?;
    assert!(o[0] >= 0.0 && o[1] >= 0.0);
    Ok(())
}

#[test]
fn a_newcomer_peeks_at_another_schematic_sheet() -> Result<(), String> {
    let (mut app, ctx) = open(Tab::Schematic);
    app.preferences.reduced_motion = true;
    app.tool_call("schematic.new_sheet", json!({"name": "Power"}));
    app.refresh();
    let page = app
        .project
        .sheets
        .first()
        .map(|s| s.id)
        .ok_or("the sheet was not added")?;
    let part = app
        .project
        .components
        .last()
        .map(|c| c.id)
        .ok_or("no part")?;
    app.tool_call(
        "schematic.move_component",
        json!({"id": part, "sheet": page}),
    );
    app.refresh();
    settle(&ctx, &mut app, 3);
    authoring::tests::tap_icon(&ctx, &mut app, "canvas-map")?;
    settle(&ctx, &mut app, 2);
    assert!(app.minimap_visible());
    rect_of(&app, "minimap-sheet-0")?;
    let other = rect_of(&app, &format!("minimap-sheet-{page}"))?;
    let main_parts = build_overview(&app.project, false, 0).parts.len();
    tap_at(&ctx, &mut app, other.center())?;
    settle(&ctx, &mut app, 2);
    assert_eq!(
        app.minimap.peek_page,
        Some(page),
        "the tab peeks that sheet"
    );
    assert_eq!(
        app.schematic_page(),
        0,
        "the canvas stays on the sheet it was on"
    );
    let power_parts = build_overview(&app.project, false, page).parts.len();
    assert_eq!(power_parts, 1);
    assert!(main_parts >= 1);
    Ok(())
}
