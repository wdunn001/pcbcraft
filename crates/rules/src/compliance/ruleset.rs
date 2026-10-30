use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
pub struct ComplianceRuleSet {
    pub schema: u32,
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    pub rules: Vec<ComplianceRuleSpec>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ComplianceRuleSpec {
    pub id: String,
    pub kind: String,
    #[serde(default)]
    pub severity_hit: String,
    #[serde(default = "default_warning")]
    pub severity_unverified: String,
    #[serde(default)]
    pub message_unverified: String,
    #[serde(default)]
    pub message_hit: String,
    pub citation: String,
    #[serde(default)]
    pub list: String,
    #[serde(default)]
    pub entry_type: String,
    #[serde(default)]
    pub roles: Vec<String>,
}

fn default_warning() -> String {
    "warning".into()
}

pub fn load_rule_set(toml_text: &str) -> Result<ComplianceRuleSet, String> {
    let set: ComplianceRuleSet = toml::from_str(toml_text).map_err(|e| e.to_string())?;
    if set.schema != 1 {
        return Err(format!("unsupported compliance ruleset schema {}", set.schema));
    }
    if set.id.is_empty() {
        return Err("ruleset id is required".into());
    }
    Ok(set)
}
