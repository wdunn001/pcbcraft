use pcbcraft_doc::{Layer, LayerRecord, Net, Project, Track};
use pcbcraft_geom::Point;
use pcbcraft_rules::{drc, width_advice, width_advice_on_layer};

type T = Result<(), String>;

fn four_layer_stack() -> Project {
    let mut p = Project::default();
    p.board.layer_records = vec![
        LayerRecord {
            name: "F.Cu".into(),
            kind: "copper".into(),
            thickness: 0.035,
            copper_weight_oz: Some(1.0),
            side: Some("top".into()),
            ..LayerRecord::default()
        },
        LayerRecord {
            name: "In1.Cu".into(),
            kind: "copper".into(),
            thickness: 0.035,
            copper_weight_oz: Some(1.0),
            side: Some("internal".into()),
            ..LayerRecord::default()
        },
        LayerRecord {
            name: "In2.Cu".into(),
            kind: "copper".into(),
            thickness: 0.035,
            copper_weight_oz: Some(1.0),
            side: Some("internal".into()),
            ..LayerRecord::default()
        },
        LayerRecord {
            name: "B.Cu".into(),
            kind: "copper".into(),
            thickness: 0.035,
            copper_weight_oz: Some(1.0),
            side: Some("bottom".into()),
            ..LayerRecord::default()
        },
    ];
    p
}

#[test]
fn advice_matches_the_published_example_values() -> T {
    let mut p = four_layer_stack();
    p.nets.push(Net {
        id: 1,
        name: "VCC".into(),
        current: Some(1.0),
        temperature_rise: Some(10.0),
        ..Default::default()
    });
    p.board.tracks.push(Track {
        id: 10,
        net: 1,
        start: Point::new(0.0, 0.0),
        end: Point::new(10.0, 0.0),
        width: 0.5,
        layer: Layer::Front,
        arc: None,
    });
    let advice = width_advice(&p, 1).ok_or("advice when current is set")?;
    assert_eq!(advice.method, "ipc2221");
    assert!(
        (advice.width_mm - 0.3022).abs() < 0.001,
        "published 1 A external 1 oz 10 K example, got {}",
        advice.width_mm
    );
    Ok(())
}

#[test]
fn an_inner_layer_needs_a_wider_track_than_an_outer_layer() -> T {
    let mut p = four_layer_stack();
    p.nets.push(Net {
        id: 1,
        name: "PWR".into(),
        current: Some(2.0),
        temperature_rise: Some(10.0),
        ..Default::default()
    });
    let outer = width_advice_on_layer(&p, 1, Layer::Front).ok_or("outer advice")?;
    let inner = width_advice_on_layer(&p, 1, Layer::from_index(1).map_err(|e| e.to_string())?)
        .ok_or("inner advice")?;
    assert!(inner.width_mm > outer.width_mm + 0.01);
    assert_eq!(outer.method, "ipc2221");
    assert_eq!(inner.method, "ipc2221");
    Ok(())
}

#[test]
fn no_current_gives_no_advice() -> T {
    let mut p = Project::default();
    p.nets.push(Net {
        id: 1,
        name: "GND".into(),
        ..Default::default()
    });
    p.board.tracks.push(Track {
        id: 10,
        net: 1,
        start: Point::new(0.0, 0.0),
        end: Point::new(5.0, 0.0),
        width: 0.2,
        layer: Layer::Front,
        arc: None,
    });
    assert!(width_advice(&p, 1).is_none());
    Ok(())
}

#[test]
fn a_narrow_track_gets_a_warning_not_an_error() -> T {
    let mut p = four_layer_stack();
    p.nets.push(Net {
        id: 1,
        name: "VCC".into(),
        current: Some(1.0),
        temperature_rise: Some(10.0),
        ..Default::default()
    });
    p.board.tracks.push(Track {
        id: 10,
        net: 1,
        start: Point::new(0.0, 0.0),
        end: Point::new(10.0, 0.0),
        width: 0.15,
        layer: Layer::Front,
        arc: None,
    });
    let findings: Vec<_> = drc(&p)
        .into_iter()
        .filter(|f| f.code == "track_width_advice")
        .collect();
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].severity, "warning");
    Ok(())
}
