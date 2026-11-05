//! Schematic sheet text tool: put a free-standing note on the sheet.
//!
//! Context: the Add menu arms the tool. A click on the canvas stores the spot
//! and opens a one-line field beside it. Enter places the note with the engine
//! command `graphic.add`, Esc drops it.
//! Local rules: the typed draft lives on the app until it is placed or dropped;
//! the document changes only when the note is placed.
//!
//! # Dependencies
//! - crates/engine/src/lib.rs (`graphic.add`)
//! - crates/ui-egui/src/a11y.rs (`Named` gives the field its spoken name)
//! - crates/ui-egui/src/targets.rs (`text_field`)
//! - crates/text/strings/en.txt (every label and tooltip)
//!
//! # Do not
//! - Use this on the board tab; the note belongs to the schematic only.

use super::*;
use crate::a11y::Named;
use crate::canvas::snap;
use crate::canvas::text_style::{TextRole, role_font};

/// Stable id of the note field, so focus survives a repaint.
const FIELD_ID: &str = "sheet-text-field";

impl App {
    /// Handle a click while the sheet text tool is armed.
    ///
    /// # Rules
    /// - The click stores the snapped spot, clears the draft and opens the field.
    /// - A second click moves the spot and keeps what was typed.
    ///
    /// # Do not
    /// - Place anything here; placing is Enter in the field.
    pub(crate) fn place_sheet_text(&mut self, pos: Point) {
        let moved = self.sheet_text_position.is_some();
        self.sheet_text_position = Some(snap(pos));
        if !moved {
            self.sheet_text_input.clear();
        }
    }

    /// Place the typed note at the stored spot.
    ///
    /// # Rules
    /// - Empty or blank text places nothing.
    /// - After the call the tool is Select and the draft is gone.
    ///
    /// # Dependencies
    /// - crates/engine/src/lib.rs (`graphic.add`: a text graphic has one point)
    pub(crate) fn submit_sheet_text(&mut self) {
        let Some(at) = self.sheet_text_position else {
            return;
        };
        let text = self.sheet_text_input.trim().to_owned();
        if !text.is_empty() {
            self.run(
                "graphic.add",
                json!({
                    "layer": "schematic",
                    "shape": "text",
                    "points": [at],
                    "width": TextRole::Reference.mm(),
                    "text": text,
                }),
            );
        }
        self.cancel_sheet_text();
    }

    /// Drop the draft and go back to Select.
    pub(crate) fn cancel_sheet_text(&mut self) {
        self.sheet_text_position = None;
        self.sheet_text_input.clear();
        self.tool = Tool::Select;
    }

    /// The field beside the clicked spot while the tool is armed.
    ///
    /// # Rules
    /// - Shown only on the Schematic tab, with the tool armed and a spot stored.
    /// - Enter places, Esc cancels, the field keeps focus until then.
    /// - A tool change by other means drops the draft.
    pub(crate) fn sheet_text_prompt(&mut self, ctx: &egui::Context) {
        if !matches!(self.tool, Tool::SheetText) || self.tab != Tab::Schematic {
            self.sheet_text_position = None;
            return;
        }
        let Some(at) = self.sheet_text_position else {
            return;
        };
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.cancel_sheet_text();
            return;
        }
        let camera = self.schematic;
        let spot = camera.screen(at, self.canvas_rect) + egui::vec2(8.0, 8.0);
        let font = role_font(TextRole::NetName, camera.zoom.clamp(5.0, 8.0));
        let mut enter = false;
        egui::Area::new(egui::Id::new("sheet-text-prompt"))
            .order(egui::Order::Foreground)
            .fixed_pos(spot)
            .constrain_to(self.canvas_rect)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    let field = ui
                        .add(
                            crate::targets::text_field(&mut self.sheet_text_input)
                                .id(egui::Id::new(FIELD_ID))
                                .hint_text(t!("schematic.text.field.hint"))
                                .font(font)
                                .desired_width(220.0),
                        )
                        .on_hover_text(t!("schematic.text.field.tip"))
                        .named(&t!("schematic.text.field"));
                    if field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        enter = true;
                    } else {
                        field.request_focus();
                    }
                });
            });
        if enter {
            self.submit_sheet_text();
        }
    }
}
