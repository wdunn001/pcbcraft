//! Typed fields for distributor source data on a placed component.
//!
//! The engine `part.attach_source` command writes these fields. Projects saved
//! with Manufacturer, MPN and distributor SKU properties still load because
//! migration copies empty typed slots from the legacy property names.
//!
//! # Rules
//! - Text lengths and orderable counts match the engine command bounds.
//! - Legacy properties stay on the component after migration; typed fields win on read.
//!
//! # Dependencies
//! - crates/engine/src/lib.rs (`part.attach_source`).
//! - crates/io/src/generate.rs `migrate_loaded_project`.
//!
//! # Do not
//! - Store live network credentials here; only catalogue answers belong on the record.
use serde::{Deserialize, Serialize};

/// Most distributor order lines stored on one component.
pub const MAX_SOURCE_ORDERABLES: usize = 16;
const MAX_TEXT: usize = 256;
const MAX_DISTRIBUTOR: usize = 48;
const MAX_URL: usize = 512;

/// One distributor line: SKU, product link and stock note.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceOrderable {
    pub distributor: String,
    pub sku: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub stock: String,
}

/// Datasheet URL and when the source last returned it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceDatasheet {
    pub url: String,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub retrieved_at: u64,
}

/// Lifecycle text and when it was last checked.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceLifecycle {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub status: String,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub checked_at: u64,
}

/// Manufacturer, orderable lines and freshness from an online part source.
///
/// # Rules
/// - At most `MAX_SOURCE_ORDERABLES` order lines; distributor names are unique per component.
/// - Datasheet and lifecycle timestamps are Unix seconds from the source answer.
///
/// # Dependencies
/// - crates/library/src/sources.rs `PartResult` (filled by `part.attach_source`).
///
/// # Do not
/// - Treat empty strings as missing typed data during migration; only lift from properties when typed slots are empty.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartSourceRecord {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub manufacturer: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub mpn: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub orderables: Vec<SourceOrderable>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub datasheet: Option<SourceDatasheet>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifecycle: Option<SourceLifecycle>,
}

fn is_zero(v: &u64) -> bool {
    *v == 0
}

fn bounded(name: &str, v: &str, max: usize) -> Result<(), String> {
    if v.len() > max || v.chars().any(char::is_control) {
        return Err(format!(
            "{name} must be at most {max} bytes without control characters"
        ));
    }
    Ok(())
}

impl PartSourceRecord {
    /// True when every field is at its default.
    pub fn is_empty(&self) -> bool {
        self.manufacturer.is_empty()
            && self.mpn.is_empty()
            && self.orderables.is_empty()
            && self.datasheet.is_none()
            && self.lifecycle.is_none()
    }

    /// Length and duplicate checks used by project validation.
    pub fn validate(&self) -> Result<(), String> {
        bounded("manufacturer", &self.manufacturer, MAX_TEXT)?;
        bounded("mpn", &self.mpn, MAX_TEXT)?;
        if self.orderables.len() > MAX_SOURCE_ORDERABLES {
            return Err("a component carries at most 16 source orderables".into());
        }
        let mut distributors = std::collections::BTreeSet::new();
        for (i, o) in self.orderables.iter().enumerate() {
            let label = format!("orderables[{i}].distributor");
            bounded(&label, &o.distributor, MAX_DISTRIBUTOR)?;
            bounded(&format!("orderables[{i}].sku"), &o.sku, MAX_TEXT)?;
            bounded(&format!("orderables[{i}].url"), &o.url, MAX_URL)?;
            bounded(&format!("orderables[{i}].stock"), &o.stock, MAX_TEXT)?;
            if !o.distributor.is_empty() && !distributors.insert(o.distributor.to_ascii_lowercase())
            {
                return Err("duplicate source distributor on one component".into());
            }
        }
        if let Some(d) = &self.datasheet {
            bounded("datasheet.url", &d.url, MAX_URL)?;
        }
        if let Some(l) = &self.lifecycle {
            bounded("lifecycle.status", &l.status, MAX_TEXT)?;
        }
        Ok(())
    }
}

/// Named property value from typed source data when no free-form field uses that key.
pub(crate) fn part_source_field<'a>(record: &'a PartSourceRecord, key: &str) -> Option<&'a str> {
    match key {
        "Manufacturer" | "manufacturer" if !record.manufacturer.is_empty() => {
            Some(record.manufacturer.as_str())
        }
        "MPN" | "mpn" | "Manufacturer Part Number" if !record.mpn.is_empty() => {
            Some(record.mpn.as_str())
        }
        "Datasheet" | "datasheet" => record
            .datasheet
            .as_ref()
            .map(|d| d.url.as_str())
            .filter(|u| !u.is_empty()),
        "Lifecycle" | "lifecycle" => record
            .lifecycle
            .as_ref()
            .map(|l| l.status.as_str())
            .filter(|s| !s.is_empty()),
        _ if key.eq_ignore_ascii_case("SKU") => record
            .orderables
            .first()
            .map(|o| o.sku.as_str())
            .filter(|s| !s.is_empty()),
        _ => {
            let suffix = key.strip_suffix(" SKU").or_else(|| key.strip_suffix(" sku"))?;
            let distributor = suffix.trim();
            record.orderables.iter().find_map(|o| {
                if o.distributor.eq_ignore_ascii_case(distributor) && !o.sku.is_empty() {
                    Some(o.sku.as_str())
                } else {
                    None
                }
            })
        }
    }
}

impl super::Component {
    /// Copy legacy property names into typed source fields when those fields are still empty.
    pub fn adopt_property_part_source(&mut self) {
        if self.part_source.manufacturer.is_empty()
            && let Some(v) = self.field("Manufacturer").or(self.field("manufacturer"))
        {
            self.part_source.manufacturer = v.to_string();
        }
        if self.part_source.mpn.is_empty()
            && let Some(v) = self
                .field("MPN")
                .or(self.field("mpn"))
                .or(self.field("Manufacturer Part Number"))
        {
            self.part_source.mpn = v.to_string();
        }
        if self.part_source.datasheet.is_none()
            && let Some(url) = self.field("Datasheet").or(self.field("datasheet"))
            && !url.is_empty()
        {
            self.part_source.datasheet = Some(SourceDatasheet {
                url: url.to_string(),
                retrieved_at: 0,
            });
        }
        if self.part_source.lifecycle.is_none()
            && let Some(status) = self.field("Lifecycle").or(self.field("lifecycle"))
            && !status.is_empty()
        {
            self.part_source.lifecycle = Some(SourceLifecycle {
                status: status.to_string(),
                checked_at: 0,
            });
        }
        if self.part_source.orderables.is_empty() {
            for f in &self.fields {
                let key = f.key.as_str();
                if key.eq_ignore_ascii_case("SKU") && !f.value.is_empty() {
                    self.part_source.orderables.push(SourceOrderable {
                        distributor: String::new(),
                        sku: f.value.clone(),
                        url: String::new(),
                        stock: String::new(),
                    });
                    continue;
                }
                if let Some(distributor) = key.strip_suffix(" SKU").or_else(|| key.strip_suffix(" sku"))
                {
                    if !f.value.is_empty() {
                        self.part_source.orderables.push(SourceOrderable {
                            distributor: distributor.trim().to_string(),
                            sku: f.value.clone(),
                            url: String::new(),
                            stock: String::new(),
                        });
                    }
                }
            }
        }
    }
}

impl super::Project {
    /// Lift legacy Manufacturer or MPN properties into typed source fields.
    pub fn migrate_component_part_sources(&mut self) {
        for c in &mut self.components {
            c.adopt_property_part_source();
        }
    }
}
