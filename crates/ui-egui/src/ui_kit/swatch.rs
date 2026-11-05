//! Colour swatch row: a few round colour choices, one of them selected.
//!
//! Context: the Accent colour row of Preferences in the approved mockups. Each swatch is
//! a 28 point round target with a 20 point disc. The selected one gets a 2 px ring in
//! `text`, 2 px outside the disc.
//! Local rules: every swatch has a colour name as its accessible name and tooltip, so the
//! choice never rests on colour alone.
//!
//! # Dependencies
//! - crates/ui-egui/src/theme.rs
//! - crates/ui-egui/src/ui_kit/focus.rs
//!
//! # Do not
//! - Use for more than eight colours or for a free colour picker.
use super::focus;
use crate::theme;
use egui::{Color32, Rect, Sense, Stroke, Ui, Vec2, WidgetInfo, WidgetType};

/// Disc diameter in points.
pub const DISC: f32 = 20.0;

/// One choice: a colour, its plain name and what picking it does.
#[derive(Clone, Copy, Debug)]
pub struct Swatch<'a> {
    /// The colour drawn in the disc.
    pub color: Color32,
    /// Plain colour name, the accessible name.
    pub name: &'a str,
    /// Tooltip: what choosing it does.
    pub tip: &'a str,
}

/// Draw the swatches left to right; `selected` is the index in use, or `None`.
///
/// Why: a colour choice among a handful of values reads faster as colours than as a list.
///
/// # Rules
/// - Returns the index pressed this frame.
/// - The selected swatch is a radio button in the accessibility tree.
///
/// # Dependencies
/// - crates/ui-egui/src/ui_kit/focus.rs
///
/// # Do not
/// - Pass an empty slice.
pub fn swatches(ui: &mut Ui, items: &[Swatch<'_>], selected: Option<usize>) -> Option<usize> {
    let t = theme::tokens(ui.ctx());
    let mut pressed = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme::SPACE[0];
        for (i, item) in items.iter().enumerate() {
            let (target, response) =
                ui.allocate_exact_size(Vec2::splat(theme::CONTROL_HEIGHT), Sense::click());
            let on = selected == Some(i);
            response
                .widget_info(|| WidgetInfo::selected(WidgetType::RadioButton, true, on, item.name));
            let disc = Rect::from_center_size(target.center(), Vec2::splat(DISC));
            let painter = ui.painter();
            painter.circle_filled(disc.center(), DISC / 2.0, item.color);
            if on {
                painter.circle_stroke(
                    disc.center(),
                    DISC / 2.0 + theme::FOCUS_STROKE,
                    Stroke::new(theme::FOCUS_STROKE, t.text),
                );
            } else if response.hovered() {
                painter.circle_stroke(
                    disc.center(),
                    DISC / 2.0 + theme::BORDER,
                    Stroke::new(theme::BORDER, t.edge),
                );
            }
            focus::focus_ring(ui, &response, theme::RADIUS_CARD);
            if response.clicked() {
                pressed = Some(i);
            }
            response.on_hover_text(item.tip);
        }
    });
    pressed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui_kit::testing::{click, context, frame};

    /// A click on the second swatch is reported as index 1.
    #[test]
    fn clicking_reports_the_index() {
        let ctx = context(true);
        let items = [
            Swatch {
                color: Color32::RED,
                name: "Red",
                tip: "Use red",
            },
            Swatch {
                color: Color32::BLUE,
                name: "Blue",
                tip: "Use blue",
            },
        ];
        let mut at = egui::Pos2::ZERO;
        frame(&ctx, vec![], |ui| {
            let start = ui.cursor().min;
            at = start + Vec2::new(theme::CONTROL_HEIGHT * 1.5 + theme::SPACE[0], 14.0);
            swatches(ui, &items, Some(0));
        });
        let mut got = None;
        frame(&ctx, click(at), |ui| {
            got = swatches(ui, &items, Some(0));
        });
        assert_eq!(got, Some(1));
    }
}
