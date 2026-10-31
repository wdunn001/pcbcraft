//! Open-library sku import with optional 3D model fetch.
mod common;
use common::{Boxed, Recorded};
use pcbcraft_library::cad_cache::CadCache;
use pcbcraft_library::open_import::import_open;
use pcbcraft_library::open_library::OpenLibrarySource;

fn source(rec: &Recorded, cache: CadCache) -> OpenLibrarySource {
    OpenLibrarySource::new(Box::new(rec.clone()), cache).with_clock(Box::new(|| 1_000))
}

#[test]
fn open_library_3d_models_fetch_only_when_asked() -> Result<(), Boxed> {
    let tmp = tempfile::tempdir()?;
    let rec = Recorded::new()?;
    let s = source(&rec, CadCache::open(tmp.path()));
    let off = import_open(&s, "Device/R", false)?;
    assert!(off.get("model").is_none());
    assert_eq!(off["notices"].as_array().map(|a| a.len()), Some(2));
    let on = import_open(&s, "Device/R", true)?;
    assert!(on.get("model").is_some());
    assert_eq!(on["notices"].as_array().map(|a| a.len()), Some(3));
    let last = on["notices"]
        .as_array()
        .and_then(|a| a.last())
        .and_then(|v| v.as_str())
        .ok_or("missing model notice")?;
    assert!(
        last.contains("CC-BY-SA 4.0") && last.contains(".step"),
        "the model notice carries the licence and file"
    );
    Ok(())
}

#[test]
fn footprint_sku_skips_model_when_not_asked() -> Result<(), Boxed> {
    let tmp = tempfile::tempdir()?;
    let rec = Recorded::new()?;
    let s = source(&rec, CadCache::open(tmp.path()));
    let off = import_open(
        &s,
        "footprint:Resistor_SMD/R_0603_1608Metric",
        false,
    )?;
    assert!(off.get("model").is_none());
    assert_eq!(off["notices"].as_array().map(|a| a.len()), Some(1));
    Ok(())
}
