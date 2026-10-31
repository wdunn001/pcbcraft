//! Compliance rule set loader (schema 1 TOML).
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
pub struct RuleSetDoc {
    pub schema: u32,
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub interpretation: InterpretationDoc,
    #[serde(default)]
    pub rules: Vec<RuleDoc>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct InterpretationDoc {
    #[serde(default = "default_module")]
    pub default: String,
    #[serde(default)]
    pub module_help: String,
    #[serde(default)]
    pub silicon_help: String,
}

fn default_module() -> String {
    "module".into()
}

#[derive(Clone, Debug, Deserialize)]
pub struct RuleDoc {
    pub id: String,
    pub kind: String,
    #[serde(default)]
    pub roles: Vec<String>,
    #[serde(default)]
    pub regime: String,
    #[serde(default)]
    pub covered_countries_list: String,
    #[serde(default)]
    pub framework_list: String,
    #[serde(default)]
    pub bands_list: String,
    #[serde(default)]
    pub template: String,
    #[serde(default)]
    pub attribute: String,
    #[serde(default)]
    pub limit: f64,
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub finish_leaded: Vec<String>,
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
