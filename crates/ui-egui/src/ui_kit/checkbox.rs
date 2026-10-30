//! Checkbox: a 16 px box that sits at the right of a form row.
//!
//! Context: the Properties panel of the design (Include in parts list, Lock position).
//! The label lives in the form row; the box carries the accessible name.
//! Local rules: unchecked is a raised box with an edge border; checked is copper with
//! a dark tick. The click target is 28 by 28 so it meets the target-size rule.
//!
//! # Dependencies
//! - crates/ui-egui/src/theme.rs
//! - crates/ui-egui/src/ui_kit/icons.rs
//! - crates/ui-egui/src/ui_kit/focus.rs
//!
//! # Do not
//! - Show a state by colour alone; the tick is the checked shape.
use super::focus;
use super::icons::{self, ICON_SIZE, icon_rect};
use crate::theme::{self, CONTROL_HEIGHT};
use egui::{
    CornerRadius, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2, WidgetInfo, WidgetType,
};

/// The visible box size in points.
pub const BOX_SIZE: f32 = 16.0;

/// A checkbox bound to `checked`; the response reports `changed()` when toggled.
///
/// Why: yes or no properties read the same everywhere.
///
/// # Rules
/// - `name` is the accessible name (the row's label), `tip` the tooltip.
/// - Space or Enter toggles it while focused.
///
/// # Dependencies
/// - crates/ui-egui/src/ui_kit/icons.rs (`CHECK`)
///
/// # Do not
/// - Pass an empty `name`.
pub fn checkbox(ui: &mut Ui, checked: &mut bool, name: &str, tip: &str) -> Response {
    let t = theme::tokens(ui.ctx());
    let (target, mut response) =
        ui.allocate_exact_size(Vec2::splat(CONTROL_HEIGHT), Sense::click());
    if response.clicked() {
        *checked = !*checked;
        response.mark_changed();
    }
    let value = *checked;
    response.widget_info(|| WidgetInfo::selected(WidgetType::Checkbox, true, value, name));
    let rect = Rect::from_center_size(target.center(), Vec2::splat(BOX_SIZE));
    let radius = CornerRadius::same(theme::RADIUS_CONTROL);
    let painter = ui.painter();
    if value {
        painter.rect_filled(rect, radius, t.copper);
        (icons::CHECK.0)(
            painter,
            icon_rect(Rect::from_center_size(
                rect.center(),
                Vec2::splat(ICON_SIZE),
            )),
            t.onacc,
        );
    } else {
        let border = if response.hovered() { t.text } else { t.edge };
        painter.rect_filled(rect, radius, t.raised);
        painter.rect_stroke(
            rect,
            radius,
            Stroke::new(theme::BORDER, border),
            StrokeKind::Inside,
        );
    }
    focus::focus_ring(ui, &response, theme::RADIUS_CONTROL);
    response.on_hover_text(tip)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui_kit::testing::{click, context, frame};

    /// A click toggles the value and reports a change.
    #[test]
    fn clicking_toggles() {
        let ctx = context(true);
        let mut on = false;
        let mut rect = Rect::NOTHING;
        frame(&ctx, vec![], |ui| {
            rect = checkbox(ui, &mut on, "Lock position", "Stop the part moving").rect;
        });
        frame(&ctx, click(rect.center()), |ui| {
            checkbox(ui, &mut on, "Lock position", "Stop the part moving");
        });
        assert!(on);
    }
}
