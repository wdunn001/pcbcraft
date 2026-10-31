//! Stackup-aware copper layer names in saved project JSON.
//!
//! [`Layer`](pcbcraft_doc::Layer) serializes with index-derived names such as
//! `In3.Cu` for the bottom of a four-layer board. On save we rewrite those
//! strings to the names recorded in the stackup; on load we resolve every
//! stored name back to a layer index so legacy files still open.

use pcbcraft_doc::{Board, Project};
use serde_json::Value;

const COPPER_LAYER_KEYS: &[&str] = &["layer", "from_layer", "to_layer", "side"];

/// Rewrite copper layer strings in project JSON to stackup names before writing.
pub fn write_stackup_copper_names(root: &mut Value, project: &Project) {
    rewrite_copper_strings(root, project);
}

/// After JSON deserialization, fix copper indices using the strings in the file.
pub fn read_stackup_copper_names(project: &mut Project, root: &Value) -> Result<(), String> {
    let board = root
        .get("board")
        .ok_or_else(|| "project file missing board".to_string())?;
    read_board_copper_names(&mut project.board, project, board)
}

fn rewrite_copper_strings(value: &mut Value, project: &Project) {
    match value {
        Value::Object(map) => {
            for (key, child) in map.iter_mut() {
                if COPPER_LAYER_KEYS.contains(&key.as_str()) {
                    if let Value::String(name) = child {
                        if let Ok(layer) = project.resolve_copper_layer(name) {
                            *name = project.layer_name(layer);
                        }
                    }
                } else {
                    rewrite_copper_strings(child, project);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                rewrite_copper_strings(item, project);
            }
        }
        _ => {}
    }
}

fn read_board_copper_names(board: &mut Board, project: &Project, value: &Value) -> Result<(), String> {
    if let Some(items) = value.get("tracks").and_then(Value::as_array) {
        for (track, raw) in board.tracks.iter_mut().zip(items.iter()) {
            track.layer = required_layer(project, raw.get("layer"))?;
        }
    }
    if let Some(items) = value.get("zones").and_then(Value::as_array) {
        for (zone, raw) in board.zones.iter_mut().zip(items.iter()) {
            zone.layer = required_layer(project, raw.get("layer"))?;
        }
    }
    if let Some(items) = value.get("texts").and_then(Value::as_array) {
        for (text, raw) in board.texts.iter_mut().zip(items.iter()) {
            text.layer = required_layer(project, raw.get("layer"))?;
        }
    }
    if let Some(items) = value.get("fiducials").and_then(Value::as_array) {
        for (fid, raw) in board.fiducials.iter_mut().zip(items.iter()) {
            fid.layer = required_layer(project, raw.get("layer"))?;
        }
    }
    if let Some(items) = value.get("rule_areas").and_then(Value::as_array) {
        for (area, raw) in board.rule_areas.iter_mut().zip(items.iter()) {
            area.layer = optional_layer(project, raw.get("layer"))?;
        }
    }
    if let Some(items) = value.get("vias").and_then(Value::as_array) {
        for (via, raw) in board.vias.iter_mut().zip(items.iter()) {
            via.from_layer = optional_layer(project, raw.get("from_layer"))?;
            via.to_layer = optional_layer(project, raw.get("to_layer"))?;
        }
    }
    if let Some(items) = value.get("footprints").and_then(Value::as_array) {
        for (footprint, raw) in board.footprints.iter_mut().zip(items.iter()) {
            footprint.side = optional_layer(project, raw.get("side"))?;
        }
    }
    Ok(())
}

fn required_layer(project: &Project, value: Option<&Value>) -> Result<pcbcraft_doc::Layer, String> {
    let name = value
        .and_then(Value::as_str)
        .ok_or_else(|| "copper layer field is missing".to_string())?;
    project.resolve_copper_layer(name)
}

fn optional_layer(
    project: &Project,
    value: Option<&Value>,
) -> Result<Option<pcbcraft_doc::Layer>, String> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(name)) => project.resolve_copper_layer(name).map(Some),
        _ => Err("copper layer field must be a string".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pcbcraft_doc::{Layer, LayerRecord, Net, Point, Project, Track};
    fn four_layer() -> Result<Project, String> {
        let mut p = Project::default();
        p.nets.push(Net {
            id: 1,
            name: "SIG".into(),
            ..Default::default()
        });
        p.board.layer_records = vec![
            LayerRecord {
                name: "F.Cu".into(),
                kind: "copper".into(),
                thickness: 0.035,
                ..Default::default()
            },
            LayerRecord {
                name: "In1.Cu".into(),
                kind: "copper".into(),
                thickness: 0.0175,
                ..Default::default()
            },
            LayerRecord {
                name: "In2.Cu".into(),
                kind: "copper".into(),
                thickness: 0.0175,
                ..Default::default()
            },
            LayerRecord {
                name: "B.Cu".into(),
                kind: "copper".into(),
                thickness: 0.035,
                ..Default::default()
            },
        ];
        p.board.tracks.push(Track {
            id: 1,
            net: 1,
            start: Point::default(),
            end: Point::new(1.0, 0.0),
            width: 0.2,
            layer: Layer::from_index(3)?,
            arc: None,
        });
        Ok(p)
    }

    #[test]
    fn save_rewrites_bottom_copper_to_stackup_name() -> Result<(), String> {
        let project = four_layer()?;
        let mut value = serde_json::to_value(&project).map_err(|e| e.to_string())?;
        assert!(
            value["board"]["tracks"][0]["layer"] == "In3.Cu",
            "serde still writes index-derived inner names"
        );
        write_stackup_copper_names(&mut value, &project);
        assert_eq!(value["board"]["tracks"][0]["layer"], "B.Cu");
        Ok(())
    }

    #[test]
    fn load_keeps_legacy_index_derived_bottom_name() -> Result<(), String> {
        let project = four_layer()?;
        let root: Value = serde_json::to_value(&project).map_err(|e| e.to_string())?;
        let mut loaded: Project =
            serde_json::from_value(root.clone()).map_err(|e| e.to_string())?;
        assert_eq!(loaded.board.tracks[0].layer.index(), 3);
        read_stackup_copper_names(&mut loaded, &root)?;
        assert_eq!(loaded.board.tracks[0].layer.index(), 3);
        Ok(())
    }

    #[test]
    fn load_maps_bottom_name_on_multilayer_board() -> Result<(), String> {
        let project = four_layer()?;
        let text = serde_json::to_string(&project).map_err(|e| e.to_string())?;
        let text = text.replace("\"In3.Cu\"", "\"B.Cu\"");
        let root: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        let mut loaded: Project =
            serde_json::from_value(root.clone()).map_err(|e| e.to_string())?;
        assert_eq!(
            loaded.board.tracks[0].layer.index(),
            1,
            "plain Layer deserialize treats B.Cu as index 1"
        );
        read_stackup_copper_names(&mut loaded, &root)?;
        assert_eq!(loaded.board.tracks[0].layer.index(), 3);
        Ok(())
    }
}
