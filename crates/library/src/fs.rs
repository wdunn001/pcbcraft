//! File system behind a trait so the library crate and the web build share one shape.
//!
//! # Rules
//! - Paths are always UTF-8 and canonical.
//! - Byte-oriented methods (`read_bytes`, `write_bytes`) are for binary data;
//!   text methods (`read_string`, `write_string`) are for UTF-8.
//!
//! # Dependencies
//! - `crates/io` for the byte-oriented project I/O that sits on top.
//!
//! # Do not
//! - Add async methods; the desktop and web builds are synchronous.

use std::path::Path;

/// Why a file system operation failed.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum FsError {
    #[error("{0}")]
    Failed(String),
}

/// Byte-oriented file system operations.
pub trait FileSystem: Send + Sync {
    fn exists(&self, path: &Path) -> bool;
    fn is_dir(&self, path: &Path) -> bool;
    fn read_bytes(&self, path: &Path) -> Result<Vec<u8>, FsError>;
    fn write_bytes(&self, path: &Path, data: &[u8]) -> Result<(), FsError>;
    fn read_string(&self, path: &Path) -> Result<String, FsError>;
    fn write_string(&self, path: &Path, text: &str) -> Result<(), FsError>;
    fn remove_file(&self, path: &Path) -> Result<(), FsError>;
    fn list_dir(&self, path: &Path) -> Result<Vec<String>, FsError>;
    fn create_dir_all(&self, path: &Path) -> Result<(), FsError>;
}

/// In-memory file system for unit tests.
pub struct MemFs {
    inner: std::sync::Mutex<std::collections::BTreeMap<String, Vec<u8>>>,
}

impl Default for MemFs {
    fn default() -> Self {
        Self::new()
    }
}

impl MemFs {
    pub fn new() -> Self {
        Self {
            inner: std::sync::Mutex::new(std::collections::BTreeMap::new()),
        }
    }
}

impl FileSystem for MemFs {
    fn exists(&self, path: &Path) -> bool {
        let map = self
            .inner
            .lock()
            .map_err(|_| "the mem-fs lock failed")
            .ok()
            .unwrap_or_default();
        let key = path.to_string_lossy().to_string();
        map.contains_key(&key)
    }

    fn is_dir(&self, _path: &Path) -> bool {
        false
    }

    fn read_bytes(&self, path: &Path) -> Result<Vec<u8>, FsError> {
        let map = self
            .inner
            .lock()
            .map_err(|_| FsError::Failed("the mem-fs lock failed".into()))?;
        let key = path.to_string_lossy().to_string();
        map.get(&key)
            .cloned()
            .ok_or_else(|| FsError::Failed(format!("file not found: {key}")))
    }

    fn write_bytes(&self, path: &Path, data: &[u8]) -> Result<(), FsError> {
        let mut map = self
            .inner
            .lock()
            .map_err(|_| FsError::Failed("the mem-fs lock failed".into()))?;
        let key = path.to_string_lossy().to_string();
        map.insert(key, data.to_vec());
        Ok(())
    }

    fn read_string(&self, path: &Path) -> Result<String, FsError> {
        let bytes = self.read_bytes(path)?;
        String::from_utf8(bytes).map_err(|e| FsError::Failed(format!("not utf-8: {e}")))
    }

    fn write_string(&self, path: &Path, text: &str) -> Result<(), FsError> {
        self.write_bytes(path, text.as_bytes())
    }

    fn remove_file(&self, path: &Path) -> Result<(), FsError> {
        let mut map = self
            .inner
            .lock()
            .map_err(|_| FsError::Failed("the mem-fs lock failed".into()))?;
        let key = path.to_string_lossy().to_string();
        if map.remove(&key).is_none() {
            return Err(FsError::Failed(format!("file not found: {key}")));
        }
        Ok(())
    }

    fn list_dir(&self, _path: &Path) -> Result<Vec<String>, FsError> {
        Err(FsError::Failed("list_dir not supported on mem-fs".into()))
    }

    fn create_dir_all(&self, _path: &Path) -> Result<(), FsError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn memfs_write_then_read() -> Result<(), String> {
        let fs = MemFs::new();
        let path = PathBuf::from("test.txt");
        fs.write_string(&path, "hello")?;
        assert_eq!(fs.read_string(&path)?, "hello");
        Ok(())
    }

    #[test]
    fn memfs_exists() -> Result<(), String> {
        let fs = MemFs::new();
        let path = PathBuf::from("test.txt");
        assert!(!fs.exists(&path));
        fs.write_string(&path, "hello")?;
        assert!(fs.exists(&path));
        Ok(())
    }

    #[test]
    fn memfs_remove() -> Result<(), String> {
        let fs = MemFs::new();
        let path = PathBuf::from("test.txt");
        fs.write_string(&path, "hello")?;
        fs.remove_file(&path)?;
        assert!(!fs.exists(&path));
        Ok(())
    }
}
