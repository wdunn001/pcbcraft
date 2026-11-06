//! File storage abstraction for project files.
//!
//! Paths are logical project paths, not OS filesystem paths. The
//! implementation translates these to real locations.

use pcbcraft_clock::SystemTime;
use thiserror::Error;

#[derive(Error, Debug, PartialEq, Eq)]
pub enum FileStoreError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("already exists: {0}")]
    AlreadyExists(String),
    #[error("is a directory: {0}")]
    IsDirectory(String),
    #[error("not a directory: {0}")]
    NotDirectory(String),
    #[error("permission denied: {0}")]
    PermissionDenied(String),
    #[error("io error: {0}")]
    Io(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileMetadata {
    pub size: u64,
    pub modified: SystemTime,
    pub is_dir: bool,
}

pub trait FileStore: Send + Sync {
    fn read(&self, path: &str) -> Result<Vec<u8>, FileStoreError>;
    fn write(&self, path: &str, content: &[u8]) -> Result<(), FileStoreError>;
    fn write_atomic(&self, path: &str, content: &[u8]) -> Result<(), FileStoreError>;
    fn list(&self, path: &str) -> Result<Vec<String>, FileStoreError>;
    fn exists(&self, path: &str) -> bool;
    fn remove(&self, path: &str) -> Result<(), FileStoreError>;
    fn remove_dir(&self, path: &str) -> Result<(), FileStoreError>;
    fn create_dir(&self, path: &str) -> Result<(), FileStoreError>;
    fn create_dir_all(&self, path: &str) -> Result<(), FileStoreError>;
    fn rename(&self, from: &str, to: &str) -> Result<(), FileStoreError>;
    fn metadata(&self, path: &str) -> Result<FileMetadata, FileStoreError>;
}
