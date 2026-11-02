use super::super::*;
use super::support::frame;
use crate::keymap::KeyAction;
use egui::Key;
use serde_json::json;

/// Where the last copy of `label` was painted, if it is on screen.
fn painted_at(output: &egui::FullOutput, label: &str) -> Option<egui::Rect> {
    let mut found = None;
    for shape in &output.shapes {
        if let egui::Shape::Text(text) = &shape.shape
            && text.galley.text().contains(label)
        {
            found = Some(egui::Rect::from_min_size(text.pos, text.galley.size()));
        }
    }
    found
}

/// True when `label` is painted inside the visible part of its scroll area.
fn visible_row(output: &egui::FullOutput, label: &str) -> bool {
    output.shapes.iter().any(|shape| {
        if let egui::Shape::Text(text) = &shape.shape
            && text.galley.text().contains(label)
        {
            // Well inside the clip, not on its edge, so a click lands.
            return shape
                .clip_rect
                .shrink(24.0)
                .contains(text.pos + text.galley.size() / 2.0);
        }
        false
    })
}

/// Find the "Change" button that sits on the same row as the given action label.
fn change_button_for_row(
    ctx: &egui::Context,
    app: &App,
    output: &egui::FullOutput,
    label: &str,
) -> Result<egui::Pos2, String> {
    let chord_pos =
        painted_at(output, label).ok_or_else(|| format!("the row for {label} is not on screen"))?;
    let all = super::audit::controls(ctx, app);
    let changes: Vec<_> = all
        .iter()
        .filter(|c| {
            c.id.as_deref() == Some("button:Change")
                || c.id.as_deref() == Some("button:Press keys…")
        })
        .collect();
    // The button on the label's row is the one whose rectangle shares the
    // label's height; the nearest centre can belong to a neighbouring row.
    let overlap = |c: &&super::audit::Control| {
        (c.rect.max.y.min(chord_pos.max.y) - c.rect.min.y.max(chord_pos.min.y)).max(0.0)
    };
    changes
        .into_iter()
        .filter(|c| overlap(c) > 0.0)
        .max_by(|a, b| {
            overlap(a)
                .partial_cmp(&overlap(b))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|c| c.rect.center())
        .ok_or_else(|| "No Change buttons found on the shortcuts list".to_owned())
}

#[test]
fn a_newcomer_rebinds_rotate_to_another_key() -> Result<(), String> {
    let session =
        pcbcraft_mcp::shared(pcbcraft_engine::Session::sample().map_err(|e| e.to_string())?);
    let mut app = App::create(session, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    app.change_tab(Tab::Pcb);
    app.select(Some(11));
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.change_tab(Tab::Settings);
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    super::support::tap_id(&ctx, &mut app, "button:Edit shortcuts…")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    // The Rotate row is below the fold of the list. Scroll in tiny steps, one
    // frame each, so the row cannot glide past before the click lands.
    let label = "Rotate the hovered or selected object by 90 degrees";
    let mut output = frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    for _ in 0..600 {
        if visible_row(&output, label) {
            break;
        }
        let wheel = egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, -4.0),
            phase: egui::TouchPhase::Move,
            modifiers: egui::Modifiers::NONE,
        };
        output = frame(
            &ctx,
            &mut app,
            vec![egui::Event::PointerMoved(egui::pos2(700.0, 500.0)), wheel],
            egui::Modifiers::NONE,
        );
    }
    assert!(
        visible_row(&output, label),
        "the Rotate row never scrolled into view"
    );
    let change_pos = change_button_for_row(&ctx, &app, &output, label)?;
    super::support::tap_at(&ctx, &mut app, change_pos)?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert_eq!(
        app.binding_capture,
        Some(KeyAction::Rotate),
        "Clicking Change on the Rotate row should start capture for Rotate"
    );
    let key = |key, modifiers| egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    };
    frame(
        &ctx,
        &mut app,
        vec![key(Key::T, egui::Modifiers::NONE)],
        egui::Modifiers::NONE,
    );
    assert!(
        app.binding_capture.is_none(),
        "Capture should end after a key press"
    );
    // The person lets go of the key, as a hand does.
    frame(
        &ctx,
        &mut app,
        vec![egui::Event::Key {
            key: Key::T,
            physical_key: None,
            pressed: false,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
        egui::Modifiers::NONE,
    );
    super::support::tap_id(&ctx, &mut app, "button:Done")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        !app.shortcuts_settings_open,
        "The shortcuts panel should be closed after Done"
    );
    frame(
        &ctx,
        &mut app,
        vec![key(Key::T, egui::Modifiers::NONE)],
        egui::Modifiers::NONE,
    );
    assert_eq!(
        app.project.footprint(11).map(|f| f.rotation),
        Some(90.0),
        "T should rotate the footprint: {}",
        app.message
    );
    frame(
        &ctx,
        &mut app,
        vec![key(Key::R, egui::Modifiers::NONE)],
        egui::Modifiers::NONE,
    );
    assert_eq!(
        app.project.footprint(11).map(|f| f.rotation),
        Some(90.0),
        "R should no longer rotate after the rebind: {}",
        app.message
    );
    Ok(())
}

#[test]
fn a_newcomer_turns_automatic_board_sync_off_from_the_keyboard() -> Result<(), String> {
    let session = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(session, Preferences::default(), None, None);
    app.refresh();
    assert!(app.project.auto_sync, "new designs start with sync on");
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    frame(
        &ctx,
        &mut app,
        vec![egui::Event::Key {
            key: Key::B,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers {
                ctrl: true,
                shift: true,
                ..Default::default()
            },
        }],
        egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
    );
    app.refresh();
    assert!(
        !app.project.auto_sync,
        "Ctrl+Shift+B should turn automatic sync off: {}",
        app.message
    );
    Ok(())
}
