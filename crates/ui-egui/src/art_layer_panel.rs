//! Properties of a drawn layer: its name, kind (Cut, Perforation, etc.) and
//! dotted toggle.
//!
//! Context: `layers_tree.rs` calls `art_layer_properties` when the user selects
//! a user-drawn layer row in the tree. The user edits the layer's kind and the
//! dotted flag, then leaves the control or presses Enter to commit.
//! Local rules: the kind picker uses the five kinds the engine understands:
//! Cut, Perforation, Silk, Mask, Other; the dotted flag uses the engine
//! command `board.layer_dotted_set`.
//!
//! # Dependencies
//! - crates/engine/src/lib.rs (`board.layer_kind_set`, `board.layer_dotted_set`)
//! - crates/ui-egui/src/ui_kit/segmented.rs
//! - crates/ui-egui/src/ui_kit/checkbox.rs
//!
//! # Do not
//! - Call engine commands while the user is still editing.
use super::*;
use crate::ui_kit::checkbox::checkbox;
use crate::ui_kit::form_row::{FormWidth, form_row};
use crate::ui_kit::segmented::{Segment, SegmentedStyle, segmented_styled};
use egui::Ui;

/// Segments for the kind picker.
const KINDS: [Segment<'static>; 5] = [
    Segment::text("Cut", "Cutter follows this line"),
    Segment::text("Perforation", "A row of holes for snapping apart"),
    Segment::text("Silkscreen", "Printed marks on the board"),
    Segment::text("Mask", "Where solder mask is left off"),
    Segment::text("Other", "Reference or documentation art"),
];

fn kind_index(kind: pcbcraft_doc::ArtKind) -> usize {
    match kind {
        pcbcraft_doc::ArtKind::Cut => 0,
        pcbcraft_doc::ArtKind::Perforation => 1,
        pcbcraft_doc::ArtKind::Silk => 2,
        pcbcraft_doc::ArtKind::Mask => 3,
        pcbcraft_doc::ArtKind::Other
        | pcbcraft_doc::ArtKind::Engrave
        | pcbcraft_doc::ArtKind::Fabrication => 4,
    }
}

fn kind_from_index(i: usize) -> pcbcraft_doc::ArtKind {
    match i {
        0 => pcbcraft_doc::ArtKind::Cut,
        1 => pcbcraft_doc::ArtKind::Perforation,
        2 => pcbcraft_doc::ArtKind::Silk,
        3 => pcbcraft_doc::ArtKind::Mask,
        _ => pcbcraft_doc::ArtKind::Other,
    }
}

fn kind_name(i: usize) -> &'static str {
    match i {
        0 => "cut",
        1 => "perforation",
        2 => "silk",
        3 => "mask",
        _ => "other",
    }
}

impl App {
    /// Properties of a user-drawn layer: name, kind and dotted flag.
    ///
    /// # Rules
    /// - Shows the layer name as a heading.
    /// - Kind picker uses a segmented control over five options.
    /// - Dotted flag uses a checkbox bound to the engine command.
    ///
    /// # Dependencies
    /// - crates/engine/src/lib.rs (`board.layer_kind_set`, `board.layer_dotted_set`)
    ///
    /// # Do not
    /// - Call engine commands while the user is still editing.
    pub(crate) fn art_layer_properties(&mut self, ui: &mut Ui, id: u64) {
        let Some(layer) = self.project.board.user_layers.iter().find(|l| l.id == id) else {
            ui.label(t!("tree.art_layer.missing"));
            return;
        };
        ui.label(crate::fonts::title(layer.name.clone()));
        ui.label(
            egui::RichText::new(t!("tree.art_layer.help"))
                .font(crate::fonts::Role::Caption.font())
                .color(self.muted()),
        );
        ui.add_space(8.0);
        let key = egui::Id::new(("art-layer-edit", id));
        let mut kind: usize = ui
            .data(|d| d.get_temp(key.with("k")))
            .unwrap_or(kind_index(layer.kind));
        let mut dotted: bool = ui
            .data(|d| d.get_temp(key.with("d")))
            .unwrap_or(layer.dotted);
        ui.label(t!("tree.art_layer.kind"));
        let k_resp = segmented_styled(
            ui,
            key.with("seg"),
            &mut kind,
            &KINDS,
            SegmentedStyle {
                group_name: Some("Kind"),
                ..SegmentedStyle::default()
            },
        )
        .on_hover_text(t!("tree.art_layer.kind.tip"))
        .named(&t!("tree.art_layer.kind"));
        ui.add_space(6.0);
        // The box paints no text of its own: the form row carries the visible label.
        let d_resp = form_row(
            ui,
            FormWidth::Aside,
            &t!("tree.art_layer.dotted"),
            None,
            None,
            |ui| {
                checkbox(
                    ui,
                    &mut dotted,
                    &t!("tree.art_layer.dotted"),
                    &t!("tree.art_layer.dotted.tip"),
                )
                .named(&t!("tree.art_layer.dotted"))
            },
        );
        let commit = k_resp.changed() || d_resp.changed();
        if commit {
            let current_kind = layer.kind;
            let current_dotted = layer.dotted;
            let target_kind = kind_from_index(kind);
            let target_dotted = dotted;
            if target_kind != current_kind {
                self.run(
                    "board.layer_kind_set",
                    json!({ "layer": id, "kind": kind_name(kind) }),
                );
            }
            if target_dotted != current_dotted {
                self.run(
                    "board.layer_dotted_set",
                    json!({ "layer": id, "on": target_dotted }),
                );
            }
            ui.data_mut(|d| {
                d.remove::<usize>(key.with("k"));
                d.remove::<bool>(key.with("d"));
            });
        } else {
            ui.data_mut(|d| {
                d.insert_temp(key.with("k"), kind);
                d.insert_temp(key.with("d"), dotted);
            });
        }
    }
}
