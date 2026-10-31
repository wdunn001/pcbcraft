//! Moderation policy rules stored as data and evaluated before the human queue.

use serde::{Deserialize, Serialize};

use crate::schema::TrustTier;

/// One rule row loaded into the policy engine.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum PolicyRule {
    NewPublisherToQueue,
    TrustTierBypass {
        minimum: TrustTier,
    },
    RateLimitUploads {
        max_per_day: u32,
    },
    QuotaBytes {
        max_publication_bytes: u64,
        max_publisher_bytes: u64,
    },
    BlockLicence {
        spdx_id: String,
    },
}

/// Outcome of evaluating policy against an upload attempt.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyDecision {
    pub allow_publish: bool,
    pub requires_human_queue: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reasons: Vec<String>,
}
