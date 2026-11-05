//! Moderation policy rules stored as data and evaluated before the human queue.
//!
//! Policy rules are loaded from the database as JSON and evaluated in order.
//! The engine is a pure function: no clock, no I/O, no global state.

use serde::{Deserialize, Serialize};

use crate::schema::TrustTier;

/// One rule row loaded into the policy engine.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum PolicyRule {
    /// A publisher with no earlier publication always goes to the human queue.
    NewPublisherToQueue,
    /// A publisher at or above this tier skips the human queue.
    TrustTierBypass {
        /// The lowest tier that skips the queue.
        minimum: TrustTier,
    },
    /// A publisher may upload this many times in one day.
    RateLimitUploads {
        /// The most uploads allowed in one day.
        max_per_day: u32,
    },
    /// Size limits for one publication and for all of a publisher's files.
    QuotaBytes {
        /// The most bytes one publication may hold.
        max_publication_bytes: u64,
        /// The most bytes one publisher may hold in total.
        max_publisher_bytes: u64,
    },
    /// Refuses a publication under this licence.
    BlockLicence {
        /// The SPDX identifier of the blocked licence.
        spdx_id: String,
    },
}

/// Outcome of evaluating policy against an upload attempt.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyDecision {
    /// False when a rule refuses the upload.
    pub allow_publish: bool,
    /// True when a person must look at the upload before it is published.
    pub requires_human_queue: bool,
    /// One plain sentence for each rule that changed the outcome.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reasons: Vec<String>,
}

/// Context values needed to evaluate policy for an upload attempt.
///
/// # Rules
/// - Tier reflects the publisher's current trust tier at upload time.
/// - Byte counts are exact sizes, not estimates.
///
/// # Dependencies
/// - crates/portal/src/schema.rs (TrustTier)
///
/// # Do not
/// - Pass stale or estimated values; the engine trusts this context.
#[derive(Clone, Debug)]
pub struct UploadContext {
    /// The publisher's trust tier at upload time.
    pub tier: TrustTier,
    /// The size of this upload in bytes.
    pub total_bytes: u64,
    /// The bytes the publisher already holds.
    pub publisher_bytes_total: u64,
    /// How many uploads the publisher made today.
    pub uploads_today: u32,
    /// The SPDX licence of the upload, when it names one.
    pub licence: Option<String>,
}

/// Rank a trust tier for comparison: New=0, Standard=1, Trusted=2.
fn trust_tier_rank(tier: &TrustTier) -> u8 {
    match tier {
        TrustTier::New => 0,
        TrustTier::Standard => 1,
        TrustTier::Trusted => 2,
    }
}

/// Evaluate a list of policy rules against an upload context.
///
/// Returns a PolicyDecision that determines whether the upload can proceed
/// and whether it must go to the human moderation queue. Rules are evaluated
/// in order; each failing rule appends a plain-sentence reason.
///
/// # Rules
/// - Human queue is the default; only TrustTierBypass can clear it.
/// - New tier always requires the human queue regardless of bypass rules.
/// - A blocked licence or exceeded quota/refuses the upload.
///
/// # Dependencies
/// - crates/portal/src/schema.rs (TrustTier)
/// - crates/portal/src/policy.rs (PolicyRule, PolicyDecision, UploadContext)
///
/// # Do not
/// - Add clock or I/O; this function is pure and deterministic.
pub fn evaluate_policy(rules: &[PolicyRule], ctx: &UploadContext) -> PolicyDecision {
    let mut decision = PolicyDecision {
        allow_publish: true,
        requires_human_queue: true,
        reasons: Vec::new(),
    };

    for rule in rules {
        match rule {
            PolicyRule::NewPublisherToQueue => {
                if matches!(ctx.tier, TrustTier::New) {
                    decision.requires_human_queue = true;
                }
            }
            PolicyRule::TrustTierBypass { minimum } => {
                if trust_tier_rank(&ctx.tier) >= trust_tier_rank(minimum)
                    && !matches!(ctx.tier, TrustTier::New)
                {
                    decision.requires_human_queue = false;
                }
            }
            PolicyRule::RateLimitUploads { max_per_day } => {
                if ctx.uploads_today >= *max_per_day {
                    decision.allow_publish = false;
                    decision.requires_human_queue = true;
                    decision.reasons.push(format!(
                        "Upload limit reached: {} uploads today (max {})",
                        ctx.uploads_today, max_per_day
                    ));
                }
            }
            PolicyRule::QuotaBytes {
                max_publication_bytes,
                max_publisher_bytes,
            } => {
                if ctx.total_bytes > *max_publication_bytes {
                    decision.allow_publish = false;
                    decision.requires_human_queue = true;
                    decision.reasons.push(format!(
                        "Publication too large: {} bytes (max {})",
                        ctx.total_bytes, max_publication_bytes
                    ));
                } else if ctx.publisher_bytes_total + ctx.total_bytes > *max_publisher_bytes {
                    decision.allow_publish = false;
                    decision.requires_human_queue = true;
                    decision.reasons.push(format!(
                        "Publisher quota exceeded: {} total bytes (max {})",
                        ctx.publisher_bytes_total + ctx.total_bytes,
                        max_publisher_bytes
                    ));
                }
            }
            PolicyRule::BlockLicence { spdx_id } => {
                if let Some(licence) = &ctx.licence
                    && licence.eq_ignore_ascii_case(spdx_id)
                {
                    decision.allow_publish = false;
                    decision.requires_human_queue = true;
                    decision
                        .reasons
                        .push(format!("Blocked licence: {}", spdx_id));
                }
            }
        }
    }

    decision
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context(tier: TrustTier) -> UploadContext {
        UploadContext {
            tier,
            total_bytes: 100,
            publisher_bytes_total: 0,
            uploads_today: 0,
            licence: None,
        }
    }

    #[test]
    fn a_new_publisher_always_goes_to_the_human_queue() {
        let rules = vec![
            PolicyRule::NewPublisherToQueue,
            PolicyRule::TrustTierBypass {
                minimum: TrustTier::Standard,
            },
        ];
        let ctx = context(TrustTier::New);
        let result = evaluate_policy(&rules, &ctx);
        assert!(result.requires_human_queue);
    }

    #[test]
    fn a_trusted_publisher_skips_the_queue() {
        let rules = vec![PolicyRule::TrustTierBypass {
            minimum: TrustTier::Standard,
        }];
        let ctx = context(TrustTier::Trusted);
        let result = evaluate_policy(&rules, &ctx);
        assert!(!result.requires_human_queue);
        assert!(result.allow_publish);
    }

    #[test]
    fn an_oversize_upload_is_refused() {
        let rules = vec![PolicyRule::QuotaBytes {
            max_publication_bytes: 1000,
            max_publisher_bytes: 100000,
        }];
        let mut ctx = context(TrustTier::Trusted);
        ctx.total_bytes = 1001;
        let result = evaluate_policy(&rules, &ctx);
        assert!(!result.allow_publish);
        assert_eq!(result.reasons.len(), 1);
    }

    #[test]
    fn a_blocked_licence_is_refused_in_any_case() {
        let rules = vec![PolicyRule::BlockLicence {
            spdx_id: "gpl-3.0-only".to_string(),
        }];
        let mut ctx = context(TrustTier::Trusted);
        ctx.licence = Some("GPL-3.0-ONLY".to_string());
        let result = evaluate_policy(&rules, &ctx);
        assert!(!result.allow_publish);
        assert_eq!(result.reasons.len(), 1);
    }

    #[test]
    fn the_eleventh_upload_of_the_day_is_refused() {
        let rules = vec![PolicyRule::RateLimitUploads { max_per_day: 10 }];
        let mut ctx = context(TrustTier::Trusted);
        ctx.uploads_today = 10;
        let result = evaluate_policy(&rules, &ctx);
        assert!(!result.allow_publish);
        assert_eq!(result.reasons.len(), 1);
    }
}
