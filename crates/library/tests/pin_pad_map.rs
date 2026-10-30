use pcbcraft_geom::Point;
use pcbcraft_library::listing::Category;
use pcbcraft_library::{
    FootprintDefinition, FootprintPad, Graphic, PadRole, PartCatalogFields, SymbolDefinition,
    SymbolPin, UnitGraphic, auto_fill, replacement_check, validate_part, validate_pin_pad_map,
};
use pcbcraft_library::{LandPatternDensity, LandPatternParams, generate_footprint};

fn pin(number: &str, name: &str, ty: &str) -> SymbolPin {
    SymbolPin {
        number: number.into(),
        name: name.into(),
        position: Point::new(0.0, 0.0),
        angle: 0.0,
        length: 2.54,
        electrical_type: ty.into(),
        unit: 1,
        body_style: 1,
        graphical_style: "line".into(),
        visible: true,
    }
}

fn qfn_symbol_and_footprint() -> Result<(SymbolDefinition, FootprintDefinition), String> {
    let mut p = LandPatternParams {
        name: "QFN32_test".into(),
        description: String::new(),
        keywords: String::new(),
        pitch: 0.5,
        pin_count: 32,
        body_length: 5.0,
        body_width: 5.0,
        row_spacing: 0.0,
        lead_width: 0.0,
        lead_diameter: 0.0,
        grid_rows: 0,
        grid_cols: 0,
        thermal_size: 2.5,
    };
    let fp = generate_footprint("qfn", LandPatternDensity::Nominal, &p)?;
    let mut pins: Vec<SymbolPin> = (1..=32)
        .map(|n| pin(&n.to_string(), &n.to_string(), "passive"))
        .collect();
    pins.push(pin("GND", "GND", "passive"));
    let symbol = SymbolDefinition {
        name: "QFN32".into(),
        reference: "U".into(),
        value: "QFN32".into(),
        description: String::new(),
        keywords: String::new(),
        footprint: fp.name.clone(),
        set: String::new(),
        pins,
        graphics: vec![UnitGraphic {
            unit: 1,
            body_style: 1,
            primitive: Graphic::Rectangle {
                start: Point::new(-1.0, -1.0),
                end: Point::new(1.0, 1.0),
                width: 0.1,
                fill: "none".into(),
                layer: String::new(),
            },
        }],
        extends: None,
        raw_source: String::new(),
        unsupported: Vec::new(),
    };
    let mut fp = fp;
    let gnd = fp.pads[0].clone();
    fp.pads.push(FootprintPad {
        number: "GND".into(),
        ..gnd.clone()
    });
    fp.pads.push(FootprintPad {
        number: "GND".into(),
        ..gnd
    });
    Ok((symbol, fp))
}

#[test]
fn qfn_auto_fill_groups_repeated_ground_pads_under_one_pin() -> Result<(), String> {
    let (symbol, footprint) = qfn_symbol_and_footprint()?;
    let map = auto_fill(&symbol, &footprint);
    let gnd_row = map
        .rows
        .iter()
        .find(|r| r.pin == "GND")
        .ok_or("expected a GND pin row")?;
    assert_eq!(
        gnd_row.pads.len(),
        2,
        "one GND pin should claim both GND pads: {:?}",
        gnd_row.pads
    );
    Ok(())
}

#[test]
fn qfn_missing_pad_names_the_pad() -> Result<(), String> {
    let (symbol, mut footprint) = qfn_symbol_and_footprint()?;
    let map = auto_fill(&symbol, &footprint);
    let findings = validate_pin_pad_map(&symbol, &footprint, &map);
    assert!(findings.is_empty(), "{findings:?}");

    footprint.pads.push(FootprintPad {
        number: "99".into(),
        pad_type: "smd".into(),
        shape: "rect".into(),
        position: Point::new(4.0, 4.0),
        rotation: 0.0,
        size: Point::new(0.3, 0.6),
        layers: vec!["F.Cu".into()],
        drill: None,
        roundrect_ratio: None,
        chamfer: None,
        polygon: Vec::new(),
        raw_source: String::new(),
    });
    let map = auto_fill(&symbol, &footprint);
    let findings = validate_pin_pad_map(&symbol, &footprint, &map);
    assert!(
        findings.iter().any(|f| f.pad.as_deref() == Some("99")),
        "unclaimed pad 99: {findings:?}"
    );
    Ok(())
}

#[test]
fn bga_alphanumeric_pad_numbers_map_by_symbol_pin() -> Result<(), String> {
    let mut p = LandPatternParams {
        name: "BGA16".into(),
        description: String::new(),
        keywords: String::new(),
        pitch: 1.0,
        pin_count: 16,
        body_length: 4.0,
        body_width: 4.0,
        row_spacing: 0.0,
        lead_width: 0.0,
        lead_diameter: 0.0,
        grid_rows: 4,
        grid_cols: 4,
        thermal_size: 0.0,
    };
    let mut fp = generate_footprint("bga", LandPatternDensity::Nominal, &p)?;
    for (i, pad) in fp.pads.iter_mut().enumerate() {
        pad.number = format!("A{}", i + 1);
    }
    let pins: Vec<SymbolPin> = fp
        .pads
        .iter()
        .map(|pad| pin(&pad.number, &pad.number, "passive"))
        .collect();
    let symbol = SymbolDefinition {
        name: "BGA16".into(),
        reference: "U".into(),
        value: "BGA16".into(),
        description: String::new(),
        keywords: String::new(),
        footprint: fp.name.clone(),
        set: String::new(),
        pins,
        graphics: vec![],
        extends: None,
        raw_source: String::new(),
        unsupported: Vec::new(),
    };
    let map = auto_fill(&symbol, &fp);
    assert_eq!(map.rows.len(), 16);
    assert!(validate_pin_pad_map(&symbol, &fp, &map).is_empty());
    Ok(())
}

#[test]
fn unclaimed_thermal_pad_is_ok_when_marked_thermal() -> Result<(), String> {
    let (symbol, footprint) = qfn_symbol_and_footprint()?;
    let labels = pcbcraft_library::footprint_pad_labels(&footprint);
    let ep_label = footprint
        .pads
        .iter()
        .enumerate()
        .find(|(_, p)| p.number == "EP")
        .map(|(i, _)| labels[i].clone())
        .ok_or("QFN fixture should include EP")?;
    let mut map = auto_fill(&symbol, &footprint);
    for row in &mut map.rows {
        row.pads.retain(|p| p != &ep_label);
    }
    map.set_pad_role(&ep_label, PadRole::Thermal);
    let findings = validate_pin_pad_map(&symbol, &footprint, &map);
    assert!(
        !findings.iter().any(|f| f.pad.as_deref() == Some(ep_label.as_str())),
        "thermal EP should not count as unclaimed: {findings:?}"
    );
    Ok(())
}

#[test]
fn unclaimed_thermal_pad_fails_when_left_electrical() -> Result<(), String> {
    let (symbol, footprint) = qfn_symbol_and_footprint()?;
    let labels = pcbcraft_library::footprint_pad_labels(&footprint);
    let ep_label = footprint
        .pads
        .iter()
        .enumerate()
        .find(|(_, p)| p.number == "EP")
        .map(|(i, _)| labels[i].clone())
        .ok_or("QFN fixture should include EP")?;
    let mut map = auto_fill(&symbol, &footprint);
    for row in &mut map.rows {
        row.pads.retain(|p| p != &ep_label);
    }
    let findings = validate_pin_pad_map(&symbol, &footprint, &map);
    assert!(
        findings
            .iter()
            .any(|f| f.code == "pad_unclaimed" && f.pad.as_deref() == Some(ep_label.as_str())),
        "{findings:?}"
    );
    Ok(())
}

#[test]
fn remapping_by_pick_moves_pad_between_pins() -> Result<(), String> {
    let (symbol, footprint) = qfn_symbol_and_footprint()?;
    let mut map = auto_fill(&symbol, &footprint);
    map.assign_pad_to_pin("1", "2");
    assert!(map.pads_for_pin("1").is_some_and(|p| p.contains(&"2".to_string())));
    assert!(map.pads_for_pin("2").is_none_or(|p| !p.contains(&"2".to_string())));
    Ok(())
}

#[test]
fn replacement_check_requires_matching_pad_sets_per_pin() -> Result<(), String> {
    let (symbol, footprint) = qfn_symbol_and_footprint()?;
    let reference = auto_fill(&symbol, &footprint);
    let mut candidate = reference.clone();
    candidate.assign_pad_to_pin("3", "4");
    let result = replacement_check(&reference, &candidate);
    assert!(!result.drop_in);
    assert!(
        result
            .findings
            .iter()
            .any(|f| f.pin.as_deref() == Some("3") || f.pin.as_deref() == Some("4"))
    );
    Ok(())
}

#[test]
fn validate_part_names_pin_and_pad_in_findings() -> Result<(), String> {
    let (symbol, footprint) = qfn_symbol_and_footprint()?;
    let map = auto_fill(&symbol, &footprint);
    let (hx, hy) = pcbcraft_library::courtyard_half_extents(&footprint).ok_or("courtyard")?;
    let pkg = pcbcraft_library::listing::package_key("QFN-32 5x5");
    let fields = PartCatalogFields {
        manufacturer: "Test".into(),
        mpn: "MPN-1".into(),
        datasheet_url: Some("https://example.test/ds.pdf".into()),
        package_key: pkg.key.clone(),
        package_text: pkg.text.clone(),
        category: Category::IcMcu,
        parameters: vec![("package_key".into(), pkg.key.clone())],
        body_half_mm: Some([hx * 0.5, hy * 0.5, 1.0]),
    };
    let ok = validate_part(&symbol, &footprint, &map, &fields);
    if !ok.verified {
        assert!(
            ok.pin_pad_ok,
            "catalog may fail on package name; pin pad should pass: {:?}",
            ok.findings
        );
    }

    let mut bad_map = map;
    let labels = pcbcraft_library::footprint_pad_labels(&footprint);
    let ep_label = footprint
        .pads
        .iter()
        .enumerate()
        .find(|(_, p)| p.number == "EP")
        .map(|(i, _)| labels[i].clone())
        .ok_or("EP")?;
    for row in &mut bad_map.rows {
        row.pads.retain(|p| p != &ep_label);
    }
    let bad = validate_part(&symbol, &footprint, &bad_map, &fields);
    assert!(
        bad.findings.iter().any(|f| f.pin.is_some() || f.pad.is_some()),
        "{:?}",
        bad.findings
    );
    Ok(())
}
