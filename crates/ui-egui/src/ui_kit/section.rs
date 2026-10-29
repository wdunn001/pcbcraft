//! Collapsible section: the GENERAL / POSITION / PINS headers of the Properties aside.
//!
//! Context: design system 5.6 and the Layout mockup. A 32 high header strip with a
//! chevron and an uppercase 11 pt caption, a 1 px `line` rule above, and a body that
//! opens and closes. Open state lives in egui memory under the section's id, so a
//! section keeps its state while the selection changes.
//! Local rules: the header is a single focusable target with a tooltip and an
//! accessible name; the body gets 12 pt side padding.
//!
//! # Dependencies
//! - crates/ui-egui/src/theme.rs
//! - crates/ui-egui/src/fonts.rs
//! - crates/ui-egui/src/ui_kit/icons.rs
//!
//! # Do not
//! - Nest a section inside another; asides use one level.
use super::icons::{self, ICON_SIZE};
use crate::fonts;
use crate::theme;
use egui::{Sense, Stroke, Ui, Vec2, WidgetInfo, WidgetType, pos2};

/// Height of the header strip.
pub const SECTION_HEADER_HEIGHT: f32 = 32.0;

/// A collapsible section; `body` runs only while it is open.
///
/// Why: long property lists group into a few named blocks the reader can fold.
///
/// # Rules
/// - `id_salt` plus the caller's ui id keys the open state; new sections start open.
/// - Returns the header response; the body's result is not needed by callers.
///
/// # Dependencies
/// - crates/ui-egui/src/ui_kit/icons.rs (`CHEVRON_DOWN`, `CHEVRON_RIGHT`)
///
/// # Do not
/// - Pass an empty `title` or `tip`.
pub fn section(
    ui: &mut Ui,
    id_salt: impl std::hash::Hash + std::fmt::Debug,
    title: &str,
    tip: &str,
    body: impl FnOnce(&mut Ui),
) -> egui::Response {
    let t = theme::tokens(ui.ctx());
    let id = ui.id().with(("kit-section", id_salt));
    let mut open = ui.data_mut(|d| *d.get_persisted_mut_or(id, true));
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), SECTION_HEADER_HEIGHT),
        Sense::click(),
    );
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, title));
    let response = response.on_hover_text(tip);
    if response.clicked() {
        open = !open;
        ui.data_mut(|d| d.insert_persisted(id, open));
    }
    let painter = ui.painter();
    painter.hline(
        rect.x_range(),
        rect.min.y + 0.5,
        Stroke::new(theme::BORDER, t.line),
    );
    if response.hovered() {
        painter.rect_filled(
            rect.shrink2(Vec2::new(0.0, 1.0)),
            0.0,
            t.raised.gamma_multiply(0.5),
        );
    }
    let chevron = if open {
        icons::CHEVRON_DOWN
    } else {
        icons::CHEVRON_RIGHT
    };
    let icon_rect = egui::Rect::from_center_size(
        pos2(rect.min.x + 12.0 + ICON_SIZE / 2.0, rect.center().y),
        Vec2::splat(ICON_SIZE),
    );
    (chevron.0)(painter, icon_rect, t.muted);
    let galley = painter.layout_no_wrap(title.to_uppercase(), fonts::Role::Caption.font(), t.muted);
    painter.galley(
        pos2(
            rect.min.x + 12.0 + ICON_SIZE + 8.0,
            rect.center().y - galley.size().y / 2.0,
        ),
        galley,
        t.muted,
    );
    if open {
        egui::Frame::NONE
            .inner_margin(egui::Margin::symmetric(12, 0))
            .show(ui, body);
        ui.add_space(theme::SPACE[0]);
    }
    response
}
