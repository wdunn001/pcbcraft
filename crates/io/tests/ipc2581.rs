//! IPC-2581C export tests (issue #660).

use pcbcraft_doc::{Layer, LayerRecord, Net, Project, Track, Via};
use pcbcraft_geom::Point;
use pcbcraft_io::ipc2581_text;

fn six_layer_board() -> Result<Project, String> {
    let mut p = Project::default();
    for (id, name) in [
        (1, "BLIND"),
        (2, "BURIED"),
        (3, "THROUGH"),
        (4, "TOP"),
        (5, "THRU"),
        (6, "BOT"),
    ] {
        p.nets.push(Net {
            id,
            name: name.into(),
            ..Default::default()
        });
    }
    p.board.layer_records = ["F.Cu", "In1.Cu", "In2.Cu", "In3.Cu", "In4.Cu", "B.Cu"]
        .iter()
        .map(|name| LayerRecord {
            name: (*name).into(),
            kind: "copper".into(),
            thickness: 0.035,
            copper_role: Some("signal".into()),
            ..LayerRecord::default()
        })
        .collect();
    p.board.vias.push(Via {
        id: 10,
        net: 1,
        position: Point::new(10.0, 10.0),
        diameter: 0.6,
        drill: 0.3,
        from_layer: Some(Layer::Front),
        to_layer: Some(Layer::from_index(2)?),
        covering: None,
    });
    p.board.vias.push(Via {
        id: 11,
        net: 2,
        position: Point::new(20.0, 10.0),
        diameter: 0.6,
        drill: 0.3,
        from_layer: Some(Layer::from_index(2)?),
        to_layer: Some(Layer::from_index(4)?),
        covering: None,
    });
    p.board.vias.push(Via {
        id: 12,
        net: 3,
        position: Point::new(30.0, 10.0),
        diameter: 0.6,
        drill: 0.3,
        from_layer: None,
        to_layer: None,
        covering: None,
    });
    p.validate()?;
    Ok(p)
}

#[test]
fn the_output_validates_against_the_ipc2581c_schema() -> Result<(), String> {
    let p = Project::default();
    let text = ipc2581_text(&p)?;
    ipc2581::validate(&text).map_err(|e| e.to_string())?;
    Ok(())
}

#[test]
fn blind_and_buried_vias_write_one_drill_layer_per_span() -> Result<(), String> {
    let p = six_layer_board()?;
    let text = ipc2581_text(&p)?;
    ipc2581::validate(&text).map_err(|e| e.to_string())?;
    assert_eq!(
        text.matches("layerFunction=\"DRILL\"").count(),
        3,
        "blind, buried and through spans each get a drill layer"
    );
    assert!(
        text.contains("fromLayer=\"F.Cu\" toLayer=\"In2.Cu\""),
        "blind span is recorded on its drill layer"
    );
    assert!(
        text.contains("fromLayer=\"In2.Cu\" toLayer=\"In4.Cu\""),
        "buried span is recorded on its drill layer"
    );
    assert!(
        text.contains("fromLayer=\"F.Cu\" toLayer=\"B.Cu\""),
        "through span is recorded on its drill layer"
    );
    Ok(())
}

#[test]
fn every_stackup_row_appears_once_with_its_function() -> Result<(), String> {
    let mut p = Project::default();
    p.board.layer_records = vec![
        LayerRecord {
            name: "F.Cu".into(),
            kind: "copper".into(),
            thickness: 0.035,
            copper_role: Some("plane".into()),
            ..LayerRecord::default()
        },
        LayerRecord {
            name: "Core".into(),
            kind: "core".into(),
            thickness: 1.0,
            material: Some("FR4".into()),
            dk: Some(4.5),
            df: Some(0.02),
            ..LayerRecord::default()
        },
        LayerRecord {
            name: "B.Cu".into(),
            kind: "copper".into(),
            thickness: 0.035,
            ..LayerRecord::default()
        },
    ];
    p.validate()?;
    let text = ipc2581_text(&p)?;
    ipc2581::validate(&text).map_err(|e| e.to_string())?;
    for row in &p.board.layer_records {
        assert_eq!(
            text.matches(&format!("<Layer name=\"{}\"", row.name))
                .count(),
            1,
            "cad layer list includes {}",
            row.name
        );
        assert_eq!(
            text.matches(&format!("<StackupLayer layerOrGroupRef=\"{}\"", row.name))
                .count(),
            1,
            "stackup lists {}",
            row.name
        );
        let function = match row.kind.as_str() {
            "copper" if row.copper_role.as_deref() == Some("plane") => "PLANE",
            "copper" => "SIGNAL",
            "core" => "DIELCORE",
            _ => "OTHER",
        };
        assert!(
            text.contains(&format!(
                "<Layer name=\"{}\" layerFunction=\"{function}\"",
                row.name
            )),
            "layer function is exported for {}",
            row.name
        );
        assert!(
            text.contains(&format!("<StackupLayer layerOrGroupRef=\"{}\"", row.name)),
            "stackup lists {}",
            row.name
        );
    }
    Ok(())
}

#[test]
fn the_old_custom_root_element_is_gone() -> Result<(), String> {
    let text = ipc2581_text(&Project::default())?;
    assert!(
        !text.contains("<EDA3D"),
        "the legacy root element must not appear"
    );
    assert!(
        text.contains("<IPC-2581 revision=\"C\""),
        "the document uses the IPC-2581C root"
    );
    Ok(())
}

#[test]
fn logical_nets_list_connected_pins() -> Result<(), String> {
    let mut p = Project::default();
    p.nets.push(Net {
        id: 1,
        name: "SIG".into(),
        ..Default::default()
    });
    p.components.push(pcbcraft_doc::Component {
        sheet: 0,
        id: 1,
        reference: "R1".into(),
        value: String::new(),
        kind: "resistor".into(),
        position: Point::default(),
        rotation: 0.0,
        pins: vec![pcbcraft_doc::Pin {
            number: "1".into(),
            name: String::new(),
            offset: Point::default(),
            net: Some(1),
            electrical_type: "passive".into(),
            direction: None,
            unit: 1,
        }],
        units: vec![],
        library_symbol: None,
        library_footprint: None,
        fields: vec![],
    });
    p.board.tracks.push(Track {
        id: 1,
        net: 1,
        start: Point::new(0.0, 0.0),
        end: Point::new(1.0, 0.0),
        width: 0.25,
        layer: Layer::Front,
        arc: None,
    });
    p.validate()?;
    let text = ipc2581_text(&p)?;
    assert!(text.contains("<LogicalNet name=\"SIG\">"));
    assert!(text.contains("<PinRef componentRef=\"R1\" pin=\"1\"/>"));
    assert!(text.contains("<LayerFeature layerRef=\"F.Cu\">"));
    Ok(())
}
