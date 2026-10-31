//! Electrical rules tab: how strict the electrical checks are, and the pin connection
//! grid that says what happens when two pin types share a net.
//!
//! Context: ported from the held pin matrix PR (#38, `pin_matrix.rs`). Each pair of the
//! twelve pin types is one cell: OK, warning or error. A click cycles the cell; a star
//! marks a cell that differs from the built-in table.
//! Local rules: only the lower triangle is drawn because a cell and its mirror are one
//! setting; levels are judged by the rules crate and this file only shows them and sends
//! `erc.matrix_set` or `erc.matrix_reset`, so Undo works.
//!
//! # Dependencies
//! - crates/doc/src/lib.rs (`PIN_ELECTRICAL_TYPES`, `default_pin_matrix_level`,
//!   `Project::pin_matrix_level`, `pin_matrix_key`)
//! - crates/engine/src/lib.rs (`erc.matrix_set`, `erc.matrix_reset`, `rules.severity`)
//!
//! # Do not
//! - Compute findings here; `crates/rules/src/lib.rs` owns that.
use super::{ghost, note};
use crate::ui_kit::card::card;
use crate::{App, a11y, theme};
use pcbcraft_doc::{PIN_ELECTRICAL_TYPES, default_pin_matrix_level, pin_matrix_key};
use pcbcraft_text::t;
use serde_json::json;

/// Check codes of the electrical pass whose strictness is set on this tab.
const ELECTRICAL_CODES: [&str; 4] = [
    "unassigned_pin",
    "single_pin_net",
    "pin_conflict",
    "undriven_input",
];

/// The plain name, short column title and meaning of one pin type.
fn words(pin_type: &str) -> (String, String, String) {
    let look =
        |part: &str| pcbcraft_text::lookup(&format!("checks.pin_matrix.{part}.{pin_type}"), &[]);
    (
        look("type"),
        look("short"),
        pcbcraft_text::lookup(&format!("checks.pin_matrix.type.{pin_type}.tip"), &[]),
    )
}

/// The word shown on a cell for a level.
fn level_word(level: &str) -> String {
    match level {
        "error" => t!("checks.pin_matrix.level.error"),
        "warning" => t!("checks.pin_matrix.level.warning"),
        _ => t!("checks.pin_matrix.level.ok"),
    }
}

/// The level a click moves a cell to: ok, then warning, then error, then ok.
pub(crate) fn next_level(level: &str) -> &'static str {
    match level {
        "ok" => "warning",
        "warning" => "error",
        _ => "ok",
    }
}

impl App {
    /// The Electrical rules tab.
    ///
    /// # Rules
    /// - Cell edits go through `erc.matrix_set`; Reset runs `erc.matrix_reset`.
    ///
    /// # Dependencies
    /// - crates/ui-egui/src/ui_kit/card.rs
    ///
    /// # Do not
    /// - Write `Project::pin_matrix` directly.
    pub(super) fn settings_electrical(&mut self, ui: &mut egui::Ui) {
        card(
            ui,
            &t!("settings.project.electrical.strict"),
            None,
            |_| {},
            |ui| {
                note(ui, t!("settings.project.electrical.strict.intro"));
                self.severity_rows(ui, &ELECTRICAL_CODES);
            },
        );
        ui.add_space(8.0);
        let changed = !self.project.pin_matrix.is_empty();
        let mut reset = false;
        card(
            ui,
            &t!("checks.pin_matrix.title"),
            Some(&t!("settings.project.electrical.grid.caption")),
            |ui| {
                let tip = if changed {
                    t!("checks.pin_matrix.reset.tip")
                } else {
                    t!("settings.project.electrical.reset.disabled_tip")
                };
                reset = ghost(ui, &t!("checks.pin_matrix.reset"), &tip, changed).clicked();
            },
            |ui| {
                note(ui, t!("checks.pin_matrix.blurb"));
                ui.add_space(8.0);
                if let Some((a, b, level)) = self.pin_grid(ui) {
                    self.settings_run(
                        "erc.matrix_set",
                        json!({"type_a": a, "type_b": b, "level": level}),
                    );
                }
            },
        );
        if reset {
            self.settings_run("erc.matrix_reset", json!({}));
        }
    }

    /// Draw the grid and return the cell pressed this frame: (type a, type b, new level).
    fn pin_grid(&self, ui: &mut egui::Ui) -> Option<(String, String, String)> {
        let t = theme::tokens(ui.ctx());
        let mut chosen = None;
        egui::ScrollArea::horizontal()
            .id_salt("ps-pin-scroll")
            .show(ui, |ui| {
                egui::Grid::new("ps-pin-grid")
                    .spacing(egui::vec2(4.0, 4.0))
                    .show(ui, |ui| {
                        ui.label("");
                        for column in PIN_ELECTRICAL_TYPES {
                            let (_, short, tip) = words(column);
                            ui.label(crate::fonts::caption(short).color(t.muted))
                                .on_hover_text(tip);
                        }
                        ui.end_row();
                        for (row_index, row) in PIN_ELECTRICAL_TYPES.iter().enumerate() {
                            let (row_name, _, tip) = words(row);
                            ui.label(
                                crate::fonts::text(crate::fonts::Role::Body, &row_name)
                                    .color(t.text),
                            )
                            .on_hover_text(tip);
                            for (column_index, column) in PIN_ELECTRICAL_TYPES.iter().enumerate() {
                                if column_index > row_index {
                                    ui.label("");
                                    continue;
                                }
                                let level = self.project.pin_matrix_level(row, column);
                                let default = default_pin_matrix_level(row, column);
                                let custom = level != default;
                                let (column_name, _, _) = words(column);
                                let word = level_word(&level);
                                let face = if custom {
                                    format!("{word} *")
                                } else {
                                    word.clone()
                                };
                                let tone = match level.as_str() {
                                    "error" => t.red,
                                    "warning" => t.gold,
                                    _ => t.mask,
                                };
                                let button =
                                    egui::Button::new(crate::fonts::caption(face).color(t.text))
                                        .fill(tone.gamma_multiply(0.3))
                                        .stroke(egui::Stroke::new(theme::BORDER, tone))
                                        .min_size(egui::vec2(60.0, 24.0));
                                let tip = if custom {
                                    t!(
                                        "checks.pin_matrix.cell.changed.tip",
                                        a = row_name,
                                        b = column_name,
                                        level = word.clone(),
                                        default = level_word(default)
                                    )
                                } else {
                                    t!(
                                        "checks.pin_matrix.cell.tip",
                                        a = row_name,
                                        b = column_name,
                                        level = word.clone()
                                    )
                                };
                                let response = ui.add(button).on_hover_text(tip);
                                a11y::name_control(
                                    &response,
                                    &t!(
                                        "checks.pin_matrix.cell.name",
                                        a = row_name,
                                        b = column_name,
                                        level = word
                                    ),
                                    None,
                                );
                                ui.ctx().data_mut(|d| {
                                    d.insert_temp(cell_memory_id(row, column), response.rect);
                                });
                                if response.clicked() {
                                    chosen = Some((
                                        (*row).to_owned(),
                                        (*column).to_owned(),
                                        next_level(&level).to_owned(),
                                    ));
                                }
                            }
                            ui.end_row();
                        }
                    });
            });
        chosen
    }
}

/// Where the last painted cell for a pair sat, kept in egui memory so a story can click
/// it like a person would.
pub(crate) fn cell_memory_id(type_a: &str, type_b: &str) -> egui::Id {
    egui::Id::new(("pin-matrix-cell-rect", pin_matrix_key(type_a, type_b)))
}
