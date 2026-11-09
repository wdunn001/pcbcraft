use super::super::*;
use super::support::frame;
use crate::keymap::KeyAction;
use egui::Key;
use serde_json::json;

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
    app.open_preferences(crate::preferences::TAB_KEYS);
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    // Search for the row, so its Change button is the only one on the list.
    super::support::type_into(&ctx, &mut app, "textinput:Search shortcuts", "Rotate")?;
    super::support::tap_id(&ctx, &mut app, "button:Change")?;
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
        !app.prefs_ui.open,
        "Preferences should be closed after Done"
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
