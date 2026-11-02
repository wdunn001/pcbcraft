//! What the Layout canvas paints for each copper layer, and what a selected layer row lights.
//!
//! Context: the Layers tree hides a layer by its eye (`Preferences::hidden_layers`). The canvas
//! painter and the row hint both ask the questions here, so what is painted and what the hint
//! counts cannot drift apart.
//! Rules: a part belongs to the layer it is mounted on and is shown or hidden as one thing; a
//! via belongs to every layer of its span and is hidden only when all of them are hidden; a
//! ratsnest line is hidden when every one of its ends is hidden.
//! Do not: paint from here; these are questions, the painter keeps the drawing.

use super::*;
use crate::layer_visibility::{self as lv, SubLayer};
use pcbcraft_doc::{Footprint, Layer, Via};

/// Items of one layer, split into what exists and what is painted right now.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct LayerCount {
    /// Tracks on the layer.
    pub tracks: usize,
    /// Pads of parts mounted on the layer.
    pub pads: usize,
    /// Package artwork: part outlines, courtyards and drawn silkscreen.
    pub silkscreen: usize,
    /// Reference and value labels and board text.
    pub labels: usize,
    /// Copper zones.
    pub zones: usize,
    /// Vias that touch the layer.
    pub vias: usize,
    /// Drawn board shapes bound to the layer (front or back artwork).
    pub drawn: usize,
    /// Ratsnest lines with an end on the layer.
    pub ratsnest: usize,
}

impl App {
    /// The tree path of a copper layer's group (`side:top`, `side:bottom`, `inner:n`).
    pub(crate) fn layer_path(&self, layer: Layer) -> String {
        let count = usize::from(self.project.copper_layer_count());
        let layer = if layer == Layer::Back {
            self.project.bottom_layer()
        } else {
            layer
        };
        lv::side_path_for_layer(count, layer)
    }

    /// True when the layer's eye is on.
    pub(crate) fn layer_group_shown(&self, layer: Layer) -> bool {
        lv::group_stored(&self.preferences.hidden_layers, &self.layer_path(layer))
    }

    /// True when a slice of the layer (tracks, pads, zones, artwork, text) is painted.
    pub(crate) fn layer_part_shown(&self, layer: Layer, sub: SubLayer) -> bool {
        lv::sub_effective(
            &self.preferences.hidden_layers,
            &self.layer_path(layer),
            sub,
        )
    }

    /// The copper layer a part is mounted on, in this board's numbering.
    pub(crate) fn mount_layer(&self, f: &Footprint) -> Layer {
        crate::stack_view::mount_copper(self.project.copper_layer_count(), f.board_side())
    }

    /// A part is painted at all: its side is on. Everything of the part follows this.
    pub(crate) fn part_shown(&self, f: &Footprint) -> bool {
        self.layer_group_shown(self.mount_layer(f))
    }

    /// The part's pads are painted.
    pub(crate) fn part_pads_shown(&self, f: &Footprint) -> bool {
        self.part_shown(f) && self.layer_part_shown(self.mount_layer(f), SubLayer::Pads)
    }

    /// The part's outline, courtyard and drawn artwork are painted.
    pub(crate) fn part_artwork_shown(&self, f: &Footprint) -> bool {
        self.part_shown(f) && self.layer_part_shown(self.mount_layer(f), SubLayer::Silk)
    }

    /// The part's reference and value are painted.
    pub(crate) fn part_labels_shown(&self, f: &Footprint) -> bool {
        self.part_artwork_shown(f)
    }

    /// A via is painted when the Vias row is on and any layer of its span is on.
    pub(crate) fn via_shown(&self, v: &Via) -> bool {
        if !self.layer_shown("Vias") {
            return false;
        }
        let (upper, lower) = self.project.via_span(v);
        (upper.index()..=lower.index())
            .filter_map(|i| Layer::from_index(i).ok())
            .any(|l| self.layer_group_shown(l))
    }

    /// The layer a free board graphic belongs to: back for `B.*`, front otherwise.
    pub(crate) fn graphic_layer(g: &pcbcraft_doc::Graphic) -> Layer {
        if g.sublayer.starts_with("B.") {
            Layer::Back
        } else {
            Layer::Front
        }
    }

    /// A free board graphic is painted: its side is on, and for silkscreen and plain artwork
    /// the artwork slice is on too.
    pub(crate) fn graphic_shown(&self, g: &pcbcraft_doc::Graphic) -> bool {
        let layer = Self::graphic_layer(g);
        if g.sublayer.ends_with("Mask") {
            self.layer_group_shown(layer)
        } else {
            self.layer_part_shown(layer, SubLayer::Silk)
        }
    }

    /// The layer one end of a ratsnest line sits on: a part's mount layer, or a copper object's
    /// own layer. A through part or via has ends on every layer it reaches.
    fn object_layers(&self, id: Id) -> Vec<Layer> {
        let p = &self.project;
        if let Some(f) = p.board.footprints.iter().find(|f| f.id == id) {
            return vec![self.mount_layer(f)];
        }
        if let Some(t) = p.board.tracks.iter().find(|t| t.id == id) {
            return vec![t.layer];
        }
        if let Some(z) = p.board.zones.iter().find(|z| z.id == id) {
            return vec![z.layer];
        }
        if let Some(v) = p.board.vias.iter().find(|v| v.id == id) {
            let (a, b) = p.via_span(v);
            return (a.index()..=b.index())
                .filter_map(|i| Layer::from_index(i).ok())
                .collect();
        }
        vec![Layer::Front]
    }

    /// A ratsnest line is painted unless every end of it is on a hidden layer.
    pub(crate) fn airwire_shown(&self, from_object: Id, to_object: Id) -> bool {
        [from_object, to_object].iter().any(|id| {
            self.object_layers(*id)
                .into_iter()
                .any(|l| self.layer_group_shown(l))
        })
    }

    /// The airwires check results as (from object, to object) pairs.
    fn airwire_ends(&self) -> Vec<(Id, Id)> {
        self.checks
            .get("airwires")
            .and_then(Value::as_array)
            .map(|all| {
                all.iter()
                    .filter_map(|w| {
                        Some((
                            w.get("from_object")?.as_u64()?,
                            w.get("to_object")?.as_u64()?,
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Count what lives on `layer` and what of it is painted now.
    ///
    /// Returns (existing, painted). A part counts on its mount layer; a via on every layer of
    /// its span.
    pub(crate) fn layer_counts(&self, layer: Layer) -> (LayerCount, LayerCount) {
        let p = &self.project;
        let here = |l: Layer| self.layer_path(l) == self.layer_path(layer);
        let mut have = LayerCount::default();
        let mut shown = LayerCount::default();
        for t in p.board.tracks.iter().filter(|t| here(t.layer)) {
            have.tracks += 1;
            shown.tracks += usize::from(self.layer_part_shown(t.layer, SubLayer::Tracks));
        }
        for z in p.board.zones.iter().filter(|z| here(z.layer)) {
            have.zones += 1;
            shown.zones += usize::from(self.layer_part_shown(z.layer, SubLayer::Zones));
        }
        for t in p.board.texts.iter().filter(|t| here(t.layer)) {
            have.labels += 1;
            shown.labels += usize::from(self.layer_part_shown(t.layer, SubLayer::Text));
        }
        for f in p
            .board
            .footprints
            .iter()
            .filter(|f| here(self.mount_layer(f)))
        {
            let pads = f.pads.len();
            have.pads += pads;
            if self.part_pads_shown(f) {
                shown.pads += pads;
            }
            // Outline and courtyard, plus the library drawing, as one item each.
            let art = 2 + p
                .component(f.component)
                .and_then(|c| c.library_footprint.as_ref())
                .map_or(0, |l| l.definition.graphics.len());
            have.silkscreen += art;
            if self.part_artwork_shown(f) {
                shown.silkscreen += art;
            }
            have.labels += 2;
            if self.part_labels_shown(f) {
                shown.labels += 2;
            }
        }
        for v in p.board.vias.iter() {
            let (a, b) = p.via_span(v);
            if (a.index()..=b.index())
                .filter_map(|i| Layer::from_index(i).ok())
                .any(here)
            {
                have.vias += 1;
                shown.vias += usize::from(self.via_shown(v));
            }
        }
        for g in p.graphics.iter().filter(|g| g.layer == "board") {
            if here(Self::graphic_layer(g)) {
                have.drawn += 1;
                shown.drawn += usize::from(self.graphic_shown(g));
            }
        }
        for (a, b) in self.airwire_ends() {
            let ends: Vec<Layer> = [a, b]
                .iter()
                .flat_map(|id| self.object_layers(*id))
                .collect();
            if ends.into_iter().any(here) {
                have.ratsnest += 1;
                shown.ratsnest += usize::from(self.airwire_shown(a, b));
            }
        }
        (have, shown)
    }

    /// Whether the lit layer (a selected layer row) includes `layer`, or none is lit.
    pub(crate) fn layer_is_lit(&self, layer: Layer) -> bool {
        match &self.layer_lit {
            Some(path) => *path == self.layer_path(layer),
            None => true,
        }
    }

    /// Brightness of an element on `layer`: full on the lit layer or when none is lit, faint
    /// elsewhere.
    pub(crate) fn lit_alpha(&self, layer: Layer) -> f32 {
        if self.layer_is_lit(layer) { 1.0 } else { 0.22 }
    }

    /// The copper layer a tree path names.
    fn layer_for_path(&self, path: &str) -> Option<Layer> {
        match path {
            "side:top" => Some(Layer::Front),
            "side:bottom" => Some(self.project.bottom_layer()),
            _ => path
                .strip_prefix("inner:")
                .and_then(|n| n.parse::<u8>().ok())
                .and_then(|n| Layer::from_index(n).ok()),
        }
    }

    /// Light one layer row (or clear it) and say in plain words what it holds.
    pub(crate) fn light_layer(&mut self, path: Option<String>) {
        self.layer_lit = path.clone();
        self.message = match path.and_then(|p| self.layer_for_path(&p)) {
            Some(layer) => self.layer_hint(layer),
            None => t!("tree.layer_hint_clear"),
        };
    }

    /// The plain sentence a layer row says when it is picked.
    pub(crate) fn layer_hint(&self, layer: Layer) -> String {
        let (_, shown) = self.layer_counts(layer);
        t!(
            "tree.layer_hint",
            name = self.project.layer_name(layer),
            tracks = shown.tracks,
            pads = shown.pads,
            silk = shown.silkscreen,
            vias = shown.vias
        )
    }
}
