//! Variant controls for the Parts tab: build selector and per-part DNP cell.
//!
//! Context: issue #169. A variant is a named build with a set of components
//! that are not populated. The picker lets the user choose which variant to
//! edit; the DNP cell flips whether one part is left off that build.
//!
//! # Dependencies
//! - crates/ui-egui/src/ui_kit/select.rs (`select`)
//! - crates/engine/src/lib.rs (`variant.set_dnp`)
//! - crates/text/strings/en.txt (parts_table.variant.*, parts_table.dnp.*)
//!
//! # Do not
//! - Change the document directly; call back to the caller who runs the command.

use super::*;
use crate::a11y::Named;
use crate::ui_kit::select::select;

/// Build variant selector; returns the index of the selected variant.
///
/// Why: the user picks which build they are editing DNP settings for.
/// "No variant" (index 0) means the design as placed, with no skips.
///
/// # Rules
/// - `variants` is the list of variant names; a "No variant" entry is prepended.
/// - `chosen` is the selected index; 0 = no variant, 1+ = that variant.
/// - Returns `Some(index)` when the selection changed, `None` otherwise.
///
/// # Dependencies
/// - crates/ui-egui/src/ui_kit/select.rs (`select`)
///
/// # Do not
/// - Pass an empty variants list without first showing the picker.
pub(crate) fn variant_picker(
    ui: &mut egui::Ui,
    variants: &[String],
    chosen: &mut usize,
) -> Option<usize> {
    let options: Vec<&str> = std::iter::once("No variant")
        .chain(variants.iter().map(|v| v.as_str()))
        .collect();
    let response = select(
        ui,
        "parts-variant",
        chosen,
        &options,
        &t!("parts_table.variant.name"),
        &t!("parts_table.variant.tip"),
        160.0,
    );
    if response.changed() {
        Some(*chosen)
    } else {
        None
    }
}

/// A DNP checkbox for one part in one variant.
///
/// Why: the user marks which parts are left off a build.
///
/// # Rules
/// - `reference` is shown in the checkbox name and tooltip.
/// - Returns `Some(new_value)` when the checkbox changed, `None` otherwise.
///
/// # Dependencies
/// - crates/text/strings/en.txt (parts_table.dnp.name, parts_table.dnp.tip)
///
/// # Do not
/// - Use this without a variant selected; the DNP set is per-variant.
pub(crate) fn dnp_cell(
    ui: &mut egui::Ui,
    row_id: Id,
    reference: &str,
    is_dnp: bool,
) -> Option<bool> {
    let mut checked = is_dnp;
    ui.push_id(row_id, |ui| {
        let response = ui
            .checkbox(&mut checked, "")
            .named(&t!("parts_table.dnp.name", part = reference))
            .on_hover_text(t!("parts_table.dnp.tip"));
        if response.changed() {
            Some(checked)
        } else {
            None
        }
    })
    .inner
}
