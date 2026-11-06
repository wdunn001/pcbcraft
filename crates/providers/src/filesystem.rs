//! File system operations behind a trait for wasm and native builds.
//!
//! Portable code calls this trait; the native provider wraps std::fs and the
//! browser provider uses the File System Access API or IndexedDB. Paths are
//! opaque strings: the provider maps them to real locations.
//!
//! # Rules
//! - Paths use forward slashes on every platform.
//! - Byte ranges are bounded; callers must validate before passing them here.
//! - Errors are descriptive strings, never panics.
//!
//! # Dependencies
//! - `crates/clock` (for modification times)
//!
//! # Do not
//! - Use std::fs types in portable code; use this trait instead.
//! - Pass unbounded paths; the provider may impose limits.

use std::collections::BTreeMap;
use std::sync::Mutex;

/// A file system entry returned by listing a directory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirEntry {
    /// The name of the entry (not the full path).
    pub name: String,
    /// True if this entry is a directory.
    pub is_dir: bool,
}

/// Why a file system operation failed.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum FsError {
    /// The operation failed for a reason described in the string.
    #[error("{0}")]
    Failed(String),
}

/// A file system that works on wasm and native builds.
///
/// Portable code holds a Box<dyn FileSystem> and calls its methods. The
/// concrete implementation is chosen at startup based on the target platform.
///
/// # Rules
/// - All methods are Send + Sync so the file system can be shared.
/// - Paths are opaque strings with forward slashes.
/// - Byte sizes are bounded by the caller; this trait does not enforce limits.
///
/// # Dependencies
/// - `crates/clock` (for modification times)
///
/// # Do not
/// - Call std::fs directly in portable code; use this trait instead.
pub trait FileSystem: Send + Sync {
    /// Read the entire contents of a file as bytes.
    fn read(&self, path: &str) -> Result<Vec<u8>, FsError>;

    /// Read the entire contents of a file as UTF-8 text.
    fn read_to_string(&self, path: &str) -> Result<String, FsError>;

    /// Write bytes to a file, creating or overwriting it.
    fn write(&self, path: &str, bytes: &[u8]) -> Result<(), FsError>;

    /// Write text to a file, creating or overwriting it.
    fn write_string(&self, path: &str, text: &str) -> Result<(), FsError>;

    /// Remove a file.
    fn remove_file(&self, path: &str) -> Result<(), FsError>;

    /// Remove a directory and all its contents.
    fn remove_dir_all(&self, path: &str) -> Result<(), FsError>;

    /// Create a directory and all its parents if they do not exist.
    fn create_dir_all(&self, path: &str) -> Result<(), FsError>;

    /// List the entries in a directory.
    fn read_dir(&self, path: &str) -> Result<Vec<DirEntry>, FsError>;

    /// Copy a file from src to dest.
    fn copy(&self, src: &str, dest: &str) -> Result<(), FsError>;

    /// Rename or move a file or directory.
    fn rename(&self, src: &str, dest: &str) -> Result<(), FsError>;

    /// Check if a path exists.
    fn exists(&self, path: &str) -> bool;

    /// Get the modification time of a path in seconds since epoch.
    fn modified(&self, path: &str) -> Option<u64>;
}

/// In-memory file system for tests.
#[derive(Default)]
pub struct MemoryFs {
    files: Mutex<BTreeMap<String, Vec<u8>>>,
    dirs: Mutex<BTreeMap<String, bool>>,
}

impl MemoryFs {
    pub fn new() -> Self {
        Self {
            files: Mutex::new(BTreeMap::new()),
            dirs: Mutex::new(BTreeMap::new()),
        }
    }
}

impl FileSystem for MemoryFs {
    fn read(&self, path: &str) -> Result<Vec<u8>, FsError> {
        self.files
            .lock()
            .map_err(|_| FsError::Failed("lock failed".into()))?
            .get(path)
            .cloned()
            .ok_or_else(|| FsError::Failed(format!("file not found: {path}")))
    }

    fn read_to_string(&self, path: &str) -> Result<String, FsError> {
        let bytes = self.read(path)?;
        String::from_utf8(bytes).map_err(|e| FsError::Failed(format!("not UTF-8: {e}")))
    }

    fn write(&self, path: &str, bytes: &[u8]) -> Result<(), FsError> {
        self.files
            .lock()
            .map_err(|_| FsError::Failed("lock failed".into()))?
            .insert(path.to_string(), bytes.to_vec());
        Ok(())
    }

    fn write_string(&self, path: &str, text: &str) -> Result<(), FsError> {
        self.write(path, text.as_bytes())
    }

    fn remove_file(&self, path: &str) -> Result<(), FsError> {
        self.files
            .lock()
            .map_err(|_| FsError::Failed("lock failed".into()))?
            .remove(path);
        Ok(())
    }

    fn remove_dir_all(&self, path: &str) -> Result<(), FsError> {
        let prefix = path.to_string();
        let mut files = self
            .files
            .lock()
            .map_err(|_| FsError::Failed("lock failed".into()))?;
        files.retain(|k, _| !k.starts_with(&prefix));
        let mut dirs = self
            .dirs
            .lock()
            .map_err(|_| FsError::Failed("lock failed".into()))?;
        dirs.retain(|k, _| !k.starts_with(&prefix));
        Ok(())
    }

    fn create_dir_all(&self, path: &str) -> Result<(), FsError> {
        self.dirs
            .lock()
            .map_err(|_| FsError::Failed("lock failed".into()))?
            .insert(path.to_string(), true);
        Ok(())
    }

    fn read_dir(&self, path: &str) -> Result<Vec<DirEntry>, FsError> {
        let prefix = if path.ends_with('/') {
            path.to_string()
        } else {
            format!("{path}/")
        };
        let files = self
            .files
            .lock()
            .map_err(|_| FsError::Failed("lock failed".into()))?;
        let dirs = self
            .dirs
            .lock()
            .map_err(|_| FsError::Failed("lock failed".into()))?;
        let mut entries = Vec::new();
        for (k, _) in files.iter() {
            if k.starts_with(&prefix) {
                let rest = &k[prefix.len()..];
                if !rest.contains('/') {
                    entries.push(DirEntry {
                        name: rest.to_string(),
                        is_dir: false,
                    });
                }
            }
        }
        for (k, _) in dirs.iter() {
            if k.starts_with(&prefix) {
                let rest = &k[prefix.len()..];
                if !rest.contains('/') {
                    entries.push(DirEntry {
                        name: rest.to_string(),
                        is_dir: true,
                    });
                }
            }
        }
        Ok(entries)
    }

    fn copy(&self, src: &str, dest: &str) -> Result<(), FsError> {
        let bytes = self.read(src)?;
        self.write(dest, &bytes)
    }

    fn rename(&self, src: &str, dest: &str) -> Result<(), FsError> {
        let bytes = self.read(src)?;
        self.write(dest, &bytes)?;
        self.remove_file(src)?;
        Ok(())
    }

    fn exists(&self, path: &str) -> bool {
        self.files
            .lock()
            .map(|f| f.contains_key(path))
            .unwrap_or(false)
    }

    fn modified(&self, _path: &str) -> Option<u64> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_fs_write_and_read() {
        let fs = MemoryFs::new();
        fs.write_string("/test.txt", "hello").unwrap();
        assert_eq!(fs.read_to_string("/test.txt").unwrap(), "hello");
    }

    #[test]
    fn memory_fs_exists() {
        let fs = MemoryFs::new();
        assert!(!fs.exists("/missing.txt"));
        fs.write_string("/present.txt", "x").unwrap();
        assert!(fs.exists("/present.txt"));
    }
}
