use pcbcraft_doc::{
    Compliance, ComplianceProjectSettings, CountryOfOrigin, Declaration, DeclarationSource,
    DeclarationSourceKind, DeclarationStatus, Project,
};

fn demo_project() -> Project {
    let mut p = Project::default();
    p.compliance_settings = ComplianceProjectSettings {
        rule_sets: vec!["synthetic_demo".into()],
        ..ComplianceProjectSettings::default()
    };
    let mut c = pcbcraft_doc::component_template(
        "ic",
        1,
        "U1".into(),
        pcbcraft_geom::Point::default(),
    )
    .expect("template");
    c.fields.push(pcbcraft_doc::ComponentField {
        key: "Manufacturer".into(),
        value: "Synthetic Entity Alpha".into(),
    });
    p.components.push(c);
    p.components.push(
        pcbcraft_doc::component_template(
            "resistor",
            2,
            "R1".into(),
            pcbcraft_geom::Point::default(),
        )
        .expect("template"),
    );
    p.validate().expect("valid project");
    p
}

fn user_source() -> DeclarationSource {
    DeclarationSource {
        kind: DeclarationSourceKind::User,
        label: "lab".into(),
        url: None,
        document_ref: None,
    }
}

#[test]
fn fixture_rule_set_yields_hit_unverified_and_stale() {
    let p = demo_project();
    let findings = pcbcraft_rules::compliance_check(&p);
    let states: Vec<_> = findings.iter().map(|f| f.state.as_str()).collect();
    assert!(
        states.contains(&"hit"),
        "listed manufacturer should hit: {findings:?}"
    );
    assert!(
        states.contains(&"unverified"),
        "missing declarations should stay unverified: {findings:?}"
    );
    assert!(
        findings.iter().all(|f| !f.citation.is_empty()),
        "every finding cites a source"
    );

    let mut stale_project = demo_project();
    let part = stale_project
        .components
        .iter_mut()
        .find(|c| c.id == 1)
        .expect("U1");
    part.compliance = Compliance {
        declarations: vec![Declaration {
            regime: "demo.list_regime".into(),
            status: DeclarationStatus::DeclaredCompliant,
            detail: String::new(),
            source: user_source(),
            as_of: "2026-01-01".into(),
        }],
        ..Compliance::default()
    };
    stale_project.validate().expect("valid");
    let stale = pcbcraft_rules::compliance_check(&stale_project);
    assert!(
        stale.iter().any(|f| f.state == "stale"),
        "old declaration against list should stale: {stale:?}"
    );
}

#[test]
fn missing_data_never_reports_pass() {
    let p = demo_project();
    let findings = pcbcraft_rules::compliance_check(&p);
    assert!(
        !findings.iter().any(|f| f.state == "pass"),
        "compliance never emits a plain pass"
    );
    assert!(
        findings
            .iter()
            .any(|f| f.state == "unverified" && f.rule == "demo.declaration_required"),
        "missing regime declaration stays unverified"
    );
}

#[test]
fn rule_set_choice_changes_visible_rows() {
    let bare = demo_project();
    let none = pcbcraft_rules::compliance_check(&bare);
    let mut other = demo_project();
    other.compliance_settings.rule_sets = vec![];
    other.validate().expect("valid");
    let empty = pcbcraft_rules::compliance_check(&other);
    assert!(empty.is_empty());
    assert!(!none.is_empty());
}

#[test]
fn origin_unverified_without_country() {
    let p = demo_project();
    let origin = pcbcraft_rules::compliance_check(&p)
        .into_iter()
        .find(|f| f.rule == "demo.origin")
        .expect("origin rule runs");
    assert_eq!(origin.state, "unverified");

    let mut with_country = demo_project();
    let part = with_country.components.iter_mut().find(|c| c.id == 2).expect("R1");
    part.compliance.country_of_origin = Some(CountryOfOrigin {
        code: "xx".into(),
        basis: String::new(),
        as_of: String::new(),
    });
    with_country.validate().expect("valid");
    let hit = pcbcraft_rules::compliance_check(&with_country)
        .into_iter()
        .find(|f| f.rule == "demo.origin" && f.objects == vec![2])
        .expect("restricted country hits");
    assert_eq!(hit.state, "hit");
}
