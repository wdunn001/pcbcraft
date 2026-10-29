//! The Assistant section of Settings and the Add connection window.
//!
//! Context: adapted in shape from SolidCraft (MIT OR Apache-2.0), commit 32558d0a,
//! `crates/ui-egui/src/agent_chat/settings.rs`. Called from `settings_tab()` and from
//! `App::assistant_frame`.
//! Rules: a key goes to the keychain through `Host.secrets` and never into the preferences, a
//! log line or the design; the key field shows only for an address the key may be sent to; Test
//! runs on a thread through `Host.http` and shows its outcome as text; every control has a tooltip.
//! Do not: send a request from anywhere but a button the user pressed.

use super::text;
use crate::App;
use pcbcraft_assistant::{
    Connection, HttpFn, Mode, PRESETS, Protocol, anthropic_model_list, key_safe_for, model_list,
    new_id, normalize_url, test_anthropic_endpoint,
};
use std::sync::{Arc, Mutex};

/// A test running or done, shared with its thread.
pub(crate) type SharedTest = Arc<Mutex<TestState>>;

/// Where a test stands.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) enum TestState {
    #[default]
    Idle,
    Running,
    /// The result line and the model ids the server listed.
    Done(Result<(String, Vec<String>), String>),
}

/// The Add connection window.
pub(crate) struct Dialog {
    preset: usize,
    name: String,
    url: String,
    model: String,
    key: String,
    error: String,
    test: SharedTest,
}

impl Dialog {
    fn new() -> Self {
        let mut d = Self {
            preset: 0,
            name: String::new(),
            url: String::new(),
            model: String::new(),
            key: String::new(),
            error: String::new(),
            test: SharedTest::default(),
        };
        d.apply_preset(0);
        d
    }

    fn apply_preset(&mut self, at: usize) {
        if let Some(p) = PRESETS.get(at) {
            self.preset = at;
            self.name = p.name.to_string();
            self.url = p.base_url.to_string();
            self.test = SharedTest::default();
        }
    }

    /// Type an address (tests).
    #[cfg(test)]
    pub(crate) fn set_url(&mut self, url: &str) {
        self.url = url.to_string();
    }

    /// Type a key (tests).
    #[cfg(test)]
    pub(crate) fn set_key(&mut self, key: &str) {
        self.key = key.to_string();
    }

    fn protocol(&self) -> Protocol {
        PRESETS
            .get(self.preset)
            .map_or(Protocol::OpenAi, |p| p.protocol)
    }
}

fn lock(t: &SharedTest) -> std::sync::MutexGuard<'_, TestState> {
    match t.lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
    }
}

/// Start a test on a thread of its own; the outcome lands in `slot`.
fn start_test(
    http: HttpFn,
    protocol: Protocol,
    url: String,
    key: Option<String>,
    model: String,
    slot: SharedTest,
) {
    *lock(&slot) = TestState::Running;
    std::thread::spawn(move || {
        let key = key.as_deref();
        let listed = match protocol {
            Protocol::OpenAi => model_list(&http, &url, key),
            Protocol::Anthropic => anthropic_model_list(&http, &url, key),
        };
        let outcome = match listed {
            Ok(models) => Ok((
                text::models_found(models.len()),
                models.into_iter().map(|m| m.id).collect(),
            )),
            Err(e) if protocol == Protocol::Anthropic && !model.trim().is_empty() => {
                test_anthropic_endpoint(&http, &url, key, &model)
                    .map(|said| (said, Vec::new()))
                    .map_err(|second| if second.is_empty() { e } else { second })
            }
            Err(e) => Err(e),
        };
        *lock(&slot) = TestState::Done(outcome);
    });
}

fn result_line(ui: &mut egui::Ui, t: &TestState, muted: egui::Color32) {
    match t {
        TestState::Idle => {}
        TestState::Running => {
            ui.label(egui::RichText::new(text::TESTING).color(muted));
        }
        TestState::Done(Ok((line, _))) => {
            ui.add(egui::Label::new(line.as_str()).wrap());
        }
        TestState::Done(Err(e)) => {
            ui.add(
                egui::Label::new(egui::RichText::new(e.as_str()).color(egui::Color32::LIGHT_RED))
                    .wrap(),
            );
        }
    }
}

/// The Assistant part of the Settings page.
pub(crate) fn section(app: &mut App, ui: &mut egui::Ui) {
    let muted = app.muted();
    ui.label(
        egui::RichText::new(text::SETTINGS_HEADING)
            .size(10.0)
            .color(muted),
    );
    ui.checkbox(&mut app.preferences.assistant_enabled, text::SHOW)
        .on_hover_text(text::SHOW_TIP);
    if !app.preferences.assistant_enabled {
        return;
    }
    ui.add_space(8.0);
    ui.label(egui::RichText::new(text::CONNECTIONS).strong());
    connections(app, ui, muted);
    if ui
        .button(text::ADD_CONNECTION)
        .on_hover_text(text::ADD_CONNECTION_TIP)
        .clicked()
    {
        app.assistant.dialog = Some(Dialog::new());
    }
    ui.add_space(12.0);
    ui.label(egui::RichText::new(text::PERMISSIONS).strong());
    for mode in Mode::ALL {
        let tip = match mode {
            Mode::Ask => text::MODE_ASK_TIP,
            Mode::Edits => text::MODE_EDITS_TIP,
            Mode::All => text::MODE_ALL_TIP,
        };
        ui.radio_value(&mut app.preferences.assistant_mode, mode, mode.label())
            .on_hover_text(tip);
    }
    if app.preferences.assistant_mode == Mode::All {
        ui.add(
            egui::Label::new(
                egui::RichText::new(text::MODE_ALL_WARNING)
                    .small()
                    .color(muted),
            )
            .wrap(),
        );
    }
    ui.add_space(12.0);
    ui.horizontal(|ui| {
        ui.label(text::LIMIT);
        ui.add(egui::DragValue::new(&mut app.preferences.assistant_turn_limit).range(1..=200))
            .on_hover_text(text::LIMIT_TIP);
    });
    if ui
        .button(text::RESET_ALWAYS)
        .on_hover_text(text::RESET_ALWAYS_TIP)
        .clicked()
    {
        app.preferences.assistant_always_allow.clear();
    }
}

fn connections(app: &mut App, ui: &mut egui::Ui, muted: egui::Color32) {
    if app.preferences.assistant_connections.is_empty() {
        ui.add(egui::Label::new(egui::RichText::new(text::NO_CONNECTIONS).color(muted)).wrap());
        return;
    }
    let rows = app.preferences.assistant_connections.clone();
    let mut remove = None;
    let mut make_default = None;
    let mut test = None;
    for c in &rows {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_width(ui.available_width());
            let key = if c.has_key {
                text::KEY_STORED
            } else {
                text::NO_KEY
            };
            ui.add(egui::Label::new(egui::RichText::new(&c.name).strong()).wrap());
            ui.add(
                egui::Label::new(
                    egui::RichText::new(format!("{} · {} · {key}", c.base_url, c.model))
                        .small()
                        .color(muted),
                )
                .wrap(),
            );
            ui.horizontal_wrapped(|ui| {
                let current = app.preferences.assistant_default == c.id;
                if ui
                    .add_enabled(
                        !current,
                        egui::Button::new(if current {
                            text::IN_USE
                        } else {
                            text::MAKE_DEFAULT
                        }),
                    )
                    .on_hover_text(if current {
                        text::IN_USE_TIP
                    } else {
                        text::MAKE_DEFAULT_TIP
                    })
                    .clicked()
                {
                    make_default = Some(c.id.clone());
                }
                if ui
                    .add_enabled(
                        app.assistant.host.http.is_some(),
                        egui::Button::new(text::TEST),
                    )
                    .on_hover_text(text::TEST_TIP)
                    .clicked()
                {
                    test = Some(c.clone());
                }
                if ui
                    .button(text::REMOVE)
                    .on_hover_text(text::REMOVE_TIP)
                    .clicked()
                {
                    remove = Some(c.id.clone());
                }
            });
            let mut images = c.images;
            if ui
                .checkbox(&mut images, text::TAKES_PICTURES)
                .on_hover_text(text::TAKES_PICTURES_TIP)
                .changed()
                && let Some(row) = app
                    .preferences
                    .assistant_connections
                    .iter_mut()
                    .find(|x| x.id == c.id)
            {
                row.images = images;
            }
            let current: Mode = app
                .preferences
                .assistant_modes
                .get(&c.id)
                .copied()
                .unwrap_or(app.preferences.assistant_mode);
            let overridden = app.preferences.assistant_modes.contains_key(&c.id);
            egui::ComboBox::from_id_salt(("assistant-mode", &c.id))
                .selected_text(if overridden {
                    current.label()
                } else {
                    text::SAME_AS_DEFAULT
                })
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_label(!overridden, text::SAME_AS_DEFAULT)
                        .clicked()
                    {
                        app.preferences.assistant_modes.remove(&c.id);
                    }
                    for mode in Mode::ALL {
                        if ui
                            .selectable_label(overridden && current == mode, mode.label())
                            .clicked()
                        {
                            app.preferences.assistant_modes.insert(c.id.clone(), mode);
                        }
                    }
                })
                .response
                .on_hover_text(text::MODE_OVERRIDE_TIP);
            if let Some((id, slot)) = &app.assistant.row_test
                && *id == c.id
            {
                result_line(ui, &lock(slot), muted);
            }
        });
        ui.add_space(4.0);
    }
    if let Some(id) = make_default {
        app.preferences.assistant_default = id;
        app.assistant_reconnect();
    }
    if let Some(c) = test
        && let Some(http) = app.assistant.host.http.clone()
    {
        let key = stored_key(app, &c);
        let slot = SharedTest::default();
        start_test(
            http,
            c.protocol,
            c.base_url.clone(),
            key,
            c.model.clone(),
            slot.clone(),
        );
        app.assistant.row_test = Some((c.id, slot));
    }
    if let Some(id) = remove {
        remove_connection(app, &id);
    }
}

fn stored_key(app: &App, c: &Connection) -> Option<String> {
    if !c.has_key {
        return None;
    }
    app.assistant
        .host
        .secrets
        .as_ref()
        .and_then(|s| s.get(&pcbcraft_assistant::key_id(&c.id)).ok().flatten())
}

/// Forget a connection and delete its keychain entry.
pub(crate) fn remove_connection(app: &mut App, id: &str) {
    if let Some(secrets) = &app.assistant.host.secrets {
        let _ = secrets.delete(&pcbcraft_assistant::key_id(id));
    }
    app.preferences.assistant_connections.retain(|c| c.id != id);
    app.preferences.assistant_modes.remove(id);
    app.preferences.assistant_always_allow.remove(id);
    if app.preferences.assistant_default == id {
        app.preferences.assistant_default = app
            .preferences
            .assistant_connections
            .first()
            .map(|c| c.id.clone())
            .unwrap_or_default();
        app.assistant_reconnect();
    }
}

/// The Add connection window, when it is open.
pub(crate) fn dialog(app: &mut App, ctx: &egui::Context) {
    let Some(mut d) = app.assistant.dialog.take() else {
        return;
    };
    let muted = app.muted();
    let mut open = true;
    let mut add = false;
    let mut cancel = false;
    egui::Window::new(text::DIALOG_TITLE)
        .open(&mut open)
        .collapsible(false)
        .default_width(360.0)
        .show(ctx, |ui| {
            egui::Frame::new()
                .inner_margin(egui::Margin::same(12))
                .show(ui, |ui| {
                    let before = d.preset;
                    egui::ComboBox::from_label(text::PRESET)
                        .selected_text(PRESETS.get(d.preset).map_or("", |p| p.name))
                        .show_ui(ui, |ui| {
                            for (i, p) in PRESETS.iter().enumerate() {
                                ui.selectable_value(&mut d.preset, i, p.name);
                            }
                        })
                        .response
                        .on_hover_text(text::PRESET_TIP);
                    if d.preset != before {
                        d.apply_preset(d.preset);
                    }
                    ui.add_space(8.0);
                    ui.label(text::NAME);
                    ui.text_edit_singleline(&mut d.name)
                        .on_hover_text(text::NAME_TIP);
                    ui.label(text::URL);
                    ui.text_edit_singleline(&mut d.url)
                        .on_hover_text(text::URL_TIP);
                    ui.label(text::MODEL);
                    let listed = match &*lock(&d.test) {
                        TestState::Done(Ok((_, ids))) => ids.clone(),
                        _ => Vec::new(),
                    };
                    if listed.is_empty() {
                        ui.text_edit_singleline(&mut d.model)
                            .on_hover_text(text::MODEL_TIP);
                    } else {
                        egui::ComboBox::from_id_salt("assistant-model")
                            .selected_text(&d.model)
                            .show_ui(ui, |ui| {
                                for id in &listed {
                                    ui.selectable_value(&mut d.model, id.clone(), id);
                                }
                            })
                            .response
                            .on_hover_text(text::MODEL_TIP);
                    }
                    key_field(app, ui, &mut d, muted);
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(
                                app.assistant.host.http.is_some(),
                                egui::Button::new(text::TEST),
                            )
                            .on_hover_text(text::TEST_TIP)
                            .clicked()
                            && let Some(http) = app.assistant.host.http.clone()
                        {
                            match normalize_url(&d.url) {
                                Ok(url) => {
                                    d.error.clear();
                                    let key = (!d.key.is_empty() && key_safe_for(&url))
                                        .then(|| d.key.clone());
                                    start_test(
                                        http,
                                        d.protocol(),
                                        url,
                                        key,
                                        d.model.clone(),
                                        d.test.clone(),
                                    );
                                }
                                Err(e) => d.error = e,
                            }
                        }
                        if ui.button(text::ADD).on_hover_text(text::ADD_TIP).clicked() {
                            add = true;
                        }
                        if ui
                            .button(text::CANCEL)
                            .on_hover_text(text::CANCEL_TIP)
                            .clicked()
                        {
                            cancel = true;
                        }
                    });
                    result_line(ui, &lock(&d.test), muted);
                    if !d.error.is_empty() {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(d.error.as_str())
                                    .color(egui::Color32::LIGHT_RED),
                            )
                            .wrap(),
                        );
                    }
                });
        });
    if add {
        match finish(app, &d) {
            Ok(()) => return,
            Err(e) => d.error = e,
        }
    }
    if open && !cancel {
        app.assistant.dialog = Some(d);
    }
}

fn key_field(app: &App, ui: &mut egui::Ui, d: &mut Dialog, muted: egui::Color32) {
    if app.assistant.host.secrets.is_none() {
        ui.add(
            egui::Label::new(
                egui::RichText::new(text::KEY_NEEDS_DESKTOP)
                    .small()
                    .color(muted),
            )
            .wrap(),
        );
    } else if key_safe_for(&d.url) {
        ui.label(text::KEY);
        ui.add(egui::TextEdit::singleline(&mut d.key).password(true))
            .on_hover_text(text::KEY_TIP);
    } else {
        d.key.clear();
        ui.add(
            egui::Label::new(egui::RichText::new(text::KEY_REFUSED).small().color(muted)).wrap(),
        );
    }
}

/// Save the dialog's connection; the key goes to the keychain and nowhere else.
fn finish(app: &mut App, d: &Dialog) -> Result<(), String> {
    let url = normalize_url(&d.url)?;
    if d.name.trim().is_empty() {
        return Err("Give the connection a name.".into());
    }
    let id = new_id(&app.preferences.assistant_connections, "api");
    let mut has_key = false;
    if !d.key.is_empty() {
        if !key_safe_for(&url) {
            return Err(text::KEY_REFUSED.into());
        }
        let secrets = app
            .assistant
            .host
            .secrets
            .clone()
            .ok_or(text::KEY_NEEDS_DESKTOP)?;
        secrets.set(&pcbcraft_assistant::key_id(&id), &d.key)?;
        has_key = true;
    }
    app.preferences.assistant_connections.push(Connection {
        id: id.clone(),
        name: d.name.trim().to_string(),
        protocol: d.protocol(),
        base_url: url,
        model: d.model.trim().to_string(),
        has_key,
        images: false,
    });
    if app.preferences.assistant_default.is_empty()
        || !app
            .preferences
            .assistant_connections
            .iter()
            .any(|c| c.id == app.preferences.assistant_default)
    {
        app.preferences.assistant_default = id;
    }
    app.assistant_reconnect();
    Ok(())
}
