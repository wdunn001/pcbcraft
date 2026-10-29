use pcbcraft_library::sources::{
    FixtureSource, PartResult, PartSource, ResponseCache, SourceError,
};
use std::sync::atomic::{AtomicU64, Ordering};

const SAMPLE: &str = include_str!("fixtures/sources/sample.json");

fn source() -> Result<FixtureSource, SourceError> {
    FixtureSource::from_json(SAMPLE)
}

#[test]
fn fixture_search_returns_stock_and_price() -> Result<(), SourceError> {
    let found = source()?.search("sample-r-10k")?;
    assert_eq!(found.len(), 1, "a part number finds exactly that part");
    let part = &found[0];
    assert_eq!(part.mpn, "SAMPLE-R-10K");
    assert!(part.stock > 0, "the card can say the part is in stock");
    assert!(
        !part.price_breaks.is_empty(),
        "the card shows a price break"
    );
    Ok(())
}

struct Counting(AtomicU64);
impl PartSource for Counting {
    fn id(&self) -> &str {
        "counting"
    }
    fn name(&self) -> &str {
        "Counting"
    }
    fn search(&self, _q: &str) -> Result<Vec<PartResult>, SourceError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(vec![])
    }
    fn fetch(&self, m: &str) -> Result<PartResult, SourceError> {
        Err(SourceError::NotFound(m.into()))
    }
    fn setup(&self) -> pcbcraft_library::sources::SetupCard {
        empty_card()
    }
    fn attribution(&self) -> pcbcraft_library::sources::Attribution {
        pcbcraft_library::sources::Attribution {
            text: String::new(),
            url: String::new(),
        }
    }
    fn cache_secs(&self) -> u64 {
        100
    }
}
fn empty_card() -> pcbcraft_library::sources::SetupCard {
    pcbcraft_library::sources::SetupCard {
        steps: vec![],
        portal_url: String::new(),
        fields: vec![],
    }
}

#[test]
fn cache_expires_after_its_window() -> Result<(), SourceError> {
    let s = Counting(AtomicU64::new(0));
    let mut cache = ResponseCache::default();
    cache.search(&s, "abc", 1000)?;
    cache.search(&s, "ABC ", 1099)?;
    assert_eq!(s.0.load(Ordering::SeqCst), 1, "a fresh answer is reused");
    cache.search(&s, "abc", 1100)?;
    assert_eq!(
        s.0.load(Ordering::SeqCst),
        2,
        "an old answer is asked again"
    );
    cache.purge();
    assert!(cache.is_empty());
    Ok(())
}

#[test]
fn empty_and_oversized_queries_are_refused() -> Result<(), SourceError> {
    let s = source()?;
    assert!(s.search("   ").is_err());
    assert!(s.search(&"x".repeat(200)).is_err());
    Ok(())
}
