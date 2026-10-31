//! Shared types for PCBCraft export plugins (data and code outside the core).
use pcbcraft_doc::Project;

/// Metadata shipped beside a plugin (see `plugins/*/plugin.toml`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginManifest {
    pub id: &'static str,
    pub name: &'static str,
    pub version: &'static str,
    pub license: &'static str,
    pub description: &'static str,
}

/// One export kind a plugin adds outside [`pcbcraft_doc::OUTPUT_KINDS`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginExportKind {
    pub id: &'static str,
    pub title: &'static str,
    pub summary: &'static str,
}

/// Read-only export from the open project.
pub trait ExportPlugin: Send + Sync {
    fn manifest(&self) -> &PluginManifest;
    fn export_kinds(&self) -> &[PluginExportKind];
    fn export_files(&self, kind: &str, project: &Project)
    -> Result<Vec<(String, Vec<u8>)>, String>;
}
