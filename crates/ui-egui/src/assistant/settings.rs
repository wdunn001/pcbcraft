//! The Assistant page of Preferences: the connection list, the Add connection form and the
//! behaviour card.
//!
//! Context: adapted in shape from SolidCraft (MIT OR Apache-2.0), commit 32558d0a,
//! `crates/ui-egui/src/agent_chat/settings.rs`. Called from `preferences/mod.rs` and
//! `App::assistant_frame`.
//! Rules: a key goes to the key store of the shell through `Host.keys` and never into the preferences, a
//! log line or the design; the key field shows only for an address the key may be sent to and is
//! never shown in clear text; Test runs on a thread through `Host.http` and shows its outcome as
//! text; every control has a tooltip.
//! Do not: send a request from anywhere but a button the user pressed.

use super::text;
use crate::a11y::Named;
use crate::ui_kit::button::{self, Size};
use crate::ui_kit::card::card;
use crate::ui_kit::checkbox::checkbox;
use crate::ui_kit::chip::{BadgeKind, badge};
use crate::ui_kit::form_row::{FormWidth, form_row};
use crate::ui_kit::icons;
use crate::ui_kit::number_field::{NumberSpec, number_field};
use crate::ui_kit::segmented::{Segment, SegmentedStyle, segmented_styled};
use crate::ui_kit::select::select;
use crate::ui_kit::table::{self, Column};
use crate::{App, fonts, theme};
use pcbcraft_assistant::{
    Connection, HttpFn, KeptIn, Mode, PRESETS, Protocol, ProviderKind, anthropic_model_list,
    key_safe_for, model_list, new_id, normalize_url, test_anthropic_endpoint,
};
use pcbcraft_text::t;
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
    kind: ProviderKind,
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
            kind: ProviderKind::Cloud,
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

fn result_line(ui: &mut egui::Ui, state: &TestState) {
    let t = theme::tokens(ui.ctx());
    match state {
        TestState::Idle => {}
        TestState::Running => {
            ui.label(fonts::text(fonts::Role::Body, text::testing()).color(t.muted));
        }
        TestState::Done(Ok((line, _))) => {
            ui.label(fonts::text(fonts::Role::Body, line.as_str()).color(t.mask));
        }
        TestState::Done(Err(e)) => {
            ui.label(fonts::text(fonts::Role::Body, e.as_str()).color(t.red));
        }
    }
}

/// Rows of the connection table: name, kind, default mark, actions.
fn columns<'a>(name: &'a str, kind: &'a str, state: &'a str) -> [Column<'a>; 4] {
    [
        Column {
            title: name,
            width: 220.0,
        },
        Column {
            title: kind,
            width: 300.0,
        },
        Column {
            title: state,
            width: 100.0,
        },
        Column {
            title: "",
            width: 0.0,
        },
    ]
}

/// The Connections card: the saved connections and the Add connection button.
///
/// # Rules
/// - Edit opens the details of one row under it; the key is never shown, only whether one is kept.
///
/// # Dependencies
/// - crates/ui-egui/src/ui_kit/table.rs
///
/// # Do not
/// - Show a stored key, in full or in part.
pub(crate) fn connections_card(app: &mut App, ui: &mut egui::Ui) {
    let t = theme::tokens(ui.ctx());
    card(
        ui,
        &text::connections(),
        None,
        |_| {},
        |ui| {
            if app.preferences.assistant_connections.is_empty() {
                ui.label(fonts::text(fonts::Role::Body, text::no_connections()).color(t.muted));
            } else {
                connection_rows(app, ui);
            }
            ui.add_space(theme::SPACE[0]);
            ui.horizontal(|ui| {
                if button::secondary_button(
                    ui,
                    Some(icons::PLUS),
                    &text::add_connection(),
                    &text::add_connection_tip(),
                    Size::Normal,
                    app.assistant.dialog.is_none(),
                )
                .clicked()
                {
                    app.assistant.dialog = Some(Dialog::new());
                }
            });
            let kept = match app.assistant.host.keys.as_ref().map(|k| k.kept_in()) {
                Some(KeptIn::SystemKeychain) => t!("prefs.assistant.keys_note.keychain"),
                Some(KeptIn::BrowserStorage) => t!("prefs.assistant.keys_note.browser"),
                _ => t!("prefs.assistant.keys_note.memory"),
            };
            ui.label(fonts::caption(kept).color(t.muted));
        },
    );
}

fn connection_rows(app: &mut App, ui: &mut egui::Ui) {
    let t = theme::tokens(ui.ctx());
    let rows = app.preferences.assistant_connections.clone();
    let heads = (
        t!("prefs.assistant.col.name"),
        t!("prefs.assistant.col.kind"),
        t!("prefs.assistant.col.state"),
    );
    let cols = columns(&heads.0, &heads.1, &heads.2);
    table::head(ui, &cols);
    let mut remove = None;
    let mut make_default = None;
    let mut test = None;
    let mut clear_key = None;
    for c in &rows {
        let open = app.prefs_ui.editing.as_deref() == Some(c.id.as_str());
        let current = app.preferences.assistant_default == c.id;
        table::row(ui, &cols, |cells| {
            cells.cell(|ui| {
                ui.label(fonts::strong(&c.name).color(t.text));
            });
            cells.cell(|ui| {
                let key = if c.has_key {
                    text::key_stored()
                } else {
                    text::no_key()
                };
                let model = if c.model.is_empty() {
                    protocol_name(c.protocol)
                } else {
                    c.model.clone()
                };
                ui.add(
                    egui::Label::new(fonts::mono(format!("{model} · {key}")).color(t.muted))
                        .truncate(),
                );
            });
            cells.cell(|ui| {
                if current {
                    badge(ui, &text::in_use(), BadgeKind::Success, &text::in_use_tip());
                }
            });
            cells.cell(|ui| {
                if button::secondary_button(
                    ui,
                    None,
                    &t!("prefs.assistant.edit"),
                    &t!("prefs.assistant.edit.tip"),
                    Size::Small,
                    true,
                )
                .clicked()
                {
                    app.prefs_ui.editing = if open { None } else { Some(c.id.clone()) };
                }
                if button::secondary_button(
                    ui,
                    None,
                    &text::test(),
                    &text::test_tip(),
                    Size::Small,
                    app.assistant.host.http.is_some(),
                )
                .clicked()
                {
                    test = Some(c.clone());
                }
            });
        });
        if open {
            connection_details(
                app,
                ui,
                c,
                current,
                &mut make_default,
                &mut clear_key,
                &mut remove,
            );
        }
        if let Some((id, slot)) = &app.assistant.row_test
            && *id == c.id
        {
            result_line(ui, &lock(slot));
        }
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
    if let Some(id) = clear_key {
        clear_connection_key(app, &id);
    }
    if let Some(id) = remove {
        if app.prefs_ui.editing.as_deref() == Some(id.as_str()) {
            app.prefs_ui.editing = None;
        }
        remove_connection(app, &id);
    }
}

/// The details under an opened row: default, key, pictures, permission override and removal.
fn connection_details(
    app: &mut App,
    ui: &mut egui::Ui,
    c: &Connection,
    current: bool,
    make_default: &mut Option<String>,
    clear_key: &mut Option<String>,
    remove: &mut Option<String>,
) {
    let t = theme::tokens(ui.ctx());
    egui::Frame::new()
        .fill(t.raised.gamma_multiply(0.5))
        .inner_margin(egui::Margin::symmetric(
            theme::SPACE[3] as i8,
            theme::SPACE[1] as i8,
        ))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.label(fonts::mono(&c.base_url).color(t.muted));
            let mut images = c.images;
            form_row(
                ui,
                FormWidth::Page,
                &text::takes_pictures(),
                None,
                None,
                |ui| {
                    checkbox(
                        ui,
                        &mut images,
                        &text::takes_pictures(),
                        &text::takes_pictures_tip(),
                    )
                    .changed()
                },
            )
            .then(|| {
                if let Some(row) = app
                    .preferences
                    .assistant_connections
                    .iter_mut()
                    .find(|x| x.id == c.id)
                {
                    row.images = images;
                }
            });
            let overridden = app.preferences.assistant_modes.get(&c.id).copied();
            let mut at = match overridden {
                None => 0,
                Some(mode) => 1 + Mode::ALL.iter().position(|m| *m == mode).unwrap_or(0),
            };
            let before = at;
            let same = text::same_as_default();
            let labels: Vec<String> = std::iter::once(same)
                .chain(Mode::ALL.iter().map(|m| m.label().to_string()))
                .collect();
            let options: Vec<&str> = labels.iter().map(String::as_str).collect();
            form_row(
                ui,
                FormWidth::Page,
                &t!("prefs.assistant.override"),
                None,
                None,
                |ui| {
                    select(
                        ui,
                        ("assistant-mode", &c.id),
                        &mut at,
                        &options,
                        &t!("prefs.assistant.override"),
                        &text::mode_override_tip(),
                        200.0,
                    );
                },
            );
            if at != before {
                if at == 0 {
                    app.preferences.assistant_modes.remove(&c.id);
                } else if let Some(mode) = Mode::ALL.get(at - 1) {
                    app.preferences.assistant_modes.insert(c.id.clone(), *mode);
                }
            }
            ui.add_space(theme::SPACE[1]);
            ui.horizontal(|ui| {
                if button::secondary_button(
                    ui,
                    None,
                    &text::make_default(),
                    &text::make_default_tip(),
                    Size::Small,
                    !current,
                )
                .clicked()
                {
                    *make_default = Some(c.id.clone());
                }
                if button::secondary_button(
                    ui,
                    None,
                    &text::clear_key(),
                    &text::clear_key_tip(),
                    Size::Small,
                    c.has_key,
                )
                .clicked()
                {
                    *clear_key = Some(c.id.clone());
                }
                if button::ghost_button(
                    ui,
                    None,
                    &text::remove(),
                    &text::remove_tip(),
                    Size::Small,
                    true,
                )
                .clicked()
                {
                    *remove = Some(c.id.clone());
                }
            });
            ui.add_space(theme::SPACE[1]);
        });
}

fn protocol_name(p: Protocol) -> String {
    match p {
        Protocol::OpenAi => t!("prefs.assistant.protocol.openai"),
        Protocol::Anthropic => t!("prefs.assistant.protocol.anthropic"),
    }
}

/// The Behaviour card: show the tab, what it may do unasked, always-allowed tools, the cap.
///
/// # Rules
/// - Every row writes the preference the moment it changes.
///
/// # Dependencies
/// - crates/ui-egui/src/ui_kit/form_row.rs, segmented.rs, number_field.rs
///
/// # Do not
/// - Hide a row when the tab is off; the rest of the card stays reachable.
pub(crate) fn behaviour_card(app: &mut App, ui: &mut egui::Ui) {
    let t = theme::tokens(ui.ctx());
    card(
        ui,
        &t!("prefs.assistant.behaviour"),
        None,
        |_| {},
        |ui| {
            form_row(
                ui,
                FormWidth::Page,
                &text::show(),
                Some(&t!("prefs.assistant.show.hint")),
                None,
                |ui| {
                    checkbox(
                        ui,
                        &mut app.preferences.assistant_enabled,
                        &text::show(),
                        &text::show_tip(),
                    );
                },
            );
            let mut at = Mode::ALL
                .iter()
                .position(|m| *m == app.preferences.assistant_mode)
                .unwrap_or(0);
            let before = at;
            let (ask, edits, all) = (
                Mode::Ask.label().to_string(),
                Mode::Edits.label().to_string(),
                Mode::All.label().to_string(),
            );
            let (ask_tip, edits_tip, all_tip) = (
                text::mode_ask_tip(),
                text::mode_edits_tip(),
                text::mode_all_tip(),
            );
            let group = text::permissions();
            form_row(
                ui,
                FormWidth::Page,
                &group,
                Some(&t!("prefs.assistant.permissions.hint")),
                None,
                |ui| {
                    segmented_styled(
                        ui,
                        "prefs-assistant-mode",
                        &mut at,
                        &[
                            Segment::text(&ask, &ask_tip),
                            Segment::text(&edits, &edits_tip),
                            Segment::text(&all, &all_tip),
                        ],
                        SegmentedStyle {
                            full_width: false,
                            group_name: Some(&group),
                        },
                    );
                },
            );
            if at != before
                && let Some(mode) = Mode::ALL.get(at)
            {
                app.preferences.assistant_mode = *mode;
            }
            if app.preferences.assistant_mode == Mode::All {
                ui.label(fonts::caption(text::mode_all_warning()).color(t.gold));
            }
            let mut limit = f64::from(app.preferences.assistant_turn_limit);
            form_row(
                ui,
                FormWidth::Page,
                &text::limit(),
                Some(&t!("prefs.assistant.limit.hint")),
                None,
                |ui| {
                    number_field(
                        ui,
                        "prefs-assistant-limit",
                        &mut limit,
                        &NumberSpec {
                            unit: "",
                            step: 1.0,
                            min: 1.0,
                            max: 200.0,
                            places: 0,
                            width: 104.0,
                        },
                        &text::limit(),
                        &text::limit_tip(),
                    );
                },
            );
            app.preferences.assistant_turn_limit = limit.clamp(1.0, 200.0) as u32;
            let chosen: usize = app
                .preferences
                .assistant_always_allow
                .values()
                .map(Vec::len)
                .sum();
            form_row(
                ui,
                FormWidth::Page,
                &t!("prefs.assistant.always"),
                None,
                None,
                |ui| {
                    let line = if chosen == 0 {
                        t!("prefs.assistant.always.none")
                    } else {
                        t!("prefs.assistant.always.count", n = chosen)
                    };
                    ui.label(fonts::text(fonts::Role::Body, line).color(t.muted));
                    if button::secondary_button(
                        ui,
                        None,
                        &t!("prefs.assistant.forget"),
                        &text::reset_always_tip(),
                        Size::Small,
                        chosen > 0,
                    )
                    .clicked()
                    {
                        app.preferences.assistant_always_allow.clear();
                    }
                },
            );
        },
    );
}

/// The Add connection card, when it is open.
///
/// # Rules
/// - Test and Add behave as the old window did; a key goes to the key store only.
/// - Cancel closes the card without saving anything.
///
/// # Dependencies
/// - crates/ui-egui/src/ui_kit/form_row.rs, select.rs
///
/// # Do not
/// - Show a typed key in clear text.
pub(crate) fn add_form(app: &mut App, ui: &mut egui::Ui) {
    let Some(mut d) = app.assistant.dialog.take() else {
        return;
    };
    let t = theme::tokens(ui.ctx());
    let mut add = false;
    let mut cancel = false;
    card(
        ui,
        &text::dialog_title(),
        None,
        |_| {},
        |ui| {
            let names: Vec<&str> = PRESETS.iter().map(|p| p.name).collect();
            let mut at = d.preset;
            form_row(ui, FormWidth::Page, &text::preset(), None, None, |ui| {
                select(
                    ui,
                    "prefs-assistant-preset",
                    &mut at,
                    &names,
                    &text::preset(),
                    &text::preset_tip(),
                    260.0,
                );
            });
            if at != d.preset {
                d.apply_preset(at);
            }
            let kinds: Vec<String> = ProviderKind::ALL.iter().map(|k| kind_name(*k)).collect();
            let kind_options: Vec<&str> = kinds.iter().map(String::as_str).collect();
            let mut kind_at = ProviderKind::ALL
                .iter()
                .position(|k| *k == d.kind)
                .unwrap_or(0);
            let kind_label = t!("prefs.assistant.col.kind");
            form_row(ui, FormWidth::Page, &kind_label, None, None, |ui| {
                select(
                    ui,
                    "prefs-assistant-kind",
                    &mut kind_at,
                    &kind_options,
                    &kind_label,
                    &text::kind_tip(),
                    260.0,
                );
            });
            if let Some(k) = ProviderKind::ALL.get(kind_at) {
                d.kind = *k;
            }
            if !d.kind.is_built() {
                ui.label(fonts::caption(text::kind_not_built(&kind_name(d.kind))).color(t.gold));
            }
            form_row(ui, FormWidth::Page, &text::name(), None, None, |ui| {
                text_input(
                    ui,
                    "prefs-assistant-name",
                    &mut d.name,
                    320.0,
                    &text::name(),
                    &text::name_tip(),
                    false,
                );
            });
            form_row(ui, FormWidth::Page, &text::url(), None, None, |ui| {
                text_input(
                    ui,
                    "prefs-assistant-url",
                    &mut d.url,
                    320.0,
                    &text::url(),
                    &text::url_tip(),
                    false,
                );
            });
            let listed = match &*lock(&d.test) {
                TestState::Done(Ok((_, ids))) => ids.clone(),
                _ => Vec::new(),
            };
            form_row(ui, FormWidth::Page, &text::model(), None, None, |ui| {
                if listed.is_empty() {
                    text_input(
                        ui,
                        "prefs-assistant-model",
                        &mut d.model,
                        320.0,
                        &text::model(),
                        &text::model_tip(),
                        false,
                    );
                } else {
                    let options: Vec<&str> = listed.iter().map(String::as_str).collect();
                    let mut chosen = listed.iter().position(|m| *m == d.model).unwrap_or(0);
                    select(
                        ui,
                        "prefs-assistant-model-list",
                        &mut chosen,
                        &options,
                        &text::model(),
                        &text::model_tip(),
                        320.0,
                    );
                    if let Some(id) = listed.get(chosen) {
                        d.model = id.clone();
                    }
                }
            });
            key_field(app, ui, &mut d);
            ui.add_space(theme::SPACE[1]);
            ui.horizontal(|ui| {
                if button::primary_button(
                    ui,
                    None,
                    &text::add(),
                    &text::add_tip(),
                    Size::Normal,
                    true,
                )
                .clicked()
                {
                    add = true;
                }
                if button::secondary_button(
                    ui,
                    None,
                    &text::test(),
                    &text::test_tip(),
                    Size::Normal,
                    app.assistant.host.http.is_some(),
                )
                .clicked()
                    && let Some(http) = app.assistant.host.http.clone()
                {
                    match normalize_url(&d.url) {
                        Ok(url) => {
                            d.error.clear();
                            let key =
                                (!d.key.is_empty() && key_safe_for(&url)).then(|| d.key.clone());
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
                if button::ghost_button(
                    ui,
                    None,
                    &text::cancel(),
                    &text::cancel_tip(),
                    Size::Normal,
                    true,
                )
                .clicked()
                {
                    cancel = true;
                }
            });
            result_line(ui, &lock(&d.test));
            if !d.error.is_empty() {
                ui.label(fonts::text(fonts::Role::Body, d.error.as_str()).color(t.red));
            }
        },
    );
    if add {
        match finish(app, &d) {
            Ok(()) => return,
            Err(e) => d.error = e,
        }
    }
    if !cancel {
        app.assistant.dialog = Some(d);
    }
}

/// A one-line text box in the form's control column.
///
/// Why: the Add connection fields share one look and one way to be named.
///
/// # Rules
/// - `password` hides the characters; a hidden box never offers its text to a screen reader.
///
/// # Do not
/// - Log or echo what was typed.
pub(crate) fn text_input(
    ui: &mut egui::Ui,
    salt: &str,
    text: &mut String,
    width: f32,
    name: &str,
    tip: &str,
    password: bool,
) -> egui::Response {
    ui.add(
        crate::targets::text_field(text)
            .id_salt(salt)
            .password(password)
            .desired_width(width),
    )
    .named(name)
    .on_hover_text(tip)
}

/// Delete the stored key of one connection and keep the connection.
pub(crate) fn clear_connection_key(app: &mut App, id: &str) {
    if let Some(keys) = &app.assistant.host.keys {
        let _ = keys.clear(&pcbcraft_assistant::key_id(id));
    }
    if let Some(row) = app
        .preferences
        .assistant_connections
        .iter_mut()
        .find(|c| c.id == id)
    {
        row.has_key = false;
    }
    app.assistant_reconnect();
}

fn stored_key(app: &App, c: &Connection) -> Option<String> {
    if !c.has_key {
        return None;
    }
    app.assistant
        .host
        .keys
        .as_ref()
        .and_then(|s| s.get(&pcbcraft_assistant::key_id(&c.id)).ok().flatten())
        .map(|k| k.expose().to_string())
}

/// Forget a connection and delete its keychain entry.
pub(crate) fn remove_connection(app: &mut App, id: &str) {
    if let Some(keys) = &app.assistant.host.keys {
        let _ = keys.clear(&pcbcraft_assistant::key_id(id));
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

fn key_field(app: &App, ui: &mut egui::Ui, d: &mut Dialog) {
    let t = theme::tokens(ui.ctx());
    let Some(keys) = app.assistant.host.keys.as_ref() else {
        ui.label(fonts::caption(text::key_no_store()).color(t.muted));
        return;
    };
    if key_safe_for(&d.url) {
        let tip = match keys.kept_in() {
            KeptIn::SystemKeychain => text::key_tip(),
            KeptIn::BrowserStorage => text::key_tip_browser(),
            KeptIn::Memory => text::key_tip_memory(),
        };
        form_row(ui, FormWidth::Page, &text::key(), None, None, |ui| {
            text_input(
                ui,
                "assistant-key",
                &mut d.key,
                320.0,
                &text::key(),
                &tip,
                true,
            );
        });
        if keys.kept_in() == KeptIn::BrowserStorage {
            ui.label(fonts::caption(text::key_warning_browser()).color(t.muted));
        }
    } else {
        d.key.clear();
        ui.label(fonts::caption(text::key_refused()).color(t.muted));
    }
}

fn kind_name(k: ProviderKind) -> String {
    match k {
        ProviderKind::Cloud => text::kind_cloud(),
        ProviderKind::WebGpu => text::kind_webgpu(),
        ProviderKind::WebMcp => text::kind_webmcp(),
    }
}

/// Save the dialog's connection; the key goes to the keychain and nowhere else.
fn finish(app: &mut App, d: &Dialog) -> Result<(), String> {
    if !d.kind.is_built() {
        return Err(text::kind_not_built(&kind_name(d.kind)));
    }
    let url = normalize_url(&d.url)?;
    if d.name.trim().is_empty() {
        return Err(t!("prefs.assistant.name_needed"));
    }
    let id = new_id(&app.preferences.assistant_connections, "api");
    let mut has_key = false;
    if !d.key.is_empty() {
        if !key_safe_for(&url) {
            return Err(text::key_refused());
        }
        let keys = app
            .assistant
            .host
            .keys
            .clone()
            .ok_or(text::key_no_store())?;
        keys.set(&pcbcraft_assistant::key_id(&id), &d.key)?;
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
