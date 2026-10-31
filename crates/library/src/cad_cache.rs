//! Disk cache for files fetched by an online CAD source, so a part fetched
//! once opens with no network. Files are keyed by source, upstream path and
//! hash and never expire (the open licence allows keeping copies); a size cap
//! evicts the least recently used files. Listings are stored beside them and
//! refreshed on demand.
//!
//! # Rules
//! - Every path piece is validated before it touches the disk; nothing climbs out of the root.
//! - A file that fails its stored checksum, or a listing that does not parse, is a miss and is removed.
//! - Writes go to a temporary file and are renamed into place.
//! - With no cache directory the cache keeps a bounded copy in memory only.
//!
//! # Dependencies
//! - crates/library/src/cad.rs
//! - crates/library/src/sources.rs
use crate::sources::SourceError;
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Largest single file the cache stores: 8 MiB.
pub const MAX_CACHED_FILE_BYTES: usize = 8 * 1024 * 1024;
/// Default cap on the whole cache: 512 MiB.
pub const DEFAULT_CACHE_CAP_BYTES: u64 = 512 * 1024 * 1024;
/// Most bytes the memory-only fallback keeps.
const MEMORY_CAP_BYTES: usize = 32 * 1024 * 1024;
const MAX_PATH_BYTES: usize = 512;
const MAX_PIECE_BYTES: usize = 200;
const LATEST: &str = "LATEST";

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// What the cache holds, for the line the Library tab shows next to the Online source.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CacheStats {
    /// Stored files.
    pub files: usize,
    /// Bytes of stored files.
    pub bytes: u64,
    /// Bytes of stored listings.
    pub listing_bytes: u64,
    /// The cap on stored file bytes.
    pub cap: u64,
    /// Stored listings.
    pub listings: usize,
    /// Second the oldest stored listing was saved, when there is one.
    pub oldest_listing: Option<u64>,
}

impl CacheStats {
    /// Whole seconds since the oldest listing was saved, or `None` with no listings.
    pub fn oldest_age_secs(&self, now: u64) -> Option<u64> {
        self.oldest_listing.map(|t| now.saturating_sub(t))
    }
}

/// A stored answer to a listing request, with the second it was stored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredListing {
    /// Seconds since the Unix epoch when the answer was stored.
    pub stored_at: u64,
    /// The stored text.
    pub body: String,
}

/// Cheap content hash used as the key when a source gives no revision.
/// It guards against truncated or damaged files, not against an attacker.
pub fn content_hash(bytes: &[u8]) -> String {
    let mut h: u128 = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d;
    for b in bytes {
        h ^= u128::from(*b);
        h = h.wrapping_mul(0x0000_0000_0100_0000_0000_0000_0000_013b);
    }
    format!("{h:032x}")
}

fn bad(text: &str) -> SourceError {
    SourceError::Invalid(text.to_owned())
}

fn check_source(source: &str) -> Result<(), SourceError> {
    check_piece(source)
}

fn check_piece(piece: &str) -> Result<(), SourceError> {
    if piece.is_empty()
        || piece.len() > MAX_PIECE_BYTES
        || piece == "."
        || piece.contains("..")
        || piece
            .chars()
            .any(|c| c.is_control() || matches!(c, '/' | '\\' | ':'))
    {
        return Err(bad("the cache key has an unusable path piece"));
    }
    Ok(())
}

/// Upstream path split into checked pieces.
fn pieces(path: &str) -> Result<Vec<&str>, SourceError> {
    if path.is_empty() || path.len() > MAX_PATH_BYTES || path.starts_with('/') {
        return Err(bad("the cache key path is empty, too long or absolute"));
    }
    let parts: Vec<&str> = path.split('/').collect();
    for p in &parts {
        check_piece(p)?;
    }
    Ok(parts)
}

fn check_hash(hash: &str) -> Result<(), SourceError> {
    if hash.len() < 8
        || hash.len() > 64
        || !hash
            .chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
    {
        return Err(bad("the cache hash must be 8 to 64 lowercase hex digits"));
    }
    Ok(())
}

fn now_secs() -> u64 {
    pcbcraft_clock::unix_secs()
}

/// Platform cache folder for online library files, or `None` when the
/// environment names no home. `get` reads one variable.
///
/// # Rules
/// - Linux and others: `XDG_CACHE_HOME`, else `HOME/.cache`; macOS: `HOME/Library/Caches`;
///   Windows: `LOCALAPPDATA`. Always a `pcbcraft/online` folder below that.
/// - Relative values are ignored.
pub fn default_cache_dir_with(get: &dyn Fn(&str) -> Option<String>, os: &str) -> Option<PathBuf> {
    let absolute = |name: &str| {
        get(name)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
    };
    let base = match os {
        "windows" => absolute("LOCALAPPDATA")?,
        "macos" => absolute("HOME")?.join("Library").join("Caches"),
        _ => absolute("XDG_CACHE_HOME").or_else(|| absolute("HOME").map(|h| h.join(".cache")))?,
    };
    Some(base.join("pcbcraft").join("online"))
}

/// [`default_cache_dir_with`] over the real environment and operating system.
pub fn default_cache_dir() -> Option<PathBuf> {
    default_cache_dir_with(&|n| std::env::var(n).ok(), std::env::consts::OS)
}

#[derive(Default)]
struct Memory {
    files: BTreeMap<String, Vec<u8>>,
    listings: BTreeMap<String, StoredListing>,
    bytes: usize,
}

/// Files and listings fetched from online CAD sources.
///
/// # Rules
/// - Keys are source id, upstream path and hash; see [`CadCache::put`].
/// - Reading a file marks it recently used for eviction.
///
/// # Dependencies
/// - crates/library/src/cad.rs
///
/// # Do not
/// - Store distributor answers here; they live in `ResponseCache` in crates/library/src/sources.rs.
/// - Expire files by age.
pub struct CadCache {
    root: Option<PathBuf>,
    cap: u64,
    memory: Mutex<Memory>,
}

impl CadCache {
    /// A cache rooted at `root`; the folder is created on first write.
    pub fn open(root: &Path) -> Self {
        Self {
            root: Some(root.to_path_buf()),
            cap: DEFAULT_CACHE_CAP_BYTES,
            memory: Mutex::new(Memory::default()),
        }
    }

    /// A cache that keeps a bounded copy in memory only.
    pub fn in_memory() -> Self {
        Self {
            root: None,
            cap: DEFAULT_CACHE_CAP_BYTES,
            memory: Mutex::new(Memory::default()),
        }
    }

    /// The same cache with a different total size cap, for tests and settings.
    pub fn with_cap(mut self, cap_bytes: u64) -> Self {
        self.cap = cap_bytes;
        self
    }

    /// The folder files are stored in, when there is one.
    pub fn root(&self) -> Option<&Path> {
        self.root.as_deref()
    }

    fn file_dir(&self, source: &str, path: &str) -> Result<Option<PathBuf>, SourceError> {
        check_source(source)?;
        let parts = pieces(path)?;
        Ok(self.root.as_ref().map(|r| {
            let mut dir = r.join("files").join(source);
            for p in parts {
                dir.push(p);
            }
            dir
        }))
    }

    fn mem_key(source: &str, path: &str, hash: &str) -> String {
        format!("{source}\u{1f}{path}\u{1f}{hash}")
    }

    /// Store a file and return the hash it is keyed by: `revision` when the
    /// source gave one, otherwise a hash of the bytes. The newest put becomes
    /// the answer to a lookup that names no hash.
    pub fn put(
        &self,
        source: &str,
        path: &str,
        revision: Option<&str>,
        bytes: &[u8],
    ) -> Result<String, SourceError> {
        if bytes.len() > MAX_CACHED_FILE_BYTES {
            return Err(bad("the file is too large to keep"));
        }
        let hash = match revision {
            Some(r) if !r.is_empty() => r.to_ascii_lowercase(),
            _ => content_hash(bytes),
        };
        check_hash(&hash)?;
        let Some(dir) = self.file_dir(source, path)? else {
            let mut m = self.memory.lock().map_err(|_| bad("cache is busy"))?;
            let key = Self::mem_key(source, path, &hash);
            if let Some(old) = m.files.insert(key, bytes.to_vec()) {
                m.bytes = m.bytes.saturating_sub(old.len());
            }
            m.bytes += bytes.len();
            m.files.insert(
                Self::mem_key(source, path, LATEST),
                hash.clone().into_bytes(),
            );
            while m.bytes > MEMORY_CAP_BYTES {
                let Some(first) = m.files.keys().find(|k| !k.ends_with(LATEST)).cloned() else {
                    break;
                };
                if let Some(old) = m.files.remove(&first) {
                    m.bytes = m.bytes.saturating_sub(old.len());
                }
            }
            return Ok(hash);
        };
        let io = |e: std::io::Error| SourceError::Unavailable(format!("cache write failed: {e}"));
        fs::create_dir_all(&dir).map_err(io)?;
        write_atomic(&dir.join(&hash), bytes).map_err(io)?;
        write_atomic(
            &dir.join(format!("{hash}.sum")),
            content_hash(bytes).as_bytes(),
        )
        .map_err(io)?;
        write_atomic(&dir.join(LATEST), hash.as_bytes()).map_err(io)?;
        self.evict();
        Ok(hash)
    }

    /// Read a stored file. With no `hash` the newest stored version answers.
    /// A damaged file is removed and counts as a miss.
    pub fn get(
        &self,
        source: &str,
        path: &str,
        hash: Option<&str>,
    ) -> Result<Option<Vec<u8>>, SourceError> {
        if let Some(h) = hash.filter(|h| !h.is_empty()) {
            check_hash(&h.to_ascii_lowercase())?;
        }
        let Some(dir) = self.file_dir(source, path)? else {
            let m = self.memory.lock().map_err(|_| bad("cache is busy"))?;
            let hash = match hash.filter(|h| !h.is_empty()) {
                Some(h) => h.to_ascii_lowercase(),
                None => match m.files.get(&Self::mem_key(source, path, LATEST)) {
                    Some(v) => String::from_utf8_lossy(v).into_owned(),
                    None => return Ok(None),
                },
            };
            return Ok(m.files.get(&Self::mem_key(source, path, &hash)).cloned());
        };
        let hash = match hash.filter(|h| !h.is_empty()) {
            Some(h) => h.to_ascii_lowercase(),
            None => match fs::read_to_string(dir.join(LATEST)) {
                Ok(h) => {
                    let h = h.trim().to_owned();
                    if check_hash(&h).is_err() {
                        return Ok(None);
                    }
                    h
                }
                Err(_) => return Ok(None),
            },
        };
        let file = dir.join(&hash);
        let Ok(bytes) = fs::read(&file) else {
            return Ok(None);
        };
        let sum_ok = fs::read_to_string(dir.join(format!("{hash}.sum")))
            .map(|s| s.trim() == content_hash(&bytes))
            .unwrap_or(false);
        if !sum_ok {
            let _ = fs::remove_file(&file);
            let _ = fs::remove_file(dir.join(format!("{hash}.sum")));
            return Ok(None);
        }
        touch(&file);
        Ok(Some(bytes))
    }

    /// Where a stored file lives on disk, when the cache has a folder and the
    /// file is there. `None` for the memory-only cache or an unknown file.
    pub fn file_path(&self, source: &str, path: &str, hash: &str) -> Option<PathBuf> {
        check_hash(&hash.to_ascii_lowercase()).ok()?;
        let dir = self.file_dir(source, path).ok()??;
        let file = dir.join(hash.to_ascii_lowercase());
        file.is_file().then_some(file)
    }

    fn listing_path(&self, source: &str, key: &str) -> Result<Option<PathBuf>, SourceError> {
        check_source(source)?;
        let parts = pieces(key)?;
        Ok(self.root.as_ref().map(|r| {
            let mut p = r.join("listings").join(source);
            for piece in parts {
                p.push(piece);
            }
            p
        }))
    }

    /// Store the answer to a listing request. `now` is seconds since the Unix epoch.
    pub fn put_listing(
        &self,
        source: &str,
        key: &str,
        body: &str,
        now: u64,
    ) -> Result<(), SourceError> {
        if body.len() > MAX_CACHED_FILE_BYTES {
            return Err(bad("the listing is too large to keep"));
        }
        let Some(path) = self.listing_path(source, key)? else {
            let mut m = self.memory.lock().map_err(|_| bad("cache is busy"))?;
            if m.listings.len() < 4096 {
                m.listings.insert(
                    format!("{source}\u{1f}{key}"),
                    StoredListing {
                        stored_at: now,
                        body: body.to_owned(),
                    },
                );
            }
            return Ok(());
        };
        let io = |e: std::io::Error| SourceError::Unavailable(format!("cache write failed: {e}"));
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(io)?;
        }
        let text = format!("{now}\n{body}");
        write_atomic(&path, text.as_bytes()).map_err(io)?;
        Ok(())
    }

    /// Read a stored listing; a file that does not parse is a miss and is removed.
    pub fn get_listing(
        &self,
        source: &str,
        key: &str,
    ) -> Result<Option<StoredListing>, SourceError> {
        let Some(path) = self.listing_path(source, key)? else {
            let m = self.memory.lock().map_err(|_| bad("cache is busy"))?;
            return Ok(m.listings.get(&format!("{source}\u{1f}{key}")).cloned());
        };
        let Ok(text) = fs::read_to_string(&path) else {
            return Ok(None);
        };
        let parsed = text.split_once('\n').and_then(|(t, body)| {
            t.parse::<u64>().ok().map(|stored_at| StoredListing {
                stored_at,
                body: body.to_owned(),
            })
        });
        if parsed.is_none() {
            let _ = fs::remove_file(&path);
        }
        Ok(parsed)
    }

    /// Forget one listing, so the next lookup asks the source again.
    pub fn forget_listing(&self, source: &str, key: &str) -> Result<(), SourceError> {
        if let Some(path) = self.listing_path(source, key)? {
            let _ = fs::remove_file(path);
        } else if let Ok(mut m) = self.memory.lock() {
            m.listings.remove(&format!("{source}\u{1f}{key}"));
        }
        Ok(())
    }

    /// Delete every stored file and listing.
    pub fn purge(&self) -> Result<(), SourceError> {
        if let Ok(mut m) = self.memory.lock() {
            *m = Memory::default();
        }
        if let Some(root) = &self.root {
            for sub in ["files", "listings"] {
                let dir = root.join(sub);
                if dir.exists() {
                    fs::remove_dir_all(&dir).map_err(|e| {
                        SourceError::Unavailable(format!("cache purge failed: {e}"))
                    })?;
                }
            }
        }
        Ok(())
    }

    /// Total bytes of stored files on disk (listings and checksums excluded).
    pub fn disk_bytes(&self) -> u64 {
        self.root
            .as_ref()
            .map(|r| stored_files(&r.join("files")).iter().map(|f| f.size).sum())
            .unwrap_or(0)
    }

    /// Count what is stored. Reads file sizes and the first line of each listing, never a file body.
    ///
    /// # Rules
    /// - Memory-only caches report their own copy, which is bounded by [`MEMORY_CAP_BYTES`].
    pub fn stats(&self) -> CacheStats {
        let mut out = CacheStats {
            cap: self.cap,
            ..CacheStats::default()
        };
        let mut stamps: Vec<u64> = Vec::new();
        let mut listing_bytes = 0u64;
        if let Some(root) = &self.root {
            let files = stored_files(&root.join("files"));
            out.files = files.len();
            out.bytes = files.iter().map(|f| f.size).sum();
            let mut stack = vec![root.join("listings")];
            while let Some(dir) = stack.pop() {
                let Ok(entries) = fs::read_dir(&dir) else {
                    continue;
                };
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        stack.push(path);
                    } else if let Some(t) = listing_stamp(&path) {
                        listing_bytes += entry.metadata().map_or(0, |m| m.len());
                        stamps.push(t);
                    }
                }
            }
        } else if let Ok(m) = self.memory.lock() {
            out.files = m.files.len();
            out.bytes = m.bytes as u64;
            for l in m.listings.values() {
                listing_bytes += l.body.len() as u64;
                stamps.push(l.stored_at);
            }
        }
        out.listing_bytes = listing_bytes;
        out.listings = stamps.len();
        out.oldest_listing = stamps.into_iter().min();
        out
    }

    /// Remove the least recently used files until the cache fits its cap.
    fn evict(&self) {
        let Some(root) = &self.root else {
            return;
        };
        let mut files = stored_files(&root.join("files"));
        let mut total: u64 = files.iter().map(|f| f.size).sum();
        if total <= self.cap {
            return;
        }
        files.sort_by_key(|f| f.used);
        for f in files {
            if total <= self.cap {
                break;
            }
            let _ = fs::remove_file(&f.path);
            if let Some(name) = f.path.file_name().and_then(|n| n.to_str()) {
                let _ = fs::remove_file(f.path.with_file_name(format!("{name}.sum")));
            }
            total = total.saturating_sub(f.size);
        }
    }
}

/// The save time on the first line of a listing file.
fn listing_stamp(path: &Path) -> Option<u64> {
    let mut head = [0u8; 24];
    let n = std::io::Read::read(&mut fs::File::open(path).ok()?, &mut head).ok()?;
    let text = std::str::from_utf8(head.get(..n)?).ok()?;
    text.split('\n').next()?.parse().ok()
}

struct StoredFile {
    path: PathBuf,
    size: u64,
    used: SystemTime,
}

/// Every hash-named file under `dir`, skipping checksums and pointers.
fn stored_files(dir: &Path) -> Vec<StoredFile> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    let mut seen = 0usize;
    while let Some(d) = stack.pop() {
        let Ok(read) = fs::read_dir(&d) else {
            continue;
        };
        for entry in read.flatten() {
            seen += 1;
            if seen > 200_000 {
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
            if name == LATEST || name.ends_with(".sum") || name.starts_with(".tmp-") {
                continue;
            }
            out.push(StoredFile {
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
        let now = UNIX_EPOCH + std::time::Duration::from_secs(pcbcraft_clock::unix_secs());
        let _ = f.set_modified(now);
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
