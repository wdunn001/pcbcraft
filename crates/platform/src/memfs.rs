//! In-memory file system for tests and the wasm fallback.
//!
//! Uses a simple map of path strings to content. Supports directories
//! implicitly (a path ending in "/" is a directory). Thread-safe via
//! internal Mutex.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};

use pcbcraft_clock::{SystemTime, unix_secs};
use std::time::Duration;

use crate::file_store::{FileMetadata, FileStore, FileStoreError};

struct Entry {
    content: Vec<u8>,
    is_dir: bool,
    modified: u64,
}

pub struct MemFs {
    inner: Mutex<MemFsInner>,
}

struct MemFsInner {
    entries: HashMap<String, Entry>,
}

impl Default for MemFs {
    fn default() -> Self {
        let mut entries = HashMap::new();
        entries.insert(
            "/".to_string(),
            Entry {
                content: Vec::new(),
                is_dir: true,
                modified: 0,
            },
        );
        Self {
            inner: Mutex::new(MemFsInner { entries }),
        }
    }
}

impl MemFs {
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> MutexGuard<'_, MemFsInner> {
        self.inner.lock().expect("MemFs lock poisoned")
    }

    fn parent(path: &str) -> Option<String> {
        let path = path.trim_end_matches('/');
        if let Some(pos) = path.rfind('/') {
            if pos == 0 {
                return Some("/".to_string());
            }
            return Some(path[..pos].to_string());
        }
        None
    }

    fn basename(path: &str) -> String {
        let path = path.trim_end_matches('/');
        if let Some(pos) = path.rfind('/') {
            path[pos + 1..].to_string()
        } else {
            path.to_string()
        }
    }

    fn list_dir(&self, path: &str) -> Result<Vec<String>, FileStoreError> {
        let inner = self.lock();
        let path = path.trim_end_matches('/').to_string();
        let entry = inner
            .entries
            .get(&path)
            .ok_or(FileStoreError::NotFound(path.clone()))?;
        if !entry.is_dir {
            return Err(FileStoreError::NotDirectory(path));
        }

        let mut names = Vec::new();
        for (key, _val) in &inner.entries {
            if key == &path {
                continue;
            }
            if let Some(parent) = Self::parent(key) {
                if parent == path {
                    names.push(Self::basename(key));
                }
            }
        }
        Ok(names)
    }
}

impl FileStore for MemFs {
    fn read(&self, path: &str) -> Result<Vec<u8>, FileStoreError> {
        let inner = self.lock();
        let entry = inner
            .entries
            .get(path)
            .ok_or(FileStoreError::NotFound(path.to_string()))?;
        if entry.is_dir {
            return Err(FileStoreError::IsDirectory(path.to_string()));
        }
        Ok(entry.content.clone())
    }

    fn write(&self, path: &str, content: &[u8]) -> Result<(), FileStoreError> {
        let _ = (path, content);
        Err(FileStoreError::Io(
            "MemFs::write is not implemented; use write_atomic".to_string(),
        ))
    }

    fn write_atomic(&self, path: &str, content: &[u8]) -> Result<(), FileStoreError> {
        let path = path.to_string();
        let mut inner = self.lock();
        if let Some(parent) = Self::parent(&path) {
            let parent_entry = inner
                .entries
                .get(&parent)
                .ok_or(FileStoreError::NotFound(parent.clone()))?;
            if !parent_entry.is_dir {
                return Err(FileStoreError::NotDirectory(parent));
            }
        }
        inner.entries.insert(
            path,
            Entry {
                content: content.to_vec(),
                is_dir: false,
                modified: unix_secs(),
            },
        );
        Ok(())
    }

    fn list(&self, path: &str) -> Result<Vec<String>, FileStoreError> {
        self.list_dir(path)
    }

    fn exists(&self, path: &str) -> bool {
        let inner = self.lock();
        inner.entries.contains_key(path)
    }

    fn remove(&self, path: &str) -> Result<(), FileStoreError> {
        let path = path.to_string();
        let mut inner = self.lock();
        if inner.entries.remove(&path).is_none() {
            return Err(FileStoreError::NotFound(path));
        }
        Ok(())
    }

    fn remove_dir(&self, path: &str) -> Result<(), FileStoreError> {
        let path = path.to_string();
        let mut inner = self.lock();
        let entry = inner
            .entries
            .get(&path)
            .ok_or(FileStoreError::NotFound(path.clone()))?;
        if !entry.is_dir {
            return Err(FileStoreError::NotDirectory(path));
        }
        let names = self.list_dir(&path)?;
        if !names.is_empty() {
            return Err(FileStoreError::Io(format!("directory not empty: {path}")));
        }
        inner.entries.remove(&path);
        Ok(())
    }

    fn create_dir(&self, path: &str) -> Result<(), FileStoreError> {
        let path = path.to_string();
        let mut inner = self.lock();
        if inner.entries.contains_key(&path) {
            return Err(FileStoreError::AlreadyExists(path));
        }
        inner.entries.insert(
            path.clone(),
            Entry {
                content: Vec::new(),
                is_dir: true,
                modified: unix_secs(),
            },
        );
        Ok(())
    }

    fn create_dir_all(&self, path: &str) -> Result<(), FileStoreError> {
        let path = path.to_string();
        let mut inner = self.lock();
        if inner.entries.contains_key(&path) {
            return Ok(());
        }
        let mut current = path.clone();
        let mut dirs = Vec::new();
        while let Some(parent) = Self::parent(&current) {
            if inner.entries.contains_key(&parent) {
                break;
            }
            dirs.push(current.clone());
            current = parent;
        }
        for d in dirs.into_iter().rev() {
            inner.entries.insert(
                d.clone(),
                Entry {
                    content: Vec::new(),
                    is_dir: true,
                    modified: unix_secs(),
                },
            );
        }
        // Create the final directory if it wasn't created
        if !inner.entries.contains_key(&path) {
            inner.entries.insert(
                path.clone(),
                Entry {
                    content: Vec::new(),
                    is_dir: true,
                    modified: unix_secs(),
                },
            );
        }
        Ok(())
    }

    fn rename(&self, from: &str, to: &str) -> Result<(), FileStoreError> {
        let from = from.to_string();
        let to = to.to_string();
        let mut inner = self.lock();
        let entry = inner
            .entries
            .remove(&from)
            .ok_or(FileStoreError::NotFound(from))?;
        inner.entries.insert(to, entry);
        Ok(())
    }

    fn metadata(&self, path: &str) -> Result<FileMetadata, FileStoreError> {
        let path = path.to_string();
        let inner = self.lock();
        let entry = inner
            .entries
            .get(&path)
            .ok_or(FileStoreError::NotFound(path))?;
        Ok(FileMetadata {
            size: entry.content.len() as u64,
            modified: SystemTime::UNIX_EPOCH + Duration::from_secs(entry.modified),
            is_dir: entry.is_dir,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_and_read_roundtrip() {
        let fs = MemFs::new();
        fs.create_dir_all("/project/src").expect("create dirs");
        fs.write_atomic("/project/src/main.rs", b"fn main() {}")
            .expect("write file");
        let content = fs.read("/project/src/main.rs").expect("read file");
        assert_eq!(content, b"fn main() {}");
    }

    #[test]
    fn list_directory() {
        let fs = MemFs::new();
        fs.create_dir_all("/project").expect("create dir");
        fs.write_atomic("/project/a.txt", b"a").expect("write a");
        fs.write_atomic("/project/b.txt", b"b").expect("write b");
        let names = fs.list("/project").expect("list dir");
        assert!(names.contains(&"a.txt".to_string()));
        assert!(names.contains(&"b.txt".to_string()));
    }

    #[test]
    fn exists_and_remove() {
        let fs = MemFs::new();
        fs.create_dir_all("/project").expect("create dir");
        fs.write_atomic("/project/file.txt", b"content")
            .expect("write");
        assert!(fs.exists("/project/file.txt"));
        fs.remove("/project/file.txt").expect("remove");
        assert!(!fs.exists("/project/file.txt"));
    }

    #[test]
    fn rename_file() {
        let fs = MemFs::new();
        fs.create_dir_all("/project").expect("create dir");
        fs.write_atomic("/project/old.txt", b"content")
            .expect("write");
        fs.rename("/project/old.txt", "/project/new.txt")
            .expect("rename");
        assert!(!fs.exists("/project/old.txt"));
        assert!(fs.exists("/project/new.txt"));
    }

    #[test]
    fn metadata_returns_correct_info() {
        let fs = MemFs::new();
        fs.create_dir_all("/project").expect("create dir");
        fs.write_atomic("/project/file.txt", b"hello")
            .expect("write");
        let meta = fs.metadata("/project/file.txt").expect("metadata");
        assert_eq!(meta.size, 5);
        assert!(!meta.is_dir);
    }
}
