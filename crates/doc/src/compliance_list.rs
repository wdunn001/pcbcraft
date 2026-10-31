//! Dated compliance list snapshots: entity and substance entries with source
//! metadata. Rule engines match part and manufacturer names against these
//! files; list text never belongs in Rust source.
//!
//! # Rules
//! - Pure data and bounds checks. TOML lives in crates/io/src/compliance_list.rs.
//! - `schema` must be `1` for files this release reads.
//!
//! # Dependencies
//! - compliance-lists/SCHEMA.md
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const COMPLIANCE_LIST_SCHEMA: u32 = 1;

const MAX_ID_LEN: usize = 64;
const MAX_TITLE_LEN: usize = 256;
const MAX_URL_LEN: usize = 2048;
const MAX_LICENCE_LEN: usize = 256;
const MAX_DATE_LEN: usize = 32;
const MAX_NAME_LEN: usize = 256;
const MAX_ALIAS_COUNT: usize = 32;
const MAX_ALIAS_LEN: usize = 256;
const MAX_IDENTIFIER_COUNT: usize = 16;
const MAX_IDENTIFIER_LEN: usize = 128;
const MAX_COUNTRY_LEN: usize = 8;
const MAX_PARENT_LEN: usize = 256;
const MAX_CITATION_LEN: usize = 512;
const MAX_CAS_LEN: usize = 32;
const MAX_EC_LEN: usize = 32;
pub const MAX_COMPLIANCE_LIST_ENTRIES: usize = 8192;
pub const MAX_COMPLIANCE_LIST_BYTES: usize = 2 * 1024 * 1024;

fn valid_slug(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_ID_LEN
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn bounded(s: &str, max: usize) -> bool {
    !s.is_empty() && s.len() <= max
}

fn optional_bounded(s: &str, max: usize) -> bool {
    s.len() <= max
}

fn valid_date(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= MAX_DATE_LEN
        && s.bytes().all(|b| b.is_ascii_digit() || b == b'-')
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ComplianceListEntityEntry {
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub identifiers: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub country: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub parent: String,
    #[serde(default, rename = "added")]
    pub added_date: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub citation: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ComplianceListSubstanceEntry {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub cas: String,
    #[serde(default, rename = "ec", skip_serializing_if = "String::is_empty")]
    pub ec_number: String,
    pub name: String,
    #[serde(default, rename = "added")]
    pub added_date: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ComplianceListSnapshot {
    pub schema: u32,
    pub id: String,
    pub title: String,
    #[serde(default, rename = "source_url", skip_serializing_if = "String::is_empty")]
    pub source_url: String,
    #[serde(default, rename = "published", skip_serializing_if = "String::is_empty")]
    pub published_date: String,
    #[serde(rename = "retrieved")]
    pub retrieved_date: String,
    pub licence: String,
    #[serde(default)]
    pub cadence_days: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entities: Vec<ComplianceListEntityEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub substances: Vec<ComplianceListSubstanceEntry>,
}

impl ComplianceListEntityEntry {
    pub fn validate(&self) -> Result<(), String> {
        if !bounded(&self.name, MAX_NAME_LEN) {
            return Err("each entity needs a name of 1 to 256 bytes".into());
        }
        if self.aliases.len() > MAX_ALIAS_COUNT {
            return Err("each entity lists at most 32 aliases".into());
        }
        for alias in &self.aliases {
            if !bounded(alias, MAX_ALIAS_LEN) {
                return Err("each entity alias must be 1 to 256 bytes".into());
            }
        }
        if self.identifiers.len() > MAX_IDENTIFIER_COUNT {
            return Err("each entity lists at most 16 identifiers".into());
        }
        for id in &self.identifiers {
            if !bounded(id, MAX_IDENTIFIER_LEN) {
                return Err("each entity identifier must be 1 to 128 bytes".into());
            }
        }
        if !optional_bounded(&self.country, MAX_COUNTRY_LEN) {
            return Err("entity country must be at most 8 bytes".into());
        }
        if !optional_bounded(&self.parent, MAX_PARENT_LEN) {
            return Err("entity parent must be at most 256 bytes".into());
        }
        if !self.added_date.is_empty() && !valid_date(&self.added_date) {
            return Err("entity added date must look like YYYY-MM-DD".into());
        }
        if !optional_bounded(&self.citation, MAX_CITATION_LEN) {
            return Err("entity citation must be at most 512 bytes".into());
        }
        Ok(())
    }
}

impl ComplianceListSubstanceEntry {
    pub fn validate(&self) -> Result<(), String> {
        if self.cas.is_empty() && self.ec_number.is_empty() && self.name.is_empty() {
            return Err("each substance needs a CAS number, EC number or name".into());
        }
        if !optional_bounded(&self.cas, MAX_CAS_LEN) {
            return Err("substance CAS must be at most 32 bytes".into());
        }
        if !optional_bounded(&self.ec_number, MAX_EC_LEN) {
            return Err("substance EC number must be at most 32 bytes".into());
        }
        if !self.name.is_empty() && !bounded(&self.name, MAX_NAME_LEN) {
            return Err("substance name must be 1 to 256 bytes when set".into());
        }
        if !self.added_date.is_empty() && !valid_date(&self.added_date) {
            return Err("substance added date must look like YYYY-MM-DD".into());
        }
        Ok(())
    }
}

impl ComplianceListSnapshot {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema != COMPLIANCE_LIST_SCHEMA {
            return Err(format!(
                "compliance list schema must be {}",
                COMPLIANCE_LIST_SCHEMA
            ));
        }
        if !valid_slug(&self.id) {
            return Err("compliance list id must be 1 to 64 bytes of a-z, 0-9 and hyphen".into());
        }
        if !bounded(&self.title, MAX_TITLE_LEN) {
            return Err("compliance list title must be 1 to 256 bytes".into());
        }
        if !optional_bounded(&self.source_url, MAX_URL_LEN) {
            return Err("compliance list source url must be at most 2048 bytes".into());
        }
        if !self.published_date.is_empty() && !valid_date(&self.published_date) {
            return Err("compliance list published date must look like YYYY-MM-DD".into());
        }
        if !valid_date(&self.retrieved_date) {
            return Err("compliance list retrieved date must look like YYYY-MM-DD".into());
        }
        if !bounded(&self.licence, MAX_LICENCE_LEN) {
            return Err("compliance list licence must be 1 to 256 bytes".into());
        }
        let entry_count = self.entities.len() + self.substances.len();
        if entry_count > MAX_COMPLIANCE_LIST_ENTRIES {
            return Err(format!(
                "compliance list holds at most {MAX_COMPLIANCE_LIST_ENTRIES} entries"
            ));
        }
        let mut entity_keys = BTreeSet::new();
        for entity in &self.entities {
            entity.validate()?;
            let key = entity.name.trim().to_lowercase();
            if !entity_keys.insert(key) {
                return Err("compliance list entity names must be unique".into());
            }
        }
        let mut substance_keys = BTreeSet::new();
        for substance in &self.substances {
            substance.validate()?;
            let key = if !substance.cas.is_empty() {
                format!("cas:{}", substance.cas.trim().to_uppercase())
            } else if !substance.ec_number.is_empty() {
                format!("ec:{}", substance.ec_number.trim().to_uppercase())
            } else {
                format!("name:{}", substance.name.trim().to_lowercase())
            };
            if !substance_keys.insert(key) {
                return Err("compliance list substance entries must be unique".into());
            }
        }
        Ok(())
    }

    /// True when bundled metadata is complete enough to ship.
    pub fn bundled_metadata_ok(&self) -> bool {
        !self.source_url.is_empty()
            && !self.retrieved_date.is_empty()
            && !self.licence.is_empty()
    }
}
