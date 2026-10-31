//! Types for the sharing portal service: HTTP API bodies, SQL row shapes, and
//! trait boundaries for object storage and the database. No network server lives
//! here yet; callers and future binaries share these definitions.
//!
//! # Rules
//! - Every JSON type uses `deny_unknown_fields` unless it is an open policy map.
//! - Content reads are keyed by lowercase sha256 hex; uploads are two-step (init, complete).
//! - The static registry feed matches the layout in crates/library static registry work (#987).
//!
//! # Dependencies
//! - docs/portal-api.md
//! - crates/portal/tests/fixtures/
//!
//! # Do not
//! - Commit deployment secrets, hostnames or lab addresses.
//! - Panic, unwrap or expect in library code paths.
pub mod accounts;
pub mod api;
pub mod config;
pub mod error;
pub mod feed;
pub mod moderation;
pub mod policy;
pub mod schema;
pub mod storage;

pub use error::PortalError;
