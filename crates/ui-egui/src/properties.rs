//! The Properties aside built from kit sections and form rows.
//!
//! Context: the Layout mockup. A header (reference, kind), then collapsible sections
//! GENERAL, POSITION, PINS and MANUFACTURING for a part, a GENERAL section for other
//! objects and a BOARD / STACKUP pair for the board root, with the action buttons last.
//! Local rules: every edit goes through an engine command (`component.value`,
//! `footprint.move`, `footprint.rotate`, `footprint.flip`, `footprint.set_locked`,
//! `board.resize`); a field commits on Enter or when it loses focus; every control has
//! a tooltip from the catalog. The Sourcing section lives in `sourcing_panel.rs`.
//!
//! # Dependencies
//! - crates/ui-egui/src/ui_kit/section.rs, form_row.rs, number_field.rs, segmented.rs
//! - crates/ui-egui/src/lib.rs (`run`, `duplicate_selection`, `delete_selection`)
//!
//! # Do not
//! - Keep state of the document here; read it from `App::project` each frame.
use super::*;
use crate::fonts::{self, Role};
use crate::theme;
use crate::ui_kit::{
    button::{Size, ghost_button, secondary_button},
    chip::chip,
    form_row::{FormWidth, form_row, toggle_row},
    number_field::{NumberSpec, number_field},
    section::section,
    segmented::{Segment, segmented},
};

const ROW: FormWidth = FormWidth::Aside;

/// A read-only value in a form row: mono text, muted when empty.
fn value_text(ui: &mut egui::Ui, text: &str) {
    let t = theme::tokens(ui.ctx());
    ui.label(fonts::text(Role::Mono, text).color(t.text));
}

impl App {
    /// The header of the Properties aside: reference or name, then a caption line.
    pub(crate) fn properties_header(&self, ui: &mut egui::Ui, title: &str, caption: &str) {
        let t = theme::tokens(ui.ctx());
        egui::Frame::NONE
            .inner_margin(egui::Margin::ZERO)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(fonts::title(title).color(t.text));
                if !caption.is_empty() {
                    ui.label(fonts::caption(caption).color(t.muted));
                }
            });
    }

    /// A part (schematic symbol or board footprint) as sections.
    ///
    /// # Rules
    /// - Value commits as `component.value`; X, Y and Rotation commit as moves.
    /// - Side flips the footprint; Lock position toggles `footprint.set_locked`.
    ///
    /// # Dependencies
    /// - crates/engine/src/lib.rs for the commands named above.
    ///
    /// # Do not
    /// - Offer a parts-list or placement toggle: the document has no per-part flag for them.
    pub(crate) fn part_properties(
        &mut self,
        ui: &mut egui::Ui,
        id: Id,
        c: &pcbcraft_doc::Component,
    ) {
        let t = theme::tokens(ui.ctx());
        self.properties_header(ui, &c.reference, &c.kind);
        let footprint = self.project.footprint(id).cloned();
        let on_board = footprint.is_some();
        section(
            ui,
            "general",
            &t!("props.general"),
            &t!("props.general.tip"),
            |ui| {
                form_row(ui, ROW, &t!("props.reference"), None, None, |ui| {
                    value_text(ui, &c.reference);
                });
                if !c.units.is_empty() && self.project.component(id).is_some() {
                    let before = self.selected_unit;
                    form_row(ui, ROW, &t!("props.unit"), None, None, |ui| {
                        egui::ComboBox::from_id_salt("selected-symbol-unit")
                            .selected_text(t!(
                                "board.inspector.unit_selected_unit",
                                selected_unit = self.selected_unit
                            ))
                            .show_ui(ui, |ui| {
                                for unit in std::iter::once(1).chain(c.units.iter().map(|u| u.unit))
                                {
                                    ui.selectable_value(
                                        &mut self.selected_unit,
                                        unit,
                                        t!("board.inspector.unit_unit", unit = unit),
                                    )
                                    .on_hover_text(t!("board.inspector.unit_unit.tip"));
                                }
                            })
                            .response
                            .named("Part unit");
                    });
                    if before != self.selected_unit {
                        self.load_properties();
                    }
                }
                let mut commit = false;
                form_row(
                    ui,
                    ROW,
                    &t!("props.value"),
                    Some(&t!("props.value.hint")),
                    None,
                    |ui| {
                        let r = ui
                            .add(
                                crate::targets::text_field(&mut self.value)
                                    .desired_width(f32::INFINITY),
                            )
                            .named(&t!("board.inspector.value"))
                            .on_hover_text(t!("props.value.hint"));
                        commit = r.lost_focus() && self.value != c.value;
                    },
                );
                if commit {
                    self.run("component.value", json!({"id": c.id, "value": self.value}));
                }
                let footprint_name = c
                    .library_footprint
                    .as_ref()
                    .map(|f| f.definition.name.clone())
                    .unwrap_or_else(|| {
                        if c.library_symbol.is_some() {
                            t!("props.footprint_none")
                        } else {
                            t!("props.footprint_starter")
                        }
                    });
                form_row(ui, ROW, &t!("props.footprint"), None, None, |ui| {
                    value_text(ui, &footprint_name);
                });
            },
        );
        section(
            ui,
            "position",
            &t!("props.position"),
            &t!("props.position.tip"),
            |ui| {
                let spec = NumberSpec {
                    width: 110.0,
                    ..NumberSpec::default()
                };
                let (mut x, mut y) = (self.x, self.y);
                let moved_x = form_row(ui, ROW, "X", None, None, |ui| {
                    number_field(
                        ui,
                        "prop-x",
                        &mut x,
                        &spec,
                        "X position",
                        &t!("props.x.tip"),
                    )
                    .changed()
                });
                let moved_y = form_row(ui, ROW, "Y", None, None, |ui| {
                    number_field(
                        ui,
                        "prop-y",
                        &mut y,
                        &spec,
                        "Y position",
                        &t!("props.y.tip"),
                    )
                    .changed()
                });
                if moved_x || moved_y {
                    self.x = x;
                    self.y = y;
                    let mut params = json!({"id": id, "position": {"x": x, "y": y}});
                    if self.project.component(id).is_some() {
                        params["unit"] = json!(self.selected_unit);
                    }
                    self.run(
                        if on_board {
                            "footprint.move"
                        } else {
                            "component.move"
                        },
                        params,
                    );
                }
                let rotation = footprint.as_ref().map_or_else(
                    || {
                        c.unit_placement(self.selected_unit)
                            .map_or(c.rotation, |p| p.1)
                    },
                    |f| f.rotation,
                );
                let mut degrees = rotation;
                let spin = NumberSpec {
                    unit: "°",
                    step: 90.0,
                    min: -360.0,
                    max: 360.0,
                    places: 0,
                    width: 110.0,
                };
                let turned = form_row(ui, ROW, &t!("props.rotation"), None, None, |ui| {
                    number_field(
                        ui,
                        "prop-rot",
                        &mut degrees,
                        &spin,
                        "Rotation",
                        &t!("props.rotation.tip"),
                    )
                    .changed()
                });
                if turned {
                    if on_board {
                        self.run("footprint.rotate", json!({"id": id, "degrees": degrees}));
                    } else {
                        self.run(
                            "component.rotate",
                            json!({"id": id, "unit": self.selected_unit, "degrees": degrees}),
                        );
                    }
                }
                if on_board {
                    let bottom = footprint
                        .as_ref()
                        .is_some_and(|f| f.board_side() == pcbcraft_doc::Layer::Back);
                    let mut side = usize::from(bottom);
                    let (top_label, bottom_label) =
                        (t!("workspace.side_top"), t!("workspace.side_bottom"));
                    let (top_tip, bottom_tip) =
                        (t!("props.side_top.tip"), t!("props.side_bottom.tip"));
                    form_row(ui, ROW, &t!("props.side"), None, None, |ui| {
                        segmented(
                            ui,
                            "prop-side",
                            &mut side,
                            &[
                                Segment::text(&top_label, &top_tip),
                                Segment::text(&bottom_label, &bottom_tip),
                            ],
                        );
                    });
                    if side != usize::from(bottom) {
                        self.run("footprint.flip", json!({"id": id}));
                    }
                    ui.label(
                        fonts::caption(t!("board.inspector.dock_to_board_edge")).color(t.muted),
                    )
                    .on_hover_text(t!("board.inspector.for_an_edge_connector_the"));
                    ui.horizontal_wrapped(|ui| {
                        for edge in ["left", "right", "top", "bottom"] {
                            if secondary_button(
                                ui,
                                None,
                                &t!("board.inspector.dock_edge", edge = edge),
                                &t!("board.inspector.dock_edge.tip", edge = edge),
                                Size::Small,
                                true,
                            )
                            .clicked()
                            {
                                self.run("footprint.dock_edge", json!({"id": id, "edge": edge}));
                            }
                        }
                    });
                }
            },
        );
        section(ui, "pins", &t!("props.pins"), &t!("props.pins.tip"), |ui| {
            for pin in &c.pins {
                let net = pin.net.map(|n| self.project.net_name(n).to_owned());
                let name = if pin.name.is_empty() {
                    String::new()
                } else {
                    pin.name.clone()
                };
                form_row(ui, ROW, &pin.number, None, None, |ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        match &net {
                            Some(net) => {
                                chip(ui, net, &t!("props.pin_net.tip"));
                            }
                            None => {
                                ui.label(fonts::caption(t!("props.pin_unassigned")).color(t.muted));
                            }
                        }
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            value_text(ui, &name);
                        });
                    });
                });
            }
        });
        section(
            ui,
            "manufacturing",
            &t!("props.manufacturing"),
            &t!("props.manufacturing.tip"),
            |ui| {
                if let Some(f) = &footprint {
                    let mut locked = f.locked == Some(true);
                    let was = locked;
                    toggle_row(
                        ui,
                        ROW,
                        &t!("board.inspector.lock_in_place"),
                        &t!("board.inspector.freeze_this_part_so_no"),
                        &mut locked,
                    );
                    if locked != was {
                        self.run("footprint.set_locked", json!({"id": id, "locked": locked}));
                    }
                }
                let sheet = Self::datasheet_url(c.field(&t!("board.inspector.datasheet")));
                form_row(ui, ROW, &t!("props.datasheet"), None, None, |ui| {
                    if ghost_button(
                        ui,
                        None,
                        &t!("board.inspector.view_datasheet"),
                        &if sheet.is_some() {
                            t!("board.inspector.open_this_part_s_datasheet")
                        } else {
                            t!("board.inspector.this_part_has_no_web")
                        },
                        Size::Small,
                        sheet.is_some(),
                    )
                    .clicked()
                        && let Some(url) = sheet.clone()
                    {
                        ui.ctx().open_url(egui::OpenUrl::new_tab(url));
                    }
                });
            },
        );
        self.sourcing_section(ui, c);
        egui::Frame::NONE
            .inner_margin(egui::Margin::ZERO)
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    if secondary_button(
                        ui,
                        None,
                        &t!("board.inspector.rotate_90_r"),
                        &command_tip("component.rotate"),
                        Size::Normal,
                        true,
                    )
                    .clicked()
                    {
                        self.rotate();
                    }
                    if secondary_button(
                        ui,
                        None,
                        &t!("board.inspector.duplicate"),
                        &t!("board.inspector.duplicate.tip"),
                        Size::Normal,
                        true,
                    )
                    .clicked()
                    {
                        self.duplicate_selection();
                    }
                    if secondary_button(
                        ui,
                        None,
                        &t!("board.inspector.assign_footprint_in_libraries"),
                        &t!("board.inspector.assign_footprint_in_libraries.tip"),
                        Size::Normal,
                        true,
                    )
                    .clicked()
                    {
                        self.change_tab(Tab::Libraries);
                        self.library.kind = true;
                        self.library.pending = None;
                        self.library.path.clear();
                        self.library.results = None;
                        self.library.preview = None;
                        self.library.query.clear();
                    }
                    if secondary_button(
                        ui,
                        None,
                        &t!("board.inspector.delete_del"),
                        &command_tip("object.delete"),
                        Size::Normal,
                        true,
                    )
                    .clicked()
                    {
                        self.delete_selection();
                    }
                });
            });
    }

    /// Any other selected object (track, via, zone, text, label): a GENERAL section with
    /// its detail rows, then the object's own controls drawn by the caller.
    pub(crate) fn object_general(&mut self, ui: &mut egui::Ui, id: Id) {
        let rows = object_detail_rows(&self.project, id);
        let title = t!("board.inspector.selected_object");
        let caption = t!("board.inspector.stable_id_id", id = id);
        self.properties_header(ui, &title, &caption);
        section(
            ui,
            "general",
            &t!("props.general"),
            &t!("props.general.tip"),
            |ui| {
                for (name, value) in &rows {
                    form_row(ui, ROW, name, None, None, |ui| value_text(ui, value));
                }
            },
        );
    }

    /// The board root: size, clearance and a stackup summary, then the stackup editor.
    pub(crate) fn board_root_properties(&mut self, ui: &mut egui::Ui) {
        let t = theme::tokens(ui.ctx());
        self.properties_header(
            ui,
            &t!("tree.board_settings"),
            &t!("tree.board_settings.help"),
        );
        let (w, h) = (self.project.board.width, self.project.board.height);
        let clearance = self.project.board.clearance;
        let copper = self
            .project
            .board
            .layer_records
            .iter()
            .filter(|r| r.kind == "copper")
            .count()
            .max(2);
        let thickness: f64 = self
            .project
            .board
            .layer_records
            .iter()
            .map(|r| r.thickness)
            .sum();
        section(
            ui,
            "board-size",
            &t!("props.board"),
            &t!("props.board.tip"),
            |ui| {
                let spec = NumberSpec {
                    min: 1.0,
                    max: 2000.0,
                    step: 0.5,
                    width: 110.0,
                    ..NumberSpec::default()
                };
                let (mut nw, mut nh) = (w, h);
                let cw = form_row(ui, ROW, &t!("props.width"), None, None, |ui| {
                    number_field(
                        ui,
                        "board-w",
                        &mut nw,
                        &spec,
                        "Board width",
                        &t!("settings.settings_tab.how_wide_the_board_is.tip"),
                    )
                    .changed()
                });
                let ch = form_row(ui, ROW, &t!("props.height"), None, None, |ui| {
                    number_field(
                        ui,
                        "board-h",
                        &mut nh,
                        &spec,
                        "Board height",
                        &t!("settings.settings_tab.how_tall_the_board_is.tip"),
                    )
                    .changed()
                });
                if cw || ch {
                    self.width = nw;
                    self.height = nh;
                    self.run("board.resize", json!({"width": nw, "height": nh}));
                }
                form_row(
                    ui,
                    ROW,
                    &t!("props.clearance"),
                    Some(&t!("props.clearance.hint")),
                    None,
                    |ui| {
                        value_text(ui, &format!("{clearance:.2} mm"));
                    },
                );
            },
        );
        section(
            ui,
            "board-stackup",
            &t!("props.stackup"),
            &t!("props.stackup.tip"),
            |ui| {
                form_row(ui, ROW, &t!("props.copper_layers"), None, None, |ui| {
                    value_text(ui, &copper.to_string());
                });
                form_row(ui, ROW, &t!("props.thickness"), None, None, |ui| {
                    value_text(ui, &format!("{thickness:.2} mm"));
                });
                ui.add_space(theme::SPACE[1]);
                self.stackup_section(ui);
            },
        );
        egui::Frame::NONE
            .inner_margin(egui::Margin::ZERO)
            .show(ui, |ui| {
                ui.label(fonts::caption(t!("props.board_more")).color(t.muted));
                self.holes_section(ui);
                self.datums_section(ui);
                self.fiducials_section(ui);
                self.via_spans_section(ui);
            });
    }
}
