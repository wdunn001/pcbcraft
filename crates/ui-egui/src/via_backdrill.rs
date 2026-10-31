//! Back-drill controls for a via, shown under the protection section of the
//! Properties pane.
//!
//! Context: called from `lib.rs` right after `via_covering_controls`. Every
//! change goes through the `via.set_backdrill` command, so it is undoable and
//! the stub and drill table update from the model.
//!
//! Local rules:
//! - Four fields: where the drill starts, the layer it must not cut, the longest stub left and the drill diameter.
//! - Turning it on picks a sensible layer: the layer above the bottom end of the span.
//! - A via with no drilled hole cannot be back-drilled, so the picker is disabled and says why.
use super::*;
use crate::a11y::Named;
use pcbcraft_doc::{BackDrill, BackDrillFrom};

impl App {
    /// Back-drill fields for the selected via.
    ///
    /// # Rules
    /// - Reads the via's own setting; a class default is shown by the type line only.
    ///
    /// # Dependencies
    /// - crates/engine/src/backdrill.rs `via.set_backdrill`.
    /// - crates/doc/src/backdrill.rs `BackDrill`, `Project::backdrill_stub`.
    ///
    /// # Do not
    /// - Edit the model directly; run the command.
    pub(crate) fn via_backdrill_controls(&mut self, ui: &mut egui::Ui, id: Id) {
        let Some(via) = self.project.board.vias.iter().find(|v| v.id == id).cloned() else {
            return;
        };
        let own = via.backdrill;
        let (upper, lower) = self.project.via_span(&via);
        let has_hole = via.drill > 0.0;
        ui.add_space(10.0);
        ui.label(t!("via_backdrill.title"));
        ui.add_space(4.0);
        let shown = match own.map(|b| b.from) {
            None => t!("via_backdrill.off"),
            Some(BackDrillFrom::Top) => t!("via_backdrill.top"),
            Some(BackDrillFrom::Bottom) => t!("via_backdrill.bottom"),
        };
        ui.add_enabled_ui(has_hole, |ui| {
            ui.horizontal(|ui| {
                ui.label(t!("via_backdrill.start"));
                egui::ComboBox::from_id_salt("via-backdrill-from")
                    .selected_text(shown)
                    .show_ui(ui, |ui| {
                        let choices = [
                            (None, t!("via_backdrill.off")),
                            (Some(BackDrillFrom::Top), t!("via_backdrill.top")),
                            (Some(BackDrillFrom::Bottom), t!("via_backdrill.bottom")),
                        ];
                        for (choice, label) in choices {
                            if ui
                                .selectable_label(own.map(|b| b.from) == choice, label)
                                .clicked()
                            {
                                let spec = choice.map(|from| BackDrill {
                                    from,
                                    must_not_cut: own.map_or_else(
                                        || default_layer(&self.project, upper, lower, from),
                                        |b| b.must_not_cut,
                                    ),
                                    max_stub: own.map_or(0.25, |b| b.max_stub),
                                    diameter: own.map_or(via.drill + 0.2, |b| b.diameter),
                                });
                                self.set_via_backdrill(id, spec);
                                ui.close();
                            }
                        }
                    })
                    .response
                    .named("Back-drill start")
                    .on_hover_text(if has_hole {
                        t!("via_backdrill.start.tip")
                    } else {
                        t!("via_backdrill.nohole.tip")
                    });
            });
        });
        let Some(spec) = own else {
            return;
        };
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(t!("via_backdrill.keep"));
            let layers = self.project.via_span_layers(&via);
            egui::ComboBox::from_id_salt("via-backdrill-keep")
                .selected_text(self.project.layer_name(spec.must_not_cut))
                .show_ui(ui, |ui| {
                    for l in layers {
                        if ui
                            .selectable_label(l == spec.must_not_cut, self.project.layer_name(l))
                            .clicked()
                        {
                            self.set_via_backdrill(
                                id,
                                Some(BackDrill {
                                    must_not_cut: l,
                                    ..spec
                                }),
                            );
                            ui.close();
                        }
                    }
                })
                .response
                .named("Back-drill must not cut")
                .on_hover_text(t!("via_backdrill.keep.tip"));
        });
        let (mut stub, mut dia) = (spec.max_stub, spec.diameter);
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.label(t!("via_backdrill.stub"));
            changed |= ui
                .add(
                    egui::DragValue::new(&mut stub)
                        .range(0.0..=20.0)
                        .speed(0.01)
                        .suffix(" mm"),
                )
                .named("Back-drill longest stub")
                .on_hover_text(t!("via_backdrill.stub.tip"))
                .changed();
        });
        ui.horizontal(|ui| {
            ui.label(t!("via_backdrill.size"));
            changed |= ui
                .add(
                    egui::DragValue::new(&mut dia)
                        .range(0.05..=20.0)
                        .speed(0.01)
                        .suffix(" mm"),
                )
                .named("Back-drill diameter")
                .on_hover_text(t!("via_backdrill.size.tip"))
                .changed();
        });
        if changed {
            self.set_via_backdrill(
                id,
                Some(BackDrill {
                    max_stub: stub,
                    diameter: dia,
                    ..spec
                }),
            );
        }
        if let Some(mm) = self.project.backdrill_stub(&via, &spec) {
            let over = mm > spec.max_stub + 1e-9;
            let text = egui::RichText::new(t!("via_backdrill.left", mm = format!("{mm:.3}")));
            ui.label(if over {
                text.color(ui.visuals().warn_fg_color)
            } else {
                text.color(self.muted())
            })
            .on_hover_text(t!("via_backdrill.left.tip"));
        }
        ui.add_space(6.0);
    }

    /// Run `via.set_backdrill` for one via; `None` clears it.
    fn set_via_backdrill(&mut self, id: Id, spec: Option<BackDrill>) {
        let params = match spec {
            None => json!({"id": id}),
            Some(b) => json!({
                "id": id,
                "from": b.from.label(),
                "must_not_cut": self.project.layer_name(b.must_not_cut),
                "max_stub": b.max_stub,
                "diameter": b.diameter,
            }),
        };
        self.run("via.set_backdrill", params);
    }
}

/// The layer a new back-drill keeps: the one next to the far end of the span.
fn default_layer(
    project: &Project,
    upper: pcbcraft_doc::Layer,
    lower: pcbcraft_doc::Layer,
    from: BackDrillFrom,
) -> pcbcraft_doc::Layer {
    let _ = project;
    let index = match from {
        BackDrillFrom::Bottom => lower.index().saturating_sub(1).max(upper.index()),
        BackDrillFrom::Top => (upper.index() + 1).min(lower.index()),
    };
    pcbcraft_doc::Layer::from_index(index).unwrap_or(upper)
}
