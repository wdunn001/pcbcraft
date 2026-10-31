//! Disk cache for datasheet PDFs fetched from the web or copied from a local
//! file. The library crate never opens a socket; a transport implements
//! [`DatasheetTransport`] elsewhere.
//!
//! # Rules
//! - Only `http://` and `https://` URLs are accepted for fetch.
//! - A stored body must begin with `%PDF-` and fit [`MAX_PDF_BYTES`].
//! - Project files keep only the URL, content hash and revision (ETag); bytes
//!   stay in the per-user cache.
//! - Cache layout lives under `pcbcraft/datasheets` beside the online library
//!   cache described in crates/library/src/cad_cache.rs.
//!
//! # Dependencies
//! - crates/library/src/cad_cache.rs (same cache root convention)
//! - crates/library/src/lib.rs
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Largest PDF read from the network or from disk for import.
pub const MAX_PDF_BYTES: usize = 25 * 1024 * 1024;
/// Default cap on the whole cache: 500 MiB.
pub const DEFAULT_CACHE_CAP_BYTES: u64 = 500 * 1024 * 1024;
/// Longest URL stored or fetched.
pub const MAX_URL_BYTES: usize = 2048;
/// Most redirects a transport may follow for one fetch.
pub const MAX_REDIRECTS: usize = 5;

const MEMORY_CAP_BYTES: usize = 16 * 1024 * 1024;

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// What a project file stores about a cached datasheet.
///
/// # Rules
/// - `content_sha256` is 64 lowercase hex digits of the PDF bytes.
/// - `revision` holds the last ETag from the host, or is empty when there was none.
///
/// # Do not
/// - Embed PDF bytes in the project.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectDatasheetRef {
    pub url: String,
    pub content_sha256: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub revision: String,
}

/// Metadata kept beside each cached PDF.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredMeta {
    source_url: String,
    content_sha256: String,
    fetched_at: u64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    etag: String,
}

#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum DatasheetError {
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    Offline(String),
    #[error("no cached copy for that address")]
    NotCached,
    #[error("the host has no file at that address")]
    NotFound,
    #[error("{0}")]
    Unavailable(String),
}

/// Answer from a conditional GET.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FetchAnswer {
    /// A new body arrived.
    Fresh {
        bytes: Vec<u8>,
        etag: Option<String>,
    },
    /// The host said the cached copy is still current.
    NotModified,
}

/// Reads PDF bytes over the network. Implementations live outside this crate.
///
/// # Rules
/// - When `offline` is true, the transport must not open a socket.
/// - `etag`, when set, is sent as `If-None-Match`.
///
/// # Do not
/// - Store bytes; return them to the caller only.
pub trait DatasheetTransport: Send + Sync {
    fn fetch(
        &self,
        url: &str,
        etag: Option<&str>,
        offline: bool,
    ) -> Result<FetchAnswer, DatasheetError>;
}

fn bad(text: &str) -> DatasheetError {
    DatasheetError::Invalid(text.to_owned())
}

fn io_err(e: std::io::Error) -> DatasheetError {
    DatasheetError::Unavailable(format!("cache access failed: {e}"))
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Lowercase hex SHA-256 of `bytes`.
pub fn content_sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

fn check_sha256(hex: &str) -> Result<(), DatasheetError> {
    if hex.len() != 64 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(bad("the content hash must be 64 hex digits"));
    }
    Ok(())
}

/// True when `bytes` look like a PDF header.
pub fn is_pdf(bytes: &[u8]) -> bool {
    bytes.starts_with(b"%PDF-")
}

/// Accept a datasheet URL for fetch: plain http or https, bounded length, no spaces.
pub fn accept_datasheet_url(raw: &str) -> Result<String, DatasheetError> {
    let url = raw.trim();
    if url.is_empty() || url.len() > MAX_URL_BYTES {
        return Err(bad("the address is empty or too long"));
    }
    if url.chars().any(char::is_whitespace) {
        return Err(bad("the address must not contain spaces"));
    }
    let lower = url.to_ascii_lowercase();
    if !lower.starts_with("http://") && !lower.starts_with("https://") {
        return Err(bad("only web addresses are used for datasheets"));
    }
    if url.contains('@') {
        return Err(bad("the address must not carry a username or password"));
    }
    Ok(url.to_owned())
}

fn url_index_key(url: &str) -> String {
    content_sha256(url.as_bytes())
}

fn file_dir(root: &Path, hash: &str) -> PathBuf {
    let prefix = &hash[..2.min(hash.len())];
    root.join("files").join(prefix)
}

fn meta_path(root: &Path, hash: &str) -> PathBuf {
    file_dir(root, hash).join(format!("{hash}.meta"))
}

fn pdf_path(root: &Path, hash: &str) -> PathBuf {
    file_dir(root, hash).join(hash)
}

fn link_path(root: &Path, url: &str) -> PathBuf {
    root.join("urls").join(url_index_key(url))
}

#[derive(Default)]
struct MemoryEntry {
    bytes: Vec<u8>,
    meta: StoredMeta,
}

#[derive(Default)]
struct Memory {
    by_hash: std::collections::BTreeMap<String, MemoryEntry>,
    url_to_hash: std::collections::BTreeMap<String, String>,
    bytes: usize,
}

/// Per-user PDF cache with LRU eviction.
///
/// # Rules
/// - PDFs are keyed by [`content_sha256`]; the URL index points at the newest hash for an address.
/// - Reading a file updates its modification time for eviction.
///
/// # Do not
/// - Upload or bundle PDFs into project files.
pub struct DatasheetCache {
    root: Option<PathBuf>,
    cap: u64,
    memory: Mutex<Memory>,
}

impl DatasheetCache {
    /// Open a disk cache at `root`, creating folders on first write.
    pub fn open(root: &Path) -> Self {
        Self {
            root: Some(root.to_path_buf()),
            cap: DEFAULT_CACHE_CAP_BYTES,
            memory: Mutex::new(Memory::default()),
        }
    }

    /// Memory-only cache for tests.
    pub fn in_memory() -> Self {
        Self {
            root: None,
            cap: DEFAULT_CACHE_CAP_BYTES,
            memory: Mutex::new(Memory::default()),
        }
    }

    /// Same cache with a different total byte cap.
    pub fn with_cap(mut self, cap_bytes: u64) -> Self {
        self.cap = cap_bytes;
        self
    }

    /// Platform folder for datasheet files, sibling to the online library cache.
    pub fn default_cache_dir_with(get: &dyn Fn(&str) -> Option<String>, os: &str) -> Option<PathBuf> {
        crate::cad_cache::default_cache_dir_with(get, os).map(|p| {
            p.parent()
                .map(|parent| parent.join("datasheets"))
                .unwrap_or_else(|| p.join("../datasheets"))
        })
    }

    /// [`default_cache_dir_with`] using the process environment.
    pub fn default_cache_dir() -> Option<PathBuf> {
        Self::default_cache_dir_with(&|n| std::env::var(n).ok(), std::env::consts::OS)
    }

    fn store(&self, url: &str, bytes: &[u8], etag: &str, fetched_at: u64) -> Result<ProjectDatasheetRef, DatasheetError> {
        if bytes.len() > MAX_PDF_BYTES {
            return Err(bad("the file is too large to keep"));
        }
        if !is_pdf(bytes) {
            return Err(bad("the file is not a PDF"));
        }
        let hash = content_sha256(bytes);
        check_sha256(&hash)?;
        let meta = StoredMeta {
            source_url: url.to_owned(),
            content_sha256: hash.clone(),
            fetched_at,
            etag: etag.to_owned(),
        };
        if let Some(root) = &self.root {
            let dir = file_dir(root, &hash);
            fs::create_dir_all(&dir).map_err(io_err)?;
            write_atomic(&pdf_path(root, &hash), bytes).map_err(io_err)?;
            let meta_json = serde_json::to_vec(&meta).map_err(|e| bad(&format!("metadata encode failed: {e}")))?;
            write_atomic(&meta_path(root, &hash), &meta_json).map_err(io_err)?;
            let link_parent = root.join("urls");
            fs::create_dir_all(&link_parent).map_err(io_err)?;
            if !url.is_empty() {
                write_atomic(&link_path(root, url), hash.as_bytes()).map_err(io_err)?;
            }
            touch(&pdf_path(root, &hash));
            self.evict_disk(root);
        } else {
            let mut m = self.memory.lock().map_err(|_| bad("cache is busy"))?;
            if let Some(old) = m.by_hash.insert(hash.clone(), MemoryEntry {
                bytes: bytes.to_vec(),
                meta: meta.clone(),
            }) {
                m.bytes = m.bytes.saturating_sub(old.bytes.len());
            }
            m.bytes += bytes.len();
            if !url.is_empty() {
                m.url_to_hash.insert(url.to_owned(), hash.clone());
            }
            while m.bytes > MEMORY_CAP_BYTES {
                let Some(first_hash) = m.by_hash.keys().next().cloned() else {
                    break;
                };
                if let Some(old) = m.by_hash.remove(&first_hash) {
                    m.bytes = m.bytes.saturating_sub(old.bytes.len());
                    m.url_to_hash.retain(|_, h| h != &first_hash);
                }
            }
        }
        Ok(ProjectDatasheetRef {
            url: url.to_owned(),
            content_sha256: hash,
            revision: etag.to_owned(),
        })
    }

    /// Fetch when online, or read the cache when offline. Returns the project ref and path to bytes.
    pub fn fetch_url(
        &self,
        transport: &dyn DatasheetTransport,
        raw_url: &str,
        offline: bool,
    ) -> Result<(ProjectDatasheetRef, Vec<u8>), DatasheetError> {
        let url = accept_datasheet_url(raw_url)?;
        let prior_etag = self.meta_for_url(&url)?.and_then(|m| {
            if m.etag.is_empty() {
                None
            } else {
                Some(m.etag)
            }
        });
        if offline {
            return self
                .open_url(&url)
                .ok_or_else(|| DatasheetError::Offline("There is no network and no saved copy.".into()));
        }
        match transport.fetch(&url, prior_etag.as_deref(), false)? {
            FetchAnswer::NotModified => self.open_url(&url),
            FetchAnswer::Fresh { bytes, etag } => {
                let tag = etag.as_deref().unwrap_or("");
                let reference = self.store(&url, &bytes, tag, now_secs())?;
                Ok((reference, bytes))
            }
        }
    }

    /// Read a cached PDF by URL without network.
    pub fn open_url(&self, raw_url: &str) -> Result<(ProjectDatasheetRef, Vec<u8>), DatasheetError> {
        let url = accept_datasheet_url(raw_url)?;
        let meta = self
            .meta_for_url(&url)?
            .ok_or(DatasheetError::NotCached)?;
        let bytes = self.open_hash(&meta.content_sha256)?;
        touch_disk(self.root.as_deref(), &meta.content_sha256);
        Ok((
            ProjectDatasheetRef {
                url: meta.source_url,
                content_sha256: meta.content_sha256,
                revision: meta.etag,
            },
            bytes,
        ))
    }

    /// Read a cached PDF by content hash.
    pub fn open_hash(&self, hash: &str) -> Result<Vec<u8>, DatasheetError> {
        let hash = hash.to_ascii_lowercase();
        check_sha256(&hash)?;
        if let Some(root) = &self.root {
            let path = pdf_path(root, &hash);
            let bytes = fs::read(&path).map_err(|_| DatasheetError::NotCached)?;
            if !is_pdf(&bytes) {
                let _ = fs::remove_file(&path);
                let _ = fs::remove_file(meta_path(root, &hash));
                return Err(bad("the saved file is not a PDF"));
            }
            touch(&path);
            return Ok(bytes);
        }
        let m = self.memory.lock().map_err(|_| bad("cache is busy"))?;
        m.by_hash
            .get(&hash)
            .map(|e| e.bytes.clone())
            .ok_or(DatasheetError::NotCached)
    }

    /// Path to a cached PDF on disk, when the cache uses a folder and the file is present.
    pub fn pdf_path_for_hash(&self, hash: &str) -> Result<Option<PathBuf>, DatasheetError> {
        let hash = hash.to_ascii_lowercase();
        check_sha256(&hash)?;
        let Some(root) = &self.root else {
            return Ok(None);
        };
        let path = pdf_path(root, &hash);
        Ok(path.is_file().then_some(path))
    }

    /// Copy a user-chosen local PDF into the cache. The project ref URL is empty.
    pub fn import_local_file(&self, path: &Path) -> Result<(ProjectDatasheetRef, Vec<u8>), DatasheetError> {
        if !path.is_absolute() {
            return Err(bad("choose a full path to the file"));
        }
        let meta = fs::metadata(path).map_err(|e| DatasheetError::Unavailable(format!("could not read the file: {e}")))?;
        if !meta.is_file() {
            return Err(bad("that path is not a file"));
        }
        let len = meta.len();
        if len > MAX_PDF_BYTES as u64 {
            return Err(bad("the file is too large to keep"));
        }
        let bytes = fs::read(path).map_err(|e| DatasheetError::Unavailable(format!("could not read the file: {e}")))?;
        let reference = self.store("", &bytes, "", now_secs())?;
        Ok((reference, bytes))
    }

    /// Total bytes of stored PDFs on disk.
    pub fn disk_bytes(&self) -> u64 {
        self.root
            .as_ref()
            .map(|r| stored_pdfs(&r.join("files")).iter().map(|f| f.size).sum())
            .unwrap_or(0)
    }

    /// Remove every stored file and URL index entry.
    pub fn purge(&self) -> Result<(), DatasheetError> {
        if let Ok(mut m) = self.memory.lock() {
            *m = Memory::default();
        }
        if let Some(root) = &self.root {
            for sub in ["files", "urls"] {
                let dir = root.join(sub);
                if dir.exists() {
                    fs::remove_dir_all(&dir).map_err(io_err)?;
                }
            }
        }
        Ok(())
    }

    fn meta_for_url(&self, url: &str) -> Result<Option<StoredMeta>, DatasheetError> {
        if let Some(root) = &self.root {
            let link = link_path(root, url);
            let Ok(hash_bytes) = fs::read(&link) else {
                return Ok(None);
            };
            let hash = String::from_utf8_lossy(&hash_bytes).trim().to_ascii_lowercase();
            check_sha256(&hash)?;
            let Ok(text) = fs::read_to_string(meta_path(root, &hash)) else {
                return Ok(None);
            };
            let meta: StoredMeta = serde_json::from_str(&text).map_err(|_| bad("cache metadata is damaged"))?;
            if meta.content_sha256 != hash {
                return Ok(None);
            }
            return Ok(Some(meta));
        }
        let m = self.memory.lock().map_err(|_| bad("cache is busy"))?;
        let hash = match m.url_to_hash.get(url) {
            Some(h) => h,
            None => return Ok(None),
        };
        Ok(m.by_hash.get(hash).map(|e| e.meta.clone()))
    }

    fn evict_disk(&self, root: &Path) {
        let mut files = stored_pdfs(&root.join("files"));
        let mut total: u64 = files.iter().map(|f| f.size).sum();
        if total <= self.cap {
            return;
        }
        files.sort_by_key(|f| f.used);
        for f in files {
            if total <= self.cap {
                break;
            }
            let hash = f
                .path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.to_owned());
            let _ = fs::remove_file(&f.path);
            if let Some(h) = hash {
                let _ = fs::remove_file(meta_path(root, &h));
            }
            total = total.saturating_sub(f.size);
        }
    }
}

fn touch_disk(root: Option<&Path>, hash: &str) {
    if let Some(root) = root {
        touch(&pdf_path(root, hash));
    }
}

struct StoredPdf {
    path: PathBuf,
    size: u64,
    used: SystemTime,
}

fn stored_pdfs(dir: &Path) -> Vec<StoredPdf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    let mut seen = 0usize;
    while let Some(d) = stack.pop() {
        let Ok(read) = fs::read_dir(&d) else {
            continue;
        };
        for entry in read.flatten() {
            seen += 1;
            if seen > 100_000 {
                return out;
            }
            let path = entry.path();
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            if meta.is_dir() {
                stack.push(path);
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.ends_with(".meta") || name.starts_with(".tmp-") {
                continue;
            }
            if name.len() != 64 || !name.chars().all(|c| c.is_ascii_hexdigit()) {
                continue;
            }
            out.push(StoredPdf {
                path,
                size: meta.len(),
                used: meta.modified().unwrap_or(UNIX_EPOCH),
            });
        }
    }
    out
}

fn touch(path: &Path) {
    if let Ok(f) = fs::OpenOptions::new().append(true).open(path) {
        let _ = f.set_modified(SystemTime::now());
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let n = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp = path.with_file_name(format!(".tmp-{}-{n}-{}", std::process::id(), now_secs()));
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all().ok();
    }
    fs::rename(&tmp, path).inspect_err(|_| {
        let _ = fs::remove_file(&tmp);
    })
}
