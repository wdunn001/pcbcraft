//! A loading view with spinner, progress bar and status line.
//!
//! Shown while a project is opening. The view takes the current step index and
//! total step count, and displays the appropriate status line from the catalog.

use egui::{Align, Layout, RichText};
use pcbcraft_text::t;

/// The four loading steps.
pub(super) const STEP_COUNT: usize = 4;

/// Status line index: reading the file.
pub(super) const STEP_READING: usize = 0;
/// Status line index: building the design.
pub(super) const STEP_BUILDING: usize = 1;
/// Status line index: preparing the board.
pub(super) const STEP_PREPARING: usize = 2;
/// Status line index: drawing.
pub(super) const STEP_DRAWING: usize = 3;

/// The four status line keys, in step order.
fn status_key(step: usize) -> &'static str {
    match step {
        STEP_READING => "files.open.loading.reading",
        STEP_BUILDING => "files.open.loading.building",
        STEP_PREPARING => "files.open.loading.preparing",
        STEP_DRAWING => "files.open.loading.drawing",
        _ => "files.open.loading.reading",
    }
}

/// Show the loading view.
///
/// `step` is the current step index (0-3). The progress bar fills as steps complete.
pub(super) fn loading_view(ui: &mut egui::Ui, step: usize) {
    ui.set_layout(Layout::top_down(Align::Center));

    // Spinner
    ui.add_space(24.0);
    ui.add(egui::Spinner::new().size(32.0));
    ui.add_space(16.0);

    // Title
    ui.label(RichText::new("Opening project").size(18.0).strong());
    ui.add_space(8.0);

    // Status line (screen-reader readable)
    let status = t!(status_key(step));
    ui.label(status.clone());
    ui.add_space(16.0);

    // Progress bar
    let fraction = (step as f32) / (STEP_COUNT as f32);
    let bar = egui::ProgressBar::new(fraction).show_limits(false);
    ui.add(bar);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_keys_are_correct() {
        assert_eq!(status_key(STEP_READING), "files.open.loading.reading");
        assert_eq!(status_key(STEP_BUILDING), "files.open.loading.building");
        assert_eq!(status_key(STEP_PREPARING), "files.open.loading.preparing");
        assert_eq!(status_key(STEP_DRAWING), "files.open.loading.drawing");
    }

    #[test]
    fn step_count_is_four() {
        assert_eq!(STEP_COUNT, 4);
    }

    #[test]
    fn progress_fractions_increase() {
        let f0 = (STEP_READING as f32) / (STEP_COUNT as f32);
        let f1 = (STEP_BUILDING as f32) / (STEP_COUNT as f32);
        let f2 = (STEP_PREPARING as f32) / (STEP_COUNT as f32);
        let f3 = (STEP_DRAWING as f32) / (STEP_COUNT as f32);
        assert!(f0 < f1);
        assert!(f1 < f2);
        assert!(f2 < f3);
    }
}
