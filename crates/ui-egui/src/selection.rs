//! The one selection value the Layers tree, the canvas, Properties and the
//! Manufacture tab all read and write, and the relation rules that decide what
//! lights up weakly around it.
//!
//! Context: `lib.rs` holds one [`Selection`] in `App::subject`. The older
//! fields `App::selected` (the object id) and `TreeView::selected_key` (a row
//! that is not one object) are projections of it; `App::select_subject` writes
//! all of them in one place and `App::adopt_legacy_selection` turns a write to
//! either old field into a new value, so no reader sees two different answers.
//! Local rules: this file is pure. It reads a [`Project`] and never draws,
//! never mutates and never reads `App`.

use pcbcraft_doc::{Id, Layer, Project};
use std::collections::BTreeSet;

/// What is selected. Exactly one thing at a time.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub(crate) enum Selection {
    /// Nothing is selected.
    #[default]
    Empty,
    /// One document object: a part, via, track, zone, text, hole or label.
    Object(Id),
    /// A net. It is one thing wherever its rows appear in the tree.
    Net(Id),
    /// A physical layer of the stackup, by its tree key (`side:top`, `inner:1`,
    /// `side:bottom`, `diel:3`).
    Layer(String),
    /// A sublayer inside a layer, by its tree key (`side:top/silk`).
    Sublayer(String),
    /// A tree row that is not one of the above: the board outline, a sheet row.
    Row(String),
    /// A manufacturing artifact, by key (`drill:1-4:300`).
    Artifact(String),
}

/// How strongly something is lit.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Level {
    /// Not related to the selection.
    None,
    /// Related to the selection: a net's pads, a via's span.
    Weak,
    /// The selection itself.
    Strong,
}

/// What the selection touches, as plain sets the tree and the Manufacture
/// tab test rows against.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub(crate) struct Relations {
    /// The net the selection belongs to, whose rows light weakly.
    pub net: Option<Id>,
    /// Objects that light weakly: a net's tracks, zones, vias and parts.
    pub objects: BTreeSet<Id>,
    /// Copper layer indexes the selection reaches. When set, layer rows
    /// outside it dim. None means no layer is singled out.
    pub layers: Option<BTreeSet<u8>>,
}

impl Selection {
    /// The document object id this value stands for, when it is one.
    ///
    /// # Rules
    /// - Only [`Selection::Object`] has one; a net or a layer is not an object.
    ///
    /// # Do not
    /// - Use this to test for "something is selected"; compare with `Empty`.
    pub(crate) fn object(&self) -> Option<Id> {
        match self {
            Self::Object(id) => Some(*id),
            _ => None,
        }
    }

    /// Build the value from a tree row key that is not one object.
    ///
    /// # Rules
    /// - A last segment `net:N` is that net, wherever the row sits.
    /// - A key that starts with `side:`, `inner:` or `diel:` is a layer when
    ///   it is one segment and a sublayer when it has more.
    /// - Anything else is a plain row.
    pub(crate) fn from_key(key: &str) -> Self {
        let last = key.rsplit('/').next().unwrap_or("");
        if let Some(id) = last.strip_prefix("net:").and_then(|n| n.parse::<Id>().ok()) {
            return Self::Net(id);
        }
        if let Some(rest) = key.strip_prefix("artifact:") {
            return Self::Artifact(rest.to_owned());
        }
        let first = key.split('/').next().unwrap_or("");
        let stacked =
            first.starts_with("side:") || first.starts_with("inner:") || first.starts_with("diel:");
        match (stacked, key.contains('/')) {
            (true, false) => Self::Layer(key.to_owned()),
            (true, true) => Self::Sublayer(key.to_owned()),
            _ => Self::Row(key.to_owned()),
        }
    }

    /// Build the value from the two older fields: the selected object and the
    /// selected tree row.
    ///
    /// # Rules
    /// - An object wins over a row, as in the old Properties.
    pub(crate) fn from_legacy(selected: Option<Id>, key: Option<&str>) -> Self {
        match (selected, key) {
            (Some(id), _) => Self::Object(id),
            (None, Some(k)) => Self::from_key(k),
            (None, None) => Self::Empty,
        }
    }

    /// The tree row key the older field should hold for this value.
    ///
    /// `current` is the key already there; a net keeps it when it names that
    /// net, so the row the person clicked stays the lit one.
    pub(crate) fn legacy_key(&self, current: Option<&str>) -> Option<String> {
        match self {
            Self::Empty | Self::Object(_) => None,
            Self::Layer(k) | Self::Sublayer(k) | Self::Row(k) => Some(k.clone()),
            Self::Net(id) => {
                let last_is_net = current
                    .is_some_and(|c| c.rsplit('/').next() == Some(format!("net:{id}").as_str()));
                if last_is_net {
                    current.map(str::to_owned)
                } else {
                    Some(format!("nets/net:{id}"))
                }
            }
            Self::Artifact(rest) => Some(format!("artifact:{rest}")),
        }
    }

    /// The same value with a copper layer index taken from a tree key.
    ///
    /// Returns the copper index of the layer the key's first segment names.
    pub(crate) fn layer_index(project: &Project, key: &str) -> Option<u8> {
        let first = key.split('/').next()?;
        match first {
            "side:top" => Some(0),
            "side:bottom" => Some(project.bottom_layer().index()),
            _ => first
                .strip_prefix("inner:")
                .and_then(|n| n.parse::<u8>().ok()),
        }
    }
}

fn outer_index(project: &Project, layer: Layer) -> u8 {
    if project.is_bottom(layer) {
        project.bottom_layer().index()
    } else {
        layer.index()
    }
}

/// What `selection` touches in `project`.
///
/// Why: one place decides what lights weakly, so the tree, the canvas and the
/// Manufacture tab can never disagree.
///
/// # Rules
/// - A net relates to its tracks, zones, vias and parts, and reaches every
///   layer it runs on; a via counts for each layer of its span.
/// - A via reaches exactly the layers of its span.
/// - A track or zone relates to its net and reaches its own layer only as a
///   lit net; no layer dims, because it names one place, not a reach.
/// - A drill artifact reaches the layers of its span and relates to the vias
///   that make it.
///
/// # Dependencies
/// - crates/doc/src/net_members.rs (`Project::net_members` gives a net's members)
/// - crates/doc/src/lib.rs (`via_span_layers`, `is_bottom`)
///
/// # Do not
/// - Store the answer; ask again after an edit.
pub(crate) fn relations(project: &Project, selection: &Selection) -> Relations {
    let mut out = Relations::default();
    match selection {
        Selection::Net(id) => net_relations(project, *id, &mut out),
        Selection::Object(id) => {
            if let Some(v) = project.board.vias.iter().find(|v| v.id == *id) {
                out.net = (v.net != 0).then_some(v.net);
                out.layers = Some(
                    project
                        .via_span_layers(v)
                        .iter()
                        .map(|l| l.index())
                        .collect(),
                );
            } else if let Some(t) = project.board.tracks.iter().find(|t| t.id == *id) {
                out.net = Some(t.net);
            } else if let Some(z) = project.board.zones.iter().find(|z| z.id == *id) {
                out.net = Some(z.net);
            }
        }
        Selection::Artifact(rest) => {
            if let Some(d) = DrillKey::parse(rest) {
                out.layers = Some((d.from..=d.to).collect());
                for v in &project.board.vias {
                    if d.matches_via(project, v) {
                        out.objects.insert(v.id);
                    }
                }
            }
        }
        _ => {}
    }
    out
}

fn net_relations(project: &Project, id: Id, out: &mut Relations) {
    out.net = Some(id);
    let Some(m) = project.net_members(id) else {
        return;
    };
    let mut layers: BTreeSet<u8> = m.layers.iter().copied().collect();
    for lm in &m.by_layer {
        out.objects.extend(lm.tracks.iter().copied());
        out.objects.extend(lm.zones.iter().copied());
        out.objects.extend(lm.vias.iter().copied());
    }
    for v in &m.vias {
        out.objects.insert(v.id);
    }
    for p in &m.pads {
        out.objects.insert(p.footprint);
        if let Some(f) = project.footprint(p.footprint) {
            layers.insert(outer_index(project, f.side.unwrap_or(Layer::Front)));
        }
    }
    out.layers = Some(layers);
}

/// How strongly the row of an object lights for `selection`.
///
/// # Rules
/// - The selected object is strong; objects in its relations are weak.
pub(crate) fn object_level(selection: &Selection, rel: &Relations, id: Id) -> Level {
    if selection.object() == Some(id) {
        Level::Strong
    } else if rel.objects.contains(&id) {
        Level::Weak
    } else {
        Level::None
    }
}

/// True when a layer row at copper index `index` falls outside the layers the
/// selection reaches, so it should dim.
pub(crate) fn dims_layer(rel: &Relations, index: u8) -> bool {
    rel.layers.as_ref().is_some_and(|set| !set.contains(&index))
}

/// The key of a drill artifact: the span (1-based copper layers), then the
/// hole diameter in micrometres.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct DrillKey {
    /// Upper copper layer index of the span, 0 is the top.
    pub from: u8,
    /// Lower copper layer index of the span.
    pub to: u8,
    /// Hole diameter in micrometres.
    pub d_um: u64,
}

impl DrillKey {
    /// The drill artifact for a span given in 1-based layers and a diameter in mm.
    pub(crate) fn new(from_layer: u8, to_layer: u8, diameter_mm: f64) -> Self {
        Self {
            from: from_layer.saturating_sub(1),
            to: to_layer.saturating_sub(1),
            d_um: (diameter_mm * 1000.0).round().max(0.0) as u64,
        }
    }

    /// The text after `artifact:` in a tree key.
    pub(crate) fn text(self) -> String {
        format!("drill:{}-{}:{}", self.from + 1, self.to + 1, self.d_um)
    }

    /// Read the text written by [`DrillKey::text`].
    pub(crate) fn parse(text: &str) -> Option<Self> {
        let rest = text.strip_prefix("drill:")?;
        let (span, d) = rest.split_once(':')?;
        let (a, b) = span.split_once('-')?;
        let a: u8 = a.parse().ok()?;
        let b: u8 = b.parse().ok()?;
        Some(Self {
            from: a.checked_sub(1)?,
            to: b.checked_sub(1)?,
            d_um: d.parse().ok()?,
        })
    }

    /// True when this via is drilled into this artifact.
    pub(crate) fn matches_via(self, project: &Project, v: &pcbcraft_doc::Via) -> bool {
        let (a, b) = project.via_span(v);
        a.index() == self.from
            && b.index() == self.to
            && (v.drill * 1000.0).round().max(0.0) as u64 == self.d_um
    }
}

/// How strongly a drill table row lights for `selection`.
///
/// # Rules
/// - Strong when the artifact itself, or a via that is drilled into it, is selected.
/// - Weak when a copper layer inside its span, or a net with a via in it, is selected.
pub(crate) fn drill_level(
    project: &Project,
    selection: &Selection,
    rel: &Relations,
    row: DrillKey,
) -> Level {
    match selection {
        Selection::Artifact(rest) if DrillKey::parse(rest) == Some(row) => Level::Strong,
        Selection::Object(id)
            if project
                .board
                .vias
                .iter()
                .any(|v| v.id == *id && row.matches_via(project, v)) =>
        {
            Level::Strong
        }
        Selection::Layer(k) | Selection::Sublayer(k) => match Selection::layer_index(project, k) {
            Some(i) if (row.from..=row.to).contains(&i) => Level::Weak,
            _ => Level::None,
        },
        Selection::Net(_)
            if project
                .board
                .vias
                .iter()
                .any(|v| rel.objects.contains(&v.id) && row.matches_via(project, v)) =>
        {
            Level::Weak
        }
        _ => Level::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layer_visibility;

    fn sample() -> Project {
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = crate::App::create(shared, crate::Preferences::default(), None, None);
        app.tool_call("sample_design", serde_json::json!({}));
        app.project.clone()
    }

    fn six_layer() -> Project {
        let mut p = sample();
        p.board.layer_records.clear();
        for i in 1..=6 {
            p.board.layer_records.push(pcbcraft_doc::LayerRecord {
                name: format!("L{i}"),
                kind: "copper".into(),
                thickness: 0.035,
                ..Default::default()
            });
            if i < 6 {
                p.board.layer_records.push(pcbcraft_doc::LayerRecord {
                    name: format!("P{i}"),
                    kind: "prepreg".into(),
                    thickness: 0.2,
                    ..Default::default()
                });
            }
        }
        p
    }

    #[test]
    fn relations_of_a_net_include_its_pads_vias_and_tracks() {
        let mut p = sample();
        let net = p.nets[0].id;
        let first = p.board.footprints[0].id;
        p.board.footprints[0].pads[0].net = Some(net);
        let track = pcbcraft_doc::Track {
            id: 9_001,
            net,
            start: pcbcraft_geom::Point::new(1.0, 1.0),
            end: pcbcraft_geom::Point::new(5.0, 1.0),
            width: 0.25,
            layer: Layer::Front,
            arc: None,
            preserved: pcbcraft_doc::PreservedSource::default(),
        };
        p.board.tracks.push(track);
        let via = pcbcraft_doc::Via {
            id: 9_002,
            net,
            position: pcbcraft_geom::Point::new(5.0, 1.0),
            diameter: 0.6,
            drill: 0.3,
            preserved: pcbcraft_doc::PreservedSource::default(),
            from_layer: None,
            to_layer: None,
            covering: None,
            covering_bottom: None,
            backdrill: None,
        };
        p.board.vias.push(via);
        let rel = relations(&p, &Selection::Net(net));
        assert_eq!(rel.net, Some(net), "a net relates to itself");
        assert!(rel.objects.contains(&9_001), "its track lights weakly");
        assert!(rel.objects.contains(&9_002), "its via lights weakly");
        assert!(
            rel.objects.contains(&first),
            "the part holding its pad lights weakly"
        );
        assert_eq!(
            object_level(&Selection::Net(net), &rel, 9_001),
            Level::Weak,
            "a related track is weak, not strong"
        );
    }

    #[test]
    fn a_via_relates_to_exactly_the_layers_of_its_span() {
        let mut p = six_layer();
        let net = p.nets[0].id;
        let via = pcbcraft_doc::Via {
            id: 9_100,
            net,
            position: pcbcraft_geom::Point::new(5.0, 5.0),
            diameter: 0.6,
            drill: 0.3,
            from_layer: Layer::from_index(1).ok(),
            to_layer: Layer::from_index(4).ok(),
            preserved: pcbcraft_doc::PreservedSource::default(),
            covering: None,
            covering_bottom: None,
            backdrill: None,
        };
        p.board.vias.push(via);
        let rel = relations(&p, &Selection::Object(9_100));
        let layers: Vec<u8> = rel.layers.iter().flatten().copied().collect();
        assert_eq!(layers, vec![1, 2, 3, 4], "layers 2 to 5 are in the span");
        assert!(dims_layer(&rel, 0), "the top layer is outside and dims");
        assert!(dims_layer(&rel, 5), "the bottom layer is outside and dims");
        assert!(!dims_layer(&rel, 3), "a layer inside the span does not dim");
    }

    #[test]
    fn hiding_a_layer_hides_its_sublayers_but_not_the_reverse() {
        let mut hidden: Vec<String> = vec![];
        layer_visibility::set_group_stored(&mut hidden, "side:top", false);
        assert!(
            !layer_visibility::sub_effective(&hidden, "side:top", layer_visibility::SubLayer::Silk),
            "a hidden layer hides its silkscreen sublayer"
        );
        layer_visibility::set_group_stored(&mut hidden, "side:top", true);
        layer_visibility::set_sub_stored(
            &mut hidden,
            "side:top",
            layer_visibility::SubLayer::Silk,
            false,
        );
        assert!(
            layer_visibility::group_stored(&hidden, "side:top"),
            "hiding one sublayer leaves its layer shown"
        );
        layer_visibility::set_sub_stored(
            &mut hidden,
            "side:top",
            layer_visibility::SubLayer::Silk,
            true,
        );
        layer_visibility::set_group_stored(&mut hidden, "side:top", false);
        layer_visibility::set_sub_stored(
            &mut hidden,
            "side:top",
            layer_visibility::SubLayer::Silk,
            true,
        );
        assert!(
            !layer_visibility::group_stored(&hidden, "side:top"),
            "showing one sublayer again does not show its layer"
        );
    }

    #[test]
    fn a_tree_key_becomes_the_matching_selection() {
        assert_eq!(
            Selection::from_key("side:top"),
            Selection::Layer("side:top".into())
        );
        assert_eq!(
            Selection::from_key("side:top/silk"),
            Selection::Sublayer("side:top/silk".into())
        );
        assert_eq!(
            Selection::from_key("side:top/copper/net:7"),
            Selection::Net(7)
        );
        assert_eq!(Selection::from_key("nets/net:3"), Selection::Net(3));
        assert_eq!(Selection::from_key("edge"), Selection::Row("edge".into()));
        assert_eq!(
            Selection::from_legacy(Some(4), Some("edge")),
            Selection::Object(4),
            "an object wins over a row"
        );
    }

    #[test]
    fn a_drill_artifact_key_reads_back_as_written() {
        let key = DrillKey::new(1, 4, 0.3);
        assert_eq!(
            DrillKey::parse(&key.text()),
            Some(key),
            "the key round trips"
        );
        assert_eq!(DrillKey::parse("drill:x"), None, "junk is refused");
    }
}
