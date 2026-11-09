//! Library service HTTP API request and response types with validation.
//!
//! # Rules
//! - All types use `deny_unknown_fields` for strict JSON parsing.
//! - `validate()` enforces all business rules before serialization.
//! - Path constants define the canonical API surface.
//!
//! # Dependencies
//! - crates/library/src/lib.rs
//!
//! # Do not
//! - Add fields that are not validated in `validate()`.

use serde::{Deserialize, Serialize};

/// Canonical API path for searching library records.
pub const SEARCH_PATH: &str = "/v1/library/search";
/// Canonical API path for retrieving a record by id.
pub const RECORD_PATH: &str = "/v1/library/records/{id}";
/// Canonical API path for retrieving a record by manufacturer part number.
pub const BY_PART_NUMBER_PATH: &str = "/v1/library/by-mpn/{mpn}";
/// Canonical API path for listing revisions of a record.
pub const REVISIONS_PATH: &str = "/v1/library/records/{id}/revisions";
/// Canonical API path for publishing a new record.
pub const PUBLISH_PATH: &str = "/v1/library/publish";

/// Maximum search result limit per page.
pub const MAX_SEARCH_LIMIT: u32 = 100;
/// Maximum tags allowed in a search request.
pub const MAX_SEARCH_TAGS: usize = 32;
/// Minimum search query length.
pub const MIN_SEARCH_NEED: usize = 1;
/// Maximum search query length.
pub const MAX_SEARCH_NEED: usize = 256;
/// SHA-256 hex digest length.
pub const HASH_HEX_LENGTH: usize = 64;

/// A search request for library records.
///
/// # Rules
/// - `need` is 1 to 256 characters.
/// - `tags` has at most 32 entries.
/// - `limit` is 1 to 100.
///
/// # Dependencies
/// - crates/library/src/lib.rs
///
/// # Do not
/// - Serialize without calling `validate()` first.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceSearchRequest {
    /// The search query text.
    pub need: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    /// Optional filter tags.
    pub tags: Vec<String>,
    /// Maximum results to return.
    pub limit: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// Pagination cursor.
    pub cursor: Option<String>,
}

impl ServiceSearchRequest {
    /// Validate all business rules on this request.
    ///
    /// # Rules
    /// - Returns `Err` with a plain reason when any rule is violated.
    /// - Does not modify the request.
    ///
    /// # Dependencies
    /// - crates/library/src/lib.rs
    ///
    /// # Do not
    /// - Call this in a hot loop; it is for admission control.
    pub fn validate(&self) -> Result<(), String> {
        if self.need.len() < MIN_SEARCH_NEED || self.need.len() > MAX_SEARCH_NEED {
            return Err("search query must be 1 to 256 characters".into());
        }
        if self.tags.len() > MAX_SEARCH_TAGS {
            return Err("at most 32 tags are allowed".into());
        }
        if self.limit < 1 || self.limit > MAX_SEARCH_LIMIT {
            return Err("limit must be 1 to 100".into());
        }
        Ok(())
    }
}

/// Kind of library record.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceRecordKind {
    /// A generic reusable component definition.
    Component,
    /// An orderable part from a specific distributor.
    Orderable,
}

/// A library record returned by search or get-by-id.
///
/// # Rules
/// - `content_hash` is a lowercase SHA-256 hex digest (64 characters).
/// - `mpn` may be empty for generic components.
/// - `fetched_at` is a Unix timestamp in seconds.
///
/// # Dependencies
/// - crates/library/src/lib.rs
///
/// # Do not
/// - Serialize without calling `validate()` first.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceRecord {
    /// Unique record identifier.
    pub id: String,
    /// SHA-256 hash of record content.
    pub content_hash: String,
    /// Record kind.
    pub kind: ServiceRecordKind,
    /// Manufacturer part number.
    pub mpn: String,
    /// Human-readable title.
    pub title: String,
    /// SPDX licence identifier.
    pub licence: String,
    /// Identifier of the source record.
    pub source_id: String,
    /// Unix timestamp when fetched.
    pub fetched_at: u64,
    /// Record revision number.
    pub revision: u32,
}

impl ServiceRecord {
    /// Validate all business rules on this record.
    ///
    /// # Rules
    /// - Returns `Err` with a plain reason when any rule is violated.
    /// - Does not modify the record.
    ///
    /// # Dependencies
    /// - crates/library/src/lib.rs
    ///
    /// # Do not
    /// - Call this in a hot loop; it is for admission control.
    pub fn validate(&self) -> Result<(), String> {
        if self.id.is_empty() {
            return Err("record id is required".into());
        }
        if self.content_hash.len() != HASH_HEX_LENGTH {
            return Err("content_hash must be a 64-character hex digest".into());
        }
        if !self.content_hash.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err("content_hash must be hexadecimal".into());
        }
        if self.title.is_empty() {
            return Err("record title is required".into());
        }
        if self.licence.is_empty() {
            return Err("record licence is required".into());
        }
        if self.source_id.is_empty() {
            return Err("record source_id is required".into());
        }
        Ok(())
    }
}

/// A paginated search response.
///
/// # Rules
/// - `items` has at most 100 entries.
/// - `next_cursor` is present when more results exist.
///
/// # Dependencies
/// - crates/library/src/lib.rs
///
/// # Do not
/// - Serialize without calling `validate()` first.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceSearchResponse {
    /// Matching records.
    pub items: Vec<ServiceRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// Cursor for next page.
    pub next_cursor: Option<String>,
}

impl ServiceSearchResponse {
    /// Validate all business rules on this response.
    ///
    /// # Rules
    /// - Returns `Err` with a plain reason when any rule is violated.
    /// - Does not modify the response.
    ///
    /// # Dependencies
    /// - crates/library/src/lib.rs
    ///
    /// # Do not
    /// - Call this in a hot loop; it is for admission control.
    pub fn validate(&self) -> Result<(), String> {
        if self.items.len() > MAX_SEARCH_LIMIT as usize {
            return Err("response has more than 100 items".into());
        }
        for item in &self.items {
            item.validate()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_api_fixtures_round_trip() -> Result<(), String> {
        let fixture = include_str!("../fixtures/service_api/search.json");
        let response: ServiceSearchResponse =
            serde_json::from_str(fixture).map_err(|e| e.to_string())?;
        response.validate()?;
        let serialized = serde_json::to_string(&response).map_err(|e| e.to_string())?;
        let reparsed: ServiceSearchResponse =
            serde_json::from_str(&serialized).map_err(|e| e.to_string())?;
        assert_eq!(response, reparsed);
        Ok(())
    }

    #[test]
    fn service_api_rejects_a_short_hash() {
        let mut record = ServiceRecord {
            id: "test".into(),
            content_hash: "abcdef".into(),
            kind: ServiceRecordKind::Component,
            mpn: String::new(),
            title: "test".into(),
            licence: "MIT".into(),
            source_id: "test".into(),
            fetched_at: 0,
            revision: 1,
        };
        assert!(record.validate().is_err());
        record.content_hash =
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string();
        assert!(record.validate().is_ok());
    }

    #[test]
    fn service_api_rejects_a_limit_of_zero() {
        let request = ServiceSearchRequest {
            need: "test".into(),
            tags: Vec::new(),
            limit: 0,
            cursor: None,
        };
        assert!(request.validate().is_err());
    }
}
