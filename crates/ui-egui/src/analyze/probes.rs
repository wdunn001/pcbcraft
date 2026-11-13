//! The Probes card of the Scope page: every virtual probe with its point, reference and DC voltage.
//!
//! Context: probes are placed on the Schematic or the Layout with the Probe tool and stored with the design.
//! The voltages come from the engine command `probe.read`, read once per design revision.
//!
//! Local rules: a voltage is shown only when the solver produced it. When the solve cannot run,
//! the card says why in the solver's own sentence and the rows read "No reading". Each row has a
//! Remove button that removes the probe as one undo step.
//!
//! Each row also has the range the probe should read: two boxes, Expected min and Expected max,
//! that stay empty ("No limit") until a bound is typed. Enter or leaving a box sets the range
//! through `probe.expect` as one undo step; an empty box drops that bound and, with both empty,
//! the range. A refused entry changes nothing and the card says why. The Status column shows
//! the engine's verdict in words and an icon: In range, Out of range, or Not checked.
//!
//! # Dependencies
//! - crates/ui-egui/src/probe_tool.rs (`ProbeReading`, `ProbeRow`, `ProbeStatus`, `volts_text`,
//!   `App::set_probe_expect`)
//! - crates/ui-egui/src/ui_kit/card.rs (`card_flush`)
//! - crates/ui-egui/src/ui_kit/table.rs (`head`, `row`)
//! - crates/ui-egui/src/analyze/pending.rs (`plain_empty`)
//!
//! # Do not
//! - Run the solver here; read `App::probe_reading`.
//! - Judge a reading here; the status comes from `probe.read`.
//! - Draw a waveform; this card is DC only.
use super::*;
use crate::probe_tool::ProbeStatus;
use crate::ui_kit::{
    card::card_flush,
    icons,
    table::{self, Column},
};
use pcbcraft_doc::MAX_PROBE_VOLTS;

/// Which end of the expected range a box edits.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Bound {
    Min,
    Max,
}

/// Where the last refused entry is kept so the card can say why on the next frame.
fn error_slot() -> egui::Id {
    egui::Id::new("probe-expect-error")
}

/// Read what a person typed into a bound box: empty drops the bound, a number sets it.
fn read_bound(text: &str) -> Result<Option<f64>, String> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    match crate::number_entry::parse_box(text) {
        Ok(v) if v.is_finite() && v.abs() <= MAX_PROBE_VOLTS => Ok(Some(v)),
        _ => Err(t!("analyze.probes.expect_invalid", limit = MAX_PROBE_VOLTS)),
    }
}

/// One editable bound. Returns the new bound when the person committed a different one, or
/// the plain sentence that says why the entry was refused.
///
/// # Rules
/// - The box shows the saved bound, or stays empty with a hint, until it is clicked into.
/// - Enter or leaving the box commits; Escape puts the saved bound back.
fn bound_box(
    ui: &mut egui::Ui,
    probe: Id,
    which: Bound,
    current: Option<f64>,
    name: &str,
    tip: &str,
) -> Option<Result<Option<f64>, String>> {
    let buffer = egui::Id::new(("probe-expect-buffer", probe, which));
    let mut editing: Option<String> = ui.data(|d| d.get_temp(buffer));
    let mut text = editing
        .clone()
        .unwrap_or_else(|| current.map(pcbcraft_doc::plain_volts).unwrap_or_default());
    let hint = t!("analyze.probes.no_limit");
    let response = ui.add(
        crate::targets::text_field(&mut text)
            .id(egui::Id::new(("probe-expect-box", probe, which)))
            .font(fonts::Role::Mono.font())
            .hint_text(fonts::text(fonts::Role::Mono, &hint).color(theme::tokens(ui.ctx()).muted))
            .desired_width(72.0),
    );
    {
        use crate::a11y::Named;
        response.clone().named(name).on_hover_text(tip);
    }
    if (response.gained_focus() && editing.is_none()) || response.changed() {
        editing = Some(text.clone());
    }
    if !response.lost_focus() {
        match editing {
            Some(held) => ui.data_mut(|d| d.insert_temp(buffer, held)),
            None => ui.data_mut(|d| {
                d.remove_temp::<String>(buffer);
            }),
        }
        return None;
    }
    ui.data_mut(|d| d.remove_temp::<String>(buffer));
    let escaped = ui.input(|i| i.key_pressed(egui::Key::Escape));
    if escaped || editing.is_none() {
        return None;
    }
    match read_bound(&text) {
        Ok(value) if value == current => None,
        Ok(value) => Some(Ok(value)),
        Err(why) => Some(Err(why)),
    }
}

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
        // A change of one bound: the probe, the new minimum and the new maximum.
        let mut set: Option<(Id, Option<f64>, Option<f64>)> = None;
        let mut refused: Option<String> = None;
        let shown_error: Option<String> = ui.data(|d| d.get_temp(error_slot()));
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
                if let Some(why) = &shown_error {
                    ui.horizontal(|ui| {
                        ui.add_space(16.0);
                        ui.label(fonts::text(fonts::Role::Body, why).color(tokens.red));
                    });
                    ui.add_space(8.0);
                }
                egui::Frame::NONE
                    .inner_margin(egui::Margin::symmetric(12, 0))
                    .show(ui, |ui| {
                        let free = (ui.available_width() - 90.0 * 3.0 - 84.0 * 2.0 - 116.0 - 90.0)
                            .max(60.0);
                        let columns = [
                            Column {
                                title: &t!("analyze.probes.col_label"),
                                width: free,
                            },
                            Column {
                                title: &t!("analyze.probes.col_point"),
                                width: 90.0,
                            },
                            Column {
                                title: &t!("analyze.probes.col_reference"),
                                width: 90.0,
                            },
                            Column {
                                title: &t!("analyze.probes.col_voltage"),
                                width: 90.0,
                            },
                            Column {
                                title: &t!("analyze.probes.col_expect_min"),
                                width: 84.0,
                            },
                            Column {
                                title: &t!("analyze.probes.col_expect_max"),
                                width: 84.0,
                            },
                            Column {
                                title: &t!("analyze.probes.col_status"),
                                width: 116.0,
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
                                for which in [Bound::Min, Bound::Max] {
                                    cells.cell(|ui| {
                                        let (current, name, tip) = match which {
                                            Bound::Min => (
                                                row.min,
                                                t!(
                                                    "analyze.probes.expect_min_named",
                                                    name = row.name.clone()
                                                ),
                                                t!("analyze.probes.expect_min_named.tip"),
                                            ),
                                            Bound::Max => (
                                                row.max,
                                                t!(
                                                    "analyze.probes.expect_max_named",
                                                    name = row.name.clone()
                                                ),
                                                t!("analyze.probes.expect_max_named.tip"),
                                            ),
                                        };
                                        match bound_box(ui, row.id, which, current, &name, &tip) {
                                            Some(Ok(value)) => {
                                                set = Some(match which {
                                                    Bound::Min => (row.id, value, row.max),
                                                    Bound::Max => (row.id, row.min, value),
                                                });
                                            }
                                            Some(Err(why)) => refused = Some(why),
                                            None => {}
                                        }
                                    });
                                }
                                cells.cell(|ui| {
                                    let (words, ink, icon) = match row.status {
                                        ProbeStatus::Pass => (
                                            t!("analyze.probes.status_pass"),
                                            tokens.mask,
                                            Some(icons::OK),
                                        ),
                                        ProbeStatus::Fail => (
                                            t!("analyze.probes.status_fail"),
                                            tokens.red,
                                            Some(icons::WARNING),
                                        ),
                                        ProbeStatus::Unchecked => (
                                            t!("analyze.probes.status_unchecked"),
                                            tokens.muted,
                                            None,
                                        ),
                                    };
                                    if let Some(icon) = icon {
                                        let (rect, _) = ui.allocate_exact_size(
                                            egui::Vec2::splat(icons::ICON_SIZE),
                                            egui::Sense::hover(),
                                        );
                                        (icon.0)(ui.painter(), rect, ink);
                                    }
                                    let shown =
                                        ui.label(fonts::text(fonts::Role::Body, &words).color(ink));
                                    if let Some(why) = &row.status_reason {
                                        shown.on_hover_text(why);
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
        if let Some((id, min, max)) = set {
            refused = self.set_probe_expect(id, min, max).err();
            if refused.is_none() {
                ui.data_mut(|d| d.remove_temp::<String>(error_slot()));
            }
        }
        if let Some(why) = refused {
            ui.data_mut(|d| d.insert_temp(error_slot(), why));
        }
    }
}
