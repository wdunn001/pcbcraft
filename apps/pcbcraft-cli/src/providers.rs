//! The command line tool's only door to the file system.
//! std::fs lives here and nowhere else in this crate.
//! Saves go through a temp file and a rename so an interrupted run
//! never truncates a project.
//!
//! # Rules
//! Every call answers with Result, no unwrap, expect or panic.
//! Reads and writes are size-bounded.
//!
//! # Dependencies
//! apps/pcbcraft-cli/src/main.rs
//!
//! # Do not
//! No network and no keychain here, the desktop app owns those.

use std::path::Path;

/// Hard cap for any single file we read or write.
#[allow(dead_code)]
const MAX_FILE_BYTES: usize = 32 * 1024 * 1024;

/// Thin wrapper around std::fs for the CLI.
///
/// # Rules
/// Writes land through a same-folder temp file and a rename.
///
/// # Dependencies
/// std::fs only, see this file.
///
/// # Do not
/// Never call this from a shared crate.
#[allow(dead_code)]
#[derive(Debug, Default)]
pub struct NativeFs;

impl NativeFs {
    /// Read the whole file into memory.
    ///
    /// # Rules
    /// Rejects content longer than MAX_FILE_BYTES.
    ///
    /// # Dependencies
    /// std::fs only, see this file.
    ///
    /// # Do not
    /// Do not use for streaming large files.
    pub fn read(&self, path: &Path) -> Result<Vec<u8>, String> {
        let bytes =
            std::fs::read(path).map_err(|e| format!("could not read {}: {e}", path.display()))?;
        if bytes.len() > MAX_FILE_BYTES {
            return Err("the file is too large to open".to_string());
        }
        Ok(bytes)
    }

    /// Write bytes atomically via a temp file and rename.
    ///
    /// # Rules
    /// Rejects payloads longer than MAX_FILE_BYTES.
    ///
    /// # Dependencies
    /// std::fs only, see this file.
    ///
    /// # Do not
    /// Do not call with a path whose parent cannot be created.
    pub fn write_atomic(&self, path: &Path, bytes: &[u8]) -> Result<(), String> {
        if bytes.len() > MAX_FILE_BYTES {
            return Err("the file is too large to save".to_string());
        }
        if let Some(parent) = path.parent()
            && !parent.exists()
        {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("could not save {}: {e}", path.display()))?;
        }
        let temp = path.with_extension("pcbcraft-tmp");
        std::fs::write(&temp, bytes).map_err(|e| {
            let _ = std::fs::remove_file(&temp);
            format!("could not save {}: {e}", path.display())
        })?;
        std::fs::rename(&temp, path).map_err(|e| {
            let _ = std::fs::remove_file(&temp);
            format!("could not save {}: {e}", path.display())
        })?;
        Ok(())
    }

    /// Remove a file, ignoring the case where it is already gone.
    ///
    /// # Rules
    /// Returns Ok when the path does not exist.
    ///
    /// # Dependencies
    /// std::fs only, see this file.
    ///
    /// # Do not
    /// Do not use to remove directories.
    pub fn remove(&self, path: &Path) -> Result<(), String> {
        if !path.exists() {
            return Ok(());
        }
        std::fs::remove_file(path)
            .map_err(|e| format!("could not remove {}: {e}", path.display()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_writes_and_reads_a_file() -> Result<(), String> {
        let fs = NativeFs;
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let path = dir.path().join("project.pcbcraft");
        let payload = b"hello pcbcraft";
        fs.write_atomic(&path, payload)?;
        let back = fs.read(&path)?;
        if back != payload {
            return Err("round trip mismatch".to_string());
        }
        Ok(())
    }

    #[test]
    fn write_atomic_replaces_an_existing_file_and_leaves_no_temp_file() -> Result<(), String> {
        let fs = NativeFs;
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let path = dir.path().join("project.pcbcraft");
        fs.write_atomic(&path, b"first")?;
        fs.write_atomic(&path, b"second")?;
        let back = fs.read(&path)?;
        if back != b"second" {
            return Err("second write did not replace".to_string());
        }
        let temp = dir.path().join("project.pcbcraft-tmp");
        if temp.exists() {
            return Err("temp file was left behind".to_string());
        }
        Ok(())
    }

    #[test]
    fn remove_deletes_a_file_and_ignores_a_missing_one() -> Result<(), String> {
        let fs = NativeFs;
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let path = dir.path().join("project.pcbcraft");
        fs.write_atomic(&path, b"data")?;
        fs.remove(&path)?;
        if path.exists() {
            return Err("file still exists after remove".to_string());
        }
        fs.remove(&path)?;
        Ok(())
    }

    #[test]
    fn save_rejects_a_body_over_the_size_limit() -> Result<(), String> {
        let fs = NativeFs;
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let path = dir.path().join("project.pcbcraft");
        fs.write_atomic(&path, &[])?;
        match fs.write_atomic(&path, &vec![0u8; MAX_FILE_BYTES + 1]) {
            Ok(()) => return Err("an oversized save was accepted".to_string()),
            Err(message) => {
                if !message.contains("too large") {
                    return Err("wrong error for oversized save".to_string());
                }
            }
        }
        Ok(())
    }
}
