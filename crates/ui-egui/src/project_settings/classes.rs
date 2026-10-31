//! Net classes tab: the list of classes with their rules, the nets each one holds and the
//! class-to-class gap table.
//!
//! Context: ported from the held Rules tab PR (#37): per-class via and pair numbers, each
//! with an origin word, and the gap table between two classes. The old Rules tab showed
//! one class per box with a Tune button; here every number is a field that writes on its
//! own, so there is no draft state to lose.
//! Local rules: `netclass.set` keeps the via hole under the pad and the engine refuses a
//! bad value with a message the dialog shows; a net is in at most one class.
//!
//! # Dependencies
//! - crates/doc/src/lib.rs (`NetClass`, `ClassPairRule`)
//! - crates/engine/src/lib.rs (`netclass.add`, `netclass.set`, `netclass.clearance`,
//!   `netclass.remove`, `netclass.assign`, `rules.class_pair_set`, `rules.class_pair_remove`)
//!
//! # Do not
//! - Offer a pair of one class with itself; the engine refuses it.
use super::{ghost, mm_spec, note, num, secondary};
use crate::ui_kit::card::card;
use crate::ui_kit::chip::chip;
use crate::ui_kit::form_row::{FormWidth, form_row};
use crate::ui_kit::select::select;
use crate::{App, theme};
use pcbcraft_doc::{Id, NetClass};
use pcbcraft_text::t;
use serde_json::json;

/// An optional millimetre number: the field when set, a Set button when not.
///
/// Returns the number to write, either an edit or the starting value.
fn optional_mm(
    ui: &mut egui::Ui,
    salt: impl std::hash::Hash + std::fmt::Debug,
    current: Option<f64>,
    start: f64,
    name: &str,
    tip: &str,
) -> Option<f64> {
    match current {
        Some(v) => num(ui, salt, v, &mm_spec(0.001, 100.0), name, tip),
        None => secondary(
            ui,
            &t!("settings.project.rules.set_limit"),
            &t!(
                "settings.project.classes.set.tip",
                name = name.to_lowercase()
            ),
            true,
        )
        .clicked()
        .then_some(start),
    }
}

impl App {
    /// The Net classes tab.
    ///
    /// # Rules
    /// - Every number is its own engine command, so each edit is one undo step.
    ///
    /// # Dependencies
    /// - crates/ui-egui/src/ui_kit/card.rs, chip.rs
    ///
    /// # Do not
    /// - Hold a copy of a class; read it from the project each frame.
    pub(super) fn settings_classes(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if secondary(
                ui,
                &t!("settings.project.classes.new"),
                &t!("settings.project.classes.new.tip"),
                self.project.net_classes.len() < 64,
            )
            .clicked()
            {
                let name = t!(
                    "settings.project.classes.default_name",
                    n = self.project.net_classes.len() + 1
                );
                let clearance = self.project.board.clearance;
                self.settings_run(
                    "netclass.add",
                    json!({"name": name, "clearance": clearance, "width": 0.25}),
                );
            }
            note(ui, t!("settings.project.classes.intro"));
        });
        ui.add_space(8.0);
        if self.project.net_classes.is_empty() {
            note(ui, t!("settings.project.classes.empty"));
        }
        for class in self.project.net_classes.clone() {
            self.settings_one_class(ui, &class);
            ui.add_space(8.0);
        }
        let taken: Vec<Id> = self
            .project
            .net_classes
            .iter()
            .flat_map(|k| k.nets.clone())
            .collect();
        let loose = self
            .project
            .nets
            .iter()
            .filter(|n| !taken.contains(&n.id))
            .count();
        if loose > 0 && !self.project.net_classes.is_empty() {
            note(ui, t!("settings.project.classes.loose", n = loose));
        }
        self.settings_class_pairs(ui);
    }

    fn settings_one_class(&mut self, ui: &mut egui::Ui, class: &NetClass) {
        let id = class.id;
        let mut remove = false;
        let caption = t!("settings.project.classes.nets_count", n = class.nets.len());
        card(
            ui,
            &class.name,
            Some(&caption),
            |ui| {
                remove = ghost(
                    ui,
                    &t!("settings.project.classes.remove"),
                    &t!("settings.project.classes.remove.tip"),
                    true,
                )
                .clicked();
            },
            |ui| {
                let name = t!("settings.project.classes.name");
                if let Some(v) = form_row(ui, FormWidth::Page, &name, None, None, |ui| {
                    super::text_field(
                        ui,
                        ("class-name", id),
                        &class.name,
                        220.0,
                        &name,
                        &t!("settings.project.classes.name.tip"),
                    )
                }) {
                    self.settings_run("netclass.set", json!({"class": id, "name": v}));
                }
                let name = t!("settings.project.classes.clearance");
                if let Some(v) = form_row(
                    ui,
                    FormWidth::Page,
                    &name,
                    Some(&t!("settings.project.classes.clearance.hint")),
                    None,
                    |ui| {
                        num(
                            ui,
                            ("ps-class-clearance", id),
                            class.clearance,
                            &mm_spec(0.001, 100.0),
                            &name,
                            &t!("settings.project.classes.clearance.tip"),
                        )
                    },
                ) {
                    self.settings_run("netclass.clearance", json!({"class": id, "clearance": v}));
                }
                let name = t!("settings.project.classes.width");
                if let Some(v) = form_row(
                    ui,
                    FormWidth::Page,
                    &name,
                    Some(&t!("settings.project.classes.width.hint")),
                    None,
                    |ui| {
                        num(
                            ui,
                            ("ps-class-width", id),
                            class.width,
                            &mm_spec(0.001, 100.0),
                            &name,
                            &t!("settings.project.classes.width.tip"),
                        )
                    },
                ) {
                    self.settings_run("netclass.set", json!({"class": id, "width": v}));
                }
                let rows: [(&str, Option<f64>, f64); 4] = [
                    ("via_diameter", class.via_diameter, 0.8),
                    ("via_drill", class.via_drill, 0.4),
                    ("pair_width", class.pair_width, 0.2),
                    ("pair_gap", class.pair_gap, 0.2),
                ];
                for (key, current, start) in rows {
                    let name =
                        pcbcraft_text::lookup(&format!("settings.project.classes.{key}"), &[]);
                    let tip =
                        pcbcraft_text::lookup(&format!("settings.project.classes.{key}.tip"), &[]);
                    let origin = if current.is_some() {
                        t!("settings.project.origin.class")
                    } else {
                        t!("settings.project.origin.default")
                    };
                    let write = form_row(ui, FormWidth::Page, &name, Some(&origin), None, |ui| {
                        optional_mm(ui, ("ps-class", id, key), current, start, &name, &tip)
                    });
                    if let Some(v) = write {
                        self.settings_run("netclass.set", json!({"class": id, key: v}));
                    }
                }
                ui.add_space(4.0);
                self.settings_class_nets(ui, class);
            },
        );
        if remove {
            self.settings_run("netclass.remove", json!({"class": id}));
        }
    }

    /// The nets in one class as chips, and the row that adds another.
    fn settings_class_nets(&mut self, ui: &mut egui::Ui, class: &NetClass) {
        let muted = theme::tokens(ui.ctx()).muted;
        let name = t!("settings.project.classes.members");
        ui.label(crate::fonts::section_caption(&name).color(muted));
        let mut leave = None;
        // A wrapping row with top alignment: `horizontal_wrapped` centres its rows in the
        // free height, which grows the scrolling dialog a little on every frame.
        ui.with_layout(
            egui::Layout::left_to_right(egui::Align::TOP).with_main_wrap(true),
            |ui| {
                for net in &class.nets {
                    let label = self.project.net_name(*net).to_owned();
                    if chip(
                        ui,
                        &label,
                        &t!("settings.project.classes.member.tip", net = label),
                    )
                    .clicked()
                    {
                        leave = Some(*net);
                    }
                }
                if class.nets.is_empty() {
                    note(ui, t!("settings.project.classes.members.empty"));
                }
            },
        );
        if let Some(net) = leave {
            self.settings_run("netclass.assign", json!({"net": net, "class": null}));
        }
        let taken: Vec<Id> = self
            .project
            .net_classes
            .iter()
            .flat_map(|k| k.nets.clone())
            .collect();
        let free: Vec<(Id, String)> = self
            .project
            .nets
            .iter()
            .filter(|n| !taken.contains(&n.id))
            .map(|n| (n.id, self.project.net_name(n.id).to_owned()))
            .collect();
        let pick_id = ui.id().with(("ps-add-net", class.id));
        let mut at: usize = ui.data(|d| d.get_temp(pick_id)).unwrap_or(0);
        ui.horizontal(|ui| {
            if free.is_empty() {
                note(ui, t!("settings.project.classes.free.empty"));
                return;
            }
            at = at.min(free.len() - 1);
            let options: Vec<&str> = free.iter().map(|(_, n)| n.as_str()).collect();
            select(
                ui,
                pick_id,
                &mut at,
                &options,
                &t!("settings.project.classes.add_net.pick"),
                &t!("settings.project.classes.add_net.pick.tip"),
                200.0,
            );
            if secondary(
                ui,
                &t!("settings.project.classes.add_net"),
                &t!("settings.project.classes.add_net.tip"),
                true,
            )
            .clicked()
            {
                self.settings_run(
                    "netclass.assign",
                    json!({"net": free[at].0, "class": class.id}),
                );
            }
        });
        ui.data_mut(|d| d.insert_temp(pick_id, at));
    }

    /// The class-to-class gap table with an Add row.
    fn settings_class_pairs(&mut self, ui: &mut egui::Ui) {
        let classes: Vec<(Id, String)> = self
            .project
            .net_classes
            .iter()
            .map(|c| (c.id, c.name.clone()))
            .collect();
        let name_of = |id: Id| {
            classes
                .iter()
                .find(|(c, _)| *c == id)
                .map_or_else(|| id.to_string(), |(_, n)| n.clone())
        };
        card(
            ui,
            &t!("settings.project.pairs"),
            Some(&t!("settings.project.pairs.caption")),
            |_| {},
            |ui| {
                if self.project.class_pairs.is_empty() {
                    note(ui, t!("settings.project.pairs.empty"));
                }
                for rule in self.project.class_pairs.clone() {
                    let label = t!(
                        "settings.project.pairs.row",
                        a = name_of(rule.class_a),
                        b = name_of(rule.class_b)
                    );
                    let mut remove = false;
                    let origin = t!("settings.project.origin.pair");
                    let write = form_row(ui, FormWidth::Page, &label, Some(&origin), None, |ui| {
                        let out = num(
                            ui,
                            ("ps-pair", rule.class_a, rule.class_b),
                            rule.clearance,
                            &mm_spec(0.001, 100.0),
                            &label,
                            &t!("settings.project.pairs.gap.tip"),
                        );
                        ui.add_space(8.0);
                        remove = ghost(
                            ui,
                            &t!("settings.project.pairs.remove"),
                            &t!("settings.project.pairs.remove.tip"),
                            true,
                        )
                        .clicked();
                        out
                    });
                    if let Some(v) = write {
                        self.settings_run(
                            "rules.class_pair_set",
                            json!({"class_a": rule.class_a, "class_b": rule.class_b, "clearance": v}),
                        );
                    }
                    if remove {
                        self.settings_run(
                            "rules.class_pair_remove",
                            json!({"class_a": rule.class_a, "class_b": rule.class_b}),
                        );
                    }
                }
                if classes.len() < 2 {
                    note(ui, t!("settings.project.pairs.need_two"));
                    return;
                }
                ui.add_space(8.0);
                let key_a = ui.id().with("ps-pair-a");
                let key_b = ui.id().with("ps-pair-b");
                let key_gap = ui.id().with("ps-pair-gap");
                let mut a: usize = ui.data(|d| d.get_temp(key_a)).unwrap_or(0);
                let mut b: usize = ui.data(|d| d.get_temp(key_b)).unwrap_or(1);
                let mut gap: f64 = ui.data(|d| d.get_temp(key_gap)).unwrap_or(0.5);
                a = a.min(classes.len() - 1);
                b = b.min(classes.len() - 1);
                let options: Vec<&str> = classes.iter().map(|(_, n)| n.as_str()).collect();
                ui.horizontal(|ui| {
                    select(
                        ui,
                        key_a,
                        &mut a,
                        &options,
                        &t!("settings.project.pairs.first"),
                        &t!("settings.project.pairs.first.tip"),
                        160.0,
                    );
                    select(
                        ui,
                        key_b,
                        &mut b,
                        &options,
                        &t!("settings.project.pairs.second"),
                        &t!("settings.project.pairs.second.tip"),
                        160.0,
                    );
                    if let Some(v) = num(
                        ui,
                        "ps-pair-new-gap",
                        gap,
                        &mm_spec(0.001, 100.0),
                        &t!("settings.project.pairs.new_gap"),
                        &t!("settings.project.pairs.new_gap.tip"),
                    ) {
                        gap = v;
                    }
                    let usable = a != b;
                    let tip = if usable {
                        t!("settings.project.pairs.add.tip")
                    } else {
                        t!("settings.project.pairs.add.disabled_tip")
                    };
                    if secondary(ui, &t!("settings.project.pairs.add"), &tip, usable).clicked() {
                        self.settings_run(
                            "rules.class_pair_set",
                            json!({"class_a": classes[a].0, "class_b": classes[b].0, "clearance": gap}),
                        );
                    }
                });
                ui.data_mut(|d| {
                    d.insert_temp(key_a, a);
                    d.insert_temp(key_b, b);
                    d.insert_temp(key_gap, gap);
                });
            },
        );
    }
}
