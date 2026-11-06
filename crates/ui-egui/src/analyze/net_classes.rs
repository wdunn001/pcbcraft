//! The Net classes page of the Analyze tab: the design's net classes, read only.
//!
//! Context: issue #1421. The rows are `Project::net_classes`. Classes are made, changed and removed
//! in Project settings, on the Net classes tab; this page reads them and opens that tab with one button,
//! so there is one place that edits a class and one undo history.
//!
//! Local rules: a value a class does not set is shown as not set, never as a default of our own.
//!
//! # Dependencies
//! - crates/doc/src/lib.rs (`NetClass`)
//! - crates/ui-egui/src/project_settings/mod.rs (`open_project_settings`, `TAB_CLASSES`)
//! - crates/ui-egui/src/ui_kit/rows.rs (`table_head`, `table_row`)
//! - crates/ui-egui/src/ui_kit/card.rs (`card_flush`)
//!
//! # Do not
//! - Edit a class here; send the person to Project settings.
use super::*;
use crate::ui_kit::{
    card::card_flush,
    empty_state::empty_state,
    form_row::{FormWidth, form_row},
    rows::{table_head, table_row},
};

/// Column weights of the table.
const WEIGHTS: [f32; 6] = [2.4, 1.4, 1.4, 1.4, 2.2, 1.0];

/// A millimetre value for a cell, or the not-set words.
fn mm(value: Option<f64>) -> String {
    match value {
        Some(v) => t!("analyze.value.mm", value = format!("{v:.2}")),
        None => t!("analyze.not_set"),
    }
}

impl App {
    /// The table of classes with its Edit button, or an empty state when there are none.
    pub(super) fn analyze_classes_center(&mut self, ui: &mut egui::Ui) {
        let tokens = theme::tokens(ui.ctx());
        let classes = self.project.net_classes.clone();
        self.analyze.class = self.analyze.class.min(classes.len().saturating_sub(1));
        let caption = t!("analyze.classes.caption");
        let mut open = false;
        card_flush(
            ui,
            &t!("analyze.entry.net_classes"),
            Some(&caption),
            |_| {},
            |ui| {
                if classes.is_empty() {
                    ui.add_space(16.0);
                    open |= empty_state(
                        ui,
                        crate::ui_kit::icons::LAYERS,
                        &t!("analyze.classes.empty"),
                        &t!("analyze.classes.empty.message"),
                        &t!("analyze.classes.edit"),
                        &t!("analyze.classes.edit.tip"),
                    )
                    .clicked();
                    ui.add_space(16.0);
                    return;
                }
                let heads = [
                    t!("analyze.classes.col_class"),
                    t!("analyze.classes.col_width"),
                    t!("analyze.classes.col_clearance"),
                    t!("analyze.classes.col_via"),
                    t!("analyze.classes.col_impedance"),
                    t!("analyze.classes.col_nets"),
                ];
                let names: Vec<&str> = heads.iter().map(String::as_str).collect();
                table_head(ui, &WEIGHTS, &names);
                for (index, class) in classes.iter().enumerate() {
                    let tip = t!("analyze.classes.row.tip");
                    let response = table_row(
                        ui,
                        index == self.analyze.class,
                        &class.name,
                        &tip,
                        &WEIGHTS,
                        |ui, column| {
                            let text = match column {
                                0 => fonts::strong(&class.name),
                                1 => fonts::mono(mm(Some(class.width))),
                                2 => fonts::mono(mm(Some(class.clearance))),
                                3 => fonts::mono(mm(class.via_drill)),
                                4 => fonts::mono(match class.impedance_target {
                                    Some(z) => t!("analyze.value.ohm", value = format!("{z:.0}")),
                                    None => t!("analyze.not_set"),
                                }),
                                _ => fonts::mono(class.nets.len().to_string()),
                            };
                            ui.add(egui::Label::new(text.color(tokens.text)).selectable(false));
                        },
                    );
                    if response.clicked() {
                        self.analyze.class = index;
                    }
                }
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    ui.add_space(12.0);
                    open |= secondary_button(
                        ui,
                        Some(crate::ui_kit::icons::GEAR),
                        &t!("analyze.classes.edit"),
                        &t!("analyze.classes.edit.tip"),
                        Size::Normal,
                        true,
                    )
                    .clicked();
                });
                ui.add_space(12.0);
            },
        );
        if open {
            self.open_project_settings(project_settings::TAB_CLASSES);
        }
    }

    /// Properties of the picked class: name, nets, geometry and signal integrity values.
    pub(super) fn analyze_classes_properties(&mut self, ui: &mut egui::Ui) {
        let tokens = theme::tokens(ui.ctx());
        let Some(class) = self.project.net_classes.get(self.analyze.class).cloned() else {
            ui.label(
                fonts::text(fonts::Role::Body, t!("analyze.classes.props_empty"))
                    .color(tokens.muted),
            );
            return;
        };
        let nets: Vec<String> = class
            .nets
            .iter()
            .map(|id| self.project.net_name(*id).to_owned())
            .collect();
        let nets = if nets.is_empty() {
            t!("analyze.not_set")
        } else {
            nets.join(", ")
        };
        let sections: [(String, Vec<(String, String)>); 3] = [
            (
                t!("analyze.classes.props_class"),
                vec![
                    (t!("analyze.classes.props_name"), class.name.clone()),
                    (t!("analyze.classes.props_nets"), nets),
                ],
            ),
            (
                t!("analyze.classes.props_geometry"),
                vec![
                    (t!("analyze.classes.props_width"), mm(Some(class.width))),
                    (
                        t!("analyze.classes.props_clearance"),
                        mm(Some(class.clearance)),
                    ),
                    (t!("analyze.classes.props_via_drill"), mm(class.via_drill)),
                ],
            ),
            (
                t!("analyze.classes.props_si"),
                vec![
                    (
                        t!("analyze.classes.props_impedance"),
                        class.impedance_target.map_or_else(
                            || t!("analyze.not_set"),
                            |z| t!("analyze.value.ohm", value = format!("{z:.0}")),
                        ),
                    ),
                    (
                        t!("analyze.classes.props_tolerance"),
                        class.impedance_tolerance_pct.map_or_else(
                            || t!("analyze.not_set"),
                            |p| t!("analyze.value.percent", value = format!("{p:.0}")),
                        ),
                    ),
                    (
                        t!("analyze.classes.props_rise"),
                        class.rise_time.map_or_else(
                            || t!("analyze.not_set"),
                            |s| t!("analyze.value.ns", value = format!("{:.2}", s * 1e9)),
                        ),
                    ),
                ],
            ),
        ];
        for (title, rows) in sections {
            ui.label(fonts::section_caption(title).color(tokens.muted));
            for (label, value) in rows {
                form_row(ui, FormWidth::Aside, &label, None, None, |ui| {
                    ui.label(fonts::text(fonts::Role::Body, &value).color(tokens.text));
                });
            }
        }
    }
}
