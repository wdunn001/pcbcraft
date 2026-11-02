use super::super::*;
use super::support::{frame, scrolled_to};
use crate::keymap::KeyAction;
use egui::Key;
use serde_json::json;

/// Where the last copy of `label` was painted, if it is on screen.
fn painted_at(output: &egui::FullOutput, label: &str) -> Option<egui::Pos2> {
    let mut found = None;
    for shape in &output.shapes {
        if let egui::Shape::Text(text) = &shape.shape
            && text.galley.text() == label
        {
            found = Some(text.pos + text.galley.size() / 2.0);
        }
    }
    found
}

/// Find the "Change" button that sits on the same row as the given chord label.
fn change_button_for_chord(
    ctx: &egui::Context,
    app: &App,
    output: &egui::FullOutput,
    chord: &str,
) -> Result<egui::Pos2, String> {
    let chord_pos = painted_at(output, chord)
        .ok_or_else(|| format!("Chord label '{}' is not on screen", chord))?;
    let all = super::audit::controls(ctx, app);
    let changes: Vec<_> = all
        .iter()
        .filter(|c| {
            c.id.as_deref() == Some("button:Change")
                || c.id.as_deref() == Some("button:Press keys…")
        })
        .collect();
    changes
        .into_iter()
        .min_by(|a, b| {
            let da = (a.rect.center().y - chord_pos.y).abs();
            let db = (b.rect.center().y - chord_pos.y).abs();
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
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
    // Scroll to make the Rotate row visible
    scrolled_to(&ctx, &mut app, "Rotate the hovered")?;
    let output = frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let change_pos = change_button_for_chord(&ctx, &app, &output, "R")?;
    super::support::tap_at(&ctx, &mut app, change_pos)?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert_eq!(
        app.binding_capture,
        Some(KeyAction::Rotate),
        "Clicking Change on the R row should start capture for Rotate"
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
