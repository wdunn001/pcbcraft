//! Dated on-disk cache and per-source request pacing for part source searches.
//! Answers live under the same online cache root as CAD files, in a `responses`
//! folder, so transport and settings can share one directory (#630).
//!
//! # Rules
//! - Each record holds source id, query key, stored time and a JSON body.
//! - While `now - stored_at < cache_secs` the answer is fresh and re-used online.
//! - After that the answer is stale: offline mode still serves it with its date;
//!   online mode asks the source again. Expired entries are removed from disk.
//! - A token bucket per source caps how often the source is asked.
use pcbcraft_library::cad_cache::{content_hash, default_cache_dir};
use pcbcraft_library::sources::{
    MAX_RESULTS, PartResult, PartSource, ResponseCache, SourceError, check_query,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[cfg(not(target_arch = "wasm32"))]
use std::fs;
#[cfg(not(target_arch = "wasm32"))]
use std::io::Write;
#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;
use std::path::PathBuf;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

const MAX_SOURCE_ID_BYTES: usize = 64;
const MAX_BODY_BYTES: usize = 4 * 1024 * 1024;
/// Most answers kept for one source; the oldest go first.
const MAX_ANSWERS_PER_SOURCE: usize = 128;
/// Words that mark text as holding a key or token. An answer with any of them is never stored.
const KEY_MARKERS: [&str; 12] = [
    "client_secret",
    "client_id",
    "clientid",
    "secret",
    "api_key",
    "apikey",
    "api-key",
    "access_token",
    "authorization",
    "bearer ",
    "password",
    "token=",
];
#[cfg(not(target_arch = "wasm32"))]
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Whether a stored answer should be shown as current, dated, or gone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnswerAge {
    Fresh,
    Stale { retrieved_at: u64 },
}

/// One search outcome, with how old the bytes are.
#[derive(Clone, Debug, PartialEq)]
pub struct CachedSearch {
    /// Parts returned for the query.
    pub results: Vec<PartResult>,
    /// Fresh or stale with the retrieval time stored on disk.
    pub age: AnswerAge,
}

fn bad(text: &str) -> SourceError {
    SourceError::Invalid(text.to_owned())
}

fn check_source_id(source: &str) -> Result<(), SourceError> {
    if source.is_empty()
        || source.len() > MAX_SOURCE_ID_BYTES
        || source
            .chars()
            .any(|c| c.is_control() || matches!(c, '/' | '\\' | ':'))
    {
        return Err(bad("the source id is unusable for the response cache"));
    }
    Ok(())
}

fn cache_key(source: &str, query: &str) -> String {
    format!("{source}\u{1f}search\u{1f}{}", query.trim().to_lowercase())
}

/// True when the text names a key, secret or token.
fn holds_a_key(text: &str) -> bool {
    let low = text.to_ascii_lowercase();
    KEY_MARKERS.iter().any(|m| low.contains(m))
}

/// Where dated answers are kept: one small text file per answer, grouped by source.
///
/// # Rules
/// - `name` is a file name made by the cache; `source` was checked by the cache.
/// - The desktop app uses [`DiskFiles`]; the browser build and the tests use [`MemoryFiles`].
///
/// # Do not
/// - Call `std::fs` for answers anywhere else: the browser build has no disk.
pub trait AnswerFiles: Send + Sync {
    /// The text of one answer file, or `None` when it is missing or unreadable.
    fn read(&self, source: &str, name: &str) -> Option<String>;
    /// Replace one answer file.
    fn write(&self, source: &str, name: &str, text: &str) -> Result<(), SourceError>;
    /// Delete one answer file; a missing file is fine.
    fn remove(&self, source: &str, name: &str);
    /// Every answer file name held for the source.
    fn names(&self, source: &str) -> Vec<String>;
    /// Delete every answer of every source.
    fn clear(&self) -> Result<(), SourceError>;
}

/// Answers held in memory, lost when the program ends.
#[derive(Default)]
pub struct MemoryFiles {
    files: Mutex<BTreeMap<(String, String), String>>,
}

/// Contract: [`AnswerFiles`]
impl AnswerFiles for MemoryFiles {
    fn read(&self, source: &str, name: &str) -> Option<String> {
        let m = self.files.lock().ok()?;
        m.get(&(source.to_owned(), name.to_owned())).cloned()
    }
    fn write(&self, source: &str, name: &str, text: &str) -> Result<(), SourceError> {
        let mut m = self.files.lock().map_err(lock_err)?;
        m.insert((source.to_owned(), name.to_owned()), text.to_owned());
        Ok(())
    }
    fn remove(&self, source: &str, name: &str) {
        if let Ok(mut m) = self.files.lock() {
            m.remove(&(source.to_owned(), name.to_owned()));
        }
    }
    fn names(&self, source: &str) -> Vec<String> {
        match self.files.lock() {
            Ok(m) => m
                .keys()
                .filter(|(s, _)| s == source)
                .map(|(_, n)| n.clone())
                .collect(),
            Err(_) => Vec::new(),
        }
    }
    fn clear(&self) -> Result<(), SourceError> {
        self.files.lock().map_err(lock_err)?.clear();
        Ok(())
    }
}

/// Answers kept as files under `{root}/responses/{source}/`. Not built for the browser.
#[cfg(not(target_arch = "wasm32"))]
pub struct DiskFiles {
    root: PathBuf,
}

#[cfg(not(target_arch = "wasm32"))]
impl DiskFiles {
    fn dir(&self, source: &str) -> PathBuf {
        self.root.join("responses").join(source)
    }
}

/// Contract: [`AnswerFiles`]
#[cfg(not(target_arch = "wasm32"))]
impl AnswerFiles for DiskFiles {
    fn read(&self, source: &str, name: &str) -> Option<String> {
        fs::read_to_string(self.dir(source).join(name)).ok()
    }
    fn write(&self, source: &str, name: &str, text: &str) -> Result<(), SourceError> {
        let dir = self.dir(source);
        fs::create_dir_all(&dir)
            .map_err(|e| SourceError::Unavailable(format!("response cache write failed: {e}")))?;
        write_atomic(&dir.join(name), text.as_bytes())
    }
    fn remove(&self, source: &str, name: &str) {
        let _ = fs::remove_file(self.dir(source).join(name));
    }
    fn names(&self, source: &str) -> Vec<String> {
        let dir = self.dir(source);
        if !dir.is_dir() {
            return Vec::new();
        }
        fs::read_dir(&dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.path().is_file())
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|n| !n.starts_with(".tmp-"))
            .collect()
    }
    fn clear(&self) -> Result<(), SourceError> {
        let dir = self.root.join("responses");
        if dir.exists() {
            fs::remove_dir_all(&dir).map_err(|e| {
                SourceError::Unavailable(format!("response cache purge failed: {e}"))
            })?;
        }
        Ok(())
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), SourceError> {
    let n = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let stamp = pcbcraft_clock::unix_secs();
    let tmp = path.with_file_name(format!(".tmp-{}-{n}-{stamp}", std::process::id()));
    {
        let mut f = fs::File::create(&tmp)
            .map_err(|e| SourceError::Unavailable(format!("response cache write failed: {e}")))?;
        f.write_all(bytes)
            .map_err(|e| SourceError::Unavailable(format!("response cache write failed: {e}")))?;
        f.sync_all().ok();
    }
    fs::rename(&tmp, path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        SourceError::Unavailable(format!("response cache write failed: {e}"))
    })
}

#[derive(Default, Clone, Debug, PartialEq, Serialize, Deserialize)]
struct StoredResponse {
    stored_at: u64,
    results: Vec<PartResult>,
}

/// Reads and writes dated answers through an [`AnswerFiles`].
struct DiskLayer {
    files: Arc<dyn AnswerFiles>,
}

fn file_name(key: &str) -> String {
    format!("{}.json", content_hash(key.as_bytes()))
}

fn stored_time(text: &str) -> Option<u64> {
    text.split_once('\n')
        .and_then(|(h, _)| h.parse::<u64>().ok())
}

impl DiskLayer {
    fn put(
        &self,
        source: &str,
        key: &str,
        stored_at: u64,
        results: &[PartResult],
    ) -> Result<(), SourceError> {
        check_source_id(source)?;
        let body = serde_json::to_string(results)
            .map_err(|e| bad(&format!("could not store the answer: {e}")))?;
        if body.len() > MAX_BODY_BYTES {
            return Err(bad("the answer is too large to keep"));
        }
        if holds_a_key(&body) {
            return Err(bad("the answer holds a key and is not kept"));
        }
        let text = format!("{stored_at}\n{body}");
        self.files.write(source, &file_name(key), &text)?;
        self.trim(source);
        Ok(())
    }

    /// Drop the oldest answers of a source while it holds more than the bound.
    fn trim(&self, source: &str) {
        let names = self.files.names(source);
        if names.len() <= MAX_ANSWERS_PER_SOURCE {
            return;
        }
        let mut dated: Vec<(u64, String)> = names
            .into_iter()
            .map(|n| {
                let at = self
                    .files
                    .read(source, &n)
                    .as_deref()
                    .and_then(stored_time)
                    .unwrap_or(0);
                (at, n)
            })
            .collect();
        dated.sort();
        let extra = dated.len() - MAX_ANSWERS_PER_SOURCE;
        for (_, name) in dated.into_iter().take(extra) {
            self.files.remove(source, &name);
        }
    }

    fn get(&self, source: &str, key: &str) -> Result<Option<StoredResponse>, SourceError> {
        check_source_id(source)?;
        let name = file_name(key);
        let Some(text) = self.files.read(source, &name) else {
            return Ok(None);
        };
        let Some((head, body)) = text.split_once('\n') else {
            self.files.remove(source, &name);
            return Ok(None);
        };
        let Ok(stored_at) = head.parse::<u64>() else {
            self.files.remove(source, &name);
            return Ok(None);
        };
        let parsed: Result<Vec<PartResult>, _> = serde_json::from_str(body);
        let Ok(results) = parsed else {
            self.files.remove(source, &name);
            return Ok(None);
        };
        Ok(Some(StoredResponse { stored_at, results }))
    }

    fn purge(&self) -> Result<(), SourceError> {
        self.files.clear()
    }
}

fn lock_err<T>(_: PoisonError<T>) -> SourceError {
    bad("the response cache is busy")
}

fn classify(stored_at: u64, cache_secs: u64, now: u64) -> AnswerAge {
    let age = now.saturating_sub(stored_at);
    if age < cache_secs {
        AnswerAge::Fresh
    } else {
        AnswerAge::Stale {
            retrieved_at: stored_at,
        }
    }
}

/// Token bucket shared by every search on one source id.
#[derive(Clone, Debug)]
pub struct TokenBucket {
    capacity: f64,
    refill_per_sec: f64,
    tokens: f64,
    updated_at: u64,
    paused_until: u64,
}

impl TokenBucket {
    /// A bucket that allows at most `max_per_sec` requests, capped at one burst.
    pub fn new(max_per_sec: f64) -> Self {
        let rate = max_per_sec.max(0.01);
        Self {
            capacity: 1.0,
            refill_per_sec: rate,
            tokens: 1.0,
            updated_at: 0,
            paused_until: 0,
        }
    }

    fn refill(&mut self, now: u64) {
        if now < self.paused_until {
            return;
        }
        let elapsed = now.saturating_sub(self.updated_at) as f64;
        self.tokens = (self.tokens + elapsed * self.refill_per_sec).min(self.capacity);
        self.updated_at = now;
    }

    /// Take one token, or return seconds to wait.
    pub fn acquire(&mut self, now: u64) -> Result<(), u64> {
        if now < self.paused_until {
            return Err(self.paused_until - now);
        }
        self.refill(now);
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            Ok(())
        } else {
            let need = 1.0 - self.tokens;
            Err((need / self.refill_per_sec).ceil().max(1.0) as u64)
        }
    }

    /// Pause after the source returned rate limited.
    pub fn pause(&mut self, now: u64, retry_after_secs: u64) {
        self.paused_until = now.saturating_add(retry_after_secs.max(1));
        self.tokens = 0.0;
        self.updated_at = now;
    }

    /// Seconds left in a source-requested pause, for a status line.
    pub fn pause_remaining(&self, now: u64) -> Option<u64> {
        (now < self.paused_until).then_some(self.paused_until - now)
    }
}

/// Per-source buckets keyed by `PartSource::id`.
#[derive(Default, Debug)]
pub struct SourcePacer {
    buckets: BTreeMap<String, TokenBucket>,
    default_rate: f64,
}

impl SourcePacer {
    pub fn new(default_requests_per_sec: f64) -> Self {
        Self {
            buckets: BTreeMap::new(),
            default_rate: default_requests_per_sec.max(0.01),
        }
    }

    fn bucket(&mut self, source: &str, max_per_sec: f64) -> &mut TokenBucket {
        let rate = if max_per_sec > 0.0 {
            max_per_sec
        } else {
            self.default_rate
        };
        self.buckets
            .entry(source.to_owned())
            .or_insert_with(|| TokenBucket::new(rate))
    }

    pub fn acquire(&mut self, source_id: &str, max_per_sec: f64, now: u64) -> Result<(), u64> {
        self.bucket(source_id, max_per_sec).acquire(now)
    }

    pub fn pause(&mut self, source_id: &str, now: u64, retry_after_secs: u64) {
        self.bucket(source_id, self.default_rate)
            .pause(now, retry_after_secs);
    }

    pub fn pause_remaining(&self, source_id: &str, now: u64) -> Option<u64> {
        self.buckets
            .get(source_id)
            .and_then(|b| b.pause_remaining(now))
    }
}

/// Memory, disk and pacing for part source searches.
///
/// # Rules
/// - Answers live as files under `{cache root}/responses/{source id}/` (through [`AnswerFiles`]).
/// - Offline mode serves stored rows, fresh or stale, and never calls the source; online mode re-asks
///   after the source window.
/// - Every answer is dated: each part carries `retrieved_at`, set when the source answered.
/// - An answer that names a key, secret or token, or is too large, is returned but never stored.
/// - A failure to store an answer never fails the search.
/// - `search` uses `now` as seconds since the Unix epoch (tests pass a fake clock).
///
/// # Dependencies
/// - crates/library/src/sources.rs `PartSource`, `ResponseCache`
/// - crates/library/src/cad_cache.rs `default_cache_dir`, `content_hash`
/// - crates/ui-egui/src/distributors.rs (the Library dock search)
///
/// # Do not
/// - Open the disk store in the browser build; it uses [`MemoryFiles`] there.
pub struct PartSourceCache {
    memory: ResponseCache,
    disk: DiskLayer,
    pacer: SourcePacer,
    offline: bool,
}

impl Default for PartSourceCache {
    fn default() -> Self {
        Self::in_memory()
    }
}

impl PartSourceCache {
    /// Cache that keeps answers in memory only.
    pub fn in_memory() -> Self {
        Self::with_files(Arc::new(MemoryFiles::default()))
    }

    /// Cache over any file store, such as an in-memory stand-in in tests.
    pub fn with_files(files: Arc<dyn AnswerFiles>) -> Self {
        Self {
            memory: ResponseCache::default(),
            disk: DiskLayer { files },
            pacer: SourcePacer::new(4.0),
            offline: false,
        }
    }

    /// Open or create the response cache under `root` (desktop builds only).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn open(root: &Path) -> Self {
        Self::with_files(Arc::new(DiskFiles {
            root: root.to_path_buf(),
        }))
    }

    /// Set the fallback requests-per-second when a source returns zero.
    pub fn with_default_rate(mut self, requests_per_sec: f64) -> Self {
        self.pacer = SourcePacer::new(requests_per_sec);
        self
    }

    /// When true, stale disk answers are returned instead of calling the source.
    pub fn set_offline(&mut self, offline: bool) {
        self.offline = offline;
    }

    /// Whether offline mode is active.
    pub fn is_offline(&self) -> bool {
        self.offline
    }

    /// Seconds left in a source-requested rate-limit pause, if any.
    pub fn pause_remaining(&self, source: &dyn PartSource, now: u64) -> Option<u64> {
        self.pacer.pause_remaining(source.id(), now)
    }

    /// Drop answers older than each source's own window from the store.
    pub fn purge_expired(&self, source: &dyn PartSource, now: u64) -> Result<(), SourceError> {
        check_source_id(source.id())?;
        let window = source.cache_secs();
        let files = &self.disk.files;
        for name in files.names(source.id()) {
            let Some(text) = files.read(source.id(), &name) else {
                files.remove(source.id(), &name);
                continue;
            };
            let expired = match stored_time(&text) {
                Some(t) => now.saturating_sub(t) >= window,
                None => true,
            };
            if expired {
                files.remove(source.id(), &name);
            }
        }
        Ok(())
    }

    /// Remove every cached answer from memory and disk.
    pub fn purge(&mut self) -> Result<(), SourceError> {
        self.memory.purge();
        self.disk.purge()
    }

    fn max_rate(source: &dyn PartSource) -> f64 {
        source.requests_per_sec()
    }

    fn read_disk(
        &self,
        source: &dyn PartSource,
        key: &str,
        now: u64,
    ) -> Result<Option<CachedSearch>, SourceError> {
        let Some(stored) = self.disk.get(source.id(), key)? else {
            return Ok(None);
        };
        let age = classify(stored.stored_at, source.cache_secs(), now);
        Ok(Some(CachedSearch {
            results: stored.results,
            age,
        }))
    }

    /// Search with a fake clock. When online, stale disk entries trigger a new
    /// ask after pacing; when offline, stale entries are returned with their age.
    pub fn search(
        &mut self,
        source: &dyn PartSource,
        query: &str,
        now: u64,
    ) -> Result<CachedSearch, SourceError> {
        let query = check_query(query)?;
        let key = cache_key(source.id(), query);

        if let Some(results) = self.memory.get_if_fresh(source, query, now) {
            return Ok(CachedSearch {
                results,
                age: AnswerAge::Fresh,
            });
        }

        if let Some(disk) = self.read_disk(source, &key, now)? {
            match disk.age {
                AnswerAge::Fresh => return Ok(disk),
                AnswerAge::Stale { .. } if self.offline => return Ok(disk),
                AnswerAge::Stale { .. } => {}
            }
        }

        if self.offline {
            return Err(SourceError::Unavailable(
                "Offline: no saved answer for that search.".into(),
            ));
        }

        if let Err(wait) = self.pacer.acquire(source.id(), Self::max_rate(source), now) {
            return Err(SourceError::RateLimited {
                retry_after_secs: wait,
            });
        }

        let mut results = match source.search(query) {
            Ok(r) => r,
            Err(SourceError::RateLimited { retry_after_secs }) => {
                self.pacer.pause(source.id(), now, retry_after_secs);
                return Err(SourceError::RateLimited { retry_after_secs });
            }
            Err(e) => return Err(e),
        };
        results.truncate(MAX_RESULTS);
        for part in &mut results {
            part.retrieved_at = now;
        }
        // Keeping the answer is best effort: a refused or failed write still returns it.
        if self.disk.put(source.id(), &key, now, &results).is_ok() {
            self.memory.remember(source, query, now, results.clone())?;
        }
        Ok(CachedSearch {
            results,
            age: AnswerAge::Fresh,
        })
    }
}

/// Default folder for part source responses: the online cache root.
pub fn default_response_cache_dir() -> Option<PathBuf> {
    default_cache_dir()
}

#[cfg(test)]
#[path = "source_cache_tests.rs"]
mod tests;
