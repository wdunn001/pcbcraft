//! Plugin hooks for exports that live outside the core crate.
//!
//! Context: issue #978. The core stays general; optional workspace crates implement
//! [`pcbcraft_plugin_sdk::ExportPlugin`] and register through the engine feature
//! `plugin-firmware-target`.
pub use pcbcraft_plugin_sdk::{ExportPlugin, PluginExportKind, PluginManifest};
