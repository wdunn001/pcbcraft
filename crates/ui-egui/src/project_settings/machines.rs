//! Machines tab: the machine profiles recorded on the project, and the library machines
//! that can be copied onto it.
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
//! - crates/ui-egui/src/library_workspace/machines.rs (the library list)
//! - crates/engine/src/lib.rs (`machine.install`, `machine.set`, `machine.remove`)
//!
//! # Do not
//! - Invent machines; the list is whatever the machine library holds.
use super::{SELECT_WIDE, TEXT_WIDTH, ghost, mm_spec, note, num};
use crate::ui_kit::button::{self, Size};
use crate::ui_kit::card::card_flush;
use crate::ui_kit::empty_state::empty_state;
use crate::ui_kit::form_row::{FormWidth, form_row};
use crate::ui_kit::icons;
use crate::ui_kit::rows::list_row_high;
use crate::ui_kit::select::select;
use crate::{App, fonts, theme};
use pcbcraft_doc::MachineProfile;
use pcbcraft_text::t;
use serde_json::{Value, json};

/// Height of a machine row: a name and a line under it.
const MACHINE_ROW: f32 = crate::ui_kit::rows::FINDING_ROW_HEIGHT;
/// Space kept at the right of a machine row for its Edit and Remove buttons.
const MACHINE_BUTTONS: f32 = 190.0;

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
        ui.add_space(theme::SPACE[2]);
        self.machine_lib_refresh();
        let list: Vec<(String, String)> = self
            .libws
            .machines
            .cards
            .iter()
            .filter(|c| c.get("source").and_then(Value::as_str) != Some("project"))
            .filter(|c| c.get("hidden").and_then(Value::as_bool) != Some(true))
            .filter_map(|c| {
                Some((
                    c.get("id")?.as_str()?.to_owned(),
                    c.get("name")?.as_str()?.to_owned(),
                ))
            })
            .collect();
        let install: std::cell::RefCell<Option<String>> = std::cell::RefCell::new(None);
        let pick = std::cell::Cell::new(
            self.project_settings
                .machine_pick
                .min(list.len().saturating_sub(1)),
        );
        let taken_ids: Vec<String> = self
            .project
            .machine_profiles
            .iter()
            .map(|p| p.id.clone())
            .collect();
        let mut remove: Option<String> = None;
        let mut edit: Option<String> = None;
        let profiles = self.project.machine_profiles.clone();
        card_flush(
            ui,
            &t!("settings.project.machines.title"),
            None,
            |ui| {
                let labels: Vec<&str> = list.iter().map(|(_, n)| n.as_str()).collect();
                if labels.is_empty() {
                    return;
                }
                let id = list[pick.get()].0.clone();
                let taken = taken_ids.contains(&id);
                let tip = if taken {
                    t!("settings.project.machines.add.disabled_tip")
                } else {
                    t!("settings.project.machines.add.tip")
                };
                if button::secondary_button(
                    ui,
                    Some(icons::PLUS),
                    &t!("settings.project.machines.add_from_library"),
                    &tip,
                    Size::Small,
                    !taken,
                )
                .clicked()
                {
                    *install.borrow_mut() = Some(id);
                }
                let mut at = pick.get();
                select(
                    ui,
                    "ps-machine-pick",
                    &mut at,
                    &labels,
                    &t!("settings.project.machines.pick"),
                    &t!("settings.project.machines.pick.tip"),
                    SELECT_WIDE,
                );
                pick.set(at);
            },
            |ui| {
                if profiles.is_empty() {
                    ui.add_space(theme::SPACE[4]);
                    if empty_state(
                        ui,
                        icons::OVEN,
                        &t!("settings.project.machines.empty"),
                        &t!("settings.project.machines.empty.message"),
                        &t!("settings.project.machines.add"),
                        &t!("settings.project.machines.add.tip"),
                    )
                    .clicked()
                    {
                        *install.borrow_mut() = list.get(pick.get()).map(|(id, _)| id.clone());
                    }
                    ui.add_space(theme::SPACE[4]);
                    return;
                }
                for profile in &profiles {
                    let editing =
                        self.project_settings.machine_edit.as_deref() == Some(profile.id.as_str());
                    let caption =
                        format!("{} {} · {}", profile.vendor, profile.model, profile.kind);
                    list_row_high(
                        ui,
                        MACHINE_ROW,
                        editing,
                        &profile.name,
                        &t!("settings.project.machines.row.tip"),
                        |ui| {
                            let tokens = theme::tokens(ui.ctx());
                            let text_width = (ui.available_width() - MACHINE_BUTTONS).max(0.0);
                            ui.allocate_ui_with_layout(
                                egui::vec2(text_width, MACHINE_ROW),
                                egui::Layout::top_down(egui::Align::Min)
                                    .with_main_align(egui::Align::Center),
                                |ui| {
                                    ui.set_width(text_width);
                                    ui.spacing_mut().item_spacing.y = 0.0;
                                    ui.add(
                                        egui::Label::new(
                                            fonts::strong(&profile.name).color(tokens.text),
                                        )
                                        .truncate(),
                                    );
                                    ui.add(
                                        egui::Label::new(
                                            fonts::caption(caption.trim()).color(tokens.muted),
                                        )
                                        .truncate(),
                                    );
                                },
                            );
                            if ghost(
                                ui,
                                &t!("settings.project.machines.edit"),
                                &t!("settings.project.machines.edit.tip"),
                                true,
                            )
                            .clicked()
                            {
                                edit = Some(profile.id.clone());
                            }
                            if ghost(
                                ui,
                                &t!("settings.project.machines.remove"),
                                &t!("settings.project.machines.remove.tip"),
                                true,
                            )
                            .clicked()
                            {
                                remove = Some(profile.id.clone());
                            }
                        },
                    );
                    if editing {
                        egui::Frame::NONE
                            .inner_margin(egui::Margin::symmetric(
                                theme::SPACE[3] as i8,
                                theme::SPACE[1] as i8,
                            ))
                            .show(ui, |ui| self.machine_rows(ui, profile));
                    }
                }
            },
        );
        if let Some(id) = edit {
            let now = self.project_settings.machine_edit.as_deref() == Some(id.as_str());
            self.project_settings.machine_edit = (!now).then_some(id);
        }
        self.project_settings.machine_pick = pick.get();
        if let Some(id) = install.into_inner() {
            self.settings_run("machine.install", json!({"id": id}));
            self.libws.machines.stale = true;
        }
        if let Some(id) = remove {
            self.settings_run("machine.remove", json!({"id": id}));
        }
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
                TEXT_WIDTH,
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
                TEXT_WIDTH,
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
