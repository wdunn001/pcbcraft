//! Static registry site generation for CI and offline bundles.
//!
//! # Rules
//! - Delegates to the library crate; no separate on-disk format lives here.
//!
//! # Dependencies
//! - crates/library/src/static_registry.rs
pub use pcbcraft_library::static_registry::{
    build_catalogue, generate_index_html, rebuild_site, verify_registry_manifest,
};
