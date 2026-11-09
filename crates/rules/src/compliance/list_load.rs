use pcbcraft_doc::{ComplianceListSnapshot, MAX_COMPLIANCE_LIST_BYTES};

const SYNTHETIC_DEMO_ENTITIES: &str =
    include_str!("../../../../compliance-lists/synthetic-demo-entities.toml");
const US_NDAA_SYNTHETIC: &str =
    include_str!("../../../../compliance-lists/us-ndaa-synthetic-entities.toml");
const REACH_SVHC: &str = include_str!("../../data/compliance/lists/reach_svhc_candidate.toml");
const ROHS_EXEMPTIONS: &str = include_str!("../../data/compliance/lists/rohs_exemptions.toml");
const PROP65_FIXTURE: &str =
    include_str!("../../data/compliance/lists/prop65_chemicals_fixture.toml");

/// A list snapshot that ships with the program, by id.
pub fn load_bundled_list(id: &str) -> Result<ComplianceListSnapshot, String> {
    let text = match id {
        "synthetic-demo-entities" => SYNTHETIC_DEMO_ENTITIES,
        "us-ndaa-synthetic-entities" => US_NDAA_SYNTHETIC,
        "reach_svhc_candidate" => REACH_SVHC,
        "rohs_exemptions" => ROHS_EXEMPTIONS,
        "prop65_chemicals_fixture" => PROP65_FIXTURE,
        _ => return Err(format!("unknown bundled compliance list {id}")),
    };
    list_from_toml(text)
}

/// Parse and check a list snapshot given as TOML text.
pub fn load_list_snapshot(text: &str) -> Result<ComplianceListSnapshot, String> {
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
