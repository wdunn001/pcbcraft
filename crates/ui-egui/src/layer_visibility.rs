//! Board layer and sub-layer visibility in view settings (`Preferences::hidden_layers`).
//!
//! Each physical layer group (`side:top`, `side:bottom`, `inner:n`) has one stored
//! flag. Each sub-layer row (tracks, pads, zones, silk, text) has its own flag.
//! The canvas draws a sub-layer only when both the group and the sub-layer are on.

use pcbcraft_doc::Layer;

/// A toggled slice of one layer group.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SubLayer {
    Tracks,
    Pads,
    Zones,
    Silk,
    Text,
}

impl SubLayer {
    pub(crate) fn storage_key(self, side_path: &str) -> String {
        format!("sub:{side_path}:{}", self.segment())
    }

    fn segment(self) -> &'static str {
        match self {
            Self::Tracks => "tracks",
            Self::Pads => "pads",
            Self::Zones => "zones",
            Self::Silk => "silk",
            Self::Text => "text",
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Tracks => "Tracks",
            Self::Pads => "Pads",
            Self::Zones => "Zones",
            Self::Silk => "Package artwork",
            Self::Text => "Text",
        }
    }
}

pub(crate) fn group_storage_key(side_path: &str) -> String {
    format!("group:{side_path}")
}

pub(crate) fn group_stored(hidden: &[String], side_path: &str) -> bool {
    !hidden.iter().any(|h| h == &group_storage_key(side_path))
}

pub(crate) fn sub_stored(hidden: &[String], side_path: &str, sub: SubLayer) -> bool {
    !hidden.iter().any(|h| h == &sub.storage_key(side_path))
}

pub(crate) fn sub_effective(hidden: &[String], side_path: &str, sub: SubLayer) -> bool {
    group_stored(hidden, side_path) && sub_stored(hidden, side_path, sub)
}

/// Map a copper layer to its tree path key (`side:top`, `side:bottom`, `inner:n`).
pub(crate) fn side_path_for_layer(board_coppers: usize, layer: Layer) -> String {
    if layer == Layer::Front {
        "side:top".into()
    } else if layer.index() + 1 >= board_coppers.max(2) {
        "side:bottom".into()
    } else {
        format!("inner:{}", layer.index())
    }
}

/// Legacy `hidden_layers` names from before layer groups; map once when toggling.
pub(crate) fn legacy_group_for_name(name: &str) -> Option<&'static str> {
    match name {
        "Top copper" => Some("side:top"),
        "Bottom copper" => Some("side:bottom"),
        _ => None,
    }
}

/// Read a legacy canvas name as a sub-layer flag on both outers when it was global.
pub(crate) fn legacy_global_sub(name: &str) -> Option<SubLayer> {
    match name {
        "Pads" => Some(SubLayer::Pads),
        "Copper zones" => Some(SubLayer::Zones),
        "Package artwork" => Some(SubLayer::Silk),
        _ => None,
    }
}

pub(crate) fn hidden_contains_legacy(hidden: &[String], name: &str) -> bool {
    if hidden.iter().any(|h| h == name) {
        return true;
    }
    if let Some(side) = legacy_group_for_name(name) {
        return !group_stored(hidden, side);
    }
    if let Some(sub) = legacy_global_sub(name) {
        return !sub_stored(hidden, "side:top", sub) || !sub_stored(hidden, "side:bottom", sub);
    }
    false
}

pub(crate) fn set_group_stored(hidden: &mut Vec<String>, side_path: &str, on: bool) {
    let key = group_storage_key(side_path);
    hidden.retain(|h| h != &key);
    hidden.retain(|h| h != legacy_name_for_group(side_path));
    if !on {
        hidden.push(key);
    }
}

pub(crate) fn set_sub_stored(hidden: &mut Vec<String>, side_path: &str, sub: SubLayer, on: bool) {
    let key = sub.storage_key(side_path);
    hidden.retain(|h| h != &key);
    if !on {
        hidden.push(key);
    }
}

fn legacy_name_for_group(side_path: &str) -> &'static str {
    match side_path {
        "side:top" => "Top copper",
        "side:bottom" => "Bottom copper",
        _ => "",
    }
}

pub(crate) fn set_legacy_canvas(hidden: &mut Vec<String>, name: &str, on: bool) {
    if let Some(side) = legacy_group_for_name(name) {
        set_group_stored(hidden, side, on);
        return;
    }
    if let Some(sub) = legacy_global_sub(name) {
        for side in ["side:top", "side:bottom"] {
            set_sub_stored(hidden, side, sub, on);
        }
        return;
    }
    hidden.retain(|h| h != name);
    if !on {
        hidden.push(name.to_owned());
    }
}

/// All layout layer-group paths for hide-others.
pub(crate) fn layout_layer_groups() -> &'static [&'static str] {
    &["side:top", "side:bottom"]
}
