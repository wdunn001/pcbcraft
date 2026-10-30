//! Fabrication limits in the Rules tab: the maker preset picker, one field
//! per limit, the per-class via and pair numbers, and the class-to-class gap
//! table. Every value is shown with a muted word saying where it came from.
//!
//! # Rules
//! - Every change goes through an engine command; this file holds no rule data.
//! - Text comes from the catalog (`rules.fab.*` in crates/text/strings/en.txt).
//! - Draft numbers live in egui memory, not in `App`, so lib.rs stays small.
//!
//! # Dependencies
//! - crates/doc/src/fab.rs
//! - crates/engine/src/lib.rs#rules.preset_apply
//! - crates/text/strings/en.txt

use super::*;
use pcbcraft_doc::{FabRules, fab_preset, fab_preset_list};

/// What kind of number a field holds, for its range and unit.
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    /// Millimetres, 0.001 to 100.
    Mm,
    /// Millimetres, 0.001 to 10 (a pair gap).
    MmSmall,
    /// A plain ratio, 1 to 50.
    Ratio,
    /// Millimetres, 0 to 0.5 (plating allowance).
    Plating,
}

impl Kind {
    fn range(self) -> (f64, f64) {
        match self {
            Kind::Mm => (0.001, 100.0),
            Kind::MmSmall => (0.001, 10.0),
            Kind::Ratio => (1.0, 50.0),
            Kind::Plating => (0.0, 0.5),
        }
    }
    fn unit(self) -> &'static str {
        if self == Kind::Ratio { "" } else { " mm" }
    }
}

/// One fabrication limit: the `board.rules.set` key, its catalog label and
/// tooltip, and what kind of number it is.
struct FabField {
    key: &'static str,
    label: &'static str,
    tip: &'static str,
    kind: Kind,
}

const FAB_FIELDS: [FabField; 19] = [
    FabField {
        key: "min_track",
        label: "rules.fab.min_track",
        tip: "rules.fab.min_track.tip",
        kind: Kind::Mm,
    },
    FabField {
        key: "min_space",
        label: "rules.fab.min_space",
        tip: "rules.fab.min_space.tip",
        kind: Kind::Mm,
    },
    FabField {
        key: "min_drill",
        label: "rules.fab.min_drill",
        tip: "rules.fab.min_drill.tip",
        kind: Kind::Mm,
    },
    FabField {
        key: "min_npth",
        label: "rules.fab.min_npth",
        tip: "rules.fab.min_npth.tip",
        kind: Kind::Mm,
    },
    FabField {
        key: "min_ring_via",
        label: "rules.fab.min_ring_via",
        tip: "rules.fab.min_ring_via.tip",
        kind: Kind::Mm,
    },
    FabField {
        key: "min_ring_pth",
        label: "rules.fab.min_ring_pth",
        tip: "rules.fab.min_ring_pth.tip",
        kind: Kind::Mm,
    },
    FabField {
        key: "hole_to_hole_via",
        label: "rules.fab.hole_to_hole_via",
        tip: "rules.fab.hole_to_hole_via.tip",
        kind: Kind::Mm,
    },
    FabField {
        key: "hole_to_hole_pad",
        label: "rules.fab.hole_to_hole_pad",
        tip: "rules.fab.hole_to_hole_pad.tip",
        kind: Kind::Mm,
    },
    FabField {
        key: "hole_to_copper_via",
        label: "rules.fab.hole_to_copper_via",
        tip: "rules.fab.hole_to_copper_via.tip",
        kind: Kind::Mm,
    },
    FabField {
        key: "hole_to_copper_pth",
        label: "rules.fab.hole_to_copper_pth",
        tip: "rules.fab.hole_to_copper_pth.tip",
        kind: Kind::Mm,
    },
    FabField {
        key: "hole_to_copper_npth",
        label: "rules.fab.hole_to_copper_npth",
        tip: "rules.fab.hole_to_copper_npth.tip",
        kind: Kind::Mm,
    },
    FabField {
        key: "edge_routed",
        label: "rules.fab.edge_routed",
        tip: "rules.fab.edge_routed.tip",
        kind: Kind::Mm,
    },
    FabField {
        key: "edge_vscore",
        label: "rules.fab.edge_vscore",
        tip: "rules.fab.edge_vscore.tip",
        kind: Kind::Mm,
    },
    FabField {
        key: "silk_line",
        label: "rules.fab.silk_line",
        tip: "rules.fab.silk_line.tip",
        kind: Kind::Mm,
    },
    FabField {
        key: "silk_text_height",
        label: "rules.fab.silk_text_height",
        tip: "rules.fab.silk_text_height.tip",
        kind: Kind::Mm,
    },
    FabField {
        key: "silk_to_pad",
        label: "rules.fab.silk_to_pad",
        tip: "rules.fab.silk_to_pad.tip",
        kind: Kind::Mm,
    },
    FabField {
        key: "mask_dam",
        label: "rules.fab.mask_dam",
        tip: "rules.fab.mask_dam.tip",
        kind: Kind::Mm,
    },
    FabField {
        key: "max_aspect_ratio",
        label: "rules.fab.max_aspect_ratio",
        tip: "rules.fab.max_aspect_ratio.tip",
        kind: Kind::Ratio,
    },
    FabField {
        key: "plating_allowance",
        label: "rules.fab.plating_allowance",
        tip: "rules.fab.plating_allowance.tip",
        kind: Kind::Plating,
    },
];

/// A limit as a plain number, read by its key.
fn rule_value(rules: &FabRules, key: &str) -> Option<f64> {
    serde_json::to_value(rules)
        .ok()
        .and_then(|v| v.get(key).and_then(Value::as_f64))
}

/// Where a fabrication limit came from: the named preset when the number
/// still equals the preset's own, otherwise the person who typed it.
fn fab_origin(rules: &FabRules, key: &str, value: f64) -> String {
    if let Some(name) = rules.preset.as_deref()
        && let Some(preset) = fab_preset(name)
        && rule_value(&preset.rules, key).is_some_and(|p| (p - value).abs() < 1e-9)
    {
        return t!("rules.fab.origin.preset", name = name);
    }
    t!("rules.fab.origin.yours")
}

/// Give a number field the name a screen reader announces.
fn name_field(response: &egui::Response, name: &str) {
    let enabled = response.enabled();
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::DragValue, enabled, name));
}

/// A millimetre field that commits once, when the drag ends or the typed
/// number is accepted, so one edit is one undo step. An unset value reads
/// "not set". Returns the new number when it should be written.
fn commit_field(
    ui: &mut egui::Ui,
    salt: impl std::hash::Hash + std::fmt::Debug,
    name: &str,
    tip: &str,
    current: Option<f64>,
    kind: Kind,
) -> Option<f64> {
    let key = ui.id().with(salt);
    let stored: Option<f64> = ui.data(|d| d.get_temp(key));
    let mut v = stored.unwrap_or_else(|| current.unwrap_or(0.0));
    let (low, high) = kind.range();
    let unit = kind.unit();
    let unset = current.is_none();
    let field_w = ui.available_width().clamp(40.0, 160.0);
    let response = ui
        .add_sized(
            egui::vec2(field_w, ui.spacing().interact_size.y),
            egui::DragValue::new(&mut v)
                .speed(0.01)
                .range(0.0..=high)
                .custom_formatter(move |n, _| {
                    if n == 0.0 && unset {
                        t!("rules.fab.not_set")
                    } else {
                        format!("{n:.3}{unit}")
                    }
                })
                .custom_parser(|s| s.trim().trim_end_matches("mm").trim().parse::<f64>().ok()),
        )
        .on_hover_text(tip);
    name_field(&response, name);
    if response.changed() {
        ui.data_mut(|d| d.insert_temp(key, v));
    }
    if (response.drag_stopped() || response.lost_focus())
        && ui.data(|d| d.get_temp::<f64>(key)).is_some()
    {
        ui.data_mut(|d| d.remove_temp::<f64>(key));
        if v >= low && v <= high && current.is_none_or(|c| (c - v).abs() > 1e-9) {
            return Some(v);
        }
    }
    None
}

impl App {
    /// Preset picker, source line and one field per fabrication limit.
    ///
    /// # Rules
    /// - Applying a preset runs `rules.preset_apply`, one undo step.
    /// - A hand edit runs `board.rules.set` with that single key.
    ///
    /// # Dependencies
    /// - crates/doc/src/fab.rs
    ///
    /// # Do not
    /// - Write limits into the project directly; use the engine commands.
    pub(crate) fn fab_rules_section(&mut self, ui: &mut egui::Ui) {
        ui.add_space(6.0);
        ui.label(crate::fonts::strong(t!("rules.fab.heading")));
        ui.label(crate::fonts::caption(t!("rules.fab.intro")).color(self.muted()));
        ui.add_space(4.0);
        let rules = self.project.board.rules.clone().unwrap_or_default();
        let chosen = rules.preset.clone();
        ui.horizontal_wrapped(|ui| {
            ui.label(t!("rules.fab.preset"));
            let combo = egui::ComboBox::from_id_salt("fab-preset")
                .selected_text(
                    chosen
                        .clone()
                        .unwrap_or_else(|| t!("rules.fab.choose_preset")),
                )
                .show_ui(ui, |ui| {
                    for name in fab_preset_list() {
                        let tip = fab_preset(name).map_or_else(String::new, |p| {
                            t!(
                                "rules.fab.preset_item.tip",
                                source = p.rules.source_url.unwrap_or_default(),
                                date = p.rules.retrieved.unwrap_or_default()
                            )
                        });
                        if ui
                            .selectable_label(chosen.as_deref() == Some(name), name)
                            .on_hover_text(tip)
                            .clicked()
                        {
                            self.run("rules.preset_apply", json!({"name": name}));
                            ui.close();
                        }
                    }
                });
            let shown = combo.response.on_hover_text(t!("rules.fab.preset.tip"));
            name_field(&shown, &t!("rules.fab.preset"));
        });
        if let (Some(name), Some(date)) = (&rules.preset, &rules.retrieved) {
            ui.label(
                crate::fonts::caption(t!(
                    "rules.fab.source",
                    name = name,
                    date = date,
                    url = rules.source_url.clone().unwrap_or_default()
                ))
                .color(self.muted()),
            );
        }
        ui.add_space(4.0);
        if ui.ctx().content_rect().width() < 400.0 {
            return;
        }
        for field in &FAB_FIELDS {
            let current = rule_value(&rules, field.key);
            let name = pcbcraft_text::lookup(field.label, &[]);
            let mut write = None;
            let narrow = ui.available_width() < 400.0;
            if narrow {
                ui.label(&name);
                write = commit_field(
                    ui,
                    field.key,
                    &name,
                    &pcbcraft_text::lookup(field.tip, &[]),
                    current,
                    field.kind,
                );
                let origin = match current {
                    Some(v) => fab_origin(&rules, field.key, v),
                    None => t!("rules.fab.origin.default"),
                };
                ui.label(crate::fonts::caption(origin).color(self.muted()));
            } else {
                ui.horizontal_wrapped(|ui| {
                    ui.label(&name);
                    write = commit_field(
                        ui,
                        field.key,
                        &name,
                        &pcbcraft_text::lookup(field.tip, &[]),
                        current,
                        field.kind,
                    );
                    let origin = match current {
                        Some(v) => fab_origin(&rules, field.key, v),
                        None => t!("rules.fab.origin.default"),
                    };
                    ui.label(crate::fonts::caption(origin).color(self.muted()));
                });
            }
            if let Some(v) = write {
                self.run("board.rules.set", json!({ field.key: v }));
            }
        }
        ui.add_space(8.0);
    }

    /// Via and pair numbers for one class, each with its origin word.
    ///
    /// # Rules
    /// - A blank number says "default"; a set one says "class".
    /// - Writes go through `netclass.set`, which keeps drill under the pad.
    ///
    /// # Do not
    /// - Edit the class in place; the engine owns validation.
    pub(crate) fn class_fab_fields(&mut self, ui: &mut egui::Ui, class: &pcbcraft_doc::NetClass) {
        let rows: [(&str, &str, &str, Option<f64>, Kind); 4] = [
            (
                "via_diameter",
                "rules.fab.via_diameter",
                "rules.fab.via_diameter.tip",
                class.via_diameter,
                Kind::Mm,
            ),
            (
                "via_drill",
                "rules.fab.via_drill",
                "rules.fab.via_drill.tip",
                class.via_drill,
                Kind::Mm,
            ),
            (
                "pair_width",
                "rules.fab.pair_width",
                "rules.fab.pair_width.tip",
                class.pair_width,
                Kind::Mm,
            ),
            (
                "pair_gap",
                "rules.fab.pair_gap",
                "rules.fab.pair_gap.tip",
                class.pair_gap,
                Kind::MmSmall,
            ),
        ];
        let muted = self.muted();
        let mut writes: Vec<(&str, f64)> = Vec::new();
        ui.collapsing(t!("rules.fab.class_numbers"), |ui| {
            for (key, label, tip, current, kind) in rows {
                let name = pcbcraft_text::lookup(label, &[]);
                ui.horizontal_wrapped(|ui| {
                    ui.label(&name);
                    if let Some(v) = commit_field(
                        ui,
                        (class.id, key),
                        &name,
                        &pcbcraft_text::lookup(tip, &[]),
                        current,
                        kind,
                    ) {
                        writes.push((key, v));
                    }
                    let origin = if current.is_some() {
                        t!("rules.fab.origin.class")
                    } else {
                        t!("rules.fab.origin.default")
                    };
                    ui.label(crate::fonts::caption(origin).color(muted));
                });
            }
        });
        for (key, v) in writes {
            self.run("netclass.set", json!({"class": class.id, key: v}));
        }
    }

    /// The class-to-class gap table with Add and Remove.
    ///
    /// # Rules
    /// - Add runs `rules.class_pair_set`; Remove runs `rules.class_pair_remove`.
    /// - The two pickers and the new gap are drafts kept in egui memory.
    ///
    /// # Do not
    /// - Offer a pair of one class with itself; the engine refuses it.
    pub(crate) fn class_pair_section(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        ui.label(crate::fonts::strong(t!("rules.fab.pairs_heading")));
        ui.label(crate::fonts::caption(t!("rules.fab.pairs_intro")).color(self.muted()));
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
        for rule in self.project.class_pairs.clone() {
            let label = t!(
                "rules.fab.pair_row",
                a = name_of(rule.class_a),
                b = name_of(rule.class_b)
            );
            let mut write = None;
            let mut remove = false;
            ui.horizontal_wrapped(|ui| {
                ui.label(&label);
                write = commit_field(
                    ui,
                    ("pair", rule.class_a, rule.class_b),
                    &label,
                    &t!("rules.fab.pair_gap_edit.tip"),
                    Some(rule.clearance),
                    Kind::Mm,
                );
                ui.label(
                    crate::fonts::caption(t!("rules.fab.origin.class_pair")).color(self.muted()),
                );
                if ui
                    .tap_button(t!("rules.fab.remove_pair"))
                    .on_hover_text(t!("rules.fab.remove_pair.tip"))
                    .clicked()
                {
                    remove = true;
                }
            });
            if let Some(v) = write {
                self.run(
                    "rules.class_pair_set",
                    json!({"class_a": rule.class_a, "class_b": rule.class_b, "clearance": v}),
                );
            }
            if remove {
                self.run(
                    "rules.class_pair_remove",
                    json!({"class_a": rule.class_a, "class_b": rule.class_b}),
                );
            }
        }
        if self.project.class_pairs.is_empty() {
            ui.label(crate::fonts::caption(t!("rules.fab.no_pairs")).color(self.muted()));
        }
        if classes.len() < 2 {
            ui.label(crate::fonts::caption(t!("rules.fab.need_two_classes")).color(self.muted()));
            return;
        }
        let pick_a = ui.id().with("pair-pick-a");
        let pick_b = ui.id().with("pair-pick-b");
        let gap_id = ui.id().with("pair-gap-draft");
        let mut a: Id = ui
            .data(|d| d.get_temp(pick_a))
            .filter(|id| classes.iter().any(|(c, _)| c == id))
            .unwrap_or(classes[0].0);
        let mut b: Id = ui
            .data(|d| d.get_temp(pick_b))
            .filter(|id| classes.iter().any(|(c, _)| c == id))
            .unwrap_or(classes[1].0);
        let mut gap: f64 = ui.data(|d| d.get_temp(gap_id)).unwrap_or(0.5);
        ui.horizontal_wrapped(|ui| {
            for (salt, pick, tip) in [
                ("pair-a", &mut a, "rules.fab.pair_first.tip"),
                ("pair-b", &mut b, "rules.fab.pair_second.tip"),
            ] {
                let shown = name_of(*pick);
                let combo = egui::ComboBox::from_id_salt(salt)
                    .selected_text(shown.clone())
                    .show_ui(ui, |ui| {
                        for (id, name) in &classes {
                            if ui
                                .selectable_label(*pick == *id, name)
                                .on_hover_text(t!("rules.fab.pair_class_item.tip"))
                                .clicked()
                            {
                                *pick = *id;
                                ui.close();
                            }
                        }
                    });
                let shown = combo
                    .response
                    .on_hover_text(pcbcraft_text::lookup(tip, &[]));
                name_field(&shown, &pcbcraft_text::lookup(tip, &[]));
            }
            let response = ui
                .add(
                    egui::DragValue::new(&mut gap)
                        .speed(0.01)
                        .range(0.001..=100.0)
                        .suffix(format!(" {}", t!("unit.mm"))),
                )
                .on_hover_text(t!("rules.fab.pair_gap_new.tip"));
            name_field(&response, &t!("rules.fab.pair_gap_new"));
            let usable = a != b;
            let add = ui.add_enabled(usable, egui::Button::new(t!("rules.fab.add_pair")));
            let add = if usable {
                add.on_hover_text(t!("rules.fab.add_pair.tip"))
            } else {
                add.on_disabled_hover_text(t!("rules.fab.add_pair.disabled_tip"))
            };
            if add.clicked() {
                self.run(
                    "rules.class_pair_set",
                    json!({"class_a": a, "class_b": b, "clearance": gap}),
                );
            }
        });
        ui.data_mut(|d| {
            d.insert_temp(pick_a, a);
            d.insert_temp(pick_b, b);
            d.insert_temp(gap_id, gap);
        });
        ui.add_space(8.0);
    }
}
