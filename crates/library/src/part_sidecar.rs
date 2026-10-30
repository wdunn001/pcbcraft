//! Sidecar records for parameters, sourcing and alternates beside symbol and footprint files.
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const PART_SIDECAR_FORMAT: &str = "pcbcraft-part";
pub const PART_SIDECAR_VERSION: u32 = 1;
pub const PART_SIDECAR_EXTENSION: &str = ".pcbcraft-part.json";
const MAX_FIELDS: usize = 32;
const MAX_ALTERNATES: usize = 8;
const MAX_NAME: usize = 128;
const MAX_TEXT: usize = 256;
const MAX_NOTICE: usize = 8 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartSidecarField {
    pub key: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartAlternate {
    pub manufacturer: String,
    pub mpn: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub datasheet: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub distributor: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub sku: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartProvenance {
    pub notice: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source_project: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartSidecar {
    pub format: String,
    pub version: u32,
    pub symbol: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub footprint: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub value: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<PartSidecarField>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alternates: Vec<PartAlternate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<PartProvenance>,
}

impl PartSidecar {
    pub fn validate(&self) -> Result<(), String> {
        if self.format != PART_SIDECAR_FORMAT {
            return Err("part sidecar format must be pcbcraft-part".into());
        }
        if self.version != PART_SIDECAR_VERSION {
            return Err(format!(
                "part sidecar version must be {PART_SIDECAR_VERSION}"
            ));
        }
        bounded_name("symbol", &self.symbol)?;
        if !self.footprint.is_empty() {
            bounded_name("footprint", &self.footprint)?;
        }
        if self.value.len() > MAX_TEXT {
            return Err("part sidecar value is too long".into());
        }
        if self.kind.len() > MAX_TEXT {
            return Err("part sidecar kind is too long".into());
        }
        if self.fields.len() > MAX_FIELDS {
            return Err("part sidecar supports at most 32 fields".into());
        }
        let mut keys = std::collections::BTreeSet::new();
        for field in &self.fields {
            if field.key.is_empty() || field.key.len() > 64 {
                return Err("part sidecar field key is out of range".into());
            }
            if field.value.len() > MAX_TEXT {
                return Err("part sidecar field value is too long".into());
            }
            if !keys.insert(field.key.as_str()) {
                return Err("part sidecar field keys must be unique".into());
            }
        }
        if self.alternates.len() > MAX_ALTERNATES {
            return Err("part sidecar supports at most 8 alternates".into());
        }
        for alt in &self.alternates {
            validate_alternate(alt)?;
        }
        if let Some(provenance) = &self.provenance {
            if provenance.notice.is_empty() || provenance.notice.len() > MAX_NOTICE {
                return Err("part sidecar provenance notice is out of range".into());
            }
            if provenance.source_project.len() > MAX_TEXT {
                return Err("part sidecar source project name is too long".into());
            }
        }
        Ok(())
    }
}

fn bounded_name(label: &str, name: &str) -> Result<(), String> {
    if name.is_empty() || name.len() > MAX_NAME {
        return Err(format!("{label} name is out of range"));
    }
    Ok(())
}

fn validate_alternate(alt: &PartAlternate) -> Result<(), String> {
    if alt.manufacturer.trim().is_empty() || alt.manufacturer.len() > MAX_TEXT {
        return Err("alternate manufacturer is out of range".into());
    }
    if alt.mpn.trim().is_empty() || alt.mpn.len() > MAX_TEXT {
        return Err("alternate mpn is out of range".into());
    }
    if alt.datasheet.len() > MAX_TEXT || alt.distributor.len() > 48 || alt.sku.len() > MAX_TEXT {
        return Err("alternate field is out of range".into());
    }
    Ok(())
}

pub fn sidecar_path_in_folder(folder: &Path, symbol_name: &str) -> PathBuf {
    folder.join(format!("{symbol_name}{PART_SIDECAR_EXTENSION}"))
}

pub fn read_part_sidecar(path: &Path) -> Result<PartSidecar, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if text.len() > 256 * 1024 {
        return Err("part sidecar exceeds 256 KiB".into());
    }
    let sidecar: PartSidecar = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    sidecar.validate()?;
    Ok(sidecar)
}

pub fn write_part_sidecar(path: &Path, sidecar: &PartSidecar) -> Result<(), String> {
    sidecar.validate()?;
    let text = serde_json::to_string_pretty(sidecar).map_err(|e| e.to_string())?;
    if text.len() > 256 * 1024 {
        return Err("part sidecar exceeds 256 KiB".into());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("part-sidecar.tmp");
    std::fs::write(&tmp, text.as_bytes()).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())?;
    Ok(())
}
