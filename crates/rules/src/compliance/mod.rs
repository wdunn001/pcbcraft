//! Regulatory compliance rule sets driven by bundled TOML data.
mod entity_match;
mod list_load;
mod ruleset;
mod us_government;

pub use ruleset::{load_rule_set, ComplianceRuleSet, ComplianceRuleSpec};
pub use us_government::{evaluate as evaluate_us_government, US_GOVERNMENT_NDAA_RULE_SET};

use pcbcraft_doc::Project;
use serde::Serialize;

pub(crate) const EVAL_DAY: &str = "2026-06-15";

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

/// Run compliance rule sets named in the project settings.
pub fn check(project: &Project) -> Vec<ComplianceFinding> {
    let mut out = Vec::new();
    for id in &project.compliance_settings.rule_sets {
        if id == "us_government_ndaa" {
            out.extend(us_government::evaluate(project));
        }
    }
    out
}
