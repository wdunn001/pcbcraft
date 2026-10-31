//! Static registry feed documents the service publishes for pull-based clients.

use serde::{Deserialize, Serialize};

/// Instance feed pointer document (matches static registry instance feed layout).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StaticFeedDocument {
    pub format: String,
    pub schema_version: u32,
    pub instance_id: String,
    pub title: String,
    pub catalogue_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,
}

pub const STATIC_FEED_FORMAT: &str = "pcbcraft-portal-static-feed";
