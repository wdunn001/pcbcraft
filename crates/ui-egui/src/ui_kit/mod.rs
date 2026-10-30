//! The component kit of the approved app design: small widget helpers that every
//! page adopts instead of drawing its own buttons, tabs, rows and fields.
//!
//! Context: tokens come from `crates/ui-egui/src/theme.rs`; the gallery in
//! `./gallery.rs` draws every component in both themes and is the visual proof.
//! Local rules: each helper takes a label and a tooltip, returns a `Response`
//! and sets `widget_info`; colour, size and radius come from `theme`, never literals.
//!
//! # Dependencies
//! - crates/ui-egui/src/theme.rs
//! - crates/ui-egui/src/ui_kit/gallery.rs
//! - crates/ui-egui/src/ui_kit/button.rs
//! - crates/ui-egui/src/ui_kit/chip.rs
//! - crates/ui-egui/src/ui_kit/focus.rs
//! - crates/ui-egui/src/ui_kit/gates.rs
//! - crates/ui-egui/src/ui_kit/icons.rs
//! - crates/ui-egui/src/ui_kit/menu.rs
//! - crates/ui-egui/src/ui_kit/menu_icons.rs
//! - crates/ui-egui/src/ui_kit/card.rs
//! - crates/ui-egui/src/ui_kit/checkbox.rs
//! - crates/ui-egui/src/ui_kit/dialog.rs
//! - crates/ui-egui/src/ui_kit/form_row.rs
//! - crates/ui-egui/src/ui_kit/number_field.rs
//! - crates/ui-egui/src/ui_kit/rows.rs
//! - crates/ui-egui/src/ui_kit/section.rs
//! - crates/ui-egui/src/ui_kit/select.rs
//! - crates/ui-egui/src/ui_kit/empty_state.rs
//! - crates/ui-egui/src/ui_kit/panel_header.rs
//! - crates/ui-egui/src/ui_kit/segmented.rs
//! - crates/ui-egui/src/ui_kit/status_item.rs
//! - crates/ui-egui/src/ui_kit/tab_bar.rs
//!
//! # Do not
//! - Touch `crates/ui-egui/src/lib.rs` from here; pages call into the kit, not the reverse.

pub mod button;
pub mod card;
pub mod checkbox;
pub mod chip;
pub mod dialog;
pub mod empty_state;
pub mod focus;
pub mod form_row;
pub mod gallery;
#[cfg(test)]
mod gates;
pub mod icons;
pub mod menu;
pub mod menu_icons;
pub mod number_field;
pub mod panel_header;
pub mod rows;
pub mod section;
pub mod segmented;
pub mod select;
pub mod status_item;
pub mod tab_bar;
#[cfg(test)]
mod testing;
