//! Replays recorded answers from tests/fixtures/open_library/. No test here
//! reaches a live host.
use pcbcraft_library::cad::{CadKind, CadRef, CadSource};
use pcbcraft_library::cad_cache::CadCache;
use pcbcraft_library::open_library::{CardScope, OpenLibrarySource};
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

fn graphics_on<'a>(
    fp: &'a pcbcraft_library::FootprintDefinition,
    layer: &str,
) -> Vec<&'a pcbcraft_library::Graphic> {
    fp.graphics
        .iter()
        .filter(|g| match g {
            pcbcraft_library::Graphic::Line { layer: l, .. }
            | pcbcraft_library::Graphic::Rectangle { layer: l, .. }
            | pcbcraft_library::Graphic::Circle { layer: l, .. }
            | pcbcraft_library::Graphic::Arc { layer: l, .. }
            | pcbcraft_library::Graphic::Polyline { layer: l, .. }
            | pcbcraft_library::Graphic::Text { layer: l, .. } => l == layer,
        })
        .collect()
}

#[test]
fn a_package_name_is_found_however_its_dashes_and_case_are_typed() -> Result<(), Boxed> {
    for query in ["LQFP-64", "lqfp64", "lqfp 64", "LQFP_64", "Lqfp-64 10x10"] {
        let rec = Recorded::new()?;
        let found = CadSource::search(&source(&rec), CadKind::Footprint, query)?;
        let first = found
            .first()
            .ok_or_else(|| format!("nothing for {query}"))?;
        assert_eq!(
            (first.library.as_str(), first.name.as_str()),
            ("Package_QFP", "LQFP-64_10x10mm_P0.5mm"),
            "{query}: the plain 64-pin package comes first, got {found:?}"
        );
        assert!(
            found.iter().all(|r| r.name.starts_with("LQFP-64")),
            "{query}: only 64-pin packages match: {found:?}"
        );
    }
    Ok(())
}

#[test]
fn a_footprint_search_reads_folder_listings_and_never_a_file() -> Result<(), Boxed> {
    let rec = Recorded::new()?;
    CadSource::search(&source(&rec), CadKind::Footprint, "LQFP-64")?;
    let urls = rec.urls();
    assert!(
        urls.iter().any(|u| u.contains("path=Package_QFP.pretty")),
        "the package family folder is listed: {urls:?}"
    );
    assert!(
        !urls.iter().any(|u| u.contains("/files/")),
        "no file is fetched by a search: {urls:?}"
    );
    assert!(
        urls.iter()
            .all(|u| u.starts_with("https://gitlab.com/api/v4/projects/")),
        "only the allowlisted host is asked"
    );
    Ok(())
}

#[test]
fn a_second_footprint_search_is_answered_from_the_saved_listings() -> Result<(), Boxed> {
    let tmp = tempfile::tempdir()?;
    let rec = Recorded::new()?;
    let first = CadSource::search(
        &source_over(CadCache::open(tmp.path()), &rec),
        CadKind::Footprint,
        "LQFP-64",
    )?;
    let asked = rec.count();
    assert!(asked > 0);
    rec.offline.store(true, Ordering::SeqCst);
    let again = CadSource::search(
        &source_over(CadCache::open(tmp.path()), &rec),
        CadKind::Footprint,
        "lqfp64",
    )?;
    assert_eq!(again, first);
    assert_eq!(
        rec.count(),
        asked,
        "no request was sent for the second search"
    );
    Ok(())
}

#[test]
fn footprint_cards_have_their_own_sku_and_say_footprint() -> Result<(), Boxed> {
    let rec = Recorded::new()?;
    let cards = source(&rec).search_cards(CardScope::Footprints, "LQFP-64")?;
    let card = cards.first().ok_or("no card")?;
    assert_eq!(card.mpn, "LQFP-64_10x10mm_P0.5mm");
    assert_eq!(card.sku, "footprint:Package_QFP/LQFP-64_10x10mm_P0.5mm");
    assert!(card.description.starts_with("Footprint in Package_QFP"));
    assert!(card.description.contains("CC-BY-SA 4.0"));
    assert!(
        card.product_url
            .contains("Package_QFP.pretty/LQFP-64_10x10mm_P0.5mm.kicad_mod")
    );
    Ok(())
}

#[test]
fn nothing_matching_says_nothing_instead_of_failing() -> Result<(), Boxed> {
    let rec = Recorded::new()?;
    let found = CadSource::search(&source(&rec), CadKind::Footprint, "LQFP-64 zzzz");
    // The recording holds one folder; the others are "not recorded" and must not hide the empty answer.
    match found {
        Ok(v) => assert!(v.is_empty()),
        Err(SourceError::Unavailable(_)) => {}
        Err(e) => return Err(format!("{e:?}").into()),
    }
    Ok(())
}

#[test]
fn the_lqfp64_footprint_imports_through_the_cache_and_parses_in_full() -> Result<(), Boxed> {
    use pcbcraft_library::{Graphic, PadShape};
    let rec = Recorded::new()?;
    let s = source(&rec);
    let r = CadSource::search(&s, CadKind::Footprint, "LQFP-64")?
        .into_iter()
        .next()
        .ok_or("no match")?;
    let file = s.fetch_footprint(&r)?;
    assert!(
        file.provenance
            .notice()
            .contains("Package_QFP.pretty/LQFP-64_10x10mm_P0.5mm.kicad_mod")
    );
    assert_eq!(file.provenance.license_name, "CC-BY-SA 4.0");
    let text = file.text()?;
    assert!(text.contains("(version 20260206)"), "the newest layout");
    let fp = read_footprint(text)?;
    fp.validate().map_err(|e| e.to_string())?;
    assert_eq!(fp.name, "LQFP-64_10x10mm_P0.5mm");
    assert!(fp.unsupported.is_empty(), "{:?}", fp.unsupported);

    // 64 pads, numbered 1..=64, all roundrect with the recorded 0.25 ratio.
    assert_eq!(fp.pads.len(), 64);
    for (i, pad) in fp.pads.iter().enumerate() {
        assert_eq!(pad.number, (i + 1).to_string());
        assert_eq!(pad.pad_type, "smd");
        assert_eq!(pad.layers, ["F.Cu", "F.Mask", "F.Paste"]);
        match pad.shape_kind() {
            PadShape::RoundRect { ratio } => {
                assert!((ratio - 0.25).abs() < 1e-9, "pad {}", pad.number)
            }
            other => return Err(format!("pad {} is {other:?}", pad.number).into()),
        }
    }

    // Four rows of 16. Side rows are 1.55 x 0.3 (long side across the row), top and bottom the other way.
    let rows: [(std::ops::Range<usize>, f64, bool); 4] = [
        (0..16, -5.675, true),   // left, pads 1-16, x fixed
        (16..32, 5.675, false),  // bottom, pads 17-32, y fixed
        (32..48, 5.675, true),   // right, pads 33-48
        (48..64, -5.675, false), // top, pads 49-64
    ];
    for (range, edge, fixed_x) in rows {
        let row = &fp.pads[range];
        for pad in row {
            let (along, across) = if fixed_x {
                (pad.position.y, pad.position.x)
            } else {
                (pad.position.x, pad.position.y)
            };
            assert!(
                (across - edge).abs() < 1e-9,
                "pad {} sits on the edge",
                pad.number
            );
            assert!(along.abs() <= 3.75 + 1e-9);
            let (long, short) = if fixed_x {
                (pad.size.x, pad.size.y)
            } else {
                (pad.size.y, pad.size.x)
            };
            assert!(
                (long - 1.55).abs() < 1e-9 && (short - 0.3).abs() < 1e-9,
                "pad {} size {:?}",
                pad.number,
                pad.size
            );
        }
        for pair in row.windows(2) {
            let (a, b) = if fixed_x {
                (pair[0].position.y, pair[1].position.y)
            } else {
                (pair[0].position.x, pair[1].position.x)
            };
            assert!(
                ((b - a).abs() - 0.5).abs() < 1e-9,
                "pitch 0.5 mm between pads {} and {}",
                pair[0].number,
                pair[1].number
            );
        }
    }
    // Pad 1 is the top of the left row; its neighbour 0.5 mm below.
    assert_eq!(
        (fp.pads[0].position.x, fp.pads[0].position.y),
        (-5.675, -3.75)
    );
    assert!(
        fp.pads.iter().all(|p| p.rotation == 0.0),
        "the file turns no pad; the rows differ by size"
    );

    // Artwork: silkscreen corner marks, courtyard, fabrication outline.
    let silk = graphics_on(&fp, "F.SilkS");
    assert_eq!(silk.len(), 9, "eight corner lines and the pin 1 triangle");
    assert!(
        silk.iter()
            .any(|g| matches!(g, Graphic::Polyline { points, .. } if points.len() == 3)),
        "the pin 1 mark"
    );
    let courtyard = graphics_on(&fp, "F.CrtYd");
    assert_eq!(courtyard.len(), 20);
    let fab = graphics_on(&fp, "F.Fab");
    assert_eq!(fab.len(), 2, "the body outline and the reference text");
    assert!(
        fab.iter().any(
            |g| matches!(g, Graphic::Polyline { points, fill, width, .. }
            if points.len() == 5 && fill == "none" && (*width - 0.1).abs() < 1e-9)
        ),
        "the body outline with its cut corner"
    );
    assert!(
        fab.iter()
            .any(|g| matches!(g, Graphic::Text { text, .. } if text == "${REFERENCE}"))
    );
    assert!(
        courtyard
            .iter()
            .all(|g| matches!(g, Graphic::Line { width, .. } if (*width - 0.05).abs() < 1e-9))
    );

    // 3D model.
    let model = fp.model.as_ref().ok_or("no model reference")?;
    assert_eq!(
        model.path,
        "${KICAD10_3DMODEL_DIR}/Package_QFP.3dshapes/LQFP-64_10x10mm_P0.5mm.step"
    );
    assert_eq!(model.offset, [0.0; 3]);
    assert_eq!(model.scale, [1.0; 3]);
    assert_eq!(model.rotate, [0.0; 3]);
    Ok(())
}
