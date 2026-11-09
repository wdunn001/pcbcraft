//! Library service HTTP client over the shared HttpClient trait.
//!
//! # Rules
//! - Base URL must be https or http://localhost, at most 256 bytes.
//! - Search posts JSON to /v1/library/search and validates the response.
//! - record_by_mpn percent-encodes the part number and maps 404 to None.
//!
//! # Dependencies
//! - crates/library/src/http_client.rs
//! - crates/library/src/service_api.rs
//!
//! # Do not
//! - Add network transport code here; use the HttpClient trait only.

use crate::http_client::{HttpClient, HttpRequest};
use crate::service_api::{
    BY_PART_NUMBER_PATH, SEARCH_PATH, ServiceRecord, ServiceSearchRequest, ServiceSearchResponse,
};
use std::collections::BTreeMap;
use std::sync::Arc;

/// Maximum length of the service base URL in bytes.
pub const MAX_BASE_URL_BYTES: usize = 256;

/// Client for the library service HTTP API.
///
/// # Rules
/// - Takes an Arc<dyn HttpClient> for transport; tests supply RecordedHttp.
/// - Validates the base URL at construction.
///
/// # Dependencies
/// - crates/library/src/http_client.rs
/// - crates/library/src/service_api.rs
///
/// # Do not
/// - Store mutable state; the client is read-only after construction.
pub struct ServiceClient {
    http: Arc<dyn HttpClient>,
    base_url: String,
}

impl ServiceClient {
    /// Create a client for the given base URL.
    ///
    /// # Rules
    /// - Base URL must start with https:// or http://localhost.
    /// - Base URL must be at most 256 bytes.
    ///
    /// # Dependencies
    /// - crates/library/src/http_client.rs
    ///
    /// # Do not
    /// - Pass a URL with a trailing slash; it is rejected.
    pub fn new(http: Arc<dyn HttpClient>, base_url: &str) -> Result<Self, String> {
        if !base_url.starts_with("https://") && !base_url.starts_with("http://localhost") {
            return Err("base URL must start with https:// or http://localhost".into());
        }
        if base_url.len() > MAX_BASE_URL_BYTES {
            return Err("base URL is too long".into());
        }
        Ok(Self {
            http,
            base_url: base_url.to_string(),
        })
    }

    /// Search the library service for matching records.
    ///
    /// # Rules
    /// - POSTs JSON to /v1/library/search with content-type application/json.
    /// - Validates the response body against ServiceSearchResponse.
    ///
    /// # Dependencies
    /// - crates/library/src/service_api.rs
    ///
    /// # Do not
    /// - Call this without first validating the request.
    pub fn search(&self, request: &ServiceSearchRequest) -> Result<ServiceSearchResponse, String> {
        let body = serde_json::to_string(request).map_err(|e| e.to_string())?;
        let mut headers = BTreeMap::new();
        headers.insert("content-type".into(), "application/json".into());
        let http_request = HttpRequest {
            method: "POST".into(),
            url: format!("{}{}", self.base_url, SEARCH_PATH),
            headers,
            body: body.into_bytes(),
        };
        let http_response = self.http.call(&http_request).map_err(|e| e.to_string())?;
        if http_response.status != 200 {
            return Err(format!(
                "search failed with status {}",
                http_response.status
            ));
        }
        let text = String::from_utf8(http_response.body).map_err(|e| e.to_string())?;
        let response: ServiceSearchResponse =
            serde_json::from_str(&text).map_err(|e| e.to_string())?;
        response.validate()?;
        Ok(response)
    }

    /// Look up a record by manufacturer part number.
    ///
    /// # Rules
    /// - GETs /v1/library/by-mpn/{mpn} with the MPN percent-encoded.
    /// - Status 404 returns Ok(None); other non-200 is an error.
    ///
    /// # Dependencies
    /// - crates/library/src/service_api.rs
    ///
    /// # Do not
    /// - Pass an empty MPN; it is a caller error.
    pub fn record_by_mpn(&self, mpn: &str) -> Result<Option<ServiceRecord>, String> {
        let encoded = percent_encode_mpn(mpn);
        let url = format!(
            "{}{}",
            self.base_url,
            BY_PART_NUMBER_PATH.replace("{mpn}", &encoded)
        );
        let http_request = HttpRequest {
            method: "GET".into(),
            url,
            headers: BTreeMap::new(),
            body: Vec::new(),
        };
        let http_response = self.http.call(&http_request).map_err(|e| e.to_string())?;
        if http_response.status == 404 {
            return Ok(None);
        }
        if http_response.status != 200 {
            return Err(format!(
                "record lookup failed with status {}",
                http_response.status
            ));
        }
        let text = String::from_utf8(http_response.body).map_err(|e| e.to_string())?;
        let record: ServiceRecord = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        record.validate()?;
        Ok(Some(record))
    }
}

/// Percent-encode a manufacturer part number for use in a URL path.
///
/// # Rules
/// - Encodes every byte outside A-Z a-z 0-9 - _ . ~.
///
/// # Do not
/// - Use this for query strings; it is path-only.
fn percent_encode_mpn(mpn: &str) -> String {
    let mut out = String::with_capacity(mpn.len());
    for byte in mpn.bytes() {
        let c = byte as char;
        if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '~') {
            out.push(c);
        } else {
            out.push_str(&format!("%{:02X}", byte));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http_client::{RecordedHttp, get, post_json, response};

    fn fixture_search_response() -> String {
        include_str!("../fixtures/service_api/search.json").to_string()
    }

    #[test]
    fn client_searches_with_recorded_answers() -> Result<(), String> {
        let script = vec![(
            post_json(
                "https://lib.example.com/v1/library/search",
                "{\"need\":\"10k\"}",
            ),
            response(200, &fixture_search_response()),
        )];
        let http = Arc::new(RecordedHttp::new(script));
        let client = ServiceClient::new(http, "https://lib.example.com")?;
        let request = ServiceSearchRequest {
            need: "10k".into(),
            tags: Vec::new(),
            limit: 10,
            cursor: None,
        };
        let result = client.search(&request)?;
        assert_eq!(result.items.len(), 2);
        Ok(())
    }

    #[test]
    fn client_maps_404_to_none() -> Result<(), String> {
        let script = vec![(
            get("https://lib.example.com/v1/library/by-mpn/UNKNOWN"),
            response(404, "{}"),
        )];
        let http = Arc::new(RecordedHttp::new(script));
        let client = ServiceClient::new(http, "https://lib.example.com")?;
        let result = client.record_by_mpn("UNKNOWN")?;
        assert!(result.is_none());
        Ok(())
    }

    #[test]
    fn client_refuses_a_non_https_base_url() {
        let http = Arc::new(RecordedHttp::new(Vec::new()));
        let result = ServiceClient::new(http, "http://insecure.example.com");
        assert!(result.is_err());
    }

    #[test]
    fn client_encodes_a_mpn_with_a_slash() -> Result<(), String> {
        let script = vec![(
            get("https://lib.example.com/v1/library/by-mpn/A%2FB"),
            response(404, "{}"),
        )];
        let http = Arc::new(RecordedHttp::new(script));
        let client = ServiceClient::new(http, "https://lib.example.com")?;
        let result = client.record_by_mpn("A/B")?;
        assert!(result.is_none());
        Ok(())
    }
}
