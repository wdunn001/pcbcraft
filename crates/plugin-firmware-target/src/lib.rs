//! Betaflight-style flight-controller target export (plugin, outside the core).
mod export;
mod firmware_design;
mod mcu;

pub use export::{EXPORT_KIND, firmware_config_compare, firmware_target_files};
pub use firmware_design::{
    DEFAULT_DESIGN_FILE, FirmwareContext, FirmwareDesign, load_firmware_context,
    parse_firmware_design,
};
pub use mcu::{McuRecord, parse_mcu_record};

use pcbcraft_doc::Project;
use pcbcraft_plugin_sdk::{ExportPlugin, PluginExportKind, PluginManifest};

/// Shipped plugin id (matches `plugins/firmware-target-betaflight/plugin.toml`).
pub const PLUGIN_ID: &str = "firmware-target-betaflight";

static MANIFEST: PluginManifest = PluginManifest {
    id: PLUGIN_ID,
    name: "Flight controller firmware target (Betaflight)",
    version: "0.1.0",
    license: "GPL-3.0-or-later",
    description: "Writes board config.h and a manifest from the schematic firmware design.",
};

static EXPORT_KINDS: [PluginExportKind; 1] = [PluginExportKind {
    id: export::EXPORT_KIND,
    title: "Flight controller target",
    summary: "Board config header and manifest from the firmware pin map.",
}];

/// Default export plugin instance for registration in the engine.
pub struct FirmwareTargetPlugin;

impl ExportPlugin for FirmwareTargetPlugin {
    fn manifest(&self) -> &PluginManifest {
        &MANIFEST
    }

    fn export_kinds(&self) -> &[PluginExportKind] {
        &EXPORT_KINDS
    }

    fn export_files(&self, kind: &str, project: &Project) -> Result<Vec<(String, Vec<u8>)>, String> {
        if kind != export::EXPORT_KIND {
            return Err(format!("unknown export kind {kind}"));
        }
        export::firmware_target_files(project)
    }
}
