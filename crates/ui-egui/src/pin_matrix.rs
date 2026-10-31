//! Pin-type connection matrix on the Checks tab: a grid of the twelve pin
//! types where each cell says whether two such pins on one net is fine, a
//! warning or an error. Levels are judged by the rules crate; this file only
//! shows them and sends `erc.matrix_set` and `erc.matrix_reset`.
//!
//! Local rules: only the lower triangle is drawn because a cell and its mirror
//! are the same setting; never write the project here, always go through
//! `App::run` so Undo works.
use super::*;
use pcbcraft_doc::{PIN_ELECTRICAL_TYPES, default_pin_matrix_level, pin_matrix_key};

/// Plain words for one pin type: (full name, short column title, what it means).
fn type_words(pin_type: &str) -> (String, String, String) {
    match pin_type {
        "input" => (
            t!("checks.pin_matrix.type.input"),
            t!("checks.pin_matrix.short.input"),
            t!("checks.pin_matrix.type.input.tip"),
        ),
        "output" => (
            t!("checks.pin_matrix.type.output"),
            t!("checks.pin_matrix.short.output"),
            t!("checks.pin_matrix.type.output.tip"),
        ),
        "bidirectional" => (
            t!("checks.pin_matrix.type.bidirectional"),
            t!("checks.pin_matrix.short.bidirectional"),
            t!("checks.pin_matrix.type.bidirectional.tip"),
        ),
        "tri_state" => (
            t!("checks.pin_matrix.type.tri_state"),
            t!("checks.pin_matrix.short.tri_state"),
            t!("checks.pin_matrix.type.tri_state.tip"),
        ),
        "passive" => (
            t!("checks.pin_matrix.type.passive"),
            t!("checks.pin_matrix.short.passive"),
            t!("checks.pin_matrix.type.passive.tip"),
        ),
        "power_in" => (
            t!("checks.pin_matrix.type.power_in"),
            t!("checks.pin_matrix.short.power_in"),
            t!("checks.pin_matrix.type.power_in.tip"),
        ),
        "power_out" => (
            t!("checks.pin_matrix.type.power_out"),
            t!("checks.pin_matrix.short.power_out"),
            t!("checks.pin_matrix.type.power_out.tip"),
        ),
        "open_collector" => (
            t!("checks.pin_matrix.type.open_collector"),
            t!("checks.pin_matrix.short.open_collector"),
            t!("checks.pin_matrix.type.open_collector.tip"),
        ),
        "open_emitter" => (
            t!("checks.pin_matrix.type.open_emitter"),
            t!("checks.pin_matrix.short.open_emitter"),
            t!("checks.pin_matrix.type.open_emitter.tip"),
        ),
        "free" => (
            t!("checks.pin_matrix.type.free"),
            t!("checks.pin_matrix.short.free"),
            t!("checks.pin_matrix.type.free.tip"),
        ),
        "no_connect" => (
            t!("checks.pin_matrix.type.no_connect"),
            t!("checks.pin_matrix.short.no_connect"),
            t!("checks.pin_matrix.type.no_connect.tip"),
        ),
        _ => (
            t!("checks.pin_matrix.type.unspecified"),
            t!("checks.pin_matrix.short.unspecified"),
            t!("checks.pin_matrix.type.unspecified.tip"),
        ),
    }
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

/// Where the last painted cell for a pair sat, kept in egui memory so a test
/// can click it like a person would.
pub(crate) fn cell_memory_id(type_a: &str, type_b: &str) -> egui::Id {
    egui::Id::new(("pin-matrix-cell-rect", pin_matrix_key(type_a, type_b)))
}

impl App {
    /// The collapsible matrix section at the end of the Checks tab.
    ///
    /// # Rules
    /// - Edits go through `erc.matrix_set` and `erc.matrix_reset` only.
    /// - Cells that differ from the default table are marked with a star.
    ///
    /// # Dependencies
    /// - `crates/doc/src/lib.rs` `pin_matrix_level`, `default_pin_matrix_level`.
    /// - `crates/engine/src/lib.rs` commands `erc.matrix_set`, `erc.matrix_reset`.
    ///
    /// # Do not
    /// - Compute findings here; `crates/rules/src/lib.rs` owns that.
    pub(crate) fn pin_matrix_section(&mut self, ui: &mut egui::Ui) {
        ui.add_space(12.0);
        let header = egui::CollapsingHeader::new(
            egui::RichText::new(t!("checks.pin_matrix.title"))
                .size(18.0)
                .strong(),
        )
        .default_open(true)
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(t!("checks.pin_matrix.blurb"))
                    .small()
                    .color(self.muted()),
            );
            ui.add_space(8.0);
            let changed = !self.project.pin_matrix.is_empty();
            ui.add_enabled_ui(changed, |ui| {
                if ui
                    .tap_button(t!("checks.pin_matrix.reset"))
                    .on_hover_text(t!("checks.pin_matrix.reset.tip"))
                    .clicked()
                {
                    self.run("erc.matrix_reset", json!({}));
                }
            });
            ui.add_space(8.0);
            let mut chosen: Option<(String, String, String)> = None;
            egui::ScrollArea::horizontal()
                .id_salt("pin-matrix-scroll")
                .show(ui, |ui| {
                    egui::Grid::new("pin-matrix-grid")
                        .spacing(egui::vec2(4.0, 4.0))
                        .show(ui, |ui| {
                            ui.label("");
                            for column in PIN_ELECTRICAL_TYPES {
                                let (_, short, tip) = type_words(column);
                                ui.label(egui::RichText::new(short).small())
                                    .on_hover_text(tip);
                            }
                            ui.end_row();
                            for (row_index, row) in PIN_ELECTRICAL_TYPES.iter().enumerate() {
                                let (name, _, tip) = type_words(row);
                                ui.label(name).on_hover_text(tip);
                                for (column_index, column) in
                                    PIN_ELECTRICAL_TYPES.iter().enumerate()
                                {
                                    if column_index > row_index {
                                        ui.label("");
                                        continue;
                                    }
                                    let level = self.project.pin_matrix_level(row, column);
                                    let custom = level != default_pin_matrix_level(row, column);
                                    let (row_name, _, _) = type_words(row);
                                    let (column_name, _, _) = type_words(column);
                                    let word = level_word(&level);
                                    let face = if custom {
                                        format!("{word} *")
                                    } else {
                                        word.clone()
                                    };
                                    let tint = match level.as_str() {
                                        "error" => egui::Color32::from_rgb(150, 50, 50),
                                        "warning" => egui::Color32::from_rgb(150, 110, 30),
                                        _ => egui::Color32::from_rgb(40, 100, 60),
                                    };
                                    let response = ui
                                        .add(
                                            egui::Button::new(
                                                egui::RichText::new(face)
                                                    .small()
                                                    .color(egui::Color32::WHITE),
                                            )
                                            .fill(tint)
                                            .min_size(egui::vec2(60.0, 24.0)),
                                        )
                                        .on_hover_text(if custom {
                                            t!(
                                                "checks.pin_matrix.cell.changed.tip",
                                                a = row_name,
                                                b = column_name,
                                                level = word.clone(),
                                                default = level_word(default_pin_matrix_level(
                                                    row, column
                                                ))
                                            )
                                        } else {
                                            t!(
                                                "checks.pin_matrix.cell.tip",
                                                a = row_name,
                                                b = column_name,
                                                level = word.clone()
                                            )
                                        });
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
            if let Some((type_a, type_b, level)) = chosen {
                self.run(
                    "erc.matrix_set",
                    json!({"type_a": type_a, "type_b": type_b, "level": level}),
                );
            }
        });
        header
            .header_response
            .on_hover_text(t!("checks.pin_matrix.title.tip"));
        ui.add_space(12.0);
    }
}
