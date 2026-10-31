use pcbcraft_library::{PackageFamily, PackageKey, ParamBody};

#[test]
fn param_body_step_export_is_non_empty_ap214() -> Result<(), String> {
    let key = PackageKey {
        family: PackageFamily::GullWing,
        pin_count: 8,
        pitch_mm: 1.27,
        body_length_mm: 4.9,
        body_width_mm: 3.9,
        lead_span_mm: 4.9,
        exposed_pad_mm: 0.0,
        height_mm: 1.1,
        courtyard_half_x: None,
        courtyard_half_y: None,
    };
    let body = ParamBody::from_key(&key, "ic");
    let text = pcbcraft_io::step_param_body(&body)?;
    assert!(text.starts_with("ISO-10303-21;"), "STEP header missing");
    assert!(text.ends_with("END-ISO-10303-21;\n"));
    assert!(text.contains("MANIFOLD_SOLID_BREP"), "solid missing");
    assert!(text.contains("ADVANCED_FACE("), "faces missing");
    Ok(())
}
