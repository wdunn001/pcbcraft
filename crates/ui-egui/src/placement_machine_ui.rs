//! Pick and place machines in the window: the "Placement machine" section of a part's
//! Properties, and the machine view in Manufacture, Machines.
//!
//! Context: a pick and place machine cares which nozzle picks which part. The profile
//! (`Project.machine_profiles`) holds the nozzles and the rules; this file shows what
//! the rules chose for each part and lets a person override it.
//!
//! # Rules
//! - Every edit goes through an engine command: `component.field` for a part's
//!   override, `machine.add` and `machine.set` for the profile, `placement_yy1.export`
//!   for the job file.
//! - An empty override property means "use the rule"; the section shows which one is in force.
//! - Every control has a tooltip from the catalog (`machines.*`, `props.placement.*`).
//!
//! # Dependencies
//! - crates/doc/src/placement_machine.rs (`plan_placement`, the field names)
//! - crates/ui-egui/src/ui_kit/section.rs, form_row.rs, number_field.rs
//! - crates/ui-egui/src/properties.rs and workspace.rs call into this file.
//!
//! # Do not
//! - Keep a copy of the plan; compute it from `App::project` each frame.
use super::*;
use crate::ui_kit::{
    button::{Size, secondary_button},
    form_row::{FormWidth, form_row, toggle_row},
    number_field::{NumberSpec, number_field},
    section::section,
};
use pcbcraft_doc::{
    FIELD_FEEDER, FIELD_NOZZLE, FIELD_PICK_HEIGHT, FIELD_PLACE_HEIGHT, FIELD_SKIP, FIELD_SPEED,
    MachineProfile, PartPlacement, PlannedPart, plan_placement,
};

const ROW: FormWidth = FormWidth::Aside;
const SELECTED: &str = "selected-machine-id";

/// The first pick and place machine saved in the design.
fn pick_place_machine(project: &Project) -> Option<&MachineProfile> {
    project
        .machine_profiles
        .iter()
        .find(|m| m.kind == "pick_place")
}

/// One numeric override row of the Placement machine section.
struct NumberRow {
    key: &'static str,
    salt: &'static str,
    unit: &'static str,
    min: f64,
    max: f64,
    current: f64,
    places: usize,
    step: f64,
    is_own: bool,
    label: String,
    tip: String,
}

impl App {
    /// The "Placement machine" section of a part's Properties.
    ///
    /// # Rules
    /// - Shows the values in force and whether each is the rule's or the part's own.
    /// - Writes one property per edit through `component.field`; "Use the rules"
    ///   clears all six.
    ///
    /// # Dependencies
    /// - crates/doc/src/placement_machine.rs
    ///
    /// # Do not
    /// - Draw the section for a part that is not on the board.
    pub(crate) fn placement_machine_section(&mut self, ui: &mut egui::Ui, id: Id) {
        let Some(machine) = pick_place_machine(&self.project).cloned() else {
            return;
        };
        let Some(component) = self.project.component(id).cloned() else {
            return;
        };
        let plan = plan_placement(&self.project, &machine.pick_place, None);
        let Some(part) = plan.parts.iter().find(|q| q.component == id).cloned() else {
            return;
        };
        let own = PartPlacement::from_component(&component).unwrap_or_default();
        section(
            ui,
            "placement-machine",
            &t!("props.placement"),
            &t!("props.placement.tip"),
            |ui| {
                form_row(ui, ROW, &t!("props.placement.machine"), None, None, |ui| {
                    ui.label(&machine.name);
                });
                self.placement_nozzle_row(ui, id, &machine, &part, &own);
                self.placement_feeder_row(ui, id, &part, &own);
                self.placement_number_rows(ui, id, &part, &own);
                let mut skip = part.skip;
                let was = skip;
                toggle_row(
                    ui,
                    ROW,
                    &t!("props.placement.skip"),
                    &t!("props.placement.skip.tip"),
                    &mut skip,
                );
                if skip != was {
                    self.run(
                        "component.field",
                        json!({"id": id, "key": FIELD_SKIP, "value": if skip { "yes" } else { "no" }}),
                    );
                }
                if !own.is_empty()
                    && secondary_button(
                        ui,
                        None,
                        &t!("props.placement.reset"),
                        &t!("props.placement.reset.tip"),
                        Size::Small,
                        true,
                    )
                    .clicked()
                {
                    for key in [
                        FIELD_NOZZLE,
                        FIELD_FEEDER,
                        FIELD_SPEED,
                        FIELD_PICK_HEIGHT,
                        FIELD_PLACE_HEIGHT,
                        FIELD_SKIP,
                    ] {
                        self.run(
                            "component.field",
                            json!({"id": id, "key": key, "value": null}),
                        );
                    }
                }
            },
        );
    }

    fn placement_nozzle_row(
        &mut self,
        ui: &mut egui::Ui,
        id: Id,
        machine: &MachineProfile,
        part: &PlannedPart,
        own: &PartPlacement,
    ) {
        let hint = if part.resolved.nozzle_overridden {
            t!("props.placement.own")
        } else {
            t!("props.placement.rule")
        };
        let current = part.resolved.nozzle.clone().unwrap_or_default();
        let mut chosen: Option<Option<String>> = None;
        form_row(
            ui,
            ROW,
            &t!("props.placement.nozzle"),
            Some(&hint),
            None,
            |ui| {
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .selectable_label(own.nozzle.is_none(), t!("props.placement.automatic"))
                        .on_hover_text(t!("props.placement.automatic.tip"))
                        .clicked()
                    {
                        chosen = Some(None);
                    }
                    for n in &machine.pick_place.nozzle {
                        let tip = t!(
                            "props.placement.nozzle_choice.tip",
                            id = n.id,
                            size = format!("{:.1}", n.diameter_mm)
                        );
                        if ui
                            .selectable_label(current == n.id, &n.id)
                            .on_hover_text(tip)
                            .clicked()
                        {
                            chosen = Some(Some(n.id.clone()));
                        }
                    }
                });
            },
        );
        if let Some(choice) = chosen {
            self.run(
                "component.field",
                json!({"id": id, "key": FIELD_NOZZLE, "value": choice}),
            );
        }
    }

    fn placement_feeder_row(
        &mut self,
        ui: &mut egui::Ui,
        id: Id,
        part: &PlannedPart,
        own: &PartPlacement,
    ) {
        let hint = if own.feeder.is_some() {
            t!("props.placement.own")
        } else {
            t!("props.placement.rule")
        };
        let mut feeder = f64::from(part.feeder.unwrap_or(0));
        let spec = NumberSpec {
            unit: "",
            step: 1.0,
            min: 0.0,
            max: 999.0,
            places: 0,
            width: 110.0,
        };
        let changed = form_row(
            ui,
            ROW,
            &t!("props.placement.feeder"),
            Some(&hint),
            None,
            |ui| {
                number_field(
                    ui,
                    "placement-feeder",
                    &mut feeder,
                    &spec,
                    &t!("props.placement.feeder"),
                    &t!("props.placement.feeder.tip"),
                )
                .changed()
            },
        );
        if changed {
            let value = if feeder < 1.0 {
                serde_json::Value::Null
            } else {
                json!(format!("{}", feeder.round() as u32))
            };
            self.run(
                "component.field",
                json!({"id": id, "key": FIELD_FEEDER, "value": value}),
            );
        }
    }

    fn placement_number_rows(
        &mut self,
        ui: &mut egui::Ui,
        id: Id,
        part: &PlannedPart,
        own: &PartPlacement,
    ) {
        let rows = [
            NumberRow {
                key: FIELD_SPEED,
                salt: "placement-speed",
                unit: "%",
                min: 1.0,
                max: 100.0,
                current: f64::from(part.resolved.speed_pct),
                places: 0,
                step: 1.0,
                is_own: own.speed_pct.is_some(),
                label: t!("props.placement.speed"),
                tip: t!("props.placement.speed.tip"),
            },
            NumberRow {
                key: FIELD_PICK_HEIGHT,
                salt: "placement-pick",
                unit: "mm",
                min: -50.0,
                max: 50.0,
                current: part.resolved.pick_height_mm,
                places: 1,
                step: 0.1,
                is_own: own.pick_height_mm.is_some(),
                label: t!("props.placement.pick"),
                tip: t!("props.placement.pick.tip"),
            },
            NumberRow {
                key: FIELD_PLACE_HEIGHT,
                salt: "placement-place",
                unit: "mm",
                min: -50.0,
                max: 50.0,
                current: part.resolved.place_height_mm,
                places: 1,
                step: 0.1,
                is_own: own.place_height_mm.is_some(),
                label: t!("props.placement.place"),
                tip: t!("props.placement.place.tip"),
            },
        ];
        for NumberRow {
            key,
            salt,
            unit,
            min,
            max,
            current,
            places,
            step,
            is_own,
            label,
            tip,
        } in rows
        {
            let hint = if is_own {
                t!("props.placement.own")
            } else {
                t!("props.placement.rule")
            };
            let mut value = current;
            let spec = NumberSpec {
                unit,
                step,
                min,
                max,
                places,
                width: 110.0,
            };
            let changed = form_row(ui, ROW, &label, Some(&hint), None, |ui| {
                number_field(ui, salt, &mut value, &spec, &label, &tip).changed()
            });
            if changed {
                let text = if places == 0 {
                    format!("{}", value.round() as i64)
                } else {
                    format!("{value:.1}")
                };
                self.run(
                    "component.field",
                    json!({"id": id, "key": key, "value": text}),
                );
            }
        }
    }

    /// Left tab "Machines": the machines in the design, and the built-in ones to add.
    ///
    /// # Rules
    /// - Picking a machine selects it for the centre view; adding copies the built-in preset.
    pub(crate) fn machines_left(&mut self, ui: &mut egui::Ui) {
        egui::Frame::NONE
            .inner_margin(egui::Margin::symmetric(12, 8))
            .show(ui, |ui| self.machines_list(ui));
    }

    fn machines_list(&mut self, ui: &mut egui::Ui) {
        let selected: Option<String> = ui.data(|d| d.get_temp(egui::Id::new(SELECTED)));
        ui.label(egui::RichText::new(t!("machines.in_design")).color(self.muted()));
        if self.project.machine_profiles.is_empty() {
            ui.label(egui::RichText::new(t!("workspace.machines_hint")).color(self.muted()));
        }
        let rows: Vec<(String, String)> = self
            .project
            .machine_profiles
            .iter()
            .map(|m| (m.id.clone(), m.name.clone()))
            .collect();
        for (id, name) in rows {
            let on = selected.as_deref() == Some(id.as_str());
            if ui
                .selectable_label(on, &name)
                .on_hover_text(t!("machines.pick.tip"))
                .clicked()
            {
                ui.data_mut(|d| d.insert_temp(egui::Id::new(SELECTED), id.clone()));
            }
        }
        ui.add_space(8.0);
        ui.label(egui::RichText::new(t!("machines.built_in")).color(self.muted()));
        for preset in pcbcraft_io::builtin_machine_profile_ids() {
            if self
                .project
                .machine_profiles
                .iter()
                .any(|m| m.id == *preset)
            {
                continue;
            }
            let Ok(profile) = pcbcraft_io::builtin_machine_profile(preset) else {
                continue;
            };
            if secondary_button(
                ui,
                None,
                &t!("machines.add", name = profile.name),
                &t!("machines.add.tip"),
                Size::Small,
                true,
            )
            .clicked()
            {
                self.run("machine.add", json!({"preset": preset}));
                ui.data_mut(|d| d.insert_temp(egui::Id::new(SELECTED), (*preset).to_string()));
            }
        }
    }

    /// Centre view for the selected machine, or false when none is selected.
    ///
    /// # Rules
    /// - Pick and place machines show nozzles, feeder banks, the rules, the part plan,
    ///   the problems and the "Write job file" button; other kinds show a summary.
    pub(crate) fn machine_view(&mut self, ui: &mut egui::Ui) -> bool {
        let mut selected: Option<String> = ui.data(|d| d.get_temp(egui::Id::new(SELECTED)));
        if selected.is_none() {
            // Open on the first machine the design already has.
            selected = self.project.machine_profiles.first().map(|m| m.id.clone());
        }
        let Some(id) = selected else { return false };
        let Some(machine) = self
            .project
            .machine_profiles
            .iter()
            .find(|m| m.id == id)
            .cloned()
        else {
            return false;
        };
        ui.label(egui::RichText::new(&machine.name).font(crate::fonts::Role::Display.font()));
        ui.label(
            egui::RichText::new(format!(
                "{} {}  ({})",
                machine.vendor,
                machine.model,
                machine.kind.replace('_', " ")
            ))
            .color(self.muted()),
        );
        ui.add_space(8.0);
        if machine.kind != "pick_place" {
            ui.label(&machine.notes);
            return true;
        }
        self.pick_place_view(ui, &machine);
        true
    }

    fn pick_place_view(&mut self, ui: &mut egui::Ui, machine: &MachineProfile) {
        let pp = &machine.pick_place;
        let plan = plan_placement(&self.project, pp, None);
        section(
            ui,
            "machine-nozzles",
            &t!("machines.nozzles"),
            &t!("machines.nozzles.tip"),
            |ui| {
                egui::Grid::new("machine-nozzle-grid")
                    .num_columns(3)
                    .spacing([16.0, 4.0])
                    .show(ui, |ui| {
                        for n in &pp.nozzle {
                            ui.label(&n.id);
                            ui.label(format!("{:.1} mm", n.diameter_mm));
                            ui.label(n.packages.join(", "));
                            ui.end_row();
                        }
                    });
                for bank in &pp.feeders {
                    ui.label(format!("{}: {} to {}", bank.name, bank.first, bank.last));
                }
                if pp.nozzle_slots > 0 {
                    ui.label(t!("machines.slots", count = pp.nozzle_slots));
                }
            },
        );
        section(
            ui,
            "machine-rules",
            &t!("machines.rules"),
            &t!("machines.rules.tip"),
            |ui| self.rules_grid(ui, machine),
        );
        section(
            ui,
            "machine-parts",
            &t!("machines.parts"),
            &t!("machines.parts.tip"),
            |ui| {
                egui::Grid::new("machine-part-grid")
                    .num_columns(5)
                    .spacing([16.0, 4.0])
                    .show(ui, |ui| {
                        for h in [
                            t!("props.reference"),
                            t!("props.footprint"),
                            t!("props.placement.nozzle"),
                            t!("props.placement.feeder"),
                            t!("props.placement.speed"),
                        ] {
                            ui.label(egui::RichText::new(h).strong());
                        }
                        ui.end_row();
                        for q in &plan.parts {
                            ui.label(&q.reference);
                            ui.label(&q.package);
                            ui.label(if q.skip {
                                t!("machines.left_out")
                            } else {
                                q.resolved.nozzle.clone().unwrap_or_else(|| "-".into())
                            });
                            ui.label(q.feeder.map_or("-".into(), |n| n.to_string()));
                            ui.label(format!("{}%", q.resolved.speed_pct));
                            ui.end_row();
                        }
                    });
            },
        );
        section(
            ui,
            "machine-problems",
            &t!("machines.problems"),
            &t!("machines.problems.tip"),
            |ui| {
                if plan.problems.is_empty() {
                    ui.label(t!("machines.no_problems"));
                }
                for q in &plan.problems {
                    ui.label(egui::RichText::new(&q.message).color(self.accent()));
                }
            },
        );
        ui.add_space(8.0);
        if secondary_button(
            ui,
            None,
            &t!("machines.write_job"),
            &t!("machines.write_job.tip"),
            Size::Normal,
            machine.output.dialect == "neoden_yy1",
        )
        .clicked()
        {
            let dir = std::path::Path::new(&self.make_directory()).join("pick-and-place");
            self.run(
                "placement_yy1.export",
                json!({"machine": machine.id, "directory": dir.to_string_lossy(), "side": "both", "layout": "native"}),
            );
        }
    }

    fn rules_grid(&mut self, ui: &mut egui::Ui, machine: &MachineProfile) {
        let mut edit: Option<(usize, String)> = None;
        egui::Grid::new("machine-rule-grid")
            .num_columns(4)
            .spacing([16.0, 4.0])
            .show(ui, |ui| {
                for h in [
                    t!("machines.rule.package"),
                    t!("machines.rule.size"),
                    t!("machines.rule.height"),
                    t!("props.placement.nozzle"),
                ] {
                    ui.label(egui::RichText::new(h).strong());
                }
                ui.end_row();
                for (i, r) in machine.pick_place.placement_rule.iter().enumerate() {
                    ui.label(if r.package.is_empty() {
                        t!("machines.rule.any")
                    } else {
                        r.package.clone()
                    })
                    .on_hover_text(&r.note);
                    ui.label(bounds(r.min_size_mm, r.max_size_mm));
                    ui.label(bounds(r.min_height_mm, r.max_height_mm));
                    let shown = if r.skip {
                        t!("machines.left_out")
                    } else {
                        r.nozzle.clone()
                    };
                    if r.skip {
                        ui.label(shown);
                    } else {
                        egui::ComboBox::from_id_salt(("rule-nozzle", i))
                            .selected_text(shown)
                            .show_ui(ui, |ui| {
                                for n in &machine.pick_place.nozzle {
                                    if ui
                                        .selectable_label(n.id == r.nozzle, &n.id)
                                        .on_hover_text(t!("machines.rule.nozzle.tip"))
                                        .clicked()
                                    {
                                        edit = Some((i, n.id.clone()));
                                    }
                                }
                            })
                            .response
                            .named(&t!("machines.rule.nozzle"))
                            .on_hover_text(t!("machines.rule.nozzle.tip"));
                    }
                    ui.end_row();
                }
            });
        if let Some((i, nozzle)) = edit {
            let mut profile = machine.clone();
            if let Some(row) = profile.pick_place.placement_rule.get_mut(i) {
                row.nozzle = nozzle;
            }
            if let Ok(value) = serde_json::to_value(&profile) {
                self.run("machine.set", json!({"profile": value}));
            }
        }
    }
}

fn bounds(min: f64, max: f64) -> String {
    match (min > 0.0, max > 0.0) {
        (false, false) => "-".into(),
        (true, false) => format!("{min:.1} mm and up"),
        (false, true) => format!("up to {max:.1} mm"),
        (true, true) => format!("{min:.1} to {max:.1} mm"),
    }
}
