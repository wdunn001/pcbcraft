//! The pad assignments card on the Edit page: which pad each symbol pin drives, and what each pad is.
//!
//! Context: issue #759. A project-library part keeps a pin to pad map beside its symbol. The card
//! shows that map, a pad picker for every pin and a role picker for every pad, and lists the
//! problems the map has. Every pick is one engine command, so one undo step, and the CLI and MCP
//! reach the same change.
//!
//! Local rules: only a part in the project library with a project-library footprint has a map to
//! edit; any other part says how to get one. A pad picked for a pin leaves other pins alone, so a
//! pad given to two pins shows up as a problem until one of them is moved.
//!
//! # Dependencies
//! - crates/engine/src/authoring.rs (`library.pin_pad_assign`, `library.pin_pad_role`)
//! - crates/library/src/pin_pad_map.rs (`PinPadMap`, `validate_pin_pad_map`)
//!
//! # Do not
//! - Change the map here; call the engine commands so undo and the CLI see the same edit.
use super::*;
use crate::a11y::Named;
use pcbcraft_library::PadRole;

/// The roles a pad can take, in the order the picker lists them.
const ROLES: [(PadRole, &str); 4] = [
    (PadRole::Electrical, "electrical"),
    (PadRole::Mechanical, "mechanical"),
    (PadRole::Thermal, "thermal"),
    (PadRole::NoConnect, "no_connect"),
];

/// Plain words for a pad role.
fn role_label(role: PadRole) -> String {
    match role {
        PadRole::Electrical => t!("libws.padmap.role.electrical"),
        PadRole::Mechanical => t!("libws.padmap.role.mechanical"),
        PadRole::Thermal => t!("libws.padmap.role.thermal"),
        PadRole::NoConnect => t!("libws.padmap.role.no_connect"),
    }
}

impl App {
    /// The pad assignments card for the showing part.
    pub(super) fn library_pad_assignments(&mut self, ui: &mut egui::Ui) {
        let t = theme::tokens(ui.ctx());
        let Some(card) = self.library_showing() else {
            return;
        };
        let name = card.name();
        let found = if matches!(card, Card::Project(_)) {
            self.project
                .library
                .symbols
                .iter()
                .find(|s| s.name == name)
                .and_then(|symbol| {
                    self.project
                        .library
                        .footprints
                        .iter()
                        .find(|f| f.name == symbol.footprint)
                        .map(|footprint| (symbol.clone(), footprint.clone()))
                })
        } else {
            None
        };
        let Some((symbol, footprint)) = found else {
            crate::ui_kit::card::card(
                ui,
                &t!("libws.padmap.title"),
                Some(&t!("libws.padmap.caption")),
                |_| {},
                |ui| {
                    ui.label(
                        fonts::text(fonts::Role::Body, t!("libws.padmap.unavailable"))
                            .color(t.muted),
                    );
                },
            );
            return;
        };
        let map = self
            .project
            .library
            .part_pin_maps
            .get(&name)
            .cloned()
            .unwrap_or_else(|| pcbcraft_library::auto_fill(&symbol, &footprint));
        let findings = pcbcraft_library::validate_pin_pad_map(&symbol, &footprint, &map);
        let pads = pcbcraft_library::footprint_pad_labels(&footprint);
        let mut pins: Vec<(String, String)> = Vec::new();
        for pin in &symbol.pins {
            if !pins.iter().any(|(n, _)| *n == pin.number) {
                pins.push((pin.number.clone(), pin.name.clone()));
            }
        }
        let mut assign: Option<(String, String)> = None;
        let mut role: Option<(String, &'static str)> = None;
        crate::ui_kit::card::card(
            ui,
            &t!("libws.padmap.title"),
            Some(&t!("libws.padmap.caption")),
            |ui| {
                let (text, kind) = if findings.is_empty() {
                    (
                        t!("libws.padmap.clean"),
                        crate::ui_kit::chip::BadgeKind::Success,
                    )
                } else {
                    (
                        t!("libws.padmap.problems"),
                        crate::ui_kit::chip::BadgeKind::Warning,
                    )
                };
                crate::ui_kit::chip::badge(ui, &text, kind, &t!("libws.padmap.badge.tip"));
            },
            |ui| {
                for finding in &findings {
                    ui.label(fonts::text(fonts::Role::Body, &finding.message).color(t.text));
                }
                ui.label(fonts::section_caption(t!("libws.padmap.pins")).color(t.muted));
                egui::Grid::new("library-part-pad-map")
                    .num_columns(3)
                    .spacing([24.0, 8.0])
                    .show(ui, |ui| {
                        for (number, pin_name) in &pins {
                            ui.label(fonts::mono(number).color(t.text));
                            ui.label(fonts::text(fonts::Role::Body, pin_name).color(t.text));
                            let current = map
                                .pads_for_pin(number)
                                .and_then(|list| list.first().cloned());
                            let shown = match &current {
                                Some(pad) => t!("libws.padmap.pad_n", number = pad),
                                None => t!("libws.padmap.no_pad"),
                            };
                            egui::ComboBox::from_id_salt(("library-part-pad", number.clone()))
                                .selected_text(shown)
                                .show_ui(ui, |ui| {
                                    for pad in &pads {
                                        if ui
                                            .selectable_label(
                                                current.as_deref() == Some(pad.as_str()),
                                                t!("libws.padmap.pad_n", number = pad),
                                            )
                                            .on_hover_text(t!("libws.padmap.pick.tip"))
                                            .clicked()
                                        {
                                            assign = Some((number.clone(), pad.clone()));
                                        }
                                    }
                                })
                                .response
                                .named(&t!("libws.padmap.pad_for", pin = number))
                                .on_hover_text(t!("libws.padmap.pick.tip"));
                            ui.end_row();
                        }
                    });
                ui.add_space(8.0);
                ui.label(fonts::section_caption(t!("libws.padmap.pads")).color(t.muted));
                egui::Grid::new("library-part-pad-roles")
                    .num_columns(3)
                    .spacing([24.0, 8.0])
                    .show(ui, |ui| {
                        for pad in &pads {
                            ui.label(fonts::mono(pad).color(t.text));
                            let driven: Vec<String> = map
                                .rows
                                .iter()
                                .filter(|r| r.pads.iter().any(|p| p == pad))
                                .map(|r| r.pin.clone())
                                .collect();
                            let who = if driven.is_empty() {
                                t!("libws.padmap.no_pin")
                            } else {
                                t!("libws.padmap.driven_by", pins = driven.join(", "))
                            };
                            ui.label(fonts::text(fonts::Role::Body, who).color(t.muted));
                            let current = map.pad_role(pad);
                            egui::ComboBox::from_id_salt(("library-part-role", pad.clone()))
                                .selected_text(role_label(current))
                                .show_ui(ui, |ui| {
                                    for (candidate, word) in ROLES {
                                        if ui
                                            .selectable_label(
                                                candidate == current,
                                                role_label(candidate),
                                            )
                                            .on_hover_text(t!("libws.padmap.role.tip"))
                                            .clicked()
                                        {
                                            role = Some((pad.clone(), word));
                                        }
                                    }
                                })
                                .response
                                .named(&t!("libws.padmap.role_for", pad = pad))
                                .on_hover_text(t!("libws.padmap.role.tip"));
                            ui.end_row();
                        }
                    });
            },
        );
        if let Some((pin, pad)) = assign {
            self.tool_call(
                "library.pin_pad_assign",
                json!({"name": name, "pin": pin, "pad": pad, "only": true}),
            );
            self.refresh();
        }
        if let Some((pad, word)) = role {
            self.tool_call(
                "library.pin_pad_role",
                json!({"name": name, "pad": pad, "role": word}),
            );
            self.refresh();
        }
    }
}
