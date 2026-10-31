//! Design rules tab: the fabrication preset picker, one field per limit with the word
//! for where its number came from, how strict each check is, and the checks you write.
//!
//! Context: ported from the held Rules tab PR (#37, `rules_fab.rs`), laid out as the
//! `dialog-project-rules` mockup shows: a Copper card and a Drilling card side by side.
//! Local rules: a limit the project does not carry reads "not set" with a button that
//! records a sensible starting number; a preset writes all its numbers in one undo step
//! (`rules.preset_apply`); a hand edit writes one key (`board.rules.set`). The origin
//! word is "preset NAME" only while the number still equals the preset's own.
//! The decoupling distance of #439 is not on main yet, so it has no row here.
//!
//! # Dependencies
//! - crates/doc/src/fab.rs (`FabRules`, `fab_preset`, `fab_preset_list`)
//! - crates/engine/src/lib.rs (`rules.preset_apply`, `board.rules.set`, `rules.severity`,
//!   `checks.custom_add`, `checks.custom_remove`)
//! - crates/ui-egui/src/lib.rs (`CUSTOM_MEASURES`, `custom_measure_line`, `human_code`)
//!
//! # Do not
//! - Hold limit numbers here; `FIELDS` names keys, the project holds values.
use super::{ghost, mm_spec, note, num, secondary};
use crate::ui_kit::card::card;
use crate::ui_kit::form_row::{FormWidth, form_row};
use crate::ui_kit::number_field::NumberSpec;
use crate::ui_kit::select::select;
use crate::{App, CUSTOM_MEASURES, custom_measure_line, human_code, theme};
use pcbcraft_doc::{FabRules, fab_preset, fab_preset_list};
use pcbcraft_text::t;
use serde_json::{Value, json};

/// What kind of number a limit holds, for its range and unit.
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    /// Millimetres, 0.001 to 100.
    Mm,
    /// A plain ratio, 1 to 50.
    Ratio,
    /// Millimetres, 0 to 0.5.
    Plating,
}

/// One fabrication limit: the `board.rules.set` key, the number recorded when the
/// person presses Set a limit, and its kind.
struct Field {
    key: &'static str,
    start: f64,
    kind: Kind,
}

const fn f(key: &'static str, start: f64, kind: Kind) -> Field {
    Field { key, start, kind }
}

/// Copper card: the narrowest track, the gaps and rings that keep copper buildable.
const COPPER: [Field; 6] = [
    f("min_track", 0.15, Kind::Mm),
    f("min_space", 0.15, Kind::Mm),
    f("min_ring_via", 0.13, Kind::Mm),
    f("min_ring_pth", 0.15, Kind::Mm),
    f("edge_routed", 0.3, Kind::Mm),
    f("edge_vscore", 0.4, Kind::Mm),
];

/// Drilling card.
const DRILLING: [Field; 9] = [
    f("min_drill", 0.3, Kind::Mm),
    f("min_npth", 0.5, Kind::Mm),
    f("hole_to_hole_via", 0.25, Kind::Mm),
    f("hole_to_hole_pad", 0.5, Kind::Mm),
    f("hole_to_copper_via", 0.25, Kind::Mm),
    f("hole_to_copper_pth", 0.25, Kind::Mm),
    f("hole_to_copper_npth", 0.25, Kind::Mm),
    f("max_aspect_ratio", 10.0, Kind::Ratio),
    f("plating_allowance", 0.1, Kind::Plating),
];

/// Mask and silkscreen card.
const MASK_SILK: [Field; 4] = [
    f("silk_line", 0.15, Kind::Mm),
    f("silk_text_height", 0.8, Kind::Mm),
    f("silk_to_pad", 0.15, Kind::Mm),
    f("mask_dam", 0.1, Kind::Mm),
];

/// Check codes whose strictness the person may change on this tab.
const STRICTNESS: [&str; 7] = [
    "clearance",
    "copper_short",
    "board_edge",
    "track_width",
    "track_length",
    "unrouted",
    "missing_footprint",
];

/// A limit as a plain number, read by its key.
fn rule_value(rules: &FabRules, key: &str) -> Option<f64> {
    serde_json::to_value(rules)
        .ok()
        .and_then(|v| v.get(key).and_then(Value::as_f64))
}

/// Where a limit came from: the named preset while the number still equals the
/// preset's own, otherwise the person who typed it.
fn origin(rules: &FabRules, key: &str, value: f64) -> String {
    if let Some(name) = rules.preset.as_deref()
        && let Some(preset) = fab_preset(name)
        && rule_value(&preset.rules, key).is_some_and(|p| (p - value).abs() < 1e-9)
    {
        return t!("settings.project.origin.preset", name = name);
    }
    t!("settings.project.origin.yours")
}

impl App {
    /// The Design rules tab.
    ///
    /// # Rules
    /// - A preset runs `rules.preset_apply` (one undo step); a field runs `board.rules.set`.
    ///
    /// # Dependencies
    /// - crates/doc/src/fab.rs
    ///
    /// # Do not
    /// - Write limits into the project directly.
    pub(super) fn settings_rules(&mut self, ui: &mut egui::Ui) {
        self.settings_preset(ui);
        ui.add_space(8.0);
        let rules = self.project.board.rules.clone().unwrap_or_default();
        ui.columns(2, |columns| {
            let copper = &mut columns[0];
            card(
                copper,
                &t!("settings.project.rules.copper"),
                None,
                |_| {},
                |ui| {
                    let name = t!("settings.project.rules.clearance");
                    let spec = mm_spec(0.001, 100.0);
                    if let Some(v) = form_row(
                        ui,
                        FormWidth::Page,
                        &name,
                        Some(&t!("settings.project.rules.clearance.hint")),
                        None,
                        |ui| {
                            num(
                                ui,
                                "ps-clearance",
                                self.project.board.clearance,
                                &spec,
                                &name,
                                &t!("settings.project.rules.clearance.tip"),
                            )
                        },
                    ) {
                        self.settings_run("board.rules.set", json!({"clearance": v}));
                    }
                    self.fab_rows(ui, &COPPER, &rules);
                },
            );
            let drilling = &mut columns[1];
            card(
                drilling,
                &t!("settings.project.rules.drilling"),
                None,
                |_| {},
                |ui| {
                    self.fab_rows(ui, &DRILLING, &rules);
                },
            );
        });
        ui.add_space(8.0);
        ui.columns(2, |columns| {
            card(
                &mut columns[0],
                &t!("settings.project.rules.mask"),
                None,
                |_| {},
                |ui| {
                    self.fab_rows(ui, &MASK_SILK, &rules);
                },
            );
            card(
                &mut columns[1],
                &t!("settings.project.rules.strict"),
                None,
                |_| {},
                |ui| {
                    note(ui, t!("settings.project.rules.strict.intro"));
                    self.severity_rows(ui, &STRICTNESS);
                },
            );
        });
        ui.add_space(8.0);
        self.settings_custom_rules(ui);
    }

    /// The preset picker and the line saying where the preset's numbers come from.
    fn settings_preset(&mut self, ui: &mut egui::Ui) {
        let rules = self.project.board.rules.clone().unwrap_or_default();
        card(
            ui,
            &t!("settings.project.rules.preset"),
            Some(&t!("settings.project.rules.preset.caption")),
            |_| {},
            |ui| {
                let names = fab_preset_list();
                let mut labels: Vec<String> = vec![t!("settings.project.rules.preset.none")];
                labels.extend(names.iter().map(|n| (*n).to_owned()));
                let options: Vec<&str> = labels.iter().map(String::as_str).collect();
                let mut at = rules
                    .preset
                    .as_deref()
                    .and_then(|p| names.iter().position(|n| *n == p))
                    .map_or(0, |p| p + 1);
                let name = t!("settings.project.rules.preset.name");
                let changed = form_row(
                    ui,
                    FormWidth::Page,
                    &name,
                    Some(&t!("settings.project.rules.preset.hint")),
                    None,
                    |ui| {
                        select(
                            ui,
                            "ps-preset",
                            &mut at,
                            &options,
                            &name,
                            &t!("settings.project.rules.preset.tip"),
                            260.0,
                        )
                        .changed()
                    },
                );
                if changed && at > 0 {
                    self.settings_run("rules.preset_apply", json!({"name": names[at - 1]}));
                }
                if let (Some(preset), Some(date)) = (&rules.preset, &rules.retrieved) {
                    note(
                        ui,
                        t!(
                            "settings.project.rules.source",
                            name = preset,
                            date = date,
                            url = rules.source_url.clone().unwrap_or_default()
                        ),
                    );
                }
            },
        );
    }

    /// One form row per limit: label, field, origin word.
    fn fab_rows(&mut self, ui: &mut egui::Ui, fields: &[Field], rules: &FabRules) {
        for field in fields {
            let key = field.key;
            let name = pcbcraft_text::lookup(&format!("settings.project.rules.{key}"), &[]);
            let tip = pcbcraft_text::lookup(&format!("settings.project.rules.{key}.tip"), &[]);
            let current = rule_value(rules, key);
            let origin_word = match current {
                Some(v) => origin(rules, key, v),
                None => t!("settings.project.origin.unset"),
            };
            let write =
                form_row(
                    ui,
                    FormWidth::Page,
                    &name,
                    Some(&origin_word),
                    None,
                    |ui| match current {
                        Some(v) => {
                            let (lo, hi, unit, step, places) = match field.kind {
                                Kind::Mm => (0.001, 100.0, "mm", 0.01, 3),
                                Kind::Ratio => (1.0, 50.0, "", 0.5, 1),
                                Kind::Plating => (0.0, 0.5, "mm", 0.01, 3),
                            };
                            let spec = NumberSpec {
                                unit,
                                min: lo,
                                max: hi,
                                step,
                                places,
                                width: 104.0,
                            };
                            num(ui, ("ps-fab", key), v, &spec, &name, &tip)
                        }
                        None => secondary(
                            ui,
                            &t!("settings.project.rules.set_limit"),
                            &t!(
                                "settings.project.rules.set_limit.tip",
                                name = name.to_lowercase()
                            ),
                            true,
                        )
                        .clicked()
                        .then_some(field.start),
                    },
                );
            if let Some(v) = write {
                self.settings_run("board.rules.set", json!({ key: v }));
            }
        }
    }

    /// One row per check code: how strict it is, with a default setting.
    ///
    /// # Rules
    /// - Runs `rules.severity`; Default clears the override.
    ///
    /// # Do not
    /// - Offer codes the engine does not list.
    pub(super) fn severity_rows(&mut self, ui: &mut egui::Ui, codes: &[&str]) {
        let levels = ["default", "error", "warning", "off"];
        let labels: Vec<String> = levels
            .iter()
            .map(|l| pcbcraft_text::lookup(&format!("settings.project.level.{l}"), &[]))
            .collect();
        let options: Vec<&str> = labels.iter().map(String::as_str).collect();
        for code in codes {
            let current = self.project.rule_severity.get(*code).map(String::as_str);
            let mut at = levels.iter().position(|l| Some(*l) == current).unwrap_or(0);
            let before = at;
            let name = human_code(code);
            form_row(ui, FormWidth::Page, &name, None, None, |ui| {
                select(
                    ui,
                    ("ps-severity", *code),
                    &mut at,
                    &options,
                    &name,
                    &t!("settings.project.level.tip", name = name.to_lowercase()),
                    140.0,
                );
            });
            if at != before {
                let severity = if at == 0 {
                    Value::Null
                } else {
                    json!(levels[at])
                };
                self.settings_run(
                    "rules.severity",
                    json!({"code": code, "severity": severity}),
                );
            }
        }
    }

    /// The checks you write: the list, and the row that adds one.
    fn settings_custom_rules(&mut self, ui: &mut egui::Ui) {
        use crate::a11y::Named;
        let checks = self.project.custom_checks.clone();
        card(
            ui,
            &t!("settings.project.custom"),
            Some(&t!("settings.project.custom.caption")),
            |_| {},
            |ui| {
                let text = theme::tokens(ui.ctx()).text;
                if checks.is_empty() {
                    note(ui, t!("settings.project.custom.empty"));
                }
                for c in &checks {
                    ui.horizontal(|ui| {
                        ui.label(crate::fonts::text(crate::fonts::Role::Body, &c.name).color(text))
                            .on_hover_text(t!("settings.project.custom.row.tip"));
                        note(ui, custom_measure_line(&c.measure, c.limit));
                        let level = if c.severity == "error" {
                            t!("settings.project.custom.loud")
                        } else {
                            t!("settings.project.custom.soft")
                        };
                        note(ui, level);
                        if ghost(
                            ui,
                            &t!("settings.project.custom.remove"),
                            &t!("settings.project.custom.remove.tip"),
                            true,
                        )
                        .clicked()
                        {
                            self.settings_run("checks.custom_remove", json!({"id": c.id}));
                        }
                    });
                }
                if checks.len() >= 32 {
                    note(ui, t!("settings.project.custom.full"));
                    return;
                }
                ui.add_space(8.0);
                let name_label = t!("settings.project.custom.name");
                ui.horizontal(|ui| {
                    let state = &mut self.project_settings;
                    ui.add(
                        crate::targets::text_field(&mut state.rule_name)
                            .hint_text(&name_label)
                            .desired_width(200.0),
                    )
                    .named(&name_label)
                    .on_hover_text(t!("settings.project.custom.name.tip"));
                    let labels: Vec<String> = CUSTOM_MEASURES
                        .iter()
                        .map(|(key, _)| pcbcraft_text::lookup(key, &[]))
                        .collect();
                    let options: Vec<&str> = labels.iter().map(String::as_str).collect();
                    state.rule_measure = state.rule_measure.min(CUSTOM_MEASURES.len() - 1);
                    select(
                        ui,
                        "ps-custom-measure",
                        &mut state.rule_measure,
                        &options,
                        &t!("settings.project.custom.measure"),
                        &t!("settings.project.custom.measure.tip"),
                        220.0,
                    );
                    let count = CUSTOM_MEASURES[state.rule_measure].1.contains("count");
                    let spec = if count {
                        NumberSpec {
                            unit: "",
                            min: 1.0,
                            max: 100000.0,
                            step: 1.0,
                            places: 0,
                            width: 96.0,
                        }
                    } else {
                        mm_spec(0.01, 100000.0)
                    };
                    if let Some(v) = num(
                        ui,
                        "ps-custom-limit",
                        state.rule_limit,
                        &spec,
                        &t!("settings.project.custom.limit"),
                        &t!("settings.project.custom.limit.tip"),
                    ) {
                        state.rule_limit = v;
                    }
                    let mut loud = usize::from(!state.rule_loud);
                    let levels = [
                        t!("settings.project.custom.loud"),
                        t!("settings.project.custom.soft"),
                    ];
                    let options: Vec<&str> = levels.iter().map(String::as_str).collect();
                    select(
                        ui,
                        "ps-custom-level",
                        &mut loud,
                        &options,
                        &t!("settings.project.custom.level"),
                        &t!("settings.project.custom.level.tip"),
                        120.0,
                    );
                    state.rule_loud = loud == 0;
                    let ready = !state.rule_name.trim().is_empty();
                    let tip = if ready {
                        t!("settings.project.custom.add.tip")
                    } else {
                        t!("settings.project.custom.add.disabled_tip")
                    };
                    if secondary(ui, &t!("settings.project.custom.add"), &tip, ready).clicked() {
                        let params = json!({
                            "name": state.rule_name.trim(),
                            "measure": CUSTOM_MEASURES[state.rule_measure].1,
                            "limit": state.rule_limit,
                            "severity": if state.rule_loud { "error" } else { "warning" },
                        });
                        state.rule_name.clear();
                        self.settings_run("checks.custom_add", params);
                    }
                });
            },
        );
    }
}
