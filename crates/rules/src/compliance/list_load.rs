use pcbcraft_doc::{ComplianceListSnapshot, MAX_COMPLIANCE_LIST_BYTES};

const US_NDAA_SYNTHETIC: &str =
    include_str!("../../../../compliance-lists/us-ndaa-synthetic-entities.toml");

pub fn load_bundled_list(id: &str) -> Result<ComplianceListSnapshot, String> {
    let text = match id {
        "us-ndaa-synthetic-entities" => US_NDAA_SYNTHETIC,
        _ => return Err(format!("unknown bundled compliance list {id}")),
    };
    list_from_toml(text)
}

pub fn list_from_toml(text: &str) -> Result<ComplianceListSnapshot, String> {
    if text.len() > MAX_COMPLIANCE_LIST_BYTES {
        return Err(format!(
            "compliance list file is larger than {MAX_COMPLIANCE_LIST_BYTES} bytes"
        ));
    }
    let parsed: toml::Value = toml::from_str(text).map_err(|e| e.to_string())?;
    let json = toml_value_to_json(parsed);
    let snapshot: ComplianceListSnapshot =
        serde_json::from_value(json).map_err(|e| e.to_string())?;
    snapshot.validate()?;
    Ok(snapshot)
}

fn toml_value_to_json(value: toml::Value) -> serde_json::Value {
    match value {
        toml::Value::String(s) => serde_json::Value::String(s),
        toml::Value::Integer(i) => serde_json::Value::Number(i.into()),
        toml::Value::Float(f) => match serde_json::Number::from_f64(f) {
            Some(n) => serde_json::Value::Number(n),
            None => serde_json::Value::String(f.to_string()),
        },
        toml::Value::Boolean(b) => serde_json::Value::Bool(b),
        toml::Value::Datetime(dt) => serde_json::Value::String(dt.to_string()),
        toml::Value::Array(items) => {
            serde_json::Value::Array(items.into_iter().map(toml_value_to_json).collect())
        }
        toml::Value::Table(table) => {
            let mut map = serde_json::Map::new();
            for (key, item) in table {
                map.insert(key, toml_value_to_json(item));
            }
            serde_json::Value::Object(map)
        }
    }
}
