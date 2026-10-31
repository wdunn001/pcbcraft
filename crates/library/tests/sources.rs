use pcbcraft_library::sources::{
    DISTRIBUTOR_KEY_HINT, FixtureSource, PartResult, PartSource, ResponseCache, SourceError,
    distributor_network_allowed, join_distributor_into_cards, merge_distributor_fields,
    part_match_key,
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

fn cad_card(mpn: &str) -> PartResult {
    PartResult {
        source: "Open library".into(),
        mpn: mpn.into(),
        manufacturer: String::new(),
        description: "Symbol in Device. Licence: CC-BY-SA 4.0.".into(),
        kind: String::new(),
        distributor: String::new(),
        sku: format!("Device/{mpn}"),
        stock: 0,
        price_breaks: vec![],
        lifecycle: String::new(),
        datasheet_url: String::new(),
        product_url: "https://example.invalid/cad".into(),
        retrieved_at: 1,
    }
}

#[test]
fn join_matches_by_part_number_ignoring_case() -> Result<(), SourceError> {
    let distributor = source()?;
    let dist = distributor.fetch("sample-r-10k")?;
    assert_eq!(
        part_match_key("SAMPLE-R-10K"),
        part_match_key("sample-r-10k")
    );
    let merged = merge_distributor_fields(cad_card("SAMPLE-R-10K"), &dist);
    assert_eq!(merged.stock, dist.stock);
    assert!(!merged.price_breaks.is_empty());
    assert_eq!(merged.distributor, dist.distributor);
    assert_eq!(
        merged.sku, "Device/SAMPLE-R-10K",
        "the open-library sku stays"
    );
    assert_eq!(
        merged.description, "Symbol in Device. Licence: CC-BY-SA 4.0.",
        "the licence line stays on the card"
    );
    let mut cache = ResponseCache::default();
    let joined = join_distributor_into_cards(
        vec![cad_card("sample-r-10k")],
        Some(&distributor),
        &mut cache,
        2000,
        true,
    )?;
    assert_eq!(joined.len(), 1);
    assert!(joined[0].stock > 0);
    let no_key = join_distributor_into_cards(
        vec![cad_card("sample-r-10k")],
        Some(&distributor),
        &mut cache,
        2000,
        false,
    )?;
    assert_eq!(no_key[0].stock, 0, "without a key the card stays CAD-only");
    assert_eq!(DISTRIBUTOR_KEY_HINT, "Add a key to see stock");
    assert!(
        !distributor_network_allowed(),
        "live distributor network stays off by default"
    );
    Ok(())
}
