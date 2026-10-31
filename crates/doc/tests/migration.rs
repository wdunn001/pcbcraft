//! Migration and layer validation tests for multi-layer boards.
//!
//! Verifies that:
//! - Every example file loads and validates
//! - Two-layer files round-trip byte-equal
//! - Tracks on inner layers validate against the stackup
//! - Tracks on invalid layers are rejected

use pcbcraft_doc::{Layer, LayerRecord, Net, Project, Track};
use pcbcraft_geom::Point;
use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn stackup(copper: u8) -> Project {
    let mut p = Project::default();
    if copper > 2 {
        for i in 0..copper {
            let name = match i {
                0 => "F.Cu".to_string(),
                i if i + 1 == copper => "B.Cu".to_string(),
                i => format!("In{i}.Cu"),
            };
            p.board.layer_records.push(LayerRecord {
                name,
                kind: "copper".into(),
                thickness: 0.035,
                ..LayerRecord::default()
            });
        }
    }
    p
}

fn project_with_net(copper: u8) -> Project {
    let mut p = stackup(copper);
    p.nets.push(Net {
        id: 10,
        name: "SIG".into(),
        ..Default::default()
    });
    p
}

#[test]
fn all_example_files_load_and_validate() {
    let examples_dir = root().join("examples");
    let entries = match std::fs::read_dir(&examples_dir) {
        Ok(e) => e,
        Err(e) => {
            assert!(false, "examples dir: {}", e);
            return;
        }
    };

    let mut loaded = 0;
    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                assert!(false, "entry: {}", e);
                return;
            }
        };
        let path = entry.path();
        if !path.extension().map_or(false, |e| e == "pcbcraft") {
            continue;
        }

        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                assert!(false, "read {}: {}", path.display(), e);
                return;
            }
        };
        let project: Project = match serde_json::from_str(&content) {
            Ok(p) => p,
            Err(e) => {
                assert!(false, "deserialize {}: {}", path.display(), e);
                return;
            }
        };

        // Validate the loaded project
        let result = project.validate();
        if let Err(msg) = result {
            assert!(false, "{}: validation failed: {}", path.display(), msg);
            return;
        }

        loaded += 1;
    }

    assert!(loaded > 0, "no example files found");
}

#[test]
fn two_layer_file_round_trips_byte_equal() -> Result<(), String> {
    let mut p = project_with_net(2);
    // Add a track to make it non-trivial
    p.board.tracks.push(Track {
        id: 1,
        net: 10,
        layer: Layer::from_index(0)?,
        start: Point { x: 0.0, y: 0.0 },
        end: Point { x: 1.0, y: 0.0 },
        width: 0.2,
        arc: None,
    });

    let first = serde_json::to_string(&p).map_err(|e| e.to_string())?;
    let reloaded: Project = serde_json::from_str(&first).map_err(|e| e.to_string())?;
    let second = serde_json::to_string(&reloaded).map_err(|e| e.to_string())?;

    assert_eq!(first, second, "two-layer file should round-trip byte-equal");
    Ok(())
}

#[test]
fn inner_layer_track_validates_on_four_layer_board() -> Result<(), String> {
    let mut p = project_with_net(4);
    // Layer 2 is In2.Cu on a four-layer board
    let layer = Layer::from_index(2)?;
    p.board.tracks.push(Track {
        id: 1,
        net: 10,
        layer,
        start: Point { x: 0.0, y: 0.0 },
        end: Point { x: 1.0, y: 0.0 },
        width: 0.2,
        arc: None,
    });

    let result = p.validate();
    assert!(
        result.is_ok(),
        "track on In2.Cu should validate on four-layer board: {:?}",
        result
    );
    Ok(())
}

#[test]
fn inner_layer_track_fails_on_two_layer_board() -> Result<(), String> {
    let mut p = project_with_net(2);
    // Layer 2 does not exist on a two-layer board
    let layer = Layer::from_index(2)?;
    p.board.tracks.push(Track {
        id: 1,
        net: 10,
        layer,
        start: Point { x: 0.0, y: 0.0 },
        end: Point { x: 1.0, y: 0.0 },
        width: 0.2,
        arc: None,
    });

    let result = p.validate();
    assert!(
        result.is_err(),
        "track on layer 2 should fail on two-layer board"
    );

    // Extract the error message
    let err = match result {
        Ok(()) => return Err("expected validation error".to_string()),
        Err(e) => e,
    };

    assert!(
        err.contains("layer") || err.contains("Layer"),
        "should have a layer validation issue: {}",
        err
    );
    Ok(())
}
