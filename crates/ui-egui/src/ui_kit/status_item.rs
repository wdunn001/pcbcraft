//! Status bar item: icon and text in a 24 high slot, a button when it opens something.
//!
//! Context: design system 5.13. Muted 12 px content with 10 px side padding; hover
//! raises the fill and the text to `text`.
//! Local rules: words use Caption (11); readouts such as `X 44.00 Y 20.00 mm` use
//! Mono (12) through `mono = true`. Only the five type sizes exist, so the spec's
//! "12 px" words are drawn at 11.
//!
//! # Dependencies
//! - crates/ui-egui/src/theme.rs
//! - crates/ui-egui/src/ui_kit/icons.rs
//!
//! # Do not
//! - Put anything but state in the status bar.
use super::focus;
use super::icons::{ICON_SIZE, Icon};
use crate::fonts::Role;
use crate::theme::{self, STATUS_HEIGHT};
use egui::CornerRadius;
use egui::{Rect, Response, Sense, Ui, Vec2, WidgetInfo, WidgetType, pos2};

/// One status bar item; the response senses clicks so it can open something.
///
/// Why: every status slot (Ready, error counts, readouts) shares one look.
///
/// # Rules
/// - Always has a tooltip; `text` is also the accessible name.
///
/// # Dependencies
/// - crates/ui-egui/src/ui_kit/focus.rs
///
/// # Do not
/// - Make it interactive without a tooltip that says what it opens.
pub fn status_item(ui: &mut Ui, icon: Option<Icon>, text: &str, tip: &str, mono: bool) -> Response {
    let t = theme::tokens(ui.ctx());
    let font = if mono {
        Role::Mono.font()
    } else {
        Role::Caption.font()
    };
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_owned(), font, egui::Color32::PLACEHOLDER);
    let icon_w = if icon.is_some() { ICON_SIZE + 4.0 } else { 0.0 };
    let size = Vec2::new(galley.size().x + icon_w + 20.0, STATUS_HEIGHT);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, text));
    let hovered = response.hovered();
    if hovered {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(theme::RADIUS_CONTROL), t.raised);
    }
    let ink = if hovered { t.text } else { t.muted };
    let mut x = rect.min.x + 10.0;
    if let Some(icon) = icon {
        let r = Rect::from_min_size(
            pos2(x, rect.center().y - ICON_SIZE / 2.0),
            Vec2::splat(ICON_SIZE),
        );
        (icon.0)(ui.painter(), r, ink);
        x += ICON_SIZE + 4.0;
    }
    let galley = ui.painter().layout_no_wrap(
        text.to_owned(),
        if mono {
            Role::Mono.font()
        } else {
            Role::Caption.font()
        },
        ink,
    );
    ui.painter().galley(
        pos2(x, rect.center().y - galley.size().y / 2.0),
        galley,
        ink,
    );
    focus::focus_ring(ui, &response, theme::RADIUS_CONTROL);
    response.on_hover_text(tip)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui_kit::testing::{context, frame};

    /// A status item is 24 high.
    #[test]
    fn status_item_is_24_high() {
        let ctx = context(true);
        let mut h = 0.0;
        frame(&ctx, vec![], |ui| {
            h = status_item(ui, None, "Ready", "The project is saved", false)
                .rect
                .height();
        });
        assert_eq!(h, 24.0);
    }
}
