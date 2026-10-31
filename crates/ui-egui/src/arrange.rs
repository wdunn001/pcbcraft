//! Align, distribute and array placement. Menu items and a small dialog that
//! call the typed `align.distribute`, `footprint.array` and
//! `components.reannotate` engine commands; nothing here moves an object by
//! itself, so every action is one undo step (numbering is a second step).
//!
//! Local rules: Align and Distribute need two or more selected objects; the
//! array dialog works on the Layout tab only, because the engine has no
//! schematic array.

use super::*;

/// The most rows or columns the engine accepts in one array.
pub(super) const ARRAY_MAX: u32 = 64;
/// Largest pitch, in mm, the engine accepts.
pub(super) const PITCH_MAX: f64 = 10_000.0;

/// The eight modes of `align.distribute`: menu label, engine mode, tooltip.
const MODES: [(&str, &str, &str); 8] = [
    (
        "Align left",
        "left",
        "Line the left edges up with the leftmost selected object.",
    ),
    (
        "Align centres",
        "center_x",
        "Line the centres up on one vertical line.",
    ),
    (
        "Align right",
        "right",
        "Line the right edges up with the rightmost selected object.",
    ),
    (
        "Align top",
        "top",
        "Line the top edges up with the topmost selected object.",
    ),
    (
        "Align middles",
        "center_y",
        "Line the centres up on one horizontal line.",
    ),
    (
        "Align bottom",
        "bottom",
        "Line the bottom edges up with the lowest selected object.",
    ),
    (
        "Distribute horizontally",
        "distribute_x",
        "Space the objects evenly from the leftmost to the rightmost. The two outer ones stay put.",
    ),
    (
        "Distribute vertically",
        "distribute_y",
        "Space the objects evenly from the topmost to the lowest. The two outer ones stay put.",
    ),
];

/// What the array dialog remembers while it is open.
pub(super) struct ArrangeState {
    pub open: bool,
    pub rows: u32,
    pub columns: u32,
    pub pitch_x: f64,
    pub pitch_y: f64,
    pub renumber: bool,
}

impl Default for ArrangeState {
    fn default() -> Self {
        Self {
            open: false,
            rows: 1,
            columns: 4,
            pitch_x: 5.0,
            pitch_y: 5.0,
            renumber: false,
        }
    }
}

impl App {
    /// The footprint an array would copy: the primary selection, or the
    /// footprint of the selected part.
    pub(super) fn array_source(&self) -> Option<Id> {
        let id = self.selected?;
        if self.project.footprint(id).is_some() {
            return Some(id);
        }
        self.project
            .board
            .footprints
            .iter()
            .find(|f| f.component == id)
            .map(|f| f.id)
    }

    /// Run one align or distribute mode on the selection.
    ///
    /// # Rules
    /// Needs two selected objects; the engine commits it as one undo step.
    /// # Dependencies
    /// `align.distribute` in ../../engine/src/lib.rs.
    /// # Do not
    /// Move objects here; the engine owns the geometry.
    pub(super) fn align_selection(&mut self, mode: &str) {
        let ids = self.selection();
        if ids.len() < 2 {
            self.message = "Select at least two objects first".into();
            return;
        }
        self.run("align.distribute", json!({"ids": ids, "mode": mode}));
    }

    /// Align and Distribute entries for the Edit menu.
    pub(super) fn arrange_menu(&mut self, ui: &mut egui::Ui) {
        let enough = self.selection().len() >= 2;
        ui.menu_button("Align and distribute", |ui| {
            for (label, mode, tip) in MODES {
                if mode == "distribute_x" {
                    ui.separator();
                }
                if ui
                    .add_enabled(enough, egui::Button::new(label))
                    .on_hover_text(tip)
                    .on_disabled_hover_text("Select at least two objects first.")
                    .clicked()
                {
                    self.align_selection(mode);
                    ui.close();
                }
            }
        })
        .response
        .on_hover_text("Line up or space out the selected objects. Needs two or more.");
        let layout = self.tab == Tab::Pcb;
        let source = self.array_source();
        if ui
            .add_enabled(layout && source.is_some(), egui::Button::new("Array..."))
            .on_hover_text("Make rows and columns of copies of the selected part.")
            .on_disabled_hover_text(if layout {
                "Select a part on the board first."
            } else {
                "Arrays work on the Layout tab only."
            })
            .clicked()
        {
            self.open_array_dialog();
            ui.close();
        }
    }

    /// Open the array dialog with a pitch just wider than the part.
    pub(super) fn open_array_dialog(&mut self) {
        let Some(source) = self.array_source() else {
            self.message = "Select a part on the board first".into();
            return;
        };
        if let Some((lo, hi)) = self.bounds_of(source) {
            self.edit.arrange.pitch_x = ((hi.x - lo.x) + 1.0).clamp(0.1, PITCH_MAX);
            self.edit.arrange.pitch_y = ((hi.y - lo.y) + 1.0).clamp(0.1, PITCH_MAX);
        }
        self.edit.arrange.open = true;
    }

    /// Run the array (and the optional renumbering) from the dialog values.
    ///
    /// # Rules
    /// Bounds are the engine's; a refusal shows its reason in the message.
    /// Renumbering is a second command, so it is a second undo step.
    /// # Dependencies
    /// `footprint.array`, `components.reannotate` in ../../engine/src/lib.rs.
    pub(super) fn run_array(&mut self) {
        let Some(source) = self.array_source() else {
            self.message = "Select a part on the board first".into();
            return;
        };
        let a = &self.edit.arrange;
        let params = json!({
            "footprint": source, "columns": a.columns, "rows": a.rows,
            "dx": a.pitch_x, "dy": a.pitch_y,
        });
        let renumber = a.renumber;
        let letters: String = self
            .array_reference(source)
            .chars()
            .take_while(char::is_ascii_alphabetic)
            .collect();
        let before = self.project.components.len();
        self.run("footprint.array", params);
        if self.project.components.len() == before {
            return; // refused: the message holds the engine reason
        }
        self.edit.arrange.open = false;
        if renumber && !letters.is_empty() {
            self.run(
                "components.reannotate",
                json!({"prefix": letters, "start": 1}),
            );
        }
    }

    fn array_reference(&self, footprint: Id) -> String {
        self.project
            .footprint(footprint)
            .and_then(|f| self.project.component(f.component))
            .map(|c| c.reference.clone())
            .unwrap_or_default()
    }

    /// The array dialog window.
    pub(super) fn array_window(&mut self, ctx: &egui::Context) {
        if !self.edit.arrange.open {
            return;
        }
        let mut open = true;
        let mut go = false;
        let mut cancel = false;
        egui::Window::new("Array")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.add_space(6.0);
                let a = &mut self.edit.arrange;
                egui::Grid::new("array-grid").spacing([16.0, 8.0]).show(ui, |ui| {
                    ui.label("Rows");
                    ui.add(egui::DragValue::new(&mut a.rows).range(1..=ARRAY_MAX))
                        .on_hover_text("How many rows, 1 to 64.");
                    ui.end_row();
                    ui.label("Columns");
                    ui.add(egui::DragValue::new(&mut a.columns).range(1..=ARRAY_MAX))
                        .on_hover_text("How many columns, 1 to 64.");
                    ui.end_row();
                    ui.label("Pitch across (mm)");
                    ui.add(egui::DragValue::new(&mut a.pitch_x).range(0.1..=PITCH_MAX).speed(0.1))
                        .on_hover_text("Distance between copies left to right, up to 10000 mm.");
                    ui.end_row();
                    ui.label("Pitch down (mm)");
                    ui.add(egui::DragValue::new(&mut a.pitch_y).range(0.1..=PITCH_MAX).speed(0.1))
                        .on_hover_text("Distance between copies top to bottom, up to 10000 mm.");
                    ui.end_row();
                });
                ui.add_space(6.0);
                ui.checkbox(&mut a.renumber, "Renumber references")
                    .on_hover_text(
                        "Number every part with the same letters 1, 2, 3 by position. Copies are named like R1-2 until you do. This is a second undo step.",
                    );
                ui.add_space(6.0);
                ui.label(format!(
                    "Makes {} copies of the part (at most 64 by 64).",
                    (a.rows * a.columns).saturating_sub(1)
                ));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(a.rows * a.columns >= 2, egui::Button::new("Make array"))
                        .on_hover_text("Place the copies. One undo removes them.")
                        .on_disabled_hover_text("Pick at least two places: more rows or columns.")
                        .clicked()
                    {
                        go = true;
                    }
                    if ui.button("Cancel").on_hover_text("Close without changing the board.").clicked() {
                        cancel = true;
                    }
                });
                ui.add_space(6.0);
            });
        if go {
            self.run_array();
        }
        if !open || cancel {
            self.edit.arrange.open = false;
        }
    }
}
