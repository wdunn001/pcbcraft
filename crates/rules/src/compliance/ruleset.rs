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
    pub attribute: String,
    #[serde(default)]
    pub markets: Vec<String>,
    #[serde(default = "default_error")]
    pub severity_hit: String,
    #[serde(default = "default_warning")]
    pub severity_unverified: String,
    pub text: String,
    pub citation: String,
}

fn default_error() -> String {
    "error".into()
}

fn default_warning() -> String {
    "warning".into()
}

pub fn parse_ruleset(text: &str) -> Result<RuleSetDoc, String> {
    toml::from_str(text).map_err(|e| e.to_string())
}
