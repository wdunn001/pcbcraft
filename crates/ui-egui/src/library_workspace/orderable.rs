//! The "Orderable part" row: pick an orderable part record for a placed generic part.
//!
//! Context: issue #813. A placed part starts generic. This row lists every orderable
//! part record on the project library as "manufacturer mpn" plus a first entry for
//! generic. Choosing an orderable part runs `part.make_orderable`; choosing generic
//! runs `part.make_generic`. Both are one undo step and leave geometry unchanged.
//!
//! # Dependencies
//! - crates/ui-egui/src/library_workspace/mod.rs (`App`, `Card`)
//! - crates/ui-egui/src/ui_kit/form_row.rs (`form_row`, `FormWidth`)
//! - crates/engine/src/lib.rs (`part.make_orderable`, `part.make_generic`)
//!
//! # Do not
//! - Store state here; the project library owns orderable part records.

use super::*;
use crate::ui_kit::form_row::{FormWidth, form_row};

/// Draw the "Orderable part" row under the part page facts.
///
/// # Rules
/// - Only shown when a placed component backs the card (has an id).
/// - First entry is "Generic (no maker part)" which calls `part.make_generic`.
/// - Every project orderable part record is listed as "manufacturer mpn".
///
/// # Dependencies
/// - crates/ui-egui/src/library_workspace/mod.rs (`App`, `Card`)
///
/// # Do not
/// - Show this row for cards without a placed component.
pub(super) fn library_orderable_row(
    app: &mut App,
    ui: &mut egui::Ui,
    _card: &Card,
    component_id: pcbcraft_doc::Id,
) {
    let _t = crate::theme::tokens(ui.ctx());
    let label = t!("libws.orderable.label");
    let _tip = t!("libws.orderable.tip");
    let generic_label = t!("libws.orderable.generic");

    let current = app
        .project
        .part_links
        .get(&component_id)
        .and_then(|l| l.orderable_part_id.as_ref())
        .cloned();

    let mut options = vec![(None, generic_label)];
    for o in &app.project.library.orderable_parts {
        let display = format!("{} {}", o.manufacturer, o.mpn);
        options.push((Some(o.id.clone()), display));
    }

    let mut current_index = options
        .iter()
        .position(|(id, _)| id.as_ref() == current.as_ref())
        .unwrap_or(0);
    let current_display = options[current_index].1.clone();

    form_row(ui, FormWidth::Page, &label, None, None, |ui| {
        let combo = egui::ComboBox::from_id_salt("libws-orderable").selected_text(&current_display);
        combo.show_ui(ui, |ui| {
            for (i, (id, display)) in options.iter().enumerate() {
                if ui
                    .selectable_value(&mut current_index, i, display)
                    .changed()
                {
                    match id {
                        Some(orderable_id) => {
                            app.tool_call(
                                "part.make_orderable",
                                json!({
                                    "id": component_id,
                                    "orderable_id": orderable_id
                                }),
                            );
                        }
                        None => {
                            app.tool_call("part.make_generic", json!({ "id": component_id }));
                        }
                    }
                }
            }
        });
    });
}
