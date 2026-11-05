//! Stat card: a small panel with a caption, one large number and a detail line.
//!
//! Context: the Files page mockup shows four of them in a row (Parts, Nets, Board,
//! Checks). Design system 5.16 sets the panel look: `panel` fill, 1 px `line`, radius 6,
//! padding 16.
//! Local rules: the caption is a section caption, the value is Display 22, the detail is
//! Caption in `muted`; the card is not a control and has no focus ring.
//!
//! # Dependencies
//! - crates/ui-egui/src/theme.rs
//! - crates/ui-egui/src/fonts.rs
//!
//! # Do not
//! - Put a button in a stat card; make it a card with actions.
use crate::fonts;
use crate::theme::{self, RADIUS_CARD};
use egui::{CornerRadius, Frame, Margin, Response, Sense, Stroke, Ui, WidgetInfo, WidgetType};

/// Height of a stat card in points: 16 padding, caption, value and detail lines.
pub const STAT_CARD_HEIGHT: f32 = 92.0;

/// Draw a stat card `width` wide and return its response.
///
/// Why: a page that answers "what is in this project" shows the numbers first.
///
/// # Rules
/// - `label` is one word, `value` is short (a count or a size), `detail` is one line.
/// - `tip` is the tooltip; the accessible name reads label, value and detail.
///
/// # Dependencies
/// - crates/ui-egui/src/theme.rs
///
/// # Do not
/// - Pass an empty `label` or `tip`.
pub fn stat_card(
    ui: &mut Ui,
    width: f32,
    label: &str,
    value: &str,
    detail: &str,
    tip: &str,
) -> Response {
    let t = theme::tokens(ui.ctx());
    let outer = Frame::new()
        .fill(t.panel)
        .stroke(Stroke::new(theme::BORDER, t.line))
        .corner_radius(CornerRadius::same(RADIUS_CARD))
        .inner_margin(Margin::same(theme::SPACE[3] as i8))
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.set_width(width - 2.0 * theme::SPACE[3] - 2.0 * theme::BORDER);
                ui.set_min_height(STAT_CARD_HEIGHT - 2.0 * theme::SPACE[3] - 2.0 * theme::BORDER);
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.add(
                    egui::Label::new(fonts::section_caption(label).color(t.muted))
                        .selectable(false)
                        .truncate(),
                );
                ui.add(
                    egui::Label::new(fonts::display(value).color(t.text))
                        .selectable(false)
                        .truncate(),
                );
                ui.add(
                    egui::Label::new(fonts::caption(detail).color(t.muted))
                        .selectable(false)
                        .truncate(),
                );
            });
        });
    let response = ui.interact(outer.response.rect, outer.response.id, Sense::hover());
    let name = format!("{label} {value} {detail}");
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Label, true, &name));
    response.on_hover_text(tip)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui_kit::testing::{context, frame};

    /// A stat card is as wide as asked and at least the design height.
    #[test]
    fn stat_card_takes_the_width_it_is_given() {
        let ctx = context(true);
        let mut size = egui::Vec2::ZERO;
        frame(&ctx, vec![], |ui| {
            size = stat_card(ui, 240.0, "Parts", "3", "J1, R1, D1", "How many parts")
                .rect
                .size();
        });
        assert_eq!(size.x, 240.0);
        assert!(size.y >= STAT_CARD_HEIGHT);
    }
}
