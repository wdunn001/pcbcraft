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
    PartResult, PartSource, ResponseCache, SourceError, MAX_RESULTS, check_query,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};

const MAX_SOURCE_ID_BYTES: usize = 64;
const MAX_BODY_BYTES: usize = 4 * 1024 * 1024;
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Whether a stored answer should be shown as current, dated, or gone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnswerAge {
    Fresh,
    Stale { retrieved_at: u64 },
    Expired,
}

/// One search outcome, with how old the bytes are.
#[derive(Clone, Debug, PartialEq)]
pub struct CachedSearch {
    pub results: Vec<PartResult>,
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

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), SourceError> {
    let n = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp = path.with_file_name(format!(
        ".tmp-{}-{n}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    ));
    {
        let mut f = fs::File::create(&tmp).map_err(|e| {
            SourceError::Unavailable(format!("response cache write failed: {e}"))
        })?;
        f.write_all(bytes).map_err(|e| {
            SourceError::Unavailable(format!("response cache write failed: {e}"))
        })?;
        f.sync_all().ok();
    }
    fs::rename(&tmp, path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        SourceError::Unavailable(format!("response cache write failed: {e}"))
    })
}

#[derive(Default, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct StoredResponse {
    stored_at: u64,
    results: Vec<PartResult>,
}

struct DiskLayer {
    root: Option<PathBuf>,
    memory: Mutex<BTreeMap<String, StoredResponse>>,
}

impl DiskLayer {
    fn open(root: Option<PathBuf>) -> Self {
        Self {
            root,
            memory: Mutex::new(BTreeMap::new()),
        }
    }

    fn path(&self, source: &str, key: &str) -> Result<Option<PathBuf>, SourceError> {
        check_source_id(source)?;
        Ok(self.root.as_ref().map(|r| {
            r.join("responses")
                .join(source)
                .join(format!("{}.json", content_hash(key.as_bytes())))
        }))
    }

    fn put(&self, source: &str, key: &str, stored_at: u64, results: &[PartResult]) -> Result<(), SourceError> {
        let body = serde_json::to_string(results).map_err(|e| bad(&format!("could not store the answer: {e}")))?;
        if body.len() > MAX_BODY_BYTES {
            return Err(bad("the answer is too large to keep"));
        }
        let record = StoredResponse {
            stored_at,
            results: results.to_vec(),
        };
        let Some(path) = self.path(source, key)? else {
            let mut m = self.memory.lock().map_err(lock_err)?;
            if m.len() < 512 {
                m.insert(format!("{source}\u{1f}{key}"), record);
            }
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                SourceError::Unavailable(format!("response cache write failed: {e}"))
            })?;
        }
        let text = format!("{stored_at}\n{body}");
        write_atomic(&path, text.as_bytes())?;
        Ok(())
    }

    fn get(&self, source: &str, key: &str) -> Result<Option<StoredResponse>, SourceError> {
        let Some(path) = self.path(source, key)? else {
            let m = self.memory.lock().map_err(lock_err)?;
            return Ok(m.get(&format!("{source}\u{1f}{key}")).cloned());
        };
        let Ok(text) = fs::read_to_string(&path) else {
            return Ok(None);
        };
        let Some((head, body)) = text.split_once('\n') else {
            let _ = fs::remove_file(&path);
            return Ok(None);
        };
        let Ok(stored_at) = head.parse::<u64>() else {
            let _ = fs::remove_file(&path);
            return Ok(None);
        };
        let parsed: Result<Vec<PartResult>, _> = serde_json::from_str(body);
        let Ok(results) = parsed else {
            let _ = fs::remove_file(&path);
            return Ok(None);
        };
        Ok(Some(StoredResponse {
            stored_at,
            results,
        }))
    }

    fn remove(&self, source: &str, key: &str) -> Result<(), SourceError> {
        if let Some(path) = self.path(source, key)? {
            let _ = fs::remove_file(path);
        }
        if let Ok(mut m) = self.memory.lock() {
            m.remove(&format!("{source}\u{1f}{key}"));
        }
        Ok(())
    }

    fn purge(&self) -> Result<(), SourceError> {
        if let Ok(mut m) = self.memory.lock() {
            m.clear();
        }
        if let Some(root) = &self.root {
            let dir = root.join("responses");
            if dir.exists() {
                fs::remove_dir_all(&dir).map_err(|e| {
                    SourceError::Unavailable(format!("response cache purge failed: {e}"))
                })?;
            }
        }
        Ok(())
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
        AnswerAge::Stale { retrieved_at: stored_at }
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
pub struct PartSourceCache {
    memory: ResponseCache,
    disk: DiskLayer,
    pacer: SourcePacer,
    offline: bool,
}

impl PartSourceCache {
    pub fn in_memory() -> Self {
        Self {
            memory: ResponseCache::default(),
            disk: DiskLayer::open(None),
            pacer: SourcePacer::new(4.0),
            offline: false,
        }
    }

    pub fn open(root: &Path) -> Self {
        Self {
            memory: ResponseCache::default(),
            disk: DiskLayer::open(Some(root.to_path_buf())),
            pacer: SourcePacer::new(4.0),
            offline: false,
        }
    }

    pub fn with_default_rate(mut self, requests_per_sec: f64) -> Self {
        self.pacer = SourcePacer::new(requests_per_sec);
        self
    }

    pub fn set_offline(&mut self, offline: bool) {
        self.offline = offline;
    }

    pub fn is_offline(&self) -> bool {
        self.offline
    }

    pub fn pause_remaining(&self, source: &dyn PartSource, now: u64) -> Option<u64> {
        self.pacer.pause_remaining(source.id(), now)
    }

    /// Drop answers older than each source's own window from disk.
    pub fn purge_expired(&self, source: &dyn PartSource, now: u64) -> Result<(), SourceError> {
        let window = source.cache_secs();
        if let Some(root) = self.disk.root.as_ref() {
            let base = root.join("responses").join(source.id());
            if base.is_dir() {
                for entry in fs::read_dir(&base).into_iter().flatten().flatten() {
                    let path = entry.path();
                    if !path.is_file() {
                        continue;
                    }
                    let Ok(text) = fs::read_to_string(&path) else {
                        let _ = fs::remove_file(&path);
                        continue;
                    };
                    let stored_at = text
                        .split_once('\n')
                        .and_then(|(h, _)| h.parse::<u64>().ok());
                    if stored_at.is_some_and(|t| now.saturating_sub(t) >= window) {
                        let _ = fs::remove_file(&path);
                    }
                }
            }
        }
        Ok(())
    }

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
                AnswerAge::Stale { .. } | AnswerAge::Expired => {}
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
        self.disk.put(source.id(), &key, now, &results)?;
        self.memory
            .remember(source, query, now, results.clone())?;
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
