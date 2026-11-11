use super::*;
use pcbcraft_library::sources::{Attribution, PartResult, PartSource, SetupCard, SourceError};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tempfile::TempDir;

struct Stub {
    hits: AtomicU64,
    cache_secs: u64,
    rate: f64,
}

impl Stub {
    fn part(at: u64) -> PartResult {
        PartResult {
            source: "stub".into(),
            mpn: "TEST-1".into(),
            manufacturer: "Stub".into(),
            description: String::new(),
            kind: String::new(),
            distributor: String::new(),
            sku: String::new(),
            stock: 1,
            price_breaks: vec![],
            lifecycle: String::new(),
            datasheet_url: String::new(),
            product_url: String::new(),
            retrieved_at: at,
        }
    }
}

impl PartSource for Stub {
    fn id(&self) -> &str {
        "stub"
    }
    fn name(&self) -> &str {
        "Stub"
    }
    fn search(&self, _q: &str) -> Result<Vec<PartResult>, SourceError> {
        let n = self.hits.fetch_add(1, Ordering::SeqCst);
        Ok(vec![Self::part(1000 + n)])
    }
    fn fetch(&self, m: &str) -> Result<PartResult, SourceError> {
        Err(SourceError::NotFound(m.into()))
    }
    fn setup(&self) -> SetupCard {
        SetupCard {
            steps: vec![],
            portal_url: String::new(),
            fields: vec![],
        }
    }
    fn attribution(&self) -> Attribution {
        Attribution {
            text: String::new(),
            url: String::new(),
        }
    }
    fn cache_secs(&self) -> u64 {
        self.cache_secs
    }
    fn requests_per_sec(&self) -> f64 {
        self.rate
    }
}

#[test]
fn fresh_stale_and_expired_with_a_fake_clock() -> Result<(), SourceError> {
    let dir = TempDir::new().map_err(|e| bad(&e.to_string()))?;
    let source = Stub {
        hits: AtomicU64::new(0),
        cache_secs: 100,
        rate: 100.0,
    };
    let mut cache = PartSourceCache::open(dir.path()).with_default_rate(100.0);
    let out = cache.search(&source, "abc", 1_000)?;
    assert_eq!(out.age, AnswerAge::Fresh);
    assert_eq!(source.hits.load(Ordering::SeqCst), 1);

    let again = cache.search(&source, "abc", 1_050)?;
    assert_eq!(again.age, AnswerAge::Fresh);
    assert_eq!(source.hits.load(Ordering::SeqCst), 1, "memory still fresh");

    cache.set_offline(true);
    let stale = cache.search(&source, "abc", 1_150)?;
    assert_eq!(
        stale.age,
        AnswerAge::Stale {
            retrieved_at: 1_000
        }
    );
    assert_eq!(
        source.hits.load(Ordering::SeqCst),
        1,
        "offline keeps the dated answer"
    );

    cache.purge_expired(&source, 1_200)?;
    assert!(
        cache
            .read_disk(&source, &cache_key(source.id(), "abc"), 1_200)?
            .is_none(),
        "expired rows are deleted from disk"
    );
    assert!(cache.search(&source, "abc", 1_200).is_err());
    Ok(())
}

#[test]
fn online_re_asks_after_the_cache_window() -> Result<(), SourceError> {
    let dir = TempDir::new().map_err(|e| bad(&e.to_string()))?;
    let source = Stub {
        hits: AtomicU64::new(0),
        cache_secs: 100,
        rate: 100.0,
    };
    let mut cache = PartSourceCache::open(dir.path());
    cache.search(&source, "abc", 1_000)?;
    let refreshed = cache.search(&source, "abc", 1_150)?;
    assert_eq!(refreshed.age, AnswerAge::Fresh);
    assert_eq!(source.hits.load(Ordering::SeqCst), 2);
    Ok(())
}

#[test]
fn offline_serves_dated_values_without_error() -> Result<(), SourceError> {
    let dir = TempDir::new().map_err(|e| bad(&e.to_string()))?;
    let source = Stub {
        hits: AtomicU64::new(0),
        cache_secs: 60,
        rate: 10.0,
    };
    let mut cache = PartSourceCache::open(dir.path());
    cache.search(&source, "part", 500)?;
    cache.set_offline(true);
    let out = cache.search(&source, "part", 700)?;
    assert!(matches!(out.age, AnswerAge::Stale { retrieved_at: 500 }));
    assert!(!out.results.is_empty());
    Ok(())
}

#[test]
fn rate_limit_pause_surfaces_a_countdown() -> Result<(), SourceError> {
    let mut pacer = SourcePacer::new(1.0);
    pacer.pause("stub", 10, 5);
    assert_eq!(pacer.pause_remaining("stub", 12), Some(3));
    assert!(matches!(
        pacer.acquire("stub", 1.0, 12),
        Err(3) | Err(2) | Err(1)
    ));
    Ok(())
}

#[test]
fn pacer_never_exceeds_the_configured_rate() -> Result<(), SourceError> {
    let max_rps = 2.0;
    let mut pacer = SourcePacer::new(max_rps);
    let mut granted = Vec::new();
    let mut now = 0u64;
    for _ in 0..20 {
        loop {
            match pacer.acquire("s", max_rps, now) {
                Ok(()) => {
                    granted.push(now);
                    break;
                }
                Err(wait) => now += wait.max(1),
            }
        }
        now += 1;
    }
    for window in 1..=30u64 {
        let start = now.saturating_sub(window);
        let count = granted.iter().filter(|&&t| t >= start && t <= now).count() as f64;
        assert!(
            count <= max_rps * window as f64 + 1.0,
            "window {window}s saw {count} grants at t={now}"
        );
    }
    Ok(())
}

#[test]
fn purge_clears_disk_and_memory() -> Result<(), SourceError> {
    let dir = TempDir::new().map_err(|e| bad(&e.to_string()))?;
    let source = Stub {
        hits: AtomicU64::new(0),
        cache_secs: 100,
        rate: 10.0,
    };
    let mut cache = PartSourceCache::open(dir.path());
    cache.search(&source, "x", 1)?;
    cache.purge()?;
    cache.set_offline(true);
    assert!(cache.search(&source, "x", 2).is_err());
    Ok(())
}

/// A stub whose answers carry a chosen description.
struct Described {
    inner: Stub,
    description: String,
}

impl Described {
    fn new(description: String) -> Self {
        Self {
            inner: Stub {
                hits: AtomicU64::new(0),
                cache_secs: 1_000_000,
                rate: 100.0,
            },
            description,
        }
    }
}

impl PartSource for Described {
    fn id(&self) -> &str {
        self.inner.id()
    }
    fn name(&self) -> &str {
        self.inner.name()
    }
    fn search(&self, q: &str) -> Result<Vec<PartResult>, SourceError> {
        let mut found = self.inner.search(q)?;
        for part in &mut found {
            part.description = self.description.clone();
        }
        Ok(found)
    }
    fn fetch(&self, m: &str) -> Result<PartResult, SourceError> {
        self.inner.fetch(m)
    }
    fn setup(&self) -> SetupCard {
        self.inner.setup()
    }
    fn attribution(&self) -> Attribution {
        self.inner.attribution()
    }
    fn cache_secs(&self) -> u64 {
        self.inner.cache_secs()
    }
    fn requests_per_sec(&self) -> f64 {
        self.inner.requests_per_sec()
    }
}

fn stub(cache_secs: u64) -> Stub {
    Stub {
        hits: AtomicU64::new(0),
        cache_secs,
        rate: 100.0,
    }
}

fn session(files: &Arc<MemoryFiles>) -> PartSourceCache {
    PartSourceCache::with_files(files.clone()).with_default_rate(100.0)
}

fn row_name(query: &str) -> String {
    file_name(&cache_key("stub", query))
}

#[test]
fn a_stored_answer_is_a_hit_for_a_new_session_and_a_new_query_is_a_miss() -> Result<(), SourceError>
{
    let files = Arc::new(MemoryFiles::default());
    let source = stub(100);
    let first = session(&files).search(&source, "abc", 1_000)?;
    assert_eq!(first.age, AnswerAge::Fresh);
    assert_eq!(source.hits.load(Ordering::SeqCst), 1);

    let hit = session(&files).search(&source, "ABC ", 1_050)?;
    assert_eq!(hit.age, AnswerAge::Fresh);
    assert_eq!(hit.results, first.results);
    assert_eq!(
        hit.results[0].retrieved_at, 1_000,
        "the answer keeps its date"
    );
    assert_eq!(source.hits.load(Ordering::SeqCst), 1, "a hit asks nobody");

    session(&files).search(&source, "other", 1_050)?;
    assert_eq!(
        source.hits.load(Ordering::SeqCst),
        2,
        "a new query is a miss"
    );
    Ok(())
}

#[test]
fn an_old_answer_is_asked_again_online_and_kept_dated_offline() -> Result<(), SourceError> {
    let files = Arc::new(MemoryFiles::default());
    let source = stub(100);
    session(&files).search(&source, "abc", 1_000)?;

    let mut offline = session(&files);
    offline.set_offline(true);
    let kept = offline.search(&source, "abc", 5_000)?;
    assert_eq!(
        kept.age,
        AnswerAge::Stale {
            retrieved_at: 1_000
        }
    );
    assert_eq!(source.hits.load(Ordering::SeqCst), 1, "offline asks nobody");

    let fresh = session(&files).search(&source, "abc", 5_000)?;
    assert_eq!(fresh.age, AnswerAge::Fresh);
    assert_eq!(source.hits.load(Ordering::SeqCst), 2, "online asks again");
    assert_eq!(fresh.results[0].retrieved_at, 5_000);
    Ok(())
}

#[test]
fn offline_answers_from_the_store_before_any_connection_and_says_when_nothing_is_there()
-> Result<(), SourceError> {
    let files = Arc::new(MemoryFiles::default());
    let source = stub(100);
    let mut cache = session(&files);
    cache.set_offline(true);
    assert!(matches!(
        cache.search(&source, "abc", 10),
        Err(SourceError::Unavailable(_))
    ));
    assert_eq!(
        source.hits.load(Ordering::SeqCst),
        0,
        "no connection opened"
    );

    session(&files).search(&source, "abc", 20)?;
    let mut again = session(&files);
    again.set_offline(true);
    let out = again.search(&source, "abc", 30)?;
    assert_eq!(out.results[0].retrieved_at, 20);
    assert_eq!(source.hits.load(Ordering::SeqCst), 1);
    Ok(())
}

#[test]
fn a_corrupt_file_is_ignored_and_removed() -> Result<(), SourceError> {
    let files = Arc::new(MemoryFiles::default());
    let source = stub(100);
    for bad_text in ["garbage", "12\nnot json", "nope\n[]"] {
        files.write("stub", &row_name("abc"), bad_text)?;
        let mut offline = session(&files);
        offline.set_offline(true);
        assert!(offline.search(&source, "abc", 50).is_err());
        assert!(
            files.read("stub", &row_name("abc")).is_none(),
            "the corrupt file is gone"
        );
    }
    files.write("stub", &row_name("abc"), "garbage")?;
    let out = session(&files).search(&source, "abc", 60)?;
    assert_eq!(out.age, AnswerAge::Fresh);
    assert_eq!(
        source.hits.load(Ordering::SeqCst),
        1,
        "asked once, then stored"
    );
    assert!(files.read("stub", &row_name("abc")).is_some());
    Ok(())
}

#[test]
fn an_oversize_answer_is_returned_but_not_kept() -> Result<(), SourceError> {
    let files = Arc::new(MemoryFiles::default());
    let source = Described::new("x".repeat(MAX_BODY_BYTES + 1));
    let out = session(&files).search(&source, "abc", 10)?;
    assert_eq!(out.results.len(), 1);
    assert!(files.names("stub").is_empty(), "nothing was stored");
    Ok(())
}

#[test]
fn a_source_keeps_no_more_than_the_bound_and_loses_the_oldest() -> Result<(), SourceError> {
    let files = Arc::new(MemoryFiles::default());
    let source = stub(1_000_000);
    let mut cache = session(&files);
    let total = MAX_ANSWERS_PER_SOURCE + 5;
    for i in 0..total {
        cache.search(&source, &format!("query {i}"), 1_000 + 2 * i as u64)?;
    }
    assert_eq!(files.names("stub").len(), MAX_ANSWERS_PER_SOURCE);
    assert!(
        files.read("stub", &row_name("query 0")).is_none(),
        "the oldest went"
    );
    assert!(
        files
            .read("stub", &row_name(&format!("query {}", total - 1)))
            .is_some(),
        "the newest stayed"
    );
    Ok(())
}

#[test]
fn an_answer_that_names_a_key_is_never_stored() -> Result<(), SourceError> {
    for leak in [
        "see https://example.invalid/x?api_key=abc",
        "Client_Secret is abc",
        "Authorization: Bearer abc",
        "https://example.invalid/p?token=abc",
    ] {
        let files = Arc::new(MemoryFiles::default());
        let source = Described::new(leak.to_owned());
        let mut cache = session(&files);
        let out = cache.search(&source, "abc", 10)?;
        assert_eq!(out.results.len(), 1, "the person still sees the answer");
        assert!(files.names("stub").is_empty(), "{leak} was stored");
        cache.search(&source, "abc", 20)?;
        assert_eq!(
            source.inner.hits.load(Ordering::SeqCst),
            2,
            "nothing was kept in memory either"
        );
    }
    let files = Arc::new(MemoryFiles::default());
    let source = stub(100);
    session(&files).search(&source, "abc", 10)?;
    for name in files.names("stub") {
        let text = files.read("stub", &name).unwrap_or_default();
        assert!(!holds_a_key(&text), "a plain answer holds no key marker");
    }
    Ok(())
}

#[test]
fn purge_expired_removes_old_and_unreadable_rows_only() -> Result<(), SourceError> {
    let files = Arc::new(MemoryFiles::default());
    let source = stub(100);
    let mut cache = session(&files);
    cache.search(&source, "old", 1_000)?;
    cache.search(&source, "new", 1_090)?;
    files.write("stub", "broken.json", "garbage")?;
    cache.purge_expired(&source, 1_120)?;
    assert!(files.read("stub", &row_name("old")).is_none());
    assert!(files.read("stub", &row_name("new")).is_some());
    assert!(files.read("stub", "broken.json").is_none());
    Ok(())
}
