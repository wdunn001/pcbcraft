//! Row shapes for the portal SQL database. Identifiers are opaque strings; times
//! are Unix seconds UTC.

use serde::{Deserialize, Serialize};

pub const MAX_PUBLISHER_SLUG: usize = 64;
pub const MAX_PUBLICATION_ID: usize = 128;
pub const MAX_VERSION_LABEL: usize = 64;
pub const MAX_TAG: usize = 64;
pub const MAX_HASH_HEX: usize = 64;
pub const MAX_EMAIL: usize = 320;
pub const MAX_TOKEN_NAME: usize = 128;

/// One publisher account on this instance.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublisherRow {
    pub id: String,
    pub slug: String,
    pub display_name: String,
    pub trust_tier: TrustTier,
    pub created_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verified_domain: Option<String>,
}

/// Trust tier controls whether uploads skip the human moderation queue.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TrustTier {
    New,
    Standard,
    Trusted,
}

/// Logical publication (all versions share one id).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationRow {
    pub id: String,
    pub publisher_id: String,
    pub kind: PublicationKind,
    pub created_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub yanked_at: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PublicationKind {
    Design,
    Part,
}

/// One immutable published revision.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VersionRow {
    pub publication_id: String,
    pub version_label: String,
    pub bundle_root_hash: String,
    pub manifest_object_hash: String,
    pub title: String,
    pub published_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_version_hash: Option<String>,
    pub stage0_passed: bool,
}

/// Stored blob pointer (content-addressed object store).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileRow {
    pub content_hash: String,
    pub size: u64,
    pub media_type: String,
    pub stored_at: u64,
}

/// Tag on a publication version.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TagRow {
    pub publication_id: String,
    pub version_label: String,
    pub tag: String,
}

/// User report against a publication version.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportRow {
    pub id: String,
    pub publication_id: String,
    pub version_label: String,
    pub state: ReportState,
    pub reason_code: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub detail: String,
    pub reporter_account_id: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReportState {
    Open,
    Triaged,
    Resolved,
    Dismissed,
}

/// Moderator action on a queue item or report.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ModerationAction {
    Approve,
    Reject,
    RequestChanges,
    ApproveAndTrust,
}

/// Moderator decision on a queue item or report.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecisionLogRow {
    pub id: String,
    pub subject: DecisionSubject,
    pub action: ModerationAction,
    pub moderator_account_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
    pub created_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum DecisionSubject {
    Upload { upload_id: String },
    Publication {
        publication_id: String,
        version_label: String,
    },
    Report { report_id: String },
}

/// Legal takedown workflow state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TakedownRow {
    pub id: String,
    pub publication_id: String,
    pub state: TakedownState,
    pub notice_received_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub counter_notice_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restored_at: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TakedownState {
    NoticePending,
    MaterialRemoved,
    CounterNoticePending,
    Restored,
    Closed,
}

/// End-user account (email link, passkey, or federated subject).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccountRow {
    pub id: String,
    pub created_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oidc_subject: Option<String>,
    pub role: AccountRole,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AccountRole {
    Anonymous,
    Member,
    Contributor,
    Moderator,
    Administrator,
}

/// API token for automation (hashed secret stored, never the raw token).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TokenRow {
    pub id: String,
    pub account_id: String,
    pub name: String,
    pub secret_hash: String,
    pub created_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<u64>,
}

/// Publisher signing or domain-verification key metadata.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeyRow {
    pub id: String,
    pub publisher_id: String,
    pub algorithm: String,
    pub public_material: String,
    pub created_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revoked_at: Option<u64>,
}
