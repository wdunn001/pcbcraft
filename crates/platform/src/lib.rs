//! Platform provider traits and in-memory implementations for PCB design tools.
//!
//! This crate defines the abstract interfaces that connect the application
//! to the operating system and browser: file storage, HTTP, secrets, and
//! file pickers. It provides the `MemFs` in-memory file system for tests and
//! the wasm fallback, and re-exports the clock crate.
//!
//! # Rules
//! - Traits define the minimum surface the application layer requires.
//! - Implementations must be Send + Sync for shared use.
//! - `MemFs` is thread-safe; it uses internal synchronization.
//!
//! # Dependencies
//! - `pcbcraft-clock` (re-exported Clock functions)
//! - `serde` (serialization for MemFs state)
//! - `thiserror` (error types)
//!
//! # Do not
//! - Add platform-specific types to the trait signatures.
//! - Implement the traits for production use in this crate; use `pcbcraft-io`.

pub use pcbcraft_clock::{Instant, SystemTime, instant_now, system_now, unix_nanos, unix_secs};

mod file_picker;
mod file_store;
mod http_client;
mod memfs;
mod secret_store;

pub use file_picker::{FileHandle, FilePicker, OpenOptions, PickerError, SaveOptions};
pub use file_store::{FileMetadata, FileStore, FileStoreError};
pub use http_client::{HttpClient, HttpError, HttpRequest, HttpResponse};
pub use memfs::MemFs;
pub use secret_store::{SecretError, SecretStore};
