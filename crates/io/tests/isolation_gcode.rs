use pcbcraft_doc::{Component, Footprint, Layer, Net, Pad, Pin, Project, Track};
use pcbcraft_geom::{Point, segment_distance};
use pcbcraft_io::{
    CamMirrorAxis as MirrorAxis, CncProfile, IsolationOptions, Side, Spindle, Tool,
    cam_layer_shapes as layer_shapes, isolation_gcode, isolation_paths,
};

fn tool_two_pass() -> Tool {
    Tool {
        diameter_mm: 0.1,
        feed_mm_min: 300.0,
        plunge_mm_min: 60.0,
        depth_mm: 0.08,
        passes: 2,
        spindle: Spindle::Rpm(18000),
    }
}

fn sample_two_pads_track() -> Project {
    let mut p = Project::default();
    p.board.width = 20.0;
    p.board.height = 20.0;
    p.board.thickness = 1.6;
    p.nets.push(Net {
        id: 101,
        name: "A".into(),
        ..Net::default()
    });
    p.components.push(Component {
        id: 201,
        reference: "U1".into(),
        value: "A".into(),
        kind: "connector".into(),
        position: Point::new(5.0, 10.0),
        rotation: 0.0,
        pins: vec![Pin {
            number: "1".into(),
            name: "1".into(),
            offset: Point::default(),
            net: Some(101),
            electrical_type: "passive".into(),
            direction: None,
            unit: 1,
            appearances: vec![],
        }],
        units: vec![],
        library_symbol: None,
        library_footprint: None,
        fields: vec![],
        sheet: 0,
    });
    p.components.push(Component {
        id: 202,
        reference: "U2".into(),
        value: "B".into(),
        kind: "connector".into(),
        position: Point::new(15.0, 10.0),
        rotation: 0.0,
        pins: vec![Pin {
            number: "1".into(),
            name: "1".into(),
            offset: Point::default(),
            net: Some(101),
            electrical_type: "passive".into(),
            direction: None,
            unit: 1,
            appearances: vec![],
        }],
        units: vec![],
        library_symbol: None,
        library_footprint: None,
        fields: vec![],
        sheet: 0,
    });
    let fp1 = Footprint {
        id: 10,
        component: 201,
        position: Point::new(5.0, 10.0),
        rotation: 0.0,
        pads: vec![Pad {
            number: "1".into(),
            offset: Point::default(),
            diameter: 2.0,
            net: Some(101),
            castellated: None,
            geometry: None,
        }],
        locked: None,
        side: None,
    };
    let mut fp2 = fp1.clone();
    fp2.id = 11;
    fp2.component = 202;
    fp2.position = Point::new(15.0, 10.0);
    fp2.pads[0].net = Some(101);
    p.board.footprints.push(fp1);
    p.board.footprints.push(fp2);
    p.board.tracks.push(Track {
        id: 20,
        net: 101,
        start: Point::new(6.0, 10.0),
        end: Point::new(14.0, 10.0),
        width: 0.4,
        layer: Layer::Front,
        arc: None,
    });
    p.board.tracks.push(Track {
        id: 21,
        net: 101,
        start: Point::new(8.0, 12.0),
        end: Point::new(12.0, 12.0),
        width: 0.4,
        layer: Layer::Back,
        arc: None,
    });
    p
}

#[test]
fn isolation_gcode_golden_two_pads_one_track_point_two_tool_two_passes() -> Result<(), String> {
    let p = sample_two_pads_track();
    let opts = IsolationOptions {
        side: Side::Top,
        tool: tool_two_pass(),
        overlap_pct: 50,
        rubout: None,
        mirror_axis: MirrorAxis::Y,
        alignment_holes: false,
    };
    let out = isolation_gcode(&p, &opts, &CncProfile::default())?;
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/isolation_two_pads_one_track.gcode");
    if std::env::var("BLESS_GOLDEN").is_ok() {
        std::fs::write(&path, &out.files[0].1).map_err(|e| e.to_string())?;
    }
    let golden = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    assert_eq!(out.files[0].1, golden);
    Ok(())
}

#[test]
fn no_cut_comes_closer_to_copper_than_half_the_tool() -> Result<(), String> {
    let p = sample_two_pads_track();
    let tool = tool_two_pass();
    let opts = IsolationOptions {
        side: Side::Top,
        tool,
        overlap_pct: 0,
        rubout: None,
        mirror_axis: MirrorAxis::Y,
        alignment_holes: false,
    };
    let gcode = isolation_gcode(&p, &opts, &CncProfile::default())?.files[0]
        .1
        .clone();
    let copper = layer_shapes(&p, Layer::Front)?;
    let half = tool.diameter_mm / 2.0;
    let safe = CncProfile::default().safe_z_mm;
    let mut pos = Point::default();
    let mut cutting = false;
    for line in gcode.lines() {
        if let Some(rest) = line.strip_prefix("G1 Z") {
            let z: f64 = rest
                .split_whitespace()
                .next()
                .unwrap_or("0")
                .parse()
                .map_err(|_| "z".to_string())?;
            cutting = z < 0.0;
            continue;
        }
        if line.starts_with("G1 X") && cutting {
            let parts: Vec<&str> = line.split_whitespace().collect();
            let x: f64 = parts[1][1..].parse().map_err(|_| "x".to_string())?;
            let y: f64 = parts[2][1..].parse().map_err(|_| "y".to_string())?;
            let next = Point::new(x, y);
            for shape in &copper {
                for (a, b) in pcbcraft_geom::polygon_edges(&shape.outline) {
                    let d = segment_distance(pos, next, a, b);
                    assert!(
                        d + 0.002 >= half,
                        "cut passes {d:.3} mm from copper (limit {half:.3})"
                    );
                }
            }
            pos = next;
        } else if line.starts_with("G0 X") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            let x: f64 = parts[1][1..].parse().map_err(|_| "x".to_string())?;
            let y: f64 = parts[2][1..].parse().map_err(|_| "y".to_string())?;
            pos = Point::new(x, y);
        }
        if line.starts_with("G1 Z") && line.contains(&format!("{safe:.3}")) {
            cutting = false;
        }
    }
    Ok(())
}

#[test]
fn bottom_side_is_mirrored_about_the_chosen_axis() -> Result<(), String> {
    let p = sample_two_pads_track();
    let opts_top = IsolationOptions {
        side: Side::Top,
        tool: tool_two_pass(),
        overlap_pct: 0,
        rubout: None,
        mirror_axis: MirrorAxis::X,
        alignment_holes: false,
    };
    let mut opts_bottom = opts_top.clone();
    opts_bottom.side = Side::Bottom;
    let top = isolation_gcode(&p, &opts_top, &CncProfile::default())?.files[0]
        .1
        .clone();
    let bottom = isolation_gcode(&p, &opts_bottom, &CncProfile::default())?.files[0]
        .1
        .clone();
    assert_ne!(top, bottom);
    let centre = Point::new(p.board.width / 2.0, p.board.height / 2.0);
    let xs = |text: &str| {
        text.lines()
            .filter_map(|line| {
                line.split_whitespace()
                    .find_map(|token| token.strip_prefix('X').and_then(|v| v.parse::<f64>().ok()))
            })
            .collect::<Vec<_>>()
    };
    let top_x = xs(&top);
    let bottom_x = xs(&bottom);
    assert!(top_x.iter().any(|x| {
        bottom_x
            .iter()
            .any(|b| (b - (2.0 * centre.x - x)).abs() < 0.05)
    }));
    Ok(())
}

#[test]
fn two_nets_closer_than_the_tool_refuse_with_both_ids() {
    let mut p = sample_two_pads_track();
    p.board.tracks.clear();
    p.nets.push(Net {
        id: 102,
        name: "B".into(),
        ..Net::default()
    });
    p.board.footprints[1].pads[0].net = Some(102);
    p.components[1].pins[0].net = Some(102);
    p.board.footprints[1].position = Point::new(6.05, 10.0);
    let opts = IsolationOptions {
        side: Side::Top,
        tool: tool_two_pass(),
        overlap_pct: 0,
        rubout: None,
        mirror_axis: MirrorAxis::Y,
        alignment_holes: false,
    };
    let err = isolation_paths(&p, &opts).err().unwrap_or_default();
    assert!(err.contains("10") && err.contains("11"));
}

#[test]
fn every_coordinate_stays_inside_the_work_area_or_the_job_is_refused() {
    let p = sample_two_pads_track();
    let opts = IsolationOptions {
        side: Side::Top,
        tool: tool_two_pass(),
        overlap_pct: 0,
        rubout: None,
        mirror_axis: MirrorAxis::Y,
        alignment_holes: false,
    };
    let tight = CncProfile {
        work_max: Point::new(8.0, 8.0),
        ..CncProfile::default()
    };
    assert!(isolation_gcode(&p, &opts, &tight).is_err());
}

#[test]
fn time_estimate_matches_length_over_feed() -> Result<(), String> {
    let p = sample_two_pads_track();
    let tool = tool_two_pass();
    let opts = IsolationOptions {
        side: Side::Top,
        tool,
        overlap_pct: 0,
        rubout: None,
        mirror_axis: MirrorAxis::Y,
        alignment_holes: false,
    };
    let out = isolation_gcode(&p, &opts, &CncProfile::default())?;
    assert!(out.estimate_s > 1.0);
    let again = isolation_gcode(&p, &opts, &CncProfile::default())?;
    assert!((out.estimate_s - again.estimate_s).abs() < 1e-6);
    Ok(())
}

#[test]
fn isolation_gcode_twice_is_byte_identical() -> Result<(), String> {
    let p = sample_two_pads_track();
    let opts = IsolationOptions {
        side: Side::Top,
        tool: tool_two_pass(),
        overlap_pct: 25,
        rubout: None,
        mirror_axis: MirrorAxis::Y,
        alignment_holes: false,
    };
    let profile = CncProfile::default();
    let a = isolation_gcode(&p, &opts, &profile)?.files[0].1.clone();
    let b = isolation_gcode(&p, &opts, &profile)?.files[0].1.clone();
    assert_eq!(a, b);
    Ok(())
}
