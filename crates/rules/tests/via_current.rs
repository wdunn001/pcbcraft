use pcbcraft_doc::{Id, LayerRecord, Net, Project, Via};
use pcbcraft_geom::Point;
use pcbcraft_rules::{drc, via_capacity};

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

fn standard_via(id: Id, net: Id) -> Via {
    Via {
        id,
        net,
        position: Point::new(0.0, 0.0),
        drill: 0.3,
        diameter: 0.6,
        preserved: Default::default(),
        from_layer: None,
        to_layer: None,
        covering: None,
        covering_bottom: None,
        backdrill: None,
    }
}

#[test]
fn via_capacity_is_positive_and_finite() -> T {
    let p = four_layer_stack();
    let via = standard_via(10, 0);
    let advice = via_capacity(&p, &via).ok_or("capacity for standard via")?;
    assert!(advice.current > 0.0);
    assert!(advice.current.is_finite());
    Ok(())
}

#[test]
fn a_via_carrying_too_much_current_gets_a_warning() -> T {
    let mut p = four_layer_stack();
    p.nets.push(Net {
        id: 1,
        name: "VCC".into(),
        current: Some(5.0),
        temperature_rise: Some(10.0),
        ..Default::default()
    });
    p.board.vias.push(standard_via(10, 1));
    let findings: Vec<_> = drc(&p)
        .into_iter()
        .filter(|f| f.code == "via_current_advice")
        .collect();
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].severity, "warning");
    Ok(())
}

#[test]
fn a_via_within_capacity_gives_no_warning() -> T {
    let mut p = four_layer_stack();
    p.nets.push(Net {
        id: 1,
        name: "VCC".into(),
        current: Some(0.5),
        temperature_rise: Some(10.0),
        ..Default::default()
    });
    p.board.vias.push(standard_via(10, 1));
    let findings: Vec<_> = drc(&p)
        .into_iter()
        .filter(|f| f.code == "via_current_advice")
        .collect();
    assert!(findings.is_empty());
    Ok(())
}

#[test]
fn no_current_gives_no_via_advice() -> T {
    let mut p = four_layer_stack();
    p.nets.push(Net {
        id: 1,
        name: "GND".into(),
        ..Default::default()
    });
    p.board.vias.push(standard_via(10, 1));
    let findings: Vec<_> = drc(&p)
        .into_iter()
        .filter(|f| f.code == "via_current_advice")
        .collect();
    assert!(findings.is_empty());
    Ok(())
}

#[test]
fn multiple_via_count_is_suggested() -> T {
    let mut p = four_layer_stack();
    p.nets.push(Net {
        id: 1,
        name: "VCC".into(),
        current: Some(10.0),
        temperature_rise: Some(10.0),
        ..Default::default()
    });
    p.board.vias.push(standard_via(10, 1));
    let via = &p.board.vias[0];
    let advice = via_capacity(&p, via).ok_or("capacity")?;
    assert!(if let Some(count) = advice.count {
        count > 1.0
    } else {
        false
    });
    Ok(())
}
