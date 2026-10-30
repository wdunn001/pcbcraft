//! Turn a SolidCraft design into a [`crate::model_load::LoadedModel`] for footprint 3D.
//!
//! Context: issue #733. Geometry comes from embedded meshes, STEP text on import
//! features, or the container display cache when the design is parametric and warm-saved.
//! Model space is millimetres, Z up, matching the board mesh.
//!
//! # Rules
//! - Without a part name, meshes on the root component merge into one model.
//! - With a part name, only that component's bodies are kept.
//! - Body colours come from `appearances.bodies` keyed by mesh name.
//! - PCBCraft footprint exports in `org.pcbcraft` metadata use the pad-plane frame;
//!   callers keep identity placement on the footprint.
//!
//! # Dependencies
//! - `crate::solidcraft_format`, `crate::model_load`, `crate::step_read`
//!
//! # Do not
//! - Do not link SolidCraft crates; only the documented subset is read.

use crate::model_load::{
    LoadedModel, MAX_MODEL_FACES, MAX_MODEL_VERTICES, ModelError,
};
use crate::solidcraft_format::{read_solidcraft, SolidcraftDesign};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// Metadata key PCBCraft writes in exported SolidCraft files.
pub const PCBCraft_METADATA_KEY: &str = "org.pcbcraft";
const DISPLAY_PREVIEW_JSON: &str = "cache/preview.json";
const DISPLAY_PREVIEW_BIN: &str = "cache/preview.bin";

/// Load triangles from a `.solidcraft` file for the 3D model reader.
pub fn load_solidcraft_model(
    bytes: &[u8],
    cad_part: Option<&str>,
) -> Result<LoadedModel, ModelError> {
    let design = read_solidcraft(bytes).map_err(ModelError)?;
    design_to_model(&design, cad_part, bytes)
}

/// Cache token from file bytes and an optional in-file part name.
pub fn solidcraft_cache_token(bytes: &[u8], cad_part: Option<&str>) -> String {
    let hash = content_sha256_hex(bytes);
    match cad_part.filter(|p| !p.is_empty()) {
        Some(part) => format!("{hash}::{part}"),
        None => hash,
    }
}

/// True when metadata marks a PCBCraft footprint-body export (pad-plane frame).
pub fn solidcraft_is_footprint_export(design: &SolidcraftDesign) -> bool {
    design
        .metadata
        .get(PCBCraft_METADATA_KEY)
        .and_then(Value::as_object)
        .and_then(|m| m.get("frame"))
        .and_then(Value::as_str)
        .is_some_and(|f| f == "footprint_pad_plane")
}

pub fn content_sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn design_to_model(
    design: &SolidcraftDesign,
    cad_part: Option<&str>,
    raw_bytes: &[u8],
) -> Result<LoadedModel, ModelError> {
    let component_id = resolve_component(design, cad_part)?;
    let mut out = MeshOut::default();
    for feature in &design.features {
        if !feature_matches_component(feature, component_id) {
            continue;
        }
        push_feature_geometry(feature, &design.appearances, &mut out)?;
    }
    if out.positions.is_empty() && raw_bytes.starts_with(b"PK\x03\x04") {
        try_display_preview(raw_bytes, component_id, cad_part, &mut out)?;
    }
    if out.positions.is_empty() {
        return err(
            "This SolidCraft part has no display geometry yet. Save the part with the display preview on.",
        );
    }
    let color = out.body_colors.first().copied();
    Ok(LoadedModel {
        positions: out.positions,
        indices: out.indices,
        color,
        body_colors: if out.body_colors.len() > 1 {
            Some(out.body_colors)
        } else {
            None
        },
    })
}

fn resolve_component(design: &SolidcraftDesign, cad_part: Option<&str>) -> Result<u64, ModelError> {
    if let Some(name) = cad_part.filter(|n| !n.is_empty()) {
        design
            .components
            .iter()
            .find(|c| c.name == name)
            .map(|c| c.id)
            .ok_or_else(|| {
                ModelError(format!("The SolidCraft file has no component named {name}."))
            })
    } else {
        Ok(root_component_id(design))
    }
}

fn root_component_id(design: &SolidcraftDesign) -> u64 {
    design
        .components
        .iter()
        .find(|c| c.parent == c.id || (c.parent == 0 && c.id == 0))
        .map(|c| c.id)
        .or_else(|| design.components.first().map(|c| c.id))
        .unwrap_or(0)
}

fn feature_matches_component(feature: &Value, component_id: u64) -> bool {
    match feature.get("component").and_then(Value::as_u64) {
        Some(id) => id == component_id,
        None => component_id == 0,
    }
}

#[derive(Default)]
struct MeshOut {
    positions: Vec<[f64; 3]>,
    indices: Vec<u32>,
    body_colors: Vec<[f32; 3]>,
}

fn push_feature_geometry(
    feature: &Value,
    appearances: &crate::solidcraft_format::SolidcraftAppearances,
    out: &mut MeshOut,
) -> Result<(), ModelError> {
    let kind = feature.get("type").and_then(Value::as_str).unwrap_or("");
    match kind {
        "mesh_import" => push_mesh_import(feature, appearances, out),
        "import" => push_step_import(feature, appearances, out),
        _ => Ok(()),
    }
}

fn push_mesh_import(
    feature: &Value,
    appearances: &crate::solidcraft_format::SolidcraftAppearances,
    out: &mut MeshOut,
) -> Result<(), ModelError> {
    let Some(meshes) = feature.get("meshes").and_then(Value::as_array) else {
        return Ok(());
    };
    for mesh in meshes {
        let name = mesh.get("name").and_then(Value::as_str);
        let color = name.and_then(|n| appearance_from_map(appearances, n));
        push_one_mesh(mesh, out, color)?;
    }
    Ok(())
}

fn push_step_import(
    feature: &Value,
    appearances: &crate::solidcraft_format::SolidcraftAppearances,
    out: &mut MeshOut,
) -> Result<(), ModelError> {
    let Some(text) = feature.get("step").and_then(Value::as_str) else {
        return Ok(());
    };
    if text.trim().is_empty() {
        return Ok(());
    }
    let loaded = crate::step_read::read_step(text.as_bytes())?;
    let name = feature.get("name").and_then(Value::as_str);
    let color = name.and_then(|n| appearance_from_map(appearances, n));
    merge_loaded(&loaded, out, color)?;
    Ok(())
}

fn appearance_from_map(
    appearances: &crate::solidcraft_format::SolidcraftAppearances,
    body_name: &str,
) -> Option<[f32; 3]> {
    appearances
        .bodies
        .get(body_name)
        .and_then(|v| v.get("color"))
        .and_then(parse_color)
}

fn push_one_mesh(
    mesh: &Value,
    out: &mut MeshOut,
    fallback_color: Option<[f32; 3]>,
) -> Result<(), ModelError> {
    let name = mesh.get("name").and_then(Value::as_str);
    let color = fallback_color.or_else(|| appearance_color(mesh, name));
    let positions = mesh
        .get("positions")
        .and_then(Value::as_array)
        .ok_or_else(|| ModelError("A mesh is missing its positions.".into()))?;
    let triangles = mesh
        .get("triangles")
        .and_then(Value::as_array)
        .ok_or_else(|| ModelError("A mesh is missing its triangles.".into()))?;
    if positions.is_empty() || triangles.is_empty() {
        return Ok(());
    }
    let base = out.positions.len() as u32;
    let before = out.positions.len();
    for chunk in positions.as_chunks::<3>().0 {
        let p = [
            chunk[0].as_f64().ok_or_else(|| ModelError("A mesh position is not a number.".into()))?,
            chunk[1].as_f64().ok_or_else(|| ModelError("A mesh position is not a number.".into()))?,
            chunk[2].as_f64().ok_or_else(|| ModelError("A mesh position is not a number.".into()))?,
        ];
        if !p.iter().all(|v| v.is_finite()) {
            return err("A mesh position is not finite.");
        }
        if out.positions.len() >= MAX_MODEL_VERTICES {
            return err(format!(
                "The model has more than {MAX_MODEL_VERTICES} vertices, over the limit."
            ));
        }
        out.positions.push(p);
    }
    let vert_count = out.positions.len() - before;
    for tri in triangles.as_chunks::<3>().0 {
        let mut idx = [0u32; 3];
        for (i, slot) in idx.iter_mut().enumerate() {
            let n = tri[i]
                .as_u64()
                .ok_or_else(|| ModelError("A mesh index is not an integer.".into()))?;
            *slot = u32::try_from(n)
                .map_err(|_| ModelError("A mesh index is too large.".into()))?;
            if *slot as usize >= vert_count {
                return err("The model has a face that points past its vertices.");
            }
        }
        if out.indices.len() / 3 + 1 > MAX_MODEL_FACES {
            return err(format!(
                "The model has more than {MAX_MODEL_FACES} faces, over the limit."
            ));
        }
        out.indices.extend_from_slice(&[base + idx[0], base + idx[1], base + idx[2]]);
    }
    if let Some(c) = color {
        out.body_colors.push(c);
    }
    Ok(())
}

fn merge_loaded(
    loaded: &LoadedModel,
    out: &mut MeshOut,
    color: Option<[f32; 3]>,
) -> Result<(), ModelError> {
    if loaded.positions.len() + out.positions.len() > MAX_MODEL_VERTICES {
        return err(format!(
            "The model has more than {MAX_MODEL_VERTICES} vertices, over the limit."
        ));
    }
    if loaded.indices.len() / 3 + out.indices.len() / 3 > MAX_MODEL_FACES {
        return err(format!(
            "The model has more than {MAX_MODEL_FACES} faces, over the limit."
        ));
    }
    let base = out.positions.len() as u32;
    out.positions.extend_from_slice(&loaded.positions);
    out.indices
        .extend(loaded.indices.iter().map(|i| base + i));
    let c = color.or(loaded.color);
    if let Some(c) = c {
        out.body_colors.push(c);
    }
    Ok(())
}

fn appearance_color(value: &Value, body_name: Option<&str>) -> Option<[f32; 3]> {
    if let Some(c) = value.get("color").and_then(parse_color) {
        return Some(c);
    }
    let _ = body_name?;
    None
}

fn parse_color(value: &Value) -> Option<[f32; 3]> {
    let arr = value.as_array()?;
    if arr.len() != 3 {
        return None;
    }
    let to_f = |v: &Value| {
        v.as_f64()
            .map(|n| {
                if n > 1.0 {
                    (n / 255.0) as f32
                } else {
                    n as f32
                }
            })
            .filter(|f| f.is_finite())
    };
    Some([to_f(&arr[0])?, to_f(&arr[1])?, to_f(&arr[2])?])
}

fn try_display_preview(
    bytes: &[u8],
    component_id: u64,
    cad_part: Option<&str>,
    out: &mut MeshOut,
) -> Result<(), ModelError> {
    let entries = read_preview_zip(bytes).map_err(ModelError)?;
    let Some(meta) = entries.get(DISPLAY_PREVIEW_JSON) else {
        return Ok(());
    };
    let meta: Value =
        serde_json::from_slice(meta).map_err(|_| ModelError("The display preview metadata is invalid.".into()))?;
    let Some(bodies) = meta.get("bodies").and_then(Value::as_array) else {
        return Ok(());
    };
    let bin = entries.get(DISPLAY_PREVIEW_BIN).map(Vec::as_slice).unwrap_or(&[]);
    for body in bodies {
        let comp = body.get("component").and_then(Value::as_u64);
        if comp.is_some_and(|id| id != component_id) {
            continue;
        }
        if let Some(name) = cad_part.filter(|n| !n.is_empty()) {
            let body_name = body.get("name").and_then(Value::as_str).unwrap_or("");
            if body_name != name && body.get("component_name").and_then(Value::as_str) != Some(name)
            {
                continue;
            }
        }
        if let Some(offset) = body.get("mesh_offset").and_then(Value::as_u64) {
            let len = body.get("mesh_length").and_then(Value::as_u64).unwrap_or(0) as usize;
            let start = offset as usize;
            let end = start.saturating_add(len);
            let slice = bin.get(start..end).unwrap_or(&[]);
            if let Ok((positions, triangles)) = decode_preview_mesh(slice) {
                let mesh = json_mesh(&positions, &triangles, body.get("name").and_then(Value::as_str));
                push_one_mesh(&mesh, out, appearance_color(body, body.get("name").and_then(Value::as_str)))?;
            }
        }
    }
    Ok(())
}

fn json_mesh(positions: &[f64], triangles: &[u32], name: Option<&str>) -> Value {
    serde_json::json!({
        "name": name.unwrap_or("Body"),
        "positions": positions,
        "triangles": triangles,
    })
}

fn decode_preview_mesh(bytes: &[u8]) -> Result<(Vec<f64>, Vec<u32>), String> {
    const MAGIC: &[u8; 8] = b"SCMESH01";
    if bytes.get(..8) != Some(MAGIC.as_slice()) {
        return Err("unsupported preview mesh".into());
    }
    let positions_len = u64::from_le_bytes(bytes[8..16].try_into().map_err(|_| "truncated")?) as usize;
    let triangles_len = u64::from_le_bytes(bytes[16..24].try_into().map_err(|_| "truncated")?) as usize;
    let mut at = 24;
    let mut positions = Vec::with_capacity(positions_len);
    for _ in 0..positions_len {
        let v = f64::from_le_bytes(bytes[at..at + 8].try_into().map_err(|_| "truncated")?);
        at += 8;
        positions.push(v);
    }
    let mut triangles = Vec::with_capacity(triangles_len);
    for _ in 0..triangles_len {
        let v = u32::from_le_bytes(bytes[at..at + 4].try_into().map_err(|_| "truncated")?);
        at += 4;
        triangles.push(v);
    }
    Ok((positions, triangles))
}

fn read_preview_zip(bytes: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, String> {
    if !bytes.starts_with(b"PK\x03\x04") {
        return Err("not a container".into());
    }
    crate::solidcraft_format::read_container_entries(bytes)
}

fn err<T>(s: impl Into<String>) -> Result<T, ModelError> {
    Err(ModelError(s.into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use crate::solidcraft_format::{
        SolidcraftAppearances, SolidcraftComponent, SolidcraftDesign, SUPPORTED_FORMAT,
        write_solidcraft_json,
    };
    use serde_json::json;

    type Check = Result<(), String>;

    fn two_body_part() -> SolidcraftDesign {
        SolidcraftDesign {
            format: format!("solidcraft/{SUPPORTED_FORMAT}"),
            name: "cap".into(),
            units: "mm".into(),
            metadata: BTreeMap::new(),
            params: vec![],
            features: vec![
                json!({
                    "id": 1,
                    "name": "Shell",
                    "type": "mesh_import",
                    "file": "test",
                    "meshes": [{
                        "name": "Shell",
                        "positions": [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
                        "triangles": [0, 1, 2]
                    }]
                }),
                json!({
                    "id": 2,
                    "name": "Mark",
                    "type": "mesh_import",
                    "file": "test",
                    "meshes": [{
                        "name": "Mark",
                        "positions": [0.0, 0.0, 1.0, 1.0, 0.0, 1.0, 0.0, 1.0, 1.0],
                        "triangles": [0, 1, 2]
                    }]
                }),
            ],
            next_id: 2,
            components: vec![SolidcraftComponent {
                id: 0,
                name: "Root".into(),
                parent: 0,
                extra: BTreeMap::new(),
            }],
            occurrences: vec![],
            user_materials: vec![],
            materials: BTreeMap::new(),
            appearances: SolidcraftAppearances {
                bodies: BTreeMap::from([
                    (
                        "Shell".into(),
                        json!({"name": "A", "color": [255, 0, 0], "opacity": 1.0}),
                    ),
                    (
                        "Mark".into(),
                        json!({"name": "B", "color": [0, 0, 255], "opacity": 1.0}),
                    ),
                ]),
                extra: BTreeMap::new(),
            },
            extra: BTreeMap::new(),
        }
    }

    #[test]
    fn solidcraft_part_loads_as_a_model() -> Check {
        let design = two_body_part();
        let text = write_solidcraft_json(&design)?;
        let m = load_solidcraft_model(text.as_bytes(), None).map_err(|e| e.to_string())?;
        assert_eq!(m.indices.len(), 6, "two triangles merged");
        assert_eq!(m.positions.len(), 6, "six vertices");
        let colors = m.body_colors.ok_or("expected per-body colours")?;
        assert_eq!(colors.len(), 2);
        assert!((colors[0][0] - 1.0).abs() < 1e-3);
        assert!((colors[1][2] - 1.0).abs() < 1e-3);
        Ok(())
    }

    #[test]
    fn solidcraft_part_name_selects_a_component() -> Check {
        let mut design = two_body_part();
        design.components.push(SolidcraftComponent {
            id: 5,
            name: "Child".into(),
            parent: 0,
            extra: BTreeMap::new(),
        });
        design.features[1]["component"] = json!(5);
        let text = write_solidcraft_json(&design)?;
        let m = load_solidcraft_model(text.as_bytes(), Some("Child"))
            .map_err(|e| e.to_string())?;
        assert_eq!(m.indices.len(), 3, "one triangle on the child");
        assert_eq!(m.positions.len(), 3);
        Ok(())
    }

    #[test]
    fn reimport_keeps_alignment() -> Check {
        let design = two_body_part();
        let v1 = write_solidcraft_json(&design)?;
        let key1 = solidcraft_cache_token(v1.as_bytes(), None);
        let model = pcbcraft_library::Model3D {
            path: "part.solidcraft".into(),
            offset: [2.0, 0.0, 0.0],
            scale: [1.0, 1.0, 1.0],
            rotate: [0.0, 0.0, 45.0],
            cad_part: None,
            raw_source: String::new(),
        };
        let mut design2 = design;
        design2.features[0]["meshes"][0]["positions"][0] = json!(0.5);
        let v2 = write_solidcraft_json(&design2)?;
        let key2 = solidcraft_cache_token(v2.as_bytes(), None);
        assert_ne!(key1, key2, "content hash changes when the file changes");
        assert_eq!(model.offset, [2.0, 0.0, 0.0], "alignment stays on the footprint");
        assert_eq!(model.rotate[2], 45.0);
        let _ = load_solidcraft_model(v2.as_bytes(), None).map_err(|e| e.to_string())?;
        assert_eq!(model.offset, [2.0, 0.0, 0.0], "reload does not move the footprint transform");
        Ok(())
    }
}
