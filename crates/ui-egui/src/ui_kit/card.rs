//! Card: a panel-coloured box with a 44 high header and a padded body.
//!
//! Context: design system 5.16. Cards group the fields of a page or dialog; the
//! page behind them is `ground`, so the card's `panel` fill and 1 px `line` border
//! lift it.
//! Local rules: header is Title plus an optional muted caption and right-aligned
//! actions; body padding is 8 by 16.
//!
//! # Dependencies
//! - crates/ui-egui/src/theme.rs
//! - crates/ui-egui/src/fonts.rs
//!
//! # Do not
//! - Nest a card inside a card.
use crate::fonts;
use crate::theme::{self, RADIUS_CARD};
use egui::{CornerRadius, Frame, InnerResponse, Layout, Margin, Sense, Stroke, Ui, Vec2};

/// Height of a card header in points.
pub const CARD_HEADER_HEIGHT: f32 = 44.0;

/// Draw a card with `title`, an optional `caption`, header `actions` and a `body`.
///
/// Why: one container for grouped settings, so spacing and borders never drift.
///
/// # Rules
/// - The card fills the available width.
/// - The header has a 1 px `line` rule under it; the body is inset 8 by 16.
///
/// # Dependencies
/// - crates/ui-egui/src/theme.rs
///
/// # Do not
/// - Pass an empty `title`; a card without a name is a plain frame.
pub fn card<R>(
    ui: &mut Ui,
    title: &str,
    caption: Option<&str>,
    actions: impl FnOnce(&mut Ui),
    body: impl FnOnce(&mut Ui) -> R,
) -> InnerResponse<R> {
    let t = theme::tokens(ui.ctx());
    Frame::new()
        .fill(t.panel)
        .stroke(Stroke::new(theme::BORDER, t.line))
        .corner_radius(CornerRadius::same(RADIUS_CARD))
        .inner_margin(Margin::ZERO)
        .show(ui, |ui| {
            let width = ui.available_width();
            ui.spacing_mut().item_spacing.y = 0.0;
            let (rect, response) =
                ui.allocate_exact_size(Vec2::new(width, CARD_HEADER_HEIGHT), Sense::hover());
            response
                .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, title));
            let inner = rect.shrink2(Vec2::new(16.0, 0.0));
            let mut left = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(inner)
                    .layout(Layout::left_to_right(egui::Align::Center)),
            );
            left.label(fonts::title(title).color(t.text));
            if let Some(caption) = caption {
                left.add_space(8.0);
                left.label(fonts::caption(caption).color(t.muted));
            }
            let mut right = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(inner)
                    .layout(Layout::right_to_left(egui::Align::Center)),
            );
            actions(&mut right);
            ui.painter().hline(
                rect.x_range(),
                rect.max.y - 0.5,
                Stroke::new(theme::BORDER, t.line),
            );
            Frame::new()
                .inner_margin(Margin::symmetric(16, 8))
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = theme::SPACE[1];
                    body(ui)
                })
                .inner
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui_kit::testing::{context, frame};

    /// The header is 44 high, so the card is at least that tall.
    #[test]
    fn card_is_at_least_header_tall_and_fills_the_width() {
        let ctx = context(true);
        let mut size = Vec2::ZERO;
        frame(&ctx, vec![], |ui| {
            size = card(
                ui,
                "Board outline",
                Some("60 by 40 mm"),
                |_| {},
                |ui| {
                    ui.label("Body");
                },
            )
            .response
            .rect
            .size();
        });
        assert!(size.y >= CARD_HEADER_HEIGHT + 16.0, "{size:?}");
        assert!(size.x > 700.0, "{size:?}");
    }
}
