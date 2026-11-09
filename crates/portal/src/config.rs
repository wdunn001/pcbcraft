//! Deployment configuration shape. Operators load values from environment or
//! files outside the repository; only this schema and example placeholders ship
//! in git.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PortalConfig {
    pub instance_id: String,
    pub title: String,
    pub listen: String,
    pub database_url: String,
    pub object_store: ObjectStoreConfig,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<AuthConfig>,
    /// Vendor quote handoff and marketplace switches; absent means both are off.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vendor_integrations: Option<VendorIntegrationsConfig>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "driver", rename_all = "kebab-case")]
pub enum ObjectStoreConfig {
    LocalFolder {
        root: String,
    },
    S3Compatible {
        bucket: String,
        region: String,
        endpoint: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthConfig {
    pub email_link: bool,
    pub passkey: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oidc_issuer: Option<String>,
}

/// Per-instance switches for the optional vendor features.
///
/// # Rules
/// - Both switches default to off.
/// - An instance without this block shows no vendor control.
///
/// # Dependencies
/// - crates/library/src/vendor_integrations.rs (the adapter types these switches gate)
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VendorIntegrationsConfig {
    /// Whether vendor quote handoff is enabled on this instance.
    pub quotes_enabled: bool,
    /// Whether the marketplace is enabled on this instance.
    #[serde(default)]
    pub marketplace_enabled: bool,
}
