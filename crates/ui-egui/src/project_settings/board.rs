//! Board tab: finish and the layer table (the shape cards live in `board_shape.rs`).
//!
//! Context: mockup `dialog-project-board`. Finish and the plated edge are board
//! facts; the stackup card edits the recorded layer rows (material, thickness, Dk, Df,
//! copper weight) and the number of copper layers.
//! Local rules: layer rows are written with `board.set_layer_record`, which wants name,
//! kind and thickness on every call, so `record_params` always sends all three.
//! The copper count changes only through `board.stackup_add_pair` and
//! `board.stackup_remove_layer`, which keep copper already drawn on its layer.
//!
//! # Dependencies
//! - crates/doc/src/lib.rs (`LayerRecord`), crates/doc/src/material.rs (`MATERIALS`)
//! - crates/engine/src/lib.rs (`board.set_finish`, `board.set_stackup`,
//!   `board.set_machining`, `board.set_layer_record`, `board.remove_layer_record`,
//!   `board.stackup_add_pair`, `board.stackup_remove_layer`, `board.clear_layer_records`)
//!
//! Corner treatment (None, Round, Chamfer on export) is not offered: the old corner styles
//! only rebuilt a rectangle from a width and a height, which no free-form outline has.
//!
//! # Do not
//! - Compute outline vertices here; the engine owns the geometry.
use super::{NUMBER_WIDTH, SELECT_WIDE, SELECT_WIDTH, mm_spec, note, num, secondary};
use crate::ui_kit::button::{self, Size};
use crate::ui_kit::card::{card, card_flush};
use crate::ui_kit::empty_state::empty_state;
use crate::ui_kit::form_row::{FormWidth, form_row, toggle_row};
use crate::ui_kit::icons;
use crate::ui_kit::number_field::NumberSpec;
use crate::ui_kit::rows::{table_head, table_row};
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

/// The swatch colour of a stackup row of this kind.
fn layer_swatch(kind: &str, ctx: &egui::Context, first_copper: bool) -> egui::Color32 {
    let canvas = theme::CanvasColors::for_dark(ctx.global_style().visuals.dark_mode);
    let tokens = theme::tokens(ctx);
    match kind {
        "copper" if first_copper => canvas.top_copper,
        "copper" => canvas.bottom_copper,
        "core" | "prepreg" => canvas.board_fill,
        "soldermask" => tokens.mask,
        "silkscreen" => canvas.silkscreen,
        _ => tokens.muted,
    }
}

impl App {
    /// The Board tab.
    ///
    /// # Rules
    /// - Board shape and Shape of each layer stack in the left column and Finish takes the
    ///   right; Stackup runs full width below.
    /// - No control here types a width or a height of the board.
    ///
    /// # Dependencies
    /// - crates/ui-egui/src/ui_kit/card.rs
    /// - crates/ui-egui/src/project_settings/board_shape.rs
    ///
    /// # Do not
    /// - Keep a second copy of the shape; read it from the project each frame.
    pub(super) fn settings_board(&mut self, ui: &mut egui::Ui) {
        let gap = theme::SPACE[3];
        let total = ui.available_width();
        if total < 760.0 {
            self.settings_board_shape(ui);
            ui.add_space(gap);
            self.settings_layer_shapes(ui);
            ui.add_space(gap);
            self.settings_board_finish(ui);
            ui.add_space(gap);
            self.settings_stackup(ui);
            return;
        }
        let left = (total - gap) * 0.6;
        let right = total - gap - left;
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            ui.allocate_ui_with_layout(
                egui::vec2(left, 0.0),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.set_width(left);
                    self.settings_board_shape(ui);
                    ui.add_space(gap);
                    self.settings_layer_shapes(ui);
                },
            );
            ui.allocate_ui_with_layout(
                egui::vec2(right, 0.0),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.set_width(right);
                    self.settings_board_finish(ui);
                },
            );
        });
        ui.add_space(gap);
        self.settings_stackup(ui);
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
                            SELECT_WIDTH,
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
                    width: NUMBER_WIDTH,
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

    /// Replace the layer rows with one of the starting stackups, as one undo step.
    fn settings_apply_shape(&mut self, shape: usize) {
        let mut steps: Vec<(&str, Value)> = vec![("board.clear_layer_records", json!({}))];
        for (row, kind, thickness) in SHAPES[shape].1 {
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
        let shape_pick = std::cell::Cell::new(0usize);
        let add_layer = std::cell::Cell::new(false);
        card_flush(
            ui,
            &t!("settings.project.stack"),
            Some(&t!("settings.project.stack.caption")),
            |ui| {
                add_layer.set(secondary_with_icon(
                    ui,
                    &t!("settings.project.stack.new.add"),
                    &t!("settings.project.stack.new.add.tip"),
                ));
                let name = t!("settings.project.stack.start");
                let labels: Vec<String> = std::iter::once(t!("settings.project.stack.choose"))
                    .chain(SHAPES.iter().map(|(stem, _)| {
                        pcbcraft_text::lookup(&format!("settings.project.stack.shape.{stem}"), &[])
                    }))
                    .collect();
                let options: Vec<&str> = labels.iter().map(String::as_str).collect();
                let mut at = 0;
                if select(
                    ui,
                    "ps-stack-start",
                    &mut at,
                    &options,
                    &name,
                    &t!("settings.project.stack.start.tip"),
                    SELECT_WIDTH,
                )
                .changed()
                {
                    shape_pick.set(at);
                }
            },
            |ui| {
                egui::Frame::NONE
                    .inner_margin(egui::Margin::symmetric(theme::SPACE[3] as i8, 0))
                    .show(ui, |ui| {
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
                                ui.add_space(theme::SPACE[2]);
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
                                    self.settings_run(
                                        "board.stackup_add_pair",
                                        json!({"after": after}),
                                    );
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
                    });
                if records.is_empty() {
                    ui.add_space(theme::SPACE[4]);
                    if empty_state(
                        ui,
                        icons::LAYERS,
                        &t!("settings.project.stack.empty.title"),
                        &t!("settings.project.stack.empty"),
                        &t!("settings.project.stack.empty.button"),
                        &t!("settings.project.stack.empty.tip"),
                    )
                    .clicked()
                    {
                        shape_pick.set(1);
                    }
                    ui.add_space(theme::SPACE[4]);
                } else {
                    self.settings_layer_table(ui, &records);
                    self.settings_layer_details(ui, &records);
                }
            },
        );
        if shape_pick.get() > 0 {
            self.settings_apply_shape(shape_pick.get() - 1);
        }
        if add_layer.get() {
            let name = t!(
                "settings.project.stack.new.name_default",
                n = records.len() + 1
            );
            self.settings_run(
                "board.set_layer_record",
                json!({"name": name, "kind": "core", "thickness": 0.2}),
            );
            self.project_settings.layer_selected = Some(name);
        }
    }

    /// One table row per recorded layer: swatch and name, role, thickness and Remove.
    fn settings_layer_table(&mut self, ui: &mut egui::Ui, records: &[LayerRecord]) {
        let tokens = theme::tokens(ui.ctx());
        const COLUMNS: [f32; 4] = [3.0, 1.6, 2.4, 1.0];
        let heads = [
            t!("settings.project.stack.col.layer"),
            t!("settings.project.stack.col.role"),
            t!("settings.project.stack.col.thickness"),
            String::new(),
        ];
        let head_refs: Vec<&str> = heads.iter().map(String::as_str).collect();
        table_head(ui, &COLUMNS, &head_refs);
        let mut writes: Vec<(&str, Value)> = Vec::new();
        let mut select_row = None;
        let first_copper = records.iter().position(|r| r.kind == "copper");
        for (i, r) in records.iter().enumerate() {
            let selected = self.project_settings.layer_selected.as_deref() == Some(r.name.as_str());
            let swatch = layer_swatch(&r.kind, ui.ctx(), first_copper == Some(i));
            let row = table_row(
                ui,
                selected,
                &r.name,
                &t!("settings.project.stack.row.tip"),
                &COLUMNS,
                |ui, column| match column {
                    0 => {
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                        ui.painter().rect_filled(rect, 2.0, swatch);
                        ui.add_space(theme::SPACE[1]);
                        ui.add(
                            egui::Label::new(
                                fonts::text(fonts::Role::Body, &r.name).color(tokens.text),
                            )
                            .truncate(),
                        );
                    }
                    1 => {
                        ui.add(
                            egui::Label::new(
                                fonts::text(
                                    fonts::Role::Body,
                                    pcbcraft_text::lookup(
                                        &format!("settings.project.role.{}", r.kind),
                                        &[],
                                    ),
                                )
                                .color(tokens.muted),
                            )
                            .truncate(),
                        );
                    }
                    2 => {
                        let label =
                            format!("{} {}", r.name, t!("settings.project.stack.col.thickness"));
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
                    }
                    _ => {
                        let removable = r.kind != "copper";
                        let tip = if removable {
                            t!("settings.project.stack.remove.tip")
                        } else {
                            t!("settings.project.stack.remove.copper_tip")
                        };
                        if button::icon_button(
                            ui,
                            icons::CLOSE,
                            &format!("{} {}", t!("settings.project.stack.remove"), r.name),
                            &tip,
                            Size::Small,
                            removable,
                        )
                        .clicked()
                        {
                            writes.push(("board.remove_layer_record", json!({"name": r.name})));
                        }
                    }
                },
            );
            if row.clicked() {
                select_row = Some(r.name.clone());
            }
        }
        if let Some(name) = select_row {
            self.project_settings.layer_selected = Some(name);
        }
        if let Some((command, params)) = writes.into_iter().next() {
            self.settings_run(command, params);
        }
    }

    /// Role, material, Dk, Df and copper weight of the layer picked in the table.
    fn settings_layer_details(&mut self, ui: &mut egui::Ui, records: &[LayerRecord]) {
        let Some(r) = self
            .project_settings
            .layer_selected
            .as_deref()
            .and_then(|name| records.iter().find(|r| r.name == name))
        else {
            ui.add_space(theme::SPACE[2]);
            egui::Frame::NONE
                .inner_margin(egui::Margin::symmetric(theme::SPACE[3] as i8, 0))
                .show(ui, |ui| note(ui, t!("settings.project.stack.pick")));
            ui.add_space(theme::SPACE[2]);
            return;
        };
        let er = self.project.board.er;
        let is_copper = r.kind == "copper";
        let mut writes: Vec<(&str, Value)> = Vec::new();
        egui::Frame::NONE
            .inner_margin(egui::Margin::symmetric(
                theme::SPACE[3] as i8,
                theme::SPACE[1] as i8,
            ))
            .show(ui, |ui| {
                ui.label(
                    fonts::section_caption(t!("settings.project.stack.details", name = &r.name))
                        .color(theme::tokens(ui.ctx()).muted),
                );
                if is_copper {
                    let label = t!("settings.project.stack.col.oz");
                    if let Some(v) = form_row(ui, FormWidth::Page, &label, None, None, |ui| {
                        let oz = NumberSpec {
                            unit: "oz",
                            min: 0.1,
                            max: 10.0,
                            step: 0.5,
                            places: 1,
                            width: NUMBER_WIDTH,
                        };
                        num(
                            ui,
                            ("ps-oz", &r.name),
                            r.copper_weight_oz.unwrap_or(1.0),
                            &oz,
                            &format!("{} {label}", r.name),
                            &t!("settings.project.stack.oz.tip"),
                        )
                    }) {
                        writes.push((
                            "board.set_layer_record",
                            record_params(r, json!({"copper_weight_oz": v})),
                        ));
                    }
                    return;
                }
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
                let kind_options: Vec<&str> = kinds.iter().map(|(_, l)| l.as_str()).collect();
                let mut kind_at = kinds
                    .iter()
                    .position(|(token, _)| *token == r.kind)
                    .unwrap_or(0);
                let kind_before = kind_at;
                let role_label = t!("settings.project.stack.col.role");
                form_row(ui, FormWidth::Page, &role_label, None, None, |ui| {
                    select(
                        ui,
                        ("ps-role", &r.name),
                        &mut kind_at,
                        &kind_options,
                        &role_label,
                        &t!("settings.project.stack.new.role.tip"),
                        SELECT_WIDTH,
                    );
                });
                if kind_at != kind_before {
                    writes.push((
                        "board.set_layer_record",
                        record_params(r, json!({"kind": kinds[kind_at].0})),
                    ));
                }
                let mut labels: Vec<String> = vec![t!("settings.project.stack.no_material")];
                labels.extend(MATERIALS.iter().map(|m| m.name.to_owned()));
                let options: Vec<&str> = labels.iter().map(String::as_str).collect();
                let mut at = r
                    .material
                    .as_deref()
                    .and_then(|m| MATERIALS.iter().position(|x| x.name == m))
                    .map_or(0, |p| p + 1);
                let before = at;
                let label = t!("settings.project.stack.col.material");
                form_row(ui, FormWidth::Page, &label, None, None, |ui| {
                    select(
                        ui,
                        ("ps-material", &r.name),
                        &mut at,
                        &options,
                        &label,
                        &t!("settings.project.stack.material.tip"),
                        SELECT_WIDE,
                    );
                });
                if at != before && at > 0 {
                    let m = &MATERIALS[at - 1];
                    writes.push((
                        "board.set_layer_record",
                        record_params(r, json!({"material": m.name, "dk": m.dk, "df": m.df})),
                    ));
                }
                let material = r.material.as_deref().and_then(pcbcraft_doc::material);
                let dk = r.dk.or(material.map(|m| m.dk)).unwrap_or(er);
                let df = r.df.or(material.map(|m| m.df)).unwrap_or(0.02);
                let dks = NumberSpec {
                    unit: "",
                    min: 1.0,
                    max: 20.0,
                    step: 0.1,
                    places: 2,
                    width: NUMBER_WIDTH,
                };
                let dfs = NumberSpec {
                    unit: "",
                    min: 0.0,
                    max: 1.0,
                    step: 0.001,
                    places: 3,
                    width: NUMBER_WIDTH,
                };
                if let Some(v) = form_row(
                    ui,
                    FormWidth::Page,
                    &t!("settings.project.stack.col.dk"),
                    None,
                    None,
                    |ui| {
                        num(
                            ui,
                            ("ps-dk", &r.name),
                            dk,
                            &dks,
                            &format!("{} Dk", r.name),
                            &t!("settings.project.stack.dk.tip"),
                        )
                    },
                ) {
                    writes.push(("board.set_layer_record", record_params(r, json!({"dk": v}))));
                }
                if let Some(v) = form_row(
                    ui,
                    FormWidth::Page,
                    &t!("settings.project.stack.col.df"),
                    None,
                    None,
                    |ui| {
                        num(
                            ui,
                            ("ps-df", &r.name),
                            df,
                            &dfs,
                            &format!("{} Df", r.name),
                            &t!("settings.project.stack.df.tip"),
                        )
                    },
                ) {
                    writes.push(("board.set_layer_record", record_params(r, json!({"df": v}))));
                }
            });
        if let Some((command, params)) = writes.into_iter().next() {
            self.settings_run(command, params);
        }
    }
}

/// A secondary button with a plus, for card header actions.
fn secondary_with_icon(ui: &mut egui::Ui, label: &str, tip: &str) -> bool {
    button::secondary_button(ui, Some(icons::PLUS), label, tip, Size::Small, true).clicked()
}
