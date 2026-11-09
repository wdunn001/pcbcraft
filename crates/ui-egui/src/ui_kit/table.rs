//! Table: a caption head and fixed-height rows with columns that line up.
//!
//! Context: the Shortcuts, Connections and Library sources lists of Preferences. The head
//! is Caption 600 uppercase on a `line` rule. A row is 40 high with a 55 percent `line`
//! divider and a `raised` fill under the pointer.
//! Local rules: columns have fixed widths in points; the last column fills what is left and
//! its content is right aligned. A row is not a control; the controls inside it are.
//!
//! # Dependencies
//! - crates/ui-egui/src/theme.rs
//! - crates/ui-egui/src/fonts.rs
//!
//! # Do not
//! - Put selection on a row; use `rows::list_row` for a list the person picks from.
use crate::fonts;
use crate::theme;
use egui::{Layout, Sense, Stroke, Ui, Vec2};

/// Height of a table row in points.
pub const ROW_HEIGHT: f32 = 40.0;
/// Height of the head in points.
pub const HEAD_HEIGHT: f32 = 28.0;

/// One column: its caption and its width in points (0 for the last, filling column).
#[derive(Clone, Copy, Debug)]
pub struct Column<'a> {
    /// Head caption; shown in capitals. May be empty for an actions column.
    pub title: &'a str,
    /// Width in points; 0 means the rest of the row.
    pub width: f32,
}

/// Draw the head of a table.
///
/// Why: every list in a dialog names its columns the same way.
///
/// # Rules
/// - The head is 28 high with a `line` rule under it.
///
/// # Dependencies
/// - crates/ui-egui/src/fonts.rs (`section_caption`)
///
/// # Do not
/// - Pass a title that needs more than one line.
pub fn head(ui: &mut Ui, columns: &[Column<'_>]) {
    let t = theme::tokens(ui.ctx());
    let width = ui.available_width();
    ui.add_space(-ui.spacing().item_spacing.y);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, HEAD_HEIGHT), Sense::hover());
    let mut x = rect.min.x;
    for column in columns {
        let w = if column.width > 0.0 {
            column.width
        } else {
            rect.max.x - x
        };
        let cell = egui::Rect::from_min_size(egui::pos2(x, rect.min.y), Vec2::new(w, HEAD_HEIGHT));
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(cell)
                .layout(Layout::left_to_right(egui::Align::Center)),
        );
        child.label(fonts::section_caption(column.title).color(t.muted));
        x += w;
    }
    ui.painter().hline(
        rect.x_range(),
        rect.max.y - 0.5,
        Stroke::new(theme::BORDER, t.line),
    );
}

/// The cells of the row being drawn, handed to the row closure.
pub struct Cells<'u, 'c> {
    ui: &'u mut Ui,
    columns: &'c [Column<'c>],
    row: egui::Rect,
    next: usize,
    x: f32,
}

impl Cells<'_, '_> {
    /// Draw the next cell. The last column's content is right aligned.
    pub fn cell<R>(&mut self, content: impl FnOnce(&mut Ui) -> R) -> Option<R> {
        let column = self.columns.get(self.next)?;
        self.next += 1;
        let last = column.width <= 0.0;
        let w = if last {
            (self.row.max.x - self.x).max(0.0)
        } else {
            column.width
        };
        let rect = egui::Rect::from_min_size(
            egui::pos2(self.x, self.row.min.y),
            Vec2::new(w, self.row.height()),
        );
        self.x += w;
        let layout = if last {
            Layout::right_to_left(egui::Align::Center)
        } else {
            Layout::left_to_right(egui::Align::Center)
        };
        let mut child = self
            .ui
            .new_child(egui::UiBuilder::new().max_rect(rect).layout(layout));
        child.spacing_mut().item_spacing.x = theme::SPACE[1];
        Some(content(&mut child))
    }
}

/// Draw one row; `cells` calls `Cells::cell` once per column.
///
/// Why: rows of one table share a height, a divider and a hover fill.
///
/// # Rules
/// - The row is 40 high and fills the width; the divider is the `line` colour at 55 percent.
///
/// # Do not
/// - Draw more cells than the table has columns; the extras are dropped.
pub fn row(ui: &mut Ui, columns: &[Column<'_>], cells: impl FnOnce(&mut Cells<'_, '_>)) {
    let t = theme::tokens(ui.ctx());
    let width = ui.available_width();
    ui.add_space(-ui.spacing().item_spacing.y);
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, ROW_HEIGHT), Sense::hover());
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, 0.0, t.raised.gamma_multiply(0.5));
    }
    ui.painter().hline(
        rect.x_range(),
        rect.max.y - 0.5,
        Stroke::new(theme::BORDER, t.line.gamma_multiply(0.55)),
    );
    let mut all = Cells {
        ui,
        columns,
        row: rect,
        next: 0,
        x: rect.min.x,
    };
    cells(&mut all);
}

/// A key cap: Mono 12 text on a `raised` fill with a 1 px `line` border, 20 high, not pressable.
///
/// Why: a shortcut reads as a key, and a list of them must not add a control per row.
///
/// # Rules
/// - The text is the accessible name; `tip` says what the key does.
///
/// # Do not
/// - Make it a button; the Change button beside it is the control.
pub fn key_cap(ui: &mut Ui, text: &str, tip: &str) -> egui::Response {
    let t = theme::tokens(ui.ctx());
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_owned(), fonts::Role::Mono.font(), t.text);
    let size = Vec2::new(galley.size().x + 12.0, super::chip::CHIP_HEIGHT);
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, text));
    let radius = egui::CornerRadius::same(theme::RADIUS_CONTROL);
    ui.painter().rect_filled(rect, radius, t.raised);
    ui.painter().rect_stroke(
        rect,
        radius,
        Stroke::new(theme::BORDER, t.line),
        egui::StrokeKind::Inside,
    );
    let pos = egui::pos2(rect.min.x + 6.0, rect.center().y - galley.size().y / 2.0);
    ui.painter().galley(pos, galley, t.text);
    response.on_hover_text(tip)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui_kit::testing::{context, frame};

    /// A row is 40 high and its last cell starts where the fixed columns end.
    #[test]
    fn row_is_forty_high_and_cells_line_up() {
        let ctx = context(true);
        let columns = [
            Column {
                title: "Action",
                width: 200.0,
            },
            Column {
                title: "",
                width: 0.0,
            },
        ];
        let mut height = 0.0;
        let mut second_x = 0.0;
        let mut start_x = 0.0;
        frame(&ctx, vec![], |ui| {
            start_x = ui.cursor().min.x;
            head(ui, &columns);
            let top = ui.cursor().min.y;
            row(ui, &columns, |cells| {
                cells.cell(|ui| ui.label("Select"));
                cells.cell(|ui| {
                    second_x = ui.max_rect().min.x;
                });
            });
            height = ui.cursor().min.y - top;
        });
        assert_eq!(height, ROW_HEIGHT);
        assert_eq!(second_x - start_x, 200.0);
    }
}
