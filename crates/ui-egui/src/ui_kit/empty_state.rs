//! Empty state: what is missing, why, and one button to fix it.
//!
//! Context: design system 5.12 and rule 6. A centred column: 40 px disc on `raised`
//! with a muted icon, a strong one-line statement, one muted sentence, one
//! secondary button.
//! Local rules: never a bare sentence; the button is mandatory.
//!
//! # Dependencies
//! - crates/ui-egui/src/ui_kit/button.rs
//! - crates/ui-egui/src/ui_kit/icons.rs
//!
//! # Do not
//! - Offer more than one button.
use super::button::{self, Size};
use super::icons::{ICON_SIZE, Icon, icon_rect};
use crate::fonts;
use crate::theme;
use egui::{Align, Layout, Response, Sense, Ui, Vec2};

/// Diameter of the icon disc in points.
pub const DISC: f32 = 40.0;

/// Draw an empty state and return the button's response.
///
/// Why: a blank panel tells a first-time user nothing; this says what is missing
/// and what to press.
///
/// # Rules
/// - `title` is one line ("No fabrication files yet"); `message` is one sentence.
/// - `button` is the label, `tip` its tooltip; the caller acts on `clicked()`.
///
/// # Dependencies
/// - crates/ui-egui/src/ui_kit/button.rs (`secondary_button`)
///
/// # Do not
/// - Pass an empty `button` label.
pub fn empty_state(
    ui: &mut Ui,
    icon: Icon,
    title: &str,
    message: &str,
    button: &str,
    tip: &str,
) -> Response {
    let t = theme::tokens(ui.ctx());
    ui.with_layout(Layout::top_down(Align::Center), |ui| {
        let (disc, _) = ui.allocate_exact_size(Vec2::splat(DISC), Sense::hover());
        ui.painter()
            .circle_filled(disc.center(), DISC / 2.0, t.raised);
        let r = icon_rect(egui::Rect::from_center_size(
            disc.center(),
            Vec2::splat(ICON_SIZE),
        ));
        (icon.0)(ui.painter(), r, t.muted);
        ui.add_space(theme::SPACE[1]);
        ui.label(fonts::strong(title).color(t.text));
        ui.label(fonts::text(fonts::Role::Body, message).color(t.muted));
        ui.add_space(theme::SPACE[1]);
        button::secondary_button(ui, None, button, tip, Size::Normal, true)
    })
    .inner
}
