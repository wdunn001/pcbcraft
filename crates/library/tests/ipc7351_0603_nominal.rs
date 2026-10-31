use pcbcraft_library::{
    generate_footprint, generate_footprint_from_key, LandPatternDensity, LandPatternParams,
    PackageFamily, PackageKey,
};

const TOL: f64 = 0.01;

#[test]
fn nominal_0603_pad_geometry_within_one_hundredth_mm() -> Result<(), String> {
    let mut p = LandPatternParams {
        name: "R_0603".into(),
        description: String::new(),
        keywords: String::new(),
        pitch: 1.0,
        pin_count: 2,
        body_length: 1.6,
        body_width: 0.8,
        row_spacing: 0.0,
        lead_width: 0.0,
        lead_diameter: 0.0,
        grid_rows: 0,
        grid_cols: 0,
        thermal_size: 0.0,
    };
    let direct = generate_footprint("chip_2terminal", LandPatternDensity::Nominal, &p)?;
    let key = PackageKey {
        family: PackageFamily::Chip,
        pin_count: 2,
        pitch_mm: 1.0,
        body_length_mm: 1.6,
        body_width_mm: 0.8,
        lead_span_mm: 1.6,
        exposed_pad_mm: 0.0,
        height_mm: 0.0,
        courtyard_half_x: None,
        courtyard_half_y: None,
    };
    let via_key = generate_footprint_from_key(&key, LandPatternDensity::Nominal, "R_0603")?;
    assert!((direct.pads[0].size.x - via_key.pads[0].size.x).abs() <= TOL);
    assert!((direct.pads[0].position.x.abs() - via_key.pads[0].position.x.abs()).abs() <= TOL);
    Ok(())
}
