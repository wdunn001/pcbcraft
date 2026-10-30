//! Typed compliance facts on a placed part: where it was made, what role it plays
//! in the product, dated declarations against named regimes, and optional substance
//! notes. Project-level manufacturer aliases live on [`Project::manufacturers`].
//!
//! # Rules
//! - Plain data with bounded text; empty records are not saved on the component.
//! - Quick RoHS, REACH and halogen-free flags on [`crate::Sourcing`] appear as
//!   synthetic declarations only when no explicit declaration exists for that regime.
//!
//! # Dependencies
//! - `crates/doc/src/lib.rs` for `Component` and `Project`.
//! - `crates/engine/src/lib.rs` for `component.set_compliance` and `manufacturer.set`.

use crate::{Component, Project, Sourcing};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Longest regime id, status detail, source label, substance name, and similar text.
pub const MAX_COMPLIANCE_TEXT: usize = 256;
/// Longest declaration source link.
pub const MAX_COMPLIANCE_LINK: usize = 512;
/// Most declarations or declared substances on one part.
pub const MAX_DECLARATIONS: usize = 64;
pub const MAX_SUBSTANCES: usize = 64;
/// Most manufacturer records in the project table.
pub const MAX_MANUFACTURERS: usize = 256;
/// Most aliases on one manufacturer record.
pub const MAX_ALIASES: usize = 32;
/// Longest canonical manufacturer or alias name.
pub const MAX_MANUFACTURER_NAME: usize = 128;

/// ISO 3166-1 alpha-2 country code with the basis of the claim and an optional date.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CountryOfOrigin {
    /// Two-letter country code, such as `US` or `DE`.
    pub code: String,
    /// Why this code applies, such as `manufactured` or `substantial transformation`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub basis: String,
    /// Calendar date of the claim in `YYYY-MM-DD` form; empty when unknown.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub as_of: String,
}

/// The part's role in the product for drone and radio compliance rules.
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
    #[default]
    Other,
}

/// One compliance answer for a named regime.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Declaration {
    /// Stable regime id, such as `rohs` or `reach`; new regimes need no schema change.
    pub regime: String,
    pub status: DeclarationStatus,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub detail: String,
    pub source: DeclarationSource,
    /// Calendar date of the declaration in `YYYY-MM-DD` form; empty when unknown.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub as_of: String,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeclarationStatus {
    DeclaredCompliant,
    DeclaredExempt,
    DeclaredNoncompliant,
    NotApplicable,
    Unknown,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeclarationSourceKind {
    Manufacturer,
    Distributor,
    User,
    Imported,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DeclarationSource {
    pub kind: DeclarationSourceKind,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document_ref: Option<String>,
}

/// Optional free-form substance note on a part.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DeclaredSubstance {
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
}

/// Typed compliance record on one component. Empty records are not saved.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Compliance {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub country_of_origin: Option<CountryOfOrigin>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<PartRole>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub declarations: Vec<Declaration>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub substances: Vec<DeclaredSubstance>,
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
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub supplier_responsibility_on_file: bool,
}

fn bounded(name: &str, text: &str, max: usize) -> Result<(), String> {
    if text.len() > max || text.chars().any(char::is_control) {
        return Err(format!(
            "{name} must be at most {max} bytes without control characters"
        ));
    }
    Ok(())
}

fn valid_iso3166_alpha2(code: &str) -> bool {
    code.len() == 2
        && code
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_lowercase())
}

fn valid_date(date: &str) -> bool {
    date.is_empty()
        || (date.len() == 10
            && date.as_bytes()[4] == b'-'
            && date.as_bytes()[7] == b'-'
            && date.chars().all(|c| c.is_ascii_digit() || c == '-'))
}

/// Lowercase letters and digits only, for alias lookup.
pub fn normalize_manufacturer_name(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

impl CountryOfOrigin {
    pub fn validate(&self) -> Result<(), String> {
        let code = self.code.trim();
        if !valid_iso3166_alpha2(code) {
            return Err("country of origin code must be two letters".into());
        }
        bounded("country of origin basis", &self.basis, MAX_COMPLIANCE_TEXT)?;
        if !valid_date(&self.as_of) {
            return Err("country of origin date must be YYYY-MM-DD or empty".into());
        }
        Ok(())
    }
}

impl DeclarationSource {
    pub fn validate(&self) -> Result<(), String> {
        bounded("declaration source label", &self.label, MAX_COMPLIANCE_TEXT)?;
        if let Some(url) = &self.url {
            bounded("declaration source link", url, MAX_COMPLIANCE_LINK)?;
        }
        if let Some(doc) = &self.document_ref {
            bounded("declaration document reference", doc, MAX_COMPLIANCE_TEXT)?;
        }
        Ok(())
    }
}

impl Declaration {
    pub fn validate(&self) -> Result<(), String> {
        let regime = self.regime.trim();
        if regime.is_empty() || regime.len() > MAX_COMPLIANCE_TEXT {
            return Err("declaration regime must be 1 to 256 bytes".into());
        }
        bounded("declaration detail", &self.detail, MAX_COMPLIANCE_TEXT)?;
        if !valid_date(&self.as_of) {
            return Err("declaration date must be YYYY-MM-DD or empty".into());
        }
        self.source.validate()
    }
}

impl DeclaredSubstance {
    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("a declared substance needs a name".into());
        }
        bounded("substance name", &self.name, MAX_COMPLIANCE_TEXT)?;
        bounded("substance note", &self.note, MAX_COMPLIANCE_TEXT)?;
        Ok(())
    }
}

impl Compliance {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    pub fn validate(&self) -> Result<(), String> {
        if let Some(origin) = &self.country_of_origin {
            origin.validate()?;
        }
        if self.declarations.len() > MAX_DECLARATIONS {
            return Err(format!("a part carries at most {MAX_DECLARATIONS} declarations"));
        }
        let mut regimes = BTreeSet::new();
        for d in &self.declarations {
            d.validate()?;
            let key = d.regime.trim().to_ascii_lowercase();
            if !regimes.insert(key) {
                return Err("each declaration regime may appear once on a part".into());
            }
        }
        if self.substances.len() > MAX_SUBSTANCES {
            return Err(format!("a part lists at most {MAX_SUBSTANCES} substances"));
        }
        for s in &self.substances {
            s.validate()?;
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
        let country = self.country.trim();
        if !country.is_empty() && !valid_iso3166_alpha2(country) {
            return Err("manufacturer country must be two letters or empty".into());
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
            if key.is_empty() || !keys.insert(key.clone()) {
                return Err("manufacturer names must be unique after normalisation".into());
            }
            let mut alias_keys = BTreeSet::new();
            for alias in &record.aliases {
                let alias_key = normalize_manufacturer_name(alias);
                if alias_key.is_empty() || alias_key == key || !alias_keys.insert(alias_key) {
                    return Err("manufacturer aliases must be unique and differ from the name".into());
                }
            }
        }
        Ok(())
    }

    /// Resolve a maker name through the manufacturer table, including aliases.
    pub fn resolve_manufacturer(&self, name: &str) -> Option<&ManufacturerRecord> {
        self.manufacturers
            .iter()
            .find(|record| record.matches_name(name))
    }

    pub fn upsert_manufacturer(&mut self, record: ManufacturerRecord) -> Result<bool, String> {
        record.validate()?;
        self.validate_manufacturers_insert(&record)?;
        if let Some(existing) = self
            .manufacturers
            .iter_mut()
            .find(|r| normalize_manufacturer_name(&r.name) == normalize_manufacturer_name(&record.name))
        {
            *existing = record;
            return Ok(true);
        }
        if self.manufacturers.len() >= MAX_MANUFACTURERS {
            return Err(format!(
                "the manufacturer table holds at most {MAX_MANUFACTURERS} entries"
            ));
        }
        self.manufacturers.push(record);
        Ok(false)
    }

    fn validate_manufacturers_insert(&self, record: &ManufacturerRecord) -> Result<(), String> {
        let key = normalize_manufacturer_name(&record.name);
        for other in &self.manufacturers {
            if normalize_manufacturer_name(&other.name) == key {
                continue;
            }
            if other.matches_name(&record.name) {
                return Err("that name matches another manufacturer entry".into());
            }
            for alias in &record.aliases {
                if other.matches_name(alias) {
                    return Err("an alias matches another manufacturer entry".into());
                }
            }
        }
        Ok(())
    }
}

fn sourcing_declaration(regime: &str, flag: bool) -> Declaration {
    Declaration {
        regime: regime.to_owned(),
        status: if flag {
            DeclarationStatus::DeclaredCompliant
        } else {
            DeclarationStatus::DeclaredNoncompliant
        },
        detail: String::new(),
        source: DeclarationSource {
            kind: DeclarationSourceKind::User,
            label: String::new(),
            url: None,
            document_ref: None,
        },
        as_of: String::new(),
    }
}

impl Component {
    /// Declarations stored on the part plus synthetic ones from quick sourcing flags
    /// when no explicit declaration exists for that regime.
    pub fn effective_declarations(&self) -> Vec<Declaration> {
        self.effective_declarations_with_sourcing(&self.sourcing)
    }

    pub fn effective_declarations_with_sourcing(&self, sourcing: &Sourcing) -> Vec<Declaration> {
        let mut out = self.compliance.declarations.clone();
        let regimes: BTreeSet<String> = out
            .iter()
            .map(|d| d.regime.trim().to_ascii_lowercase())
            .collect();
        for (regime, value) in [
            ("rohs", sourcing.rohs),
            ("reach", sourcing.reach),
            ("halogen_free", sourcing.halogen_free),
        ] {
            if regimes.contains(regime) {
                continue;
            }
            if let Some(flag) = value {
                out.push(sourcing_declaration(regime, flag));
            }
        }
        out
    }

    /// Bill-of-materials column text for compliance keys; unset fields are empty strings.
    pub fn compliance_bom_value(&self, key: &str) -> Option<String> {
        let c = &self.compliance;
        match key {
            "Country of origin" => c.country_of_origin.as_ref().map(|o| o.code.to_ascii_uppercase()),
            "Country of origin basis" => c
                .country_of_origin
                .as_ref()
                .map(|o| o.basis.clone())
                .filter(|s| !s.is_empty()),
            "Country of origin date" => c
                .country_of_origin
                .as_ref()
                .map(|o| o.as_of.clone())
                .filter(|s| !s.is_empty()),
            "Part role" => c.role.map(part_role_text).map(str::to_owned),
            _ => None,
        }
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
        PartRole::Other => "other",
    }
}

/// Write distributor-sourced declarations onto a part after a listing is applied.
pub fn apply_listing_declarations(
    compliance: &mut Compliance,
    fetch_date: &str,
    distributor: &str,
    rohs: Option<bool>,
    reach: Option<bool>,
    halogen_free: Option<bool>,
) -> Result<(), String> {
    let label = distributor.trim();
    if label.is_empty() {
        return Err("a listing needs a distributor name for its declarations".into());
    }
    bounded("distributor", label, MAX_COMPLIANCE_TEXT)?;
    if !valid_date(fetch_date) {
        return Err("listing fetch date must be YYYY-MM-DD".into());
    }
    for (regime, value) in [
        ("rohs", rohs),
        ("reach", reach),
        ("halogen_free", halogen_free),
    ] {
        let Some(flag) = value else {
            continue;
        };
        let status = if flag {
            DeclarationStatus::DeclaredCompliant
        } else {
            DeclarationStatus::DeclaredNoncompliant
        };
        compliance.set_declaration(Declaration {
            regime: regime.to_owned(),
            status,
            detail: String::new(),
            source: DeclarationSource {
                kind: DeclarationSourceKind::Distributor,
                label: label.to_owned(),
                url: None,
                document_ref: None,
            },
            as_of: fetch_date.to_owned(),
        })?;
    }
    Ok(())
}

impl Compliance {
    pub fn set_declaration(&mut self, declaration: Declaration) -> Result<(), String> {
        declaration.validate()?;
        if let Some(existing) = self
            .declarations
            .iter_mut()
            .find(|d| d.regime.trim().eq_ignore_ascii_case(&declaration.regime))
        {
            *existing = declaration;
            return self.validate();
        }
        if self.declarations.len() >= MAX_DECLARATIONS {
            return Err(format!("a part carries at most {MAX_DECLARATIONS} declarations"));
        }
        self.declarations.push(declaration);
        self.validate()
    }
}
