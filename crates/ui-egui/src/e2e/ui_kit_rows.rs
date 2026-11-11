//! Test-only e2e story for the ui kit rows.
//!
//! Context: the Layout page shows a Layers tree and a findings list; these rows
//! are the ui kit widgets that build them. This story drives the row widgets
//! directly through crate::ui_kit::testing to prove they paint, measure and
//! respond as the design system requires.
//!
//! # Rules
//! - Stories drive the ui kit widgets directly; no app harness.
//!
//! # Dependencies
//! - crates/ui-egui/src/ui_kit/rows.rs
//! - crates/ui-egui/src/ui_kit/testing.rs

#![cfg(test)]

use crate::ui_kit::rows::{Finding, Severity, TreeRow, finding_row, list_row, tree_row};
use crate::ui_kit::testing::{context, frame};
use egui::{Color32, Event, Modifiers, PointerButton, pos2};

/// A newcomer browses the Layers tree and the findings list.
///
/// This story proves that tree rows paint their name and swatch, that the eye
/// cell responds to a click, that finding rows measure at the design height and
/// expose the Show, Hide and Mute actions, and that list rows measure correctly.
///
/// # Rules
/// - Plain-word labels, no glyphs.
///
/// # Dependencies
/// - crates/ui-egui/src/ui_kit/rows.rs
/// - crates/ui-egui/src/ui_kit/testing.rs
///
/// # Do not
/// - Drive the full app harness here.
#[test]
fn a_newcomer_browses_the_layers_tree_and_the_findings_list() -> Result<(), String> {
    let ctx = context(true);

    let has_text = |out: &egui::FullOutput, needle: &str| {
        out.shapes
            .iter()
            .any(|s| matches!(&s.shape, egui::Shape::Text(t) if t.galley.text().contains(needle)))
    };

    let row = TreeRow {
        depth: 1,
        expanded: Some(true),
        swatch: Some(Color32::from_rgb(200, 120, 40)),
        icon: None,
        name: "Copper Top",
        count: Some("12 nets".to_owned()),
        locked: Some(false),
        visible: Some(true),
        selected: false,
        dimmed: false,
    };

    let mut row_rect = egui::Rect::NOTHING;
    for _ in 0..2 {
        frame(&ctx, vec![], |ui| {
            let resp = tree_row(ui, "layer-top", &row);
            row_rect = resp.row.rect;
        });
    }

    let at = pos2(row_rect.max.x - 16.0, row_rect.center().y);
    frame(
        &ctx,
        vec![Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Modifiers::NONE,
        }],
        |ui| {
            tree_row(ui, "layer-top", &row);
        },
    );

    let mut eye_pressed = false;
    let tree_out = frame(
        &ctx,
        vec![Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::NONE,
        }],
        |ui| {
            let resp = tree_row(ui, "layer-top", &row);
            eye_pressed = resp.eye_pressed;
        },
    );

    if !eye_pressed {
        return Err("the eye cell did not respond".to_owned());
    }

    if !has_text(&tree_out, "Copper Top") {
        return Err("the tree row name was not painted".to_owned());
    }

    let finding = Finding {
        severity: Severity::Error,
        title: "Track too close to the edge",
        detail: "1 of 24 tracks breaks the keepout",
        chip: Some("GND"),
        selected: false,
        compact: false,
    };

    let mut finding_rect = egui::Rect::NOTHING;
    let out = frame(&ctx, vec![], |ui| {
        finding_rect = finding_row(ui, &finding).row.rect;
    });

    if (finding_rect.height() - 48.0).abs() > 0.6 {
        return Err(format!("finding row height {0}", finding_rect.height()));
    }

    if !has_text(&out, "Track too close to the edge") {
        return Err("finding title not painted".to_owned());
    }

    if !has_text(&out, "Show") {
        return Err("finding Show action not painted".to_owned());
    }

    if !has_text(&out, "Hide") {
        return Err("finding Hide action not painted".to_owned());
    }

    if !has_text(&out, "Mute") {
        return Err("finding Mute action not painted".to_owned());
    }

    let mut list_rect = egui::Rect::NOTHING;
    frame(&ctx, vec![], |ui| {
        list_rect = list_row(ui, true, "board.top.gbr", "Open this plot", |ui| {
            ui.label("ready");
        })
        .rect;
    });

    if (list_rect.height() - 36.0).abs() > 0.6 {
        return Err(format!("list row height {0}", list_rect.height()));
    }

    Ok(())
}
