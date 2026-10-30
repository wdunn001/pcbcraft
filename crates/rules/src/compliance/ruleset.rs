//! Compliance rule set loader (schema 1, issue #691).
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ComplianceRule {
    pub id: String,
    pub kind: String,
    #[serde(default)]
    pub list_id: String,
    #[serde(default)]
    pub regime: String,
    pub severity_hit: String,
    pub severity_unverified: String,
    pub text: String,
    pub citation: String,
    pub evidence: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ComplianceRuleSet {
    pub schema: u32,
    pub id: String,
    pub title: String,
    pub summary: String,
    pub rules: Vec<ComplianceRule>,
}

impl ComplianceRuleSet {
    pub fn parse(text: &str) -> Result<Self, String> {
        let set: Self = toml::from_str(text).map_err(|e| format!("rule set parse: {e}"))?;
        set.validate()?;
        Ok(set)
    }

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

pub fn bundled_export_and_trade() -> Result<ComplianceRuleSet, String> {
    ComplianceRuleSet::parse(EXPORT_AND_TRADE_TOML)
}
