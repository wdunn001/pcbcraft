//! File > Project settings: one dialog for every rule and default that travels with
//! the design, split into tabs by concern.
//!
//! Context: issue #775 (fork #617) and the approved mockups `dialog-project-board` and
//! `dialog-project-rules` in the design folder. The tabs are Board and stackup, Design
//! rules, Net classes, Electrical rules, Machines and Title block. The preset picker and
//! per-limit origin words of the held Rules tab PR (#37) live in `rules.rs`; the pin
//! matrix grid of the held PR (#38) lives in `electrical.rs`.
//! Local rules: every edit is a typed engine command (`App::run`), so it is one undo step
//! and the checks re-run at once. The dialog shows the result live; Cancel puts the
//! project back as it was when the dialog opened (one `Session::replace`, itself undoable).
//! Text comes from the catalog (`settings.project.*` in `crates/text/strings/en.txt`).
//!
//! # Dependencies
//! - crates/ui-egui/src/ui_kit/dialog.rs (frame, tabs, footer, keys)
//! - crates/ui-egui/src/ui_kit/form_row.rs, number_field.rs, select.rs, card.rs
//! - crates/engine/src/lib.rs (`board.*`, `netclass.*`, `rules.*`, `erc.*`, `machine.*`,
//!   `project.set_meta`, `checks.custom_*`)
//! - crates/ui-egui/src/lib.rs (`App::run`, `App::refresh`)
//!
//! # Do not
//! - Write the project directly; every change is an engine command.
//! - Hold rule data here; the project is the only copy.
//! - Close on a click outside the dialog (see `ui_kit/dialog.rs`).
mod board;
mod classes;
mod electrical;
mod machines;
mod rules;
mod title;

use crate::ui_kit::button::{self, Size};
use crate::ui_kit::dialog::{self, Dialog, Outcome};
use crate::ui_kit::number_field::{NumberSpec, number_field};
use crate::ui_kit::tab_bar::TabItem;
use crate::{App, fonts, theme};
use pcbcraft_doc::Project;
use pcbcraft_text::t;
use serde_json::Value;

/// The tabs, in order: catalog key stem and the name a story opens them by.
const TABS: [&str; 6] = [
    "board",
    "rules",
    "classes",
    "electrical",
    "machines",
    "title",
];

/// Tab index of Board and stackup.
pub(crate) const TAB_BOARD: usize = 0;
/// Tab index of Design rules.
pub(crate) const TAB_RULES: usize = 1;
/// Tab index of Net classes.
pub(crate) const TAB_CLASSES: usize = 2;
/// Tab index of Electrical rules.
pub(crate) const TAB_ELECTRICAL: usize = 3;
/// Tab index of Machines.
pub(crate) const TAB_MACHINES: usize = 4;
/// Tab index of Title block.
pub(crate) const TAB_TITLE: usize = 5;

/// What the open dialog remembers between frames.
#[derive(Default)]
pub(crate) struct State {
    /// Whether the dialog is showing.
    pub open: bool,
    /// Index of the tab in view.
    pub tab: usize,
    /// The project as it was when the dialog opened, for Cancel.
    snapshot: Option<Project>,
    /// Engine revision when the dialog opened.
    opened_at: u64,
    /// The engine's refusal for the last edit, shown at the top of the body.
    error: Option<String>,
    /// Draft for the Add layer row: name.
    layer_name: String,
    /// Draft for the Add layer row: index into the role list.
    layer_kind: usize,
    /// Draft for the Add layer row: thickness in millimetres.
    layer_thickness: f64,
    /// Draft for a new custom rule: name, measure index, limit, loud.
    rule_name: String,
    rule_measure: usize,
    rule_limit: f64,
    rule_loud: bool,
    /// Draft for the machine preset to add, an index into the built-in list.
    machine_pick: usize,
}

impl State {
    fn blank() -> Self {
        Self {
            layer_thickness: 0.2,
            rule_limit: 0.15,
            ..Self::default()
        }
    }
}

/// A millimetre field spec: range, 0.01 step, three decimals.
pub(super) fn mm_spec(min: f64, max: f64) -> NumberSpec<'static> {
    NumberSpec {
        unit: "mm",
        min,
        max,
        step: 0.01,
        places: 3,
        width: 104.0,
    }
}

/// Draw a number field and hand back the new value on the frame it was accepted.
///
/// Why: every numeric row writes one engine command when the value moves, so one edit
/// is one undo step.
///
/// # Rules
/// - `salt` is unique across the dialog; `name` is the accessible name.
///
/// # Dependencies
/// - crates/ui-egui/src/ui_kit/number_field.rs
///
/// # Do not
/// - Write to the project here; the caller runs the command.
pub(super) fn num(
    ui: &mut egui::Ui,
    salt: impl std::hash::Hash + std::fmt::Debug,
    value: f64,
    spec: &NumberSpec<'_>,
    name: &str,
    tip: &str,
) -> Option<f64> {
    let mut v = value;
    number_field(ui, salt, &mut v, spec, name, tip)
        .changed()
        .then_some(v)
}

/// A muted paragraph under a heading, wrapped to the width.
pub(super) fn note(ui: &mut egui::Ui, text: impl Into<String>) {
    let muted = theme::tokens(ui.ctx()).muted;
    ui.label(fonts::caption(text).color(muted));
}

/// A single-line text field that writes only when the person leaves it with a changed
/// value. Returns the new text on that frame.
///
/// Why: a name typed key by key must be one undo step, not one per letter.
///
/// # Rules
/// - The buffer lives in egui memory keyed by `salt`; it follows `current` while the
///   field has no focus.
///
/// # Do not
/// - Use for numbers; use `num`.
pub(super) fn text_field(
    ui: &mut egui::Ui,
    salt: impl std::hash::Hash + std::fmt::Debug,
    current: &str,
    width: f32,
    name: &str,
    tip: &str,
) -> Option<String> {
    use crate::a11y::Named;
    let id = ui.id().with(("ps-text", salt));
    let mut buffer: String = ui
        .data(|d| d.get_temp(id))
        .unwrap_or_else(|| current.into());
    let focused = ui.memory(|m| m.has_focus(id));
    if !focused && buffer != current {
        buffer = current.to_owned();
    }
    let response = ui
        .add(
            crate::targets::text_field(&mut buffer)
                .id(id)
                .desired_width(width),
        )
        .named(name)
        .on_hover_text(tip);
    let done = response.lost_focus() && buffer != current;
    ui.data_mut(|d| d.insert_temp(id, buffer.clone()));
    done.then_some(buffer)
}

impl App {
    /// Open Project settings on one tab, remembering the project for Cancel.
    ///
    /// # Rules
    /// - Opening twice keeps the first snapshot.
    ///
    /// # Dependencies
    /// - crates/ui-egui/src/lib.rs (`App::project`, `App::revision`)
    ///
    /// # Do not
    /// - Call from a command path; programmatic paths never open dialogs.
    pub(crate) fn open_project_settings(&mut self, tab: usize) {
        let state = &mut self.project_settings;
        state.tab = tab.min(TABS.len() - 1);
        if state.open {
            return;
        }
        let drafts = State::blank();
        *state = State {
            open: true,
            tab: state.tab,
            snapshot: Some(self.project.clone()),
            opened_at: self.revision,
            ..drafts
        };
    }

    /// Run one engine command from the dialog and keep its refusal for the body.
    ///
    /// Why: `App::run` only writes the status bar, which sits behind the dialog.
    ///
    /// # Rules
    /// - A command that leaves the revision unchanged was refused; its message is shown.
    ///
    /// # Do not
    /// - Chain commands that must be one undo step; use `settings_batch`.
    pub(super) fn settings_run(&mut self, command: &str, params: Value) {
        let before = self.revision;
        self.run(command, params);
        self.project_settings.error = (self.revision == before).then(|| self.message.clone());
    }

    /// Run several commands as one undo step.
    ///
    /// # Rules
    /// - All or none: a refusal leaves the project untouched and shows the message.
    ///
    /// # Dependencies
    /// - crates/engine/src/lib.rs (`Session::batch`)
    pub(super) fn settings_batch(&mut self, steps: Vec<(&str, Value)>) {
        let commands: Vec<pcbcraft_engine::Command> = steps
            .into_iter()
            .map(|(command, params)| pcbcraft_engine::Command {
                command: command.to_owned(),
                params,
            })
            .collect();
        let revision = self.revision;
        let outcome = self
            .session
            .lock()
            .map_err(|_| t!("dialogs.run.session_unavailable"))
            .and_then(|mut s| {
                s.batch(&commands, Some(revision))
                    .map_err(|e| e.to_string())
            });
        self.project_settings.error = outcome.err();
        self.refresh();
    }

    /// Put the project back as it was when the dialog opened.
    fn revert_project_settings(&mut self) {
        let Some(snapshot) = self.project_settings.snapshot.take() else {
            return;
        };
        if self.revision == self.project_settings.opened_at {
            return;
        }
        let outcome = self
            .session
            .lock()
            .map_err(|_| t!("dialogs.run.session_unavailable"))
            .and_then(|mut s| s.replace(snapshot).map_err(|e| e.to_string()));
        if let Err(e) = outcome {
            self.message = e;
        }
        self.refresh();
    }

    /// Show the dialog for one frame, when open.
    ///
    /// # Rules
    /// - Save keeps what the dialog already applied; Cancel and Escape revert it.
    /// - The footer says the settings are saved with the project.
    ///
    /// # Dependencies
    /// - crates/ui-egui/src/ui_kit/dialog.rs
    ///
    /// # Do not
    /// - Draw anything when closed.
    pub(crate) fn project_settings_window(&mut self, ctx: &egui::Context) {
        if !self.project_settings.open {
            return;
        }
        let labels: Vec<(String, String)> = TABS
            .iter()
            .map(|stem| {
                (
                    pcbcraft_text::lookup(&format!("settings.project.tab.{stem}"), &[]),
                    pcbcraft_text::lookup(&format!("settings.project.tab.{stem}.tip"), &[]),
                )
            })
            .collect();
        let tabs: Vec<TabItem<'_>> = labels
            .iter()
            .map(|(label, tip)| TabItem::new(label, tip))
            .collect();
        let title = t!("settings.project.title");
        let note_text = t!("settings.project.note");
        let cancel = (
            t!("settings.project.cancel"),
            t!("settings.project.cancel.tip"),
        );
        let save = (t!("settings.project.save"), t!("settings.project.save.tip"));
        let width = (ctx.content_rect().width() - 80.0).clamp(640.0, 980.0);
        let spec = Dialog {
            id: "project-settings",
            title: &title,
            width,
            tabs: &tabs,
            note: Some(&note_text),
            cancel: Some((&cancel.0, &cancel.1)),
            primary: (&save.0, &save.1),
        };
        let mut tab = self.project_settings.tab;
        let shown = tab;
        let outcome = dialog::show(ctx, &spec, &mut tab, |ui| {
            self.settings_body(ui, shown);
        });
        self.project_settings.tab = tab;
        match outcome {
            Outcome::Open => {}
            Outcome::Primary => {
                self.project_settings.open = false;
                self.project_settings.snapshot = None;
                self.message = t!("settings.project.saved");
            }
            Outcome::Cancel => {
                self.project_settings.open = false;
                self.revert_project_settings();
                self.message = t!("settings.project.cancelled");
            }
        }
    }

    /// The body of the tab in view, with the engine's last refusal on top.
    fn settings_body(&mut self, ui: &mut egui::Ui, tab: usize) {
        if let Some(error) = self.project_settings.error.clone() {
            let red = theme::tokens(ui.ctx()).red;
            ui.label(fonts::text(fonts::Role::Body, error).color(red));
        }
        match tab {
            TAB_BOARD => self.settings_board(ui),
            TAB_RULES => self.settings_rules(ui),
            TAB_CLASSES => self.settings_classes(ui),
            TAB_ELECTRICAL => self.settings_electrical(ui),
            TAB_MACHINES => self.settings_machines(ui),
            TAB_TITLE => self.settings_title(ui),
            _ => {}
        }
    }

    /// The engine's last refusal shown in the dialog, for stories.
    #[cfg(test)]
    pub(crate) fn project_settings_error(&self) -> Option<String> {
        self.project_settings.error.clone()
    }

    /// Which catalog stem a capture scene or story names: `settings-rules` and so on.
    pub(crate) fn capture_project_settings(&mut self, scene: &str) -> bool {
        let Some(stem) = scene.strip_prefix("settings-") else {
            return false;
        };
        match TABS.iter().position(|s| *s == stem) {
            Some(index) => {
                self.open_project_settings(index);
                true
            }
            None => false,
        }
    }
}

/// A secondary button sized normally.
pub(super) fn secondary(
    ui: &mut egui::Ui,
    label: &str,
    tip: &str,
    enabled: bool,
) -> egui::Response {
    button::secondary_button(ui, None, label, tip, Size::Normal, enabled)
}

/// A ghost button sized normally.
pub(super) fn ghost(ui: &mut egui::Ui, label: &str, tip: &str, enabled: bool) -> egui::Response {
    button::ghost_button(ui, None, label, tip, Size::Normal, enabled)
}
