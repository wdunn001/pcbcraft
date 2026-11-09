//! Panel header: the 36 high strip above a list or tree inside an aside or card.
//!
//! Context: design system 5.6. Title 16 semibold on the left, optional actions
//! right aligned, a 1 px `line` rule along the bottom.
//! Local rules: actions are icon or ghost buttons from `./button.rs`; the header
//! itself is not interactive.
//!
//! # Dependencies
//! - crates/ui-egui/src/theme.rs
//! - crates/ui-egui/src/ui_kit/button.rs
//!
//! # Do not
//! - Put a primary button in a panel header; the region's primary lives in its footer.
use crate::fonts;
use crate::theme::{self, SHEET_TAB_HEIGHT};
use egui::{Layout, Sense, Stroke, Ui, Vec2};

/// A panel header with a title and a right-aligned action area.
///
/// Why: lists and trees get one consistent top strip.
///
/// # Rules
/// - Height is 36, side padding 12; `actions` run inside a right-to-left layout.
///
/// # Dependencies
/// - crates/ui-egui/src/ui_kit/button.rs (action buttons)
///
/// # Do not
/// - Draw a second bottom border below it.
pub fn panel_header(ui: &mut Ui, title: &str, actions: impl FnOnce(&mut Ui)) -> egui::Response {
    let t = theme::tokens(ui.ctx());
    let width = ui.available_width();
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(width, SHEET_TAB_HEIGHT), Sense::hover());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, title));
    ui.painter().hline(
        rect.x_range(),
        rect.max.y - 0.5,
        Stroke::new(theme::BORDER, t.line),
    );
    let inner = rect.shrink2(Vec2::new(12.0, 0.0));
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(inner)
            .layout(Layout::left_to_right(egui::Align::Center)),
    );
    child.label(fonts::title(title).color(t.text));
    let mut right = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(inner)
            .layout(Layout::right_to_left(egui::Align::Center)),
    );
    actions(&mut right);
    response
}
