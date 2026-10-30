//! JSON SolidCraft design subset: read, write, and metadata preservation.
use super::container::{self, MAX_EXPANDED};
use serde_json::{Map, Value, json};

/// Newest SolidCraft format number this reader has been tested against.
pub const SOLIDCRAFT_FORMAT_MAX: u32 = 22;

fn invalid(message: impl std::fmt::Display) -> String {
    message.to_string()
}

fn parse_format(raw: &str) -> Result<u32, String> {
    let number = raw
        .strip_prefix("solidcraft/")
        .ok_or_else(|| "format must start with solidcraft/".to_string())?;
    number
        .parse::<u32>()
        .map_err(|_| "format number is not a usable integer".to_string())
}

fn check_document(doc: &Value) -> Result<(), String> {
    let format = doc
        .get("format")
        .and_then(Value::as_str)
        .ok_or_else(|| "format is missing".to_string())?;
    let number = parse_format(format)?;
    if number > SOLIDCRAFT_FORMAT_MAX {
        return Err(format!(
            "SolidCraft format {number} is newer than the supported format {SOLIDCRAFT_FORMAT_MAX}"
        ));
    }
    let units = doc
        .get("units")
        .and_then(Value::as_str)
        .ok_or_else(|| "units are missing".to_string())?;
    if units != "mm" {
        return Err("SolidCraft designs must use millimetre units".into());
    }
    Ok(())
}

/// Read a plain JSON design or a ZIP container.
pub fn read_solidcraft(bytes: &[u8]) -> Result<Value, String> {
    if bytes.len() > MAX_EXPANDED {
        return Err(invalid("file too large"));
    }
    if container::is_container(bytes) {
        let doc = container::read_container(bytes)?;
        check_document(&doc)?;
        return Ok(doc);
    }
    let doc: Value = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    check_document(&doc)?;
    Ok(doc)
}

/// Serialize a design as pretty JSON text.
pub fn write_json(doc: &Value) -> Result<String, String> {
    check_document(doc)?;
    serde_json::to_string_pretty(doc).map_err(|e| e.to_string())
}

/// Write a container around the same JSON subset.
pub fn write_container(doc: &Value) -> Result<Vec<u8>, String> {
    check_document(doc)?;
    container::write_container(doc.clone())
}

/// Split known top-level keys from everything else so unknown keys survive a round trip.
pub fn split_unknown(doc: &Value) -> Result<(Map<String, Value>, Map<String, Value>), String> {
    let object = doc
        .as_object()
        .ok_or_else(|| "design must be an object".to_string())?;
    let known = [
        "format",
        "name",
        "units",
        "metadata",
        "params",
        "features",
        "next_id",
        "components",
        "occurrences",
        "user_materials",
        "materials",
        "appearances",
    ];
    let mut root = Map::new();
    let mut extra = Map::new();
    for (key, value) in object {
        if known.contains(&key.as_str()) {
            root.insert(key.clone(), value.clone());
        } else {
            extra.insert(key.clone(), value.clone());
        }
    }
    Ok((root, extra))
}

pub fn merge_unknown(root: Map<String, Value>, extra: Map<String, Value>) -> Value {
    let mut out = root;
    for (key, value) in extra {
        out.insert(key, value);
    }
    Value::Object(out)
}

/// Compare two designs for round-trip equality (ignores key order).
pub fn designs_equal(a: &Value, b: &Value) -> bool {
    normalize(a) == normalize(b)
}

fn normalize(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<_> = map.keys().cloned().collect();
            keys.sort();
            let mut out = Map::new();
            for key in keys {
                let child = map.get(&key).cloned().unwrap_or(Value::Null);
                out.insert(key.clone(), normalize(&child));
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(normalize).collect()),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::mesh;
    use super::*;

    #[test]
    fn unknown_metadata_survives() -> Result<(), String> {
        let mut doc = json!({
            "format": "solidcraft/22",
            "name": "meta",
            "units": "mm",
            "metadata": {"vendor.example": {"note": "keep me"}},
            "params": [],
            "features": [],
            "next_id": 0,
            "components": [],
            "occurrences": [],
            "appearances": {"bodies": {}},
            "future_field": {"alpha": 1}
        });
        let (root, extra) = split_unknown(&doc)?;
        doc = merge_unknown(root, extra);
        let text = write_json(&doc)?;
        let back = read_solidcraft(text.as_bytes())?;
        assert_eq!(back.get("future_field"), doc.get("future_field"));
        assert_eq!(
            back.get("metadata")
                .and_then(|m| m.get("vendor.example")),
            doc.get("metadata").and_then(|m| m.get("vendor.example"))
        );
        Ok(())
    }

    #[test]
    fn refuses_a_newer_format_number() {
        let doc = json!({"format": "solidcraft/99", "units": "mm"});
        assert!(check_document(&doc).is_err());
    }

    #[test]
    fn mesh_round_trip_bytes() -> Result<(), String> {
        let bytes = mesh::encode(&[0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 3.0, 0.0], &[0, 1, 2])?;
        let (p, t) = mesh::decode(&bytes)?;
        assert_eq!(p.len(), 9);
        assert_eq!(t, vec![0, 1, 2]);
        Ok(())
    }
}
