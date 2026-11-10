//! Desktop native providers for the PCB Craft app.
//!
//! This module is the only place the desktop app touches the machine: files
//! through std::fs, the network through ureq, secrets through keyring, and the
//! file dialog through rfd. The shared crates name none of these and receive
//! the providers through their traits.
//!
//! # Rules
//! - Every call answers with Result or Option, never a panic.
//! - No wall-clock calls here; callers pass the time in.
//! - Every read and write is size-bounded.
//!
//! # Dependencies
//! - crates/library/src/http_client.rs
//! - crates/library/src/credentials.rs
//! - crates/mcp/src/project_host.rs
//!
//! # Do not
//! - Keep no business logic here.
//! - Do not call those crates from any other app file.

use pcbcraft_library::credentials::{CredentialError, KeyStore};
use pcbcraft_library::http_client::{HttpClient, HttpError, HttpRequest, HttpResponse};
use std::collections::BTreeMap;
use std::path::Path;

/// Largest file a provider will read or write.
///
/// This bounds the memory any provider may swallow in one operation. A provider
/// must never read or write more bytes than this in a single call.
///
/// # Rules
/// - No provider reads or writes more bytes than this.
///
/// # Dependencies
/// - apps/pcbcraft/src/providers.rs `std::fs`
///
/// # Do not
/// - Do not raise the limit without a decision logged in the changeset.
#[allow(dead_code)]
pub const MAX_FILE_BYTES: usize = 32 * 1024 * 1024;

/// Atomic file writer that never leaves a half-written file.
///
/// # Rules
/// - Writes land through a same-folder temp file and a rename so a crash never
///   leaves a half-written file.
///
/// # Dependencies
/// - apps/pcbcraft/src/providers.rs
///
/// # Do not
/// - Never use this from a shared crate.
#[allow(dead_code)]
#[derive(Debug, Default)]
pub struct NativeFs;

impl NativeFs {
    /// Read a file into memory.
    ///
    /// # Rules
    /// - Reject content longer than MAX_FILE_BYTES.
    ///
    /// # Dependencies
    /// - apps/pcbcraft/src/providers.rs `std::fs`
    ///
    /// # Do not
    /// - Do not call from a shared crate.
    pub fn read(&self, path: &Path) -> Result<Vec<u8>, String> {
        let bytes =
            std::fs::read(path).map_err(|e| format!("could not read {}: {e}", path.display()))?;
        if bytes.len() > MAX_FILE_BYTES {
            return Err("the file is too large to open".into());
        }
        Ok(bytes)
    }

    /// Write bytes to a file atomically.
    ///
    /// # Rules
    /// - Reject content longer than MAX_FILE_BYTES.
    /// - Write to a temp file then rename.
    ///
    /// # Dependencies
    /// - apps/pcbcraft/src/providers.rs `std::fs`
    ///
    /// # Do not
    /// - Do not call from a shared crate.
    pub fn write_atomic(&self, path: &Path, bytes: &[u8]) -> Result<(), String> {
        if bytes.len() > MAX_FILE_BYTES {
            return Err("the file is too large to save".into());
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

    /// Remove a file.
    ///
    /// # Rules
    /// - Return Ok when the path does not exist.
    ///
    /// # Dependencies
    /// - apps/pcbcraft/src/providers.rs `std::fs`
    ///
    /// # Do not
    /// - Do not call from a shared crate.
    pub fn remove(&self, path: &Path) -> Result<(), String> {
        if !path.exists() {
            return Ok(());
        }
        std::fs::remove_file(path)
            .map_err(|e| format!("could not remove {}: {e}", path.display()))?;
        Ok(())
    }
}

/// Desktop HTTP client behind the shared HttpClient trait.
///
/// Wraps ureq so the rest of the app talks to one stable interface.
///
/// # Rules
/// - One call in, one answer out.
/// - Read the body in a capped loop.
/// - Transport faults become plain sentences.
///
/// # Dependencies
/// - apps/pcbcraft/src/providers.rs `ureq`
///
/// # Do not
/// - Do not open sockets in another app file.
#[allow(dead_code)]
#[derive(Debug)]
pub struct UreqClient {
    agent: ureq::Agent,
}

impl Default for UreqClient {
    fn default() -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout(std::time::Duration::from_secs(30))
            .redirects(5)
            .build();
        Self { agent }
    }
}

impl UreqClient {
    /// Build a new HTTP client.
    ///
    /// Creates a client with a 30 second timeout and at most 5 redirects.
    /// This is shorthand for `UreqClient::default()`.
    ///
    /// # Rules
    /// - No argument, no shared global agent.
    ///
    /// # Dependencies
    /// - apps/pcbcraft/src/providers.rs `ureq`
    ///
    /// # Do not
    /// - Do not reuse one client across threads by pointer without care.
    pub fn new() -> Self {
        Self::default()
    }
}

/// Contract: crates/library/src/http_client.rs
impl HttpClient for UreqClient {
    /// Send an HTTP request and return the response.
    ///
    /// # Rules
    /// - Read the body in a capped loop.
    /// - Map transport errors to plain sentences.
    ///
    /// # Dependencies
    /// - apps/pcbcraft/src/providers.rs `ureq`
    ///
    /// # Do not
    /// - Do not call from a shared crate.
    fn call(&self, request: &HttpRequest) -> Result<HttpResponse, HttpError> {
        let mut req = self.agent.request(&request.method, &request.url);
        for (name, value) in &request.headers {
            req = req.set(name, value);
        }
        let sent = if request.method == "GET" || request.body.is_empty() {
            req.call()
        } else {
            req.send(request.body.as_slice())
        };
        let response = match sent {
            Ok(r) => r,
            Err(ureq::Error::Status(_, r)) => r,
            Err(e @ ureq::Error::Transport(_)) => {
                return Err(HttpError::Failed(transport_note(&e)));
            }
        };
        let mut headers = BTreeMap::new();
        for name in response.headers_names() {
            if let Some(value) = response.header(&name) {
                headers.insert(name, value.to_string());
            }
        }
        let status = response.status();
        let body = read_capped(response.into_reader(), MAX_FILE_BYTES)?;
        Ok(HttpResponse {
            status,
            headers,
            body,
        })
    }
}

fn read_capped(mut reader: impl std::io::Read, max: usize) -> Result<Vec<u8>, HttpError> {
    let mut bytes = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|e| HttpError::Failed(format!("could not read the response: {e}")))?;
        if n == 0 {
            break;
        }
        bytes.extend_from_slice(&buf[..n]);
        if bytes.len() > max {
            return Err(HttpError::Failed("the download is too large".into()));
        }
    }
    Ok(bytes)
}

fn transport_note(e: &ureq::Error) -> String {
    match e {
        ureq::Error::Transport(t) => match t.kind() {
            ureq::ErrorKind::Dns => "The address could not be found.".into(),
            ureq::ErrorKind::ConnectionFailed => "The server did not answer.".into(),
            ureq::ErrorKind::Io => "The connection broke or timed out.".into(),
            _ => "The network did not answer.".into(),
        },
        ureq::Error::Status(code, _) => format!("The server answered {code}."),
    }
}

/// System keychain implementation of the shared KeyStore trait.
///
/// Secrets are stored in the OS keychain (macOS Keychain, Windows Credential
/// Manager, or libsecret on Linux) so they never sit in project files or
/// environment variables.
///
/// # Rules
/// - Key names are capped at 128 characters; values at 4096.
/// - A missing entry answers None instead of an error.
/// - Deleting a missing entry succeeds.
///
/// # Dependencies
/// - crates/library/src/credentials.rs (KeyStore trait)
///
/// # Do not
/// - Store secrets in project files or version control.
#[allow(dead_code)]
pub struct KeyringStore;

/// The service name every secret is stored under.
const KEYRING_SERVICE: &str = "PCBCraft";

/// Contract: crates/library/src/credentials.rs
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

/// The operating system file dialog behind a small picker shape.
///
/// Shared crates never open dialogs themselves because they must run in
/// environments without a display. This struct lives in the app crate where
/// the rfd dependency is available and where the user is actually present.
///
/// # Rules
/// - A cancelled dialog answers None.
/// - An unreadable pick answers None so no half file reaches the caller.
///
/// # Dependencies
/// - crates/mcp/src/project_host.rs
///
/// # Do not
/// - Do not call it in a browser build.
#[allow(dead_code)]
pub struct RfdPicker;

impl RfdPicker {
    /// Open a file picker and read the chosen file.
    ///
    /// # Rules
    /// - Return None when nothing was chosen or the read failed.
    ///
    /// # Dependencies
    /// - apps/pcbcraft/src/providers.rs NativeFs
    ///
    /// # Do not
    /// - Do not call it in a browser build.
    pub fn pick_open(&self) -> Option<(String, Vec<u8>)> {
        let path = rfd::FileDialog::new().pick_file()?;
        let bytes = NativeFs.read(&path).ok()?;
        Some((path.display().to_string(), bytes))
    }

    /// Open a save dialog and return the chosen path.
    ///
    /// # Rules
    /// - Reject names longer than 255 characters without opening a dialog.
    ///
    /// # Dependencies
    /// - apps/pcbcraft/src/providers.rs NativeFs
    ///
    /// # Do not
    /// - Do not call it in a browser build.
    pub fn pick_save(&self, default_name: &str) -> Option<String> {
        if default_name.chars().count() > 255 {
            return None;
        }
        let path = rfd::FileDialog::new()
            .set_file_name(default_name)
            .save_file()?;
        Some(path.display().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_saved_file_survives_a_second_save() -> Result<(), String> {
        let dir = tempfile::tempdir().map_err(|e| format!("tempdir: {e}"))?;
        let fs = NativeFs;
        let path = dir.path().join("board.pcbcraft");
        fs.write_atomic(&path, b"first")?;
        fs.write_atomic(&path, b"second")?;
        let bytes = fs.read(&path)?;
        assert_eq!(&bytes, b"second");
        assert!(
            !dir.path().join("board.pcbcraft-tmp").exists(),
            "no temp file survives a save"
        );
        Ok(())
    }

    #[test]
    fn remove_ignores_a_file_that_was_never_there() -> Result<(), String> {
        let dir = tempfile::tempdir().map_err(|e| format!("tempdir: {e}"))?;
        let fs = NativeFs;
        let path = dir.path().join("gone.pcbcraft");
        fs.write_atomic(&path, b"data")?;
        fs.remove(&path)?;
        assert!(!path.exists());
        assert!(fs.remove(&path).is_ok());
        Ok(())
    }

    #[test]
    fn a_missing_file_is_reported_in_words() -> Result<(), String> {
        let dir = tempfile::tempdir().map_err(|e| format!("tempdir: {e}"))?;
        let fs = NativeFs;
        let path = dir.path().join("nope.pcbcraft");
        match fs.read(&path) {
            Ok(_) => Err("read should have failed".to_string()),
            Err(e) => {
                assert!(e.contains("could not read"));
                Ok(())
            }
        }
    }

    #[test]
    fn the_save_dialog_refuses_a_name_that_is_too_long() -> Result<(), String> {
        let picker = RfdPicker;
        assert!(
            picker.pick_save(&"x".repeat(256)).is_none(),
            "a too long name never opens a dialog"
        );
        Ok(())
    }

    #[test]
    fn the_key_store_refuses_an_overlong_name_before_the_keychain() -> Result<(), String> {
        let store = KeyringStore;
        match store.set(&"k".repeat(200), "v") {
            Ok(()) => Err("set should have failed".to_string()),
            Err(e) => {
                assert!(e.to_string().contains("too long"));
                Ok(())
            }
        }
    }
}
