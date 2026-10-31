//! Title block tab: the title, company, author, revision, date and paper size that every
//! sheet of the schematic carries.
//!
//! Context: the values live in the project's metadata (`Project::meta`), keyed by
//! lower-case name; the schematic sheet reads `title`, `revision`, `date` and `paper`
//! (`crates/ui-egui/src/canvas/sheet.rs`). Company and author travel with the file for
//! exports and are not drawn yet.
//! Local rules: a field writes when the person leaves it with a changed value, so a
//! title typed letter by letter is one undo step; an emptied field clears its key; the
//! engine refuses to record the same value twice, so nothing is sent when unchanged.
//!
//! # Dependencies
//! - crates/engine/src/lib.rs (`project.set_meta`, `project.clear_meta`)
//! - crates/ui-egui/src/canvas/sheet.rs (reader of the keys)
//!
//! # Do not
//! - Invent keys; the six below are the whole title block.
use super::{note, text_field};
use crate::App;
use crate::ui_kit::card::card;
use crate::ui_kit::form_row::{FormWidth, form_row};
use crate::ui_kit::select::select;
use pcbcraft_text::t;
use serde_json::json;

/// The text keys of the title block, top to bottom.
const KEYS: [&str; 5] = ["title", "company", "author", "revision", "date"];

/// Paper sizes the sheet can take; the first entry means "fit the drawing".
const PAPER: [&str; 9] = ["", "A7", "A6", "A5", "A4", "A3", "A2", "A1", "A0"];

impl App {
    /// The Title block tab.
    ///
    /// # Rules
    /// - Text fields write on leaving the field; the paper list writes on a pick.
    ///
    /// # Dependencies
    /// - crates/ui-egui/src/ui_kit/card.rs
    ///
    /// # Do not
    /// - Write the date for the person; an empty date shows today on the sheet.
    pub(super) fn settings_title(&mut self, ui: &mut egui::Ui) {
        card(
            ui,
            &t!("settings.project.title_block"),
            Some(&t!("settings.project.title_block.caption")),
            |_| {},
            |ui| {
                for key in KEYS {
                    let name = pcbcraft_text::lookup(&format!("settings.project.meta.{key}"), &[]);
                    let tip =
                        pcbcraft_text::lookup(&format!("settings.project.meta.{key}.tip"), &[]);
                    let hint =
                        pcbcraft_text::lookup(&format!("settings.project.meta.{key}.hint"), &[]);
                    let current = self.project.meta.get(key).cloned().unwrap_or_default();
                    let written = form_row(ui, FormWidth::Page, &name, Some(&hint), None, |ui| {
                        text_field(ui, ("meta", key), &current, 320.0, &name, &tip)
                    });
                    if let Some(text) = written {
                        self.settings_meta(key, text.trim());
                    }
                }
                let name = t!("settings.project.meta.paper");
                let current = self.project.meta.get("paper").cloned().unwrap_or_default();
                let mut at = PAPER
                    .iter()
                    .position(|p| p.eq_ignore_ascii_case(current.trim()))
                    .unwrap_or(0);
                let before = at;
                let labels: Vec<String> = PAPER
                    .iter()
                    .map(|p| {
                        if p.is_empty() {
                            t!("settings.project.meta.paper.auto")
                        } else {
                            (*p).to_owned()
                        }
                    })
                    .collect();
                let options: Vec<&str> = labels.iter().map(String::as_str).collect();
                form_row(
                    ui,
                    FormWidth::Page,
                    &name,
                    Some(&t!("settings.project.meta.paper.hint")),
                    None,
                    |ui| {
                        select(
                            ui,
                            "ps-paper",
                            &mut at,
                            &options,
                            &name,
                            &t!("settings.project.meta.paper.tip"),
                            160.0,
                        );
                    },
                );
                if at != before {
                    self.settings_meta("paper", PAPER[at]);
                }
                ui.add_space(4.0);
                note(ui, t!("settings.project.title_block.shared"));
            },
        );
    }

    /// Record one metadata entry, or clear it when the text is empty.
    fn settings_meta(&mut self, key: &str, text: &str) {
        if text.is_empty() {
            if self.project.meta.contains_key(key) {
                self.settings_run("project.clear_meta", json!({"key": key}));
            }
        } else {
            self.settings_run("project.set_meta", json!({"key": key, "value": text}));
        }
    }
}
