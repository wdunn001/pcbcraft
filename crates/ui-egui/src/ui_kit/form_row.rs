//! Form row: a label column and a control column with a hairline divider.
//!
//! Context: design system 5.9. The label column is 200 wide on pages and dialogs and
//! 104 in an aside; the control column takes the rest. A row is 40 high (36 in an
//! aside), with an optional hint under the label and an error under the control.
//! Local rules: the unit goes inside the control, never in the label; the divider is
//! the `line` colour at 55 percent.
//!
//! # Dependencies
//! - crates/ui-egui/src/theme.rs
//! - crates/ui-egui/src/ui_kit/icons.rs
//! - crates/ui-egui/src/ui_kit/checkbox.rs
//!
//! # Do not
//! - Put two controls in one row.
use super::icons::{self, ICON_SIZE};
use crate::fonts;
use crate::theme;
use egui::{Layout, Response, Sense, Stroke, Ui, Vec2};

/// Where a form row lives, which sets its label column and height.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormWidth {
    /// A page or dialog: 200 label column, 40 high.
    Page,
    /// The right aside: 104 label column, 36 high.
    Aside,
}

impl FormWidth {
    /// Label column width in points.
    pub fn label_width(self) -> f32 {
        match self {
            FormWidth::Page => 200.0,
            FormWidth::Aside => 104.0,
        }
    }

    /// Minimum row height in points.
    pub fn row_height(self) -> f32 {
        match self {
            FormWidth::Page => 40.0,
            FormWidth::Aside => 36.0,
        }
    }
}

/// Draw a form row: `label`, an optional `hint`, the control from `control`, and an
/// optional `error` below it.
///
/// Why: all settings, rules and properties line up the same way.
///
/// # Rules
/// - The label is 13 `text`; the hint is Caption `muted`; the error is `red` with an icon.
/// - The returned value is whatever `control` returns.
///
/// # Dependencies
/// - crates/ui-egui/src/theme.rs
///
/// # Do not
/// - Add a second divider; the row draws its own.
pub fn form_row<R>(
    ui: &mut Ui,
    width: FormWidth,
    label: &str,
    hint: Option<&str>,
    error: Option<&str>,
    control: impl FnOnce(&mut Ui) -> R,
) -> R {
    let t = theme::tokens(ui.ctx());
    let total = ui.available_width();
    let start = ui.cursor().min;
    let h = width.row_height();
    let result = ui
        .horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.set_min_height(h);
            ui.allocate_ui_with_layout(
                Vec2::new(width.label_width(), h),
                Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.set_min_size(Vec2::new(width.label_width(), h));
                    if let Some(hint) = hint {
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 0.0;
                            ui.add_space(4.0);
                            ui.label(fonts::text(fonts::Role::Body, label).color(t.text));
                            ui.label(fonts::caption(hint).color(t.muted));
                        });
                    } else {
                        ui.label(fonts::text(fonts::Role::Body, label).color(t.text));
                    }
                },
            );
            ui.allocate_ui_with_layout(
                Vec2::new((total - width.label_width()).max(0.0), h),
                Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.set_min_size(Vec2::new((total - width.label_width()).max(0.0), h));
                    if error.is_none() {
                        return control(ui);
                    }
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        ui.add_space((h - theme::CONTROL_HEIGHT) / 2.0);
                        let r = control(ui);
                        if let Some(error) = error {
                            ui.horizontal(|ui| {
                                let (r, _) =
                                    ui.allocate_exact_size(Vec2::splat(ICON_SIZE), Sense::hover());
                                (icons::ERROR.0)(ui.painter(), r, t.red);
                                ui.add_space(4.0);
                                ui.label(fonts::caption(error).color(t.red));
                            });
                        }
                        r
                    })
                    .inner
                },
            )
            .inner
        })
        .inner;
    let bottom = ui.min_rect().max.y;
    ui.painter().hline(
        start.x..=start.x + total,
        bottom - 0.5,
        Stroke::new(theme::BORDER, t.line.gamma_multiply(0.55)),
    );
    result
}

/// A row whose label fills the width and whose checkbox sits at the right.
///
/// Why: the Properties panel's yes or no rows ("Include in parts list") read
/// better with the whole row for the label.
///
/// # Rules
/// - Same height and divider as `form_row`; `checked` is toggled in place.
///
/// # Dependencies
/// - crates/ui-egui/src/ui_kit/checkbox.rs
///
/// # Do not
/// - Add a second control; use `form_row` for that.
pub fn toggle_row(
    ui: &mut Ui,
    width: FormWidth,
    label: &str,
    tip: &str,
    checked: &mut bool,
) -> Response {
    let t = theme::tokens(ui.ctx());
    let total = ui.available_width();
    let start = ui.cursor().min;
    let h = width.row_height();
    let response = ui
        .allocate_ui_with_layout(
            Vec2::new(total, h),
            Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.set_min_size(Vec2::new(total, h));
                ui.label(fonts::text(fonts::Role::Body, label).color(t.text));
                ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                    super::checkbox::checkbox(ui, checked, label, tip)
                })
                .inner
            },
        )
        .inner;
    let bottom = ui.min_rect().max.y;
    ui.painter().hline(
        start.x..=start.x + total,
        bottom - 0.5,
        Stroke::new(theme::BORDER, t.line.gamma_multiply(0.55)),
    );
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui_kit::testing::{context, frame};

    /// The label column is 200 on pages and 104 in an aside, and rows are 40 or 36 high.
    #[test]
    fn label_columns_and_row_heights() {
        let ctx = context(true);
        let mut report = Vec::new();
        for (width, label_w, h) in [
            (FormWidth::Page, 200.0, 40.0),
            (FormWidth::Aside, 104.0, 36.0),
        ] {
            let mut control_x = 0.0;
            let mut y_span = 0.0;
            frame(&ctx, vec![], |ui| {
                let top = ui.cursor().min;
                form_row(ui, width, "Reference", None, None, |ui| {
                    control_x = ui.cursor().min.x - top.x;
                    ui.label("v");
                });
                y_span = ui.min_rect().max.y - top.y;
            });
            report.push((width, label_w, h, control_x, y_span));
        }
        for (width, label_w, h, control_x, y_span) in report {
            assert!(
                (control_x - label_w).abs() < 1.0,
                "{width:?}: x {control_x}"
            );
            assert!(y_span >= h && y_span < h + 4.0, "{width:?}: y {y_span}");
        }
    }

    /// A hint and an error make the row taller than the minimum.
    #[test]
    fn hint_and_error_grow_the_row() {
        let ctx = context(true);
        let mut plain = 0.0;
        let mut full = 0.0;
        frame(&ctx, vec![], |ui| {
            let top = ui.cursor().min.y;
            form_row(ui, FormWidth::Page, "Width", None, None, |ui| {
                ui.allocate_space(Vec2::new(100.0, 28.0));
            });
            plain = ui.min_rect().max.y - top;
            let top = ui.cursor().min.y;
            form_row(
                ui,
                FormWidth::Page,
                "Width",
                Some("Track width"),
                Some("Enter a value from 0.05 to 5"),
                |ui| {
                    ui.allocate_space(Vec2::new(100.0, 28.0));
                },
            );
            full = ui.min_rect().max.y - top;
        });
        assert!(full > plain, "{full} vs {plain}");
    }
}
