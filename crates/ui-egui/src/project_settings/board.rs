//! Board and stackup tab: board size, finish and the layer table.
//!
//! Context: mockup `dialog-project-board`. Size, finish and the plated edge are board
//! facts; the stackup card edits the recorded layer rows (material, thickness, Dk, Df,
//! copper weight) and the number of copper layers.
//! Local rules: layer rows are written with `board.set_layer_record`, which wants name,
//! kind and thickness on every call, so `record_params` always sends all three.
//! The copper count changes only through `board.stackup_add_pair` and
//! `board.stackup_remove_layer`, which keep copper already drawn on its layer.
//!
//! # Dependencies
//! - crates/doc/src/lib.rs (`LayerRecord`), crates/doc/src/material.rs (`MATERIALS`)
//! - crates/engine/src/lib.rs (`board.resize`, `board.set_finish`, `board.set_stackup`,
//!   `board.set_machining`, `board.set_layer_record`, `board.remove_layer_record`,
//!   `board.stackup_add_pair`, `board.stackup_remove_layer`, `board.clear_layer_records`)
//!
//! # Do not
//! - Offer corner shapes; no engine command sets them yet (see the PR notes).
use super::{mm_spec, note, num, secondary};
use crate::ui_kit::card::card;
use crate::ui_kit::form_row::{FormWidth, form_row, toggle_row};
use crate::ui_kit::number_field::NumberSpec;
use crate::ui_kit::select::select;
use crate::{App, STACK_ROLES, fonts, theme};
use pcbcraft_doc::{LayerRecord, MATERIALS};
use pcbcraft_text::t;
use serde_json::{Value, json};

/// Surface finishes offered; a project may already carry another name.
const FINISHES: [&str; 5] = ["HASL", "Lead-free HASL", "ENIG", "OSP", "Immersion silver"];

/// A starting stackup: catalog key stem and its rows (name, kind, thickness, copper oz).
type Shape = (&'static str, &'static [(&'static str, &'static str, f64)]);

/// The shapes the Start from list offers.
const SHAPES: [Shape; 3] = [
    (
        "two",
        &[
            ("Top copper", "copper", 0.035),
            ("Core", "core", 1.53),
            ("Bottom copper", "copper", 0.035),
        ],
    ),
    (
        "thin",
        &[
            ("Top copper", "copper", 0.035),
            ("Core", "core", 0.93),
            ("Bottom copper", "copper", 0.035),
        ],
    ),
    (
        "four",
        &[
            ("Top copper", "copper", 0.035),
            ("Prepreg", "prepreg", 0.2),
            ("Inner 1", "copper", 0.035),
            ("Core", "core", 0.86),
            ("Inner 2", "copper", 0.035),
            ("Prepreg 2", "prepreg", 0.2),
            ("Bottom copper", "copper", 0.035),
        ],
    ),
];

/// The parameters `board.set_layer_record` needs for a row, with one field changed.
fn record_params(record: &LayerRecord, change: Value) -> Value {
    let mut params = json!({
        "name": record.name,
        "kind": record.kind,
        "thickness": record.thickness,
    });
    if let (Some(map), Some(extra)) = (params.as_object_mut(), change.as_object()) {
        for (key, value) in extra {
            map.insert(key.clone(), value.clone());
        }
    }
    params
}

impl App {
    /// The Board and stackup tab.
    ///
    /// # Rules
    /// - Width and height run `board.resize`; the canvas follows on the same frame.
    ///
    /// # Dependencies
    /// - crates/ui-egui/src/ui_kit/card.rs
    ///
    /// # Do not
    /// - Keep a second copy of the size; read it from the project each frame.
    pub(super) fn settings_board(&mut self, ui: &mut egui::Ui) {
        ui.columns(2, |columns| {
            self.settings_board_size(&mut columns[0]);
            self.settings_board_finish(&mut columns[1]);
        });
        ui.add_space(8.0);
        self.settings_stackup(ui);
    }

    fn settings_board_size(&mut self, ui: &mut egui::Ui) {
        let (w, h) = (self.project.board.width, self.project.board.height);
        card(
            ui,
            &t!("settings.project.board.size"),
            None,
            |_| {},
            |ui| {
                let spec = NumberSpec {
                    min: 1.0,
                    max: 2000.0,
                    step: 0.5,
                    places: 2,
                    ..mm_spec(1.0, 2000.0)
                };
                let name = t!("settings.project.board.width");
                if let Some(v) = form_row(ui, FormWidth::Page, &name, None, None, |ui| {
                    num(
                        ui,
                        "ps-board-w",
                        w,
                        &spec,
                        &name,
                        &t!("settings.project.board.width.tip"),
                    )
                }) {
                    self.settings_run("board.resize", json!({"width": v, "height": h}));
                }
                let name = t!("settings.project.board.height");
                if let Some(v) = form_row(ui, FormWidth::Page, &name, None, None, |ui| {
                    num(
                        ui,
                        "ps-board-h",
                        h,
                        &spec,
                        &name,
                        &t!("settings.project.board.height.tip"),
                    )
                }) {
                    self.settings_run("board.resize", json!({"width": w, "height": v}));
                }
            },
        );
    }

    fn settings_board_finish(&mut self, ui: &mut egui::Ui) {
        let board = self.project.board.clone();
        let total: f64 = board.layer_records.iter().map(|r| r.thickness).sum();
        card(
            ui,
            &t!("settings.project.finish"),
            None,
            |_| {},
            |ui| {
                let mut options: Vec<&str> = FINISHES.to_vec();
                if !options.contains(&board.finish.as_str()) {
                    options.push(board.finish.as_str());
                }
                let mut at = options.iter().position(|f| *f == board.finish).unwrap_or(0);
                let name = t!("settings.project.finish.surface");
                let changed = form_row(
                    ui,
                    FormWidth::Page,
                    &name,
                    Some(&t!("settings.project.finish.surface.hint")),
                    None,
                    |ui| {
                        select(
                            ui,
                            "ps-finish",
                            &mut at,
                            &options,
                            &name,
                            &t!("settings.project.finish.surface.tip"),
                            168.0,
                        )
                        .changed()
                    },
                );
                if changed {
                    self.settings_run("board.set_finish", json!({"finish": options[at]}));
                }
                let name = t!("settings.project.finish.dk");
                let spec = NumberSpec {
                    unit: "",
                    min: 1.0,
                    max: 20.0,
                    step: 0.1,
                    places: 2,
                    width: 104.0,
                };
                if let Some(v) = form_row(
                    ui,
                    FormWidth::Page,
                    &name,
                    Some(&t!("settings.project.finish.dk.hint")),
                    None,
                    |ui| {
                        num(
                            ui,
                            "ps-board-er",
                            board.er,
                            &spec,
                            &name,
                            &t!("settings.project.finish.dk.tip"),
                        )
                    },
                ) {
                    self.settings_run("board.set_stackup", json!({"er": v}));
                }
                let mut plated =
                    board.machining.as_ref().and_then(|m| m.edge_plating) == Some(true);
                let before = plated;
                toggle_row(
                    ui,
                    FormWidth::Page,
                    &t!("settings.project.finish.plated"),
                    &t!("settings.project.finish.plated.tip"),
                    &mut plated,
                );
                if plated != before {
                    self.settings_run("board.set_machining", json!({"edge_plating": plated}));
                }
                let name = t!("settings.project.finish.thickness");
                if board.layer_records.is_empty() {
                    if let Some(v) = form_row(ui, FormWidth::Page, &name, None, None, |ui| {
                        num(
                            ui,
                            "ps-board-thickness",
                            board.thickness,
                            &mm_spec(0.05, 20.0),
                            &name,
                            &t!("settings.project.finish.thickness.tip"),
                        )
                    }) {
                        self.settings_run("board.set_stackup", json!({"thickness": v}));
                    }
                } else {
                    form_row(ui, FormWidth::Page, &name, None, None, |ui| {
                        let text = theme::tokens(ui.ctx()).text;
                        ui.label(fonts::mono(format!("{total:.2} mm")).color(text))
                            .on_hover_text(t!("settings.project.finish.thickness.sum.tip"));
                    });
                }
            },
        );
    }

    fn settings_stackup(&mut self, ui: &mut egui::Ui) {
        let records = self.project.board.layer_records.clone();
        let copper: Vec<&LayerRecord> = records.iter().filter(|r| r.kind == "copper").collect();
        let count = copper.len().max(2);
        let inner: Vec<String> = if copper.len() > 2 {
            copper[1..copper.len() - 1]
                .iter()
                .map(|r| r.name.clone())
                .collect()
        } else {
            Vec::new()
        };
        card(
            ui,
            &t!("settings.project.stack"),
            Some(&t!("settings.project.stack.caption")),
            |_| {},
            |ui| {
                let name = t!("settings.project.stack.start");
                let labels: Vec<String> = std::iter::once(t!("settings.project.stack.choose"))
                    .chain(SHAPES.iter().map(|(stem, _)| {
                        pcbcraft_text::lookup(&format!("settings.project.stack.shape.{stem}"), &[])
                    }))
                    .collect();
                let options: Vec<&str> = labels.iter().map(String::as_str).collect();
                let mut at = 0;
                let picked = form_row(
                    ui,
                    FormWidth::Page,
                    &name,
                    Some(&t!("settings.project.stack.start.hint")),
                    None,
                    |ui| {
                        select(
                            ui,
                            "ps-stack-start",
                            &mut at,
                            &options,
                            &name,
                            &t!("settings.project.stack.start.tip"),
                            220.0,
                        )
                        .changed()
                    },
                );
                if picked && at > 0 {
                    let mut steps: Vec<(&str, Value)> =
                        vec![("board.clear_layer_records", json!({}))];
                    for (row, kind, thickness) in SHAPES[at - 1].1 {
                        let mut params = json!({"name": row, "kind": kind, "thickness": thickness});
                        if *kind == "copper"
                            && let Some(map) = params.as_object_mut()
                        {
                            map.insert("copper_weight_oz".into(), json!(1.0));
                        }
                        steps.push(("board.set_layer_record", params));
                    }
                    self.settings_batch(steps);
                }
                form_row(
                    ui,
                    FormWidth::Page,
                    &t!("settings.project.stack.copper"),
                    Some(&t!("settings.project.stack.copper.hint")),
                    None,
                    |ui| {
                        let text = theme::tokens(ui.ctx()).text;
                        ui.label(fonts::mono(count.to_string()).color(text))
                            .on_hover_text(t!("settings.project.stack.copper.tip"));
                        ui.add_space(12.0);
                        let after = copper
                            .first()
                            .map_or_else(|| "F.Cu".to_owned(), |r| r.name.clone());
                        if secondary(
                            ui,
                            &t!("settings.project.stack.add_pair"),
                            &t!("settings.project.stack.add_pair.tip"),
                            count < 30,
                        )
                        .clicked()
                        {
                            self.settings_run("board.stackup_add_pair", json!({"after": after}));
                        }
                        let tip = if inner.is_empty() {
                            t!("settings.project.stack.remove_pair.disabled_tip")
                        } else {
                            t!("settings.project.stack.remove_pair.tip")
                        };
                        if secondary(
                            ui,
                            &t!("settings.project.stack.remove_pair"),
                            &tip,
                            !inner.is_empty(),
                        )
                        .clicked()
                        {
                            let steps: Vec<(&str, Value)> = inner
                                .iter()
                                .rev()
                                .take(2)
                                .map(|n| ("board.stackup_remove_layer", json!({"name": n})))
                                .collect();
                            self.settings_batch(steps);
                        }
                    },
                );
                ui.add_space(8.0);
                if records.is_empty() {
                    note(ui, t!("settings.project.stack.empty"));
                } else {
                    self.settings_layer_table(ui, &records);
                }
                ui.add_space(8.0);
                self.settings_add_layer(ui);
            },
        );
    }

    /// One grid row per recorded layer: material, thickness, Dk, Df and copper weight.
    fn settings_layer_table(&mut self, ui: &mut egui::Ui, records: &[LayerRecord]) {
        let muted = theme::tokens(ui.ctx()).muted;
        let er = self.project.board.er;
        let mut writes: Vec<(&str, Value)> = Vec::new();
        egui::ScrollArea::horizontal()
            .id_salt("ps-stack-scroll")
            .show(ui, |ui| {
                egui::Grid::new("ps-stack-grid")
                    .spacing(egui::vec2(8.0, 6.0))
                    .show(ui, |ui| {
                        for head in [
                            "layer",
                            "role",
                            "material",
                            "thickness",
                            "dk",
                            "df",
                            "oz",
                            "",
                        ] {
                            if head.is_empty() {
                                ui.label("");
                            } else {
                                ui.label(
                                    fonts::section_caption(pcbcraft_text::lookup(
                                        &format!("settings.project.stack.col.{head}"),
                                        &[],
                                    ))
                                    .color(muted),
                                );
                            }
                        }
                        ui.end_row();
                        for (i, r) in records.iter().enumerate() {
                            let is_copper = r.kind == "copper";
                            ui.label(fonts::text(fonts::Role::Body, &r.name));
                            ui.label(
                                fonts::caption(pcbcraft_text::lookup(
                                    &format!("settings.project.role.{}", r.kind),
                                    &[],
                                ))
                                .color(muted),
                            );
                            if is_copper {
                                ui.label(
                                    fonts::caption(t!("settings.project.stack.copper_name"))
                                        .color(muted),
                                );
                            } else {
                                let mut labels: Vec<String> =
                                    vec![t!("settings.project.stack.no_material")];
                                labels.extend(MATERIALS.iter().map(|m| m.name.to_owned()));
                                let options: Vec<&str> =
                                    labels.iter().map(String::as_str).collect();
                                let mut at = r
                                    .material
                                    .as_deref()
                                    .and_then(|m| MATERIALS.iter().position(|x| x.name == m))
                                    .map_or(0, |p| p + 1);
                                let before = at;
                                select(
                                    ui,
                                    ("ps-material", i),
                                    &mut at,
                                    &options,
                                    &t!("settings.project.stack.col.material"),
                                    &t!("settings.project.stack.material.tip"),
                                    168.0,
                                );
                                if at != before && at > 0 {
                                    let m = &MATERIALS[at - 1];
                                    writes.push((
                                        "board.set_layer_record",
                                        record_params(
                                            r,
                                            json!({"material": m.name, "dk": m.dk, "df": m.df}),
                                        ),
                                    ));
                                }
                            }
                            let label = format!(
                                "{} {}",
                                r.name,
                                t!("settings.project.stack.col.thickness")
                            );
                            if let Some(v) = num(
                                ui,
                                ("ps-thick", i),
                                r.thickness,
                                &mm_spec(0.0, 20.0),
                                &label,
                                &t!("settings.project.stack.thickness.tip"),
                            ) {
                                writes.push((
                                    "board.set_layer_record",
                                    record_params(r, json!({"thickness": v})),
                                ));
                            }
                            if is_copper {
                                ui.label(fonts::caption("-").color(muted));
                                ui.label(fonts::caption("-").color(muted));
                                let oz = NumberSpec {
                                    unit: "oz",
                                    min: 0.1,
                                    max: 10.0,
                                    step: 0.5,
                                    places: 1,
                                    width: 84.0,
                                };
                                let label =
                                    format!("{} {}", r.name, t!("settings.project.stack.col.oz"));
                                if let Some(v) = num(
                                    ui,
                                    ("ps-oz", i),
                                    r.copper_weight_oz.unwrap_or(1.0),
                                    &oz,
                                    &label,
                                    &t!("settings.project.stack.oz.tip"),
                                ) {
                                    writes.push((
                                        "board.set_layer_record",
                                        record_params(r, json!({"copper_weight_oz": v})),
                                    ));
                                }
                            } else {
                                let material =
                                    r.material.as_deref().and_then(pcbcraft_doc::material);
                                let dk = r.dk.or(material.map(|m| m.dk)).unwrap_or(er);
                                let df = r.df.or(material.map(|m| m.df)).unwrap_or(0.02);
                                let dks = NumberSpec {
                                    unit: "",
                                    min: 1.0,
                                    max: 20.0,
                                    step: 0.1,
                                    places: 2,
                                    width: 84.0,
                                };
                                let dfs = NumberSpec {
                                    unit: "",
                                    min: 0.0,
                                    max: 1.0,
                                    step: 0.001,
                                    places: 3,
                                    width: 84.0,
                                };
                                let label = format!("{} Dk", r.name);
                                if let Some(v) = num(
                                    ui,
                                    ("ps-dk", i),
                                    dk,
                                    &dks,
                                    &label,
                                    &t!("settings.project.stack.dk.tip"),
                                ) {
                                    writes.push((
                                        "board.set_layer_record",
                                        record_params(r, json!({"dk": v})),
                                    ));
                                }
                                let label = format!("{} Df", r.name);
                                if let Some(v) = num(
                                    ui,
                                    ("ps-df", i),
                                    df,
                                    &dfs,
                                    &label,
                                    &t!("settings.project.stack.df.tip"),
                                ) {
                                    writes.push((
                                        "board.set_layer_record",
                                        record_params(r, json!({"df": v})),
                                    ));
                                }
                                ui.label(fonts::caption("-").color(muted));
                            }
                            let removable = !is_copper;
                            let tip = if removable {
                                t!("settings.project.stack.remove.tip")
                            } else {
                                t!("settings.project.stack.remove.copper_tip")
                            };
                            if super::ghost(
                                ui,
                                &t!("settings.project.stack.remove"),
                                &tip,
                                removable,
                            )
                            .clicked()
                            {
                                writes.push(("board.remove_layer_record", json!({"name": r.name})));
                            }
                            ui.end_row();
                        }
                    });
            });
        if let Some((command, params)) = writes.into_iter().next() {
            self.settings_run(command, params);
        }
    }

    /// The Add layer row: name, role, thickness and the button.
    fn settings_add_layer(&mut self, ui: &mut egui::Ui) {
        use crate::a11y::Named;
        let name_label = t!("settings.project.stack.new.name");
        ui.horizontal(|ui| {
            let state = &mut self.project_settings;
            ui.add(
                crate::targets::text_field(&mut state.layer_name)
                    .hint_text(&name_label)
                    .desired_width(160.0),
            )
            .named(&name_label)
            .on_hover_text(t!("settings.project.stack.new.name.tip"));
            let kinds: Vec<(&str, String)> = STACK_ROLES
                .iter()
                .filter(|(token, _)| *token != "copper")
                .map(|(token, _)| {
                    (
                        *token,
                        pcbcraft_text::lookup(&format!("settings.project.role.{token}"), &[]),
                    )
                })
                .collect();
            let options: Vec<&str> = kinds.iter().map(|(_, l)| l.as_str()).collect();
            state.layer_kind = state.layer_kind.min(kinds.len() - 1);
            select(
                ui,
                "ps-new-kind",
                &mut state.layer_kind,
                &options,
                &t!("settings.project.stack.new.role"),
                &t!("settings.project.stack.new.role.tip"),
                140.0,
            );
            let mut thickness = state.layer_thickness;
            if let Some(v) = num(
                ui,
                "ps-new-thickness",
                thickness,
                &mm_spec(0.0, 20.0),
                &t!("settings.project.stack.new.thickness"),
                &t!("settings.project.stack.new.thickness.tip"),
            ) {
                thickness = v;
            }
            state.layer_thickness = thickness;
            let ready = !state.layer_name.trim().is_empty();
            let tip = if ready {
                t!("settings.project.stack.new.add.tip")
            } else {
                t!("settings.project.stack.new.add.disabled_tip")
            };
            let kind = kinds[state.layer_kind].0;
            let name = state.layer_name.trim().to_owned();
            if secondary(ui, &t!("settings.project.stack.new.add"), &tip, ready).clicked() {
                self.project_settings.layer_name.clear();
                self.settings_run(
                    "board.set_layer_record",
                    json!({"name": name, "kind": kind, "thickness": thickness}),
                );
            }
        });
    }
}
