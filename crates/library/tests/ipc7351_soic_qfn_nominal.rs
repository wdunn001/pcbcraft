use pcbcraft_library::{
    generate_footprint, generate_footprint_from_key, LandPatternDensity, LandPatternParams,
    PackageFamily, PackageKey,
};

const TOL: f64 = 0.01;

#[test]
fn nominal_soic8_pad_geometry_within_one_hundredth_mm() -> Result<(), String> {
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
    let direct = generate_footprint("soic", LandPatternDensity::Nominal, &p)?;
    let key = PackageKey {
        family: PackageFamily::GullWing,
        pin_count: 8,
        pitch_mm: 1.27,
        body_length_mm: 4.9,
        body_width_mm: 3.9,
        lead_span_mm: 4.9,
        exposed_pad_mm: 0.0,
        height_mm: 0.0,
        courtyard_half_x: None,
        courtyard_half_y: None,
    };
    let via_key = generate_footprint_from_key(&key, LandPatternDensity::Nominal, "SOIC8")?;
    assert_eq!(direct.pads.len(), via_key.pads.len());
    for pad in &direct.pads {
        let other = via_key
            .pads
            .iter()
            .find(|p| p.number == pad.number)
            .ok_or_else(|| format!("pad {} missing on via_key footprint", pad.number))?;
        assert!((pad.position.x - other.position.x).abs() <= TOL);
        assert!((pad.position.y - other.position.y).abs() <= TOL);
        assert!((pad.size.x - other.size.x).abs() <= TOL);
        assert!((pad.size.y - other.size.y).abs() <= TOL);
    }
    Ok(())
}

#[test]
fn nominal_qfn16_ep_pad_geometry_within_one_hundredth_mm() -> Result<(), String> {
    let mut p = LandPatternParams {
        name: "QFN16_EP".into(),
        description: String::new(),
        keywords: String::new(),
        pitch: 0.65,
        pin_count: 16,
        body_length: 3.0,
        body_width: 3.0,
        row_spacing: 0.0,
        lead_width: 0.0,
        lead_diameter: 0.0,
        grid_rows: 0,
        grid_cols: 0,
        thermal_size: 1.7,
    };
    let direct = generate_footprint("qfn", LandPatternDensity::Nominal, &p)?;
    let key = PackageKey {
        family: PackageFamily::Qfn,
        pin_count: 16,
        pitch_mm: 0.65,
        body_length_mm: 3.0,
        body_width_mm: 3.0,
        lead_span_mm: 3.0,
        exposed_pad_mm: 1.7,
        height_mm: 0.0,
        courtyard_half_x: None,
        courtyard_half_y: None,
    };
    let via_key = generate_footprint_from_key(&key, LandPatternDensity::Nominal, "QFN16_EP")?;
    assert_eq!(direct.pads.len(), via_key.pads.len());
    let ep_direct = direct.pads.iter().find(|pad| pad.number == "EP").ok_or("EP missing")?;
    let ep_via = via_key.pads.iter().find(|pad| pad.number == "EP").ok_or("EP missing")?;
    assert!((ep_direct.size.x - ep_via.size.x).abs() <= TOL);
    assert!((ep_direct.size.y - ep_via.size.y).abs() <= TOL);
    for pad in direct.pads.iter().filter(|pad| pad.number != "EP") {
        let other = via_key
            .pads
            .iter()
            .find(|p| p.number == pad.number)
            .ok_or_else(|| format!("pad {} missing on via_key footprint", pad.number))?;
        assert!((pad.position.x - other.position.x).abs() <= TOL);
        assert!((pad.position.y - other.position.y).abs() <= TOL);
    }
    Ok(())
}
