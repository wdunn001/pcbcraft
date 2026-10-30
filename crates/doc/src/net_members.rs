//! Everything on one net, answered by query from the lists the project
//! already stores.
//!
//! A net is its connections, not a list of its own: schematic wires, labels
//! and pins, and board tracks, vias, pads and zones. Nothing here is saved, so
//! a project file is identical with or without this module.
//!
//! # Rules
//! - Read only; never cache a result on the project.
//! - A via counts for every copper layer in its span, as `Project::via_span_layers` gives it.
//!
//! # Dependencies
//! - `crates/doc/src/lib.rs` for `Project`, `Track`, `Via`, `CopperZone`, `Footprint`, `Wire` and `Label`.
//!
//! # Do not
//! - Add a field to `Project` to hold these results.

use crate::{Id, Project};
use serde::Serialize;
use std::collections::BTreeSet;

/// Board copper of one net on one copper layer.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LayerMembers {
    /// Copper layer index, 0 is the top.
    pub layer: u8,
    /// Layer name from the stackup.
    pub name: String,
    /// Track ids on this layer.
    pub tracks: Vec<Id>,
    /// Zone ids on this layer.
    pub zones: Vec<Id>,
    /// Ids of the vias that conduct on this layer.
    pub vias: Vec<Id>,
}

/// One via of a net with the copper layers it conducts on.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ViaMember {
    /// Via id.
    pub id: Id,
    /// Upper and lower span layer indexes.
    pub span: (u8, u8),
    /// Every copper layer index in the span.
    pub layers: Vec<u8>,
}

/// One pad on a net.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PadMember {
    /// Footprint id.
    pub footprint: Id,
    /// Component id the footprint belongs to.
    pub component: Id,
    /// Component reference, for example "R1".
    pub reference: String,
    /// Pad number.
    pub pad: String,
}

/// One schematic pin on a net.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PinMember {
    /// Component id.
    pub component: Id,
    /// Component reference.
    pub reference: String,
    /// Pin number.
    pub pin: String,
}

/// Every member of one net.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct NetMembers {
    /// Net id.
    pub net: Id,
    /// Net name.
    pub name: String,
    /// Copper layers the net runs on (tracks, zones and via spans), top first.
    pub layers: Vec<u8>,
    /// Board copper grouped by layer, top first, only layers that carry some.
    pub by_layer: Vec<LayerMembers>,
    /// Vias with their spans.
    pub vias: Vec<ViaMember>,
    /// Footprint pads on the net.
    pub pads: Vec<PadMember>,
    /// Schematic pins on the net.
    pub pins: Vec<PinMember>,
    /// Schematic wire ids.
    pub wires: Vec<Id>,
    /// Schematic label ids.
    pub labels: Vec<Id>,
}

impl Project {
    /// All members of net `net`, or `None` when no such net exists.
    ///
    /// Why: the Layers tree and the net panel ask "what is on this net and on
    /// which layers" and should not each walk the project lists themselves.
    ///
    /// # Rules
    /// - Layers are the union of track layers, zone layers and via spans.
    ///
    /// # Dependencies
    /// - `crates/doc/src/lib.rs` for `via_span_layers` and `layer_name`.
    ///
    /// # Do not
    /// - Store the answer; callers query again after an edit.
    pub fn net_members(&self, net: Id) -> Option<NetMembers> {
        let name = self.nets.iter().find(|n| n.id == net)?.name.clone();
        let mut layers: BTreeSet<u8> = BTreeSet::new();
        let mut per: std::collections::BTreeMap<u8, LayerMembers> = Default::default();
        let mut entry = |this: &Project, i: u8| {
            layers.insert(i);
            per.entry(i).or_insert_with(|| LayerMembers {
                layer: i,
                name: crate::Layer::from_index(i)
                    .map(|l| this.layer_name(l))
                    .unwrap_or_default(),
                tracks: vec![],
                zones: vec![],
                vias: vec![],
            });
        };
        let mut tracks: Vec<(u8, Id)> = vec![];
        let mut zones: Vec<(u8, Id)> = vec![];
        for t in self.board.tracks.iter().filter(|t| t.net == net) {
            entry(self, t.layer.index());
            tracks.push((t.layer.index(), t.id));
        }
        for z in self.board.zones.iter().filter(|z| z.net == net) {
            entry(self, z.layer.index());
            zones.push((z.layer.index(), z.id));
        }
        let mut vias = vec![];
        for v in self.board.vias.iter().filter(|v| v.net == net) {
            let (a, b) = self.via_span(v);
            let span_layers: Vec<u8> = self
                .via_span_layers(v)
                .into_iter()
                .map(|l| l.index())
                .collect();
            for i in &span_layers {
                entry(self, *i);
            }
            vias.push(ViaMember {
                id: v.id,
                span: (a.index(), b.index()),
                layers: span_layers,
            });
        }
        let mut by_layer: Vec<LayerMembers> = per.into_values().collect();
        for lm in &mut by_layer {
            lm.tracks = tracks
                .iter()
                .filter(|t| t.0 == lm.layer)
                .map(|t| t.1)
                .collect();
            lm.zones = zones
                .iter()
                .filter(|z| z.0 == lm.layer)
                .map(|z| z.1)
                .collect();
            lm.vias = vias
                .iter()
                .filter(|v| v.layers.contains(&lm.layer))
                .map(|v| v.id)
                .collect();
        }
        let reference = |c: Id| {
            self.components
                .iter()
                .find(|x| x.id == c)
                .map(|x| x.reference.clone())
                .unwrap_or_default()
        };
        let pads = self
            .board
            .footprints
            .iter()
            .flat_map(|f| {
                f.pads
                    .iter()
                    .filter(|p| p.net == Some(net))
                    .map(move |p| (f, p))
            })
            .map(|(f, p)| PadMember {
                footprint: f.id,
                component: f.component,
                reference: reference(f.component),
                pad: p.number.clone(),
            })
            .collect();
        let pins = self
            .components
            .iter()
            .flat_map(|c| {
                c.pins
                    .iter()
                    .filter(|p| p.net == Some(net))
                    .map(move |p| PinMember {
                        component: c.id,
                        reference: c.reference.clone(),
                        pin: p.number.clone(),
                    })
            })
            .collect();
        Some(NetMembers {
            net,
            name,
            layers: layers.into_iter().collect(),
            by_layer,
            vias,
            pads,
            pins,
            wires: self
                .wires
                .iter()
                .filter(|w| w.net == net)
                .map(|w| w.id)
                .collect(),
            labels: self
                .labels
                .iter()
                .filter(|l| l.net == net)
                .map(|l| l.id)
                .collect(),
        })
    }
}
