//! Shared pieces of the browser entry point: online library transport and tests.
//!
//! # Rules
//! - Network tests replay fixtures from the library crate; nothing reaches a live host in CI.
//!
//! # Dependencies
//! - `crates/library/src/open_library.rs`
//! - `apps/pcbcraft-web/src/main.rs`
//!
//! # Do not
//! - Block the UI thread on the browser fetch API inside `Fetcher::get`.

pub mod transport;
