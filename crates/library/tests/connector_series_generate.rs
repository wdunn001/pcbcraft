use pcbcraft_library::{
    body_fits_courtyard, generate_connector_part, pin1_position, ConnectorParametricSpec,
    ConnectorSeriesOptions, Graphic, PARAMETRIC_MODEL_PATH,
};

const TOL: f64 = 0.01;

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= TOL
}

fn courtyard_half(fp: &pcbcraft_library::FootprintDefinition) -> Option<(f64, f64)> {
    for g in &fp.graphics {
        if let Graphic::Rectangle {
            start, end, layer, ..
        } = g
            && layer == "pcbcraft.courtyard"
        {
            return Some((
                start.x.abs().max(end.x.abs()),
                start.y.abs().max(end.y.abs()),
            ));
        }
    }
    None
}

fn options_tsw(positions: u32, rows: u32) -> ConnectorSeriesOptions {
    ConnectorSeriesOptions {
        positions_per_row: positions,
        rows,
        lead_code: "07".into(),
        stack_code: None,
        alignment_posts: false,
    }
}

#[test]
fn tsw_example_parts_validate() -> Result<(), String> {
    for (positions, rows) in [(8, 1), (10, 1), (6, 2), (20, 2)] {
        let part = generate_connector_part("TSW", &options_tsw(positions, rows))
            .map_err(|e| format!("{e:?}"))?;
        part.footprint.validate().map_err(|e| e.to_string())?;
        assert!(part.footprint.description.contains("verify against"));
        assert!(part.footprint.description.contains("samtec.com"));
    }
    Ok(())
}

#[test]
fn pad_pitch_and_count_properties_tsw() -> Result<(), String> {
    let mut state: u64 = 0xDEAD_BEEF;
    for _ in 0..1000 {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        let positions = 1 + (state % 32) as u32;
        let rows = 1 + (state % 2) as u32;
        let part = generate_connector_part("TSW", &options_tsw(positions, rows))
            .map_err(|e| format!("{e:?}"))?;
        let signal_pads: Vec<_> = part
            .footprint
            .pads
            .iter()
            .filter(|p| p.number.chars().all(|c| c.is_ascii_digit()))
            .collect();
        assert_eq!(signal_pads.len(), (positions * rows) as usize);
        if positions > 1 {
            let mut xs: Vec<f64> = signal_pads.iter().map(|p| p.position.x).collect();
            xs.sort_by(f64::total_cmp);
            let gap = xs[1] - xs[0];
            assert!(near(gap, 2.54) || near(gap, 2.54 / 2.0));
        }
        let (x1, y1) = pin1_position(&part.footprint).ok_or("pin 1 missing")?;
        assert!(x1 <= 0.0);
        assert!(near(y1, 0.0) || y1 < 0.0);
        let (hx, hy) = courtyard_half(&part.footprint).ok_or("courtyard")?;
        for pad in &part.footprint.pads {
            assert!(pad.position.x.abs() + pad.size.x / 2.0 <= hx + TOL);
            assert!(pad.position.y.abs() + pad.size.y / 2.0 <= hy + TOL);
        }
        let expected_l = pcbcraft_library::housing_length_mm(positions, 2.54);
        assert!(near(part.parametric.housing_length_mm, expected_l));
    }
    Ok(())
}

#[test]
fn bth_stack_height_matches_table() -> Result<(), String> {
    let part = generate_connector_part(
        "BTH",
        &ConnectorSeriesOptions {
            positions_per_row: 30,
            rows: 2,
            lead_code: String::new(),
            stack_code: Some("01".into()),
            alignment_posts: true,
        },
    )
    .map_err(|e| format!("{e:?}"))?;
    assert_eq!(part.mated_stack_height_mm, Some(5.0));
    assert_eq!(part.footprint.pads.len(), 60 + 2);
    assert!(body_fits_courtyard(&part.parametric, &part.footprint));
    Ok(())
}

#[test]
fn parametric_body_z0_and_json_round_trip() -> Result<(), String> {
    let part = generate_connector_part("SSW", &options_tsw(4, 1)).map_err(|e| format!("{e:?}"))?;
    assert_eq!(part.model.path, PARAMETRIC_MODEL_PATH);
    let (min, max) = part.parametric.body_above_board_bounds();
    assert!(near(min[2], 0.0));
    assert!(max[2] >= part.parametric.housing_height_mm);
    let parsed = ConnectorParametricSpec::from_json(&part.model.raw_source)?;
    assert_eq!(parsed, part.parametric);
    Ok(())
}

#[test]
fn invalid_lead_code_names_field() {
    let err = generate_connector_part(
        "TSW",
        &ConnectorSeriesOptions {
            positions_per_row: 4,
            rows: 1,
            lead_code: "ZZ".into(),
            stack_code: None,
            alignment_posts: false,
        },
    )
    .unwrap_err();
    assert_eq!(err[0].field, "lead_code");
    assert!(!err[0].valid_values.is_empty());
}
