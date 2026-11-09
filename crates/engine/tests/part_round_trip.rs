//! A part added to a project survives save and reopen.
//!
//! Builds an SOIC-8 part, adds it via `part.add`, serialises the project,
//! reopens it with `pcbcraft_io::read_project`, and asserts the part is
//! still present with the correct MPN.
//!
//! Rules:
//! - Part id must be 0 when passed to `part.add` (the engine assigns it).
//! - Use the same SOIC-8 construction as `crates/library/tests/part_verify.rs`.
//! - Use only crates already dependencies of `pcbcraft-engine`.
//!
//! Dependencies:
//! - `crates/engine/src/lib.rs` - `Session::default()`, `execute()`
//! - `crates/library/src/lib.rs` - `LandPatternParams`, `generate_footprint`
//! - `crates/library/src/part_import.rs` - `part_from_library_bundle`
//! - `crates/io/src/migrate.rs` - `read_project()`

use pcbcraft_engine::Session;
use pcbcraft_library::{
    LandPatternDensity, LandPatternParams, Model3dRef, ParamBody, PinKind, PinSpec,
    generate_footprint, part_from_library_bundle,
};
use serde_json::json;

fn build_soic8_part() -> Result<
    (
        pcbcraft_library::Part,
        pcbcraft_library::SymbolDefinition,
        pcbcraft_library::FootprintDefinition,
    ),
    String,
> {
    let p = LandPatternParams {
        name: "SOIC8".into(),
        description: String::new(),
        keywords: String::new(),
        pitch: 1.27,
        pin_count: 8,
        body_length: 4.9,
        body_width: 3.9,
        row_spacing: 0.0,
        lead_width: 0.0,
        lead_diameter: 0.0,
        grid_rows: 0,
        grid_cols: 0,
        thermal_size: 0.0,
    };
    let footprint = generate_footprint("soic", LandPatternDensity::Nominal, &p)?;
    let symbol = pcbcraft_library::SymbolDefinition {
        name: "U8".into(),
        reference: "U".into(),
        value: "TEST".into(),
        description: String::new(),
        keywords: String::new(),
        footprint: footprint.name.clone(),
        set: String::new(),
        pins: (1..=8)
            .map(|n| pcbcraft_library::SymbolPin {
                number: n.to_string(),
                name: format!("P{n}"),
                position: pcbcraft_geom::Point::new(0.0, 0.0),
                angle: 0.0,
                length: 2.54,
                electrical_type: "passive".into(),
                unit: 1,
                body_style: 1,
                graphical_style: "line".into(),
                visible: true,
            })
            .collect(),
        graphics: vec![],
        extends: None,
        raw_source: String::new(),
        unsupported: vec![],
        retained: vec![],
    };
    // Use id 1 for validation, then reset to 0 for part.add
    let (mut part, _) = part_from_library_bundle(
        1,
        symbol.clone(),
        footprint.clone(),
        "TestMfg",
        "TEST-SOIC8",
        "https://example.com/ds.pdf",
        vec![],
    )?;
    part.mpn_source = pcbcraft_library::MpnSource::Library;
    part.pins = (1..=8)
        .map(|n| PinSpec {
            number: n.to_string(),
            name: format!("P{n}"),
            pads: vec![n.to_string()],
            kind: PinKind::Passive,
            library_type: "passive".into(),
            unit: None,
        })
        .collect();
    part.model3d = Some(Model3dRef::Parametric {
        body: ParamBody::from_key(&part.package_key, "ic"),
    });
    part.id = 0;
    Ok((part, symbol, footprint))
}

fn part_json(part: &pcbcraft_library::Part) -> Result<serde_json::Value, String> {
    serde_json::to_value(part).map_err(|e| format!("serialise part: {e}"))
}

#[test]
fn a_part_survives_a_project_save_and_open() -> Result<(), String> {
    let (part, symbol, footprint) = build_soic8_part()?;

    let mut s = Session::default();
    s.project.library.symbols.push(symbol);
    s.project.library.footprints.push(footprint);

    let result = s.execute("part.add", json!({ "part": part_json(&part)? }), None);
    assert!(result.is_ok(), "part.add failed: {result:?}");
    assert_eq!(s.project.library.parts.len(), 1);

    let saved = serde_json::to_string(&s.project).map_err(|e| format!("serialise failed: {e}"))?;
    let reopened = pcbcraft_io::read_project(&saved)?;

    assert_eq!(
        reopened.library.parts.len(),
        1,
        "part count changed after round-trip"
    );
    assert_eq!(
        reopened.library.parts[0].mpn, "TEST-SOIC8",
        "mpn changed after round-trip"
    );
    Ok(())
}

#[test]
fn part_add_with_a_duplicate_mpn_is_refused() -> Result<(), String> {
    let (part, symbol, footprint) = build_soic8_part()?;

    let mut s = Session::default();
    s.project.library.symbols.push(symbol);
    s.project.library.footprints.push(footprint);

    let result = s.execute("part.add", json!({ "part": part_json(&part)? }), None);
    assert!(result.is_ok(), "first part.add failed: {result:?}");
    assert_eq!(s.project.library.parts.len(), 1);
    let rev = s.revision;

    let result = s.execute("part.add", json!({ "part": part_json(&part)? }), None);
    assert!(result.is_err(), "duplicate part.add should fail");
    let err = match result.err() {
        Some(e) => e.to_string(),
        None => return Err("no error from duplicate part.add".into()),
    };
    assert!(err.contains("duplicate part MPN"), "wrong error: {err}");
    assert_eq!(s.project.library.parts.len(), 1, "part count changed");
    assert_eq!(s.revision, rev, "revision changed");
    Ok(())
}
