//! The page, tool bar and selected-machine aside of the Machines tab.
//!
//! Context: issue #1167 and the approved machine library mockup. The centre is a page head
//! and a two column grid of machine cards. The tool bar and the right aside work on the
//! selected card, which lives in `MachineLib::selected`.
//!
//! Local rules: this file draws and sends `machine.*` commands through `machine_lib_run`. It
//! reads no machine file. A button whose engine command does not exist yet is drawn off with
//! a tip that says why.
//!
//! # Dependencies
//! - crates/ui-egui/src/library_workspace/machines.rs (state, filters, command runner)
//! - crates/ui-egui/src/ui_kit (button, chip, section, form_row, empty_state, select, icons)
//!
//! # Do not
//! - Name a machine id; the list is whatever the library holds.
//! - Draw a card with the kit `card`; its header has no room for the icon tile and chips.
use super::machines::{card_flag, card_text};
use super::*;
use crate::ui_kit::{
    button::ghost_button,
    chip::{BadgeKind, badge, chip},
    empty_state::empty_state,
    icons::{self, ICON_SIZE, Icon},
    section::section,
    select::select,
};

/// How many cards sit side by side.
const COLUMNS: usize = 2;

/// The icon of a machine kind.
fn kind_icon(kind: &str) -> Icon {
    if kind.starts_with("laser") {
        icons::BOLT
    } else {
        match kind {
            "cnc_mill" => icons::PATH,
            "uv_printer" => icons::LAYERS,
            _ => icons::CHIP,
        }
    }
}

/// The plain name of a machine kind.
fn kind_label(kind: &str) -> String {
    match kind {
        "pick_place" => t!("libws.machines.kind.pick_place"),
        "laser_galvo" => t!("libws.machines.add.kind.laser_galvo"),
        "laser_gantry" => t!("libws.machines.add.kind.laser_gantry"),
        "cnc_mill" => t!("libws.machines.kind.cnc_mill"),
        "uv_printer" => t!("libws.machines.kind.uv_printer"),
        "reflow_oven" => t!("libws.machines.kind.reflow_oven"),
        other => other.replace('_', " "),
    }
}

/// The source chip words and tip of a card.
fn source_words(source: &str) -> (String, String) {
    match source {
        "built_in" => (
            t!("libws.machines.source.built_in.badge"),
            t!("libws.machines.source.built_in.tip"),
        ),
        "mine" => (
            t!("libws.machines.source.mine.badge"),
            t!("libws.machines.source.mine.tip"),
        ),
        _ => (
            t!("libws.machines.source.project.badge"),
            t!("libws.machines.source.project.tip"),
        ),
    }
}

/// The mono summary line of a card: work area and counts.
fn summary_line(c: &Value, catalog: &pcbcraft_text::Catalog) -> String {
    let num = |p: &str| c.pointer(p).and_then(Value::as_f64).unwrap_or(0.0);
    let mut line = t!(
        "libws.machines.summary.area",
        x = num("/work_area/x"),
        y = num("/work_area/y")
    );
    let nozzles = c.get("nozzles").and_then(Value::as_u64).unwrap_or(0);
    if nozzles > 0 {
        line.push_str(" \u{b7} ");
        line.push_str(&catalog.plural("libws.machines.summary.nozzles", nozzles));
    }
    line
}

impl App {
    /// The card of the selected machine, when it is still in the list.
    pub(crate) fn machine_lib_selected_card(&self) -> Option<Value> {
        let id = self.libws.machines.selected.as_deref()?;
        self.libws
            .machines
            .cards
            .iter()
            .find(|c| card_text(c, "id") == id)
            .cloned()
    }

    /// The tool bar of the Machines tab: Add machine, Duplicate, Import, Export, Check for
    /// updates and Sort.
    pub(crate) fn machine_lib_toolbar(&mut self, ui: &mut egui::Ui) {
        let selected = self.machine_lib_selected_card();
        let id = selected
            .as_ref()
            .map(|c| card_text(c, "id").to_owned())
            .unwrap_or_default();
        let behind = selected
            .as_ref()
            .is_some_and(|c| card_text(c, "copy_state") == "library_newer");
        if ghost_button(
            ui,
            Some(icons::PLUS),
            &t!("libws.machines.add"),
            &t!("libws.machines.add.tip"),
            Size::Normal,
            true,
        )
        .clicked()
        {
            self.libws.machines.add = Some(machines::AddForm::default());
        }
        let source = selected.as_ref().map(|c| card_text(c, "source").to_owned());
        if ghost_button(
            ui,
            Some(icons::COPY),
            &t!("libws.machines.duplicate"),
            &if selected.is_some() {
                t!("libws.machines.duplicate.tip")
            } else {
                t!("libws.machines.pick_first")
            },
            Size::Normal,
            selected.is_some(),
        )
        .clicked()
        {
            self.machine_lib_run(
                "machine.duplicate",
                json!({"id": id, "keep_in_my_machines": source.as_deref() != Some("project"), "add_to_project": true}),
            );
        }
        for (icon, label, tip) in [
            (
                icons::DOWNLOAD,
                t!("libws.machines.import"),
                t!("libws.machines.import.off"),
            ),
            (
                icons::UPLOAD,
                t!("libws.machines.export"),
                t!("libws.machines.export.off"),
            ),
        ] {
            ghost_button(ui, Some(icon), &label, &tip, Size::Normal, false);
        }
        ui.separator();
        if ghost_button(
            ui,
            Some(icons::REFRESH),
            &t!("libws.machines.check_updates"),
            &if behind {
                t!("libws.machines.update.tip")
            } else {
                t!("libws.machines.check_updates.none")
            },
            Size::Normal,
            behind,
        )
        .clicked()
        {
            self.machine_lib_run("machine.update_from_library", json!({"id": id}));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(Self::PAGE_GUTTER);
            let mut by_name = 0;
            ui.add_enabled_ui(false, |ui| {
                select(
                    ui,
                    "machine-sort",
                    &mut by_name,
                    &[&t!("libws.machines.sort.name")],
                    &t!("libws.machines.sort"),
                    &t!("libws.machines.sort.off"),
                    theme::SPACE[5] * 3.0,
                );
            });
            ui.label(fonts::text(fonts::Role::Body, t!("libws.machines.sort")).color(self.muted()));
        });
    }

    /// The page head and the grid of cards.
    pub(super) fn machine_lib_page(&mut self, ui: &mut egui::Ui) {
        let muted = self.muted();
        ui.spacing_mut().item_spacing.y = 0.0;
        ui.label(fonts::section_caption(t!("libws.machines.page.caption")).color(muted));
        ui.label(fonts::display(t!("libws.machines.page.title")));
        ui.label(fonts::text(fonts::Role::Body, t!("libws.machines.page.lead")).color(muted));
        ui.add_space(theme::SPACE[3]);
        ui.spacing_mut().item_spacing = egui::vec2(theme::SPACE[3], theme::SPACE[3]);
        let cards: Vec<Value> = self
            .libws
            .machines
            .cards
            .iter()
            .filter(|c| self.machine_lib_shows(c))
            .cloned()
            .collect();
        let keep = self
            .libws
            .machines
            .selected
            .as_deref()
            .is_some_and(|id| cards.iter().any(|c| card_text(c, "id") == id));
        if !keep {
            self.libws.machines.selected = cards.first().map(|c| card_text(c, "id").to_owned());
        }
        if cards.is_empty() {
            ui.add_space(theme::SPACE[5]);
            if empty_state(
                ui,
                icons::GEAR,
                &t!("libws.machines.none.title"),
                &t!("libws.machines.none"),
                &t!("libws.machines.add"),
                &t!("libws.machines.add.tip"),
            )
            .clicked()
            {
                self.libws.machines.add = Some(machines::AddForm::default());
            }
            return;
        }
        for pair in cards.chunks(COLUMNS) {
            ui.columns(COLUMNS, |cols| {
                for (col, c) in cols.iter_mut().zip(pair) {
                    self.machine_lib_card(col, c);
                }
            });
        }
    }

    fn machine_lib_card(&mut self, ui: &mut egui::Ui, c: &Value) {
        let t = theme::tokens(ui.ctx());
        let id = card_text(c, "id").to_owned();
        let name = card_text(c, "name").to_owned();
        let kind = card_text(c, "kind").to_owned();
        let selected = self.libws.machines.selected.as_deref() == Some(id.as_str());
        let copy_state = card_text(c, "copy_state").to_owned();
        let (source_word, source_tip) = source_words(card_text(c, "source"));
        let maker = [card_text(c, "vendor"), card_text(c, "model")]
            .iter()
            .filter(|s| !s.is_empty())
            .copied()
            .collect::<Vec<_>>()
            .join(" \u{b7} ");
        let summary = summary_line(c, &self.catalog);
        let mut action: Option<(&str, Value)> = None;
        let scope = ui.scope_builder(
            egui::UiBuilder::new()
                .id_salt(("machine-card", id.as_str()))
                .sense(egui::Sense::click()),
            |ui| {
                let (fill, stroke) = if selected {
                    (t.copper_tint, egui::Stroke::new(theme::FOCUS_STROKE, t.copper))
                } else {
                    (t.panel, egui::Stroke::new(theme::BORDER, t.line))
                };
                egui::Frame::new()
                    .fill(fill)
                    .stroke(stroke)
                    .corner_radius(egui::CornerRadius::same(theme::RADIUS_CARD))
                    .inner_margin(egui::Margin::same(theme::SPACE[3] as i8))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.spacing_mut().item_spacing.y = theme::SPACE[1];
                        let tile = theme::SPACE[4] + theme::SPACE[5];
                        let (rect, _) =
                            ui.allocate_exact_size(egui::Vec2::splat(tile), egui::Sense::hover());
                        ui.painter().rect(
                            rect,
                            egui::CornerRadius::same(theme::RADIUS_CARD),
                            t.raised,
                            egui::Stroke::new(theme::BORDER, t.line),
                            egui::StrokeKind::Inside,
                        );
                        let glyph = egui::Rect::from_center_size(
                            rect.center(),
                            egui::Vec2::splat(theme::SPACE[4]),
                        );
                        (kind_icon(&kind).0)(ui.painter(), glyph, t.copper);
                        ui.add_space(theme::SPACE[1]);
                        ui.label(fonts::strong(&name).color(t.text));
                        if !maker.is_empty() {
                            ui.label(fonts::caption(&maker).color(t.muted));
                        }
                        ui.horizontal(|ui| {
                            chip(ui, &kind_label(&kind), &t!("libws.machines.chip.kind.tip"));
                            badge(ui, &source_word, BadgeKind::Neutral, &source_tip);
                        });
                        ui.label(fonts::mono(&summary).color(t.muted));
                        let notes = [
                            (copy_state == "library_newer", t!("libws.machines.behind")),
                            (copy_state == "edited", t!("libws.machines.edited")),
                            (card_flag(c, "hidden"), t!("libws.machines.hidden_note")),
                        ];
                        for (shown, note) in notes {
                            if !shown {
                                continue;
                            }
                            ui.horizontal(|ui| {
                                let (r, _) = ui.allocate_exact_size(
                                    egui::Vec2::splat(ICON_SIZE),
                                    egui::Sense::hover(),
                                );
                                (icons::WARNING.0)(ui.painter(), r, t.gold);
                                ui.label(fonts::caption(note).color(t.text));
                            });
                        }
                        ui.add_space(theme::SPACE[1]);
                        ui.horizontal(|ui| {
                            secondary_button(
                                ui,
                                None,
                                &t!("libws.machines.edit"),
                                &t!("libws.machines.edit.off"),
                                Size::Small,
                                false,
                            );
                            if secondary_button(
                                ui,
                                Some(icons::COPY),
                                &t!("libws.machines.duplicate"),
                                &t!("libws.machines.duplicate.tip"),
                                Size::Small,
                                true,
                            )
                            .clicked()
                            {
                                action = Some((
                                    "machine.duplicate",
                                    json!({"id": id, "keep_in_my_machines": card_text(c, "source") != "project", "add_to_project": true}),
                                ));
                            }
                        });
                    });
            },
        );
        let response = scope.response;
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, selected, &name)
        });
        if response.clicked() {
            self.libws.machines.selected = Some(id.clone());
        }
        if let Some((command, params)) = action {
            self.libws.machines.selected = Some(id);
            self.machine_lib_run(command, params);
        }
    }

    /// The right aside of the Machines tab: the selected machine and what can be done with it.
    pub(crate) fn machine_lib_properties(&mut self, ui: &mut egui::Ui) {
        self.machine_lib_refresh();
        let Some(c) = self.machine_lib_selected_card() else {
            ui.add_space(theme::SPACE[5]);
            if empty_state(
                ui,
                icons::GEAR,
                &t!("libws.machines.props.none.title"),
                &t!("libws.machines.props.none"),
                &t!("libws.machines.add"),
                &t!("libws.machines.add.tip"),
            )
            .clicked()
            {
                self.libws.machines.add = Some(machines::AddForm::default());
            }
            return;
        };
        let t = theme::tokens(ui.ctx());
        let (id, source) = (card_text(&c, "id").to_owned(), card_text(&c, "source"));
        let copy_state = card_text(&c, "copy_state").to_owned();
        let in_project = card_flag(&c, "in_project");
        let hidden = card_flag(&c, "hidden");
        let source = source.to_owned();
        let value = |ui: &mut egui::Ui, text: String| {
            ui.label(fonts::text(fonts::Role::Body, text).color(t.text));
        };
        let mono = |ui: &mut egui::Ui, text: String| {
            ui.label(fonts::mono(text).color(t.text));
        };
        let row = |ui: &mut egui::Ui, label: String, f: &dyn Fn(&mut egui::Ui)| {
            form_row(ui, FormWidth::Aside, &label, None, None, |ui| f(ui));
        };
        let mut action: Option<(&str, Value)> = None;
        egui::Frame::NONE
            .inner_margin(egui::Margin::symmetric(0, theme::SPACE[1] as i8))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = theme::ASIDE_ROW_GAP;
                section(
                    ui,
                    "machine-selected",
                    &t!("libws.machines.props.selected"),
                    &t!("libws.machines.props.selected.tip"),
                    |ui| {
                        egui::Frame::NONE
                            .inner_margin(egui::Margin::symmetric(theme::SPACE[2] as i8, 0))
                            .show(ui, |ui| {
                                row(ui, t!("libws.machines.props.name"), &|ui| {
                                    value(ui, card_text(&c, "name").to_owned());
                                });
                                row(ui, t!("libws.machines.props.kind"), &|ui| {
                                    value(ui, kind_label(card_text(&c, "kind")));
                                });
                                row(ui, t!("libws.machines.props.source"), &|ui| {
                                    let version =
                                        c.get("version").and_then(Value::as_u64).unwrap_or(0);
                                    value(
                                        ui,
                                        if version > 0 {
                                            t!(
                                                "libws.machines.props.source_version",
                                                source = source_words(&source).0,
                                                version = version
                                            )
                                        } else {
                                            source_words(&source).0
                                        },
                                    );
                                });
                                row(ui, t!("libws.machines.props.used_by"), &|ui| {
                                    value(
                                        ui,
                                        if in_project {
                                            t!("libws.machines.props.used.yes")
                                        } else {
                                            t!("libws.machines.props.used.no")
                                        },
                                    );
                                });
                            });
                    },
                );
                section(
                    ui,
                    "machine-summary",
                    &t!("libws.machines.props.summary"),
                    &t!("libws.machines.props.summary.tip"),
                    |ui| {
                        egui::Frame::NONE
                            .inner_margin(egui::Margin::symmetric(theme::SPACE[2] as i8, 0))
                            .show(ui, |ui| {
                                let num = |p: &str| {
                                    c.pointer(p).and_then(Value::as_f64).unwrap_or(0.0)
                                };
                                row(ui, t!("libws.machines.props.work_area"), &|ui| {
                                    mono(
                                        ui,
                                        t!(
                                            "libws.machines.props.work_area.value",
                                            x = num("/work_area/x"),
                                            y = num("/work_area/y")
                                        ),
                                    );
                                });
                                let dialect = card_text(&c, "dialect");
                                if !dialect.is_empty() {
                                    row(ui, t!("libws.machines.props.file"), &|ui| {
                                        mono(ui, dialect.to_owned());
                                    });
                                }
                                let nozzles =
                                    c.get("nozzles").and_then(Value::as_u64).unwrap_or(0);
                                if nozzles > 0 {
                                    row(ui, t!("libws.machines.props.nozzles"), &|ui| {
                                        mono(ui, nozzles.to_string());
                                    });
                                }
                            });
                    },
                );
                ui.add_space(theme::SPACE[2]);
                egui::Frame::NONE
                    .inner_margin(egui::Margin::symmetric(theme::SPACE[2] as i8, 0))
                    .show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            if primary_button(
                                ui,
                                Some(icons::PLUS),
                                &t!("libws.machines.use"),
                                &if in_project {
                                    t!("libws.machines.use.done_tip")
                                } else {
                                    t!("libws.machines.use.tip")
                                },
                                Size::Normal,
                                !in_project && source != "project",
                            )
                            .clicked()
                            {
                                action = Some(("machine.install", json!({"id": id})));
                            }
                            if secondary_button(
                                ui,
                                Some(icons::COPY),
                                &t!("libws.machines.edit_copy"),
                                &t!("libws.machines.duplicate.tip"),
                                Size::Normal,
                                true,
                            )
                            .clicked()
                            {
                                action = Some((
                                    "machine.duplicate",
                                    json!({"id": id, "keep_in_my_machines": source != "project", "add_to_project": true}),
                                ));
                            }
                            secondary_button(
                                ui,
                                Some(icons::UPLOAD),
                                &t!("libws.machines.export"),
                                &t!("libws.machines.export.off"),
                                Size::Normal,
                                false,
                            );
                            if copy_state == "library_newer"
                                && secondary_button(
                                    ui,
                                    Some(icons::REFRESH),
                                    &t!("libws.machines.update"),
                                    &t!("libws.machines.update.tip"),
                                    Size::Normal,
                                    true,
                                )
                                .clicked()
                            {
                                action =
                                    Some(("machine.update_from_library", json!({"id": id})));
                            }
                            if source == "built_in" {
                                let (label, tip) = if hidden {
                                    (t!("libws.machines.show"), t!("libws.machines.show.tip"))
                                } else {
                                    (t!("libws.machines.hide"), t!("libws.machines.hide.tip"))
                                };
                                if secondary_button(ui, None, &label, &tip, Size::Normal, true)
                                    .clicked()
                                {
                                    action = Some((
                                        "machine.library_hide",
                                        json!({"id": id, "hidden": !hidden}),
                                    ));
                                }
                            }
                            if source == "project"
                                && secondary_button(
                                    ui,
                                    None,
                                    &t!("libws.machines.save"),
                                    &t!("libws.machines.save.tip"),
                                    Size::Normal,
                                    true,
                                )
                                .clicked()
                            {
                                action = Some(("machine.library_save", json!({"id": id})));
                            }
                            if source == "mine"
                                && secondary_button(
                                    ui,
                                    None,
                                    &t!("libws.machines.remove"),
                                    &t!("libws.machines.remove.tip"),
                                    Size::Normal,
                                    true,
                                )
                                .clicked()
                            {
                                action = Some(("machine.library_remove", json!({"id": id})));
                            }
                        });
                    });
            });
        if let Some((command, params)) = action {
            self.machine_lib_run(command, params);
        }
    }
}
