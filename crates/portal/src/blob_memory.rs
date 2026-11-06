//! In-memory blob store for the portal service, keyed by lowercase sha256 hex.
//! Useful for tests and small installs where files never exceed the available RAM.
//!
//! # Rules
//! - `content_hash` must be exactly 64 lowercase hex characters.
//! - `put` verifies the hash matches the data; mismatched pairs are rejected.
//! - `put` is idempotent: writing the same hash with identical bytes is allowed.
//! - Data larger than 16 MiB is rejected as invalid.
//!
//! # Dependencies
//! - crates/portal/src/storage.rs (BlobStore trait)
//! - crates/portal/src/error.rs (PortalError)
//!
//! # Do not
//! - Use for production S3-backed deployments; that uses a separate driver.
use std::collections::BTreeMap;
use std::sync::{Mutex, PoisonError};

use sha2::{Digest, Sha256};

use crate::error::PortalError;
use crate::storage::BlobStore;

/// Lower-case hex SHA-256 of `bytes`.
///
/// # Rules
/// - The digest is computed in one pass; the result is always 64 hex characters.
///
/// # Dependencies
/// - crates/portal/Cargo.toml (the sha2 dependency)
///
/// # Do not
/// - Use for cryptographic proofs outside this crate; this is for content addressing only.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut out = String::with_capacity(64);
    for byte in digest {
        out.push(hex_nibble((byte >> 4) & 0x0f));
        out.push(hex_nibble(byte & 0x0f));
    }
    out
}

fn hex_nibble(n: u8) -> char {
    match n {
        0..=9 => (b'0' + n) as char,
        10 => 'a',
        11 => 'b',
        12 => 'c',
        13 => 'd',
        14 => 'e',
        15 => 'f',
        _ => '0',
    }
}

/// Maximum payload size for a single blob, 16 MiB.
const MAX_BLOB_BYTES: usize = 16 * 1024 * 1024;

/// In-memory implementation of the blob store trait.
///
/// # Rules
/// - Thread-safe via an interior Mutex; poisoned locks become Storage errors.
/// - Keys are lowercase sha256 hex strings; values are the raw bytes.
///
/// # Dependencies
/// - crates/portal/src/storage.rs (BlobStore trait)
/// - crates/portal/src/error.rs (PortalError)
///
/// # Do not
/// - Persist across process restarts; this is purely in-memory.
pub struct MemoryBlobStore {
    inner: Mutex<BTreeMap<String, Vec<u8>>>,
}

impl Default for MemoryBlobStore {
    fn default() -> Self {
        Self {
            inner: Mutex::new(BTreeMap::new()),
        }
    }
}

pub(crate) fn validate_hash_format(hash: &str) -> Result<(), PortalError> {
    if hash.len() != 64 {
        return Err(PortalError::Invalid(format!(
            "content hash must be 64 hex characters, got {}",
            hash.len()
        )));
    }
    for byte in hash.bytes() {
        if !byte.is_ascii_hexdigit() {
            return Err(PortalError::Invalid(format!(
                "content hash contains non-hex byte 0x{:02x}",
                byte
            )));
        }
    }
    // Must be lowercase.
    for byte in hash.bytes() {
        if byte.is_ascii_uppercase() {
            return Err(PortalError::Invalid(
                "content hash must be lowercase hex".to_string(),
            ));
        }
    }
    Ok(())
}

fn unwrap_poisoned<T>(result: Result<T, PoisonError<T>>) -> Result<T, PortalError> {
    match result {
        Ok(value) => Ok(value),
        Err(_poisoned) => Err(PortalError::Storage("mutex poisoned".to_string())),
    }
}

/// Contract: crates/portal/src/storage.rs
impl BlobStore for MemoryBlobStore {
    fn put(&self, content_hash: &str, data: &[u8]) -> Result<(), PortalError> {
        validate_hash_format(content_hash)?;
        if data.len() > MAX_BLOB_BYTES {
            return Err(PortalError::Invalid(format!(
                "blob exceeds {} byte limit",
                MAX_BLOB_BYTES
            )));
        }
        let actual_hash = sha256_hex(data);
        if actual_hash != content_hash {
            return Err(PortalError::Invalid(format!(
                "hash mismatch: expected {}, computed {}",
                content_hash, actual_hash
            )));
        }
        let mut map = unwrap_poisoned(self.inner.lock())?;
        if let Some(existing) = map.get(content_hash)
            && existing != data
        {
            return Err(PortalError::Conflict(format!(
                "hash {} already has different data",
                content_hash
            )));
        }
        map.insert(content_hash.to_string(), data.to_vec());
        Ok(())
    }

    fn get(&self, content_hash: &str) -> Result<Vec<u8>, PortalError> {
        validate_hash_format(content_hash)?;
        let map = unwrap_poisoned(self.inner.lock())?;
        match map.get(content_hash) {
            Some(data) => Ok(data.clone()),
            None => Err(PortalError::NotFound(content_hash.to_string())),
        }
    }

    fn has(&self, content_hash: &str) -> Result<bool, PortalError> {
        validate_hash_format(content_hash)?;
        let map = unwrap_poisoned(self.inner.lock())?;
        Ok(map.contains_key(content_hash))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn put_then_get_round_trips() -> Result<(), PortalError> {
        let store = MemoryBlobStore::default();
        let data = b"abc";
        let hash = sha256_hex(data);
        assert_eq!(
            hash,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        store.put(&hash, data)?;
        let got = store.get(&hash)?;
        assert_eq!(got, data);
        Ok(())
    }

    #[test]
    fn put_refuses_bytes_that_do_not_match_the_hash() {
        let store = MemoryBlobStore::default();
        let data = b"abc";
        let hash = sha256_hex(data);
        let wrong_data = b"def";
        let result = store.put(&hash, wrong_data);
        assert!(matches!(result, Err(PortalError::Invalid(_))));
    }

    #[test]
    fn put_twice_is_idempotent() -> Result<(), PortalError> {
        let store = MemoryBlobStore::default();
        let data = b"abc";
        let hash = sha256_hex(data);
        store.put(&hash, data)?;
        store.put(&hash, data)?;
        Ok(())
    }

    #[test]
    fn get_missing_is_not_found() {
        let store = MemoryBlobStore::default();
        let hash = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        let result = store.get(hash);
        assert!(matches!(result, Err(PortalError::NotFound(_))));
    }

    #[test]
    fn put_refuses_an_uppercase_hash() {
        let store = MemoryBlobStore::default();
        let data = b"abc";
        let hash = sha256_hex(data);
        let upper = hash.to_uppercase();
        let result = store.put(&upper, data);
        assert!(matches!(result, Err(PortalError::Invalid(_))));
    }
}
