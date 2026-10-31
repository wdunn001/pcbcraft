use pcbcraft_library::{
    generate_footprint, part_from_library_bundle, verify_part, LandPatternDensity,
    LandPatternParams, Model3dRef, ParamBody, PinKind, PinSpec, VerifyContext,
};

#[test]
fn soic8_verify_happy_path() -> Result<(), String> {
    let mut p = LandPatternParams {
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
    let mut symbol = pcbcraft_library::SymbolDefinition {
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
    };
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
    let ctx = VerifyContext {
        symbol: Some(&symbol),
        footprint: Some(&footprint),
    };
    let (v, messages) = verify_part(&part, ctx);
    assert!(v.checks.symbol_pads_match, "messages: {messages:?}");
    assert!(v.checks.model_fits, "messages: {messages:?}");
    assert!(v.checks.datasheet_ok);
    Ok(())
}

#[test]
fn missing_pad_fails_symbol_match() -> Result<(), String> {
    let mut p = LandPatternParams {
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
    let mut footprint = generate_footprint("soic", LandPatternDensity::Nominal, &p)?;
    footprint.pads.retain(|pad| pad.number != "8");
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
    };
    let (part, _) = part_from_library_bundle(
        1,
        symbol.clone(),
        footprint.clone(),
        "TestMfg",
        "TEST-SOIC8-BAD",
        "https://example.com/ds.pdf",
        vec![],
    )?;
    let ctx = VerifyContext {
        symbol: Some(&symbol),
        footprint: Some(&footprint),
    };
    let (v, messages) = verify_part(&part, ctx);
    assert!(!v.checks.symbol_pads_match);
    assert!(messages.iter().any(|m| m.contains('8')));
    Ok(())
}

#[test]
fn wrl_unit_scale_builds_mesh() -> Result<(), String> {
    let wrl = r#"#VRML V2.0 utf8
Shape {
  appearance Appearance { material Material { } }
  geometry IndexedFaceSet {
    coord Coordinate { point [ 0 0 0, 10 0 0, 0 10 0 ] }
    coordIndex [ 0 1 2 -1 ]
  }
}
"#;
    let mesh = pcbcraft_library::read_wrl_mesh(wrl, [1.0, 1.0, 1.0])?;
    assert!(!mesh.vertices.is_empty());
    assert!(!mesh.triangles.is_empty());
    Ok(())
}
