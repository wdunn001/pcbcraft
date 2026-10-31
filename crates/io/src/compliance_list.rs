//! TOML import for compliance list snapshots, bundled presets, entity matching,
//! and staleness notes.
//!
//! # Rules
//! - TOML is parsed here only; validation types live in crates/doc.
//! - Bundled lists are compiled from compliance-lists/ at the repo root.
//!
//! # Dependencies
//! - crates/doc/src/compliance_list.rs
//! - compliance-lists/SCHEMA.md
use pcbcraft_doc::{
    COMPLIANCE_LIST_SCHEMA, ComplianceListEntityEntry, ComplianceListSnapshot,
    ComplianceListSubstanceEntry, MAX_COMPLIANCE_LIST_BYTES,
};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};

const BUILTIN_IDS: [&str; 1] = ["synthetic-demo-entities"];

pub fn builtin_compliance_list_ids() -> &'static [&'static str] {
    &BUILTIN_IDS
}

pub fn builtin_compliance_list(id: &str) -> Result<ComplianceListSnapshot, String> {
    let text = match id {
        "synthetic-demo-entities" => {
            include_str!("../../../compliance-lists/synthetic-demo-entities.toml")
        }
        _ => return Err("unknown built-in compliance list".into()),
    };
    compliance_list_from_toml(text)
}

pub fn compliance_list_from_toml(text: &str) -> Result<ComplianceListSnapshot, String> {
    if text.len() > MAX_COMPLIANCE_LIST_BYTES {
        return Err(format!(
            "compliance list file is larger than {MAX_COMPLIANCE_LIST_BYTES} bytes"
        ));
    }
    let parsed: toml::Value = toml::from_str(text).map_err(|e| e.to_string())?;
    let json = toml_value_to_json(parsed);
    let snapshot: ComplianceListSnapshot =
        serde_json::from_value(json).map_err(|e| e.to_string())?;
    if snapshot.schema != COMPLIANCE_LIST_SCHEMA {
        return Err(format!(
            "compliance list schema must be {COMPLIANCE_LIST_SCHEMA}"
        ));
    }
    snapshot.validate()?;
    Ok(snapshot)
}

pub fn compliance_list_to_toml(snapshot: &ComplianceListSnapshot) -> Result<String, String> {
    snapshot.validate()?;
    let json = serde_json::to_value(snapshot).map_err(|e| e.to_string())?;
    let toml_value = json_value_to_toml(json);
    toml::to_string_pretty(&toml_value).map_err(|e| e.to_string())
}

/// Normalised form for exact name and alias comparison.
pub fn normalize_compliance_name(name: &str) -> String {
    let lower = name.trim().to_lowercase();
    let mut out = String::with_capacity(lower.len());
    let mut last_space = false;
    for ch in lower.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            last_space = false;
        } else if !last_space {
            out.push(' ');
            last_space = true;
        }
    }
    out.trim().to_owned()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EntityMatchConfidence {
    Exact,
    Possible,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntityMatch {
    pub entry_name: String,
    pub confidence: EntityMatchConfidence,
}

/// Match a manufacturer or entity name against list entries. Ownership is never
/// inferred beyond the `parent` field stored in the entry.
pub fn match_entity_name(
    snapshot: &ComplianceListSnapshot,
    query: &str,
) -> Option<EntityMatch> {
    let normalized = normalize_compliance_name(query);
    if normalized.is_empty() {
        return None;
    }
    for entry in &snapshot.entities {
        if normalize_compliance_name(&entry.name) == normalized {
            return Some(EntityMatch {
                entry_name: entry.name.clone(),
                confidence: EntityMatchConfidence::Exact,
            });
        }
        for alias in &entry.aliases {
            if normalize_compliance_name(alias) == normalized {
                return Some(EntityMatch {
                    entry_name: entry.name.clone(),
                    confidence: EntityMatchConfidence::Exact,
                });
            }
        }
    }
    for entry in &snapshot.entities {
        if near_name_match(&normalized, &normalize_compliance_name(&entry.name)) {
            return Some(EntityMatch {
                entry_name: entry.name.clone(),
                confidence: EntityMatchConfidence::Possible,
            });
        }
        for alias in &entry.aliases {
            if near_name_match(&normalized, &normalize_compliance_name(alias)) {
                return Some(EntityMatch {
                    entry_name: entry.name.clone(),
                    confidence: EntityMatchConfidence::Possible,
                });
            }
        }
    }
    None
}

fn near_name_match(a: &str, b: &str) -> bool {
    if a == b || a.is_empty() || b.is_empty() {
        return false;
    }
    let max_len = a.len().max(b.len());
    if max_len < 4 {
        return false;
    }
    let distance = levenshtein(a.as_bytes(), b.as_bytes());
    distance <= 2 && distance * 4 <= max_len
}

fn levenshtein(a: &[u8], b: &[u8]) -> usize {
    let n = a.len();
    let m = b.len();
    if n == 0 {
        return m;
    }
    if m == 0 {
        return n;
    }
    let mut prev: Vec<usize> = (0..=m).collect();
    let mut cur = vec![0; m + 1];
    for (i, &byte_a) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, &byte_b) in b.iter().enumerate() {
            let cost = if byte_a == byte_b { 0 } else { 1 };
            cur[j + 1] = (prev[j + 1] + 1)
                .min(cur[j] + 1)
                .min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[m]
}

/// Days since `retrieved` if the date parses as YYYY-MM-DD.
pub fn days_since_retrieved(retrieved: &str, today: &str) -> Option<u32> {
    let (ry, rm, rd) = parse_ymd(retrieved)?;
    let (ty, tm, td) = parse_ymd(today)?;
    let retrieved_ord = ymd_to_ordinal(ry, rm, rd)?;
    let today_ord = ymd_to_ordinal(ty, tm, td)?;
    today_ord.checked_sub(retrieved_ord)
}

fn parse_ymd(s: &str) -> Option<(u32, u32, u32)> {
    let mut parts = s.split('-');
    let y: u32 = parts.next()?.parse().ok()?;
    let m: u32 = parts.next()?.parse().ok()?;
    let d: u32 = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((y, m, d))
}

fn ymd_to_ordinal(y: u32, m: u32, d: u32) -> Option<u32> {
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    let mut ordinal = d;
    for month in 1..m {
        ordinal += days_in_month(y, month);
    }
    for year in 1970..y {
        ordinal += if is_leap(year) { 366 } else { 365 };
    }
    Some(ordinal)
}

fn days_in_month(y: u32, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap(y) {
                29
            } else {
                28
            }
        }
        _ => 0,
    }
}

fn is_leap(y: u32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

/// Staleness note when `cadence_days` is set and the snapshot is older than that.
pub fn snapshot_staleness_note(snapshot: &ComplianceListSnapshot, today: &str) -> Option<String> {
    if snapshot.cadence_days == 0 {
        return None;
    }
    let age = days_since_retrieved(&snapshot.retrieved_date, today)?;
    if age > snapshot.cadence_days {
        Some(format!(
            "List {} was retrieved on {}; it is {} days old and its cadence is {} days.",
            snapshot.id, snapshot.retrieved_date, age, snapshot.cadence_days
        ))
    } else {
        None
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ListUpdateSummary {
    pub list_id: String,
    pub entities_added: Vec<String>,
    pub entities_removed: Vec<String>,
    pub substances_added: Vec<String>,
    pub substances_removed: Vec<String>,
}

pub fn entity_entry_key(entry: &ComplianceListEntityEntry) -> String {
    normalize_compliance_name(&entry.name)
}

pub fn substance_entry_key(entry: &ComplianceListSubstanceEntry) -> String {
    if !entry.cas.is_empty() {
        format!("cas:{}", entry.cas.trim().to_uppercase())
    } else if !entry.ec_number.is_empty() {
        format!("ec:{}", entry.ec_number.trim().to_uppercase())
    } else {
        format!("name:{}", normalize_compliance_name(&entry.name))
    }
}

pub fn diff_compliance_lists(
    before: &ComplianceListSnapshot,
    after: &ComplianceListSnapshot,
) -> Result<ListUpdateSummary, String> {
    if before.id != after.id {
        return Err("compliance list update id must match the installed list".into());
    }
    let mut summary = ListUpdateSummary {
        list_id: after.id.clone(),
        ..Default::default()
    };
    let before_entities: BTreeMap<_, _> = before
        .entities
        .iter()
        .map(|e| (entity_entry_key(e), e.name.clone()))
        .collect();
    let after_entities: BTreeMap<_, _> = after
        .entities
        .iter()
        .map(|e| (entity_entry_key(e), e.name.clone()))
        .collect();
    for (key, name) in &after_entities {
        if !before_entities.contains_key(key) {
            summary.entities_added.push(name.clone());
        }
    }
    for (key, name) in &before_entities {
        if !after_entities.contains_key(key) {
            summary.entities_removed.push(name.clone());
        }
    }
    let before_sub: BTreeMap<_, _> = before
        .substances
        .iter()
        .map(|s| (substance_entry_key(s), substance_label(s)))
        .collect();
    let after_sub: BTreeMap<_, _> = after
        .substances
        .iter()
        .map(|s| (substance_entry_key(s), substance_label(s)))
        .collect();
    for (key, label) in &after_sub {
        if !before_sub.contains_key(key) {
            summary.substances_added.push(label.clone());
        }
    }
    for (key, label) in &before_sub {
        if !after_sub.contains_key(key) {
            summary.substances_removed.push(label.clone());
        }
    }
    Ok(summary)
}

fn substance_label(entry: &ComplianceListSubstanceEntry) -> String {
    if !entry.name.is_empty() {
        entry.name.clone()
    } else if !entry.cas.is_empty() {
        entry.cas.clone()
    } else {
        entry.ec_number.clone()
    }
}

/// Collect every searchable name token from bundled lists for CI leak checks.
pub fn bundled_list_name_tokens() -> BTreeSet<String> {
    let mut tokens = BTreeSet::new();
    for id in builtin_compliance_list_ids() {
        if let Ok(snapshot) = builtin_compliance_list(id) {
            collect_snapshot_tokens(&snapshot, &mut tokens);
        }
    }
    tokens
}

pub fn collect_snapshot_tokens(snapshot: &ComplianceListSnapshot, out: &mut BTreeSet<String>) {
    for entity in &snapshot.entities {
        push_token(&entity.name, out);
        for alias in &entity.aliases {
            push_token(alias, out);
        }
    }
    for substance in &snapshot.substances {
        push_token(&substance.name, out);
    }
}

fn push_token(name: &str, out: &mut BTreeSet<String>) {
    let normalized = normalize_compliance_name(name);
    if normalized.len() >= 4 {
        out.insert(normalized);
    }
}

fn toml_value_to_json(value: toml::Value) -> Value {
    match value {
        toml::Value::String(s) => Value::String(s),
        toml::Value::Integer(i) => Value::Number(i.into()),
        toml::Value::Float(f) => match serde_json::Number::from_f64(f) {
            Some(n) => Value::Number(n),
            None => Value::String(f.to_string()),
        },
        toml::Value::Boolean(b) => Value::Bool(b),
        toml::Value::Datetime(dt) => Value::String(dt.to_string()),
        toml::Value::Array(items) => {
            Value::Array(items.into_iter().map(toml_value_to_json).collect())
        }
        toml::Value::Table(table) => {
            let mut map = Map::new();
            for (key, item) in table {
                map.insert(key, toml_value_to_json(item));
            }
            Value::Object(map)
        }
    }
}

fn json_value_to_toml(value: Value) -> toml::Value {
    match value {
        Value::Null => toml::Value::String(String::new()),
        Value::Bool(b) => toml::Value::Boolean(b),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                toml::Value::Integer(i)
            } else if let Some(f) = n.as_f64() {
                toml::Value::Float(f)
            } else {
                toml::Value::String(n.to_string())
            }
        }
        Value::String(s) => toml::Value::String(s),
        Value::Array(items) => {
            toml::Value::Array(items.into_iter().map(json_value_to_toml).collect())
        }
        Value::Object(map) => {
            let mut table = toml::map::Map::new();
            for (key, item) in map {
                if item.is_null() {
                    continue;
                }
                table.insert(key, json_value_to_toml(item));
            }
            toml::Value::Table(table)
        }
    }
}
