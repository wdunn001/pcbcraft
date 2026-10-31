//! Plain messages for live part sources when credentials are missing or rejected.

use crate::sources::SourceError;

/// Message shown when the vendor has not approved the user's application or key.
pub const UNAPPROVED_KEY_MESSAGE: &str = "This service has not approved the key yet";

/// Map HTTP status and optional vendor codes to a user-facing error.
pub fn map_auth_or_permission(status: u16, vendor_code: Option<i64>) -> Option<SourceError> {
    if status == 401 || status == 403 {
        return Some(SourceError::Unavailable(UNAPPROVED_KEY_MESSAGE.into()));
    }
    if let Some(code) = vendor_code {
        // LCSC: API access not enabled; JLCPCB/LCSC header validation failures.
        if matches!(code, 5003 | 4003 | 4005) {
            return Some(SourceError::Unavailable(UNAPPROVED_KEY_MESSAGE.into()));
        }
    }
    None
}

/// element14 partner API error codes for invalid credentials.
pub fn map_element14_errors(body: &[u8]) -> Option<SourceError> {
    let parsed: Element14Errors = serde_json::from_slice(body).ok()?;
    let code = parsed.errorCode.or(parsed.errors?.first().map(|e| e.code))?;
    if matches!(code, 100003 | 100004 | 100005 | 100007) {
        return Some(SourceError::Unavailable(UNAPPROVED_KEY_MESSAGE.into()));
    }
    None
}

#[derive(serde::Deserialize)]
struct Element14Errors {
    #[serde(default)]
    errorCode: Option<i64>,
    #[serde(default)]
    errors: Option<Vec<Element14ErrorEntry>>,
}

#[derive(serde::Deserialize)]
struct Element14ErrorEntry {
    #[serde(default)]
    code: i64,
}
