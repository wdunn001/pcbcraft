//! Regulatory compliance rule sets driven by bundled TOML data.
mod eu_market;
mod lists;
mod ruleset;

pub use eu_market::EU_MARKET_RULE_SET;
pub use lists::{load_list_snapshot, ListSnapshot};
pub use ruleset::{ComplianceRuleSet, ComplianceRuleSpec, load_rule_set};

use pcbcraft_doc::{
    ComplianceProjectSettings, Component, Declaration, DeclarationStatus, Project, date_le,
    valid_date,
};
use serde::Serialize;

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct ComplianceFinding {
    pub code: String,
    pub state: String,
    pub severity: String,
    pub message: String,
    pub rule: String,
    pub evidence: String,
    pub citation: String,
    pub objects: Vec<u64>,
}

/// Run every rule set named in the project compliance settings.
pub fn check(project: &Project) -> Vec<ComplianceFinding> {
    let mut out = Vec::new();
    for id in &project.compliance_settings.rule_sets {
        if id == "eu_market" {
            out.extend(eu_market::evaluate(project));
        }
    }
    out
}

pub(crate) fn declaration_for(component: &Component, regime: &str) -> Option<Declaration> {
    component
        .effective_declarations()
        .into_iter()
        .find(|d| d.regime.trim().eq_ignore_ascii_case(regime))
}

pub(crate) fn declaration_as_of(decl: &Declaration) -> Option<&str> {
    if decl.as_of.is_empty() {
        None
    } else {
        Some(decl.as_of.as_str())
    }
}

pub(crate) fn is_unknown(d: &Declaration) -> bool {
    d.status == DeclarationStatus::Unknown
}

pub(crate) fn is_exempt(d: &Declaration) -> bool {
    d.status == DeclarationStatus::DeclaredExempt
}

pub(crate) fn exemption_number(d: &Declaration) -> Option<&str> {
    if is_exempt(d) && !d.detail.is_empty() {
        Some(d.detail.as_str())
    } else {
        None
    }
}

pub(crate) fn list_version_stale(
    declared_version: &str,
    snapshot_version: &str,
    snapshot_retrieved: &str,
    declaration_date: Option<&str>,
) -> bool {
    if declared_version != snapshot_version {
        return true;
    }
    if let Some(d) = declaration_date
        && valid_date(d)
        && valid_date(snapshot_retrieved)
    {
        return date_le(snapshot_retrieved, d);
    }
    false
}

pub(crate) fn finish_is_leaded(project: &Project) -> bool {
    let finish = project.board.finish.to_ascii_lowercase();
    if finish.contains("leaded") || finish.contains("lead tin") || finish.contains("sn/pb") {
        return true;
    }
    if let Some(rules) = &project.board.rules {
        if rules
            .solder
            .as_ref()
            .is_some_and(|s| s.eq_ignore_ascii_case("leaded"))
        {
            return true;
        }
    }
    false
}

pub(crate) fn push_unverified(
    out: &mut Vec<ComplianceFinding>,
    rule: &ComplianceRuleSpec,
    component_id: u64,
    reference: &str,
    message: &str,
) {
    out.push(ComplianceFinding {
        code: rule.id.clone(),
        state: "unverified".into(),
        severity: rule.severity_unverified.clone(),
        message: message.to_string(),
        rule: rule.id.clone(),
        evidence: format!("Part {reference} has no matching declaration."),
        citation: rule.citation.clone(),
        objects: vec![component_id],
    });
}

pub(crate) fn push_stale(
    out: &mut Vec<ComplianceFinding>,
    rule: &ComplianceRuleSpec,
    component_id: u64,
    reference: &str,
    evidence: &str,
    message: &str,
) {
    out.push(ComplianceFinding {
        code: rule.id.clone(),
        state: "stale".into(),
        severity: rule
            .severity_stale
            .clone()
            .unwrap_or_else(|| rule.severity_hit.clone()),
        message: message.to_string(),
        rule: rule.id.clone(),
        evidence: format!("Part {reference}: {evidence}"),
        citation: rule.citation.clone(),
        objects: vec![component_id],
    });
}

pub(crate) fn push_hit(
    out: &mut Vec<ComplianceFinding>,
    rule: &ComplianceRuleSpec,
    component_id: u64,
    reference: &str,
    evidence: &str,
    message: &str,
) {
    out.push(ComplianceFinding {
        code: rule.id.clone(),
        state: "hit".into(),
        severity: rule.severity_hit.clone(),
        message: message.to_string(),
        rule: rule.id.clone(),
        evidence: format!("Part {reference}: {evidence}"),
        citation: rule.citation.clone(),
        objects: vec![component_id],
    });
}

pub(crate) fn push_board_hit(
    out: &mut Vec<ComplianceFinding>,
    rule: &ComplianceRuleSpec,
    evidence: &str,
    message: &str,
) {
    out.push(ComplianceFinding {
        code: rule.id.clone(),
        state: "hit".into(),
        severity: rule.severity_hit.clone(),
        message: message.to_string(),
        rule: rule.id.clone(),
        evidence: evidence.to_string(),
        citation: rule.citation.clone(),
        objects: vec![],
    });
}

pub fn settings_valid(settings: &ComplianceProjectSettings) -> bool {
    settings.rule_sets.len() <= 16
        && settings
            .rule_sets
            .iter()
            .all(|s| !s.is_empty() && s.len() <= 64)
}
