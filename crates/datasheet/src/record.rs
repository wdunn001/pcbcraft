//! Metadata stored beside cached PDF bytes (desktop disk layout and browser index).

use serde::{Deserialize, Serialize};

/// One cached datasheet file. Project files store only the URL and hash, never the bytes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CacheRecord {
    /// Lower-case hex SHA-256 of the PDF bytes.
    pub sha256: String,
    /// Source link when the file came from the web; absent for a local pick.
    pub source_url: Option<String>,
    /// RFC 3339 timestamp when the bytes were stored.
    pub fetched_at: String,
    /// Entity tag from the last successful fetch, when present.
    pub etag: Option<String>,
    /// Stored byte length.
    pub byte_len: u64,
    /// RFC 3339 timestamp updated on every open or store (LRU).
    pub last_used_at: String,
}

impl CacheRecord {
    /// Build a new record after bytes passed `gate::validate_pdf_bytes`.
    pub fn new(
        sha256: String,
        source_url: Option<String>,
        etag: Option<String>,
        byte_len: u64,
        now: &str,
    ) -> Self {
        Self {
            sha256,
            source_url,
            fetched_at: now.to_owned(),
            etag,
            byte_len,
            last_used_at: now.to_owned(),
        }
    }

    /// Mark the entry as recently used (LRU touch).
    pub fn touch(&mut self, now: &str) {
        self.last_used_at = now.to_owned();
    }
}
