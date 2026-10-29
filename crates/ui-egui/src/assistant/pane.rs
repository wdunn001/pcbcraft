//! The Assistant pane body: header, conversation, composer.
//!
//! Context: drawn in the right aside's lower tab; a self-contained function so the dockable
//! workbench can host it as a tab without changes.
//! Rules: 16 px margins, 12 px between groups, every control has a tooltip, the state is a word
//! beside the dot, never colour alone. Nothing here sends a request; it calls `App::assistant_*`.

use super::cards::{self, Pressed};
use super::{Phase, text};
use crate::App;
use pcbcraft_assistant::Item;

/// Draw the pane into `ui`.
pub(crate) fn show(app: &mut App, ui: &mut egui::Ui) {
    let muted = app.muted();
    egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(16, 12))
        .show(ui, |ui| {
            header(app, ui, muted);
            ui.add_space(12.0);
            if app.assistant.phase() == Phase::Off {
                ui.label(egui::RichText::new(text::NO_MODEL_TITLE).strong());
                ui.add_space(6.0);
                let body = if app.preferences.assistant_connections.is_empty() {
                    text::NO_MODEL_BODY
                } else if app.assistant.host.http.is_none() {
                    text::NO_HTTP_BODY
                } else {
                    text::NO_MODEL_BODY
                };
                ui.add(egui::Label::new(body).wrap());
                return;
            }
            let waiting = app.assistant.waiting();
            // Questions waiting for an answer sit below the conversation so they are never
            // scrolled out of sight.
            let waiting_height = (waiting as f32 * 140.0).min(280.0);
            let height = (ui.available_height() - 110.0 - waiting_height).max(40.0);
            let mut pressed = None;
            let history = app.session.lock().map_or(0, |s| s.history_len());
            let live = app.assistant.live_undo(history);
            egui::ScrollArea::vertical()
                .max_height(height)
                .stick_to_bottom(true)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for (n, it) in app.assistant.transcript.items.iter().enumerate() {
                        if matches!(it, Item::Approval { answer: None, .. }) {
                            continue;
                        }
                        if let Some(p) = cards::item(ui, it, muted, live == Some(n)) {
                            pressed = Some(p);
                        }
                        let quiet = matches!(
                            it,
                            Item::Note { .. } | Item::ToolCall { .. } | Item::Plan(_)
                        );
                        ui.add_space(if quiet { 2.0 } else { 8.0 });
                    }
                });
            if waiting > 0 {
                egui::ScrollArea::vertical()
                    .id_salt("assistant-waiting")
                    .max_height(waiting_height)
                    .show(ui, |ui| {
                        for it in &app.assistant.transcript.items {
                            if matches!(it, Item::Approval { answer: None, .. })
                                && let Some(p) = cards::item(ui, it, muted, false)
                            {
                                pressed = Some(p);
                            }
                        }
                    });
            }
            match pressed {
                Some(Pressed::Allow(id)) => app.assistant_answer(id, true, false),
                Some(Pressed::AlwaysAllow(id)) => app.assistant_answer(id, true, true),
                Some(Pressed::Deny(id)) => app.assistant_answer(id, false, false),
                Some(Pressed::Undo) => app.assistant_undo(),
                None => {}
            }
            composer(app, ui);
        });
}

fn header(app: &mut App, ui: &mut egui::Ui, muted: egui::Color32) {
    let phase = app.assistant.phase();
    let (word, dot) = match phase {
        Phase::Off => (text::STATE_OFF, muted),
        Phase::Starting => (text::STATE_STARTING, egui::Color32::GRAY),
        Phase::Ready => (text::STATE_READY, egui::Color32::from_rgb(70, 180, 110)),
        Phase::Working => (text::STATE_WORKING, egui::Color32::from_rgb(230, 170, 60)),
        Phase::Waiting => (text::STATE_WAITING, egui::Color32::from_rgb(230, 170, 60)),
    };
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new("●").color(dot))
            .on_hover_text(text::STATE_TIP);
        ui.label(word).on_hover_text(text::STATE_TIP);
        if app.preferences.assistant_connections.len() > 1 {
            let current = app
                .preferences
                .assistant_connections
                .iter()
                .find(|c| c.id == app.preferences.assistant_default)
                .map_or_else(String::new, |c| c.name.clone());
            let mut chosen = None;
            egui::ComboBox::from_id_salt("assistant-connection")
                .selected_text(current)
                .show_ui(ui, |ui| {
                    for c in &app.preferences.assistant_connections {
                        if ui
                            .selectable_label(c.id == app.preferences.assistant_default, &c.name)
                            .clicked()
                        {
                            chosen = Some(c.id.clone());
                        }
                    }
                })
                .response
                .on_hover_text(text::SWITCH_TIP);
            if let Some(id) = chosen {
                app.preferences.assistant_default = id;
                app.assistant_reconnect();
            }
        }
        if ui
            .add_enabled(phase != Phase::Off, egui::Button::new(text::NEW_CHAT))
            .on_hover_text(text::NEW_CHAT_TIP)
            .clicked()
        {
            app.assistant_new_chat();
        }
        if ui
            .add_enabled(phase != Phase::Off, egui::Button::new(text::CLEAR))
            .on_hover_text(text::CLEAR_TIP)
            .clicked()
        {
            app.assistant
                .transcript
                .items
                .retain(|i| matches!(i, Item::Approval { answer: None, .. }));
        }
    });
}

fn composer(app: &mut App, ui: &mut egui::Ui) {
    if !app.assistant.last_note.is_empty() {
        egui::CollapsingHeader::new(text::SHOW_SENT)
            .id_salt("assistant-sent")
            .show(ui, |ui| {
                ui.add(
                    egui::Label::new(egui::RichText::new(app.assistant.last_note.as_str()).small())
                        .wrap(),
                );
            })
            .header_response
            .on_hover_text(text::SHOW_SENT_TIP);
    }
    let edit = ui.add(
        egui::TextEdit::multiline(&mut app.assistant.input)
            .desired_rows(2)
            .desired_width(f32::INFINITY)
            .hint_text(text::INPUT_HINT),
    );
    edit.on_hover_text(text::INPUT_TIP);
    let send_key = ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Enter));
    ui.horizontal(|ui| {
        ui.checkbox(&mut app.assistant.attach, text::ATTACH)
            .on_hover_text(text::ATTACH_TIP);
    });
    ui.horizontal(|ui| {
        let busy = matches!(app.assistant.phase(), Phase::Working | Phase::Waiting);
        if ui
            .button(text::SEND)
            .on_hover_text(text::SEND_TIP)
            .clicked()
            || send_key
        {
            app.assistant_send();
        }
        if ui
            .add_enabled(busy, egui::Button::new(text::STOP))
            .on_hover_text(text::STOP_TIP)
            .clicked()
        {
            app.assistant_stop();
        }
    });
}
