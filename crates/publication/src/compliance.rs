//! Declared compliance and safety flags shown as publisher claims.
//!
//! # Rules
//! - Claims are never marked certified.
//! - Mains, battery or high-voltage designs get a not-reviewed safety note.
//!
//! # Dependencies
//! - crates/publication/src/scan.rs
//!
//! # Do not
//! - Imply regulatory certification from publisher-entered text.

use crate::ScanOutcome;

/// Publisher-entered compliance and export flags.
#[derive(Clone, Debug, Default)]
pub struct ComplianceInput {
    pub claims: Vec<String>,
    pub export_review_requested: bool,
    pub mains_power: bool,
    pub battery_power: bool,
    pub high_voltage: bool,
}

/// Turn compliance input into informational notes and optional human-queue flags.
pub fn compliance_notes(input: &ComplianceInput) -> Vec<ScanOutcome> {
    let mut out = Vec::new();
    for claim in &input.claims {
        if !claim.is_empty() {
            out.push(ScanOutcome::Note {
                rule: "compliance_claim",
                detail: format!("publisher claim (not certified): {claim}"),
            });
        }
    }
    if input.export_review_requested {
        out.push(ScanOutcome::HumanQueue {
            rule: "export_review",
            path: "publication".into(),
            detail: "export review requested by the publisher".into(),
        });
    }
    if input.mains_power || input.battery_power || input.high_voltage {
        out.push(ScanOutcome::Note {
            rule: "safety_not_reviewed",
            detail: "not reviewed for safety; mains, battery or high-voltage design".into(),
        });
    }
    out
}
