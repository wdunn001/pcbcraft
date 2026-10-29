//! Named mounting datums: canvas marks, tree rows and inspector fields. Every
//! change goes through the typed `board.datum_*` commands.

use super::*;

/// Free-text and number fields the datum controls edit before a command runs.
#[derive(Default)]
pub(crate) struct DatumUi {
    pub(crate) add_name: String,
    pub(crate) add_hole: Option<Id>,
    /// Datum the edit fields below were last loaded from.
    loaded: Option<Id>,
    pub(crate) name: String,
    pub(crate) x: f64,
    pub(crate) y: f64,
}

pub(super) fn is_datum(project: &Project, id: Id) -> bool {
    project.board.datums.iter().any(|d| d.id == id)
}

impl App {
    /// Cross-hair and name for each datum on the board drawing.
    pub(super) fn paint_datums(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        camera: Camera,
        p: &Project,
    ) {
        for d in &p.board.datums {
            let at = camera.screen(p.datum_position(d), rect);
            let color = if self.selected == Some(d.id) {
                self.accent()
            } else {
                self.ink()
            };
            let stroke = Stroke::new(1.5, color);
            painter.line_segment(
                [at - egui::vec2(8.0, 0.0), at + egui::vec2(8.0, 0.0)],
                stroke,
            );
            painter.line_segment(
                [at - egui::vec2(0.0, 8.0), at + egui::vec2(0.0, 8.0)],
                stroke,
            );
            painter.circle_stroke(at, 5.0, stroke);
            painter.text(
                at + egui::vec2(8.0, -8.0),
                egui::Align2::LEFT_BOTTOM,
                &d.name,
                egui::FontId::proportional(11.0),
                color,
            );
        }
    }

    fn hole_label(&self, id: Id) -> String {
        match self.project.board.holes.iter().find(|h| h.id == id) {
            Some(h) => format!("Hole at {:.1}, {:.1}", h.position.x, h.position.y),
            None => "Free point".to_owned(),
        }
    }

    /// The DATUMS group of the board tree: an add row and one row per datum.
    pub(super) fn datums_section(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        ui.label(egui::RichText::new("DATUMS").size(10.0).color(self.muted()))
            .on_hover_text("Named reference points an enclosure can be lined up to.");
        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(8, 6))
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.datum_ui.add_name)
                            .hint_text("Datum name")
                            .desired_width(90.0),
                    )
                    .on_hover_text(
                        "Letters, digits, dash or underscore, up to 32 characters. Each name is used once.",
                    );
                    let selected = self
                        .datum_ui
                        .add_hole
                        .map_or_else(|| "Free point".to_owned(), |h| self.hole_label(h));
                    egui::ComboBox::from_id_salt("datum-add-hole")
                        .selected_text(selected)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.datum_ui.add_hole, None, "Free point")
                                .on_hover_text("Place the datum at the middle of the board.");
                            for h in self.project.board.holes.clone() {
                                ui.selectable_value(
                                    &mut self.datum_ui.add_hole,
                                    Some(h.id),
                                    format!("Hole at {:.1}, {:.1}", h.position.x, h.position.y),
                                )
                                .on_hover_text("The datum sits at this hole and follows it.");
                            }
                        })
                        .response
                        .on_hover_text("Where the new datum goes: on a drilled hole, or free.");
                    if ui
                        .button("Add datum")
                        .on_hover_text("Adds the datum. Undo takes it away again.")
                        .clicked()
                    {
                        let name = self.datum_ui.add_name.trim().to_owned();
                        let params = match self.datum_ui.add_hole {
                            Some(hole) => json!({"name": name, "hole": hole}),
                            None => {
                                let b = &self.project.board;
                                json!({"name": name, "position": {"x": b.width / 2.0, "y": b.height / 2.0}})
                            }
                        };
                        self.run("board.datum_add", params);
                        if self.message.ends_with("committed") {
                            self.datum_ui.add_name.clear();
                        }
                    }
                });
                if self.project.board.datums.is_empty() {
                    ui.label(
                        egui::RichText::new("No datums yet. Name one and press Add datum.")
                            .small()
                            .color(self.muted()),
                    );
                }
                for d in self.project.board.datums.clone() {
                    let pos = self.project.datum_position(&d);
                    ui.horizontal(|ui| {
                        if ui
                            .selectable_label(
                                self.selected == Some(d.id),
                                format!("{} · {:.1}, {:.1}", d.name, pos.x, pos.y),
                            )
                            .on_hover_text("Select this datum to edit it in Properties.")
                            .clicked()
                        {
                            self.select(Some(d.id));
                        }
                        if ui
                            .small_button("\u{d7}")
                            .on_hover_text("Take this datum off the board.")
                            .clicked()
                        {
                            self.run("board.datum_remove", json!({"id": d.id}));
                        }
                    });
                }
            });
    }

    /// Properties for a selected datum: name, position and hole to follow.
    pub(super) fn datum_inspector(&mut self, ui: &mut egui::Ui, id: Id) {
        let Some(d) = self
            .project
            .board
            .datums
            .iter()
            .find(|d| d.id == id)
            .cloned()
        else {
            return;
        };
        let pos = self.project.datum_position(&d);
        if self.datum_ui.loaded != Some(id) {
            self.datum_ui.loaded = Some(id);
            self.datum_ui.name = d.name.clone();
            self.datum_ui.x = pos.x;
            self.datum_ui.y = pos.y;
        }
        ui.heading(&d.name);
        ui.label(
            egui::RichText::new(format!("Datum · stable ID {id}"))
                .small()
                .color(self.muted()),
        );
        ui.add_space(8.0);
        ui.label("Name");
        ui.text_edit_singleline(&mut self.datum_ui.name)
            .on_hover_text("The name written into the enclosure files.");
        if ui
            .button("Rename")
            .on_hover_text("Gives the datum the name typed above.")
            .clicked()
        {
            let name = self.datum_ui.name.trim().to_owned();
            self.run("board.datum_rename", json!({"id": id, "name": name}));
        }
        ui.add_space(8.0);
        ui.label("Position · mm");
        ui.horizontal(|ui| {
            ui.label("X");
            ui.add(egui::DragValue::new(&mut self.datum_ui.x).speed(0.1))
                .on_hover_text("Across the board, in millimetres.");
            ui.label("Y");
            ui.add(egui::DragValue::new(&mut self.datum_ui.y).speed(0.1))
                .on_hover_text("Up the board, in millimetres.");
        });
        if ui
            .button("Move")
            .on_hover_text("Puts the datum at the position typed above and lets go of any hole.")
            .clicked()
        {
            let (x, y) = (self.datum_ui.x, self.datum_ui.y);
            self.run(
                "board.datum_move",
                json!({"id": id, "position": {"x": x, "y": y}}),
            );
        }
        ui.add_space(8.0);
        ui.label("Follow hole");
        let mut pick = d.hole;
        egui::ComboBox::from_id_salt("datum-follow-hole")
            .selected_text(
                d.hole
                    .map_or_else(|| "Free point".to_owned(), |h| self.hole_label(h)),
            )
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut pick, None, "Free point")
                    .on_hover_text("Keep the datum where it is, tied to nothing.");
                for h in self.project.board.holes.clone() {
                    ui.selectable_value(
                        &mut pick,
                        Some(h.id),
                        format!("Hole at {:.1}, {:.1}", h.position.x, h.position.y),
                    )
                    .on_hover_text("Move the datum to this hole's centre and follow it.");
                }
            })
            .response
            .on_hover_text("Tie the datum to a drilled hole so it moves with the hole.");
        if pick != d.hole {
            match pick {
                Some(hole) => {
                    self.run("board.datum_remove", json!({"id": id}));
                    self.run("board.datum_add", json!({"name": d.name, "hole": hole}));
                }
                None => self.run(
                    "board.datum_move",
                    json!({"id": id, "position": {"x": pos.x, "y": pos.y}}),
                ),
            }
            self.datum_ui.loaded = None;
        }
        ui.add_space(10.0);
        if ui
            .button("Remove")
            .on_hover_text("Takes this datum off the board. Undo brings it back.")
            .clicked()
        {
            self.run("board.datum_remove", json!({"id": id}));
            self.select(None);
        }
        ui.separator();
    }
}
