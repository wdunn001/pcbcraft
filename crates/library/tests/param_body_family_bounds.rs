use pcbcraft_library::{PackageFamily, PackageKey, ParamBody};

const TOL: f64 = 0.001;

fn assert_bounds(body: &ParamBody, min: [f64; 3], max: [f64; 3]) -> Result<(), String> {
    let mesh = body.mesh();
    let (got_min, got_max) = mesh.bounds().ok_or("mesh has no bounds")?;
    for i in 0..3 {
        if (got_min[i] - min[i]).abs() > TOL || (got_max[i] - max[i]).abs() > TOL {
            return Err(format!(
                "family {:?} bounds mismatch: got [{:?}..{:?}] expected [{min:?}..{max:?}]",
                body.family, got_min, got_max
            ));
        }
    }
    Ok(())
}

fn key(
    family: PackageFamily,
    pin_count: u32,
    pitch: f64,
    body_l: f64,
    body_w: f64,
    span: f64,
    ep: f64,
    height: f64,
) -> PackageKey {
    PackageKey {
        family,
        pin_count,
        pitch_mm: pitch,
        body_length_mm: body_l,
        body_width_mm: body_w,
        lead_span_mm: span,
        exposed_pad_mm: ep,
        height_mm: height,
        courtyard_half_x: None,
        courtyard_half_y: None,
    }
}

#[test]
fn chip_bounding_box_golden() -> Result<(), String> {
    let body = ParamBody::from_key(
        &key(PackageFamily::Chip, 2, 1.0, 1.6, 0.8, 1.6, 0.0, 0.55),
        "passive",
    );
    assert_bounds(&body, [-0.8, -0.4, 0.0], [0.8, 0.4, 0.55])
}

#[test]
fn sot_bounding_box_golden() -> Result<(), String> {
    let body = ParamBody::from_key(
        &key(PackageFamily::Sot, 5, 0.95, 2.9, 1.6, 2.9, 0.0, 1.1),
        "ic",
    );
    assert_bounds(&body, [-1.45, -0.8, 0.0], [1.45, 0.8, 1.1])
}

#[test]
fn gull_wing_bounding_box_golden() -> Result<(), String> {
    let body = ParamBody::from_key(
        &key(PackageFamily::GullWing, 8, 1.27, 4.9, 3.9, 4.9, 0.0, 1.1),
        "ic",
    );
    assert_bounds(&body, [-2.45, -1.95, 0.0], [2.45, 1.95, 1.1])
}

#[test]
fn qfn_bounding_box_golden() -> Result<(), String> {
    let body = ParamBody::from_key(
        &key(PackageFamily::Qfn, 16, 0.65, 3.0, 3.0, 3.0, 1.7, 0.9),
        "ic",
    );
    assert_bounds(&body, [-1.5, -1.5, 0.0], [1.5, 1.5, 0.9])
}

#[test]
fn bga_bounding_box_golden() -> Result<(), String> {
    let body = ParamBody::from_key(
        &key(PackageFamily::Bga, 100, 0.8, 8.0, 8.0, 8.0, 0.0, 0.8),
        "ic",
    );
    assert_bounds(&body, [-4.0, -4.0, 0.0], [4.0, 4.0, 0.8])
}

#[test]
fn dip_bounding_box_golden() -> Result<(), String> {
    let body = ParamBody::from_key(
        &key(PackageFamily::Dip, 8, 2.54, 8.0, 6.0, 7.62, 0.0, 3.3),
        "ic",
    );
    assert_bounds(&body, [-3.0, -4.0, 0.0], [3.0, 4.0, 3.3])
}

#[test]
fn pin_header_bounding_box_golden() -> Result<(), String> {
    let body = ParamBody::from_key(
        &key(PackageFamily::PinHeader, 6, 2.54, 15.24, 2.54, 15.24, 0.0, 2.54),
        "connector",
    );
    assert_bounds(&body, [-7.62, -1.27, 0.0], [7.62, 1.27, 2.54])
}

#[test]
fn cylinder_bounding_box_golden() -> Result<(), String> {
    let body = ParamBody::from_key(
        &key(PackageFamily::Cylinder, 2, 2.0, 5.0, 4.0, 2.0, 0.0, 5.0),
        "passive",
    );
    let mesh = body.mesh();
    let (min, max) = mesh.bounds().ok_or("mesh has no bounds")?;
    if (min[0] + 2.0).abs() > TOL
        || (max[0] - 2.0).abs() > TOL
        || (min[1] + 2.0).abs() > TOL
        || (max[1] - 2.0).abs() > TOL
        || min[2].abs() > TOL
        || (max[2] - 5.0).abs() > TOL
    {
        return Err(format!("cylinder bounds mismatch: {min:?}..{max:?}"));
    }
    Ok(())
}

#[test]
fn axial_bounding_box_golden() -> Result<(), String> {
    let body = ParamBody::from_key(
        &key(PackageFamily::Axial, 2, 10.0, 6.0, 2.5, 10.0, 0.0, 1.0),
        "passive",
    );
    assert_bounds(&body, [-3.0, -1.25, 0.0], [3.0, 1.25, 1.0])
}

#[test]
fn radial_bounding_box_golden() -> Result<(), String> {
    let body = ParamBody::from_key(
        &key(PackageFamily::Radial, 2, 2.0, 5.0, 5.0, 2.0, 0.0, 1.0),
        "passive",
    );
    assert_bounds(&body, [-2.5, -2.5, 0.0], [2.5, 2.5, 1.0])
}
