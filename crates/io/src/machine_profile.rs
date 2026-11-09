//! TOML import and export for machine profiles, plus the built-in shop presets.
//!
//! # Rules
//! - TOML is parsed here only; crates/doc stays free of it, so text becomes
//!   `serde_json::Value` first and is then read into `MachineProfile`.
//! - Built-in machines are the `*.toml` files of the repo `machine-profiles/` folder, embedded by
//!   `crates/io/build.rs` and returned as copies marked as built in. No machine id is named here.
//!
//! # Dependencies
//! - crates/doc/src/machine_profile.rs
//! - crates/io/build.rs (lists the machine files)
//! - machine-profiles/SCHEMA.md
use pcbcraft_doc::{MachineOrigin, MachineProfile};
use serde_json::{Map, Value};

/// The text of every machine file in `machine-profiles/`, embedded at build time.
const BUILTIN_FILES: &[&str] = include!(concat!(env!("OUT_DIR"), "/builtin_machine_files.rs"));

fn builtin_machines() -> &'static [MachineProfile] {
    static MACHINES: std::sync::OnceLock<Vec<MachineProfile>> = std::sync::OnceLock::new();
    MACHINES.get_or_init(|| {
        BUILTIN_FILES
            .iter()
            .filter_map(|text| machine_profile_from_toml(text).ok())
            .map(|mut profile| {
                profile.origin = MachineOrigin::BuiltIn;
                profile
            })
            .collect()
    })
}

/// The ids of the machines shipped with the program, in file order.
///
/// # Rules
/// - The list comes from the files in `machine-profiles/`; no id is named in code.
pub fn builtin_machine_profile_ids() -> Vec<String> {
    builtin_machines().iter().map(|m| m.id.clone()).collect()
}

/// Every machine shipped with the program, marked as built in.
pub fn builtin_machine_profiles() -> &'static [MachineProfile] {
    builtin_machines()
}

/// A copy of one shipped machine, parsed and validated; an unknown id is refused.
///
/// # Dependencies
/// - machine-profiles/SCHEMA.md
pub fn builtin_machine_profile(id: &str) -> Result<MachineProfile, String> {
    builtin_machines()
        .iter()
        .find(|m| m.id == id)
        .cloned()
        .ok_or_else(|| "unknown built-in machine profile".into())
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
            let profile = builtin_machine_profile(&id)?;
            if profile.id != id {
                return Err(format!("built-in id mismatch for {id}"));
            }
            profile.validate()?;
        }
        Ok(())
    }

    #[test]
    fn each_shipped_preset_toml_on_disk_matches_its_builtin() -> Result<(), String> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../machine-profiles");
        for id in builtin_machine_profile_ids() {
            let path = root.join(format!("{id}.toml"));
            let text = std::fs::read_to_string(&path)
                .map_err(|e| format!("read preset file {id}: {e}"))?;
            let from_disk = machine_profile_from_toml(&text)?;
            let from_builtin = builtin_machine_profile(&id)?;
            if from_disk != from_builtin {
                return Err(format!(
                    "machine-profiles/{id}.toml does not match the compiled built-in preset"
                ));
            }
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
