//! Platform provider traits for portable code.
//!
//! Every dependency and library must work on wasm first. Where a capability
//! exists only natively (files, network, secrets, dialogs, processes), the
//! code asks an interface provider instead, so a native implementation and a
//! browser implementation plug in behind one trait.
//!
//! # Rules
//! - Traits are Send + Sync so they can be shared across threads.
//! - Methods return Result, never panic.
//! - Path types are opaque strings; the provider decides the layout.
//!
//! # Dependencies
//! - `crates/clock` (for time-based operations)
//! - `crates/geom` (for geometry in future providers)
//!
//! # Do not
//! - Call std::fs, std::net or std::process directly in portable code.
//! - Add platform-specific dependencies to this crate.

pub mod credentials;
pub mod filesystem;
pub mod http;

pub use credentials::KeyStore;
pub use filesystem::FileSystem;
pub use http::HttpClient;
