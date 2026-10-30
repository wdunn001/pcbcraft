//! Samtec user-download import (issue 785 step 1).
use pcbcraft_engine::Session;
use serde_json::json;
use std::io::Write;

#[test]
fn import_manufacturer_file_records_user_download_provenance() -> Result<(), Box<dyn std::error::Error>>
{
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("TSW-103-07-G-S.kicad_mod");
    let fixture = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../library/tests/fixtures/R_0805_2012Metric.kicad_mod"
    );
    std::fs::copy(fixture, &path)?;

    let mut s = Session::default();
    let out = s.execute(
        "library.import_manufacturer_file",
        json!({
            "manufacturer":"samtec",
            "mpn":"TSW-103-07-G-S",
            "path": path.to_str().ok_or("path is not text")?,
        }),
        None,
    )?;
    assert_eq!(out["result"]["kind"], "footprint");
    assert!(out["result"]["notice"]
        .as_str()
        .is_some_and(|n| n.contains("not redistributed")));
    assert_eq!(s.project.library.notices.len(), 1);
    assert!(s
        .project
        .library
        .notices[0]
        .contains("downloaded by the user"));
    Ok(())
}

#[test]
fn import_manufacturer_step_attaches_model_path() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let fp_path = dir.path().join("pkg.kicad_mod");
    let fixture = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../library/tests/fixtures/R_0805_2012Metric.kicad_mod"
    );
    std::fs::copy(fixture, &fp_path)?;
    let step_path = dir.path().join("part.step");
    let mut step = std::fs::File::create(&step_path)?;
    write!(step, "ISO-10303-21;\n")?;

    let mut s = Session::default();
    s.execute(
        "library.import_manufacturer_file",
        json!({
            "manufacturer":"samtec",
            "mpn":"TSW-103-07-G-S",
            "path": fp_path.to_str().ok_or("path is not text")?,
        }),
        None,
    )?;
    let out = s.execute(
        "library.import_manufacturer_file",
        json!({
            "manufacturer":"samtec",
            "mpn":"TSW-103-07-G-S",
            "path": step_path.to_str().ok_or("path is not text")?,
            "footprint":"R_0805_2012Metric",
        }),
        None,
    )?;
    assert_eq!(out["result"]["kind"], "model");
    let fp = s
        .project
        .library
        .footprints
        .iter()
        .find(|f| f.name == "R_0805_2012Metric")
        .ok_or("footprint missing")?;
    assert_eq!(
        fp.model.as_ref().map(|m| m.path.as_str()),
        Some(step_path.to_str().ok_or("path is not text")?)
    );
    Ok(())
}
