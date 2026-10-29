use super::super::*;
use super::support::*;

fn fresh() -> App {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app
}

#[test]
fn a_newcomer_visits_settings_and_comes_back_to_the_same_zoom() -> Result<(), String> {
    let mut app = fresh();
    let ctx = egui::Context::default();
    app.change_tab(Tab::Pcb);
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.pcb.zoom = 33.0;
    for tab in [Tab::Settings, Tab::Rules, Tab::Checks, Tab::Pcb] {
        app.change_tab(tab);
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    assert_eq!(app.pcb.zoom, 33.0, "the board stays as the user left it");
    Ok(())
}

#[test]
fn a_newcomer_reads_settings_with_room_around_the_text() -> Result<(), String> {
    let mut app = fresh();
    let ctx = egui::Context::default();
    for tab in [Tab::Settings, Tab::Rules, Tab::Checks] {
        app.change_tab(tab);
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        let output = frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        let left = app.page_left;
        let mut tight = Vec::new();
        for shape in &output.shapes {
            if let egui::Shape::Text(text) = &shape.shape
                && text.pos.x > left
                && text.pos.x < left + 16.0
                && text.pos.y > 80.0
            {
                tight.push(text.galley.text().to_owned());
            }
        }
        assert!(tight.is_empty(), "{tab:?} text hugs the edge: {tight:?}");
    }
    Ok(())
}

#[test]
fn a_newcomer_finds_appearance_on_the_settings_page() -> Result<(), String> {
    let mut app = fresh();
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.change_tab(Tab::Settings);
    assert!(
        paints_text(&ctx, &mut app, "APPEARANCE")?,
        "appearance lives on the Settings page"
    );
    app.change_tab(Tab::Pcb);
    assert!(
        !paints_text(&ctx, &mut app, "APPEARANCE")?,
        "the inspector no longer shows appearance"
    );
    Ok(())
}

/// Turn the wheel once over the board and report the zoom before and after.
fn zoom_after_one_wheel_frame(reduced: bool) -> (f32, f32) {
    let mut app = fresh();
    app.preferences.reduced_motion = reduced;
    app.tool_call("sample_design", json!({}));
    let ctx = egui::Context::default();
    app.change_tab(Tab::Pcb);
    let at = egui::pos2(700.0, 450.0);
    frame(
        &ctx,
        &mut app,
        vec![egui::Event::PointerMoved(at)],
        egui::Modifiers::NONE,
    );
    let before = app.pcb.zoom;
    frame(
        &ctx,
        &mut app,
        vec![egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, 100.0),
            phase: egui::TouchPhase::Move,
            modifiers: egui::Modifiers::NONE,
        }],
        egui::Modifiers::NONE,
    );
    (before, app.pcb.zoom)
}

#[test]
fn a_newcomer_turns_off_animation_in_settings() -> Result<(), String> {
    let mut app = fresh();
    let ctx = egui::Context::default();
    app.change_tab(Tab::Settings);
    assert!(
        scrolled_to_at(&ctx, &mut app, "Reduce motion", 700.0)?,
        "Settings should offer a Reduce motion choice"
    );
    assert!(!app.preferences.reduced_motion, "off until the person asks");
    authoring::tests::tap(&ctx, &mut app, "Reduce motion")?;
    assert!(
        app.preferences.reduced_motion,
        "ticking the box turns it on"
    );
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert_eq!(
        ctx.global_style().animation_time,
        0.0,
        "fades and collapses finish in the frame they start"
    );
    // A zoom step reaches its final value in one frame when motion is reduced.
    let (before, after) = zoom_after_one_wheel_frame(true);
    let target = (before * (100.0_f32 * 0.003).exp()).clamp(0.02, 100.0);
    assert!(
        (after - target).abs() < target * 1e-3,
        "one frame should reach {target}, got {after} from {before}"
    );
    // With motion on, the same wheel notch has not finished in one frame.
    let (before_on, after_on) = zoom_after_one_wheel_frame(false);
    assert!(
        (after_on - before_on).abs() < (after - before).abs(),
        "gliding zoom should lag the instant one: {before_on} to {after_on}"
    );
    Ok(())
}
