//! A newcomer uses a companion CAD app to edit 3D part models and reimports them.
//!
//! Writes a footprint body to a companion CAD project, reads it back,
//! and verifies the part has the companion body id set for stable alignment.

use pcbcraft_engine::Session;
use pcbcraft_library::{
    LandPatternDensity, LandPatternParams, PinKind, PinSpec, generate_footprint,
    part_from_library_bundle,
};
use serde_json::json;
use std::fs;

fn build_part() -> Result<
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
    let (mut part, _) = part_from_library_bundle(
        1,
        symbol.clone(),
        footprint.clone(),
        "TestMfg",
        "COMPANION-TEST",
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
    part.id = 0;
    Ok((part, symbol, footprint))
}

#[test]
fn a_newcomer_companion_model_roundtrip() -> Result<(), String> {
    let (part, symbol, footprint) = build_part()?;
    let part_json = serde_json::to_value(&part).map_err(|e| format!("serialize: {e}"))?;

    let mut s = Session::default();
    s.project.library.symbols.push(symbol);
    s.project.library.footprints.push(footprint);

    // Add the part to the project library
    let r = s.execute("part.add", json!({ "part": part_json }), None);
    assert!(r.is_ok(), "part.add failed: {r:?}");
    let part_id = r.unwrap()["result"]["id"].as_u64().unwrap();

    // Write the footprint body to a companion CAD project
    let temp_dir = tempfile::tempdir().map_err(|e| format!("tempdir: {e}"))?;
    let dir_path = temp_dir.path().to_string_lossy().to_string();
    let write_r = s.execute(
        "part.write_footprint_body",
        json!({
            "id": part_id,
            "directory": dir_path,
            "base_name": "test_body",
            "body_id": "stable-body-1",
            "height_mm": 1.5
        }),
        None,
    );
    assert!(write_r.is_ok(), "write_footprint_body failed: {write_r:?}");
    let write_result = write_r.unwrap()["result"].clone();
    assert_eq!(
        write_result["body_id"], "stable-body-1",
        "body_id should be preserved"
    );

    // Verify the companion files were written
    let project_path = write_result["project_path"].as_str().unwrap();
    let step_path = write_result["step_path"].as_str().unwrap();
    assert!(
        fs::metadata(project_path).is_ok(),
        "companion project file should exist"
    );
    assert!(fs::metadata(step_path).is_ok(), "STEP sidecar should exist");

    // Verify the part now has body_companion_id set
    let get_r = s.execute("part.get", json!({ "id": part_id }), None);
    assert!(get_r.is_ok(), "part.get failed");
    let part_state = get_r.unwrap()["result"]["part"].clone();
    assert_eq!(
        part_state["body_companion_id"], "stable-body-1",
        "part should have body_companion_id after writing companion body"
    );

    // Read the companion body back (simulating reimport)
    let read_r = s.execute(
        "part.read_companion_body",
        json!({
            "id": part_id,
            "project_path": project_path,
            "body_id": "stable-body-1"
        }),
        None,
    );
    assert!(read_r.is_ok(), "read_companion_body failed: {read_r:?}");

    // Verify the part still has the stable body id after reimport
    let get_r2 = s.execute("part.get", json!({ "id": part_id }), None);
    assert!(get_r2.is_ok());
    let part_state2 = get_r2.unwrap()["result"]["part"].clone();
    assert_eq!(
        part_state2["body_companion_id"], "stable-body-1",
        "body_companion_id should be stable across reimport"
    );
    assert!(
        part_state2["model3d"]["path"]
            .as_str()
            .is_some_and(|p| p.ends_with(".solidcraft")),
        "model path should point to companion project"
    );

    Ok(())
}
