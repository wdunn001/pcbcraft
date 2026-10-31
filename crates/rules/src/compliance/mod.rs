//! Regulatory compliance checks driven by bundled rule set TOML.
mod entity_match;
mod evaluate;
mod list_load;
mod ruleset;

pub use evaluate::{evaluate_ruleset, EVAL_DAY};
pub use ruleset::{load_rule_set, ComplianceRuleSet, ComplianceRuleSpec};

use pcbcraft_doc::Project;
use serde::Serialize;

const SYNTHETIC_DEMO_RULE_SET: &str =
    include_str!("../../data/compliance/rulesets/synthetic_demo.toml");

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

fn bundled_ruleset_text(id: &str) -> Option<&'static str> {
    match id {
        "synthetic_demo" => Some(SYNTHETIC_DEMO_RULE_SET),
        _ => None,
    }
}

/// Run compliance rule sets named in the project settings.
pub fn compliance_check(project: &Project) -> Vec<ComplianceFinding> {
    let mut out = Vec::new();
    for id in &project.compliance_settings.rule_sets {
        let Some(text) = bundled_ruleset_text(id) else {
            out.push(ComplianceFinding {
                code: format!("{id}_missing"),
                state: "hit".into(),
                severity: "error".into(),
                message: format!("Unknown compliance rule set {id}."),
                rule: id.clone(),
                evidence: "Rule set id is not bundled with this release.".into(),
                citation: "PCBCraft compliance engine".into(),
                objects: vec![],
                manufacturer: None,
            });
            continue;
        };
        let set = match load_rule_set(text) {
            Ok(s) => s,
            Err(e) => {
                out.push(ComplianceFinding {
                    code: format!("{id}_load"),
                    state: "hit".into(),
                    severity: "error".into(),
                    message: format!("Rule set {id} failed to load."),
                    rule: id.clone(),
                    evidence: e,
                    citation: "PCBCraft compliance engine".into(),
                    objects: vec![],
                    manufacturer: None,
                });
                continue;
            }
        };
        out.extend(evaluate_ruleset(project, &set));
    }
    evaluate::apply_mutes(project, &mut out);
    out
}
