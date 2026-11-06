//! Local-folder blob store for the portal service, keyed by lowercase sha256 hex.
//! Content lives at <root>/<first-two-hex>/<full-hash> so that a single folder
//! never holds more than 256 files.
//!
//! # Rules
//! - `content_hash` must be exactly 64 lowercase hex characters.
//! - `put` verifies the hash matches the data; mismatched pairs are rejected.
//! - `put` is idempotent: writing the same hash with identical bytes is allowed.
//! - `get` re-hashes on read and rejects a file that no longer matches its key.
//! - Files are written to a temporary name first and renamed; readers never see
//!   a partial file.
//!
//! # Dependencies
//! - crates/portal/src/blob_memory.rs (validate_hash_format, sha256_hex)
//! - crates/portal/src/storage.rs (BlobStore trait)
//! - crates/portal/src/error.rs (PortalError)
//!
//! # Do not
//! - Use for high-throughput S3 deployments; this driver is for local installs.
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::blob_memory::{sha256_hex, validate_hash_format};
use crate::error::PortalError;
use crate::storage::BlobStore;

/// A content-addressed blob store backed by a local folder on disk.
///
/// # Rules
/// - Thread-safe; no internal synchronization is required because each blob
///   lives at its own path and atomic renames prevent partial reads.
/// - The root folder is created if it does not exist when the store is opened.
///
/// # Dependencies
/// - crates/portal/src/blob_memory.rs (validate_hash_format, sha256_hex)
/// - crates/portal/src/storage.rs (BlobStore trait)
///
/// # Do not
/// - Share the root folder between two instances with different layouts.
pub struct LocalFolderBlobStore {
    root: PathBuf,
}

/// Open a local-folder blob store at `root`, creating the folder if needed.
///
/// # Rules
/// - The folder is created with default permissions.
/// - An I/O error while creating the folder becomes a Storage error.
///
/// # Dependencies
/// - crates/portal/src/blob_local.rs (LocalFolderBlobStore)
///
/// # Do not
/// - Open the same root twice in different processes with incompatible layouts.
pub fn open(root: &Path) -> Result<LocalFolderBlobStore, PortalError> {
    fs::create_dir_all(root).map_err(|err| {
        PortalError::Storage(format!(
            "failed to create blob root {}: {}",
            root.display(),
            err
        ))
    })?;
    Ok(LocalFolderBlobStore {
        root: root.to_path_buf(),
    })
}

fn object_path(root: &Path, content_hash: &str) -> PathBuf {
    let prefix = &content_hash[0..2];
    root.join(prefix).join(content_hash)
}

fn validate_put(content_hash: &str, data: &[u8]) -> Result<(), PortalError> {
    validate_hash_format(content_hash)?;
    let actual_hash = sha256_hex(data);
    if actual_hash != content_hash {
        return Err(PortalError::Invalid(format!(
            "hash mismatch: expected {}, computed {}",
            content_hash, actual_hash
        )));
    }
    Ok(())
}

/// Contract: crates/portal/src/storage.rs
impl BlobStore for LocalFolderBlobStore {
    fn put(&self, content_hash: &str, data: &[u8]) -> Result<(), PortalError> {
        validate_put(content_hash, data)?;
        let path = object_path(&self.root, content_hash);
        // Create the prefix subfolder.
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|err| {
                PortalError::Storage(format!(
                    "failed to create prefix folder {}: {}",
                    parent.display(),
                    err
                ))
            })?;
        }
        // Write to a temporary name, then rename atomically.
        let temp_path = path.with_extension("tmp");
        {
            let mut file = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&temp_path)
                .map_err(|err| {
                    PortalError::Storage(format!(
                        "failed to open temp blob {}: {}",
                        temp_path.display(),
                        err
                    ))
                })?;
            file.write_all(data).map_err(|err| {
                PortalError::Storage(format!(
                    "failed to write temp blob {}: {}",
                    temp_path.display(),
                    err
                ))
            })?;
        }
        fs::rename(&temp_path, &path).map_err(|err| {
            PortalError::Storage(format!(
                "failed to rename temp blob {}: {}",
                temp_path.display(),
                err
            ))
        })?;
        Ok(())
    }

    fn get(&self, content_hash: &str) -> Result<Vec<u8>, PortalError> {
        validate_hash_format(content_hash)?;
        let path = object_path(&self.root, content_hash);
        let data = fs::read(&path).map_err(|err| {
            if err.kind() == io::ErrorKind::NotFound {
                PortalError::NotFound(content_hash.to_string())
            } else {
                PortalError::Storage(format!("failed to read blob {}: {}", path.display(), err))
            }
        })?;
        // Re-hash to detect corruption.
        let actual_hash = sha256_hex(&data);
        if actual_hash != content_hash {
            return Err(PortalError::Storage(
                "object does not match its hash".to_string(),
            ));
        }
        Ok(data)
    }

    fn has(&self, content_hash: &str) -> Result<bool, PortalError> {
        validate_hash_format(content_hash)?;
        let path = object_path(&self.root, content_hash);
        Ok(path.exists())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::write;

    /// Turn any debuggable error into the test result's string error.
    fn msg<E: std::fmt::Debug>(e: E) -> String {
        format!("{e:?}")
    }

    fn make_store() -> Result<(tempfile::TempDir, LocalFolderBlobStore), String> {
        let dir = tempfile::tempdir().map_err(msg)?;
        let store = open(dir.path()).map_err(msg)?;
        Ok((dir, store))
    }

    #[test]
    fn local_store_survives_a_new_instance() -> Result<(), String> {
        let (dir, store) = make_store()?;
        let data = b"hello";
        let hash = sha256_hex(data);
        store.put(&hash, data).map_err(msg)?;
        // Drop the first instance, open again.
        drop(store);
        let store2 = open(dir.path()).map_err(msg)?;
        let got = store2.get(&hash).map_err(msg)?;
        assert_eq!(got, data);
        Ok(())
    }

    #[test]
    fn local_store_detects_a_corrupted_object() -> Result<(), String> {
        let (dir, store) = make_store()?;
        let data = b"hello";
        let hash = sha256_hex(data);
        store.put(&hash, data).map_err(msg)?;
        // Corrupt the file on disk.
        let path = object_path(dir.path(), &hash);
        write(&path, b"corrupted").map_err(msg)?;
        let result = store.get(&hash);
        assert!(matches!(result, Err(PortalError::Storage(_))));
        Ok(())
    }

    #[test]
    fn local_store_refuses_a_traversal_hash() -> Result<(), String> {
        let (_dir, store) = make_store()?;
        let result = store.get("../../x");
        assert!(matches!(result, Err(PortalError::Invalid(_))));
        let data = b"test";
        let bad_hash = "../../x";
        let result2 = store.put(bad_hash, data);
        assert!(matches!(result2, Err(PortalError::Invalid(_))));
        Ok(())
    }

    #[test]
    fn local_store_put_is_idempotent() -> Result<(), String> {
        let (_dir, store) = make_store()?;
        let data = b"hello";
        let hash = sha256_hex(data);
        store.put(&hash, data).map_err(msg)?;
        store.put(&hash, data).map_err(msg)?;
        let got = store.get(&hash).map_err(msg)?;
        assert_eq!(got, data);
        Ok(())
    }

    #[test]
    fn local_store_get_missing_is_not_found() -> Result<(), String> {
        let (_dir, store) = make_store()?;
        let hash = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        let result = store.get(hash);
        assert!(matches!(result, Err(PortalError::NotFound(_))));
        Ok(())
    }

    #[test]
    fn local_store_has_reflects_put() -> Result<(), String> {
        let (_dir, store) = make_store()?;
        let data = b"hello";
        let hash = sha256_hex(data);
        assert!(!store.has(&hash).map_err(msg)?);
        store.put(&hash, data).map_err(msg)?;
        assert!(store.has(&hash).map_err(msg)?);
        Ok(())
    }
}
