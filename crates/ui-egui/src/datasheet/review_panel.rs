//! Review panel for datasheet-extracted values with page provenance.
//!
//! Shows a list of extracted fields as collapsible rows. Each row has:
//! - The field label and extracted value
//! - A page badge button (p. N) that jumps to the source
//! - Accept and Reject buttons that push ReviewAction::Decide events
//!
//! The panel does not own the extracted data; it receives rows via set_review_rows()
//! and reports user actions via take_review_actions().

use crate::ui_kit::button::{Size, secondary_button};
use pcbcraft_text::t;

/// One row shown in the review panel.
pub(crate) struct ReviewRow {
    /// Table identifier (e.g. "pin_table", "absolute_maximum_ratings").
    pub table: String,
    /// Row index within the table.
    pub index: usize,
    /// Display label for the row.
    pub label: String,
    /// Zero-based page index in the PDF.
    pub page_index: usize,
    /// Human-readable page label for the badge.
    pub page_label: String,
    /// Source rectangle [x0, y0, x1, y1] in page points.
    pub rect: [f32; 4],
    /// Whether the row has been decided (accept/reject).
    pub decided: bool,
}

/// An action the user performed in the review panel.
pub(crate) enum ReviewAction {
    /// User clicked a page badge: jump to the source location.
    Jump { page_index: usize, rect: [f32; 4] },
    /// User clicked Accept or Reject on a row.
    Decide {
        table: String,
        index: usize,
        accept: bool,
    },
}

/// Show the review panel as a collapsing section.
pub(crate) fn show_review_panel(
    ui: &mut egui::Ui,
    rows: &[ReviewRow],
    actions: &mut Vec<ReviewAction>,
) {
    if rows.is_empty() {
        return;
    }

    let title = t!("datasheet.review.title");
    ui.collapsing(title, |ui| {
        for row in rows {
            show_review_row(ui, row, actions);
        }
    });
}

fn show_review_row(ui: &mut egui::Ui, row: &ReviewRow, actions: &mut Vec<ReviewAction>) {
    ui.horizontal(|ui| {
        // Field label and value
        ui.label(&row.label);

        // Page badge button
        let badge_label = t!("datasheet.review.page", n = row.page_label);
        let badge = secondary_button(
            ui,
            None,
            &badge_label,
            &t!("datasheet.review.page.tip"),
            Size::Small,
            !row.decided,
        );
        if badge.clicked() {
            actions.push(ReviewAction::Jump {
                page_index: row.page_index,
                rect: row.rect,
            });
        }

        // Accept button
        let accept = secondary_button(
            ui,
            None,
            &t!("datasheet.review.accept"),
            &t!("datasheet.review.accept.tip"),
            Size::Small,
            !row.decided,
        );
        if accept.clicked() {
            actions.push(ReviewAction::Decide {
                table: row.table.clone(),
                index: row.index,
                accept: true,
            });
        }

        // Reject button
        let reject = secondary_button(
            ui,
            None,
            &t!("datasheet.review.reject"),
            &t!("datasheet.review.reject.tip"),
            Size::Small,
            !row.decided,
        );
        if reject.clicked() {
            actions.push(ReviewAction::Decide {
                table: row.table.clone(),
                index: row.index,
                accept: false,
            });
        }
    });
}
