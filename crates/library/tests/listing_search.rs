//! One search across project, installed, sample, open-library and distributor
//! sources. Everything is replayed from recorded files; no test reaches a host.
use pcbcraft_library::cad_cache::CadCache;
use pcbcraft_library::listing::{ListingSource, PartListing, listing_from_result};
use pcbcraft_library::listing_search::{
    Found, ListingProvider, OpenLibraryProvider, Rank, SourceProvider, SymbolProvider, merge,
    search, stable_id,
};
use pcbcraft_library::open_library::{CardScope, OpenLibrarySource};
use pcbcraft_library::read_symbols;
use pcbcraft_library::sources::{FixtureSource, PartResult, SourceError};
use std::collections::BTreeSet;
use std::sync::Arc;
use std::sync::atomic::Ordering;

mod common;
use common::{Boxed, Recorded};

const SAMPLE: &str = include_str!("fixtures/sources/sample.json");
const ATTRIBUTES: &str = include_str!("fixtures/sources/sample-attributes.json");
const DEVICE: &str = include_str!("fixtures/device.kicad_sym");
const NOW: u64 = 1_790_003_600;

/// A second recorded shop that lists the sample resistor under other casing.
const SHOP: &str = r#"{"name":"Other shop","parts":[
 {"source":"Other shop","mpn":"sample-r-10k","manufacturer":"Example Components",
  "description":"","kind":"resistor","distributor":"Other shop","sku":"OS-1",
  "stock":900,"price_breaks":[],"lifecycle":"Active",
  "datasheet_url":"https://example.invalid/other.pdf","product_url":"",
  "retrieved_at":1790001800}]}"#;

fn sample() -> Result<Arc<FixtureSource>, SourceError> {
    Ok(Arc::new(
        FixtureSource::from_json(SAMPLE)?.with_attributes(ATTRIBUTES)?,
    ))
}

fn project() -> Result<SymbolProvider, Boxed> {
    Ok(SymbolProvider::new(
        ListingSource::Project,
        "This project",
        "Your design",
        read_symbols(DEVICE)?,
    ))
}

fn open_over(rec: &Recorded, cache: CadCache) -> Arc<OpenLibrarySource> {
    Arc::new(OpenLibrarySource::new(Box::new(rec.clone()), cache).with_clock(Box::new(|| NOW - 60)))
}

fn stamp(mpn: &str, source: ListingSource, id: &str, manufacturer: &str) -> Found {
    let part = PartResult {
        source: id.into(),
        mpn: mpn.into(),
        manufacturer: manufacturer.into(),
        description: String::new(),
        kind: String::new(),
        distributor: String::new(),
        sku: format!("{id}-{mpn}"),
        stock: 0,
        price_breaks: vec![],
        lifecycle: String::new(),
        datasheet_url: String::new(),
        product_url: String::new(),
        retrieved_at: 100,
    };
    let mut listing: PartListing = listing_from_result(&part, &Default::default());
    listing.source = source;
    Found {
        listing,
        licence: String::new(),
        fetched_at: Some(100),
    }
}

/// Small deterministic generator so the property test needs no extra crate.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

#[test]
fn merge_and_dedupe_hold_for_any_mix_of_sources() {
    let kinds = [
        ListingSource::Project,
        ListingSource::Installed,
        ListingSource::Fixture,
        ListingSource::OpenLibrary,
        ListingSource::Distributor,
    ];
    let mpns = ["LM358", "lm358", " LM358 ", "NE555", "R", "", "STM32F4"];
    let makers = ["", "TI", "Example"];
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    for round in 0..300 {
        let n = 1 + rng.below(14);
        let mut input: Vec<Found> = (0..n)
            .map(|i| {
                let id = format!("lib{}", rng.below(3));
                let mut f = stamp(
                    mpns[rng.below(mpns.len())],
                    kinds[rng.below(kinds.len())],
                    &id,
                    makers[rng.below(makers.len())],
                );
                f.listing.source_ref = format!("ref{i}");
                f
            })
            .collect();
        let query = ["lm358", "R", "so-8 lm", "zzz"][rng.below(4)];
        let hits = merge(query, NOW, input.clone());

        // Every input is either a hit or recorded as a repeat of one.
        let mut covered: BTreeSet<String> = BTreeSet::new();
        for h in &hits {
            covered.insert(h.id.clone());
            covered.extend(h.also_in.iter().cloned());
        }
        let wanted: BTreeSet<String> = input.iter().map(|f| stable_id(&f.listing)).collect();
        assert_eq!(
            covered, wanted,
            "round {round}: nothing lost, nothing invented"
        );

        // Hit ids are unique and the list is ranked.
        let ids: BTreeSet<&String> = hits.iter().map(|h| &h.id).collect();
        assert_eq!(ids.len(), hits.len(), "round {round}: ids are unique");
        assert!(
            hits.windows(2).all(|w| w[0].rank <= w[1].rank),
            "round {round}: ranked best first"
        );

        // A repeat is only ever another source's copy of the same part number.
        for h in &hits {
            let origin = |id: &str| {
                id.rsplit_once('/')
                    .map(|x| x.0)
                    .unwrap_or_default()
                    .to_owned()
            };
            let mut origins: Vec<String> = h.also_in.iter().map(|i| origin(i)).collect();
            origins.push(origin(&h.id));
            let distinct: BTreeSet<&String> = origins.iter().collect();
            assert_eq!(
                distinct.len(),
                origins.len(),
                "round {round}: a source never repeats itself"
            );
        }

        // The answer does not depend on the order the sources replied in.
        input.reverse();
        assert_eq!(merge(query, NOW, input.clone()), hits, "round {round}");
    }
}

#[test]
fn one_part_number_in_two_shops_is_one_hit_with_the_gaps_filled() -> Result<(), Boxed> {
    let shop_b = Arc::new(FixtureSource::from_json(SHOP)?);
    let a = SourceProvider::new(ListingSource::Distributor, sample()?);
    let b = SourceProvider::new(ListingSource::Distributor, shop_b);
    let out = search(&[&a, &b], "sample-r-10k", NOW, 20)?;
    assert_eq!(out.hits.len(), 1, "{:?}", out.hits);
    let hit = &out.hits[0];
    assert_eq!(hit.rank, Rank::ExactMpn);
    assert_eq!(hit.also_in.len(), 1);
    assert_eq!(
        hit.listing.source_id, "Other shop",
        "equal sources tie on id, so order never matters"
    );
    assert_eq!(
        hit.listing.package_key, "chip-0603",
        "kept from the richer record"
    );
    Ok(())
}

#[test]
fn symbol_names_are_not_part_numbers_so_they_never_merge() -> Result<(), Boxed> {
    let installed = SymbolProvider::new(
        ListingSource::Installed,
        "Device.kicad_sym",
        "CC-BY-SA 4.0",
        read_symbols(DEVICE)?,
    );
    let out = search(&[&project()?, &installed], "R", NOW, 50)?;
    let rs: Vec<_> = out.hits.iter().filter(|h| h.listing.mpn == "R").collect();
    assert_eq!(rs.len(), 2, "project R and installed R both stay");
    Ok(())
}

#[test]
fn ranking_is_exact_part_number_then_package_then_name() -> Result<(), Boxed> {
    let sample = SourceProvider::new(ListingSource::Fixture, sample()?);
    let out = search(&[&sample], "SAMPLE-R-10K", NOW, 20)?;
    assert_eq!(out.hits[0].listing.mpn, "SAMPLE-R-10K");
    assert_eq!(out.hits[0].rank, Rank::ExactMpn);

    // Package: "0603" is the resistor's package; the capacitor is 0402 metric,
    // which is a different package, so it does not match.
    let by_package = merge("0603", NOW, sample.search("sample", NOW)?);
    let rank_of = |mpn: &str| {
        by_package
            .iter()
            .find(|h| h.listing.mpn == mpn)
            .map(|h| h.rank)
    };
    assert_eq!(rank_of("SAMPLE-R-10K"), Some(Rank::Package));
    assert_ne!(rank_of("SAMPLE-C-100N"), Some(Rank::Package));

    // Name beats nothing but sits after exact and package.
    let mixed = vec![
        stamp("XR-100", ListingSource::Distributor, "Shop", "Acme"),
        stamp("R", ListingSource::Distributor, "Shop", "Acme"),
        {
            let mut f = stamp("ZZ-1", ListingSource::Distributor, "Shop", "Acme");
            f.listing.package_key = "chip-0603".into();
            f
        },
        stamp("R-7", ListingSource::Distributor, "Shop", "Acme"),
    ];
    let order = |query: &str| -> Vec<String> {
        merge(query, NOW, mixed.clone())
            .into_iter()
            .map(|h| h.listing.mpn)
            .collect()
    };
    assert_eq!(
        order("r"),
        ["R", "R-7", "XR-100", "ZZ-1"],
        "exact, prefix, contains"
    );
    assert_eq!(order("0603")[0], "ZZ-1", "a package match comes first");
    let both = merge("r", NOW, mixed);
    assert_eq!(both[0].rank, Rank::ExactMpn);
    Ok(())
}

#[test]
fn every_hit_carries_source_licence_age_and_a_stable_id() -> Result<(), Boxed> {
    let rec = Recorded::new()?;
    let open = OpenLibraryProvider::new(open_over(&rec, CadCache::in_memory()), CardScope::Symbols);
    let sample = SourceProvider::new(ListingSource::Fixture, sample()?);
    let p = project()?;
    let providers: [&dyn ListingProvider; 3] = [&p, &sample, &open];
    let first = search(&providers, "resistor", NOW, 50)?;
    let again = search(&providers, "resistor", NOW, 50)?;
    assert_eq!(
        first.hits.iter().map(|h| &h.id).collect::<Vec<_>>(),
        again.hits.iter().map(|h| &h.id).collect::<Vec<_>>(),
        "ids are stable between searches"
    );
    let kinds: Vec<ListingSource> = first.hits.iter().map(|h| h.listing.source).collect();
    assert!(kinds.contains(&ListingSource::Fixture));
    assert!(kinds.contains(&ListingSource::OpenLibrary));
    let sample_hit = first
        .hits
        .iter()
        .find(|h| h.listing.mpn == "SAMPLE-R-10K")
        .ok_or("sample resistor missing")?;
    assert_eq!(sample_hit.age_secs, Some(NOW - 1_790_000_000));
    assert!(!sample_hit.licence.is_empty());
    let lib = first
        .hits
        .iter()
        .find(|h| h.listing.source == ListingSource::OpenLibrary)
        .ok_or("open library hit missing")?;
    assert_eq!(lib.age_secs, Some(60));
    assert!(lib.licence.contains("CC-BY-SA"), "{}", lib.licence);
    assert_eq!(lib.listing.stock, None, "a library card is not an offer");
    Ok(())
}

#[test]
fn offline_with_nothing_saved_still_lists_local_and_sample_parts() -> Result<(), Boxed> {
    let rec = Recorded::new()?;
    rec.offline.store(true, Ordering::SeqCst);
    let open = OpenLibraryProvider::new(open_over(&rec, CadCache::in_memory()), CardScope::Both);
    let sample = SourceProvider::new(ListingSource::Fixture, sample()?);
    let p = project()?;
    let out = search(&[&p, &sample, &open], "resistor", NOW, 50)?;
    assert_eq!(out.failures.len(), 1);
    assert_eq!(out.failures[0].source, "Open library");
    assert!(out.failures[0].offline);
    assert!(out.failures[0].message.contains("no connection"));
    assert!(
        out.hits
            .iter()
            .any(|h| h.listing.source == ListingSource::Project)
    );
    assert!(
        out.hits
            .iter()
            .any(|h| h.listing.source == ListingSource::Fixture)
    );
    Ok(())
}

#[test]
fn offline_after_a_visit_still_lists_what_was_saved() -> Result<(), Boxed> {
    let tmp = tempfile::tempdir()?;
    let rec = Recorded::new()?;
    let online = OpenLibraryProvider::new(
        open_over(&rec, CadCache::open(tmp.path())),
        CardScope::Symbols,
    );
    let before = search(&[&online], "resistor", NOW, 50)?;
    assert!(before.failures.is_empty());
    assert!(!before.hits.is_empty());

    rec.offline.store(true, Ordering::SeqCst);
    let asked = rec.count();
    let later = OpenLibraryProvider::new(
        open_over(&rec, CadCache::open(tmp.path())),
        CardScope::Symbols,
    );
    let after = search(&[&later], "resistor", NOW, 50)?;
    assert_eq!(rec.count(), asked, "no request left the machine");
    assert!(after.failures.is_empty(), "{:?}", after.failures);
    assert_eq!(
        after.hits.iter().map(|h| &h.id).collect::<Vec<_>>(),
        before.hits.iter().map(|h| &h.id).collect::<Vec<_>>()
    );
    Ok(())
}

#[test]
fn a_bad_query_is_refused_before_any_source_is_asked() -> Result<(), Boxed> {
    let p = project()?;
    assert!(matches!(
        search(&[&p], "   ", NOW, 10),
        Err(SourceError::Invalid(_))
    ));
    Ok(())
}

#[test]
fn the_limit_cuts_the_list_and_total_known_says_how_much_was_cut() -> Result<(), Boxed> {
    let p = project()?;
    let out = search(&[&p], "R", NOW, 1)?;
    assert_eq!(out.hits.len(), 1);
    assert!(out.total_known >= 1);
    Ok(())
}
