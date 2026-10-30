//! TOML import and export for interface rule profiles (schema 1 and 2).
use pcbcraft_doc::InterfaceProfileDocument;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

pub fn interface_profile_content_hash(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub fn interface_profile_from_toml(text: &str) -> Result<InterfaceProfileDocument, String> {
    let parsed: toml::Value = toml::from_str(text).map_err(|e| format!("TOML profile: {e}"))?;
    let json = toml_value_to_json(parsed);
    let profile: InterfaceProfileDocument =
        serde_json::from_value(json).map_err(|e| format!("profile fields: {e}"))?;
    profile.validate()?;
    Ok(profile)
}

pub fn interface_profile_to_toml(profile: &InterfaceProfileDocument) -> Result<String, String> {
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
    fn content_hash_is_stable() {
        let a = interface_profile_content_hash("schema = 1\n");
        let b = interface_profile_content_hash("schema = 1\n");
        assert_eq!(a, b);
        assert_eq!(a.len(), 64);
    }
}
