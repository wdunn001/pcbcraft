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
    /// Covering picker for the selected via; does nothing for other objects.
    pub(crate) fn via_covering_controls(&mut self, ui: &mut egui::Ui, id: Id) {
        let Some(via) = self.project.board.vias.iter().find(|v| v.id == id) else {
            return;
        };
        let own = via.covering;
        let has_hole = via.drill > 0.0;
        let shown = own.map_or("Follow board".to_string(), |c| covering_name(c).to_string());
        ui.add_space(10.0);
        ui.label("Via protection");
        ui.add_space(4.0);
        ui.add_enabled_ui(has_hole || own.is_none(), |ui| {
            egui::ComboBox::from_id_salt("via-covering")
                .selected_text(shown)
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_label(own.is_none(), "Follow board")
                        .on_hover_text("Use the covering chosen for the whole board.")
                        .clicked()
                    {
                        self.run("via.set_covering", json!({"id": id}));
                        ui.close();
                    }
                    for c in ViaCovering::ALL {
                        if c != ViaCovering::Open && !has_hole {
                            continue;
                        }
                        if ui
                            .selectable_label(own == Some(c), covering_name(c))
                            .on_hover_text(covering_help(c))
                            .clicked()
                        {
                            self.run(
                                "via.set_covering",
                                json!({"id": id, "covering": c.token()}),
                            );
                            ui.close();
                        }
                    }
                })
                .response.named("Via protection")
                .on_hover_text(
                    "How this via is protected under the solder mask. Anything but left open removes the mask opening in the fabrication files.",
                );
        });
        ui.add_space(6.0);
    }

    /// Board-wide default covering, shown with the other fabrication notes.
    pub(crate) fn via_covering_default_controls(&mut self, ui: &mut egui::Ui) {
        let current = self.project.board.via_covering_default;
        ui.horizontal(|ui| {
            ui.label("Via protection");
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
                .response.named("Via protection for board")
                .on_hover_text(
                    "How vias are protected under the solder mask unless a via says otherwise. It goes on the factory notes.",
                );
        });
    }
}
