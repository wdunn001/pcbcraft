//! Compliance rule set loader (schema 1, issue #691).
use serde::Deserialize;

/// One check rule inside a compliance rule set file.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ComplianceRule {
    /// Stable rule id referenced by findings.
    pub id: String,
    /// Rule kind, such as `entity_list` or `classification_report`.
    pub kind: String,
    /// List snapshot id for entity rules.
    #[serde(default)]
    pub list_id: String,
    /// Declaration regime id for declaration rules.
    #[serde(default)]
    pub regime: String,
    /// Severity when the rule matches.
    pub severity_hit: String,
    /// Severity when required data is missing.
    pub severity_unverified: String,
    /// Plain-language explanation shown in the UI.
    pub text: String,
    /// Source citation printed on the report.
    pub citation: String,
    /// Evidence field the engine reads.
    pub evidence: String,
}

/// Metadata and rules for one selectable compliance programme.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ComplianceRuleSet {
    /// File schema version.
    pub schema: u32,
    /// Programme id stored on the project.
    pub id: String,
    /// Short title for pickers and reports.
    pub title: String,
    /// Longer summary for tooltips.
    pub summary: String,
    /// Ordered rules evaluated for the programme.
    pub rules: Vec<ComplianceRule>,
}

impl ComplianceRuleSet {
    /// Parse and validate a rule set TOML document.
    pub fn parse(text: &str) -> Result<Self, String> {
        let set: Self = toml::from_str(text).map_err(|e| format!("rule set parse: {e}"))?;
        set.validate()?;
        Ok(set)
    }

    /// Check schema, ids and rule counts.
    pub fn validate(&self) -> Result<(), String> {
        if self.schema != 1 {
            return Err("compliance rule set schema must be 1".into());
        }
        if self.id.is_empty() || self.id.len() > 64 {
            return Err("rule set id must be 1 to 64 bytes".into());
        }
        if self.rules.is_empty() || self.rules.len() > 64 {
            return Err("a rule set needs 1 to 64 rules".into());
        }
        for rule in &self.rules {
            if rule.id.is_empty() || rule.id.len() > 64 {
                return Err("rule id must be 1 to 64 bytes".into());
            }
        }
        Ok(())
    }
}

const EXPORT_AND_TRADE_TOML: &str =
    include_str!("../../data/compliance/rulesets/export_and_trade.toml");

/// Load the bundled export-and-trade rule set shipped with the app.
pub fn bundled_export_and_trade() -> Result<ComplianceRuleSet, String> {
    ComplianceRuleSet::parse(EXPORT_AND_TRADE_TOML)
}
