use super::*;
use pcbcraft_library::sources::{Attribution, PartResult, PartSource, SetupCard, SourceError};
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
