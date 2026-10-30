//! Moderation queue, reports and takedown API types.

use serde::{Deserialize, Serialize};

use crate::schema::{ModerationAction, ReportState, TrustTier};

/// One item waiting in the human review queue.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueueItem {
    pub upload_id: String,
    pub publication_id: String,
    pub version_label: String,
    pub publisher_slug: String,
    pub trust_tier: TrustTier,
    pub title: String,
    pub bundle_root_hash: String,
    pub submitted_at: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stage0_flags: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueueListResponse {
    pub items: Vec<QueueItem>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueueDecisionRequest {
    pub action: ModerationAction,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportCreateRequest {
    pub publication_id: String,
    pub version_label: String,
    pub reason_code: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub detail: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportView {
    pub id: String,
    pub publication_id: String,
    pub version_label: String,
    pub state: ReportState,
    pub reason_code: String,
    pub created_at: u64,
}
