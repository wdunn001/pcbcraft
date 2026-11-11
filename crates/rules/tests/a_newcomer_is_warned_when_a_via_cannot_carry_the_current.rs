//! A newcomer is warned when a via cannot carry the current.
use pcbcraft_doc::{Id, LayerRecord, Net, Project, Via};
use pcbcraft_geom::Point;
use pcbcraft_rules::drc;

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
fn a_newcomer_is_warned_when_a_via_cannot_carry_the_current() -> T {
    // The newcomer is routing a 5 A power rail through a single standard via.
    // They want to know if the via can handle the current.
    let mut p = four_layer_stack();
    p.nets.push(Net {
        id: 1,
        name: "VCC".into(),
        current: Some(5.0),
        temperature_rise: Some(10.0),
        ..Default::default()
    });
    p.board.vias.push(standard_via(10, 1));

    // They run the design rule check.
    let findings = drc(&p);

    // The DRC warns them that the via cannot carry 5 A.
    let via_warnings: Vec<_> = findings
        .iter()
        .filter(|f| f.code == "via_current_advice")
        .collect();
    assert_eq!(via_warnings.len(), 1, "one via capacity warning is issued");
    assert_eq!(via_warnings[0].severity, "warning");
    // The message should mention the current and suggest more vias.
    let msg = &via_warnings[0].message;
    assert!(
        msg.contains("current") || msg.contains("5.0") || msg.contains("via"),
        "the warning mentions current or vias"
    );
    Ok(())
}
