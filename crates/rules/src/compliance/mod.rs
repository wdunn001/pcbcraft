//! Regulatory compliance checks driven by bundled rule set TOML.
//!
//! One finding type, one list snapshot type (`pcbcraft_doc::ComplianceListSnapshot`)
//! and one rule record ([`ComplianceRuleSpec`]) serve every bundled rule set.
//! Each rule kind is read by one evaluator: the generic ones live in
//! `evaluate.rs`, the rule sets with their own bundled lists in the files named
//! after them.
//!
//! # Dependencies
//! - `crates/doc/src/compliance.rs` for the data the rules read.
//! - `crates/engine/src/lib.rs` calls [`compliance_check`].
mod blue_uas;
mod eccn;
mod entity_match;
mod eu_market;
mod evaluate;
mod export_trade;
mod lists;
mod list_load;
mod list_snapshot;
mod responsible_sourcing;
mod ruleset;
mod us_government;

pub use blue_uas::BLUE_UAS_STYLE_DRONE_RULE_SET;
pub use eccn::{control_rank, highest_eccn};
pub use eu_market::EU_MARKET_RULE_SET;
pub use evaluate::{EVAL_DAY, evaluate_ruleset};
pub use export_trade::{
    ExportTradeReport, FIELD_ECCN, FIELD_HTS, FIELD_MANUFACTURER, FIELD_ORIGIN, ScreeningHit,
    TariffEstimate, eccn_known, export_trade_report, export_trade_report_with_ruleset,
};
pub use list_load::{list_from_toml, load_bundled_list, load_list_snapshot};
pub use list_snapshot::{EntityListEntry, EntityListSnapshot, bundled_list_tokens, normalize_name};
pub use responsible_sourcing::RESPONSIBLE_SOURCING_RULE_SET;
pub use ruleset::{
    ComplianceRuleSet, ComplianceRuleSpec, bundled_export_and_trade, load_rule_set,
};
pub use us_government::US_GOVERNMENT_NDAA_RULE_SET;

use pcbcraft_doc::{
    Component, Declaration, DeclarationStatus, Project, date_le, valid_date,
};
use serde::Serialize;

const SYNTHETIC_DEMO_RULE_SET: &str =
    include_str!("../../data/compliance/rulesets/synthetic_demo.toml");

/// One row of compliance output: a rule, how it came out, and the evidence.
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manufacturer: Option<String>,
}

/// A finding that reports a failure to load a rule set.
pub(crate) fn load_failure(id: &str, what: &str, severity: &str, error: String) -> ComplianceFinding {
    ComplianceFinding {
        code: format!("{id}_load"),
        state: "hit".into(),
        severity: severity.into(),
        message: format!("{what} rule set failed to load: {error}"),
        rule: id.into(),
        evidence: error,
        citation: "PCBCraft bundled compliance data".into(),
        objects: vec![],
        manufacturer: None,
    }
}

fn synthetic_demo(project: &Project) -> Vec<ComplianceFinding> {
    match load_rule_set(SYNTHETIC_DEMO_RULE_SET) {
        Ok(set) => evaluate_ruleset(project, &set),
        Err(e) => vec![load_failure("synthetic_demo", "Synthetic demo", "error", e)],
    }
}

/// Run compliance rule sets named in the project settings.
pub fn compliance_check(project: &Project) -> Vec<ComplianceFinding> {
    let mut out = Vec::new();
    for id in &project.compliance_settings.rule_sets {
        match id.as_str() {
            "synthetic_demo" => out.extend(synthetic_demo(project)),
            "us_government_ndaa" => out.extend(us_government::evaluate(project)),
            "eu_market" => out.extend(eu_market::evaluate(project)),
            "responsible_sourcing" => out.extend(responsible_sourcing::evaluate(project)),
            "blue_uas_style_drone" => out.extend(blue_uas::evaluate(project)),
            // The export-and-trade summary is a report, not a set of findings.
            "export_and_trade" => {}
            _ => out.push(ComplianceFinding {
                code: format!("{id}_missing"),
                state: "hit".into(),
                severity: "error".into(),
                message: format!("Unknown compliance rule set {id}."),
                rule: id.clone(),
                evidence: "Rule set id is not bundled with this release.".into(),
                citation: "PCBCraft compliance engine".into(),
                objects: vec![],
                manufacturer: None,
            }),
        }
    }
    evaluate::apply_mutes(project, &mut out);
    out
}

/// True when a project's rule set selection is within bounds.
pub fn settings_valid(settings: &pcbcraft_doc::ComplianceProjectSettings) -> bool {
    settings.validate().is_ok()
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

/// The declaration a part carries for `regime`, counting the quick sourcing flags.
pub(crate) fn declaration_of(component: &Component, regime: &str) -> Option<Declaration> {
    component
        .effective_declarations()
        .into_iter()
        .find(|d| d.regime.trim().eq_ignore_ascii_case(regime.trim()))
}

/// True when a declaration cites a list version that is not the bundled one, or
/// predates the snapshot it should have been checked against.
pub(crate) fn list_version_stale(
    declared_version: &str,
    snapshot_version: &str,
    snapshot_retrieved: &str,
    declaration_date: &str,
) -> bool {
    if declared_version != snapshot_version {
        return true;
    }
    if valid_date(declaration_date)
        && !declaration_date.is_empty()
        && valid_date(snapshot_retrieved)
    {
        return date_le(snapshot_retrieved, declaration_date);
    }
    false
}

pub(crate) fn finish_is_leaded(project: &Project) -> bool {
    let finish = project.board.finish.to_ascii_lowercase();
    if finish.contains("leaded") || finish.contains("lead tin") || finish.contains("sn/pb") {
        return true;
    }
    project
        .board
        .rules
        .as_ref()
        .and_then(|rules| rules.solder.as_ref())
        .is_some_and(|s| s.eq_ignore_ascii_case("leaded"))
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
        manufacturer: None,
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
        manufacturer: None,
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
        manufacturer: None,
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
        manufacturer: None,
    });
}
