//! The copper layer new tracks and pours go on, for a board with any number
//! of copper layers.
//!
//! Context: `lib.rs` keeps the choice in `App::route_layer`; the Layers tree
//! sets it too (through `draw_target.rs`). This module owns the
//! layer-step keys and the rule that keeps the choice
//! valid when the stackup changes. Every value it stores is a real index into
//! the stackup, never the two-layer shorthand `Layer::Back`.
//!
//! Local rules: commands built from the choice name the layer with
//! `Project::layer_key`, because a bare `Layer` serialised alone reads as the
//! bottom layer when its index is 1, which is an inner layer on a board with
//! more than two.

use crate::App;
use pcbcraft_doc::Layer;

impl App {
    /// Move the routing layer one step through the stackup, wrapping at the ends.
    ///
    /// # Rules
    /// - `forward` goes toward the bottom layer; the top follows the bottom.
    /// - A board with one copper layer stays where it is.
    ///
    /// # Dependencies
    /// - crates/doc/src/lib.rs (`Project::copper_layer_count`)
    ///
    /// # Do not
    /// - Step by toggling between Top and Bottom; inner layers count.
    pub(crate) fn step_route_layer(&mut self, forward: bool) {
        let count = self.project.copper_layer_count().max(1);
        let here = self.route_layer.index().min(count - 1);
        let next = if forward {
            (here + 1) % count
        } else {
            (here + count - 1) % count
        };
        if let Ok(layer) = Layer::from_index(next) {
            self.route_layer = layer;
        }
    }

    /// The routing layer as a command parameter that reads back to the same layer.
    ///
    /// # Rules
    /// - Uses the position-based name from `Project::layer_key`.
    ///
    /// # Dependencies
    /// - crates/doc/src/lib.rs (`Project::layer_key`)
    ///
    /// # Do not
    /// - Use `json!(self.route_layer)`; see the module header.
    pub(crate) fn route_layer_param(&self) -> serde_json::Value {
        serde_json::json!(self.project.layer_key(self.route_layer))
    }

    /// Whether the routing layer is the bottom of a board that has a distinct bottom.
    ///
    /// # Rules
    /// - False on a one-layer board, where top and bottom are the same copper.
    ///
    /// # Dependencies
    /// - crates/doc/src/lib.rs (`Project::is_bottom`)
    ///
    /// # Do not
    /// - Compare `route_layer` with `Layer::Back`; that is an inner layer on a four-layer board.
    pub(crate) fn route_on_bottom(&self) -> bool {
        self.route_layer != Layer::Front && self.project.is_bottom(self.route_layer)
    }

    /// Keep `route_layer` on a layer the new stackup has.
    ///
    /// Why: adding or removing an inner pair moves the bottom layer's index;
    /// someone drawing on the bottom should stay on the bottom, and a layer
    /// that no longer exists falls back to the bottom.
    ///
    /// # Rules
    /// - `was_bottom` says the old choice was the bottom layer of the old stackup.
    /// - Does nothing when the choice is still a layer of the stackup and was not the bottom.
    ///
    /// # Dependencies
    /// - crates/ui-egui/src/lib.rs (`App::refresh` calls this after loading a project)
    ///
    /// # Do not
    /// - Call it with a bottom flag read from the new project.
    pub(crate) fn keep_route_layer_valid(&mut self, was_bottom: bool) {
        let count = self.project.copper_layer_count();
        if was_bottom || self.route_layer.index() >= count {
            self.route_layer = self.project.bottom_layer();
        }
    }
}
