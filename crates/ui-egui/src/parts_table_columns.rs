//! Configure which extra columns the Parts grid and the bill of materials show.
//!
//! Context: issue #169. Projects can define up to sixteen named component
//! when the list is empty) as one command; Cancel discards the draft. A
//! refused command shows the engine message unchanged and keeps the dialog
//! open. The Datasheet suggestion adds the key Datasheet with the label
//! Datasheet.
//! properties to print in the bill and in the Parts grid. This file offers
//! one dialog to add, rename and remove those columns in a single undo step,
//! and a helper that returns the value of each column for a given component.
//!
//! Local rules: the dialog keeps its draft state in egui memory under the id
//! 'parts-columns-draft'. Apply runs bom.set_fields (or bom.clear_fields
