//! Edit > Preferences: one modal dialog for everything that belongs to the person and the
//! app, not to a project, split into tabs by concern.
//!
//! Context: issue #1690 and the approved mockups `dialog-preferences`, `prefs-appearance`,
//! `prefs-language-units`, `prefs-keys` and `prefs-assistant` in the
//! design folder. The tabs are Appearance, Language and units, Keys and
//! Assistant. Library sources live in the Library tab (issue #1799). Project rules, stackup and outputs live in File > Project settings.
//! Local rules: changes apply at once and are saved with the other preferences when the
//! window saves its state. Done and Escape only close the dialog. Reset to defaults puts
//! back the settings of the tab in view and never deletes a saved connection or its key.
//! Text comes from the catalog (`prefs.*` in `crates/text/strings/en.txt`).
//!
//! # Dependencies
//! - crates/ui-egui/src/ui_kit/dialog.rs (frame, tabs, footer, keys)
//! - crates/ui-egui/src/ui_kit/form_row.rs, card.rs, segmented.rs, select.rs, swatch.rs, table.rs
//! - crates/ui-egui/src/lib.rs (`Preferences`, `App::set_language`)
//! - crates/ui-egui/src/assistant/settings.rs (connection cards)
//!
//! # Do not
//! - Write the project; nothing here is part of a design.
//! - Show a stored key, in full or in part, or write one to the log.
//! - Close on a click outside the dialog (see `ui_kit/dialog.rs`).
mod appearance;
mod keys;
mod language;
mod libraries;

use crate::ui_kit::dialog::{self, Dialog, Outcome};
use crate::ui_kit::tab_bar::TabItem;
use crate::{App, assistant, fonts, theme};
use pcbcraft_text::t;

/// Tab index of Appearance.
pub(crate) const TAB_APPEARANCE: usize = 0;
/// Tab index of Language and units.
pub(crate) const TAB_LANGUAGE: usize = 1;
/// Tab index of Keys.
pub(crate) const TAB_KEYS: usize = 2;
/// Tab index of Assistant.
pub(crate) const TAB_ASSISTANT: usize = 3;

/// What the open dialog remembers between frames.
#[derive(Default)]
pub(crate) struct State {
    /// Whether the dialog is showing.
    pub open: bool,
    /// Index of the tab in view.
    pub tab: usize,
    /// The text typed in the shortcut search box.
    pub keys_search: String,
    /// Index of the area filter of the shortcut list.
    pub keys_area: usize,
    /// Id of the connection whose details are open on the Assistant tab.
    pub editing: Option<String>,
}

/// A muted paragraph under a card, wrapped to the width.
pub(super) fn note(ui: &mut egui::Ui, text: impl Into<String>) {
    let muted = theme::tokens(ui.ctx()).muted;
    ui.label(fonts::caption(text).color(muted));
}

impl App {
    /// Open Preferences on one tab.
    ///
    /// # Rules
    /// - Opening again only changes the tab.
    ///
    /// # Dependencies
    /// - crates/ui-egui/src/lib.rs (`App::prefs_ui`)
    ///
    /// # Do not
    /// - Call from a command path; programmatic paths never open dialogs.
    pub(crate) fn open_preferences(&mut self, tab: usize) {
        self.prefs_ui.open = true;
        self.prefs_ui.tab = tab.min(TAB_ASSISTANT);
    }

    /// Draw the Preferences dialog while it is open.
    ///
    /// # Rules
    /// - Done, Escape and the close button close it; every change was already applied.
    /// - Reset to defaults acts on the tab in view.
    ///
    /// # Dependencies
    /// - crates/ui-egui/src/ui_kit/dialog.rs (`show_with_extra`)
    ///
    /// # Do not
    /// - Draw anything when closed.
    pub(crate) fn preferences_window(&mut self, ctx: &egui::Context) {
        if !self.prefs_ui.open {
            return;
        }
        let labels = [
            (t!("prefs.tab.appearance"), t!("prefs.tab.appearance.tip")),
            (t!("prefs.tab.language"), t!("prefs.tab.language.tip")),
            (t!("prefs.tab.keys"), t!("prefs.tab.keys.tip")),
            (t!("prefs.tab.assistant"), t!("prefs.tab.assistant.tip")),
        ];
        let tabs: Vec<TabItem<'_>> = labels
            .iter()
            .map(|(label, tip)| TabItem::new(label, tip))
            .collect();
        let title = t!("prefs.title");
        let note_text = t!("prefs.note");
        let done = (t!("prefs.done"), t!("prefs.done.tip"));
        let reset = (t!("prefs.reset"), t!("prefs.reset.tip"));
        let width = (ctx.content_rect().width() - 80.0).clamp(640.0, 1100.0);
        let spec = Dialog {
            id: "preferences",
            title: &title,
            width,
            tabs: &tabs,
            note: Some(&note_text),
            cancel: None,
            primary: (&done.0, &done.1),
        };
        let mut tab = self.prefs_ui.tab;
        let shown = tab;
        let (outcome, reset_pressed) =
            dialog::show_with_extra(ctx, &spec, &mut tab, Some((&reset.0, &reset.1)), |ui| {
                self.preferences_body(ui, shown)
            });
        self.prefs_ui.tab = tab;
        if reset_pressed {
            self.reset_preferences_tab(ctx, shown);
        }
        if outcome != Outcome::Open {
            self.prefs_ui.open = false;
            self.binding_capture = None;
        }
    }

    /// The body of the tab in view.
    fn preferences_body(&mut self, ui: &mut egui::Ui, tab: usize) {
        match tab {
            TAB_APPEARANCE => self.prefs_appearance(ui),
            TAB_LANGUAGE => self.prefs_language(ui),
            TAB_KEYS => self.prefs_keys(ui),
            TAB_ASSISTANT => {
                assistant::settings::connections_card(self, ui);
                ui.add_space(theme::SPACE[3]);
                assistant::settings::add_form(self, ui);
                if self.assistant.dialog.is_some() {
                    ui.add_space(theme::SPACE[3]);
                }
                assistant::settings::behaviour_card(self, ui);
            }
            _ => {}
        }
    }

    /// Put the settings of one tab back to their defaults.
    ///
    /// # Rules
    /// - Saved assistant connections and their keys are kept.
    ///
    /// # Do not
    /// - Touch a setting that is not on the tab in view.
    fn reset_preferences_tab(&mut self, ctx: &egui::Context, tab: usize) {
        let fresh = crate::Preferences::default();
        let p = &mut self.preferences;
        match tab {
            TAB_APPEARANCE => {
                p.dark = fresh.dark;
                p.theme_system = fresh.theme_system;
                p.accent = fresh.accent;
                p.ui_scale = fresh.ui_scale;
                p.reduced_motion = fresh.reduced_motion;
                p.grid = fresh.grid;
                p.pad_labels = fresh.pad_labels;
                p.offer_tours = fresh.offer_tours;
                p.minimap.clear();
            }
            TAB_LANGUAGE => {
                p.default_units = fresh.default_units;
                p.default_grid_mm = fresh.default_grid_mm;
                p.board_grid_mm = fresh.board_grid_mm;
                p.number_format = fresh.number_format;
                self.set_language(ctx, &fresh.language);
            }
            TAB_KEYS => {
                p.active_keymap_mut().reset_to_default();
                p.modifiers.reset_all();
                self.keymap_notice = t!("prefs.keys.reset_done");
            }
            TAB_ASSISTANT => {
                p.assistant_enabled = fresh.assistant_enabled;
                p.assistant_mode = fresh.assistant_mode;
                p.assistant_turn_limit = fresh.assistant_turn_limit;
                p.assistant_always_allow.clear();
                p.assistant_modes.clear();
            }
            _ => {}
        }
        self.message = t!("prefs.reset_done");
    }

    /// Which tab a capture scene names: `preferences-appearance` and so on.
    pub(crate) fn capture_preferences(&mut self, scene: &str) -> bool {
        let Some(stem) = scene.strip_prefix("preferences-") else {
            return false;
        };
        let tab = match stem {
            "appearance" => TAB_APPEARANCE,
            "language" => TAB_LANGUAGE,
            "keys" => TAB_KEYS,
            "assistant" => TAB_ASSISTANT,
            _ => return false,
        };
        self.open_preferences(tab);
        true
    }
}
