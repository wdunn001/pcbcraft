//! Scan a local plugins directory and list manifests.
use crate::manifest::{PluginManifest, load_manifest};
use std::path::{Path, PathBuf};

const MAX_PLUGINS: usize = 256;
const MAX_ROOT_BYTES: usize = 4096;

/// Load every plugin folder under `root` that contains `plugin.toml`.
///
/// # Rules
/// - Only immediate subdirectories are scanned; order is sorted by plugin id.
/// - Stops with an error when more than `MAX_PLUGINS` manifests are found.
///
/// # Dependencies
/// - crates/plugin/src/manifest.rs `load_manifest`.
///
/// # Do not
/// - Fetch from the network or treat the path as a marketplace index.
pub fn load_registry(root: &Path) -> Result<Vec<PluginManifest>, String> {
    let root = root
        .canonicalize()
        .map_err(|e| format!("invalid plugins root: {e}"))?;
    let root_str = root.to_string_lossy();
    if root_str.len() > MAX_ROOT_BYTES {
        return Err("plugins root path is too long".into());
    }
    let read = std::fs::read_dir(&root).map_err(|e| format!("could not read plugins root: {e}"))?;
    let mut manifests = Vec::new();
    for entry in read {
        let entry = entry.map_err(|e| format!("could not read directory entry: {e}"))?;
        let file_type = entry
            .file_type()
            .map_err(|e| format!("could not read file type: {e}"))?;
        if !file_type.is_dir() {
            continue;
        }
        let dir = entry.path();
        let manifest_path = dir.join("plugin.toml");
        if !manifest_path.is_file() {
            continue;
        }
        manifests.push(load_manifest(&dir)?);
        if manifests.len() > MAX_PLUGINS {
            return Err(format!("at most {MAX_PLUGINS} plugins allowed in one root"));
        }
    }
    manifests.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(manifests)
}

/// Resolve one export template file path inside a loaded manifest.
pub fn export_template_path(
    manifest: &PluginManifest,
    template_id: &str,
) -> Result<PathBuf, String> {
    let entry = manifest
        .export_templates
        .iter()
        .find(|t| t.id == template_id)
        .ok_or_else(|| {
            format!(
                "export template {template_id} not found in plugin {}",
                manifest.id
            )
        })?;
    Ok(manifest.root.join(&entry.path))
}

/// Default output file name for a template, falling back to the template id.
pub fn export_output_name(manifest: &PluginManifest, template_id: &str) -> Result<String, String> {
    let entry = manifest
        .export_templates
        .iter()
        .find(|t| t.id == template_id)
        .ok_or_else(|| format!("export template {template_id} not found"))?;
    if entry.output_name.is_empty() {
        Ok(format!("{template_id}.txt"))
    } else {
        Ok(entry.output_name.clone())
    }
}
