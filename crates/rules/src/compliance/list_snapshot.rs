//! Dated entity list snapshots for party screening (schema aligned with issue #690).
use serde::Deserialize;
use std::collections::BTreeSet;

const MAX_LIST_ENTRIES: usize = 4096;
const MAX_ENTRY_NAME: usize = 256;
const MAX_ALIASES: usize = 16;

/// One named party on a screening list.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EntityListEntry {
    /// Primary name used for matching.
    pub name: String,
    /// Alternate spellings treated as the same party.
    #[serde(default)]
    pub aliases: Vec<String>,
    /// Country code recorded in the list file.
    #[serde(default)]
    pub country: String,
    /// Citation line copied into findings.
    #[serde(default)]
    pub citation: String,
}

/// A dated screening or entity list loaded from TOML.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EntityListSnapshot {
    /// File schema version.
    pub schema: u32,
    /// List id referenced by rule sets.
    pub id: String,
    /// Human title for reports.
    pub title: String,
    /// Where the list was published.
    pub source_url: String,
    /// Publication date on the source.
    pub published: String,
    /// Date this copy was retrieved.
    pub retrieved: String,
    /// Licence under which the list file is redistributed.
    pub licence: String,
    /// Expected refresh interval in days.
    pub cadence_days: u32,
    /// Parties on the list.
    #[serde(default)]
    pub entries: Vec<EntityListEntry>,
}

impl EntityListSnapshot {
    /// Parse and validate a list snapshot TOML document.
    pub fn parse(text: &str) -> Result<Self, String> {
        let snap: Self = toml::from_str(text).map_err(|e| format!("list snapshot parse: {e}"))?;
        snap.validate()?;
        Ok(snap)
    }

    /// Check schema, ids and entry bounds.
    pub fn validate(&self) -> Result<(), String> {
        if self.schema != 1 {
            return Err("list snapshot schema must be 1".into());
        }
        if self.id.is_empty() || self.id.len() > 64 {
            return Err("list snapshot id must be 1 to 64 bytes".into());
        }
        if self.entries.len() > MAX_LIST_ENTRIES {
            return Err("list snapshot entry limit exceeded".into());
        }
        for entry in &self.entries {
            if entry.name.is_empty() || entry.name.len() > MAX_ENTRY_NAME {
                return Err("list entry name must be 1 to 256 bytes".into());
            }
            if entry.aliases.len() > MAX_ALIASES {
                return Err("list entry alias limit exceeded".into());
            }
            for alias in &entry.aliases {
                if alias.len() > MAX_ENTRY_NAME {
                    return Err("list entry alias too long".into());
                }
            }
        }
        Ok(())
    }

    /// True when `today` is later than retrieved plus cadence.
    pub fn is_stale(&self, today: &str) -> bool {
        if self.cadence_days == 0 {
            return false;
        }
        days_since(&self.retrieved, today) > self.cadence_days as i64
    }

    /// Exact normalized name or alias match for a manufacturer string.
    pub fn match_manufacturer(&self, name: &str) -> Option<&EntityListEntry> {
        let needle = normalize_name(name);
        if needle.is_empty() {
            return None;
        }
        for entry in &self.entries {
            if normalize_name(&entry.name) == needle {
                return Some(entry);
            }
            for alias in &entry.aliases {
                if normalize_name(alias) == needle {
                    return Some(entry);
                }
            }
        }
        None
    }
}

/// Lowercase alphanumeric form used for list name equality.
pub fn normalize_name(name: &str) -> String {
    let mut out = String::new();
    let mut last_space = false;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            for lower in ch.to_lowercase() {
                out.push(lower);
            }
            last_space = false;
        } else if ch.is_whitespace() {
            if !last_space && !out.is_empty() {
                out.push(' ');
                last_space = true;
            }
        }
    }
    out.trim().to_string()
}

fn days_since(from: &str, to: &str) -> i64 {
    if from.len() != 10 || to.len() != 10 {
        return i64::MAX;
    }
    let from_days = parse_iso_days(from);
    let to_days = parse_iso_days(to);
    match (from_days, to_days) {
        (Some(a), Some(b)) => b.saturating_sub(a),
        _ => i64::MAX,
    }
}

fn parse_iso_days(iso: &str) -> Option<i64> {
    let year: i64 = iso.get(0..4)?.parse().ok()?;
    let month: i64 = iso.get(5..7)?.parse().ok()?;
    let day: i64 = iso.get(8..10)?.parse().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some(year * 372 + month * 31 + day)
}

/// Names already present in bundled lists, for CI guard tests.
pub fn bundled_list_tokens() -> BTreeSet<String> {
    let mut tokens = BTreeSet::new();
    if let Ok(snap) = EntityListSnapshot::parse(include_str!(
        "../../data/compliance/lists/party_screening_fixture.toml"
    )) {
        for entry in snap.entries {
            tokens.insert(entry.name);
            for alias in entry.aliases {
                tokens.insert(alias);
            }
        }
    }
    tokens
}
