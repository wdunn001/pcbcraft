//! TOML import and export for machine profiles, plus the built-in shop presets.
//!
//! # Rules
//! - TOML is parsed here only; crates/doc stays free of it, so text becomes
//!   `serde_json::Value` first and is then read into `MachineProfile`.
//! - Presets are compiled in from the repo `machine-profiles/` folder and returned as copies.
//!
//! # Dependencies
//! - crates/doc/src/machine_profile.rs
//! - machine-profiles/SCHEMA.md
use pcbcraft_doc::MachineProfile;
use serde_json::{Map, Value};

const BUILTIN_IDS: [&str; 6] = [
    "neoden-4",
    "neoden-yy1",
    "eufymake-e1",
    "snapmaker-2-a350t-200w-cnc",
    "snapmaker-artisan-200w-cnc",
    "commarker-b6-mopa",
];

/// The ids of the shop presets compiled into the program.
pub fn builtin_machine_profile_ids() -> &'static [&'static str] {
    &BUILTIN_IDS
}

/// A copy of one shop preset, parsed and validated; an unknown id is refused.
///
/// # Dependencies
/// - machine-profiles/SCHEMA.md
pub fn builtin_machine_profile(id: &str) -> Result<MachineProfile, String> {
    let text = match id {
        "neoden-yy1" => include_str!("../../../machine-profiles/neoden-yy1.toml"),
        "neoden-4" => include_str!("../../../machine-profiles/neoden-4.toml"),
        "eufymake-e1" => include_str!("../../../machine-profiles/eufymake-e1.toml"),
        "snapmaker-2-a350t-200w-cnc" => {
            include_str!("../../../machine-profiles/snapmaker-2-a350t-200w-cnc.toml")
        }
        "snapmaker-artisan-200w-cnc" => {
            include_str!("../../../machine-profiles/snapmaker-artisan-200w-cnc.toml")
        }
        "commarker-b6-mopa" => include_str!("../../../machine-profiles/commarker-b6-mopa.toml"),
        _ => return Err("unknown built-in machine profile".into()),
    };
    machine_profile_from_toml(text)
}

/// Read one profile from TOML text and validate it.
///
/// # Rules
/// - Unknown keys and out-of-range numbers are refused with a plain reason.
///
/// # Do not
/// - Read a file here; callers pass the text.
pub fn machine_profile_from_toml(text: &str) -> Result<MachineProfile, String> {
    let parsed: toml::Value = toml::from_str(text).map_err(|e| e.to_string())?;
    let json = toml_value_to_json(parsed);
    let profile: MachineProfile = serde_json::from_value(json).map_err(|e| e.to_string())?;
    profile.validate()?;
    Ok(profile)
}

/// Write one validated profile as TOML text that reads back to the same profile.
///
/// # Do not
/// - Write to disk here; the engine uses `atomic_write`.
pub fn machine_profile_to_toml(profile: &MachineProfile) -> Result<String, String> {
    profile.validate()?;
    let json = serde_json::to_value(profile).map_err(|e| e.to_string())?;
    let toml_value = json_value_to_toml(json);
    toml::to_string_pretty(&toml_value).map_err(|e| e.to_string())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_builtin_preset_parses_and_validates() -> Result<(), String> {
        for id in builtin_machine_profile_ids() {
            let profile = builtin_machine_profile(id)?;
            if profile.id != *id {
                return Err(format!("built-in id mismatch for {id}"));
            }
            profile.validate()?;
        }
        Ok(())
    }

    #[test]
    fn machine_profile_toml_import_export_round_trip() -> Result<(), String> {
        let profile = builtin_machine_profile("commarker-b6-mopa")?;
        let text = machine_profile_to_toml(&profile)?;
        let back = machine_profile_from_toml(&text)?;
        if back != profile {
            return Err("toml round trip changed the profile".into());
        }
        Ok(())
    }
}
