//! Machines tab: the machine profiles recorded on the project, and the built-in shop
//! presets that can be copied onto it.
//!
//! Context: the Manufacture page lists outputs and points here for the machines that
//! make them. A profile is the work area, output dialect and per-process settings of one
//! shop machine; Project settings edits the facts a newcomer needs (name, notes, work
//! area) and leaves the process sections to the profile file (`machine.import`).
//! Local rules: a profile is replaced whole through `machine.set`, built from the stored
//! profile with only the edited field changed, so no other setting is lost.
//!
//! # Dependencies
//! - crates/doc/src/machine_profile.rs (`MachineProfile`)
//! - crates/io/src/machine_profile.rs (`builtin_machine_profile_ids`, `builtin_machine_profile`)
//! - crates/engine/src/lib.rs (`machine.add`, `machine.set`, `machine.remove`)
//!
//! # Do not
//! - Invent machines; the preset list is the one compiled into the program.
use super::{ghost, mm_spec, note, num, secondary};
use crate::App;
use crate::ui_kit::card::card;
use crate::ui_kit::form_row::{FormWidth, form_row};
use crate::ui_kit::select::select;
use pcbcraft_doc::MachineProfile;
use pcbcraft_text::t;
use serde_json::{Value, json};
use std::sync::OnceLock;

/// The built-in presets as (id, display name), parsed once.
fn presets() -> &'static [(String, String)] {
    static PRESETS: OnceLock<Vec<(String, String)>> = OnceLock::new();
    PRESETS.get_or_init(|| {
        pcbcraft_io::builtin_machine_profile_ids()
            .iter()
            .filter_map(|id| {
                pcbcraft_io::builtin_machine_profile(id)
                    .ok()
                    .map(|p| ((*id).to_owned(), p.name))
            })
            .collect()
    })
}

/// The stored profile as JSON with one dotted field replaced.
fn with_field(profile: &MachineProfile, path: &[&str], value: Value) -> Option<Value> {
    let mut json = serde_json::to_value(profile).ok()?;
    let mut at = &mut json;
    for step in &path[..path.len() - 1] {
        at = at.get_mut(*step)?;
    }
    at.as_object_mut()?
        .insert((*path.last()?).to_owned(), value);
    Some(json)
}

impl App {
    /// The Machines tab.
    ///
    /// # Rules
    /// - Add copies a preset (`machine.add`); Remove runs `machine.remove`; edits run
    ///   `machine.set` with the whole profile.
    ///
    /// # Dependencies
    /// - crates/ui-egui/src/ui_kit/card.rs
    ///
    /// # Do not
    /// - Edit a profile in place; send it back through the engine.
    pub(super) fn settings_machines(&mut self, ui: &mut egui::Ui) {
        note(ui, t!("settings.project.machines.intro"));
        ui.add_space(8.0);
        self.settings_add_machine(ui);
        ui.add_space(8.0);
        if self.project.machine_profiles.is_empty() {
            note(ui, t!("settings.project.machines.empty"));
        }
        for profile in self.project.machine_profiles.clone() {
            let mut remove = false;
            let caption = format!("{} {}", profile.vendor, profile.model);
            card(
                ui,
                &profile.name,
                Some(caption.trim()),
                |ui| {
                    remove = ghost(
                        ui,
                        &t!("settings.project.machines.remove"),
                        &t!("settings.project.machines.remove.tip"),
                        true,
                    )
                    .clicked();
                },
                |ui| self.machine_rows(ui, &profile),
            );
            if remove {
                self.settings_run("machine.remove", json!({"id": profile.id}));
            }
            ui.add_space(8.0);
        }
    }

    /// The preset picker and the Add button.
    fn settings_add_machine(&mut self, ui: &mut egui::Ui) {
        let list = presets();
        let labels: Vec<&str> = list.iter().map(|(_, n)| n.as_str()).collect();
        ui.horizontal(|ui| {
            if labels.is_empty() {
                return;
            }
            let pick = &mut self.project_settings.machine_pick;
            *pick = (*pick).min(labels.len() - 1);
            select(
                ui,
                "ps-machine-pick",
                pick,
                &labels,
                &t!("settings.project.machines.pick"),
                &t!("settings.project.machines.pick.tip"),
                280.0,
            );
            let id = list[*pick].0.clone();
            let taken = self.project.machine_profiles.iter().any(|p| p.id == id);
            let tip = if taken {
                t!("settings.project.machines.add.disabled_tip")
            } else {
                t!("settings.project.machines.add.tip")
            };
            if secondary(ui, &t!("settings.project.machines.add"), &tip, !taken).clicked() {
                self.settings_run("machine.add", json!({"preset": id}));
            }
        });
    }

    /// Name, notes and work area of one profile.
    fn machine_rows(&mut self, ui: &mut egui::Ui, profile: &MachineProfile) {
        let id = profile.id.clone();
        let name = t!("settings.project.machines.name");
        if let Some(v) = form_row(ui, FormWidth::Page, &name, None, None, |ui| {
            super::text_field(
                ui,
                ("machine-name", &id),
                &profile.name,
                260.0,
                &name,
                &t!("settings.project.machines.name.tip"),
            )
        }) && !v.trim().is_empty()
            && let Some(json) = with_field(profile, &["name"], json!(v.trim()))
        {
            self.settings_run("machine.set", json!({"profile": json}));
        }
        let name = t!("settings.project.machines.notes");
        if let Some(v) = form_row(ui, FormWidth::Page, &name, None, None, |ui| {
            super::text_field(
                ui,
                ("machine-notes", &id),
                &profile.notes,
                360.0,
                &name,
                &t!("settings.project.machines.notes.tip"),
            )
        }) && let Some(json) = with_field(profile, &["notes"], json!(v))
        {
            self.settings_run("machine.set", json!({"profile": json}));
        }
        for (axis, value) in [("x", profile.work_area.x), ("y", profile.work_area.y)] {
            let name =
                pcbcraft_text::lookup(&format!("settings.project.machines.area_{axis}"), &[]);
            let tip =
                pcbcraft_text::lookup(&format!("settings.project.machines.area_{axis}.tip"), &[]);
            let write = form_row(ui, FormWidth::Page, &name, None, None, |ui| {
                num(
                    ui,
                    ("ps-area", &id, axis),
                    value,
                    &mm_spec(1.0, 5000.0),
                    &name,
                    &tip,
                )
            });
            if let Some(v) = write
                && let Some(json) = with_field(profile, &["work_area", axis], json!(v))
            {
                self.settings_run("machine.set", json!({"profile": json}));
            }
        }
        form_row(
            ui,
            FormWidth::Page,
            &t!("settings.project.machines.kind"),
            None,
            None,
            |ui| {
                note(ui, format!("{} · {}", profile.kind, profile.output.format));
            },
        );
    }
}
