use pcbcraft_doc::{
    Compliance, ComplianceProjectSettings, Component, ComponentField, ManufacturerRecord, PartRole,
    Project,
};
use pcbcraft_rules::compliance_check;

fn sample_project() -> Project {
    let mut project = Project::default();
    project.compliance_settings = ComplianceProjectSettings {
        rule_sets: vec!["us_government_ndaa".into()],
    };
    project.manufacturers.push(ManufacturerRecord {
        name: "Synthetic Telecom Vendor Orion".into(),
        aliases: vec!["Orion Telecom Demo".into()],
        country: String::new(),
        parent: None,
    });
    project
}

fn component(id: u64, reference: &str, manufacturer: Option<&str>, compliance: Option<Compliance>) -> Component {
    let mut fields = Vec::new();
    if let Some(m) = manufacturer {
        fields.push(ComponentField {
            key: "Manufacturer".into(),
            value: m.into(),
        });
    }
    Component {
        id,
        reference: reference.into(),
        value: "part".into(),
        kind: "resistor".into(),
        position: pcbcraft_geom::Point::new(0.0, 0.0),
        rotation: 0.0,
        pins: vec![],
        units: vec![],
        library_symbol: None,
        library_footprint: None,
        fields,
        compliance,
        sheet: 0,
    }
}

#[test]
fn a_newcomer_sees_a_listed_maker_flagged_on_the_us_government_rule_set() {
    let mut project = sample_project();
    project.components.push(component(
        1,
        "U1",
        Some("Orion Telecom Demo"),
        None,
    ));
    project.components.push(component(
        2,
        "U2",
        Some("Synthetic Near Match Ech"),
        None,
    ));
    project.components.push(component(3, "U3", None, None));
    project.components.push(component(
        4,
        "U4",
        Some("Example Components"),
        Some(Compliance {
            role: Some(PartRole::Processor),
            silicon_vendor: None,
        }),
    ));
    project.components.push(component(
        5,
        "U5",
        Some("Example Components"),
        Some(Compliance {
            role: Some(PartRole::Memory),
            silicon_vendor: Some("Synthetic Semiconductor Vendor Delta".into()),
        }),
    ));

    let findings = compliance_check(&project);
    let hit = findings
        .iter()
        .find(|f| f.objects == vec![1] && f.state == "hit")
        .expect("listed maker via alias produces a hit");
    assert!(hit.evidence.contains("us-ndaa-synthetic-entities"));
    assert!(hit.evidence.contains("2026-06-01"));
    assert!(hit.evidence.contains("Synthetic Telecom Vendor Orion"));

    let possible = findings
        .iter()
        .find(|f| f.objects == vec![2] && f.state == "possible")
        .expect("near name produces a possible match, not a hit");
    assert!(possible.message.contains("Possible"));

    let unverified_mfg = findings
        .iter()
        .find(|f| f.objects == vec![3] && f.state == "unverified")
        .expect("missing manufacturer stays unverified");
    assert!(!unverified_mfg.message.is_empty());

    let unverified_silicon = findings
        .iter()
        .find(|f| f.objects == vec![4] && f.state == "unverified")
        .expect("processor without silicon vendor stays unverified");
    assert!(unverified_silicon.message.contains("silicon vendor"));

    let silicon_hit = findings
        .iter()
        .find(|f| f.objects == vec![5] && f.state == "hit")
        .expect("recorded silicon vendor on memory part can hit the list");
    assert!(silicon_hit.evidence.contains("Synthetic Semiconductor Vendor Delta"));
}
