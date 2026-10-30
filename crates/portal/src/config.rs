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
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "driver", rename_all = "kebab-case")]
pub enum ObjectStoreConfig {
    LocalFolder { root: String },
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
