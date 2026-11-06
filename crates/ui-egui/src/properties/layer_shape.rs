//! Properties for shapes on the Layout tab: the board shape, a copper layer's
//! own shape, and the note on a sub-layer.
//!
//! Context: the board is a free-form shape and each copper layer may have a
//! shape of its own. These sections read the shape and offer the actions that
//! change which shape a layer uses. The only place a width and a height are
//! typed is the small "Start from a rectangle" window; everywhere else the
//! reach of the board is a read-only sentence.
//! Local rules: every change is an engine command run through `App::run`;
//! typed numbers of the window live in `OutlineUi`, never in a local variable.
//!
//! # Dependencies
//! - crates/ui-egui/src/layer_shape_edit.rs (descriptions and shape commands)
//! - crates/engine/src/lib.rs (`board.set_rectangle`)
//! - crates/ui-egui/src/file_dialogs.rs (the import window)
//!
//! # Do not
//! - Add a Width or Height field to a Properties page.
//! - Offer a shape action for a sub-layer.
use super::*;
use crate::layer_shape_edit::{copper_key_of_path, shape_description};

impl App {
    /// The shape section: kind, corners, cut-outs and the reach of the whole board.
    ///
    /// `own` names the copper layer whose own shape is described; `None` is the board shape.
    pub(crate) fn shape_summary_section(&mut self, ui: &mut egui::Ui, own: Option<String>) {
        let board = &self.project.board;
        let (points, arcs) = match &own {
            Some(key) => board.shape_of(key),
            None => board.shape_of(""),
        };
        let words = shape_description(points, arcs);
        let corners = if points.len() < 3 { 4 } else { points.len() };
        let cutouts = board.cutouts.len();
        let (w, h) = board.extent_size();
        let reach = t!(
            "tree.edge.size",
            w = crate::layers_tree::size_text(w),
            h = crate::layers_tree::size_text(h)
        );
        let title = match &own {
            Some(key) => t!("shape.section.layer", layer = key),
            None => t!("shape.section.board"),
        };
        let mut open_rect = false;
        let mut open_import = false;
        section(
            ui,
            "shape-summary",
            &title,
            &t!("shape.section.tip"),
            |ui| {
                form_row(ui, ROW, &t!("shape.outline"), None, None, |ui| {
                    value_text(ui, &words)
                });
                form_row(ui, ROW, &t!("shape.corners"), None, None, |ui| {
                    value_text(ui, &corners.to_string())
                });
                form_row(ui, ROW, &t!("shape.cutouts"), None, None, |ui| {
                    value_text(ui, &cutouts.to_string())
                });
                form_row(ui, ROW, &t!("shape.reaches"), None, None, |ui| {
                    value_text(ui, &reach)
                });
                if own.is_none() {
                    ui.horizontal_wrapped(|ui| {
                        if secondary_button(
                            ui,
                            None,
                            &t!("shape.start_rectangle"),
                            &t!("shape.start_rectangle.tip"),
                            Size::Small,
                            true,
                        )
                        .clicked()
                        {
                            open_rect = true;
                        }
                        if secondary_button(
                            ui,
                            None,
                            &t!("shape.replace_import"),
                            &t!("shape.replace_import.tip"),
                            Size::Small,
                            true,
                        )
                        .clicked()
                        {
                            open_import = true;
                        }
                    });
                }
            },
        );
        if open_rect {
            let (w, h) = self.project.board.extent_size();
            self.outline_ui.rect_w = w;
            self.outline_ui.rect_h = h;
            self.outline_ui.rect_open = true;
        }
        if open_import {
            self.open_dialog(crate::file_dialogs::Mode::Import, None, None, vec![]);
        }
    }

    /// The shape section of a copper layer's Properties: which shape it uses and
    /// the one action that changes it.
    pub(crate) fn layer_shape_properties(&mut self, ui: &mut egui::Ui, path: &str) {
        let Some(key) = copper_key_of_path(&self.project.board, path) else {
            return;
        };
        let own = self.project.board.layer_shapes.contains_key(&key);
        let (points, arcs) = self.project.board.shape_of(&key);
        let words = shape_description(points, arcs);
        let mut give = false;
        let mut follow = false;
        section(
            ui,
            "layer-shape",
            &t!("shape.section.layer", layer = key),
            &t!("shape.layer.tip"),
            |ui| {
                form_row(ui, ROW, &t!("shape.outline"), None, None, |ui| {
                    value_text(
                        ui,
                        &if own {
                            words.clone()
                        } else {
                            t!("shape.follows_board", words = words)
                        },
                    )
                });
                if own {
                    if secondary_button(
                        ui,
                        None,
                        &t!("tree.menu.follow_board"),
                        &t!("tree.menu.follow_board.tip"),
                        Size::Small,
                        true,
                    )
                    .clicked()
                    {
                        follow = true;
                    }
                } else if secondary_button(
                    ui,
                    None,
                    &t!("tree.menu.own_shape"),
                    &t!("tree.menu.own_shape.tip"),
                    Size::Small,
                    true,
                )
                .clicked()
                {
                    give = true;
                }
            },
        );
        if give {
            self.give_layer_own_shape(&key);
        }
        if follow {
            self.layer_follow_board(&key);
        }
    }

    /// The note on a sub-layer's Properties: it follows its copper layer's shape.
    pub(crate) fn sublayer_shape_note(&self, ui: &mut egui::Ui, path: &str) {
        let Some(key) = copper_key_of_path(&self.project.board, path) else {
            return;
        };
        ui.add_space(8.0);
        ui.label(fonts::caption(t!("shape.sub.follows", layer = key)).color(self.muted()));
    }

    /// The "Start from a rectangle" window: the one place a width and a height are typed.
    pub(crate) fn rectangle_window(&mut self, ctx: &egui::Context) {
        if !self.outline_ui.rect_open {
            return;
        }
        let mut open = true;
        let mut go = false;
        let mut cancel = false;
        egui::Window::new(t!("shape.start_rectangle"))
            .id(egui::Id::new("start-rectangle-window"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(fonts::caption(t!("shape.rectangle_window.help")).color(self.muted()));
                ui.horizontal(|ui| {
                    ui.label(t!("props.width"));
                    ui.add(
                        egui::DragValue::new(&mut self.outline_ui.rect_w)
                            .range(1.0..=2000.0)
                            .speed(0.5)
                            .suffix(format!(" {}", t!("unit.mm"))),
                    )
                    .named(&t!("shape.rectangle_window.width"))
                    .on_hover_text(t!("shape.rectangle_window.width.tip"));
                });
                ui.horizontal(|ui| {
                    ui.label(t!("props.height"));
                    ui.add(
                        egui::DragValue::new(&mut self.outline_ui.rect_h)
                            .range(1.0..=2000.0)
                            .speed(0.5)
                            .suffix(format!(" {}", t!("unit.mm"))),
                    )
                    .named(&t!("shape.rectangle_window.height"))
                    .on_hover_text(t!("shape.rectangle_window.height.tip"));
                });
                ui.horizontal(|ui| {
                    if ui
                        .button(t!("shape.rectangle_window.apply"))
                        .on_hover_text(t!("shape.rectangle_window.apply.tip"))
                        .clicked()
                    {
                        go = true;
                    }
                    if ui
                        .button(t!("shape.rectangle_window.cancel"))
                        .on_hover_text(t!("shape.rectangle_window.cancel.tip"))
                        .clicked()
                    {
                        cancel = true;
                    }
                });
            });
        if go {
            let (w, h) = (self.outline_ui.rect_w, self.outline_ui.rect_h);
            self.run("board.set_rectangle", json!({"width": w, "height": h}));
            self.outline_ui.clear_pick();
        }
        if !open || cancel || go {
            self.outline_ui.rect_open = false;
        }
    }
}
