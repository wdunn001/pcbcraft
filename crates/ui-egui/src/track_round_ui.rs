//! The Round corners control: an Edit menu item, a small dialog with a radius
//! field, and a button in the track inspector. All three send the typed
//! `track.round_corners` engine command, so one press is one undo step and
//! the same code runs in the browser build.
//!
//! Local rules: the command works on the Layout tab only; it rounds the
//! corners of the selected tracks, or of every track when none is selected.

use super::*;

/// Radius the dialog starts with, in millimetres.
const DEFAULT_RADIUS_MM: f64 = 1.0;
/// Smallest radius the dialog offers, in millimetres.
const MIN_RADIUS_MM: f64 = 0.05;
/// Largest radius the dialog offers, in millimetres.
const MAX_RADIUS_MM: f64 = 50.0;

/// What the Round corners dialog remembers while it is open.
pub(super) struct RoundState {
    /// Whether the window is showing.
    pub open: bool,
    /// The radius to round with, in millimetres.
    pub radius: f64,
    /// Tracks the dialog was opened for from the inspector; `None` means the
    /// current selection, or every track when nothing is selected.
    pub only: Option<Vec<Id>>,
}

impl Default for RoundState {
    fn default() -> Self {
        Self {
            open: false,
            radius: DEFAULT_RADIUS_MM,
            only: None,
        }
    }
}

impl App {
    /// The Round corners entry of the Edit menu.
    ///
    /// # Rules
    /// - Shown on the Layout tab only.
    /// - Opens the dialog; nothing changes until Round corners is pressed there.
    pub(super) fn round_corners_menu_item(&mut self, ui: &mut egui::Ui) {
        if self.tab != Tab::Pcb {
            return;
        }
        if ui
            .button(t!("menu.edit.round_corners"))
            .on_hover_text(t!("menu.edit.round_corners.tip"))
            .clicked()
        {
            self.edit.round.only = None;
            self.edit.round.open = true;
            ui.close();
        }
    }

    /// The Round corner button of the track inspector, beside Cut corner.
    ///
    /// # Rules
    /// - Enabled only when the wire meets another wire at an end.
    /// - Opens the dialog for just those two wires.
    pub(super) fn round_corner_button(&mut self, ui: &mut egui::Ui, id: Id, mate: Option<Id>) {
        let ready = mate.is_some();
        if ui
            .add_enabled(ready, egui::Button::new(t!("board.inspector.round_corner")))
            .on_hover_text(t!("board.inspector.round_corner.tip"))
            .on_disabled_hover_text(t!("board.inspector.round_corner.none"))
            .clicked()
            && let Some(other) = mate
        {
            self.edit.round.only = Some(vec![id, other]);
            self.edit.round.open = true;
        }
    }

    /// Ids of the selected objects that are tracks.
    fn selected_track_ids(&self) -> Vec<Id> {
        let tracks = &self.project.board.tracks;
        self.selection()
            .into_iter()
            .filter(|id| tracks.iter().any(|t| t.id == *id))
            .collect()
    }

    /// Round the corners from the dialog values as one undo step.
    ///
    /// # Rules
    /// - The engine checks the radius; its refusal shows as the message.
    /// - Corners that would break clearance are skipped and counted.
    ///
    /// # Dependencies
    /// - crates/engine/src/track_round.rs (`track.round_corners`)
    pub(super) fn run_round_corners(&mut self) {
        let ids = match &self.edit.round.only {
            Some(only) => only.clone(),
            None => self.selected_track_ids(),
        };
        let mut params = json!({"radius": self.edit.round.radius});
        if !ids.is_empty() {
            params["ids"] = json!(ids);
        }
        let result = self.run_and_keep("track.round_corners", params);
        if result.is_null() {
            return;
        }
        let rounded = result.get("rounded").and_then(Value::as_u64).unwrap_or(0);
        let skipped = result
            .get("skipped")
            .and_then(Value::as_array)
            .map_or(0, Vec::len);
        self.message = t!(
            "status.round_corners.done",
            rounded = rounded,
            skipped = skipped
        );
        self.edit.round.open = false;
        self.refresh();
    }

    /// The Round corners window.
    pub(super) fn round_corners_window(&mut self, ctx: &egui::Context) {
        if !self.edit.round.open {
            return;
        }
        let scope = match (&self.edit.round.only, self.selected_track_ids().len()) {
            (Some(_), _) => t!("dialogs.round_corners.scope_pair"),
            (None, 0) => t!("dialogs.round_corners.scope_all"),
            (None, _) => t!("dialogs.round_corners.scope_selected"),
        };
        let mut open = true;
        let mut go = false;
        let mut cancel = false;
        egui::Window::new(t!("dialogs.round_corners.title"))
            .id(egui::Id::new("round-corners-dialog"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(scope);
                ui.horizontal(|ui| {
                    ui.label(t!("dialogs.round_corners.radius"));
                    ui.add(
                        egui::DragValue::new(&mut self.edit.round.radius)
                            .range(MIN_RADIUS_MM..=MAX_RADIUS_MM)
                            .speed(0.05)
                            .suffix(format!(" {}", t!("unit.mm"))),
                    )
                    .named(&t!("dialogs.round_corners.radius"))
                    .on_hover_text(t!("dialogs.round_corners.radius.tip"));
                });
                ui.horizontal(|ui| {
                    if ui
                        .button(t!("dialogs.round_corners.apply"))
                        .on_hover_text(t!("dialogs.round_corners.apply.tip"))
                        .clicked()
                    {
                        go = true;
                    }
                    if ui
                        .button(t!("dialogs.round_corners.cancel"))
                        .on_hover_text(t!("dialogs.round_corners.cancel.tip"))
                        .clicked()
                    {
                        cancel = true;
                    }
                });
            });
        if go {
            self.run_round_corners();
        }
        if !open || cancel {
            self.edit.round.open = false;
        }
    }
}
