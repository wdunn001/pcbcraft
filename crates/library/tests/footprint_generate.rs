use pcbcraft_geom::Point;
use pcbcraft_library::{
    FindingSeverity, FootprintDefinition, FootprintPad, Graphic, LandPatternDensity,
    LandPatternParams, check_footprint, generate_footprint,
};

const TOL: f64 = 0.05;

fn nominal_params() -> LandPatternParams {
    LandPatternParams {
        name: "TestFootprint".into(),
        description: String::new(),
        keywords: String::new(),
        pitch: 1.27,
        pin_count: 8,
        body_length: 2.0,
        body_width: 1.25,
        row_spacing: 0.0,
        lead_width: 0.0,
        lead_diameter: 0.0,
        grid_rows: 0,
        grid_cols: 0,
        thermal_size: 0.0,
    }
}

fn courtyard_half_extents(def: &FootprintDefinition) -> Option<(f64, f64)> {
    for g in &def.graphics {
        if let Graphic::Rectangle {
            start, end, layer, ..
        } = g
        {
            if layer == "pcbcraft.courtyard" {
                return Some((
                    start.x.abs().max(end.x.abs()),
                    start.y.abs().max(end.y.abs()),
                ));
            }
        }
    }
    None
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= TOL
}

#[test]
fn chip_two_terminal_matches_0805_fixture_span() -> Result<(), String> {
    let mut p = nominal_params();
    p.body_length = 2.0;
    p.body_width = 1.25;
    p.name = "R_0805_2012Metric_gen".into();
    let def = generate_footprint("chip_2terminal", LandPatternDensity::Nominal, &p)?;
    assert_eq!(def.pads.len(), 2);
    assert!(near(def.pads[0].position.x.abs(), 0.9125));
    assert!(near(def.pads[0].size.x, 1.025));
    assert!(near(def.pads[0].size.y, 1.4));
    let (hx, hy) = courtyard_half_extents(&def).ok_or("courtyard missing")?;
    assert!(near(hx, 1.68));
    assert!(near(hy, 0.95));
    Ok(())
}

#[test]
fn sot23_5_produces_five_pads() -> Result<(), String> {
    let mut p = nominal_params();
    p.pitch = 0.95;
    p.pin_count = 5;
    p.body_length = 2.9;
    p.body_width = 1.6;
    let def = generate_footprint("sot", LandPatternDensity::Nominal, &p)?;
    assert_eq!(def.pads.len(), 5);
    Ok(())
}

#[test]
fn soic8_pitch_and_row_span() -> Result<(), String> {
    let mut p = nominal_params();
    p.pitch = 1.27;
    p.pin_count = 8;
    p.body_width = 3.9;
    let def = generate_footprint("soic", LandPatternDensity::Nominal, &p)?;
    assert_eq!(def.pads.len(), 8);
    let row_y = def.pads[0].position.y.abs();
    assert!(near(row_y, 3.9 / 2.0 + p.pitch * 0.35 / 2.0));
    let xs: Vec<f64> = def.pads.iter().take(4).map(|pad| pad.position.x).collect();
    assert!(near(xs[1] - xs[0], 1.27));
    Ok(())
}

#[test]
fn qfp48_pad_count_and_pitch() -> Result<(), String> {
    let mut p = nominal_params();
    p.pitch = 0.5;
    p.pin_count = 48;
    p.body_length = 7.0;
    p.body_width = 7.0;
    let def = generate_footprint("qfp", LandPatternDensity::Nominal, &p)?;
    assert_eq!(def.pads.len(), 48);
    assert!(near(def.pads[0].size.x, 0.5 * 0.55));
    Ok(())
}

#[test]
fn qfn33_includes_center_thermal_pad() -> Result<(), String> {
    let mut p = nominal_params();
    p.pitch = 0.5;
    p.pin_count = 32;
    p.body_length = 5.0;
    p.body_width = 5.0;
    p.thermal_size = 2.5;
    let def = generate_footprint("qfn", LandPatternDensity::Nominal, &p)?;
    assert_eq!(def.pads.len(), 33);
    assert!(def.pads.iter().any(|pad| pad.number == "EP"));
    Ok(())
}

#[test]
fn bga_10x10_grid_pad_count() -> Result<(), String> {
    let mut p = nominal_params();
    p.pitch = 0.8;
    p.grid_rows = 10;
    p.grid_cols = 10;
    p.body_length = 8.0;
    p.body_width = 8.0;
    let def = generate_footprint("bga", LandPatternDensity::Nominal, &p)?;
    assert_eq!(def.pads.len(), 100);
    Ok(())
}

#[test]
fn dip8_plated_drills_inside_pads() -> Result<(), String> {
    let mut p = nominal_params();
    p.pitch = 2.54;
    p.pin_count = 8;
    p.row_spacing = 7.62;
    let def = generate_footprint("dip", LandPatternDensity::Nominal, &p)?;
    assert_eq!(def.pads.len(), 8);
    assert!(def.pads.iter().all(|pad| {
        pad.drill
            .as_ref()
            .is_some_and(|d| d.size.x <= pad.size.x && d.size.y <= pad.size.y)
    }));
    Ok(())
}

#[test]
fn header_single_row_count() -> Result<(), String> {
    let mut p = nominal_params();
    p.pitch = 2.54;
    p.pin_count = 6;
    let def = generate_footprint("header_single", LandPatternDensity::Nominal, &p)?;
    assert_eq!(def.pads.len(), 6);
    Ok(())
}

#[test]
fn header_double_row_stagger() -> Result<(), String> {
    let mut p = nominal_params();
    p.pitch = 2.54;
    p.pin_count = 10;
    p.row_spacing = 2.54;
    let def = generate_footprint("header_double", LandPatternDensity::Nominal, &p)?;
    assert_eq!(def.pads.len(), 10);
    let top: Vec<_> = def.pads.iter().filter(|pad| pad.position.y < 0.0).collect();
    let bottom: Vec<_> = def.pads.iter().filter(|pad| pad.position.y > 0.0).collect();
    assert_eq!(top.len(), 5);
    assert_eq!(bottom.len(), 5);
    assert!(near(bottom[0].position.x - top[0].position.x, 2.54 / 2.0));
    Ok(())
}

#[test]
fn radial_two_lead_span() -> Result<(), String> {
    let mut p = nominal_params();
    p.pitch = 2.0;
    p.body_length = 5.0;
    p.body_width = 5.0;
    let def = generate_footprint("radial", LandPatternDensity::Nominal, &p)?;
    assert_eq!(def.pads.len(), 2);
    assert!(near(def.pads[1].position.x - def.pads[0].position.x, 2.0));
    Ok(())
}

#[test]
fn axial_two_lead_span() -> Result<(), String> {
    let mut p = nominal_params();
    p.pitch = 10.0;
    p.body_length = 6.0;
    p.body_width = 2.5;
    let def = generate_footprint("axial", LandPatternDensity::Nominal, &p)?;
    assert_eq!(def.pads.len(), 2);
    assert!(near(def.pads[1].position.x - def.pads[0].position.x, 10.0));
    Ok(())
}

#[test]
fn generate_rejects_non_finite_pitch() -> Result<(), String> {
    let mut p = nominal_params();
    p.pitch = f64::NAN;
    match generate_footprint("soic", LandPatternDensity::Nominal, &p) {
        Err(message) if message.contains("pitch") => Ok(()),
        Ok(_) => Err("expected pitch rejection".into()),
        Err(other) => Err(other),
    }
}

#[test]
fn check_reports_overlapping_pads() -> Result<(), String> {
    let overlap = FootprintDefinition {
        name: "OverlapFixture".into(),
        description: String::new(),
        keywords: String::new(),
        pads: vec![
            FootprintPad {
                number: "1".into(),
                pad_type: "smd".into(),
                shape: "rect".into(),
                position: Point::new(0.0, 0.0),
                rotation: 0.0,
                size: Point::new(2.0, 2.0),
                layers: vec!["F.Cu".into(), "F.Mask".into(), "F.Paste".into()],
                drill: None,
                roundrect_ratio: None,
                chamfer: None,
                polygon: vec![],
                raw_source: String::new(),
            },
            FootprintPad {
                number: "2".into(),
                pad_type: "smd".into(),
                shape: "rect".into(),
                position: Point::new(0.5, 0.0),
                rotation: 0.0,
                size: Point::new(2.0, 2.0),
                layers: vec!["F.Cu".into(), "F.Mask".into(), "F.Paste".into()],
                drill: None,
                roundrect_ratio: None,
                chamfer: None,
                polygon: vec![],
                raw_source: String::new(),
            },
        ],
        graphics: vec![Graphic::Rectangle {
            start: Point::new(-5.0, -5.0),
            end: Point::new(5.0, 5.0),
            width: 0.1,
            fill: String::new(),
            layer: "pcbcraft.courtyard".into(),
        }],
        model: None,
        rules: None,
        raw_source: String::new(),
        unsupported: vec![],
    };
    let findings = check_footprint(&overlap);
    if findings
        .iter()
        .any(|f| f.code == "lib_pad_overlap" && f.severity == FindingSeverity::Error)
    {
        Ok(())
    } else {
        Err("expected lib_pad_overlap".into())
    }
}

#[test]
fn check_accepts_fresh_qfn() -> Result<(), String> {
    let mut p = nominal_params();
    p.pitch = 0.5;
    p.pin_count = 32;
    p.body_length = 5.0;
    p.body_width = 5.0;
    p.thermal_size = 2.5;
    p.name = "QFN_test".into();
    let def = generate_footprint("qfn", LandPatternDensity::Nominal, &p)?;
    let findings = check_footprint(&def);
    if findings
        .iter()
        .any(|f| f.severity == FindingSeverity::Error)
    {
        Err(format!("unexpected errors: {findings:?}"))
    } else {
        Ok(())
    }
}
