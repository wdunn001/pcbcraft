//! Fetching a whole part through a recorded source and shaping it for the engine.
mod common;
use common::{Boxed, Recorded};
use pcbcraft_library::cad::{CadKind, CadRef};
use pcbcraft_library::cad_cache::CadCache;
use pcbcraft_library::online_import::{footprint_ref_from_field, import_part};
use pcbcraft_library::open_library::OpenLibrarySource;

fn source(rec: &Recorded, cache: CadCache) -> OpenLibrarySource {
    OpenLibrarySource::new(Box::new(rec.clone()), cache).with_clock(Box::new(|| 1_000))
}

fn refs() -> Result<(CadRef, CadRef, CadRef), Boxed> {
    Ok((
        CadRef::new(CadKind::Symbol, "Device", "R")?,
        CadRef::new(CadKind::Footprint, "Resistor_SMD", "R_0603_1608Metric")?,
        CadRef::new(CadKind::Model, "Resistor_SMD", "R_0603_1608Metric")?,
    ))
}

#[test]
fn import_part_returns_definitions_and_a_notice_from_recorded_answers() -> Result<(), Boxed> {
    let tmp = tempfile::tempdir()?;
    let rec = Recorded::new()?;
    let s = source(&rec, CadCache::open(tmp.path()));
    let (sym, fp, model) = refs()?;
    let part = import_part(&s, &sym, Some(&fp), Some(&model))?;
    assert_eq!(part.symbol.name, "R");
    assert_eq!(part.symbol.pins.len(), 2);
    let footprint = part.footprint.as_ref().ok_or("no footprint")?;
    assert_eq!(footprint.pads.len(), 2);
    assert_eq!(part.notices.len(), 3, "one notice per fetched file");
    assert!(part.notices[0].contains("Device.kicad_symdir/R.kicad_sym"));
    assert!(part.notices[1].contains("Resistor_SMD.pretty/R_0603_1608Metric.kicad_mod"));
    assert!(part.notices[2].contains("Resistor_SMD.3dshapes/R_0603_1608Metric.step"));
    assert!(part.notices.iter().all(|n| n.contains("CC-BY-SA 4.0")));
    Ok(())
}

#[test]
fn import_part_replaces_the_footprint_model_reference_with_the_cached_path() -> Result<(), Boxed> {
    let tmp = tempfile::tempdir()?;
    let rec = Recorded::new()?;
    let s = source(&rec, CadCache::open(tmp.path()));
    let (sym, fp, model) = refs()?;
    let part = import_part(&s, &sym, Some(&fp), Some(&model))?;
    let path = part.model_path.clone().ok_or("no cached model path")?;
    assert!(path.starts_with(tmp.path()), "{}", path.display());
    let bytes = std::fs::read(&path)?;
    assert!(bytes.starts_with(b"ISO-10303-21;"));
    let footprint = part.footprint.as_ref().ok_or("no footprint")?;
    let m = footprint
        .model
        .as_ref()
        .ok_or("footprint lost its model reference")?;
    assert_eq!(m.path, path.to_string_lossy());
    assert!(
        !m.path.contains("${"),
        "no unresolved variable is left in the path"
    );
    // The engine gets the model as its own parameter, not buried in the footprint.
    let params = part.to_params();
    assert_eq!(params["model"]["path"], path.to_string_lossy().as_ref());
    assert!(params["footprint"].get("model").is_none());
    assert_eq!(params["notices"].as_array().map(Vec::len), Some(3));
    assert_eq!(params["symbol"]["name"], "R");
    Ok(())
}

#[test]
fn import_part_without_a_model_drops_the_dangling_reference() -> Result<(), Boxed> {
    let rec = Recorded::new()?;
    let s = source(&rec, CadCache::in_memory());
    let (sym, fp, _) = refs()?;
    let part = import_part(&s, &sym, Some(&fp), None)?;
    let footprint = part.footprint.as_ref().ok_or("no footprint")?;
    assert!(footprint.model.is_none(), "a path to nowhere is not kept");
    assert!(part.to_params().get("model").is_none());
    assert_eq!(part.notices.len(), 2);
    Ok(())
}

#[test]
fn import_part_symbol_only_and_wrong_kinds() -> Result<(), Boxed> {
    let rec = Recorded::new()?;
    let s = source(&rec, CadCache::in_memory());
    let (sym, fp, model) = refs()?;
    let only = import_part(&s, &sym, None, None)?;
    assert!(only.footprint.is_none());
    assert!(only.to_params().get("footprint").is_none());
    assert!(
        import_part(&s, &fp, None, None).is_err(),
        "a footprint is not a symbol"
    );
    assert!(
        import_part(&s, &sym, None, Some(&model)).is_err(),
        "a model needs a footprint"
    );
    let missing = CadRef::new(CadKind::Symbol, "Device", "NoSuchPart")?;
    assert!(import_part(&s, &missing, None, None).is_err());
    Ok(())
}

#[test]
fn a_symbols_footprint_field_names_a_footprint_reference() -> Result<(), Boxed> {
    let r = footprint_ref_from_field("Resistor_SMD:R_0603_1608Metric").ok_or("not parsed")?;
    assert_eq!(
        (r.library.as_str(), r.name.as_str()),
        ("Resistor_SMD", "R_0603_1608Metric")
    );
    assert_eq!(r.kind, CadKind::Footprint);
    assert!(footprint_ref_from_field("").is_none());
    assert!(footprint_ref_from_field("NoColonHere").is_none());
    assert!(footprint_ref_from_field("../x:y").is_none());
    Ok(())
}

fn lqfp64() -> Result<CadRef, Boxed> {
    Ok(CadRef::new(
        CadKind::Footprint,
        "Package_QFP",
        "LQFP-64_10x10mm_P0.5mm",
    )?)
}

#[test]
fn a_footprint_alone_gets_a_generic_symbol_with_one_pin_per_pad() -> Result<(), Boxed> {
    use pcbcraft_library::online_import::import_footprint_only;
    let tmp = tempfile::tempdir()?;
    let rec = Recorded::new()?;
    let s = source(&rec, CadCache::open(tmp.path()));
    let part = import_footprint_only(&s, &lqfp64()?, None)?;
    let fp = part.footprint.as_ref().ok_or("no footprint")?;
    assert_eq!(fp.pads.len(), 64);
    assert_eq!(part.symbol.pins.len(), 64);
    assert_eq!(
        part.symbol.footprint, fp.name,
        "the symbol links to the footprint"
    );
    assert_eq!(part.symbol.reference, "U");
    let mut pins: Vec<&str> = part.symbol.pins.iter().map(|p| p.number.as_str()).collect();
    let mut pads: Vec<&str> = fp.pads.iter().map(|p| p.number.as_str()).collect();
    pins.sort_unstable();
    pads.sort_unstable();
    assert_eq!(pins, pads, "pin numbers match pad numbers exactly");
    assert!(
        fp.model.is_none(),
        "no model file was fetched, so no model reference is kept"
    );
    assert_eq!(part.notices.len(), 1);
    assert!(part.notices[0].contains("Package_QFP.pretty/LQFP-64_10x10mm_P0.5mm.kicad_mod"));
    // The engine takes it as the same parameters as any online part.
    let params = part.to_params();
    assert_eq!(
        params["footprint"]["pads"].as_array().map(Vec::len),
        Some(64)
    );
    assert_eq!(params["symbol"]["pins"].as_array().map(Vec::len), Some(64));
    Ok(())
}

#[test]
fn the_generic_symbol_folds_repeated_pad_numbers_and_skips_unnumbered_pads() -> Result<(), Boxed> {
    use pcbcraft_library::online_import::generic_symbol_for;
    let tmp = tempfile::tempdir()?;
    let rec = Recorded::new()?;
    let s = source(&rec, CadCache::open(tmp.path()));
    let mut fp = import_part_footprint(&s)?;
    let mut thermal = fp.pads[0].clone();
    thermal.number = "65".into();
    fp.pads.push(thermal.clone());
    fp.pads.push(thermal);
    let mut hole = fp.pads[0].clone();
    hole.number = String::new();
    fp.pads.push(hole);
    let symbol = generic_symbol_for(&fp)?;
    assert_eq!(
        symbol.pins.len(),
        65,
        "64 pads and one thermal number cut in two"
    );
    symbol.validate().map_err(|e| e.to_string())?;
    fp.pads.clear();
    assert!(generic_symbol_for(&fp).is_err(), "no pads, no symbol");
    Ok(())
}

fn import_part_footprint(
    s: &OpenLibrarySource,
) -> Result<pcbcraft_library::FootprintDefinition, Boxed> {
    let file = pcbcraft_library::cad::CadSource::fetch_footprint(s, &lqfp64()?)?;
    Ok(pcbcraft_library::read_footprint(file.text()?)?)
}
