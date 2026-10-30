//! SolidCraft ZIP containers: stored blobs named by SHA-256, bounded entry count and size.
use super::mesh;
use crate::zip_entries;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

const VERSION: &str = "solidcraft-container/1";
pub const MAX_ENTRIES: usize = 10_000;
pub const MAX_EXPANDED: usize = 512 * 1024 * 1024;
const MAX_DOCUMENT: usize = 16 * 1024 * 1024;

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn invalid(message: impl std::fmt::Display) -> String {
    format!("design container: {message}")
}

fn safe_name(name: &str) -> Result<(), String> {
    if name.is_empty()
        || name.contains('\\')
        || name.split('/').any(|c| c.is_empty() || c == "." || c == "..")
    {
        return Err(invalid("invalid archive entry"));
    }
    Ok(())
}

pub fn is_container(bytes: &[u8]) -> bool {
    bytes.starts_with(b"PK\x03\x04")
}

/// Replace inline mesh arrays with `mesh_blob` references and write a stored ZIP container.
pub fn write_container(mut doc: Value) -> Result<Vec<u8>, String> {
    let mut blobs: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let features = doc
        .get_mut("features")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| "features must be a list".to_string())?;
    for feature in features.iter_mut() {
        if feature.get("type").and_then(Value::as_str) != Some("mesh_import") {
            continue;
        }
        let meshes = feature
            .get_mut("meshes")
            .and_then(Value::as_array_mut)
            .ok_or_else(|| "mesh_import feature needs meshes".to_string())?;
        for mesh_value in meshes.iter_mut() {
            let positions: Vec<f64> = mesh_value
                .get("positions")
                .and_then(Value::as_array)
                .ok_or_else(|| "mesh positions must be a list".to_string())?
                .iter()
                .map(|v| v.as_f64().ok_or_else(|| "bad coordinate".to_string()))
                .collect::<Result<_, _>>()?;
            let triangles: Vec<u32> = mesh_value
                .get("triangles")
                .and_then(Value::as_array)
                .ok_or_else(|| "mesh triangles must be a list".to_string())?
                .iter()
                .map(|v| {
                    v.as_u64()
                        .ok_or_else(|| "bad triangle index".to_string())
                        .and_then(|n| u32::try_from(n).map_err(|_| "bad triangle index".to_string()))
                })
                .collect::<Result<_, _>>()?;
            let bytes = mesh::encode(&positions, &triangles)?;
            let name = mesh::blob_name(&bytes, "mesh");
            blobs.insert(name.clone(), bytes);
            if let Some(object) = mesh_value.as_object_mut() {
                object.insert("positions".into(), json!([]));
                object.insert("triangles".into(), json!([]));
                object.insert("mesh_blob".into(), Value::String(name));
            }
        }
    }
    let metadata = serde_json::to_vec(&doc).map_err(|e| e.to_string())?;
    if metadata.len() > MAX_DOCUMENT {
        return Err(invalid("document too large"));
    }
    let mut files: Vec<(String, Vec<u8>)> = blobs.into_iter().collect();
    files.push(("document.json".into(), metadata));
    files.push((
        "container.json".into(),
        serde_json::to_vec(&json!({"format": VERSION})).map_err(|e| e.to_string())?,
    ));
    files.sort_by(|a, b| a.0.cmp(&b.0));
    zip_entries(&files)
}

fn read_stored_zip(bytes: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, String> {
    if bytes.len() > MAX_EXPANDED {
        return Err(invalid("file too large"));
    }
    let eocd = bytes
        .windows(22)
        .rposition(|w| w.starts_with(b"PK\x05\x06"))
        .ok_or_else(|| invalid("missing ZIP directory"))?;
    let read_u16 = |at: usize| -> Result<u16, String> {
        bytes
            .get(at..at + 2)
            .ok_or_else(|| invalid("truncated ZIP directory"))?
            .try_into()
            .map(u16::from_le_bytes)
            .map_err(|_| invalid("truncated ZIP directory"))
    };
    let read_u32 = |at: usize| -> Result<u32, String> {
        bytes
            .get(at..at + 4)
            .ok_or_else(|| invalid("truncated ZIP directory"))?
            .try_into()
            .map(u32::from_le_bytes)
            .map_err(|_| invalid("truncated ZIP directory"))
    };
    if read_u16(eocd + 4)? != 0 || read_u16(eocd + 6)? != 0 {
        return Err(invalid("unsupported ZIP directory"));
    }
    let entries = read_u16(eocd + 10)? as usize;
    if entries > MAX_ENTRIES || entries != read_u16(eocd + 8)? as usize {
        return Err(invalid("too many entries or unsupported ZIP directory"));
    }
    let mut at = read_u32(eocd + 16)? as usize;
    let mut out = BTreeMap::new();
    let mut names = BTreeSet::new();
    let mut expanded = 0usize;
    for _ in 0..entries {
        if !bytes.get(at..).is_some_and(|s| s.starts_with(b"PK\x01\x02")) || at + 46 > bytes.len() {
            return Err(invalid("corrupt archive directory entry"));
        }
        let comp_size = read_u32(at + 20)? as usize;
        let name_len = read_u16(at + 28)? as usize;
        let extra_len = read_u16(at + 30)? as usize;
        let comment_len = read_u16(at + 32)? as usize;
        let method = read_u16(at + 10)?;
        if method != 0 {
            return Err(invalid("only stored ZIP entries are supported"));
        }
        let local_at = read_u32(at + 42)? as usize;
        if name_len == 0 || at + 46 + name_len > bytes.len() {
            return Err(invalid("corrupt archive directory entry"));
        }
        let name_bytes = &bytes[at + 46..at + 46 + name_len];
        let name = std::str::from_utf8(name_bytes).map_err(|_| invalid("invalid entry name"))?;
        safe_name(name)?;
        if !names.insert(name.to_string()) {
            return Err(invalid("duplicate archive entry"));
        }
        expanded = expanded
            .checked_add(comp_size)
            .ok_or_else(|| invalid("expanded size overflow"))?;
        if expanded > MAX_EXPANDED {
            return Err(invalid("expanded package too large"));
        }
        if local_at + 30 > bytes.len() || !bytes[local_at..].starts_with(b"PK\x03\x04") {
            return Err(invalid("corrupt archive entry header"));
        }
        let local_name = read_u16(local_at + 26)? as usize;
        let local_extra = read_u16(local_at + 28)? as usize;
        let local_name_bytes = &bytes[local_at + 30..local_at + 30 + local_name];
        if local_at + 30 + local_name > bytes.len() || local_name_bytes != name_bytes {
            return Err(invalid("archive entry headers disagree about the name"));
        }
        let data_at = local_at + 30 + local_name + local_extra;
        if data_at + comp_size > bytes.len() {
            return Err(invalid("archive entry runs past the end of the file"));
        }
        out.insert(
            name.to_string(),
            bytes[data_at..data_at + comp_size].to_vec(),
        );
        at += 46 + name_len + extra_len + comment_len;
    }
    Ok(out)
}

fn hydrate_document(mut doc: Value, parts: &BTreeMap<String, Vec<u8>>) -> Result<Value, String> {
    let mut hydrated = 0usize;
    let features = doc
        .get_mut("features")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| invalid("features must be a list"))?;
    for feature in features.iter_mut() {
        if feature.get("type").and_then(Value::as_str) != Some("mesh_import") {
            continue;
        }
        let meshes = feature
            .get_mut("meshes")
            .and_then(Value::as_array_mut)
            .ok_or_else(|| invalid("mesh_import feature needs meshes"))?;
        for mesh_value in meshes.iter_mut() {
            let positions = mesh_value.get("positions").and_then(Value::as_array);
            let triangles = mesh_value.get("triangles").and_then(Value::as_array);
            let empty = positions.is_some_and(|p| p.is_empty()) && triangles.is_some_and(|t| t.is_empty());
            if !empty {
                return Err(invalid("ambiguous embedded mesh and blob"));
            }
            let reference = mesh_value
                .get("mesh_blob")
                .and_then(Value::as_str)
                .ok_or_else(|| invalid("missing `mesh_blob` reference"))?;
            let hash = reference
                .strip_prefix("blobs/")
                .and_then(|n| n.strip_suffix(".mesh"))
                .filter(|h| h.len() == 64 && h.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
                .ok_or_else(|| invalid("invalid blob reference"))?;
            let bytes = parts
                .get(reference)
                .ok_or_else(|| invalid(format!("required part `{reference}` missing")))?;
            if digest(bytes) != hash {
                return Err(invalid(format!("blob hash mismatch: {reference}")));
            }
            hydrated = hydrated
                .checked_add(bytes.len())
                .ok_or_else(|| invalid("resolved payloads too large"))?;
            if hydrated > MAX_EXPANDED {
                return Err(invalid("resolved payloads too large"));
            }
            let (flat, indices) = mesh::decode(bytes)?;
            if let Some(object) = mesh_value.as_object_mut() {
                object.insert(
                    "positions".into(),
                    Value::Array(flat.into_iter().map(json).collect()),
                );
                object.insert(
                    "triangles".into(),
                    Value::Array(indices.into_iter().map(|i| json!(i)).collect()),
                );
                object.remove("mesh_blob");
            }
        }
    }
    Ok(doc)
}

/// Read `document.json` from a container and hydrate mesh blobs.
pub fn read_container(bytes: &[u8]) -> Result<Value, String> {
    let parts = read_stored_zip(bytes)?;
    let container: Value = serde_json::from_slice(
        parts
            .get("container.json")
            .ok_or_else(|| invalid("required part `container.json` missing"))?,
    )
    .map_err(|e| invalid(e))?;
    if container.get("format").and_then(Value::as_str) != Some(VERSION) {
        return Err(invalid("unsupported container version"));
    }
    let doc: Value = serde_json::from_slice(
        parts
            .get("document.json")
            .ok_or_else(|| invalid("required part `document.json` missing"))?,
    )
    .map_err(|e| invalid(e))?;
    hydrate_document(doc, &parts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::solidcraft::design::write_json;

    fn triangle_doc() -> Value {
        json!({
            "format": "solidcraft/22",
            "name": "blob",
            "units": "mm",
            "metadata": {},
            "params": [],
            "features": [{
                "id": 1,
                "name": "Body",
                "type": "mesh_import",
                "file": "native",
                "meshes": [{
                    "name": "Body",
                    "positions": [0.0,0.0,0.0, 1.0,0.0,0.0, 0.0,1.0,0.0],
                    "triangles": [0,1,2]
                }]
            }],
            "next_id": 1,
            "components": [{"id": 0, "name": "Root", "parent": null}],
            "occurrences": [],
            "appearances": {"bodies": {}}
        })
    }

    #[test]
    fn container_round_trips_the_subset() -> Result<(), String> {
        let doc = triangle_doc();
        let bytes = write_container(doc.clone())?;
        let back = read_container(&bytes)?;
        assert_eq!(write_json(&doc)?, write_json(&back)?);
        Ok(())
    }

    #[test]
    fn container_rejects_bad_hash() -> Result<(), String> {
        let mesh_bytes = mesh::encode(&[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0], &[0, 1, 2])?;
        let good = mesh::blob_name(&mesh_bytes, "mesh");
        let bad = "blobs/0000000000000000000000000000000000000000000000000000000000000000.mesh";
        let doc = json!({
            "format": "solidcraft/22",
            "name": "x",
            "units": "mm",
            "metadata": {},
            "params": [],
            "features": [{
                "id": 1,
                "name": "Body",
                "type": "mesh_import",
                "meshes": [{"name": "Body", "positions": [], "triangles": [], "mesh_blob": bad}]
            }],
            "next_id": 1,
            "components": [],
            "occurrences": [],
            "appearances": {"bodies": {}}
        });
        let zip = zip_entries(&[
            (bad.to_string(), mesh_bytes),
            (
                "document.json".into(),
                serde_json::to_vec(&doc).map_err(|e| e.to_string())?,
            ),
            (
                "container.json".into(),
                serde_json::to_vec(&json!({"format": VERSION})).map_err(|e| e.to_string())?,
            ),
            (good, vec![]),
        ])?;
        assert!(read_container(&zip).is_err());
        Ok(())
    }

    #[test]
    fn container_rejects_unsafe_names() -> Result<(), String> {
        assert!(safe_name("../escape").is_err());
        assert!(safe_name("blobs/../x.mesh").is_err());
        Ok(())
    }
}
