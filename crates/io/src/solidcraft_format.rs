//! SolidCraft design file subset: plain JSON and stored ZIP containers.
//!
//! Context: sibling CAD interchange for board and part 3D (issue #729). This module
//! reads and writes the fields PCBCraft needs (embedded meshes, components,
//! occurrences, appearances, materials, metadata) without linking SolidCraft crates.
//! Model space is millimetres, Z up, right-handed; the same frame as the board mesh.
//!
//! # Rules
//! - Writers declare file format [`SUPPORTED_FORMAT`] (`solidcraft/22`).
//! - Unknown JSON keys are kept in side maps so a read then write does not drop them.
//! - Container blobs live at `blobs/{sha256}.{ext}`; mesh payloads use the `SCMESH01`
//! layout (magic, two u64 counts, little-endian f64 positions, u32 indices).
//! - Container reads enforce at most [`MAX_CONTAINER_ENTRIES`] entries and
//! [`MAX_CONTAINER_EXPANDED`] bytes of expanded payload, refuse duplicate or unsafe
//! entry names, and verify each blob hash.
//!
//! # Dependencies
//! - `crate::zip_entries`, `crate::crc32`
//!
//! # Do not
//! - Do not use `unwrap`, `expect`, or `panic!` in production paths.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// The newest SolidCraft file format number this reader has been tested against.
pub const SUPPORTED_FORMAT: u32 = 22;
/// Container stamp written beside `document.json`.
pub const CONTAINER_VERSION: &str = "solidcraft-container/1";
const MESH_MAGIC: &[u8; 8] = b"SCMESH01";
const MESH_BLOB_FIELD: &str = "mesh_blob";
const DOCUMENT_ENTRY: &str = "document.json";
const CONTAINER_ENTRY: &str = "container.json";
/// Most ZIP entries a container may list.
pub const MAX_CONTAINER_ENTRIES: usize = 10_000;
/// Most bytes all stored payloads may expand to together.
pub const MAX_CONTAINER_EXPANDED: usize = 512 * 1024 * 1024;

/// One component in the design tree.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SolidcraftComponent {
    pub id: u64,
    pub name: String,
    #[serde(default)]
    pub parent: u64,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// One rigid placement of a component.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SolidcraftOccurrence {
    pub id: u64,
    pub component: u64,
    #[serde(default)]
    pub parent: u64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transform: Option<Value>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub grounded: bool,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

fn is_false(v: &bool) -> bool {
    !*v
}

/// Body appearance records keyed by body name.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SolidcraftAppearances {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub bodies: BTreeMap<String, Value>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl SolidcraftAppearances {
    fn is_empty(&self) -> bool {
        self.bodies.is_empty() && self.extra.is_empty()
    }
}

/// The interchange subset held in memory with meshes fully inline.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SolidcraftDesign {
    pub format: String,
    pub name: String,
    pub units: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub features: Vec<Value>,
    #[serde(default)]
    pub next_id: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub components: Vec<SolidcraftComponent>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub occurrences: Vec<SolidcraftOccurrence>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub user_materials: Vec<Value>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub materials: BTreeMap<String, Value>,
    #[serde(default, skip_serializing_if = "SolidcraftAppearances::is_empty")]
    pub appearances: SolidcraftAppearances,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// Read plain JSON or a container from bytes.
pub fn read_solidcraft(bytes: &[u8]) -> Result<SolidcraftDesign, String> {
    if bytes.starts_with(b"PK\x03\x04") {
        read_solidcraft_container(bytes)
    } else {
        let text = std::str::from_utf8(bytes).map_err(|_| "design file is not UTF-8 JSON".to_string())?;
        read_solidcraft_json(text)
    }
}

/// Read a plain JSON design.
pub fn read_solidcraft_json(text: &str) -> Result<SolidcraftDesign, String> {
    let design: SolidcraftDesign = serde_json::from_str(text).map_err(|e| format!("design JSON is invalid: {e}"))?;
    validate_design(&design)?;
    Ok(design)
}

/// Write a plain JSON design.
pub fn write_solidcraft_json(design: &SolidcraftDesign) -> Result<String, String> {
    validate_design(design)?;
    let mut ready = design.clone();
    ready.format = format_string(SUPPORTED_FORMAT);
    ready.units = "mm".to_string();
    strip_blob_fields(&mut ready);
    serde_json::to_string_pretty(&ready).map_err(|e| e.to_string())
}

/// Read a stored ZIP container.
pub fn read_solidcraft_container(bytes: &[u8]) -> Result<SolidcraftDesign, String> {
    let entries = read_stored_zip(bytes)?;
    let container = entries
        .get(CONTAINER_ENTRY)
        .ok_or_else(|| "container holds no container.json".to_string())?;
    let container: Value =
        serde_json::from_slice(container).map_err(|_| "container.json is invalid".to_string())?;
    let stamp = container
        .get("format")
        .and_then(Value::as_str)
        .ok_or_else(|| "container.json names no format".to_string())?;
    if stamp != CONTAINER_VERSION {
        return Err("unsupported container version".to_string());
    }
    let document = entries
        .get(DOCUMENT_ENTRY)
        .ok_or_else(|| "container holds no document.json".to_string())?;
    let mut design: SolidcraftDesign =
        serde_json::from_slice(document).map_err(|e| format!("document.json is invalid: {e}"))?;
    hydrate_meshes(&mut design, &entries)?;
    validate_design(&design)?;
    Ok(design)
}

/// Write a stored ZIP container with deduplicated hash-named blobs.
pub fn write_solidcraft_container(design: &SolidcraftDesign) -> Result<Vec<u8>, String> {
    validate_design(design)?;
    let mut doc = design.clone();
    doc.format = format_string(SUPPORTED_FORMAT);
    doc.units = "mm".to_string();
    let mut blobs: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    externalize_meshes(&mut doc, &mut blobs)?;
    let document = serde_json::to_vec_pretty(&doc).map_err(|e| e.to_string())?;
    let container = serde_json::to_vec(&json!({"format": CONTAINER_VERSION})).map_err(|e| e.to_string())?;
    let mut files: Vec<(String, Vec<u8>)> = vec![
        (CONTAINER_ENTRY.to_string(), container),
        (DOCUMENT_ENTRY.to_string(), document),
    ];
    for (name, bytes) in blobs {
        files.push((name, bytes));
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    crate::zip_entries(&files)
}

fn format_string(version: u32) -> String {
    format!("solidcraft/{version}")
}

fn validate_design(design: &SolidcraftDesign) -> Result<(), String> {
    let version = parse_format_version(&design.format)?;
    if version > SUPPORTED_FORMAT {
        return Err(format!(
            "design file format {version} is newer than the newest format this tool has been tested against ({SUPPORTED_FORMAT})"
        ));
    }
    if design.units != "mm" {
        return Err("design units must be millimetres".to_string());
    }
    check_axes(design)?;
    Ok(())
}

fn parse_format_version(format: &str) -> Result<u32, String> {
    let rest = format
        .strip_prefix("solidcraft/")
        .ok_or_else(|| "design format must start with solidcraft/".to_string())?;
    rest.parse::<u32>()
        .map_err(|_| "design format number is not an integer".to_string())
}

fn check_axes(design: &SolidcraftDesign) -> Result<(), String> {
    if let Some(axes) = design.extra.get("axes") {
        refuse_non_z_up(axes)?;
    }
    for value in design.metadata.values() {
        if let Some(frame) = value.get("frame").or_else(|| value.get("Frame")) {
            refuse_non_z_up(frame)?;
        }
        if value.get("up").is_some() {
            refuse_non_z_up(value)?;
        }
    }
    Ok(())
}

fn refuse_non_z_up(value: &Value) -> Result<(), String> {
    let up = value
        .get("up")
        .or_else(|| value.get("Up"))
        .and_then(Value::as_str);
    if let Some(up) = up {
        let ok = up.eq_ignore_ascii_case("z") || up.eq_ignore_ascii_case("+z");
        if !ok {
            return Err("design axes must be Z up".to_string());
        }
    }
    Ok(())
}

fn strip_blob_fields(design: &mut SolidcraftDesign) {
    for feature in &mut design.features {
        strip_mesh_blob_fields(feature);
    }
}

fn strip_mesh_blob_fields(value: &mut Value) {
    let Some(obj) = value.as_object_mut() else {
        return;
    };
    if let Some(meshes) = obj.get_mut("meshes").and_then(Value::as_array_mut) {
        for mesh in meshes {
            if let Some(mesh_obj) = mesh.as_object_mut() {
                mesh_obj.remove(MESH_BLOB_FIELD);
            }
        }
    }
}

fn externalize_meshes(design: &mut SolidcraftDesign, blobs: &mut BTreeMap<String, Vec<u8>>) -> Result<(), String> {
    for feature in &mut design.features {
        externalize_feature_meshes(feature, blobs)?;
    }
    Ok(())
}

fn externalize_feature_meshes(feature: &mut Value, blobs: &mut BTreeMap<String, Vec<u8>>) -> Result<(), String> {
    let Some(obj) = feature.as_object_mut() else {
        return Ok(());
    };
    let Some(meshes) = obj.get_mut("meshes").and_then(Value::as_array_mut) else {
        return Ok(());
    };
    for mesh in meshes {
        let Some(mesh_obj) = mesh.as_object_mut() else {
            continue;
        };
        let positions = mesh_obj.get("positions").and_then(Value::as_array).cloned().unwrap_or_default();
        let triangles = mesh_obj.get("triangles").and_then(Value::as_array).cloned().unwrap_or_default();
        if positions.is_empty() && triangles.is_empty() {
            continue;
        }
        let encoded = encode_mesh(&positions, &triangles)?;
        let name = blob_name(&encoded, "mesh");
        blobs.entry(name.clone()).or_insert(encoded);
        mesh_obj.insert(MESH_BLOB_FIELD.into(), Value::String(name));
        mesh_obj.insert("positions".into(), Value::Array(vec![]));
        mesh_obj.insert("triangles".into(), Value::Array(vec![]));
    }
    Ok(())
}

fn hydrate_meshes(design: &mut SolidcraftDesign, entries: &BTreeMap<String, Vec<u8>>) -> Result<(), String> {
    for feature in &mut design.features {
        hydrate_feature_meshes(feature, entries)?;
    }
    Ok(())
}

fn hydrate_feature_meshes(feature: &mut Value, entries: &BTreeMap<String, Vec<u8>>) -> Result<(), String> {
    let Some(obj) = feature.as_object_mut() else {
        return Ok(());
    };
    let Some(meshes) = obj.get_mut("meshes").and_then(Value::as_array_mut) else {
        return Ok(());
    };
    for mesh in meshes {
        let Some(mesh_obj) = mesh.as_object_mut() else {
            continue;
        };
        let Some(blob_ref) = mesh_obj.get(MESH_BLOB_FIELD).and_then(Value::as_str) else {
            continue;
        };
        let bytes = entries
            .get(blob_ref)
            .ok_or_else(|| format!("container references a missing blob: {blob_ref}"))?;
        verify_blob_path(blob_ref, "mesh")?;
        if digest(bytes) != blob_hash_from_path(blob_ref, "mesh")? {
            return Err(format!("blob hash mismatch: {blob_ref}"));
        }
        let (positions, triangles) = decode_mesh(bytes)?;
        mesh_obj.insert("positions".into(), Value::Array(positions));
        mesh_obj.insert("triangles".into(), Value::Array(triangles));
        mesh_obj.remove(MESH_BLOB_FIELD);
    }
    Ok(())
}

fn encode_mesh(positions: &[Value], triangles: &[Value]) -> Result<Vec<u8>, String> {
    let mut pos = Vec::with_capacity(positions.len());
    for v in positions {
        let n = v.as_f64().ok_or_else(|| "mesh position is not a number".to_string())?;
        if !n.is_finite() || n.abs() > 1e9 {
            return Err("mesh position is out of range".to_string());
        }
        pos.push(n);
    }
    if !pos.len().is_multiple_of(3) {
        return Err("mesh positions length is not a multiple of three".to_string());
    }
    let mut tri = Vec::with_capacity(triangles.len());
    for v in triangles {
        let n = v
            .as_u64()
            .ok_or_else(|| "mesh index is not an unsigned integer".to_string())?;
        let n = u32::try_from(n).map_err(|_| "mesh index is too large".to_string())?;
        tri.push(n);
    }
    if !tri.len().is_multiple_of(3) {
        return Err("mesh indices length is not a multiple of three".to_string());
    }
    let vert_count = pos.len() / 3;
    for &index in &tri {
        if index as usize >= vert_count {
            return Err("mesh index points past the vertex list".to_string());
        }
    }
    let mut bytes = Vec::new();
    bytes.extend_from_slice(MESH_MAGIC);
    bytes.extend_from_slice(&(pos.len() as u64).to_le_bytes());
    bytes.extend_from_slice(&(tri.len() as u64).to_le_bytes());
    for value in &pos {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    for value in &tri {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    Ok(bytes)
}

fn decode_mesh(bytes: &[u8]) -> Result<(Vec<Value>, Vec<Value>), String> {
    if bytes.get(..8) != Some(MESH_MAGIC.as_slice()) {
        return Err("unsupported mesh blob version".to_string());
    }
    let positions_len = read_u64(bytes, 8)? as usize;
    let triangles_len = read_u64(bytes, 16)? as usize;
    if !positions_len.is_multiple_of(3) || !triangles_len.is_multiple_of(3) {
        return Err("mesh blob counts are invalid".to_string());
    }
    let pos_bytes = positions_len
        .checked_mul(8)
        .and_then(|n| n.checked_add(24))
        .ok_or_else(|| "mesh blob size overflow".to_string())?;
    let total = triangles_len
        .checked_mul(4)
        .and_then(|n| n.checked_add(pos_bytes))
        .ok_or_else(|| "mesh blob size overflow".to_string())?;
    if total != bytes.len() {
        return Err("mesh blob length does not match its header".to_string());
    }
    let mut positions = Vec::with_capacity(positions_len);
    let mut at = 24;
    for _ in 0..positions_len {
        let value = f64::from_le_bytes(bytes[at..at + 8].try_into().map_err(|_| "truncated mesh blob")?);
        at += 8;
        positions.push(json!(value));
    }
    let mut triangles = Vec::with_capacity(triangles_len);
    for _ in 0..triangles_len {
        let value = u32::from_le_bytes(bytes[at..at + 4].try_into().map_err(|_| "truncated mesh blob")?);
        at += 4;
        triangles.push(json!(value));
    }
    Ok((positions, triangles))
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, String> {
    let slice = bytes
        .get(offset..offset + 8)
        .ok_or_else(|| "truncated mesh blob".to_string())?;
    Ok(u64::from_le_bytes(slice.try_into().map_err(|_| "truncated mesh blob")?))
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn blob_name(bytes: &[u8], extension: &str) -> String {
    format!("blobs/{}.{extension}", digest(bytes))
}

fn verify_blob_path(path: &str, extension: &str) -> Result<(), String> {
    let hash = blob_hash_from_path(path, extension)?;
    if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) {
        return Err("invalid blob reference".to_string());
    }
    Ok(())
}

fn blob_hash_from_path(path: &str, extension: &str) -> Result<String, String> {
    path.strip_prefix("blobs/")
        .and_then(|n| n.strip_suffix(&format!(".{extension}")))
        .map(str::to_string)
        .ok_or_else(|| "invalid blob reference".to_string())
}

/// Raw ZIP entries for optional display-cache reads (issue #733).
pub fn read_container_entries(bytes: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, String> {
    read_stored_zip(bytes)
}

fn read_stored_zip(bytes: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, String> {
    let eocd = bytes
        .windows(22)
        .rposition(|w| w.starts_with(b"PK\x05\x06"))
        .ok_or_else(|| "not a container: no archive directory".to_string())?;
    let read_u16 = |at: usize| -> Result<u16, String> {
        Ok(u16::from_le_bytes([bytes[at], bytes[at + 1]]))
    };
    let read_u32 = |at: usize| -> Result<u32, String> {
        Ok(u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]))
    };
    let entries = read_u16(eocd + 10)? as usize;
    if entries > MAX_CONTAINER_ENTRIES {
        return Err("container lists too many entries".to_string());
    }
    if read_u16(eocd + 4)? != 0 || read_u16(eocd + 6)? != 0 {
        return Err("unsupported ZIP directory".to_string());
    }
    let declared = read_u16(eocd + 8)? as usize;
    if declared != entries {
        return Err("duplicate entries or unsupported ZIP directory".to_string());
    }
    let comment_len = read_u16(eocd + 20)? as usize;
    if eocd + 22 + comment_len != bytes.len() {
        return Err("unsupported ZIP directory".to_string());
    }
    let mut at = read_u32(eocd + 16)? as usize;
    let mut names = BTreeMap::<String, (u32, u32, u32)>::new();
    let mut expanded = 0usize;
    for _ in 0..entries {
        if !bytes[at..].starts_with(b"PK\x01\x02") || at + 46 > bytes.len() {
            return Err("corrupt archive directory entry".to_string());
        }
        let compression = read_u16(at + 10)?;
        if compression != 0 {
            return Err("container entry is not stored".to_string());
        }
        let crc = read_u32(at + 16)?;
        let size = read_u32(at + 20)?;
        let name_len = read_u16(at + 28)? as usize;
        let extra_len = read_u16(at + 30)? as usize;
        let comment_len = read_u16(at + 32)? as usize;
        let local_at = read_u32(at + 42)? as usize;
        if name_len == 0 || at + 46 + name_len > bytes.len() {
            return Err("corrupt archive directory entry".to_string());
        }
        let name_bytes = &bytes[at + 46..at + 46 + name_len];
        let name = std::str::from_utf8(name_bytes).map_err(|_| "archive entry name is not UTF-8".to_string())?;
        if !entry_name_safe(name) {
            return Err("duplicate or invalid archive entry".to_string());
        }
        if names.insert(name.to_string(), (size, crc, local_at)).is_some() {
            return Err("duplicate or invalid archive entry".to_string());
        }
        expanded = expanded
            .checked_add(size as usize)
            .ok_or_else(|| "expanded size overflow".to_string())?;
        if expanded > MAX_CONTAINER_EXPANDED {
            return Err("expanded container is too large".to_string());
        }
        at += 46 + name_len + extra_len + comment_len;
    }
    let mut out = BTreeMap::new();
    for (name, (size, crc, local_at)) in names {
        if local_at + 30 > bytes.len() || !bytes[local_at..].starts_with(b"PK\x03\x04") {
            return Err("corrupt archive entry header".to_string());
        }
        let local_name = read_u16(local_at + 26)? as usize;
        let local_extra = read_u16(local_at + 28)? as usize;
        if local_at + 30 + local_name > bytes.len()
            || &bytes[local_at + 30..local_at + 30 + local_name] != name.as_bytes()
        {
            return Err("archive entry headers disagree about the name".to_string());
        }
        let data_at = local_at + 30 + local_name + local_extra;
        let size = size as usize;
        if data_at + size > bytes.len() {
            return Err("archive entry runs past the end of the file".to_string());
        }
        let payload = &bytes[data_at..data_at + size];
        if crate::crc32(payload) != crc {
            return Err(format!("archive entry CRC mismatch: {name}"));
        }
        if name.starts_with("blobs/") {
            if let Some((_, ext)) = name.rsplit_once('.') {
                verify_blob_path(&name, ext)?;
                if digest(payload) != blob_hash_from_path(&name, ext)? {
                    return Err(format!("blob hash mismatch: {name}"));
                }
            }
        }
        out.insert(name, payload.to_vec());
    }
    Ok(out)
}

fn entry_name_safe(name: &str) -> bool {
    if name.is_empty()
        || name.starts_with('/')
        || name.contains('\\')
        || name.split('/').any(|part| part.is_empty() || part == "." || part == "..")
    {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    type Check = Result<(), String>;

    fn sample_design() -> SolidcraftDesign {
        SolidcraftDesign {
            format: format_string(SUPPORTED_FORMAT),
            name: "demo".into(),
            units: "mm".into(),
            metadata: BTreeMap::from([(
                "com.example.unknown".into(),
                json!({"note": "keep me"}),
            )]),
            params: vec![],
            features: vec![json!({
                "id": 1,
                "name": "Body1",
                "type": "mesh_import",
                "file": "test",
                "meshes": [{
                    "name": "Body1",
                    "positions": [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
                    "triangles": [0, 1, 2]
                }]
            })],
            next_id: 1,
            components: vec![SolidcraftComponent {
                id: 0,
                name: "Root".into(),
                parent: 0,
                extra: BTreeMap::new(),
            }],
            occurrences: vec![SolidcraftOccurrence {
                id: 1,
                component: 0,
                parent: 0,
                name: "Root".into(),
                transform: None,
                grounded: false,
                extra: BTreeMap::new(),
            }],
            user_materials: vec![json!({"name": "FR-4", "density": 1.85})],
            materials: BTreeMap::from([("Body1".into(), json!("FR-4"))]),
            appearances: SolidcraftAppearances {
                bodies: BTreeMap::from([(
                    "Body1".into(),
                    json!({"name": "Part", "color": [200, 200, 200], "opacity": 1.0}),
                )],
                extra: BTreeMap::new(),
            },
            extra: BTreeMap::new(),
        }
    }

    #[test]
    fn container_round_trips_the_subset() -> Check {
        let design = sample_design();
        let json_text = write_solidcraft_json(&design)?;
        let from_json = read_solidcraft_json(&json_text)?;
        assert_eq!(from_json, design);
        let container = write_solidcraft_container(&design)?;
        let back = read_solidcraft_container(&container)?;
        assert_eq!(back, design);
        Ok(())
    }

    #[test]
    fn container_rejects_bad_hash() -> Check {
        let design = sample_design();
        let container = write_solidcraft_container(&design)?;
        let mut entries = read_stored_zip(&container)?;
        let name = entries
            .keys()
            .find(|k| k.starts_with("blobs/"))
            .cloned()
            .ok_or("no mesh blob")?;
        entries.get_mut(&name).ok_or("no mesh blob")?[0] ^= 0x01;
        let zip = crate::zip_entries(
            &entries
                .into_iter()
                .map(|(k, v)| (k, v))
                .collect::<Vec<_>>(),
        )?;
        assert!(
            read_solidcraft_container(&zip).is_err(),
            "a wrong blob hash is refused"
        );
        Ok(())
    }

    #[test]
    fn container_rejects_unsafe_names() -> Check {
        let design = sample_design();
        let document = serde_json::to_vec_pretty(&design).map_err(|e| e.to_string())?;
        let container_meta =
            serde_json::to_vec(&json!({"format": CONTAINER_VERSION})).map_err(|e| e.to_string())?;
        for bad in ["../escape", "blobs/../x.mesh", "/absolute"] {
            let files = vec![
                (CONTAINER_ENTRY.to_string(), container_meta.clone()),
                (DOCUMENT_ENTRY.to_string(), document.clone()),
                (bad.to_string(), b"x".to_vec()),
            ];
            let zip = crate::zip_entries(&files)?;
            assert!(
                read_solidcraft_container(&zip).is_err(),
                "unsafe name {bad:?} is refused"
            );
        }
        let dup = crate::zip_entries(&[
            (CONTAINER_ENTRY.to_string(), container_meta.clone()),
            (DOCUMENT_ENTRY.to_string(), document.clone()),
            ("note.txt".into(), b"a".to_vec()),
            ("note.txt".into(), b"b".to_vec()),
        ])?;
        assert!(
            read_solidcraft_container(&dup).is_err(),
            "duplicate entry names are refused"
        );
        Ok(())
    }

    #[test]
    fn unknown_metadata_survives() -> Check {
        let mut design = sample_design();
        design
            .metadata
            .insert("vendor.custom.flag".into(), json!(true));
        let text = write_solidcraft_json(&design)?;
        let once = read_solidcraft_json(&text)?;
        let again = read_solidcraft_json(&write_solidcraft_json(&once)?)?;
        assert_eq!(once.metadata.get("vendor.custom.flag"), Some(&json!(true)));
        assert_eq!(again, once);
        Ok(())
    }
}
