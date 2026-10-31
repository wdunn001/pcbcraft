//! Readers for the second open community pool: JSON part and package records
//! that name a symbol entity, a layout package and an optional STEP model path.
//!
//! # Rules
//! - Input size is capped; unknown top-level JSON fields are ignored.
//! - UUID fields must look like lowercase hex with dashes.
//! - Model paths are relative pool paths only, with no `..` segments.
//!
//! # Dependencies
//! - crates/library/src/sources.rs
use crate::sources::SourceError;
use serde_json::Value;
use std::collections::BTreeMap;

/// Largest part or package JSON the reader accepts.
pub const MAX_JSON_BYTES: usize = 512 * 1024;
/// Most tags or parametric keys on one record.
pub const MAX_KEYS: usize = 64;
/// Longest one tag, name or path string.
pub const MAX_TEXT_BYTES: usize = 256;

fn invalid(text: impl Into<String>) -> SourceError {
    SourceError::Invalid(text.into())
}

fn check_text(field: &str, text: &str) -> Result<(), SourceError> {
    if text.is_empty() || text.len() > MAX_TEXT_BYTES || text.chars().any(|c| c.is_control()) {
        return Err(invalid(format!("the {field} is missing or too long")));
    }
    Ok(())
}

fn check_uuid(field: &str, text: &str) -> Result<(), SourceError> {
    check_text(field, text)?;
    if text.len() != 36
        || !text
            .chars()
            .all(|c| c.is_ascii_hexdigit() || c == '-')
    {
        return Err(invalid(format!("the {field} is not a UUID")));
    }
    Ok(())
}

fn check_pool_path(field: &str, text: &str) -> Result<(), SourceError> {
    check_text(field, text)?;
    if text.starts_with('/') || text.contains('\\') || text.contains("..") {
        return Err(invalid(format!("the {field} is not a safe pool path")));
    }
    Ok(())
}

fn parse_json(text: &str) -> Result<Value, SourceError> {
    if text.len() > MAX_JSON_BYTES {
        return Err(invalid("the JSON is too large"));
    }
    serde_json::from_str(text).map_err(|_| invalid("the JSON could not be parsed"))
}

fn string_field(obj: &Value, key: &str) -> Result<String, SourceError> {
    let v = obj.get(key).ok_or_else(|| invalid(format!("{key} is missing")))?;
    let s = v.as_str().ok_or_else(|| invalid(format!("{key} must be text")))?;
    Ok(s.to_owned())
}

/// A `[override, text]` pair used for MPN, value and manufacturer fields.
fn labeled_pair(obj: &Value, key: &str) -> Result<String, SourceError> {
    let v = obj.get(key).ok_or_else(|| invalid(format!("{key} is missing")))?;
    let pair = v
        .as_array()
        .ok_or_else(|| invalid(format!("{key} must be a pair")))?;
    if pair.len() != 2 {
        return Err(invalid(format!("{key} must hold exactly two entries")));
    }
    let text = pair[1]
        .as_str()
        .ok_or_else(|| invalid(format!("{key} text must be a string")))?;
    check_text(key, text)?;
    Ok(text.to_owned())
}

fn string_map(obj: &Value, key: &str) -> Result<BTreeMap<String, String>, SourceError> {
    let mut out = BTreeMap::new();
    let Some(map) = obj.get(key).and_then(Value::as_object) else {
        return Ok(out);
    };
    if map.len() > MAX_KEYS {
        return Err(invalid(format!("{key} has too many entries")));
    }
    for (k, v) in map {
        if k.len() > MAX_TEXT_BYTES {
            return Err(invalid(format!("a {key} name is too long")));
        }
        let s = v
            .as_str()
            .ok_or_else(|| invalid(format!("{key} values must be text")))?;
        check_text(key, s)?;
        out.insert(k.clone(), s.to_owned());
    }
    Ok(out)
}

fn tag_list(obj: &Value) -> Result<Vec<String>, SourceError> {
    let mut out = Vec::new();
    let Some(list) = obj.get("tags").and_then(Value::as_array) else {
        return Ok(out);
    };
    if list.len() > MAX_KEYS {
        return Err(invalid("too many tags"));
    }
    for v in list {
        let s = v
            .as_str()
            .ok_or_else(|| invalid("each tag must be text"))?;
        check_text("tag", s)?;
        out.push(s.to_owned());
    }
    Ok(out)
}

/// One part record from the open pool JSON format.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenPoolPartRecord {
    /// Record id in the pool.
    pub uuid: String,
    /// Manufacturer part number text shown on the card.
    pub mpn: String,
    /// Manufacturer name when the pool gives one.
    pub manufacturer: String,
    /// Value text when the pool gives one.
    pub value: String,
    /// Symbol entity id this part uses.
    pub entity_id: String,
    /// Layout package id this part uses.
    pub package_id: String,
    /// Search tags copied from the file.
    pub tags: Vec<String>,
    /// Parametric attributes copied from the file.
    pub parametric: BTreeMap<String, String>,
}

/// Read a part JSON document from the second open pool format.
///
/// # Rules
/// - `type` must be `part`.
/// - `entity` and `package` hold UUID strings.
///
/// # Do not
/// - Fetch symbol or footprint bytes; callers use the ids against a CAD source.
pub fn read_open_pool_part(text: &str) -> Result<OpenPoolPartRecord, SourceError> {
    let root = parse_json(text)?;
    root.as_object()
        .ok_or_else(|| invalid("a part file must be a JSON object"))?;
    let kind = string_field(&root, "type")?;
    if kind != "part" {
        return Err(invalid("the JSON is not a part record"));
    }
    let uuid = string_field(&root, "uuid")?;
    check_uuid("uuid", &uuid)?;
    let entity_id = string_field(&root, "entity")?;
    check_uuid("entity", &entity_id)?;
    let package_id = string_field(&root, "package")?;
    check_uuid("package", &package_id)?;
    Ok(OpenPoolPartRecord {
        uuid,
        mpn: labeled_pair(&root, "MPN")?,
        manufacturer: labeled_pair(&root, "manufacturer")?,
        value: labeled_pair(&root, "value")?,
        entity_id,
        package_id,
        tags: tag_list(&root)?,
        parametric: string_map(&root, "parametric")?,
    })
}

/// A 3D model entry named in a package file.
#[derive(Clone, Debug, PartialEq)]
pub struct OpenPoolModelRef {
    /// Path of the STEP file inside the pool tree.
    pub path: String,
    /// Offset in nanometres along x, y and z.
    pub offset_nm: [i64; 3],
    /// Rotation in degrees as yaw, pitch and roll.
    pub rotate_deg: [f64; 3],
}

/// One package record from the open pool JSON format.
#[derive(Clone, Debug, PartialEq)]
pub struct OpenPoolPackageRecord {
    /// Record id in the pool.
    pub uuid: String,
    /// Short name of the package.
    pub name: String,
    /// Id of the default model entry, when present.
    pub default_model_id: String,
    /// Model files keyed by id.
    pub models: BTreeMap<String, OpenPoolModelRef>,
}

fn model_ref(id: &str, obj: &Value) -> Result<OpenPoolModelRef, SourceError> {
    let path = string_field(obj, "filename")?;
    check_pool_path("model path", &path)?;
    let read_i64 = |key: &str| -> Result<i64, SourceError> {
        obj.get(key)
            .and_then(Value::as_i64)
            .ok_or_else(|| invalid(format!("model {id} {key} must be a number")))
    };
    let read_f64 = |key: &str| -> Result<f64, SourceError> {
        obj.get(key)
            .and_then(Value::as_f64)
            .ok_or_else(|| invalid(format!("model {id} {key} must be a number")))
    };
    Ok(OpenPoolModelRef {
        path,
        offset_nm: [read_i64("x")?, read_i64("y")?, read_i64("z")?],
        rotate_deg: [read_f64("yaw")?, read_f64("pitch")?, read_f64("roll")?],
    })
}

/// Read a package JSON document and collect any STEP model paths it names.
///
/// # Rules
/// - `type` must be `package`.
/// - Only the `models` map is interpreted; geometry stays on the host.
///
/// # Do not
/// - Tessellate or draw the model; store the path for a later loader.
pub fn read_open_pool_package(text: &str) -> Result<OpenPoolPackageRecord, SourceError> {
    let root = parse_json(text)?;
    let kind = string_field(&root, "type")?;
    if kind != "package" {
        return Err(invalid("the JSON is not a package record"));
    }
    let uuid = string_field(&root, "uuid")?;
    check_uuid("uuid", &uuid)?;
    let name = string_field(&root, "name")?;
    check_text("name", &name)?;
    let default_model_id = root
        .get("default_model")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    if !default_model_id.is_empty() {
        check_uuid("default_model", &default_model_id)?;
    }
    let mut models = BTreeMap::new();
    if let Some(map) = root.get("models").and_then(Value::as_object) {
        if map.len() > MAX_KEYS {
            return Err(invalid("too many models on the package"));
        }
        for (id, entry) in map {
            check_uuid("model id", id)?;
            models.insert(id.clone(), model_ref(id, entry)?);
        }
    }
    Ok(OpenPoolPackageRecord {
        uuid,
        name,
        default_model_id,
        models,
    })
}

impl OpenPoolPackageRecord {
    /// The default STEP path named by the package, when one is set.
    pub fn default_step_path(&self) -> Option<&str> {
        if self.default_model_id.is_empty() {
            return None;
        }
        self.models
            .get(&self.default_model_id)
            .map(|m| m.path.as_str())
    }
}
