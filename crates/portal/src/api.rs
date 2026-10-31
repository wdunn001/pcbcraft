//! HTTP API request and response bodies for the portal service.

use serde::{Deserialize, Serialize};

use crate::feed::StaticFeedDocument;
use crate::schema::PublicationKind;

pub const API_VERSION: u32 = 1;
pub const CAPABILITIES_PATH: &str = "/v1/capabilities";
pub const STATIC_FEED_PATH: &str = "/v1/feed/static";

/// Service capabilities document (immutable reads, auth modes, upload limits).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilitiesDocument {
    pub api_version: u32,
    pub instance_id: String,
    pub title: String,
    pub anonymous_browse: bool,
    pub anonymous_download: bool,
    pub auth: AuthCapabilities,
    pub upload: UploadCapabilities,
    pub search: SearchCapabilities,
    pub static_feed: StaticFeedCapabilities,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthCapabilities {
    pub email_link: bool,
    pub passkey: bool,
    pub oidc: bool,
    pub api_tokens: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UploadCapabilities {
    pub max_publication_bytes: u64,
    pub max_files_per_upload: u32,
    pub two_step: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchCapabilities {
    pub max_results: u32,
    pub fields: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StaticFeedCapabilities {
    pub path: String,
    pub format: String,
}

/// Step one: reserve an upload slot and receive presigned part targets later.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UploadInitRequest {
    pub publication_id: String,
    pub version_label: String,
    pub kind: PublicationKind,
    pub expected_bundle_root_hash: String,
    pub manifest_object_hash: String,
    pub total_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UploadInitResponse {
    pub upload_id: String,
    pub expires_at: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parts: Vec<UploadPartTarget>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UploadPartTarget {
    pub part_number: u32,
    pub url: String,
    pub method: String,
}

/// Step two: confirm every part hash matches before the item enters moderation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UploadCompleteRequest {
    pub upload_id: String,
    pub parts: Vec<UploadPartProof>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UploadPartProof {
    pub part_number: u32,
    pub content_hash: String,
    pub size: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UploadCompleteResponse {
    pub publication_id: String,
    pub version_label: String,
    pub queue: UploadQueueState,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UploadQueueState {
    PendingReview,
    Published,
    Rejected,
}

/// Immutable object read (Cache-Control: immutable when served over HTTP).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObjectMeta {
    pub content_hash: String,
    pub size: u64,
    pub media_type: String,
}

/// List and search publications by manifest fields.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationListRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<PublicationKind>,
    pub limit: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationSummary {
    pub publication_id: String,
    pub version_label: String,
    pub title: String,
    pub kind: PublicationKind,
    pub bundle_root_hash: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    pub yanked: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationListResponse {
    pub items: Vec<PublicationSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StaticFeedResponse {
    pub feed: StaticFeedDocument,
}
