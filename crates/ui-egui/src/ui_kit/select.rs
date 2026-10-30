//! Select (dropdown): the number field's frame with a label and a chevron.
//!
//! Context: design system 5.11. Built on `egui::ComboBox` so the popup, keyboard
//! and accessibility come from egui; the button takes its fill, border and radius
//! from the theme and gets our chevron.
//! Local rules: option labels are plain words; the box is 28 high.
//!
//! # Dependencies
//! - crates/ui-egui/src/theme.rs
//! - crates/ui-egui/src/ui_kit/icons.rs
//!
//! # Do not
//! - Use for two or three options that fit side by side; use `segmented`.
use super::icons::{self, icon_rect};
use crate::fonts;
use crate::theme;
use egui::{ComboBox, Response, Ui, WidgetInfo, WidgetType};

/// Tallest the drop-down list gets before it scrolls.
const POPUP_HEIGHT: f32 = 320.0;

/// A dropdown over `options`; `selected` is the index in use and `changed()` reports a pick.
///
/// Why: a long list of exclusive choices (footprint, layer, net class) in one 28 high box.
///
/// # Rules
/// - `name` is the accessible name, `tip` the tooltip, `width` the box width.
/// - `selected` is clamped into range before drawing.
///
/// # Dependencies
/// - crates/ui-egui/src/ui_kit/icons.rs (`CHEVRON_DOWN`)
///
/// # Do not
/// - Pass an empty option list.
pub fn select(
    ui: &mut Ui,
    salt: impl std::hash::Hash + std::fmt::Debug,
    selected: &mut usize,
    options: &[&str],
    name: &str,
    tip: &str,
    width: f32,
) -> Response {
    let t = theme::tokens(ui.ctx());
    if options.is_empty() {
        return ui.allocate_response(egui::Vec2::ZERO, egui::Sense::hover());
    }
    *selected = (*selected).min(options.len() - 1);
    let before = *selected;
    let shown = options.get(*selected).copied().unwrap_or("");
    let out = ComboBox::from_id_salt(salt)
        .width(width)
        // The list drops down at most this tall; the box itself is as high as a control.
        .height(POPUP_HEIGHT)
        .selected_text(fonts::text(fonts::Role::Body, shown).color(t.text))
        .icon(move |ui, rect, visuals, _open| {
            let c = visuals.fg_stroke.color;
            (icons::CHEVRON_DOWN.0)(ui.painter(), icon_rect(rect), c);
        })
        .show_ui(ui, |ui| {
            for (i, option) in options.iter().enumerate() {
                ui.selectable_value(selected, i, fonts::text(fonts::Role::Body, *option));
            }
        });
    let mut response = out.response;
    response.widget_info(|| WidgetInfo::labeled(WidgetType::ComboBox, true, name));
    if *selected != before {
        response.mark_changed();
    }
    response.on_hover_text(tip)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui_kit::testing::{context, frame};

    /// The box is a combo box with the given name and a bounded height.
    #[test]
    fn select_is_named_and_28_high() {
        let ctx = context(true);
        let mut sel = 1usize;
        let mut h = 0.0;
        let out = frame(&ctx, vec![], |ui| {
            h = select(
                ui,
                "fp",
                &mut sel,
                &["0805", "1206", "0603"],
                "Footprint",
                "Package of the part",
                160.0,
            )
            .rect
            .height();
        });
        assert!((28.0..=30.0).contains(&h), "{h}");
        let named = out
            .platform_output
            .accesskit_update
            .map(|u| {
                u.nodes.iter().any(|(_, n)| {
                    n.role() == egui::accesskit::Role::ComboBox && n.label() == Some("Footprint")
                })
            })
            .unwrap_or(false);
        assert!(named, "combo box has no accessible name");
    }
}
