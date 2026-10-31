//! Via covering controls: how a via hole is protected under the solder mask.
use super::*;
use crate::a11y::Named;
use pcbcraft_doc::ViaCovering;

/// Menu text for a covering. "Left open" avoids clashing with the File menu.
pub(crate) fn covering_name(c: ViaCovering) -> &'static str {
    match c {
        ViaCovering::Open => "Left open",
        ViaCovering::Tented => "Tented",
        ViaCovering::TentedCovered => "Tented and covered",
        ViaCovering::Plugged => "Plugged",
        ViaCovering::PluggedCovered => "Plugged and covered",
        ViaCovering::Filled => "Filled",
        ViaCovering::FilledCovered => "Filled and covered",
        ViaCovering::FilledCapped => "Filled and capped",
    }
}

fn covering_help(c: ViaCovering) -> &'static str {
    match c {
        ViaCovering::Open => "The solder mask opens around the hole, so the copper stays bare.",
        ViaCovering::Tented => {
            "Solder mask bridges over the hole. Type I of the via protection standard."
        }
        ViaCovering::TentedCovered => "Mask covers the hole and the ring around it. Type II.",
        ViaCovering::Plugged => "The hole is plugged with resin or mask. Type III.",
        ViaCovering::PluggedCovered => "Plugged, then covered by mask. Type IV.",
        ViaCovering::Filled => "The hole is filled solid. Type V.",
        ViaCovering::FilledCovered => "Filled, then covered by mask. Type VI.",
        ViaCovering::FilledCapped => "Filled, then capped with copper. Type VII.",
    }
}

impl App {
    /// Protection pickers for the selected via, or for every selected via
    /// when several are chosen; does nothing for other objects.
    ///
    /// # Rules
    /// - The first picker sets both faces; Top and Bottom set one face each.
    /// - With several vias selected the pickers change all of them in one step.
    ///
    /// # Dependencies
    /// - crates/engine/src/via_protection.rs `run`.
    ///
    /// # Do not
    /// - Offer a protection other than open to a via with no drilled hole.
    pub(crate) fn via_covering_controls(&mut self, ui: &mut egui::Ui, id: Id) {
        let Some(via) = self.project.board.vias.iter().find(|v| v.id == id) else {
            return;
        };
        let own = via.covering;
        let board = &self.project.board;
        let (top, bottom) = (
            via.effective_covering(board),
            via.effective_covering_bottom(board),
        );
        let split = via.covering_bottom.is_some() && top != bottom;
        let kind = self.project.via_protection_type(via);
        let selected: Vec<Id> = self
            .selection()
            .into_iter()
            .filter(|s| self.project.board.vias.iter().any(|v| v.id == *s))
            .collect();
        let many = selected.len() > 1;
        let has_hole = selected
            .iter()
            .filter_map(|s| self.project.board.vias.iter().find(|v| v.id == *s))
            .all(|v| v.drill > 0.0)
            && via.drill > 0.0;
        let shown = if split {
            t!("via_protection.differs")
        } else {
            own.map_or(t!("via_protection.follow_board"), |c| {
                covering_name(c).to_string()
            })
        };
        ui.add_space(10.0);
        ui.label(t!("via_protection.title"));
        if many {
            ui.label(
                egui::RichText::new(t!("via_protection.many", count = selected.len()))
                    .color(self.muted()),
            );
        }
        ui.add_space(4.0);
        ui.add_enabled_ui(has_hole || (own.is_none() && !split), |ui| {
            egui::ComboBox::from_id_salt("via-covering")
                .selected_text(shown)
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_label(
                            own.is_none() && !split,
                            t!("via_protection.follow_board"),
                        )
                        .on_hover_text(t!("via_protection.follow_board.tip"))
                        .clicked()
                    {
                        self.set_via_covering(&selected, id, None, "both");
                        ui.close();
                    }
                    for c in ViaCovering::ALL {
                        if c != ViaCovering::Open && !has_hole {
                            continue;
                        }
                        if ui
                            .selectable_label(own == Some(c) && !split, covering_name(c))
                            .on_hover_text(covering_help(c))
                            .clicked()
                        {
                            self.set_via_covering(&selected, id, Some(c), "both");
                            ui.close();
                        }
                    }
                })
                .response
                .named("Via protection")
                .on_hover_text(t!("via_protection.both.tip"));
        });
        ui.add_space(6.0);
        for (face, salt, current) in [
            ("top", "via-covering-top", top),
            ("bottom", "via-covering-bottom", bottom),
        ] {
            let label = if face == "top" {
                t!("via_protection.top")
            } else {
                t!("via_protection.bottom")
            };
            let tip = if face == "top" {
                t!("via_protection.top.tip")
            } else {
                t!("via_protection.bottom.tip")
            };
            ui.horizontal_wrapped(|ui| {
                ui.label(label.clone());
                ui.add_enabled_ui(has_hole, |ui| {
                    egui::ComboBox::from_id_salt(salt)
                        .selected_text(covering_name(current))
                        .show_ui(ui, |ui| {
                            for c in ViaCovering::ALL {
                                if ui
                                    .selectable_label(current == c, covering_name(c))
                                    .on_hover_text(covering_help(c))
                                    .clicked()
                                {
                                    self.set_via_covering(&selected, id, Some(c), face);
                                    ui.close();
                                }
                            }
                        })
                        .response
                        .named(&label)
                        .on_hover_text(tip);
                });
            });
        }
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(t!("via_protection.type", kind = kind.gerber_token()))
                .color(self.muted()),
        )
        .on_hover_text(t!("via_protection.type.tip"));
        ui.add_space(6.0);
    }

    /// Run the protection command for one via or for the whole selection.
    fn set_via_covering(
        &mut self,
        selected: &[Id],
        id: Id,
        covering: Option<ViaCovering>,
        side: &str,
    ) {
        let mut params = if selected.len() > 1 {
            json!({"ids": selected, "side": side})
        } else {
            json!({"id": id, "side": side})
        };
        if let (Some(c), Some(map)) = (covering, params.as_object_mut()) {
            map.insert("covering".into(), json!(c.token()));
        }
        let command = if selected.len() > 1 {
            "via.set_covering_many"
        } else {
            "via.set_covering"
        };
        self.run(command, params);
    }

    /// Board-wide default covering, shown with the other fabrication notes.
    pub(crate) fn via_covering_default_controls(&mut self, ui: &mut egui::Ui) {
        let current = self.project.board.via_covering_default;
        ui.horizontal_wrapped(|ui| {
            ui.label(t!("via_protection.title"));
            egui::ComboBox::from_id_salt("board-via-covering")
                .selected_text(covering_name(current))
                .show_ui(ui, |ui| {
                    for c in ViaCovering::ALL {
                        if ui
                            .selectable_label(current == c, covering_name(c))
                            .on_hover_text(covering_help(c))
                            .clicked()
                        {
                            self.run(
                                "board.set_via_covering_default",
                                json!({"covering": c.token()}),
                            );
                            ui.close();
                        }
                    }
                })
                .response
                .named(&t!("via_protection.title"))
                .on_hover_text(t!("via_protection.follow_board.tip"));
        });
    }
}
