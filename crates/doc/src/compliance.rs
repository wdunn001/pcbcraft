//! Typed compliance facts on a placed part and the project manufacturer table.
//!
//! # Rules
//! - Plain data with bounded text; empty compliance records are not saved on the component.
//!
//! # Dependencies
//! - `crates/doc/src/lib.rs` for `Component` and `Project`.

use crate::Project;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const MAX_COMPLIANCE_TEXT: usize = 256;
pub const MAX_MANUFACTURERS: usize = 256;
pub const MAX_ALIASES: usize = 32;
pub const MAX_MANUFACTURER_NAME: usize = 128;

/// The part's role in the product for regulatory rule sets.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PartRole {
    FlightController,
    Esc,
    RadioLink,
    Camera,
    Gimbal,
    Gnss,
    Battery,
    Motor,
    CompanionComputer,
    Processor,
    Memory,
    #[default]
    Other,
}

/// Compliance record on one component. Empty records are not saved.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Compliance {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<PartRole>,
    /// Fabricator of the silicon die when the catalog brand differs from the die vendor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub silicon_vendor: Option<String>,
}

/// One row of the project manufacturer table.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ManufacturerRecord {
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub country: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ComplianceProjectSettings {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rule_sets: Vec<String>,
}

impl ComplianceProjectSettings {
    pub fn is_default(&self) -> bool {
        self.rule_sets.is_empty()
    }
}

fn bounded(name: &str, text: &str, max: usize) -> Result<(), String> {
    if text.len() > max || text.chars().any(char::is_control) {
        return Err(format!(
            "{name} must be at most {max} bytes without control characters"
        ));
    }
    Ok(())
}

pub fn normalize_manufacturer_name(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

impl Compliance {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    pub fn validate(&self) -> Result<(), String> {
        if let Some(vendor) = &self.silicon_vendor {
            bounded("silicon vendor", vendor, MAX_COMPLIANCE_TEXT)?;
        }
        Ok(())
    }
}

impl ManufacturerRecord {
    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() || self.name.len() > MAX_MANUFACTURER_NAME {
            return Err("manufacturer name must be 1 to 128 bytes".into());
        }
        if self.aliases.len() > MAX_ALIASES {
            return Err(format!("a manufacturer lists at most {MAX_ALIASES} aliases"));
        }
        for alias in &self.aliases {
            if alias.trim().is_empty() || alias.len() > MAX_MANUFACTURER_NAME {
                return Err("manufacturer aliases must be 1 to 128 bytes".into());
            }
        }
        if let Some(parent) = &self.parent {
            bounded("manufacturer parent", parent, MAX_MANUFACTURER_NAME)?;
        }
        Ok(())
    }

    pub fn matches_name(&self, name: &str) -> bool {
        let key = normalize_manufacturer_name(name);
        if key.is_empty() {
            return false;
        }
        if normalize_manufacturer_name(&self.name) == key {
            return true;
        }
        self.aliases
            .iter()
            .any(|a| normalize_manufacturer_name(a) == key)
    }
}

impl Project {
    pub fn validate_manufacturers(&self) -> Result<(), String> {
        if self.manufacturers.len() > MAX_MANUFACTURERS {
            return Err(format!(
                "the manufacturer table holds at most {MAX_MANUFACTURERS} entries"
            ));
        }
        let mut keys = BTreeSet::new();
        for record in &self.manufacturers {
            record.validate()?;
            let key = normalize_manufacturer_name(&record.name);
            if key.is_empty() || !keys.insert(key) {
                return Err("manufacturer names must be unique after normalisation".into());
            }
        }
        Ok(())
    }

    pub fn resolve_manufacturer(&self, name: &str) -> Option<&ManufacturerRecord> {
        self.manufacturers
            .iter()
            .find(|record| record.matches_name(name))
    }
}

pub fn part_role_text(role: PartRole) -> &'static str {
    match role {
        PartRole::FlightController => "flight controller",
        PartRole::Esc => "esc",
        PartRole::RadioLink => "radio link",
        PartRole::Camera => "camera",
        PartRole::Gimbal => "gimbal",
        PartRole::Gnss => "gnss",
        PartRole::Battery => "battery",
        PartRole::Motor => "motor",
        PartRole::CompanionComputer => "companion computer",
        PartRole::Processor => "processor",
        PartRole::Memory => "memory",
        PartRole::Other => "other",
    }
}
