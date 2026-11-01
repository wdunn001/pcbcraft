//! Compliance rule set loader (schema 1 TOML).
//!
//! One rule record carries every field the bundled rule sets use; a rule kind
//! reads only the fields it needs and the rest keep their defaults.
use serde::Deserialize;

/// A whole rule set file.
#[derive(Clone, Debug, Deserialize)]
pub struct ComplianceRuleSet {
    pub schema: u32,
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    /// One-line summary shown in reports.
    #[serde(default)]
    pub summary: String,
    /// How origin rules read a part: the module maker or the silicon vendor.
    #[serde(default)]
    pub interpretation: Interpretation,
    pub rules: Vec<ComplianceRuleSpec>,
}

/// Default origin reading and the help text shown beside the choice.
#[derive(Clone, Debug, Deserialize)]
pub struct Interpretation {
    #[serde(default = "default_module")]
    pub default: String,
    #[serde(default)]
    pub module_help: String,
    #[serde(default)]
    pub silicon_help: String,
}

impl Default for Interpretation {
    fn default() -> Self {
        Self {
            default: default_module(),
            module_help: String::new(),
            silicon_help: String::new(),
        }
    }
}

fn default_module() -> String {
    "module".into()
}

/// One rule in a rule set.
#[derive(Clone, Debug, Deserialize)]
pub struct ComplianceRuleSpec {
    pub id: String,
    pub kind: String,
    #[serde(default)]
    pub regime: String,
    #[serde(default)]
    pub severity_hit: String,
    #[serde(default = "default_warning")]
    pub severity_unverified: String,
    #[serde(default)]
    pub severity_stale: Option<String>,
    #[serde(default)]
    pub message_unverified: String,
    #[serde(default)]
    pub message_hit: String,
    #[serde(default)]
    pub message_stale: String,
    #[serde(default)]
    pub message_reference: String,
    /// Plain statement used when a rule has no separate messages.
    #[serde(default)]
    pub text: String,
    pub citation: String,
    #[serde(default)]
    pub list: String,
    #[serde(default)]
    pub field: String,
    #[serde(default)]
    pub template: String,
    #[serde(default)]
    pub attribute: String,
    #[serde(default)]
    pub limit: f64,
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub roles: Vec<String>,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub entry_type: String,
    #[serde(default)]
    pub finish_leaded: Vec<String>,
    #[serde(default)]
    pub covered_countries_list: String,
    #[serde(default)]
    pub framework_list: String,
    #[serde(default)]
    pub bands_list: String,
    #[serde(default)]
    pub markets: Vec<String>,
    /// Id of the list snapshot a screening rule reads.
    #[serde(default)]
    pub list_id: String,
    /// What a finding of this rule shows as evidence.
    #[serde(default)]
    pub evidence: String,
}

fn default_warning() -> String {
    "warning".into()
}

/// Parse and check a rule set file.
pub fn load_rule_set(toml_text: &str) -> Result<ComplianceRuleSet, String> {
    let set: ComplianceRuleSet = toml::from_str(toml_text).map_err(|e| e.to_string())?;
    if set.schema != 1 {
        return Err(format!("unsupported compliance ruleset schema {}", set.schema));
    }
    if set.id.is_empty() {
        return Err("ruleset id is required".into());
    }
    for rule in &set.rules {
        if rule.id.is_empty() {
            return Err("each rule needs an id".into());
        }
        if rule.citation.is_empty() {
            return Err(format!("rule {} needs a citation", rule.id));
        }
    }
    Ok(set)
}

const EXPORT_AND_TRADE_TOML: &str =
    include_str!("../../data/compliance/rulesets/export_and_trade.toml");

/// The bundled export-and-trade rule set.
pub fn bundled_export_and_trade() -> Result<ComplianceRuleSet, String> {
    load_rule_set(EXPORT_AND_TRADE_TOML)
}
