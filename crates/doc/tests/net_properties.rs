//! Net electrical property storage and class override resolution.
use pcbcraft_doc::{Id, Net, NetClass, Project};
use serde_json::Value;

type T = Result<(), String>;

#[test]
fn an_old_net_loads_without_the_new_properties() -> T {
    let raw = r#"{"nets":[{"id":1,"name":"SIG"}],"net_classes":[]}"#;
    let p: Project = serde_json::from_str(raw).map_err(|e| e.to_string())?;
    assert_eq!(p.nets.len(), 1);
    let net = &p.nets[0];
    assert!(net.impedance_target.is_none());
    assert!(net.reference_layers.is_none());
    let value = serde_json::to_value(&p).map_err(|e| e.to_string())?;
    let keys: Vec<&String> = value["nets"][0]
        .as_object()
        .ok_or("net object")?
        .keys()
        .collect();
    assert!(
        keys.iter()
            .all(|k| matches!(k.as_str(), "id" | "name")),
        "new fields leaked into an old-shape net: {keys:?}"
    );
    Ok(())
}

#[test]
fn a_net_value_overrides_the_class_value_and_clearing_it_falls_back() -> T {
    let mut p = Project::default();
    p.nets.push(Net {
        id: 1,
        name: "SIG".into(),
        ..Default::default()
    });
    p.net_classes.push(NetClass {
        id: 10,
        name: "RF".into(),
        clearance: 0.2,
        width: 0.25,
        min_width: None,
        max_width: None,
        target_length: None,
        length_tolerance: None,
        max_stub: None,
        via_diameter: None,
        via_drill: None,
        pair_width: None,
        pair_gap: None,
        nets: vec![1],
        profile: None,
        impedance_target: Some(50.0),
        impedance_tolerance_pct: Some(10.0),
        line_structure: None,
        reference_layers: vec!["F.Cu".into()],
        current: Some(2.0),
        temperature_rise: None,
        working_voltage: None,
        insulation: None,
        return_path_rule: None,
    });
    let net = p.nets.first().ok_or("net")?;
    assert_eq!(p.net_effective_impedance_target(net), Some(50.0));
    assert_eq!(p.net_effective_current(net), Some(2.0));
    p.nets[0].impedance_target = Some(75.0);
    p.nets[0].current = Some(0.5);
    let net = &p.nets[0];
    assert_eq!(p.net_effective_impedance_target(net), Some(75.0));
    assert_eq!(p.net_effective_current(net), Some(0.5));
    p.nets[0].impedance_target = None;
    p.nets[0].current = None;
    let net = &p.nets[0];
    assert_eq!(p.net_effective_impedance_target(net), Some(50.0));
    assert_eq!(p.net_effective_current(net), Some(2.0));
    p.nets[0].reference_layers = Some(vec!["B.Cu".into()]);
    assert_eq!(
        p.net_effective_reference_layers(net),
        &["B.Cu".to_string()][..]
    );
    p.nets[0].reference_layers = None;
    assert_eq!(
        p.net_effective_reference_layers(net),
        &["F.Cu".to_string()][..]
    );
    let _: Value = serde_json::to_value(&p).map_err(|e| e.to_string())?;
    Ok(())
}
