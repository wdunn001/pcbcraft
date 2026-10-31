use pcbcraft_library::{
    generate_footprint, LandPatternDensity, LandPatternParams, PackageKey,
};

#[test]
fn soic8_package_key_from_generated_footprint() -> Result<(), String> {
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
    let fp = generate_footprint("soic", LandPatternDensity::Nominal, &p)?;
    let key = PackageKey::from_footprint(&fp)?;
    assert_eq!(key.pin_count, 8);
    assert!((key.pitch_mm - 1.27).abs() < 0.1);
    Ok(())
}

#[test]
fn qfn33_includes_exposed_pad_in_key() -> Result<(), String> {
    let mut p = LandPatternParams {
        name: "QFN33".into(),
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
    let key = PackageKey::from_footprint(&fp)?;
    assert!(key.exposed_pad_mm > 0.0);
    Ok(())
}

#[test]
fn chip_0603_fixture_key() -> Result<(), String> {
    let text = include_str!("fixtures/R_0805_2012Metric.kicad_mod");
    let fp = pcbcraft_library::read_footprint(text).map_err(|e| e.to_string())?;
    let key = PackageKey::from_footprint(&fp)?;
    assert_eq!(key.pin_count, 2);
    Ok(())
}

#[test]
fn dip8_fixture_key() -> Result<(), String> {
    let text = include_str!("fixtures/DIP-8_W7.62mm.kicad_mod");
    let fp = pcbcraft_library::read_footprint(text).map_err(|e| e.to_string())?;
    let key = PackageKey::from_footprint(&fp)?;
    assert_eq!(key.pin_count, 8);
    Ok(())
}
