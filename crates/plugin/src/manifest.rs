//! Plugin manifest types and validation.
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

const MAX_ID: usize = 64;
const MAX_NAME: usize = 128;
const MAX_VERSION: usize = 32;
const MAX_LICENSE: usize = 128;
const MAX_PATH: usize = 256;
const MAX_DESC: usize = 256;
const MAX_OUTPUT: usize = 128;
const MAX_MANIFEST_BYTES: usize = 65_536;
const MAX_TEMPLATES: usize = 32;
const MAX_BUNDLES: usize = 32;
const MAX_IMPORTERS: usize = 32;

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportTemplateEntry {
    pub id: String,
    pub path: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub output_name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImporterEntry {
    pub id: String,
    pub path: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestDoc {
    schema: u32,
    id: String,
    name: String,
    version: String,
    license: String,
    #[serde(default)]
    content_hash: String,
    #[serde(default)]
    export_templates: Vec<ExportTemplateEntry>,
    #[serde(default)]
    rule_set_bundles: Vec<String>,
    #[serde(default)]
    importers: Vec<ImporterEntry>,
}

/// One installed plugin after manifest validation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub license: String,
    pub root: PathBuf,
    pub export_templates: Vec<ExportTemplateEntry>,
    pub rule_set_bundles: Vec<String>,
    pub importers: Vec<ImporterEntry>,
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_ID
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn bounded(label: &str, value: &str, max: usize) -> Result<(), String> {
    if value.is_empty() || value.len() > max {
        return Err(format!("{label} must be 1 to {max} bytes"));
    }
    Ok(())
}

fn check_relative_path(label: &str, path: &str) -> Result<(), String> {
    bounded(label, path, MAX_PATH)?;
    let p = Path::new(path);
    if p.is_absolute() || path.contains('\\') {
        return Err(format!("{label} must be a relative path with forward slashes"));
    }
    if path.split('/').any(|part| part == ".." || part.is_empty()) {
        return Err(format!("{label} must not contain parent segments"));
    }
    Ok(())
}

fn verify_content_hash(expected: &str, bytes: &[u8]) -> Result<(), String> {
    if expected.is_empty() {
        return Ok(());
    }
    let prefix = "sha256:";
    if !expected.starts_with(prefix) {
        return Err("content_hash must start with sha256:".into());
    }
    let hex = &expected[prefix.len()..];
    if hex.len() != 64 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("content_hash must be 64 hex digits after sha256:".into());
    }
    let digest = Sha256::digest(bytes);
    let got = digest
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    if got != hex {
        return Err("content_hash does not match the manifest file".into());
    }
    Ok(())
}

/// Parse and validate `plugin.toml` bytes relative to `root`.
///
/// # Rules
/// - Schema must be 1; ids must be unique within export templates.
/// - Every referenced path must exist under `root`.
///
/// # Dependencies
/// - crates/plugin/SCHEMA.md
///
/// # Do not
/// - Load template bodies here; only manifest metadata.
pub fn parse_manifest(root: &Path, bytes: &[u8]) -> Result<PluginManifest, String> {
    if bytes.len() > MAX_MANIFEST_BYTES {
        return Err("plugin manifest is too large".into());
    }
    let text = std::str::from_utf8(bytes).map_err(|_| "plugin manifest must be utf-8")?;
    let doc: ManifestDoc =
        toml::from_str(text).map_err(|e| format!("invalid plugin manifest: {e}"))?;
    if doc.schema != 1 {
        return Err(format!("unsupported plugin schema {}", doc.schema));
    }
    if !valid_id(&doc.id) {
        return Err("plugin id must be lowercase letters, digits or hyphens".into());
    }
    bounded("plugin name", &doc.name, MAX_NAME)?;
    bounded("plugin version", &doc.version, MAX_VERSION)?;
    bounded("plugin license", &doc.license, MAX_LICENSE)?;
    verify_content_hash(&doc.content_hash, bytes)?;
    if doc.export_templates.len() > MAX_TEMPLATES {
        return Err(format!("at most {MAX_TEMPLATES} export templates allowed"));
    }
    if doc.rule_set_bundles.len() > MAX_BUNDLES {
        return Err(format!("at most {MAX_BUNDLES} rule set bundles allowed"));
    }
    if doc.importers.len() > MAX_IMPORTERS {
        return Err(format!("at most {MAX_IMPORTERS} importers allowed"));
    }
    let mut seen = std::collections::BTreeSet::new();
    for entry in &doc.export_templates {
        if !valid_id(&entry.id) {
            return Err("export template id is invalid".into());
        }
        if !seen.insert(entry.id.clone()) {
            return Err(format!("duplicate export template id {}", entry.id));
        }
        check_relative_path("export template path", &entry.path)?;
        if entry.description.len() > MAX_DESC {
            return Err("export template description is too long".into());
        }
        if !entry.output_name.is_empty() && entry.output_name.len() > MAX_OUTPUT {
            return Err("export template output_name is too long".into());
        }
        let full = root.join(&entry.path);
        if !full.is_file() {
            return Err(format!("export template file missing: {}", entry.path));
        }
    }
    for path in &doc.rule_set_bundles {
        check_relative_path("rule set bundle path", path)?;
        if !root.join(path).is_file() {
            return Err(format!("rule set bundle file missing: {path}"));
        }
    }
    for entry in &doc.importers {
        if !valid_id(&entry.id) {
            return Err("importer id is invalid".into());
        }
        check_relative_path("importer path", &entry.path)?;
        if !root.join(&entry.path).is_file() {
            return Err(format!("importer file missing: {}", entry.path));
        }
    }
    Ok(PluginManifest {
        id: doc.id,
        name: doc.name,
        version: doc.version,
        license: doc.license,
        root: root.to_path_buf(),
        export_templates: doc.export_templates,
        rule_set_bundles: doc.rule_set_bundles,
        importers: doc.importers,
    })
}

/// Read `plugin.toml` from a plugin folder.
pub fn load_manifest(root: &Path) -> Result<PluginManifest, String> {
    let path = root.join("plugin.toml");
    let bytes = std::fs::read(&path)
        .map_err(|e| format!("could not read {}: {e}", path.display()))?;
    parse_manifest(root, &bytes)
}
