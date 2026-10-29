//! Via span controls: the start and end layer pickers in the properties
//! panel and the board-level list of spans a via may use.

use super::*;
use pcbcraft_doc::Layer;

/// Read-only facts about a via's span, for the properties rows.
pub(crate) fn via_span_rows(project: &Project, via: &pcbcraft_doc::Via) -> Vec<(String, String)> {
    let (upper, lower) = project.via_span(via);
    let rows = vec![
        (
            "Span".to_string(),
            format!(
                "{} to {}",
                project.layer_name(upper),
                project.layer_name(lower)
            ),
        ),
        (
            "Kind".to_string(),
            project.via_kind(via).label().to_string(),
        ),
    ];
    rows
}

/// Plain sentence for the board tree line of a via.
pub(crate) fn via_tree_span(project: &Project, via: &pcbcraft_doc::Via) -> String {
    let (upper, lower) = project.via_span(via);
    format!(
        "{} to {} · {}",
        project.layer_name(upper),
        project.layer_name(lower),
        project.via_kind(via).label()
    )
}

impl App {
    /// Start and end pickers for the selected via, limited to allowed spans.
    pub(crate) fn via_span_controls(&mut self, ui: &mut egui::Ui, id: Id) {
        let Some(via) = self.project.board.vias.iter().find(|v| v.id == id).cloned() else {
            return;
        };
        let allowed = self.project.allowed_via_spans();
        let (cur_from, cur_to) = self.project.via_span(&via);
        let name = |l: Layer| self.project.layer_name(l);
        let mut starts: Vec<Layer> = vec![];
        for (a, _) in &allowed {
            if !starts.contains(a) {
                starts.push(*a);
            }
        }
        let ends: Vec<Layer> = allowed
            .iter()
            .filter(|(a, _)| *a == cur_from)
            .map(|(_, b)| *b)
            .collect();
        let mut pick: Option<(Layer, Layer)> = None;
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new("COPPER SPAN")
                .size(10.0)
                .color(self.muted()),
        );
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Start layer");
            egui::ComboBox::from_id_salt(("via_span_start", id))
                .selected_text(name(cur_from))
                .show_ui(ui, |ui| {
                    for s in &starts {
                        if ui.selectable_label(*s == cur_from, name(*s)).clicked() && *s != cur_from
                        {
                            let end = allowed
                                .iter()
                                .find(|(a, b)| a == s && *b == cur_to)
                                .or_else(|| allowed.iter().find(|(a, _)| a == s))
                                .map(|(_, b)| *b);
                            if let Some(end) = end {
                                pick = Some((*s, end));
                            }
                        }
                    }
                })
                .response
                .on_hover_text(
                    "The upper copper layer this via touches. The kind follows: Through reaches both outer layers, Blind has one end on an outer layer, Buried stays inside, Micro is a shallow laser-drilled blind via. Only starts the board allows are listed.",
                );
        });
        ui.horizontal(|ui| {
            ui.label("End layer");
            egui::ComboBox::from_id_salt(("via_span_end", id))
                .selected_text(name(cur_to))
                .show_ui(ui, |ui| {
                    for e in &ends {
                        if ui.selectable_label(*e == cur_to, name(*e)).clicked() && *e != cur_to {
                            pick = Some((cur_from, *e));
                        }
                    }
                })
                .response
                .on_hover_text(
                    "The lower copper layer this via touches. Only ends that fit the start layer are listed.",
                );
        });
        ui.add_space(4.0);
        if let Some(note) = pcbcraft_rules::via_aspect_warning(&self.project, &via) {
            ui.label(
                egui::RichText::new(note)
                    .small()
                    .color(Color32::from_rgb(220, 160, 40)),
            )
            .on_hover_text(
                "Deep, narrow holes plate unevenly. Use a wider drill or a shorter span.",
            );
        }
        ui.add_space(6.0);
        if let Some((a, b)) = pick {
            self.run(
                "via.set_span",
                json!({"id": id, "from_layer": name(a), "to_layer": name(b)}),
            );
        }
    }

    /// Board-level list of spans a via may use, with add and remove.
    pub(crate) fn via_spans_section(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new("VIA SPANS")
                .size(10.0)
                .color(self.muted()),
        );
        ui.add_space(4.0);
        let allowed = self.project.allowed_via_spans();
        let extra = self.project.board.via_spans.clone();
        let mut remove: Option<(Layer, Layer)> = None;
        for (a, b) in &allowed {
            let custom = extra.contains(&(a.index(), b.index()));
            ui.horizontal(|ui| {
                ui.label(format!(
                    "{} to {}",
                    self.project.layer_name(*a),
                    self.project.layer_name(*b)
                ));
                if custom
                    && ui
                        .small_button("Remove")
                        .on_hover_text(
                            "Stop allowing vias on this span. Refused while a via uses it.",
                        )
                        .clicked()
                {
                    remove = Some((*a, *b));
                }
            });
        }
        let layers = self.project.copper_layers();
        if layers.len() > 2 {
            let key = egui::Id::new("via_span_add");
            let (mut x, mut y): (usize, usize) = ui
                .data(|d| d.get_temp(key))
                .unwrap_or((0, layers.len().saturating_sub(1)));
            x = x.min(layers.len() - 1);
            y = y.min(layers.len() - 1);
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt("via_span_add_from")
                    .selected_text(self.project.layer_name(layers[x]))
                    .show_ui(ui, |ui| {
                        for (i, l) in layers.iter().enumerate() {
                            ui.selectable_value(&mut x, i, self.project.layer_name(*l));
                        }
                    })
                    .response
                    .on_hover_text("Upper layer of the new span.");
                ui.label("to");
                egui::ComboBox::from_id_salt("via_span_add_to")
                    .selected_text(self.project.layer_name(layers[y]))
                    .show_ui(ui, |ui| {
                        for (i, l) in layers.iter().enumerate() {
                            ui.selectable_value(&mut y, i, self.project.layer_name(*l));
                        }
                    })
                    .response
                    .on_hover_text("Lower layer of the new span.");
            });
            ui.data_mut(|d| d.insert_temp(key, (x, y)));
            if ui
                .button("Allow span")
                .on_hover_text(
                    "Let vias run between these two layers. The start must be above the end.",
                )
                .clicked()
            {
                self.run(
                    "via.allow_span",
                    json!({
                        "from_layer": self.project.layer_name(layers[x]),
                        "to_layer": self.project.layer_name(layers[y]),
                        "allow": true
                    }),
                );
            }
        }
        if let Some((a, b)) = remove {
            self.run(
                "via.allow_span",
                json!({
                    "from_layer": self.project.layer_name(a),
                    "to_layer": self.project.layer_name(b),
                    "allow": false
                }),
            );
        }
        ui.add_space(8.0);
    }
}
