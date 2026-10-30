//! Per-span drill table rows, readme and drill map alignment, and tolerance attributes.
use pcbcraft_doc::Project;
use pcbcraft_geom::Point;
use pcbcraft_io::{
    drill_map_plot, drill_table, fab_readme_text, gerber_files, DrillTableFunction,
};
fn table_rows_from_readme(text: &str) -> Vec<String> {
    let mut rows = Vec::new();
    let mut in_table = false;
    for line in text.lines() {
        if line.starts_with("Drill table (span, plating, function") {
            in_table = true;
            continue;
        }
        if in_table {
            if let Some(rest) = line.strip_prefix("  ") {
                rows.push(rest.trim().to_string());
            } else if !line.is_empty() {
                break;
            }
        }
    }
    rows
}

fn table_rows_from_map(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| {
            line.strip_prefix("G04 TABLE ")
                .and_then(|rest| rest.strip_suffix(" *%"))
                .map(str::trim)
                .map(str::to_string)
        })
        .collect()
}

fn sample_board_with_three_drill_roles() -> Result<Project, String> {
    let mut project = Project::default();
    project.nets.push(pcbcraft_doc::Net {
        id: 8,
        name: "N1".into(),
        ..Default::default()
    });
    project.board.vias.push(pcbcraft_doc::Via {
        id: 4,
        net: 8,
        position: Point::new(5.0, 5.0),
        diameter: 0.6,
        drill: 0.3,
        from_layer: None,
        to_layer: None,
        covering: None,
    });
    project.board.vias.push(pcbcraft_doc::Via {
        id: 5,
        net: 8,
        position: Point::new(6.0, 5.0),
        diameter: 0.6,
        drill: 0.3,
        from_layer: None,
        to_layer: None,
        covering: None,
    });
    project.board.holes.push(pcbcraft_doc::Hole {
        id: 2,
        position: Point::new(15.0, 5.0),
        diameter: 3.0,
        end: None,
    });
    let pad = pcbcraft_doc::Pad {
        number: "1".into(),
        offset: Point::new(0.0, 0.0),
        diameter: 1.6,
        net: Some(8),
        castellated: None,
        geometry: Some(pcbcraft_library::FootprintPad {
            number: "1".into(),
            pad_type: "thru_hole".into(),
            shape: "circle".into(),
            position: Point::new(0.0, 0.0),
            rotation: 0.0,
            size: Point::new(1.6, 1.6),
            layers: vec!["*.Cu".into(), "*.Mask".into()],
            drill: Some(pcbcraft_library::Drill {
                size: Point::new(0.8, 0.8),
                offset: Point::default(),
            }),
            roundrect_ratio: None,
            raw_source: String::new(),
            chamfer: None,
            polygon: Vec::new(),
        }),
    };
    project.board.footprints.push(pcbcraft_doc::Footprint {
        id: 9,
        component: 0,
        position: Point::new(2.0, 0.0),
        rotation: 0.0,
        locked: None,
        side: None,
        pads: vec![pad],
    });
    project.validate()?;
    Ok(project)
}

#[test]
fn the_table_has_one_row_per_span_function_and_diameter() -> Result<(), String> {
    let project = sample_board_with_three_drill_roles()?;
    let rows = drill_table(&project)?;
    assert_eq!(rows.len(), 3, "via, pad and mounting each get one row");
    let via_row = rows
        .iter()
        .find(|r| r.function == DrillTableFunction::Via)
        .ok_or("via row missing")?;
    assert_eq!(via_row.from_layer, 1);
    assert_eq!(via_row.to_layer, 2);
    assert!(via_row.plated);
    assert_eq!(via_row.count, 2);
    assert!((via_row.diameter_mm - 0.3).abs() < 1e-6);
    let pad_row = rows
        .iter()
        .find(|r| r.function == DrillTableFunction::ComponentPad)
        .ok_or("pad row missing")?;
    assert_eq!(pad_row.count, 1);
    assert!((pad_row.diameter_mm - 0.8).abs() < 1e-6);
    let mount_row = rows
        .iter()
        .find(|r| r.function == DrillTableFunction::Mounting)
        .ok_or("mounting row missing")?;
    assert!(!mount_row.plated);
    assert_eq!(mount_row.count, 1);
    Ok(())
}

#[test]
fn the_readme_and_the_drill_map_list_the_same_rows() -> Result<(), String> {
    let project = sample_board_with_three_drill_roles()?;
    let names = vec!["pcb-DrillMap.gbr".into(), "pcb-Plated.drl".into()];
    let readme = fab_readme_text(&project, &names)?;
    let map = drill_map_plot(&project)?;
    let readme_rows = table_rows_from_readme(&readme);
    let map_rows = table_rows_from_map(&map);
    assert_eq!(readme_rows, map_rows, "readme and map share row order");
    assert_eq!(readme_rows.len(), 3);
    Ok(())
}

#[test]
fn a_drill_tolerance_is_written_only_when_the_board_records_one() -> Result<(), String> {
    let mut with_tol = sample_board_with_three_drill_roles()?;
    with_tol.board.machining = Some(pcbcraft_doc::MachiningNotes {
        edge_plating: None,
        drill_tolerance: Some(0.05),
        edge_tolerance: None,
    });
    with_tol.validate()?;
    let files = gerber_files(&with_tol)?;
    let plated = files
        .get("pcb-Plated.drl")
        .ok_or("plated drill file missing")?;
    assert!(
        plated.contains("; #@! TA.DrillTolerance,0.050,0.050\n"),
        "{plated}"
    );
    let without_tol = sample_board_with_three_drill_roles()?;
    let files = gerber_files(&without_tol)?;
    let plated = files
        .get("pcb-Plated.drl")
        .ok_or("plated drill file missing")?;
    assert!(
        !plated.contains("TA.DrillTolerance"),
        "tolerance attribute omitted without machining notes: {plated}"
    );
    Ok(())
}
