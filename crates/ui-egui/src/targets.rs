//! Controls that keep to the 24 by 24 pixel pointer-target minimum (WCAG 2.2, 2.5.8).
//!
//! Context: egui's `small_button` and a bare `TextEdit` ignore `interact_size`
//! and come out 15 to 19 pixels tall. The app uses these two wrappers instead,
//! so every such control is at least `App::MIN_TARGET` on both sides.
//! Local rules: keep the look; only the hit size grows. The check that proves it
//! is `e2e/shell.rs` `a_newcomer_never_meets_a_control_too_small_to_hit`.
use egui::{Response, TextEdit, Ui, Vec2, WidgetText};

/// The smallest side of a pointer target, in logical pixels.
pub(crate) const MIN_TARGET: f32 = 24.0;

/// Buttons that are small in look but never small to hit.
///
/// # Implementations
/// - `crates/ui-egui/src/targets.rs` `impl TapButton for egui::Ui`.
pub(crate) trait TapButton {
    /// A compact text button at least 24 by 24 pixels.
    ///
    /// Why: replaces `small_button` everywhere so no row of tools is too fine
    /// to hit.
    ///
    /// # Rules
    /// - Same return value and click behaviour as `Ui::small_button`.
    ///
    /// # Do not
    /// - Add a hover text here; callers chain `.on_hover_text` as before.
    fn tap_button(&mut self, text: impl Into<WidgetText>) -> Response;
}

impl TapButton for Ui {
    // Contract: this file, `TapButton`.
    fn tap_button(&mut self, text: impl Into<WidgetText>) -> Response {
        self.add(egui::Button::new(text).min_size(Vec2::splat(MIN_TARGET)))
    }
}

/// A single-line text field at least 24 pixels tall.
///
/// Why: egui sizes a bare field from its text, about 19 pixels.
///
/// # Rules
/// - Width is left to the caller's `desired_width`.
///
/// # Do not
/// - Use it for a field inside a table cell that must stay one text line high.
pub(crate) fn text_field(text: &mut dyn egui::TextBuffer) -> TextEdit<'_> {
    TextEdit::singleline(text).min_size(Vec2::new(0.0, MIN_TARGET))
}
