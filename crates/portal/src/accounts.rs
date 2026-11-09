//! Account and session shapes exposed by the HTTP API (not the SQL rows).

use serde::{Deserialize, Serialize};

use crate::schema::AccountRole;

/// Capabilities the authenticated caller may use on this request.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionView {
    pub account_id: String,
    pub role: AccountRole,
    pub anonymous: bool,
}

/// Request to start an email magic-link sign-in.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmailLinkStartRequest {
    pub email: String,
    pub return_url: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmailLinkStartResponse {
    pub pending: bool,
}

/// Create a named API token; the raw secret is returned once.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateTokenRequest {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateTokenResponse {
    pub token_id: String,
    pub secret: String,
}
