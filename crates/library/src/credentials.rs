//! Storage for per-user distributor keys. Keys never belong in project files.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Names of values a distributor setup card asks for.
pub mod keys {
    pub const DIGIKEY_CLIENT_ID: &str = "digikey.client_id";
    pub const DIGIKEY_CLIENT_SECRET: &str = "digikey.client_secret";
    pub const MOUSER_API_KEY: &str = "mouser.api_key";
    pub const ELEMENT14_API_KEY: &str = "element14.api_key";
    pub const ELEMENT14_STORE_ID: &str = "element14.store_id";
    pub const LCSC_API_KEY: &str = "lcsc.api_key";
    pub const LCSC_API_SECRET: &str = "lcsc.api_secret";
    pub const JLCPCB_APP_ID: &str = "jlcpcb.app_id";
    pub const JLCPCB_ACCESS_KEY: &str = "jlcpcb.access_key";
    pub const JLCPCB_SECRET_KEY: &str = "jlcpcb.secret_key";
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CredentialError {
    #[error("{0}")]
    Failed(String),
}

/// Read and write secrets outside the project.
pub trait KeyStore: Send + Sync {
    fn get(&self, name: &str) -> Result<Option<String>, CredentialError>;
    fn set(&self, name: &str, value: &str) -> Result<(), CredentialError>;
    fn delete(&self, name: &str) -> Result<(), CredentialError>;
    /// When set, the UI must show this warning (plain-file fallback).
    fn storage_warning(&self) -> Option<&str> {
        None
    }
}

/// In-memory store for tests.
#[derive(Default)]
pub struct MemoryKeyStore {
    inner: Mutex<BTreeMap<String, String>>,
}

impl KeyStore for MemoryKeyStore {
    fn get(&self, name: &str) -> Result<Option<String>, CredentialError> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| CredentialError::Failed("the key store lock failed".into()))?
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
            .map_err(|_| CredentialError::Failed("the key store lock failed".into()))?
            .insert(name.to_owned(), value.to_owned());
        Ok(())
    }

    fn delete(&self, name: &str) -> Result<(), CredentialError> {
        self.inner
            .lock()
            .map_err(|_| CredentialError::Failed("the key store lock failed".into()))?
            .remove(name);
        Ok(())
    }
}

const FILE_WARNING: &str =
    "Keys are stored in a plain file on this machine because the system keychain is unavailable.";

/// JSON map on disk; used when the OS keychain cannot be opened.
pub struct FileKeyStore {
    path: PathBuf,
}

impl FileKeyStore {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
        }
    }

    fn read_map(&self) -> Result<BTreeMap<String, String>, CredentialError> {
        if !self.path.exists() {
            return Ok(BTreeMap::new());
        }
        let text = fs::read_to_string(&self.path)
            .map_err(|e| CredentialError::Failed(format!("could not read keys: {e}")))?;
        if text.len() > 256_000 {
            return Err(CredentialError::Failed("the key file is too large".into()));
        }
        serde_json::from_str(&text)
            .map_err(|e| CredentialError::Failed(format!("the key file is not valid: {e}")))
    }

    fn write_map(&self, map: &BTreeMap<String, String>) -> Result<(), CredentialError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| CredentialError::Failed(format!("could not save keys: {e}")))?;
        }
        let text = serde_json::to_string(map)
            .map_err(|e| CredentialError::Failed(format!("could not encode keys: {e}")))?;
        fs::write(&self.path, text)
            .map_err(|e| CredentialError::Failed(format!("could not save keys: {e}")))
    }
}

impl KeyStore for FileKeyStore {
    fn get(&self, name: &str) -> Result<Option<String>, CredentialError> {
        Ok(self.read_map()?.get(name).cloned())
    }

    fn set(&self, name: &str, value: &str) -> Result<(), CredentialError> {
        if name.len() > 128 || value.len() > 4096 {
            return Err(CredentialError::Failed(
                "the key name or value is too long".into(),
            ));
        }
        let mut map = self.read_map()?;
        map.insert(name.to_owned(), value.to_owned());
        self.write_map(&map)
    }

    fn delete(&self, name: &str) -> Result<(), CredentialError> {
        let mut map = self.read_map()?;
        map.remove(name);
        self.write_map(&map)
    }

    fn storage_warning(&self) -> Option<&str> {
        Some(FILE_WARNING)
    }
}

/// Prefer the OS keychain on native builds; fall back to a file.
pub fn default_key_store(fallback_path: impl AsRef<Path>) -> Box<dyn KeyStore> {
    #[cfg(all(not(target_arch = "wasm32"), feature = "distributor-network"))]
    {
        if let Some(store) = keyring_key_store() {
            return store;
        }
    }
    Box::new(FileKeyStore::new(fallback_path))
}

#[cfg(all(not(target_arch = "wasm32"), feature = "distributor-network"))]
fn keyring_key_store() -> Option<Box<dyn KeyStore>> {
    Some(Box::new(KeyringStore))
}

#[cfg(all(not(target_arch = "wasm32"), feature = "distributor-network"))]
struct KeyringStore;

#[cfg(all(not(target_arch = "wasm32"), feature = "distributor-network"))]
const KEYRING_SERVICE: &str = "PCBCraft part sources";

#[cfg(all(not(target_arch = "wasm32"), feature = "distributor-network"))]
impl KeyStore for KeyringStore {
    fn get(&self, name: &str) -> Result<Option<String>, CredentialError> {
        let entry = keyring::Entry::new(KEYRING_SERVICE, name)
            .map_err(|e| CredentialError::Failed(e.to_string()))?;
        match entry.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(CredentialError::Failed(e.to_string())),
        }
    }

    fn set(&self, name: &str, value: &str) -> Result<(), CredentialError> {
        if name.len() > 128 || value.len() > 4096 {
            return Err(CredentialError::Failed(
                "the key name or value is too long".into(),
            ));
        }
        keyring::Entry::new(KEYRING_SERVICE, name)
            .map_err(|e| CredentialError::Failed(e.to_string()))?
            .set_password(value)
            .map_err(|e| CredentialError::Failed(e.to_string()))
    }

    fn delete(&self, name: &str) -> Result<(), CredentialError> {
        match keyring::Entry::new(KEYRING_SERVICE, name)
            .map_err(|e| CredentialError::Failed(e.to_string()))?
            .delete_credential()
        {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(CredentialError::Failed(e.to_string())),
        }
    }
}
