//! Replays recorded answers from tests/fixtures/open_library/. No test here
//! reaches a live host.
use pcbcraft_library::cad::{CadKind, CadRef, CadSource};
use pcbcraft_library::cad_cache::CadCache;
use pcbcraft_library::open_library::OpenLibrarySource;
use pcbcraft_library::sources::{PartSource, SourceError};
use pcbcraft_library::{read_footprint, read_symbols};
use std::sync::atomic::Ordering;

mod common;
use common::{Boxed, Recorded};

fn source_over(cache: CadCache, rec: &Recorded) -> OpenLibrarySource {
    OpenLibrarySource::new(Box::new(rec.clone()), cache).with_clock(Box::new(|| 1_000))
}

fn source(rec: &Recorded) -> OpenLibrarySource {
    source_over(CadCache::in_memory(), rec)
}

#[test]
fn lists_symbol_libraries_from_the_recorded_root_tree() -> Result<(), Boxed> {
    let rec = Recorded::new()?;
    let libs = source(&rec).list_libraries(CadKind::Symbol)?;
    assert!(libs.contains(&"Device".to_owned()));
    assert!(libs.contains(&"Connector_Generic".to_owned()));
    assert!(
        libs.iter().all(|l| !l.ends_with("symdir")),
        "folder suffix is hidden"
    );
    let mut sorted = libs.clone();
    sorted.sort();
    assert_eq!(libs, sorted);
    let feet = source(&rec).list_libraries(CadKind::Footprint)?;
    assert!(feet.contains(&"Resistor_SMD".to_owned()));
    Ok(())
}

#[test]
fn pages_through_a_folder_listing_until_a_short_page() -> Result<(), Boxed> {
    let rec = Recorded::new()?;
    source(&rec).list_libraries(CadKind::Symbol)?;
    let urls = rec.urls();
    assert!(
        urls.iter().any(|u| u.contains("page=3")),
        "reads the third page"
    );
    assert!(
        !urls.iter().any(|u| u.contains("page=4")),
        "stops at the short page"
    );
    assert_eq!(
        urls.len(),
        4,
        "one revision lookup and three pages: {urls:?}"
    );
    Ok(())
}

#[test]
fn searching_resistor_returns_device_cards_with_source_and_licence() -> Result<(), Boxed> {
    let rec = Recorded::new()?;
    let s = source(&rec);
    let cards = PartSource::search(&s, "resistor")?;
    let r = cards.iter().find(|c| c.mpn == "R").ok_or("no card for R")?;
    assert_eq!(r.source, "Open library");
    assert!(r.description.contains("Device"), "{}", r.description);
    assert!(
        r.description.contains("CC-BY-SA 4.0"),
        "licence is on the card: {}",
        r.description
    );
    assert!(r.product_url.contains("Device.kicad_symdir/R.kicad_sym"));
    assert!(cards.iter().any(|c| c.mpn == "R_Small"));
    assert!(
        !cards.iter().any(|c| c.mpn == "LED"),
        "other parts do not match"
    );
    Ok(())
}

#[test]
fn a_library_name_in_the_words_picks_that_library() -> Result<(), Boxed> {
    let rec = Recorded::new()?;
    let s = source(&rec);
    let found = CadSource::search(&s, CadKind::Footprint, "Resistor_SMD 0603")?;
    assert!(found.iter().any(|r| r.name == "R_0603_1608Metric"));
    assert!(
        found
            .iter()
            .all(|r| r.library == "Resistor_SMD" && r.name.contains("0603"))
    );
    assert!(
        found.iter().all(|r| !r.revision.is_empty()),
        "each carries its file revision"
    );
    Ok(())
}

#[test]
fn the_newest_symbol_file_version_parses_with_the_existing_reader() -> Result<(), Boxed> {
    let rec = Recorded::new()?;
    let s = source(&rec);
    let r = CadRef::new(CadKind::Symbol, "Device", "R")?;
    let file = s.fetch_symbol(&r)?;
    let text = file.text()?;
    assert!(
        text.contains("(version 20251024)"),
        "the recording is the newest layout"
    );
    let symbols = read_symbols(text)?;
    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "R");
    assert_eq!(symbols[0].pins.len(), 2);
    assert!(
        symbols[0].unsupported.is_empty(),
        "{:?}",
        symbols[0].unsupported
    );
    symbols[0].validate().map_err(|e| e.to_string())?;
    assert!(
        file.provenance
            .notice()
            .contains("Device.kicad_symdir/R.kicad_sym")
    );
    assert_eq!(file.provenance.license_name, "CC-BY-SA 4.0");
    Ok(())
}

#[test]
fn the_newest_footprint_file_version_parses_with_the_existing_reader() -> Result<(), Boxed> {
    let rec = Recorded::new()?;
    let s = source(&rec);
    let r = CadRef::new(CadKind::Footprint, "Resistor_SMD", "R_0603_1608Metric")?;
    let file = s.fetch_footprint(&r)?;
    let text = file.text()?;
    assert!(
        text.contains("(version 20260206)"),
        "the recording is the newest layout"
    );
    let fp = read_footprint(text)?;
    assert_eq!(fp.name, "R_0603_1608Metric");
    assert_eq!(fp.pads.len(), 2);
    assert!(fp.model.is_some(), "the model reference is read");
    fp.validate().map_err(|e| e.to_string())?;
    Ok(())
}

#[test]
fn fetch_model_returns_and_caches_step_bytes() -> Result<(), Boxed> {
    let rec = Recorded::new()?;
    let s = source(&rec);
    let r = CadRef::new(CadKind::Model, "Resistor_SMD", "R_0603_1608Metric")?;
    let file = s.fetch_model(&r)?;
    assert!(file.bytes.starts_with(b"ISO-10303-21;"));
    assert_eq!(file.provenance.kind, CadKind::Model);
    assert!(
        file.provenance
            .upstream_path
            .ends_with("R_0603_1608Metric.step")
    );
    let again = s.fetch_model(&r)?;
    assert_eq!(again.bytes, file.bytes);
    Ok(())
}

#[test]
fn a_second_fetch_is_served_from_disk_with_the_transport_off() -> Result<(), Boxed> {
    let tmp = tempfile::tempdir()?;
    let rec = Recorded::new()?;
    let r = CadRef::new(CadKind::Symbol, "Device", "R")?;
    let first = source_over(CadCache::open(tmp.path()), &rec).fetch_symbol(&r)?;
    let asked = rec.count();
    assert!(asked >= 3, "the first fetch asked the host");

    // A new session over the same folder, with the network gone.
    rec.offline.store(true, Ordering::SeqCst);
    let again = source_over(CadCache::open(tmp.path()), &rec).fetch_symbol(&r)?;
    assert_eq!(again.bytes, first.bytes);
    assert_eq!(again.provenance, first.provenance);
    assert_eq!(rec.count(), asked, "no request was sent");
    Ok(())
}

#[test]
fn a_stale_listing_still_answers_when_the_host_cannot_be_reached() -> Result<(), Boxed> {
    let tmp = tempfile::tempdir()?;
    let rec = Recorded::new()?;
    let first = OpenLibrarySource::new(Box::new(rec.clone()), CadCache::open(tmp.path()))
        .with_clock(Box::new(|| 1_000));
    let libs = first.list_libraries(CadKind::Symbol)?;
    rec.offline.store(true, Ordering::SeqCst);
    let later = OpenLibrarySource::new(Box::new(rec.clone()), CadCache::open(tmp.path()))
        .with_clock(Box::new(|| 1_000 + 30 * 24 * 3600));
    assert_eq!(later.list_libraries(CadKind::Symbol)?, libs);
    Ok(())
}

#[test]
fn nothing_saved_and_no_connection_says_so_in_plain_words() -> Result<(), Boxed> {
    let rec = Recorded::new()?;
    rec.offline.store(true, Ordering::SeqCst);
    let err = source(&rec)
        .list_libraries(CadKind::Symbol)
        .err()
        .ok_or("expected an error")?;
    match err {
        SourceError::Unavailable(m) => assert!(m.contains("no connection"), "{m}"),
        other => return Err(format!("wrong error: {other:?}").into()),
    }
    Ok(())
}

#[test]
fn a_rate_limited_answer_is_a_pause() -> Result<(), Boxed> {
    let rec = Recorded::new()?;
    rec.limited.store(30, Ordering::SeqCst);
    let err = PartSource::search(&source(&rec), "resistor")
        .err()
        .ok_or("expected an error")?;
    assert_eq!(
        err,
        SourceError::RateLimited {
            retry_after_secs: 30
        }
    );
    Ok(())
}

#[test]
fn open_library_stands_in_the_part_source_slot() -> Result<(), Boxed> {
    let rec = Recorded::new()?;
    let slot: Box<dyn PartSource> = Box::new(source(&rec));
    assert_eq!(slot.name(), "Open library");
    assert!(slot.attribution().text.contains("CC-BY-SA 4.0"));
    assert!(!slot.setup().steps.is_empty());
    let card = slot.fetch("R")?;
    assert_eq!(card.mpn, "R");
    assert!(
        card.description.contains("Resistor"),
        "the card now says what the symbol file says: {}",
        card.description
    );
    assert!(card.description.contains("CC-BY-SA 4.0"));
    Ok(())
}

#[test]
fn a_reference_that_climbs_out_of_its_library_is_refused() {
    let rec = match Recorded::new() {
        Ok(r) => r,
        Err(_) => return,
    };
    let sneaky = CadRef {
        kind: CadKind::Symbol,
        library: "../..".into(),
        name: "R".into(),
        revision: String::new(),
    };
    assert!(CadSource::fetch(&source(&rec), &sneaky).is_err());
    assert_eq!(rec.count(), 0, "nothing was sent");
}

#[test]
fn an_unknown_part_is_not_found() -> Result<(), Boxed> {
    let rec = Recorded::new()?;
    let r = CadRef::new(CadKind::Symbol, "Device", "NoSuchPart")?;
    assert!(matches!(
        source(&rec).fetch_symbol(&r),
        Err(SourceError::NotFound(_))
    ));
    Ok(())
}
