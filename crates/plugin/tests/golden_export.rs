use pcbcraft_doc::{Component, Net, Pin, Project};
use pcbcraft_geom::Point;
use pcbcraft_plugin::{
    export_output_name, export_template_path, load_export_template, load_registry, render_pin_map,
};
use std::path::PathBuf;

fn sample_project() -> Result<Project, String> {
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
    p.components.push(Component {
        sheet: 0,
        id: 11,
        reference: "R2".into(),
        value: "1k".into(),
        kind: "resistor".into(),
        position: Point::new(5.0, 0.0),
        rotation: 0.0,
        pins: vec![Pin {
            number: "1".into(),
            name: String::new(),
            offset: Point::new(0.0, 0.0),
            net: Some(2),
            electrical_type: "passive".into(),
            direction: None,
            unit: 1,
        }],
        units: vec![],
        library_symbol: None,
        library_footprint: None,
        fields: vec![],
    });
    p.validate().map_err(|e| e.to_string())?;
    Ok(p)
}

#[test]
fn registry_loads_example_plugin_and_golden_export_matches() -> Result<(), String> {
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let root = fixtures.join("example-pin-map");
    let listed = load_registry(&fixtures)?;
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, "example-pin-map");
    let template_path = export_template_path(&listed[0], "pin-map-tsv")?;
    let template = load_export_template(&template_path)?;
    let text = render_pin_map(&sample_project()?, &template)?;
    let expected =
        std::fs::read_to_string(root.join("expected/pin-map.tsv")).map_err(|e| e.to_string())?;
    assert_eq!(text, expected);
    assert_eq!(
        export_output_name(&listed[0], "pin-map-tsv")?,
        "pin-map.tsv"
    );
    Ok(())
}
