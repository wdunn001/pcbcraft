//! The Probes card of the Scope page: every virtual probe with its point, reference and DC voltage.
//!
//! Context: probes are placed on the Schematic with the Probe tool and stored with the design.
//! The voltages come from the engine command `probe.read`, read once per design revision.
//!
//! Local rules: a voltage is shown only when the solver produced it. When the solve cannot run,
//! the card says why in the solver's own sentence and the rows read "No reading". Each row has a
//! Remove button that removes the probe as one undo step.
//!
//! # Dependencies
//! - crates/ui-egui/src/probe_tool.rs (`ProbeReading`, `ProbeRow`, `volts_text`)
//! - crates/ui-egui/src/ui_kit/card.rs (`card_flush`)
//! - crates/ui-egui/src/ui_kit/table.rs (`head`, `row`)
//! - crates/ui-egui/src/analyze/pending.rs (`plain_empty`)
//!
//! # Do not
//! - Run the solver here; read `App::probe_reading`.
//! - Draw a waveform; this card is DC only.
use super::*;
use crate::ui_kit::{
    card::card_flush,
    table::{self, Column},
};

impl App {
    /// The first card of the Scope page: a table of the design's probes.
    pub(super) fn analyze_probes_card(&mut self, ui: &mut egui::Ui) {
        let tokens = theme::tokens(ui.ctx());
        self.ensure_probe_reading();
        let rows = self
            .probe_reading
            .as_ref()
            .map(|r| r.rows.clone())
            .unwrap_or_default();
        let reason = self.probe_reading.as_ref().and_then(|r| r.reason.clone());
        let caption = t!("analyze.probes.caption");
        let mut remove: Option<Id> = None;
        card_flush(
            ui,
            &t!("analyze.probes.title"),
            Some(&caption),
            |_| {},
            |ui| {
                ui.add_space(8.0);
                if rows.is_empty() {
                    super::pending::plain_empty(
                        ui,
                        &t!("analyze.probes.empty"),
                        &t!("analyze.probes.empty_message"),
                    );
                    ui.add_space(12.0);
                    return;
                }
                if let Some(reason) = &reason {
                    ui.horizontal(|ui| {
                        ui.add_space(16.0);
                        ui.label(fonts::text(fonts::Role::Body, reason).color(tokens.gold));
                    });
                    ui.add_space(8.0);
                }
                egui::Frame::NONE
                    .inner_margin(egui::Margin::symmetric(12, 0))
                    .show(ui, |ui| {
                        let free = (ui.available_width() - 110.0 - 110.0 - 110.0 - 90.0).max(60.0);
                        let columns = [
                            Column {
                                title: &t!("analyze.probes.col_label"),
                                width: free,
                            },
                            Column {
                                title: &t!("analyze.probes.col_point"),
                                width: 110.0,
                            },
                            Column {
                                title: &t!("analyze.probes.col_reference"),
                                width: 110.0,
                            },
                            Column {
                                title: &t!("analyze.probes.col_voltage"),
                                width: 110.0,
                            },
                            Column {
                                title: "",
                                width: 0.0,
                            },
                        ];
                        table::head(ui, &columns);
                        for row in &rows {
                            table::row(ui, &columns, |cells| {
                                cells.cell(|ui| {
                                    ui.add(
                                        egui::Label::new(
                                            fonts::strong(&row.name).color(tokens.text),
                                        )
                                        .truncate(),
                                    );
                                });
                                cells.cell(|ui| {
                                    ui.label(fonts::mono(row.point.clone()).color(tokens.muted));
                                });
                                cells.cell(|ui| {
                                    let reference = row
                                        .reference
                                        .clone()
                                        .unwrap_or_else(|| t!("analyze.probes.ground"));
                                    ui.label(fonts::mono(reference).color(tokens.muted));
                                });
                                cells.cell(|ui| {
                                    let text = fonts::mono(row.value_text());
                                    let text = if row.volts.is_some() {
                                        text.color(tokens.text)
                                    } else {
                                        text.color(tokens.muted)
                                    };
                                    let shown = ui.label(text);
                                    if let Some(note) = &row.note {
                                        shown.on_hover_text(note);
                                    }
                                });
                                cells.cell(|ui| {
                                    if ghost_button(
                                        ui,
                                        None,
                                        &t!("analyze.probes.remove"),
                                        &t!("analyze.probes.remove.tip"),
                                        Size::Small,
                                        true,
                                    )
                                    .clicked()
                                    {
                                        remove = Some(row.id);
                                    }
                                });
                            });
                        }
                    });
                ui.add_space(8.0);
            },
        );
        if let Some(id) = remove {
            self.remove_probe(id);
        }
    }
}
