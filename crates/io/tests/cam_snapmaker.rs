//! Golden and refusal tests for the Snapmaker CNC dialect (issue 357).
use pcbcraft_doc::{Component, Footprint, Hole, Layer, Net, Pad, Pin, Project, Track, Via};
use pcbcraft_geom::Point;
use pcbcraft_io::{
    CamProfile, CutDirection, DrillDialect, DrillMillTool, DrillOptions, DrillOrder,
    IsolationOptions, OutlineCompensation, OutlineOptions, Side, Spindle, Tabs, Tool, ToolChange,
    builtin_machine_profile, drill_gcode, isolation_gcode, outline_gcode, snapmaker_cnc_profile,
    snapmaker_files, snapmaker_finalize, snapmaker_settings, spindle_percent,
};

const A350T: &str = "snapmaker-2-a350t-200w-cnc";
const ARTISAN: &str = "snapmaker-artisan-200w-cnc";

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

fn arc_word(line: &str) -> bool {
    matches!(
        line.split_whitespace().next(),
        Some("G2" | "G02" | "G3" | "G03")
    )
}

/// Compares text with a stored fixture; `UPDATE_GOLDEN=1` rewrites the fixture.
fn golden(name: &str, text: &str) -> Result<(), String> {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    if std::env::var("UPDATE_GOLDEN").is_ok() {
        std::fs::write(&path, text).map_err(|e| e.to_string())?;
    }
    let want = std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?;
    if want != text {
        return Err(format!("output differs from fixture {name}"));
    }
    Ok(())
}

fn settings(id: &str) -> Result<pcbcraft_io::SnapmakerSettings, String> {
    snapmaker_settings(&builtin_machine_profile(id)?, false)
}

fn iso_tool(spindle: Spindle) -> Tool {
    Tool {
        diameter_mm: 0.1,
        feed_mm_min: 300.0,
        plunge_mm_min: 60.0,
        depth_mm: 0.08,
        passes: 1,
        spindle,
    }
}

fn isolation_file(p: &Project, s: &pcbcraft_io::SnapmakerSettings) -> Result<String, String> {
    let opts = IsolationOptions {
        side: Side::Top,
        tool: iso_tool(Spindle::Percent(spindle_percent(Spindle::Rpm(18000), s)?)),
        overlap_pct: 50,
        rubout: None,
        mirror_axis: pcbcraft_io::CamMirrorAxis::Y,
        alignment_holes: false,
    };
    let out = isolation_gcode(p, &opts, &snapmaker_cnc_profile(s))?;
    let files = snapmaker_files(out.files, s)?;
    files
        .into_iter()
        .next()
        .map(|(_, t)| t)
        .ok_or_else(|| "no file".to_string())
}

fn mill_profile(s: &pcbcraft_io::SnapmakerSettings) -> CamProfile {
    CamProfile {
        dialect: DrillDialect::Grbl,
        work_x_mm: s.work_x_mm,
        work_y_mm: s.work_y_mm,
        safe_z_mm: s.safe_z_mm,
        retract_z_mm: 2.0,
        direction: CutDirection::Conventional,
        plating_allowance_mm: 0.0,
        fab_plated: false,
    }
}

fn drill_tool(number: u8, d: f64) -> DrillMillTool {
    DrillMillTool {
        number,
        diameter_mm: d,
        spindle_rpm: 18000,
        cut_feed_mm_min: 300.0,
        plunge_feed_mm_min: 100.0,
    }
}

#[test]
fn snapmaker_isolation_golden_two_pads_one_track() -> Result<(), String> {
    let s = settings(A350T)?;
    let text = isolation_file(&sample_two_pads_track(), &s)?;
    assert!(text.starts_with(";Header Start\n;header_type: cnc\n;tool_head: levelTwoCNCToolheadForSM2\n;machine: Snapmaker 2.0 A350\n"));
    assert!(text.contains(";Header End\nG90\nG21\nG17\n"));
    assert!(text.contains("M3 P100\n"));
    assert!(!text.lines().any(|l| l.contains('S') && !l.starts_with(';')));
    assert!(!text.lines().any(arc_word));
    let tail: Vec<&str> = text.lines().rev().take(2).collect();
    assert_eq!(tail, vec!["M5", "M400"]);
    let declared: usize = text
        .lines()
        .find_map(|l| l.strip_prefix(";file_total_lines: "))
        .and_then(|v| v.parse().ok())
        .ok_or("no line count")?;
    assert_eq!(declared, text.lines().count());
    assert!(text.contains(";work_size_x: 320\n;work_size_y: 325\n"));
    Ok(())
}

#[test]
fn snapmaker_path_at_x_330_mm_refused_on_a350t_profile() -> Result<(), String> {
    let gcode = "G90\nG21\nM3 S18000\nG0 X330 Y10\nG1 Z-0.1 F100\nM5\n";
    let err = snapmaker_finalize(gcode, &settings(A350T)?)
        .err()
        .ok_or("expected a refusal")?;
    assert!(err.contains("X 330 mm") && err.contains(A350T), "{err}");
    Ok(())
}

#[test]
fn snapmaker_same_path_passes_on_artisan_profile() -> Result<(), String> {
    let gcode = "G90\nG21\nM3 S18000\nG0 X330 Y10\nG1 Z-0.1 F100\nM5\n";
    let out = snapmaker_finalize(gcode, &settings(ARTISAN)?)?;
    assert!(out.contains(";max_x(mm): 330\n"));
    Ok(())
}

#[test]
fn snapmaker_quick_swap_trims_y_work_area() -> Result<(), String> {
    let profile = builtin_machine_profile(A350T)?;
    let s = snapmaker_settings(&profile, true)?;
    assert_eq!(s.work_y_mm, 310.0);
    let err = snapmaker_finalize("G0 X10 Y320\nM5\n", &s).err();
    assert!(err.is_some_and(|e| e.contains("Y 320 mm")));
    assert!(snapmaker_settings(&builtin_machine_profile(ARTISAN)?, true).is_err());
    Ok(())
}

#[test]
fn snapmaker_drill_three_tool_sizes_yields_three_files_each_with_header_and_bounds()
-> Result<(), String> {
    let s = settings(A350T)?;
    let mut p = Project::default();
    p.board.width = 40.0;
    p.board.height = 30.0;
    p.board.thickness = 1.6;
    p.nets.push(Net {
        id: 100,
        name: "GND".into(),
        ..Default::default()
    });
    p.board.vias.push(Via {
        id: 1,
        net: 100,
        position: Point::new(10.0, 10.0),
        diameter: 0.6,
        drill: 0.3,
        from_layer: None,
        to_layer: None,
        covering: None,
            covering_scope: None,
    });
    p.board.holes.push(Hole {
        id: 2,
        position: Point::new(20.0, 15.0),
        diameter: 1.0,
        end: None,
    });
    p.board.holes.push(Hole {
        id: 3,
        position: Point::new(30.0, 20.0),
        diameter: 2.0,
        end: None,
    });
    let options = DrillOptions {
        tools: vec![drill_tool(1, 0.3), drill_tool(2, 1.0), drill_tool(3, 2.0)],
        peck_mm: None,
        order: DrillOrder::Nearest,
        alignment_holes: vec![],
        tool_change: ToolChange::NewFile,
    };
    let files = snapmaker_files(drill_gcode(&p, &options, &mill_profile(&s))?, &s)?;
    assert_eq!(files.len(), 3);
    let mut max_xs = Vec::new();
    for (_, text) in &files {
        assert!(text.starts_with(";Header Start\n"));
        assert!(text.contains(";min_z(mm): -1.6\n"), "{text}");
        let max_x = text
            .lines()
            .find_map(|l| l.strip_prefix(";max_x(mm): "))
            .ok_or("no max_x")?;
        max_xs.push(max_x.to_string());
        assert!(text.contains("M3 P100\n"));
        assert!(text.ends_with("M400\nM5\n"));
    }
    golden("snapmaker_drill_0.3mm.nc", &files[0].1)?;
    golden("snapmaker_drill_2.0mm.nc", &files[2].1)?;
    max_xs.sort();
    assert_eq!(max_xs, vec!["10", "20", "30"]);
    Ok(())
}

#[test]
fn snapmaker_outline_file_has_header_and_no_arcs() -> Result<(), String> {
    let s = settings(A350T)?;
    let mut p = Project::default();
    p.board.width = 40.0;
    p.board.height = 30.0;
    p.board.thickness = 1.6;
    let options = OutlineOptions {
        tool: drill_tool(1, 1.5),
        pass_depth_mm: 0.8,
        tabs: Tabs {
            count: 4,
            width_mm: 3.0,
            height_mm: 0.6,
        },
        compensation: OutlineCompensation::Outside,
        cutouts: false,
    };
    let text = snapmaker_finalize(&outline_gcode(&p, &options, &mill_profile(&s))?, &s)?;
    assert!(text.starts_with(";Header Start\n"));
    assert!(!text.lines().any(arc_word));
    assert!(text.ends_with("M400\nM5\n"));
    Ok(())
}

#[test]
fn snapmaker_profile_spindle_percent_40_refused_at_validation() -> Result<(), String> {
    let mut profile = builtin_machine_profile(A350T)?;
    if let Some(spindle) = profile.mill.spindle.as_mut() {
        spindle.default_percent = 40.0;
    }
    assert!(profile.validate().is_err());
    assert!(snapmaker_settings(&profile, false).is_err());
    let mut tool_low = builtin_machine_profile(A350T)?;
    if let Some(t) = tool_low.mill.tool.first_mut() {
        t.spindle_percent = 40.0;
    }
    assert!(tool_low.validate().is_err());
    Ok(())
}

#[test]
fn snapmaker_output_twice_is_byte_identical() -> Result<(), String> {
    let s = settings(A350T)?;
    let p = sample_two_pads_track();
    assert_eq!(isolation_file(&p, &s)?, isolation_file(&p, &s)?);
    Ok(())
}

#[test]
fn spindle_rpm_above_max_clamps_to_p100_without_s_word() -> Result<(), String> {
    let s = settings(A350T)?;
    assert_eq!(spindle_percent(Spindle::Rpm(30000), &s)?, 100);
    assert_eq!(spindle_percent(Spindle::Rpm(9000), &s)?, 50);
    assert!(spindle_percent(Spindle::Rpm(5000), &s).is_err());
    let out = snapmaker_finalize("M3 S30000\nG0 X1 Y1\nM5\n", &s)?;
    assert!(out.contains("M3 P100\n"));
    assert!(!out.contains("S30000"));
    Ok(())
}

#[test]
fn snapmaker_refuses_arcs_and_tool_changes() -> Result<(), String> {
    let s = settings(A350T)?;
    assert!(snapmaker_finalize("G2 X1 Y1 I1 J0\n", &s).is_err());
    assert!(snapmaker_finalize("M6 T2\n", &s).is_err());
    Ok(())
}
