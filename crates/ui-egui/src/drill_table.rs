//! The drill table in Manufacture: one row per span, plating, function and size.
//!
//! # Rules
//! - The rows come from `pcbcraft_io::drill_table`, the same rows the drill map sheet and the readme print.
//! - The table is read only; it changes when the design does.
//!
//! # Dependencies
//! - `crates/io/src/drill_table.rs` `drill_table`, `DrillRow`.
//! - `crates/text/strings/en.txt` keys under `drill_table.`.
//!
//! # Do not
//! - Compute counts here; ask the io crate.
use super::*;
use crate::a11y::Named;

impl App {
    /// Draw the drill table as a collapsing section with a grid of rows.
    ///
    /// # Rules
    /// - Open by default; says so in words when the design has no holes.
    ///
    /// # Dependencies
    /// - crates/io/src/drill_table.rs `drill_table`.
    ///
    /// # Do not
    /// - Draw anything when the project is invalid; show the reason instead.
    pub(crate) fn drill_table_section(&mut self, ui: &mut egui::Ui) {
        let heading = t!("drill_table.title");
        let header = egui::CollapsingHeader::new(heading.clone())
            .id_salt("drill-table")
            .default_open(true)
            .show(ui, |ui| {
                ui.add_space(4.0);
                match pcbcraft_io::drill_table(&self.project) {
                    Err(reason) => {
                        ui.label(egui::RichText::new(reason).color(self.muted()));
                    }
                    Ok(rows) if rows.is_empty() => {
                        ui.label(egui::RichText::new(t!("drill_table.empty")).color(self.muted()));
                    }
                    Ok(rows) => {
                        let rel = crate::selection::relations(&self.project, &self.subject);
                        let accent = self.accent();
                        let mut picked: Option<crate::selection::DrillKey> = None;
                        egui::Grid::new("drill-table-grid")
                            .num_columns(6)
                            .spacing([14.0, 6.0])
                            .striped(true)
                            .show(ui, |ui| {
                                for key in [
                                    "drill_table.span",
                                    "drill_table.plated",
                                    "drill_table.function",
                                    "drill_table.diameter",
                                    "drill_table.count",
                                    "drill_table.tolerance",
                                ] {
                                    ui.label(
                                        egui::RichText::new(pcbcraft_text::lookup(key, &[]))
                                            .strong(),
                                    )
                                    .on_hover_text(
                                        pcbcraft_text::lookup(&format!("{key}.tip"), &[]),
                                    );
                                }
                                ui.end_row();
                                for r in &rows {
                                    let dk = crate::selection::DrillKey::new(
                                        r.from_layer,
                                        r.to_layer,
                                        r.diameter_mm,
                                    );
                                    let level = crate::selection::drill_level(
                                        &self.project,
                                        &self.subject,
                                        &rel,
                                        dk,
                                    );
                                    let name = format!(
                                        "L{} to L{} {}",
                                        r.from_layer,
                                        r.to_layer,
                                        r.kind.label().to_lowercase()
                                    );
                                    let mut text = egui::RichText::new(name.clone());
                                    if level == crate::selection::Level::Weak {
                                        text = text.color(accent);
                                    }
                                    let resp = ui
                                        .selectable_label(
                                            level == crate::selection::Level::Strong,
                                            text,
                                        )
                                        .on_hover_text(t!("drill_table.row.tip"))
                                        .named(&format!("Drill {name} {:.3} mm", r.diameter_mm));
                                    if resp.clicked() {
                                        picked = Some(dk);
                                    }
                                    ui.label(if r.plated {
                                        t!("drill_table.yes")
                                    } else {
                                        t!("drill_table.no")
                                    });
                                    ui.label(r.function.label());
                                    ui.label(if r.slot_mm > 0.0 {
                                        format!(
                                            "{:.3} x {:.3} mm",
                                            r.diameter_mm,
                                            r.diameter_mm + r.slot_mm
                                        )
                                    } else {
                                        format!("{:.3} mm", r.diameter_mm)
                                    });
                                    ui.label(r.count.to_string());
                                    ui.label(r.tolerance_mm.map_or(t!("drill_table.none"), |t| {
                                        format!("+/- {t:.3} mm")
                                    }));
                                    ui.end_row();
                                }
                            });
                        if let Some(dk) = picked {
                            self.select_subject(crate::selection::Selection::Artifact(dk.text()));
                        }
                    }
                }
                ui.add_space(8.0);
            });
        header
            .header_response
            .named(&heading)
            .on_hover_text(t!("drill_table.title.tip"));
    }
}
