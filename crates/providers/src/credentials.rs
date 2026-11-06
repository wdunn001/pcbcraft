//! Secret storage operations behind a trait for wasm and native builds.
//!
//! Portable code calls this trait; the native provider wraps the OS keychain
//! (keyring) and the browser provider uses IndexedDB or browser storage.
//!
//! # Rules
/// - Keys are short strings (names, not data).
/// - Values are secrets (API keys, tokens, passwords).
/// - Values are never stored in project files.
///
/// # Dependencies
/// - `crates/clock` (for expiry times)
///
/// # Do not
/// - Call keyring or browser storage directly in portable code; use this trait.
use std::collections::BTreeMap;
use std::sync::Mutex;

/// Why a credential operation failed.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CredentialError {
    /// The operation failed for a reason described in the string.
    #[error("{0}")]
    Failed(String),
}

/// Read and write secrets outside the project.
///
/// Portable code holds a Box<dyn KeyStore> and calls its methods. The
/// concrete implementation is chosen at startup based on the target platform.
///
/// # Rules
/// - All methods are Send + Sync so the store can be shared.
/// - Keys are short strings (under 128 characters).
/// - Values are secrets (under 4096 characters).
///
/// # Dependencies
/// - `crates/clock` (for expiry times)
///
/// # Do not
/// - Call keyring or browser storage directly in portable code; use this trait.
pub trait KeyStore: Send + Sync {
    /// Read a secret by name. Returns None if not found.
    fn get(&self, name: &str) -> Result<Option<String>, CredentialError>;

    /// Write a secret by name.
    fn set(&self, name: &str, value: &str) -> Result<(), CredentialError>;

    /// Delete a secret by name.
    fn delete(&self, name: &str) -> Result<(), CredentialError>;

    /// If the store is insecure, return a warning message.
    fn storage_warning(&self) -> Option<&str> {
        None
    }
}

/// In-memory key store for tests.
#[derive(Default)]
pub struct MemoryKeyStore {
    inner: Mutex<BTreeMap<String, String>>,
}

impl MemoryKeyStore {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(BTreeMap::new()),
        }
    }
}

impl KeyStore for MemoryKeyStore {
    fn get(&self, name: &str) -> Result<Option<String>, CredentialError> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| CredentialError::Failed("lock failed".into()))?
            .get(name)
            .cloned())
    }

    fn set(&self, name: &str, value: &str) -> Result<(), CredentialError> {
        if name.len() > 128 || value.len() > 4096 {
            return Err(CredentialError::Failed(
                "the key name or value is too long".into(),
            ));
        }
        self.inner
            .lock()
            .map_err(|_| CredentialError::Failed("lock failed".into()))?
            .insert(name.to_owned(), value.to_owned());
        Ok(())
    }

    fn delete(&self, name: &str) -> Result<(), CredentialError> {
        self.inner
            .lock()
            .map_err(|_| CredentialError::Failed("lock failed".into()))?
            .remove(name);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_key_store_set_and_get() {
        let store = MemoryKeyStore::new();
        store.set("api_key", "secret123").unwrap();
        assert_eq!(store.get("api_key").unwrap().unwrap(), "secret123");
    }

    #[test]
    fn memory_key_store_delete() {
        let store = MemoryKeyStore::new();
        store.set("api_key", "secret123").unwrap();
        store.delete("api_key").unwrap();
        assert_eq!(store.get("api_key").unwrap(), None);
    }
}
