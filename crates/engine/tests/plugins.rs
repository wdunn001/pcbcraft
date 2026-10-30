use pcbcraft_doc::{Component, Net, Pin, Project};
use pcbcraft_engine::{Error, Session};
use pcbcraft_geom::Point;
use serde_json::json;
use std::path::PathBuf;

fn fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../plugin/tests/fixtures")
}

fn wired_project() -> Result<Project, Error> {
    let mut p = Project::default();
    for (id, name) in [(1_u64, "GND"), (2, "SIG")] {
        p.nets.push(Net {
            id,
            name: name.into(),
            ..Default::default()
        });
    }
    p.components.push(Component {
        sheet: 0,
        id: 10,
        reference: "R1".into(),
        value: "1k".into(),
        kind: "resistor".into(),
        position: Point::new(0.0, 0.0),
        rotation: 0.0,
        pins: vec![
            Pin {
                number: "1".into(),
                name: String::new(),
                offset: Point::new(-1.0, 0.0),
                net: Some(2),
                electrical_type: "passive".into(),
                direction: None,
                unit: 1,
            },
            Pin {
                number: "2".into(),
                name: String::new(),
                offset: Point::new(1.0, 0.0),
                net: Some(1),
                electrical_type: "passive".into(),
                direction: None,
                unit: 1,
            },
        ],
        units: vec![],
        library_symbol: None,
        library_footprint: None,
        fields: vec![],
    });
    p.validate().map_err(|e| Error::Invalid(e.to_string()))?;
    Ok(p)
}

#[test]
fn plugins_list_finds_the_example_manifest() -> Result<(), Error> {
    let root = fixtures_root();
    let root_str = root.to_string_lossy();
    let mut s = Session::default();
    let out = s.execute("plugins.list", json!({"root": root_str.as_ref()}), None)?;
    let plugins = out["result"]["plugins"]
        .as_array()
        .ok_or_else(|| Error::Invalid("plugins array".into()))?;
    assert_eq!(plugins.len(), 1);
    assert_eq!(plugins[0]["id"], "example-pin-map");
    Ok(())
}

#[test]
fn plugins_export_returns_text_without_changing_the_project() -> Result<(), Error> {
    let root = fixtures_root();
    let root_str = root.to_string_lossy();
    let mut s = Session::default();
    s.project = wired_project()?;
    let rev = s.revision;
    let out = s.execute(
        "plugins.export",
        json!({
            "root": root_str.as_ref(),
            "plugin_id": "example-pin-map",
            "template_id": "pin-map-tsv",
        }),
        None,
    )?;
    assert_eq!(s.revision, rev);
    assert!(out["result"]["text"].as_str().is_some_and(|t| t.contains("SIG")));
    Ok(())
}
