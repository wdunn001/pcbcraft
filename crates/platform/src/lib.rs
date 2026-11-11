//! Platform provider traits for file system, network, secrets, dialogs, and processes.
//!
//! Every capability that differs between wasm and native is exposed as a trait
//! here. Native code implements the traits with std, ureq, keyring, and rfd.
//! Browser code implements them with the web file system, fetch, browser storage,
//! and file pickers. The rest of the codebase calls the traits, never the
//! platform-specific APIs.
//!
//! Local rules: traits must be Send + Sync. Errors must be typed enums, not
//! strings. No unwrap, expect, panic, todo, or unsafe.
//!
//! # Dependencies
//! - `clippy.toml` (the ban on unwrap/expect/panic)
//! - `xtask/src/main.rs` (layer 0, no workspace dependencies)
//!
//! # Do not
//! - Add workspace dependencies; this crate sits below every other.
//! - Put platform-specific code here; only the traits and shared types.

/// Read and write files on the target platform.
///
/// # Implementations
/// - none yet
pub trait FileSystem: Send + Sync {
    /// Read the entire contents of a file as text.
    fn read_file(&self, path: &str) -> Result<String, PlatformError>;

    /// Write text to a file, creating or replacing it.
    fn write_file(&self, path: &str, contents: &str) -> Result<(), PlatformError>;

    /// List the entries in a directory.
    fn read_dir(&self, path: &str) -> Result<Vec<String>, PlatformError>;

    /// Create a directory and any missing parents.
    fn create_dir_all(&self, path: &str) -> Result<(), PlatformError>;

    /// Check whether a path exists.
    fn exists(&self, path: &str) -> bool;

    /// Check whether a path is a directory.
    fn is_dir(&self, path: &str) -> bool;
}

/// Open a URL and return the response.
///
/// # Implementations
/// - none yet
pub trait Network: Send + Sync {
    /// Fetch a URL and return the response body.
    fn get(&self, url: &str) -> Result<NetworkResponse, PlatformError>;

    /// POST JSON to a URL and return the response body.
    fn post_json(&self, url: &str, body: &str) -> Result<NetworkResponse, PlatformError>;
}

/// Store and retrieve secrets outside the project.
///
/// # Implementations
/// - none yet
pub trait SecretStore: Send + Sync {
    /// Get a secret by name.
    fn get(&self, name: &str) -> Result<Option<String>, PlatformError>;

    /// Set a secret.
    fn set(&self, name: &str, value: &str) -> Result<(), PlatformError>;

    /// Delete a secret.
    fn delete(&self, name: &str) -> Result<(), PlatformError>;
}

/// Show a dialog to the user.
///
/// # Implementations
/// - none yet
pub trait Dialogs: Send + Sync {
    /// Show an open file dialog and return the selected path, if any.
    fn open_file(&self) -> Result<Option<String>, PlatformError>;

    /// Show a save file dialog and return the chosen path, if any.
    fn save_file(&self) -> Result<Option<String>, PlatformError>;

    /// Show a message box.
    fn message(&self, title: &str, message: &str) -> Result<(), PlatformError>;
}

/// Run an external process.
///
/// # Implementations
/// - none yet
pub trait Processes: Send + Sync {
    /// Run a command and return its exit code and output.
    fn run(&self, command: &str) -> Result<ProcessOutput, PlatformError>;
}

/// A response from a network call.
#[derive(Clone, Debug)]
pub struct NetworkResponse {
    pub status: u16,
    pub body: String,
}

/// The output of a process.
#[derive(Clone, Debug)]
pub struct ProcessOutput {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

/// A platform-specific error.
#[derive(Debug, thiserror::Error)]
pub enum PlatformError {
    #[error("{0}")]
    Failed(String),
}
