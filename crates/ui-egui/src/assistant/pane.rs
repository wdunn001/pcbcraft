//! The Assistant pane body: header, conversation, composer.
//!
//! Context: drawn in the right aside's lower tab; a self-contained function so the dockable
//! workbench can host it as a tab without changes.
//! Rules: 16 px margins, 12 px between groups, every control has a tooltip, the state is a word
//! beside the dot, never colour alone. Nothing here sends a request; it calls `App::assistant_*`.

use super::cards::{self, Pressed};
use super::{Phase, text};
use crate::App;
use pcbcraft_assistant::{Item, StepState};

/// Draw the pane into `ui`.
pub(crate) fn show(app: &mut App, ui: &mut egui::Ui) {
    let muted = app.muted();
    egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(16, 12))
        .show(ui, |ui| {
            header(app, ui);
            if app.assistant.confirm_forget {
                forget_bar(app, ui);
            }
            ui.add_space(12.0);
            if app.assistant.phase() == Phase::Off {
                off_state(app, ui);
            }
            let waiting = app.assistant.waiting();
            // Questions waiting for an answer sit below the conversation so they are never
            // scrolled out of sight.
            let waiting_height = if waiting == 0 {
                0.0
            } else {
                (ui.available_height() - 200.0).clamp(140.0, 600.0)
            };
            let height = (ui.available_height() - 110.0 - waiting_height).max(40.0);
            let mut pressed = None;
            let history = app.session.lock().map_or(0, |s| s.history_len());
            let live = app.assistant.live_undo(history);
            let phase = app.assistant.phase();
            egui::ScrollArea::vertical()
                .max_height(height)
                .stick_to_bottom(true)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let items = &app.assistant.transcript.items;
                    let view = cards::View {
                        muted,
                        undo_live: false,
                        list_all: app.assistant.list_all,
                    };
                    let working = matches!(phase, Phase::Working | Phase::Waiting);
                    let mut n = 0;
                    while n < items.len() {
                        let it = &items[n];
                        if matches!(it, Item::Approval { answer: None, .. }) {
                            n += 1;
                            continue;
                        }
                        if matches!(it, Item::Step { .. }) {
                            // Consecutive steps are one run; a waiting approval does not break it.
                            let mut run: Vec<&Item> = Vec::new();
                            while n < items.len()
                                && matches!(
                                    items[n],
                                    Item::Step { .. }
                                        | Item::ToolCall { .. }
                                        | Item::Approval { answer: None, .. }
                                )
                            {
                                if matches!(items[n], Item::Step { .. }) {
                                    run.push(&items[n]);
                                }
                                n += 1;
                            }
                            let first = match run.first() {
                                Some(Item::Step { id, .. }) => *id,
                                _ => 0,
                            };
                            let last_run = n >= items.len()
                                || !items[n..].iter().any(|i| matches!(i, Item::Step { .. }));
                            let live = working
                                && last_run
                                && run.iter().any(|i| {
                                    matches!(
                                        i,
                                        Item::Step {
                                            state: StepState::Waiting | StepState::Running,
                                            ..
                                        }
                                    )
                                });
                            let open = app.assistant.open_runs.contains(&first);
                            if let Some(p) = cards::run(ui, &run, live, open) {
                                pressed = Some(p);
                            }
                            ui.add_space(8.0);
                            continue;
                        }
                        if let Item::Suggestions(cards_list) = it {
                            let fresh = n + 1 == items.len() && phase == Phase::Ready;
                            if let Some(p) = cards::suggestions(ui, cards_list, fresh) {
                                pressed = Some(p);
                            }
                            n += 1;
                            continue;
                        }
                        let v = cards::View {
                            undo_live: live == Some(n),
                            ..view
                        };
                        if let Some(p) = cards::item(ui, it, &v) {
                            pressed = Some(p);
                        }
                        let quiet = matches!(
                            it,
                            Item::Note { .. } | Item::ToolCall { .. } | Item::Plan(_)
                        );
                        ui.add_space(if quiet { 2.0 } else { 8.0 });
                        n += 1;
                    }
                    // Follow new rows, and the window growing or shrinking around them.
                    let marker = (app.assistant.transcript.items.len(), height as i32);
                    if app.assistant.seen != marker {
                        app.assistant.seen = marker;
                        ui.scroll_to_cursor(Some(egui::Align::BOTTOM));
                    }
                });
            if waiting > 0 {
                egui::ScrollArea::vertical()
                    .id_salt("assistant-waiting")
                    .max_height(waiting_height)
                    .show(ui, |ui| {
                        for it in &app.assistant.transcript.items {
                            if matches!(it, Item::Approval { answer: None, .. })
                                && let Some(p) = cards::item(
                                    ui,
                                    it,
                                    &cards::View {
                                        muted,
                                        undo_live: false,
                                        list_all: app.assistant.list_all,
                                    },
                                )
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
                Some(Pressed::ApplyEach(id)) => app.assistant_answer_each(id),
                Some(Pressed::Undo) => app.assistant_undo(),
                Some(Pressed::ToggleRun(id)) => {
                    if !app.assistant.open_runs.remove(&id) {
                        app.assistant.open_runs.insert(id);
                    }
                }
                Some(Pressed::ToggleList) => app.assistant.list_all = !app.assistant.list_all,
                Some(Pressed::Pick(title)) => app.assistant_pick(title),
                None => {}
            }
            composer(app, ui);
        });
}

/// The empty state: why nothing is connected, and where to fix it.
fn off_state(app: &mut App, ui: &mut egui::Ui) {
    let blocked = pcbcraft_assistant::can_send(
        app.preferences.assistant_enabled,
        !app.preferences.assistant_connections.is_empty(),
        app.assistant.host.http.is_some(),
    );
    ui.label(egui::RichText::new(text::no_model_title()).strong());
    ui.add_space(6.0);
    let body = match blocked {
        Err(pcbcraft_assistant::Blocked::NoHttp) => text::no_http_body(),
        Err(pcbcraft_assistant::Blocked::Disabled) => text::switched_off_body(),
        _ => text::no_model_body(),
    };
    ui.add(egui::Label::new(body).wrap());
    if blocked != Err(pcbcraft_assistant::Blocked::NoHttp)
        && ui
            .button(text::open_settings())
            .on_hover_text(text::open_settings_tip())
            .clicked()
    {
        app.assistant.settings_open = true;
        app.change_tab(crate::Tab::Settings);
    }
    ui.add_space(12.0);
}

/// The state dot: painted, because no bundled font carries a dot glyph. Grey is off or stopped,
/// amber starting, working or waiting, green ready, red after the endpoint failed. The word beside
/// it and the tooltip say the same in text.
fn state_dot(ui: &mut egui::Ui, phase: Phase, failed: bool, word: &str) {
    let t = crate::theme::tokens(ui.ctx());
    let colour = if failed {
        t.red
    } else {
        match phase {
            Phase::Off | Phase::Stopped => t.muted,
            Phase::Starting | Phase::Working | Phase::Waiting => t.gold,
            Phase::Ready => t.mask,
        }
    };
    let (rect, response) = ui.allocate_exact_size(egui::vec2(12.0, 16.0), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), 4.0, colour);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, word));
    response.on_hover_text(format!("{word}. {}", text::state_tip()));
}

fn header(app: &mut App, ui: &mut egui::Ui) {
    let phase = app.assistant.phase();
    let word = match phase {
        Phase::Off => text::state_off(),
        Phase::Starting => text::state_starting(),
        Phase::Ready => text::state_ready(),
        Phase::Working => text::state_working(),
        Phase::Waiting => text::state_waiting(),
        Phase::Stopped => text::state_stopped(),
    };
    ui.horizontal_wrapped(|ui| {
        state_dot(ui, phase, app.assistant.failed, &word);
        ui.label(word).on_hover_text(text::state_tip());
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
                .on_hover_text(text::switch_tip());
            if let Some(id) = chosen {
                app.preferences.assistant_default = id;
                app.assistant_reconnect();
            }
        }
        if ui
            .add_enabled(phase != Phase::Off, egui::Button::new(text::new_chat()))
            .on_hover_text(text::new_chat_tip())
            .clicked()
        {
            app.assistant_new_chat();
        }
        if ui
            .add_enabled(phase != Phase::Off, egui::Button::new(text::clear()))
            .on_hover_text(text::clear_tip())
            .clicked()
        {
            app.assistant.confirm_forget = true;
        }
        if ui
            .add_enabled(
                !app.assistant.transcript.items.is_empty(),
                egui::Button::new(text::export()),
            )
            .on_hover_text(text::export_tip())
            .clicked()
        {
            app.assistant_export();
        }
    });
}

fn composer(app: &mut App, ui: &mut egui::Ui) {
    if !app.assistant.last_note.is_empty() {
        egui::CollapsingHeader::new(text::show_sent())
            .id_salt("assistant-sent")
            .show(ui, |ui| {
                ui.add(
                    egui::Label::new(egui::RichText::new(app.assistant.last_note.as_str()).small())
                        .wrap(),
                );
            })
            .header_response
            .on_hover_text(text::show_sent_tip());
    }
    let off = app.assistant.phase() == Phase::Off;
    let next = matches!(
        app.assistant.transcript.items.last(),
        Some(Item::Suggestions(_))
    );
    let hint = if next {
        text::input_hint_next()
    } else {
        text::input_hint()
    };
    let edit = ui.add_enabled(
        !off,
        egui::TextEdit::multiline(&mut app.assistant.input)
            .desired_rows(2)
            .desired_width(f32::INFINITY)
            .hint_text(&hint),
    );
    use crate::a11y::Named;
    edit.named(&hint).on_hover_text(if off {
        text::disabled_tip()
    } else {
        text::input_tip()
    });
    let send_key = ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Enter));
    ui.horizontal(|ui| {
        ui.checkbox(&mut app.assistant.attach, text::attach())
            .on_hover_text(text::attach_tip());
    });
    ui.horizontal(|ui| {
        let busy = matches!(app.assistant.phase(), Phase::Working | Phase::Waiting);
        if ui
            .add_enabled(!off, egui::Button::new(text::send()))
            .on_hover_text(if off {
                text::disabled_tip()
            } else {
                text::send_tip()
            })
            .clicked()
            || send_key
        {
            app.assistant_send();
        }
        if app.assistant.failed
            && ui
                .button(text::retry())
                .on_hover_text(text::retry_tip())
                .clicked()
        {
            app.assistant_retry();
        }
        if ui
            .add_enabled(busy, egui::Button::new(text::stop()))
            .on_hover_text(text::stop_tip())
            .clicked()
        {
            app.assistant_stop();
        }
    });
}

/// The question before a conversation is deleted. Escape keeps it.
fn forget_bar(app: &mut App, ui: &mut egui::Ui) {
    ui.label(text::forget_question());
    ui.horizontal(|ui| {
        if ui
            .button(text::forget())
            .on_hover_text(text::forget_tip())
            .clicked()
        {
            app.assistant_forget();
            app.assistant.confirm_forget = false;
        }
        let keep = ui.button(text::keep()).on_hover_text(text::keep_tip());
        if keep.clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            app.assistant.confirm_forget = false;
        }
    });
}
