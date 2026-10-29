use super::super::*;
use crate::keymap::{KeyAction, KeyChord};
use crate::test_support::{frame, tap};
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
    tap(&ctx, &mut app, "Settings")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    tap(&ctx, &mut app, "Edit shortcuts…")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.binding_capture = Some(KeyAction::Rotate);
    let t = KeyChord {
        key: Key::T,
        ctrl: false,
        shift: false,
        alt: false,
    };
    app.preferences
        .active_keymap_mut()
        .set_binding(KeyAction::Rotate, t)
        .map_err(|e| e.to_string())?;
    app.binding_capture = None;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
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
