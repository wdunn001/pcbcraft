use super::super::*;
use super::support::*;

fn open_3d() -> Result<(egui::Context, App, Id), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    app.refresh();
    let j1 = app
        .project
        .components
        .iter()
        .find(|c| c.reference == "J1")
        .map(|c| c.id)
        .ok_or("the practice design should carry J1")?;
    app.change_tab(Tab::Board3d);
    let ctx = egui::Context::default();
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    Ok((ctx, app, j1))
}

fn drag(ctx: &egui::Context, app: &mut App, from: egui::Pos2, by: egui::Vec2) {
    let button = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    frame(
        ctx,
        app,
        vec![egui::Event::PointerMoved(from)],
        egui::Modifiers::NONE,
    );
    frame(ctx, app, vec![button(from, true)], egui::Modifiers::NONE);
    for step in 1..=6 {
        let pos = from + by * (step as f32 / 6.0);
        frame(
            ctx,
            app,
            vec![egui::Event::PointerMoved(pos)],
            egui::Modifiers::NONE,
        );
    }
    frame(
        ctx,
        app,
        vec![button(from + by, false)],
        egui::Modifiers::NONE,
    );
    frame(ctx, app, vec![], egui::Modifiers::NONE);
}

#[test]
fn a_newcomer_opens_the_3d_view_and_sees_the_parts() -> Result<(), String> {
    let (ctx, mut app, _) = open_3d()?;
    assert!(
        paints_text(&ctx, &mut app, "Reset view")?,
        "the 3D tab shows its controls with no GPU renderer"
    );
    assert!(
        paints_text(&ctx, &mut app, "J1")?,
        "the parts list names the connector"
    );
    assert!(
        paints_text(&ctx, &mut app, "not loaded yet")?,
        "the tab says model files are not loaded"
    );
    let output = frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        output
            .shapes
            .iter()
            .any(|s| matches!(s.shape, egui::Shape::Mesh(_))),
        "the board is drawn as triangles"
    );
    Ok(())
}

#[test]
fn a_newcomer_turns_the_board_around_in_3d() -> Result<(), String> {
    let (ctx, mut app, _) = open_3d()?;
    let before = (app.view3d.yaw, app.view3d.pitch);
    let rect = app.view3d.rect;
    assert!(rect.width() > 100.0, "the viewport has a size");
    drag(&ctx, &mut app, rect.center(), egui::vec2(80.0, 30.0));
    let after = (app.view3d.yaw, app.view3d.pitch);
    assert!(
        (after.0 - before.0).abs() > 0.1,
        "dragging sideways turns the board: {before:?} -> {after:?}"
    );
    assert!(
        (after.1 - before.1).abs() > 0.05,
        "dragging down tilts the board: {before:?} -> {after:?}"
    );
    Ok(())
}

#[test]
fn a_newcomer_clicks_a_part_in_3d_and_sees_its_properties() -> Result<(), String> {
    let (ctx, mut app, j1) = open_3d()?;
    let (lo, hi) = app
        .view3d
        .body_bounds(j1)
        .ok_or("J1 should have a body in the mesh")?;
    let top = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, hi[2]];
    let (pos, _) = app
        .view3d
        .project_point(app.view3d.rect, top)
        .ok_or("the top of J1 should be in view")?;
    assert!(app.selected != Some(j1), "nothing is selected at first");
    for pressed in [true, false] {
        frame(
            &ctx,
            &mut app,
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
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert_eq!(app.selected, Some(j1), "a click on J1 selects J1");
    assert!(
        paints_text(&ctx, &mut app, "5V input")?,
        "the inspector shows J1's value"
    );
    Ok(())
}

#[test]
fn a_newcomer_picks_a_part_from_the_3d_list() -> Result<(), String> {
    let (ctx, mut app, j1) = open_3d()?;
    let family = app.view3d.body_family(j1).ok_or("J1 should have a body")?;
    authoring::tests::tap(&ctx, &mut app, &format!("J1  {family}"))?;
    assert_eq!(app.selected, Some(j1));
    Ok(())
}

#[test]
fn a_newcomer_picks_a_part_from_the_3d_list_and_resets_the_view() -> Result<(), String> {
    let (ctx, mut app, _j1) = open_3d()?;
    app.selected = None;
    tap_id(&ctx, &mut app, "button:D# chip")?;
    assert!(app.selected.is_some(), "a part row selects that part");
    tap_id(&ctx, &mut app, "button:R# chip")?;
    assert!(app.selected.is_some(), "another row selects another part");
    app.view3d.yaw = 0.3;
    tap_id(&ctx, &mut app, "button:Top view")?;
    assert!(
        (app.view3d.yaw + std::f64::consts::FRAC_PI_2).abs() < 1e-9,
        "Top view looks straight down at the board"
    );
    app.view3d.yaw = 0.9;
    tap_id(&ctx, &mut app, "button:Reset view")?;
    assert!(
        (app.view3d.yaw - 0.9).abs() > 1e-9,
        "Reset view frames the board again"
    );
    Ok(())
}
