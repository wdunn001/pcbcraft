//! The focus ring every kit widget draws when it has keyboard focus.
//!
//! Context: design system section 5, Focus: a 2 px ring in the `text` colour,
//! 2 px outside the control. Selection stroke is not used for focus.
//! Local rules: custom widgets call `focus_ring` after painting; callers never
//! draw their own ring.
//!
//! # Dependencies
//! - crates/ui-egui/src/theme.rs
//!
//! # Do not
//! - Draw the ring inside the widget rect; it must stay clear of the border.
use crate::theme::{self, FOCUS_STROKE, Tokens};
use egui::{CornerRadius, Rect, Response, Stroke, StrokeKind};

/// Paint the ring around `rect` with the control's radius.
///
/// Why: one place decides how focus looks, so the gallery and the contrast gate
/// can check it.
pub fn paint_ring(painter: &egui::Painter, rect: Rect, radius: u8, tokens: &Tokens) {
    painter.rect_stroke(
        rect.expand(FOCUS_STROKE),
        CornerRadius::same(radius.saturating_add(2)),
        Stroke::new(FOCUS_STROKE, tokens.text),
        StrokeKind::Outside,
    );
}

/// Draw the ring when `response` has keyboard focus.
///
/// # Rules
/// - Call once per widget, after the widget is painted.
///
/// # Do not
/// - Call for widgets that are disabled; they cannot take focus.
pub fn focus_ring(ui: &egui::Ui, response: &Response, radius: u8) {
    if response.has_focus() {
        paint_ring(
            ui.painter(),
            response.rect,
            radius,
            &theme::tokens(ui.ctx()),
        );
    }
}
