//! Signal integrity settings for one net class: rise time for crosstalk and long-track checks.
//!
//! Context: issue #841. The parallel-run crosstalk and electrically-long track checks only
//! run on classes that have a rise time. This row lets a newcomer set and clear it.
//!
//! # Dependencies
//! - crates/ui-egui/src/ui_kit/number_field.rs (ns spec)
//! - crates/engine/src/lib.rs (`netclass.rise_time`)
//!
//! # Do not
//! - Write to the project directly; use `settings_run`.
use super::{ghost, secondary};
use crate::App;
use crate::ui_kit::form_row::{FormWidth, form_row};
use crate::ui_kit::number_field::{NumberSpec, number_field};
use pcbcraft_doc::NetClass;
use pcbcraft_text::t;
use serde_json::json;

fn ns_spec() -> NumberSpec<'static> {
    NumberSpec {
        unit: "ns",
        min: 0.001,
        max: 1_000_000.0,
        step: 0.1,
        places: 3,
        width: 104.0,
    }
}

impl App {
    /// Signal integrity settings for one net class.
    ///
    /// Why: rise time enables the parallel-run crosstalk and electrically-long track
    /// checks. A newcomer must be able to set it here to turn those checks on.
    ///
    /// # Rules
    /// - None means no checks; any positive value enables them.
    /// - The engine command is `netclass.rise_time`.
    ///
    /// # Dependencies
    /// - crates/engine/src/lib.rs (`netclass.rise_time`)
    ///
    /// # Do not
    /// - Write to the project directly; use `settings_run`.
    pub(super) fn settings_class_si(&mut self, ui: &mut egui::Ui, class: &NetClass) {
        let id = class.id;
        let name = t!("settings.project.classes.rise_time");
        let tip = t!("settings.project.classes.rise_time.tip");
        match class.rise_time {
            Some(s) => {
                let ns = s * 1e9;
                let mut v = ns;
                let resp = form_row(ui, FormWidth::Page, &name, None, None, |ui| {
                    number_field(ui, ("ps-class-rise", id), &mut v, &ns_spec(), &name, &tip)
                });
                if resp.changed() {
                    let seconds = v * 1e-9;
                    self.settings_run(
                        "netclass.rise_time",
                        json!({"class": id, "rise_time": seconds}),
                    );
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                    if ghost(
                        ui,
                        &t!("settings.project.classes.rise_time.clear"),
                        &t!("settings.project.classes.rise_time.clear.tip"),
                        true,
                    )
                    .clicked()
                    {
                        self.settings_run(
                            "netclass.rise_time",
                            json!({"class": id, "rise_time": null}),
                        );
                    }
                });
            }
            None => {
                let resp = form_row(ui, FormWidth::Page, &name, Some(&tip), None, |ui| {
                    secondary(
                        ui,
                        &t!("settings.project.rules.set_limit"),
                        &t!(
                            "settings.project.classes.set.tip",
                            name = name.to_lowercase()
                        ),
                        true,
                    )
                });
                if resp.clicked() {
                    self.settings_run(
                        "netclass.rise_time",
                        json!({"class": id, "rise_time": 1e-9}),
                    );
                }
            }
        }
    }
}
