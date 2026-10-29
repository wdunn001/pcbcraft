//! Settings section for reference pictures: attach a PNG, place it under the
//! board, and trace its dark parts into silk, mask, a machine layer or copper.
//! The form values live in the egui context so the app struct stays small.
use super::*;
use std::collections::BTreeMap;

const TARGETS: [(&str, &str, &str); 6] = [
    (
        "Front silk",
        "silk_front",
        "Print the outline on the top silkscreen.",
    ),
    (
        "Back silk",
        "silk_back",
        "Print the outline on the bottom silkscreen.",
    ),
    (
        "Front mask",
        "mask_front",
        "Draw the outline on the top solder mask layer.",
    ),
    (
        "Back mask",
        "mask_back",
        "Draw the outline on the bottom solder mask layer.",
    ),
    (
        "Machine layer",
        "layer",
        "Add the outline to a drawn layer you made, for example for engraving.",
    ),
    (
        "Copper",
        "copper",
        "Fill the outline with copper on the top layer, joined to a net.",
    ),
];

#[derive(Clone)]
struct Form {
    path: String,
    reference: Option<Id>,
    target: usize,
    threshold: u32,
    smooth: f64,
    net: Option<Id>,
    layer: Option<Id>,
    /// Pending opacity, scale, x and y per picture, held while being edited.
    edits: BTreeMap<Id, [f64; 4]>,
}
impl Default for Form {
    fn default() -> Self {
        Self {
            path: String::new(),
            reference: None,
            target: 0,
            threshold: 128,
            smooth: 0.05,
            net: None,
            layer: None,
            edits: BTreeMap::new(),
        }
    }
}

impl App {
    pub(crate) fn reference_image_section(&mut self, ui: &mut egui::Ui) {
        let key = egui::Id::new("reference-image-form");
        let mut form: Form = ui.data_mut(|d| d.get_temp(key)).unwrap_or_default();
        ui.label(
            egui::RichText::new("REFERENCE IMAGE")
                .size(10.0)
                .color(self.muted()),
        );
        field_help(
            ui,
            "Show a picture under the board to copy from, such as a logo. It is only for tracing and never goes into any output file.",
            self.muted(),
        );
        ui.add_space(4.0);
        ui.add(
            crate::targets::text_field(&mut form.path)
                .hint_text("PNG file path")
                .desired_width(180.0),
        )
        .on_hover_text("The path of a PNG picture on this computer, up to 4096 pixels on a side.");
        if ui
            .button("Attach PNG")
            .on_hover_text("Store the picture in this project and show it under the board.")
            .clicked()
        {
            let path = form.path.trim().to_string();
            let name = std::path::Path::new(&path)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            self.run("file.attach_binary", json!({"name": name, "path": path}));
            if self
                .project
                .files
                .iter()
                .any(|f| f.name == name && f.is_binary())
            {
                self.run(
                    "reference_image.add",
                    json!({"file": name, "view": "pcb", "x": 0.0, "y": 0.0}),
                );
                form.reference = self.project.reference_images.last().map(|r| r.id);
            }
        }
        let pictures = self.project.reference_images.clone();
        if form
            .reference
            .is_none_or(|id| !pictures.iter().any(|r| r.id == id))
        {
            form.reference = pictures.first().map(|r| r.id);
        }
        for r in &pictures {
            ui.add_space(6.0);
            ui.label(format!(
                "{} · {}",
                r.file,
                if r.view == "pcb" { "board" } else { "sheet" }
            ));
            let mut edit = *form
                .edits
                .entry(r.id)
                .or_insert([r.opacity, r.scale, r.x, r.y]);
            let mut commit = false;
            ui.horizontal(|ui| {
                ui.label("Opacity");
                let resp = ui
                    .add(egui::DragValue::new(&mut edit[0]).range(0.0..=1.0).speed(0.01))
                    .on_hover_text("How strongly the picture shows through, from 0 (hidden) to 1 (solid).");
                commit |= resp.drag_stopped() || resp.lost_focus();
                ui.label("Size");
                let resp = ui
                    .add(
                        egui::DragValue::new(&mut edit[1])
                            .range(0.001..=100.0)
                            .speed(0.005)
                            .suffix(" mm/px"),
                    )
                    .on_hover_text("Set how many millimetres one picture pixel covers; larger makes the picture bigger. From 0.001 to 100 millimetres per pixel.");
                commit |= resp.drag_stopped() || resp.lost_focus();
            });
            ui.horizontal(|ui| {
                ui.label("Top-left");
                for (slot, tip) in [
                    (2, "Left edge of the picture, in millimetres."),
                    (3, "Top edge of the picture, in millimetres."),
                ] {
                    let resp = ui
                        .add(
                            egui::DragValue::new(&mut edit[slot])
                                .speed(0.1)
                                .suffix(" mm"),
                        )
                        .on_hover_text(tip);
                    commit |= resp.drag_stopped() || resp.lost_focus();
                }
            });
            form.edits.insert(r.id, edit);
            if commit {
                form.edits.remove(&r.id);
                self.run(
                    "reference_image.set",
                    json!({"id": r.id, "opacity": edit[0], "scale": edit[1], "x": edit[2], "y": edit[3]}),
                );
            }
            ui.horizontal(|ui| {
                if ui
                    .button(if r.visible { "Hide" } else { "Show" })
                    .on_hover_text(
                        "Show or hide this picture on the canvas. It stays in the project.",
                    )
                    .clicked()
                {
                    self.run(
                        "reference_image.set",
                        json!({"id": r.id, "visible": !r.visible}),
                    );
                }
                if ui
                    .button("Remove picture")
                    .on_hover_text(
                        "Stop showing this picture. Outlines already traced from it stay.",
                    )
                    .clicked()
                {
                    form.edits.remove(&r.id);
                    self.run("reference_image.remove", json!({"id": r.id}));
                }
            });
        }
        if let Some(reference) = form.reference {
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new("TRACE TO LAYER")
                    .size(10.0)
                    .color(self.muted()),
            );
            field_help(
                ui,
                "Turns the dark parts of the picture into editable outlines on a board layer.",
                self.muted(),
            );
            if pictures.len() > 1 {
                egui::ComboBox::from_label("Picture")
                    .selected_text(
                        pictures
                            .iter()
                            .find(|r| r.id == reference)
                            .map_or("", |r| r.file.as_str()),
                    )
                    .show_ui(ui, |ui| {
                        for r in &pictures {
                            ui.selectable_value(&mut form.reference, Some(r.id), &r.file);
                        }
                    })
                    .response
                    .on_hover_text("Which picture to trace.");
            }
            egui::ComboBox::from_label("Target")
                .selected_text(TARGETS[form.target].0)
                .show_ui(ui, |ui| {
                    for (i, (label, _, tip)) in TARGETS.iter().enumerate() {
                        ui.selectable_value(&mut form.target, i, *label)
                            .on_hover_text(*tip);
                    }
                })
                .response
                .on_hover_text("Where the traced outline goes.");
            match TARGETS[form.target].1 {
                "layer" => {
                    let layers: Vec<(Id, String)> = self
                        .project
                        .board
                        .user_layers
                        .iter()
                        .map(|l| (l.id, l.name.clone()))
                        .collect();
                    if form
                        .layer
                        .is_none_or(|id| !layers.iter().any(|(l, _)| *l == id))
                    {
                        form.layer = layers.first().map(|(l, _)| *l);
                    }
                    if layers.is_empty() {
                        field_help(
                            ui,
                            "Add a drawn layer first to trace onto it.",
                            self.muted(),
                        );
                    }
                    egui::ComboBox::from_label("Layer")
                        .selected_text(
                            layers
                                .iter()
                                .find(|(l, _)| Some(*l) == form.layer)
                                .map_or("", |(_, n)| n.as_str()),
                        )
                        .show_ui(ui, |ui| {
                            for (id, name) in &layers {
                                ui.selectable_value(&mut form.layer, Some(*id), name);
                            }
                        })
                        .response
                        .on_hover_text("The drawn layer that receives the outline.");
                }
                "copper" => {
                    let nets: Vec<(Id, String)> = self
                        .project
                        .nets
                        .iter()
                        .map(|n| (n.id, n.name.clone()))
                        .collect();
                    if form
                        .net
                        .is_none_or(|id| !nets.iter().any(|(n, _)| *n == id))
                    {
                        form.net = nets.first().map(|(n, _)| *n);
                    }
                    if nets.is_empty() {
                        field_help(
                            ui,
                            "Make a net first; copper must belong to one.",
                            self.muted(),
                        );
                    }
                    egui::ComboBox::from_label("Net")
                        .selected_text(
                            nets.iter()
                                .find(|(n, _)| Some(*n) == form.net)
                                .map_or("", |(_, name)| name.as_str()),
                        )
                        .show_ui(ui, |ui| {
                            for (id, name) in &nets {
                                ui.selectable_value(&mut form.net, Some(*id), name);
                            }
                        })
                        .response
                        .on_hover_text("The net the copper fill joins.");
                }
                _ => {}
            }
            ui.horizontal(|ui| {
                ui.label("Threshold");
                ui.add(egui::DragValue::new(&mut form.threshold).range(0..=255))
                    .on_hover_text(
                        "Pixels darker than this count as ink. Raise it to catch lighter greys.",
                    );
                ui.label("Smoothing");
                ui.add(
                    egui::DragValue::new(&mut form.smooth)
                        .range(0.0..=10.0)
                        .speed(0.01)
                        .suffix(" mm"),
                )
                .on_hover_text("Round off pixel steps; larger gives fewer, straighter corners. From 0 to 10 millimetres.");
            });
            if ui
                .button("Trace to layer")
                .on_hover_text("Trace the picture's dark parts onto the chosen target. Undo removes the outlines.")
                .clicked()
            {
                let target = TARGETS[form.target];
                let mut params = json!({
                    "reference": reference,
                    "target": target.1,
                    "threshold": form.threshold,
                    "smooth": form.smooth,
                });
                if let Some(layer) = form.layer.filter(|_| target.1 == "layer") {
                    params["layer"] = json!(layer);
                }
                if let Some(net) = form.net.filter(|_| target.1 == "copper") {
                    params["net"] = json!(net);
                }
                let result = self.run_and_keep("image.trace", params);
                self.refresh();
                if let Some(added) = result.get("added").and_then(Value::as_u64) {
                    self.message = format!("Traced {added} outline(s) to {}", target.0);
                }
            }
        }
        ui.data_mut(|d| d.insert_temp(key, form));
    }
}
