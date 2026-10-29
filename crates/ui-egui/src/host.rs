//! Host bridge for opening uploads and triggering downloads without std::fs in the UI crate.
use pcbcraft_doc::Project;
use pcbcraft_io::{OpenKind, open_document, save_bytes};
use std::sync::MutexGuard;

/// Browser or desktop hooks for byte-oriented project I/O.
pub trait ProjectHost {
    /// Return the next user-picked file, if one is ready.
    fn pick_open(&mut self) -> Option<(String, Vec<u8>)>;
    /// Download one file in the browser, or write through native paths when unset.
    fn save_download(&mut self, filename: &str, bytes: &[u8]) -> Result<(), String>;
    /// Download several named files as one zip archive.
    fn save_zip(&mut self, filename: &str, entries: &[(String, Vec<u8>)]) -> Result<(), String> {
        let zip = pcbcraft_io::zip_named(entries)?;
        self.save_download(filename, &zip)
    }
    fn display_name(&self) -> Option<String> {
        None
    }
}

/// Desktop default: filesystem and MCP paths stay in the existing menu handlers.
pub struct NativeHost;

impl ProjectHost for NativeHost {
    fn pick_open(&mut self) -> Option<(String, Vec<u8>)> {
        None
    }
    fn save_download(&mut self, _filename: &str, _bytes: &[u8]) -> Result<(), String> {
        Err("native builds save through file paths".into())
    }
}

/// In-memory host for unit tests and headless round-trip checks.
pub struct MemHost {
    pub open_queue: Vec<(String, Vec<u8>)>,
    pub downloads: Vec<(String, Vec<u8>)>,
}

impl MemHost {
    pub fn new() -> Self {
        Self {
            open_queue: Vec::new(),
            downloads: Vec::new(),
        }
    }
}

impl Default for MemHost {
    fn default() -> Self {
        Self::new()
    }
}

impl ProjectHost for MemHost {
    fn pick_open(&mut self) -> Option<(String, Vec<u8>)> {
        if self.open_queue.is_empty() {
            None
        } else {
            Some(self.open_queue.remove(0))
        }
    }
    fn save_download(&mut self, filename: &str, bytes: &[u8]) -> Result<(), String> {
        self.downloads.push((filename.to_string(), bytes.to_vec()));
        Ok(())
    }
}

fn lock_session<'a>(
    session: &'a pcbcraft_mcp::SharedSession,
) -> Result<MutexGuard<'a, pcbcraft_engine::Session>, String> {
    session
        .lock()
        .map_err(|_| "session unavailable after internal failure".to_string())
}

/// Apply an opened document to the shared session when it is a full project replacement.
pub fn replace_session_from_open(
    session: &pcbcraft_mcp::SharedSession,
    kind: OpenKind,
) -> Result<(), String> {
    match kind {
        OpenKind::Project(project) => {
            let mut current = lock_session(session)?;
            current
                .replace(*project)
                .map(|_| ())
                .map_err(|e| e.to_string())
        }
        _ => Err("only a full project file can replace the session here".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pcbcraft_mcp::shared;

    #[test]
    fn project_host_memory_open_edit_save_reopen_equal() -> Result<(), String> {
        let session = shared(pcbcraft_engine::Session::default());
        let mut host = MemHost::new();
        let seed = Project {
            name: "Host round trip".into(),
            ..Project::default()
        };
        let seed_bytes = save_bytes(&seed)?;
        host.open_queue
            .push(("roundtrip.pcbcraft".into(), seed_bytes));

        let (hint, bytes) = host.pick_open().ok_or("expected a queued open file")?;
        let kind = open_document(&bytes, &hint)?;
        replace_session_from_open(&session, kind)?;

        {
            let mut current = lock_session(&session)?;
            let mut edited = current.project.clone();
            edited.name = "Host round trip edited".into();
            current.replace(edited).map_err(|e| e.to_string())?;
        }

        let saved = {
            let current = lock_session(&session)?;
            save_bytes(&current.project)?
        };
        host.save_download("roundtrip.pcbcraft", &saved)?;

        let (_, reopened_bytes) = host.downloads.last().ok_or("download was not recorded")?;
        let reopened = open_document(reopened_bytes, "roundtrip.pcbcraft")?;
        match reopened {
            OpenKind::Project(project) => {
                let current = lock_session(&session)?;
                assert_eq!(*project, current.project);
            }
            _ => return Err("reopened file was not a project".into()),
        }
        Ok(())
    }
}
