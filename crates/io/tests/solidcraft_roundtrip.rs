//! SolidCraft interchange fixtures and export round trips (#736).
use pcbcraft_io::{
    MeshLayers, SolidcraftDesign, SolidcraftLayers, board_mesh, bounds_close, design_as_value,
    designs_equal, part_snapshots, read_solidcraft, solidcraft_board, solidcraft_counts,
    transforms_close, write_solidcraft_container, write_solidcraft_json,
};
use pcbcraft_doc::{Layer, Project};
use pcbcraft_geom::Point;
use serde_json::Value;

type Check = Result<(), String>;

fn read_fixture(name: &str) -> Result<SolidcraftDesign, String> {
    read_solidcraft(include_bytes!(concat!("fixtures/solidcraft/", name)).as_slice())
}

fn body_names(doc: &Value) -> Result<Vec<String>, String> {
    let mut names = Vec::new();
    let features = doc
        .get("features")
        .and_then(Value::as_array)
        .ok_or_else(|| "features must be a list".to_string())?;
    for feature in features {
        if let Some(meshes) = feature.get("meshes").and_then(Value::as_array) {
            for mesh in meshes {
                if let Some(name) = mesh.get("name").and_then(Value::as_str) {
                    names.push(name.to_string());
                }
            }
        }
    }
    names.sort();
    Ok(names)
}

#[test]
fn solidcraft_fixture_part_reads() -> Check {
    let design = read_fixture("part_two_bodies.solidcraft")?;
    let doc = design_as_value(&design)?;
    assert_eq!(doc.get("units").and_then(Value::as_str), Some("mm"));
    assert_eq!(body_names(&doc)?, ["Blue", "Red"]);
    let looks = doc
        .get("appearances")
        .and_then(|a| a.get("bodies"))
        .and_then(Value::as_object)
        .ok_or("appearances.bodies missing")?;
    assert_eq!(
        looks.get("Red").and_then(|v| v.get("color")).and_then(Value::as_array).map(|a| a.len()),
        Some(3)
    );
    assert_eq!(
        looks.get("Blue").and_then(|v| v.get("color")).and_then(Value::as_array).map(|a| a.len()),
        Some(3)
    );
    Ok(())
}

#[test]
fn solidcraft_fixture_assembly_reads() -> Check {
    let design = read_fixture("assembly_two_occurrences.solidcraft")?;
    let (components, occurrences, bodies) = solidcraft_counts(&design)?;
    assert_eq!(components, 2);
    assert_eq!(occurrences, 2);
    assert_eq!(bodies, 2);
    let doc = design_as_value(&design)?;
    let names: Vec<&str> = doc
        .get("occurrences")
        .and_then(Value::as_array)
        .ok_or("occurrences missing")?
        .iter()
        .filter_map(|o| o.get("name").and_then(Value::as_str))
        .collect();
    assert_eq!(names, ["BrickA", "BrickB"]);
    Ok(())
}

#[test]
fn solidcraft_fixture_container_reads() -> Check {
    let design = read_fixture("container_mesh.solidcraft")?;
    let doc = design_as_value(&design)?;
    assert_eq!(body_names(&doc)?, ["Box"]);
    let positions = doc
        .get("features")
        .and_then(Value::as_array)
        .and_then(|f| f.first())
        .and_then(|f| f.get("meshes"))
        .and_then(|m| m.as_array())
        .and_then(|m| m.first())
        .and_then(|m| m.get("positions"))
        .and_then(Value::as_array)
        .ok_or("hydrated positions missing")?;
    assert_eq!(positions.len(), 9, "container mesh should hydrate to nine coordinates");
    Ok(())
}

#[test]
fn export_fixture_matches_expected_counts() -> Check {
    let design = read_fixture("board_mini.solidcraft")?;
    let (components, occurrences, bodies) = solidcraft_counts(&design)?;
    assert_eq!(components, 1);
    assert_eq!(occurrences, 1);
    assert_eq!(bodies, 2);
    Ok(())
}

#[test]
fn export_then_read_back_preserves_parts() -> Check {
    for seed in 0..8u64 {
        let project = random_board(seed)?;
        let mesh = board_mesh(&project, MeshLayers::default())?;
        let text = solidcraft_board(&project, &mesh, SolidcraftLayers::default())?;
        let exported = read_solidcraft(text.as_bytes())?;
        let round = read_solidcraft(write_solidcraft_json(&exported)?.as_bytes())?;
        assert!(designs_equal(&exported, &round));
        let before = part_snapshots(&exported)?;
        let after = part_snapshots(&round)?;
        assert_eq!(before.len(), after.len());
        for (a, b) in before.iter().zip(&after) {
            assert_eq!(a.name, b.name);
            assert!(transforms_close(&a.transform, &b.transform, 1e-9));
            assert!(bounds_close(&a.bounds, &b.bounds, 1e-6));
        }
        let container = write_solidcraft_container(&exported)?;
        let from_container = read_solidcraft(&container)?;
        let again = part_snapshots(&from_container)?;
        assert_eq!(before.len(), again.len());
    }
    Ok(())
}

fn random_board(seed: u64) -> Result<Project, String> {
    let definition = pcbcraft_library::read_symbols(include_str!(
        "../../library/tests/fixtures/device.kicad_sym"
    ))
    .map_err(|e| e.to_string())?
    .into_iter()
    .find(|s| s.name == "R")
    .ok_or("fixture resistor missing")?;
    let footprint = pcbcraft_library::read_footprint(include_str!(
        "../../library/tests/fixtures/R_0805_2012Metric.kicad_mod"
    ))
    .map_err(|e| e.to_string())?;
    let pads: Vec<pcbcraft_doc::Pad> = footprint
        .pads
        .iter()
        .map(|p| pcbcraft_doc::Pad {
            number: p.number.clone(),
            offset: p.position,
            diameter: p.size.x.min(p.size.y),
            net: None,
            castellated: None,
            geometry: Some(p.clone()),
        })
        .collect();
    let pins: Vec<pcbcraft_doc::Pin> = definition
        .pins
        .iter()
        .filter(|p| p.unit <= 1 && p.body_style <= 1)
        .map(|p| pcbcraft_doc::Pin {
            number: p.number.clone(),
            name: p.name.clone(),
            offset: p.position,
            net: None,
            electrical_type: p.electrical_type.clone(),
            direction: Some(p.angle),
            unit: p.unit,
        })
        .collect();
    let mut project = Project::default();
    project.board.width = 40.0 + (seed % 5) as f64 * 4.0;
    project.board.height = 30.0 + (seed % 3) as f64 * 5.0;
    project.name = format!("seed-{seed}");
    let count = 1 + (seed % 3) as usize;
    for n in 0..count {
        let id = 1 + 2 * n as u64;
        let reference = format!("R{}", n + 1);
        let x = 8.0 + (seed.wrapping_mul(17 + n as u64) % 200) as f64 / 10.0;
        let y = 6.0 + (seed.wrapping_mul(23 + n as u64) % 150) as f64 / 10.0;
        let rotation = (seed.wrapping_mul(11 + n as u64) % 360) as f64;
        project.components.push(pcbcraft_doc::Component {
            sheet: 0,
            id,
            reference: reference.clone(),
            value: "1k".into(),
            kind: "library".into(),
            position: Point::new(20.0, 20.0),
            rotation: 0.0,
            pins: pins.clone(),
            units: vec![],
            library_symbol: Some(pcbcraft_doc::LibrarySymbol {
                library: "Device".into(),
                unit: 1,
                definition: definition.clone(),
            }),
            library_footprint: Some(pcbcraft_doc::LibraryFootprint {
                library: "Resistor_SMD".into(),
                definition: footprint.clone(),
            }),
            fields: vec![],
        });
        project.board.footprints.push(pcbcraft_doc::Footprint {
            id: id + 1,
            component: id,
            position: Point::new(x, y),
            rotation,
            locked: None,
            side: if n % 2 == 0 { None } else { Some(Layer::Back) },
            pads: pads.clone(),
        });
    }
    project.validate()?;
    Ok(project)
}
