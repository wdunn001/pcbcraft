//! Data-only plugin manifests, a local registry, and pin-map export templates.
//!
//! # Rules
//! - Plugins never mutate the project; exports read the same pin map view as other writers.
//! - Schema version 1 is data only (TOML manifests and templates). No code execution.
//! - Rule sets bundled in a plugin still import through the rules library path.
//!
//! # Dependencies
//! - crates/plugin/SCHEMA.md manifest and template fields.
//! - crates/engine/profiles and crates/library/src/sources.rs for prior art on data packs.
//!
//! # Do not
//! - Open network connections or run WebAssembly in this crate.
//! - Change core document types; plugins sit beside the engine.
pub mod export;
pub mod manifest;
pub mod pin_map;
pub mod pin_map_check;
pub mod pin_map_detail;
pub mod pin_map_export;
pub mod registry;

pub use export::{ExportTemplate, load_export_template, render_pin_map};
pub use manifest::{
    ExportTemplateEntry, ImporterEntry, PluginManifest, load_manifest, parse_manifest,
};
pub use pin_map::{PinMapRow, pin_map_rows};
pub use pin_map_check::{PinMapMismatch, check_pin_map_text};
pub use pin_map_detail::{PinDetail, PinDetailRow, pin_map_detail};
pub use pin_map_export::{render_pin_map_csv, render_pin_map_json};
pub use registry::{export_output_name, export_template_path, load_registry};
