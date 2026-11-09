//! The right aside of the Files tab: the project's Properties, and the Start from a
//! rectangle dialog.
//!
//! Context: the approved mockup `files`. General (Name, Units, Author, Notes), Shape (Outline,
//! Corners, Reaches, three actions), Layers (copper layer count, Stackup and finish) and
//! Project settings. The board is a free-form 2D shape for a machine. Its size is only the
//! read only reach. The one place two numbers are asked for is the Start from a rectangle
//! dialog, which replaces the shape with a rectangle template as one undo step.
//! Local rules: Name, Author and Notes write when the field loses focus; Units write at
//! once. Every edit is an engine command through `App::files_edit`.
//!
//! # Dependencies
//! - crates/ui-egui/src/shape_preview.rs (`view_of`, `extent_size`)
//! - crates/ui-egui/src/ui_kit/section.rs, form_row.rs, number_field.rs, segmented.rs, dialog.rs
//! - crates/ui-egui/src/project_settings/mod.rs (`text_field`)
//!
//! # Do not
//! - Add a Width or Height field to Properties.
//! - Offer a second place to edit the stackup.
use super::home::file_name;
use super::*;
use crate::shape_preview;
use crate::ui_kit::dialog::{self, Dialog, Outcome};
use crate::ui_kit::form_row::{FormWidth, form_row};
use crate::ui_kit::number_field::{NumberSpec, number_field};
use crate::ui_kit::section::section;
use crate::ui_kit::segmented::{Segment, segmented};

/// The units the Properties aside offers: catalog stem and the value the engine takes.
const UNITS: [(&str, &str); 2] = [("mm", "mm"), ("mil", "mil")];

impl App {
    /// Make the shape a rectangle of `width` by `height`, replacing any free-form outline.
    ///
    /// # Rules
    /// - One undo step: the outline is cleared and the size set in one batch.
    ///
    /// # Do not
    /// - Call it from a path that has not asked the person for both numbers.
    pub(crate) fn files_start_from_rectangle(&mut self, width: f64, height: f64) {
        let commands = json!({"commands": [
            {"command": "board.clear_outline", "params": {}},
            {"command": "board.resize", "params": {"width": width, "height": height}},
        ]});
        self.with_deferred_checks(|app| {
            app.tool_call("batch", commands);
        });
    }

    /// The Start from a rectangle dialog: two numbers, then the shape becomes that template.
    pub(super) fn files_rectangle_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.files_home.rect_draft else {
            return;
        };
        let (title, ok, ok_tip) = (
            t!("files.rect.title"),
            t!("files.rect.apply"),
            t!("files.rect.apply.tip"),
        );
        let (cancel, cancel_tip) = (t!("files.rect.cancel"), t!("files.rect.cancel.tip"));
        let spec = Dialog {
            id: "files-rectangle",
            title: &title,
            width: 380.0,
            tabs: &[],
            note: None,
            cancel: Some((&cancel, &cancel_tip)),
            primary: (&ok, &ok_tip),
        };
        let numbers = NumberSpec {
            min: 1.0,
            max: 2000.0,
            step: 0.5,
            places: 2,
            width: 120.0,
            unit: "mm",
        };
        let mut tab = 0;
        let outcome = dialog::show(ctx, &spec, &mut tab, |ui| {
            let tokens = theme::tokens(ui.ctx());
            ui.label(fonts::text(fonts::Role::Body, t!("files.rect.message")).color(tokens.muted));
            form_row(
                ui,
                FormWidth::Page,
                &t!("files.rect.width"),
                None,
                None,
                |ui| {
                    number_field(
                        ui,
                        "files-rect-width",
                        &mut draft.width,
                        &numbers,
                        &t!("files.rect.width.field"),
                        &t!("files.rect.width.tip"),
                    );
                },
            );
            form_row(
                ui,
                FormWidth::Page,
                &t!("files.rect.height"),
                None,
                None,
                |ui| {
                    number_field(
                        ui,
                        "files-rect-height",
                        &mut draft.height,
                        &numbers,
                        &t!("files.rect.height.field"),
                        &t!("files.rect.height.tip"),
                    );
                },
            );
        });
        match outcome {
            Outcome::Open => self.files_home.rect_draft = Some(draft),
            Outcome::Cancel => self.files_home.rect_draft = None,
            Outcome::Primary => {
                self.files_home.rect_draft = None;
                self.files_start_from_rectangle(draft.width, draft.height);
            }
        }
    }

    /// The right aside of the Files tab: the project's Properties.
    ///
    /// # Rules
    /// - Shape is read only: the outline kind, the corner count and the reach. Changing the
    ///   shape happens in Layout, from a file, or from a rectangle template.
    /// - Layers shows the copper layer count; the stackup is edited in Project settings.
    ///
    /// # Dependencies
    /// - crates/ui-egui/src/ui_kit/section.rs, form_row.rs, segmented.rs
    ///
    /// # Do not
    /// - Edit the board as Width and Height.
    pub(crate) fn files_properties(&mut self, ui: &mut egui::Ui) {
        const ROW: FormWidth = FormWidth::Aside;
        let tokens = theme::tokens(ui.ctx());
        self.properties_header(ui, &t!("files.props.title"), &file_name(&self.path));
        let project_name_now = self.project.name.clone();
        let author = self.project.meta.get("author").cloned().unwrap_or_default();
        let notes = self.project.meta.get("notes").cloned().unwrap_or_default();
        let unit = self.project.unit.clone();
        let layers = self.project.copper_layer_count();
        let view = shape_preview::view_of(&self.project.board);
        let (reach_w, reach_h) = shape_preview::extent_size(&self.project.board);
        section(
            ui,
            "files-general",
            &t!("files.props.general"),
            &t!("files.props.general.tip"),
            |ui| {
                let label = t!("files.props.name");
                if let Some(text) = form_row(ui, ROW, &label, None, None, |ui| {
                    let room = ui.available_width();
                    crate::project_settings::text_field(
                        ui,
                        "files-name",
                        &project_name_now,
                        room,
                        &t!("files.props.name.field"),
                        &t!("files.props.name.tip"),
                    )
                }) && !text.trim().is_empty()
                {
                    self.files_edit("project.rename", json!({"name": text.trim()}));
                }
                let labels: Vec<String> = UNITS
                    .iter()
                    .map(|(stem, _)| {
                        pcbcraft_text::lookup(&format!("files.props.unit.{stem}"), &[])
                    })
                    .collect();
                let tips: Vec<String> = UNITS
                    .iter()
                    .map(|(stem, _)| {
                        pcbcraft_text::lookup(&format!("files.props.unit.{stem}.tip"), &[])
                    })
                    .collect();
                let items: Vec<Segment<'_>> = labels
                    .iter()
                    .zip(&tips)
                    .map(|(label, tip)| Segment::text(label, tip))
                    .collect();
                let mut at = UNITS.iter().position(|(_, v)| *v == unit).unwrap_or(0);
                let before = at;
                form_row(ui, ROW, &t!("files.props.units"), None, None, |ui| {
                    segmented(ui, "files-units", &mut at, &items);
                });
                if at != before {
                    self.files_edit("project.set_unit", json!({"unit": UNITS[at].1}));
                }
                let label = t!("files.props.author");
                if let Some(text) = form_row(ui, ROW, &label, None, None, |ui| {
                    let room = ui.available_width();
                    crate::project_settings::text_field(
                        ui,
                        "files-author",
                        &author,
                        room,
                        &t!("files.props.author.field"),
                        &t!("files.props.author.tip"),
                    )
                }) {
                    self.files_meta("author", text.trim());
                }
                let label = t!("files.props.notes");
                if let Some(text) = form_row(ui, ROW, &label, None, None, |ui| {
                    let room = ui.available_width();
                    crate::project_settings::text_field(
                        ui,
                        "files-notes",
                        &notes,
                        room,
                        &t!("files.props.notes.field"),
                        &t!("files.props.notes.tip"),
                    )
                }) {
                    self.files_meta("notes", text.trim());
                }
            },
        );
        let (shape_name, _) = self.shape_words(&view);
        section(
            ui,
            "files-shape",
            &t!("files.props.shape"),
            &t!("files.props.shape.tip"),
            |ui| {
                let value = |ui: &mut egui::Ui, text: String, tip: String| {
                    ui.label(fonts::mono(text).color(tokens.text))
                        .on_hover_text(tip);
                };
                form_row(ui, ROW, &t!("files.props.outline"), None, None, |ui| {
                    let kind = if shape_preview::is_plain_rectangle(&view) {
                        shape_name.clone()
                    } else {
                        t!("files.shape.free_form")
                    };
                    value(ui, kind, t!("files.props.outline.tip"));
                });
                form_row(ui, ROW, &t!("files.props.corners"), None, None, |ui| {
                    value(ui, view.corners.to_string(), t!("files.props.corners.tip"));
                });
                form_row(ui, ROW, &t!("files.props.reaches"), None, None, |ui| {
                    value(
                        ui,
                        t!(
                            "files.props.reaches.value",
                            width = super::shape_cards::reach_text(reach_w),
                            height = super::shape_cards::reach_text(reach_h)
                        ),
                        t!("files.props.reaches.tip"),
                    );
                });
                ui.label(fonts::caption(t!("files.props.reaches.hint")).color(tokens.muted));
                ui.add_space(theme::SPACE[1]);
                if button::primary_button(
                    ui,
                    Some(icons::PATH),
                    &t!("files.shape.edit"),
                    &t!("files.shape.edit.tip"),
                    Size::Normal,
                    true,
                )
                .clicked()
                {
                    self.files_edit_shape();
                }
                if button::secondary_button(
                    ui,
                    Some(icons::DOWNLOAD),
                    &t!("files.shape.replace"),
                    &t!("files.shape.replace.tip"),
                    Size::Normal,
                    true,
                )
                .clicked()
                {
                    self.show_files(Page::Import);
                }
                if button::ghost_button(
                    ui,
                    None,
                    &t!("files.shape.rectangle_start"),
                    &t!("files.shape.rectangle_start.tip"),
                    Size::Normal,
                    true,
                )
                .clicked()
                {
                    self.files_home.rect_draft = Some(RectDraft {
                        width: reach_w.max(1.0),
                        height: reach_h.max(1.0),
                    });
                }
            },
        );
        section(
            ui,
            "files-layers",
            &t!("files.props.layers_section"),
            &t!("files.props.layers.tip"),
            |ui| {
                form_row(ui, ROW, &t!("files.props.layers"), None, None, |ui| {
                    ui.label(fonts::mono(layers.to_string()).color(tokens.text))
                        .on_hover_text(t!("files.props.layers.tip"));
                });
                if button::secondary_button(
                    ui,
                    Some(icons::LAYERS),
                    &t!("files.props.stackup"),
                    &t!("files.props.stackup.tip"),
                    Size::Normal,
                    true,
                )
                .clicked()
                {
                    self.open_project_settings(crate::project_settings::TAB_BOARD);
                }
            },
        );
        ui.add_space(theme::SPACE[1]);
        if button::secondary_button(
            ui,
            Some(icons::GEAR),
            &t!("files.props.settings"),
            &t!("files.props.settings.tip"),
            Size::Normal,
            true,
        )
        .clicked()
        {
            self.open_project_settings(crate::project_settings::TAB_BOARD);
        }
    }
}
