//! Static registry layout: publication folders, a generated catalogue, a browse
//! page, and a pull-based index of other instances. No network calls here.
//!
//! # Rules
//! - Every file read from disk is hash-verified against its manifest.
//! - Paths stay inside the registry root; traversal and `..` are refused.
//! - Large binaries may be omitted from git and listed with an https URL in the manifest.
//!
//! # Dependencies
//! - crates/library/src/lib.rs
//!
//! # Do not
//! - Store keys, tokens or deployment addresses in committed fixtures.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

pub const PUBLICATION_MANIFEST: &str = "publication.json";
pub const REGISTRY_MANIFEST: &str = "registry.json";
pub const CATALOGUE_FILE: &str = "catalogue.json";
pub const INDEX_HTML: &str = "index.html";
pub const PUBLICATIONS_DIR: &str = "publications";
pub const INSTANCES_FILE: &str = "instances.json";
pub const SOURCES_INDEX: &str = "sources.json";

pub const MAX_PUBLICATION_ID: usize = 128;
pub const MAX_VERSION: usize = 64;
pub const MAX_TITLE: usize = 256;
pub const MAX_TAGS: usize = 32;
pub const MAX_TAG_LEN: usize = 64;
pub const MAX_FILES: usize = 256;
pub const MAX_FILE_BYTES: u64 = 32 * 1024 * 1024;
pub const MAX_CATALOGUE_ITEMS: usize = 4096;
pub const MAX_SOURCES: usize = 64;

/// SHA-256 of `bytes` as 64 lowercase hex digits.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut hex = String::with_capacity(64);
    for byte in digest {
        hex.push(nibble(byte >> 4));
        hex.push(nibble(byte & 0x0f));
    }
    hex
}

fn nibble(value: u8) -> char {
    match value {
        0..=9 => (b'0' + value) as char,
        10..=15 => (b'a' + (value - 10)) as char,
        _ => '0',
    }
}

/// Content address of a publication: hash of sorted `(path, sha256)` pairs.
pub fn bundle_root_hash(files: &[PublicationFile]) -> String {
    let mut hasher = Sha256::new();
    for file in files {
        hasher.update(file.path.as_bytes());
        hasher.update(file.sha256.as_bytes());
    }
    sha256_hex(&hasher.finalize())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationFile {
    pub path: String,
    pub sha256: String,
    pub size: u64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub role: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationManifest {
    pub format: String,
    pub schema_version: u32,
    pub id: String,
    pub title: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    pub files: Vec<PublicationFile>,
    pub bundle_root_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistrySignature {
    pub algorithm: String,
    pub key_id: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryManifest {
    pub format: String,
    pub schema_version: u32,
    pub name: String,
    pub catalogue_sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<RegistrySignature>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogueItem {
    pub id: String,
    pub title: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    pub bundle_root_hash: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<CatalogueOrigin>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogueOrigin {
    pub instance_id: String,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogueDocument {
    pub format: String,
    pub schema_version: u32,
    pub items: Vec<CatalogueItem>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum SourceEntry {
    StaticUrl {
        id: String,
        name: String,
        url: String,
    },
    Git {
        id: String,
        name: String,
        repo: String,
    },
    Pack {
        id: String,
        name: String,
        path: String,
    },
    Instance {
        id: String,
        name: String,
        feed: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourcesDocument {
    pub schema_version: u32,
    pub sources: Vec<SourceEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstanceFeed {
    pub format: String,
    pub schema_version: u32,
    pub instance_id: String,
    pub title: String,
    pub catalogue_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,
}

fn check_piece(piece: &str, label: &str) -> Result<(), String> {
    if piece.is_empty()
        || piece.len() > 256
        || piece == "."
        || piece.contains("..")
        || piece.chars().any(|c| c.is_control() || matches!(c, '/' | '\\'))
    {
        return Err(format!("{label} has an unusable path piece"));
    }
    Ok(())
}

/// Refuse relative paths that climb out of a publication folder.
pub fn safe_rel_path(path: &str) -> Result<&str, String> {
    let trimmed = path.trim();
    if trimmed.is_empty() || trimmed.len() > 255 {
        return Err("file paths are 1 to 255 bytes".into());
    }
    let as_path = Path::new(trimmed);
    if as_path.is_absolute() {
        return Err("file paths must be relative".into());
    }
    for component in as_path.components() {
        match component {
            Component::Normal(name) => {
                let piece = name.to_string_lossy();
                check_piece(&piece, "file path")?;
            }
            Component::CurDir => {}
            _ => return Err("file paths must not contain parent segments".into()),
        }
    }
    Ok(trimmed)
}

fn join_under(base: &Path, rel: &str) -> Result<PathBuf, String> {
    let rel = safe_rel_path(rel)?;
    let joined = base.join(rel);
    let canonical_base = base.canonicalize().unwrap_or_else(|_| base.to_path_buf());
    let canonical = joined.canonicalize().unwrap_or(joined.clone());
    if !canonical.starts_with(&canonical_base) {
        return Err("path escapes the publication folder".into());
    }
    Ok(joined)
}

fn read_bounded(path: &Path) -> Result<Vec<u8>, String> {
    let meta = fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if !meta.is_file() {
        return Err(format!("{} is not a regular file", path.display()));
    }
    if meta.len() > MAX_FILE_BYTES {
        return Err(format!("{} exceeds the size budget", path.display()));
    }
    fs::read(path).map_err(|e| format!("{}: {e}", path.display()))
}

fn parse_manifest(text: &str) -> Result<PublicationManifest, String> {
    let manifest: PublicationManifest =
        serde_json::from_str(text).map_err(|e| format!("publication manifest: {e}"))?;
    if manifest.format != "pcbcraft-publication" {
        return Err("publication manifest has the wrong format name".into());
    }
    if manifest.schema_version != 1 {
        return Err("publication manifest schema version is not supported".into());
    }
    if manifest.id.is_empty() || manifest.id.len() > MAX_PUBLICATION_ID {
        return Err("publication id is out of range".into());
    }
    if manifest.title.is_empty() || manifest.title.len() > MAX_TITLE {
        return Err("publication title is out of range".into());
    }
    if manifest.version.is_empty() || manifest.version.len() > MAX_VERSION {
        return Err("publication version is out of range".into());
    }
    if manifest.files.is_empty() || manifest.files.len() > MAX_FILES {
        return Err("publication file list is out of range".into());
    }
    if manifest.tags.len() > MAX_TAGS {
        return Err("too many tags".into());
    }
    for tag in &manifest.tags {
        if tag.is_empty() || tag.len() > MAX_TAG_LEN {
            return Err("a tag is out of range".into());
        }
    }
    let mut seen = BTreeMap::new();
    for file in &manifest.files {
        let rel = safe_rel_path(&file.path)?;
        if file.sha256.len() != 64 || !file.sha256.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(format!("{} has an invalid sha256", file.path));
        }
        if seen.insert(rel, ()).is_some() {
            return Err(format!("duplicate file path {}", file.path));
        }
        if let Some(url) = &file.url {
            if !url.starts_with("https://") || url.len() > 2048 {
                return Err(format!("{} has an invalid file url", file.path));
            }
        }
    }
    let expected = bundle_root_hash(&manifest.files);
    if manifest.bundle_root_hash != expected {
        return Err("bundle_root_hash does not match the file list".into());
    }
    Ok(manifest)
}

/// Read and verify one publication folder (manifest plus on-disk files).
pub fn verify_publication_dir(dir: &Path) -> Result<PublicationManifest, String> {
    let manifest_path = dir.join(PUBLICATION_MANIFEST);
    let text = fs::read_to_string(&manifest_path)
        .map_err(|e| format!("{}: {e}", manifest_path.display()))?;
    let manifest = parse_manifest(&text)?;
    for file in &manifest.files {
        if file.url.is_some() {
            continue;
        }
        let path = join_under(dir, &file.path)?;
        let bytes = read_bounded(&path)?;
        if u64::try_from(bytes.len()).map_err(|_| "file size overflow")? != file.size {
            return Err(format!("{} size does not match the manifest", file.path));
        }
        let hash = sha256_hex(&bytes);
        if hash != file.sha256.to_ascii_lowercase() {
            return Err(format!("{} hash does not match the manifest", file.path));
        }
    }
    Ok(manifest)
}

/// Scan every publication under `registry_root/publications` and build catalogue items.
pub fn scan_publications(registry_root: &Path) -> Result<Vec<CatalogueItem>, String> {
    let publications = registry_root.join(PUBLICATIONS_DIR);
    if !publications.is_dir() {
        return Err("registry has no publications folder".into());
    }
    let mut items = Vec::new();
    for pub_entry in fs::read_dir(&publications).map_err(|e| e.to_string())? {
        let pub_entry = pub_entry.map_err(|e| e.to_string())?;
        if !pub_entry.file_type().map_err(|e| e.to_string())?.is_dir() {
            continue;
        }
        let pub_id = pub_entry.file_name().to_string_lossy().into_owned();
        check_piece(&pub_id, "publication id")?;
        for ver_entry in fs::read_dir(pub_entry.path()).map_err(|e| e.to_string())? {
            let ver_entry = ver_entry.map_err(|e| e.to_string())?;
            if !ver_entry.file_type().map_err(|e| e.to_string())?.is_dir() {
                continue;
            }
            let version = ver_entry.file_name().to_string_lossy().into_owned();
            check_piece(&version, "publication version")?;
            let manifest = verify_publication_dir(&ver_entry.path())?;
            if manifest.id != pub_id {
                return Err(format!(
                    "publication folder {pub_id} holds manifest id {}",
                    manifest.id
                ));
            }
            if manifest.version != version {
                return Err(format!(
                    "folder {pub_id}/{version} holds version {}",
                    manifest.version
                ));
            }
            let rel_path = format!("{PUBLICATIONS_DIR}/{pub_id}/{version}");
            items.push(CatalogueItem {
                id: manifest.id,
                title: manifest.title,
                version: manifest.version,
                kind: manifest.kind,
                tags: manifest.tags,
                bundle_root_hash: manifest.bundle_root_hash,
                path: rel_path,
                origin: None,
            });
        }
    }
    items.sort_by(|a, b| a.id.cmp(&b.id).then_with(|| a.version.cmp(&b.version)));
    if items.len() > MAX_CATALOGUE_ITEMS {
        return Err("catalogue exceeds the item cap".into());
    }
    Ok(items)
}

/// Build the catalogue document from on-disk publications.
pub fn build_catalogue(registry_root: &Path) -> Result<CatalogueDocument, String> {
    Ok(CatalogueDocument {
        format: "pcbcraft-registry-catalogue".into(),
        schema_version: 1,
        items: scan_publications(registry_root)?,
    })
}

fn catalogue_json(catalogue: &CatalogueDocument) -> Result<String, String> {
    serde_json::to_string_pretty(catalogue).map_err(|e| e.to_string())
}

/// Minimal static browse page for a catalogue.
pub fn generate_index_html(catalogue: &CatalogueDocument) -> String {
    let mut rows = String::new();
    for item in &catalogue.items {
        let origin = item
            .origin
            .as_ref()
            .map(|o| format!(" <span class=\"origin\">from {}</span>", o.label))
            .unwrap_or_default();
        rows.push_str(&format!(
            "<tr><td>{id}</td><td>{title}{origin}</td><td>{version}</td><td>{kind}</td></tr>\n",
            id = html_escape(&item.id),
            title = html_escape(&item.title),
            origin = origin,
            version = html_escape(&item.version),
            kind = html_escape(&item.kind),
        ));
    }
    format!(
        "<!DOCTYPE html><html lang=\"en\"><head><meta charset=\"utf-8\"><title>Registry</title>\
<style>body{{font-family:system-ui,sans-serif;margin:2rem}}table{{border-collapse:collapse}}\
td,th{{border:1px solid #ccc;padding:0.4rem 0.6rem}}.origin{{color:#666;font-size:0.9em}}</style>\
</head><body><h1>Publications</h1><table><thead><tr><th>Id</th><th>Title</th><th>Version</th><th>Kind</th></tr></thead>\
<tbody>{rows}</tbody></table></body></html>"
    )
}

fn html_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(ch),
        }
    }
    out
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("tmp");
    {
        let mut file = fs::File::create(&tmp).map_err(|e| e.to_string())?;
        file.write_all(bytes).map_err(|e| e.to_string())?;
    }
    fs::rename(&tmp, path).map_err(|e| e.to_string())
}

/// Write `catalogue.json`, `index.html`, and refresh `registry.json` hashes.
pub fn rebuild_site(registry_root: &Path) -> Result<CatalogueDocument, String> {
    let catalogue = build_catalogue(registry_root)?;
    let catalogue_text = catalogue_json(&catalogue)?;
    atomic_write(
        &registry_root.join(CATALOGUE_FILE),
        catalogue_text.as_bytes(),
    )?;
    let html = generate_index_html(&catalogue);
    atomic_write(registry_root.join(INDEX_HTML).as_path(), html.as_bytes())?;
    let catalogue_hash = sha256_hex(catalogue_text.as_bytes());
    let manifest_path = registry_root.join(REGISTRY_MANIFEST);
    let registry: RegistryManifest = if manifest_path.is_file() {
        let text = fs::read_to_string(&manifest_path).map_err(|e| e.to_string())?;
        serde_json::from_str(&text).map_err(|e| format!("registry manifest: {e}"))?
    } else {
        RegistryManifest {
            format: "pcbcraft-static-registry".into(),
            schema_version: 1,
            name: "registry".into(),
            catalogue_sha256: String::new(),
            signature: None,
        }
    };
    if registry.format != "pcbcraft-static-registry" || registry.schema_version != 1 {
        return Err("registry manifest has an unsupported format".into());
    }
    let updated = RegistryManifest {
        catalogue_sha256: catalogue_hash,
        ..registry
    };
    let registry_text =
        serde_json::to_string_pretty(&updated).map_err(|e| e.to_string())?;
    atomic_write(manifest_path.as_path(), registry_text.as_bytes())?;
    Ok(catalogue)
}

/// Check that `registry.json` matches the on-disk catalogue when present.
pub fn verify_registry_manifest(registry_root: &Path) -> Result<(), String> {
    let manifest_path = registry_root.join(REGISTRY_MANIFEST);
    let catalogue_path = registry_root.join(CATALOGUE_FILE);
    if !manifest_path.is_file() || !catalogue_path.is_file() {
        return Ok(());
    }
    let manifest_text = fs::read_to_string(&manifest_path).map_err(|e| e.to_string())?;
    let manifest: RegistryManifest =
        serde_json::from_str(&manifest_text).map_err(|e| format!("registry manifest: {e}"))?;
    let catalogue_bytes = fs::read(&catalogue_path).map_err(|e| e.to_string())?;
    let hash = sha256_hex(&catalogue_bytes);
    if manifest.catalogue_sha256 != hash {
        return Err("registry manifest catalogue hash does not match catalogue.json".into());
    }
    if let Some(sig) = &manifest.signature {
        if sig.algorithm.is_empty() || sig.key_id.is_empty() || sig.value.is_empty() {
            return Err("registry signature is incomplete".into());
        }
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublishInput {
    pub id: String,
    pub title: String,
    pub version: String,
    pub kind: String,
    pub tags: Vec<String>,
    pub files: BTreeMap<String, Vec<u8>>,
}

/// Write one publication version under `registry_root/publications`.
pub fn publish_to_folder(registry_root: &Path, input: PublishInput) -> Result<PathBuf, String> {
    if input.files.is_empty() || input.files.len() > MAX_FILES {
        return Err("publication needs 1 to 256 files".into());
    }
    let mut records = Vec::new();
    for (path, bytes) in &input.files {
        safe_rel_path(path)?;
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err(format!("{path} exceeds the size budget"));
        }
        records.push(PublicationFile {
            path: path.clone(),
            sha256: sha256_hex(bytes),
            size: u64::try_from(bytes.len()).map_err(|_| "file size overflow")?,
            role: String::new(),
            url: None,
        });
    }
    records.sort_by(|a, b| a.path.cmp(&b.path));
    let manifest = PublicationManifest {
        format: "pcbcraft-publication".into(),
        schema_version: 1,
        id: input.id.clone(),
        title: input.title,
        version: input.version.clone(),
        kind: input.kind,
        tags: input.tags,
        files: records.clone(),
        bundle_root_hash: bundle_root_hash(&records),
        actor: None,
    };
    let dest = registry_root
        .join(PUBLICATIONS_DIR)
        .join(&input.id)
        .join(&input.version);
    fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
    for (path, bytes) in &input.files {
        let file_path = join_under(&dest, path)?;
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        atomic_write(&file_path, bytes)?;
    }
    let manifest_text = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
    atomic_write(&dest.join(PUBLICATION_MANIFEST), manifest_text.as_bytes())?;
    verify_publication_dir(&dest)?;
    Ok(dest)
}

/// Copy verified publication files into `dest_dir`.
pub fn install_publication(
    registry_root: &Path,
    publication_id: &str,
    version: &str,
    dest_dir: &Path,
) -> Result<Vec<String>, String> {
    check_piece(publication_id, "publication id")?;
    check_piece(version, "publication version")?;
    let dir = registry_root
        .join(PUBLICATIONS_DIR)
        .join(publication_id)
        .join(version);
    let manifest = verify_publication_dir(&dir)?;
    fs::create_dir_all(dest_dir).map_err(|e| e.to_string())?;
    let mut written = Vec::new();
    for file in &manifest.files {
        if file.url.is_some() {
            continue;
        }
        let src = join_under(&dir, &file.path)?;
        let bytes = read_bounded(&src)?;
        if sha256_hex(&bytes) != file.sha256.to_ascii_lowercase() {
            return Err(format!("{} failed hash verification on install", file.path));
        }
        let out = dest_dir.join(&file.path);
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        atomic_write(&out, &bytes)?;
        written.push(file.path.clone());
    }
    Ok(written)
}

/// Parse the public sources index.
pub fn read_sources_index(path: &Path) -> Result<SourcesDocument, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let doc: SourcesDocument =
        serde_json::from_str(&text).map_err(|e| format!("sources index: {e}"))?;
    if doc.schema_version != 1 {
        return Err("sources index schema version is not supported".into());
    }
    if doc.sources.is_empty() || doc.sources.len() > MAX_SOURCES {
        return Err("sources list is out of range".into());
    }
    Ok(doc)
}

/// Read a local instance feed (`instances.json`).
pub fn read_instance_feed(path: &Path) -> Result<InstanceFeed, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let feed: InstanceFeed =
        serde_json::from_str(&text).map_err(|e| format!("instance feed: {e}"))?;
    if feed.format != "pcbcraft-instance-feed" || feed.schema_version != 1 {
        return Err("instance feed has an unsupported format".into());
    }
    check_piece(&feed.instance_id, "instance id")?;
    safe_rel_path(&feed.catalogue_path)?;
    Ok(feed)
}

/// Load a remote catalogue file from disk and tag every item with its origin.
pub fn catalogue_with_origin(
    catalogue_path: &Path,
    instance_id: &str,
    label: &str,
) -> Result<Vec<CatalogueItem>, String> {
    let text = fs::read_to_string(catalogue_path)
        .map_err(|e| format!("{}: {e}", catalogue_path.display()))?;
    let doc: CatalogueDocument =
        serde_json::from_str(&text).map_err(|e| format!("catalogue: {e}"))?;
    let origin = CatalogueOrigin {
        instance_id: instance_id.to_owned(),
        label: label.to_owned(),
    };
    Ok(doc
        .items
        .into_iter()
        .map(|mut item| {
            item.origin = Some(origin.clone());
            item
        })
        .collect())
}

/// Merge local catalogue items with remote items; ids from remote instances stay distinct by origin.
pub fn merge_catalogues(
    local: Vec<CatalogueItem>,
    remote: Vec<CatalogueItem>,
) -> Vec<CatalogueItem> {
    let mut out = local;
    out.extend(remote);
    out.sort_by(|a, b| {
        a.id.cmp(&b.id)
            .then_with(|| a.version.cmp(&b.version))
            .then_with(|| a.origin.as_ref().map(|o| &o.instance_id).cmp(&b.origin.as_ref().map(|o| &o.instance_id)))
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    struct TempDir(PathBuf);
    impl TempDir {
        fn new() -> Result<Self, String> {
            let path = std::env::temp_dir().join(format!(
                "pcbcraft-registry-test-{}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).map_err(|e| e.to_string())?;
            Ok(Self(path))
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn publish_install_round_trip_with_hash_verification() -> Result<(), String> {
        let root = TempDir::new()?;
        let registry = root.0.join("registry");
        fs::create_dir_all(&registry).map_err(|e| e.to_string())?;
        let sym = b"(kicad_symbol_lib (version 20250114) (symbol R))";
        publish_to_folder(
            &registry,
            PublishInput {
                id: "demo-r".into(),
                title: "Demo R".into(),
                version: "1.0.0".into(),
                kind: "part".into(),
                tags: vec!["passive".into()],
                files: BTreeMap::from([("R.kicad_sym".into(), sym.to_vec())]),
            },
        )?;
        rebuild_site(&registry)?;
        verify_registry_manifest(&registry)?;
        let install_dir = root.0.join("installed");
        let files = install_publication(&registry, "demo-r", "1.0.0", &install_dir)?;
        assert_eq!(files, vec!["R.kicad_sym".to_string()]);
        Ok(())
    }

    #[test]
    fn tampered_file_is_refused() -> Result<(), String> {
        let root = TempDir::new()?;
        let registry = root.0.join("registry");
        publish_to_folder(
            &registry,
            PublishInput {
                id: "demo-r".into(),
                title: "Demo R".into(),
                version: "1.0.0".into(),
                kind: "part".into(),
                tags: vec![],
                files: BTreeMap::from([("R.kicad_sym".into(), b"good".to_vec())]),
            },
        )?;
        let file = registry
            .join(PUBLICATIONS_DIR)
            .join("demo-r")
            .join("1.0.0")
            .join("R.kicad_sym");
        let mut handle = fs::OpenOptions::new()
            .append(true)
            .open(&file)
            .map_err(|e| e.to_string())?;
        handle.write_all(b"x").map_err(|e| e.to_string())?;
        assert!(verify_publication_dir(
            &registry
                .join(PUBLICATIONS_DIR)
                .join("demo-r")
                .join("1.0.0")
        )
        .is_err());
        Ok(())
    }
}
