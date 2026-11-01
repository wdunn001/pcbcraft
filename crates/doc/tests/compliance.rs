use pcbcraft_doc::{
    Compliance, Component, CountryOfOrigin, Declaration, DeclarationSource, DeclarationSourceKind,
    DeclarationStatus, PartRole, Project,
};

fn user_source() -> DeclarationSource {
    DeclarationSource {
        kind: DeclarationSourceKind::User,
        label: String::new(),
        url: None,
        document_ref: None,
    }
}

#[test]
fn compliance_fields_round_trip_in_project_json() -> Result<(), String> {
    let mut project = Project::default();
    project
        .compliance_settings
        .rule_sets
        .push("blue_uas_style_drone".into());
    project.compliance_settings.target_markets.push("EU".into());
    project.components.push(Component {
        id: 1,
        reference: "U1".into(),
        value: "FC".into(),
        kind: "ic".into(),
        position: Default::default(),
        rotation: 0.0,
        pins: vec![],
        units: vec![],
        library_symbol: None,
        library_footprint: None,
        fields: vec![],
        sheet: 0,
        compliance: Compliance {
            role: Some(PartRole::FlightController),
            country_of_origin: Some(CountryOfOrigin {
                code: "US".into(),
                basis: "label".into(),
                as_of: "2026-01-01".into(),
            }),
            declarations: vec![Declaration {
                regime: "ndaa_848".into(),
                status: DeclarationStatus::DeclaredCompliant,
                detail: String::new(),
                source: user_source(),
                as_of: "2026-01-01".into(),
            }],
            ..Default::default()
        },
        part: None,
        sourcing: Default::default(),
        instance: Default::default(),
        prototype: Default::default(),
        include_in_bom: true,
        include_in_placement: true,
    });
    project.validate()?;
    let json = serde_json::to_string(&project).map_err(|e| e.to_string())?;
    let back: Project = serde_json::from_str(&json).map_err(|e| e.to_string())?;
    back.validate()?;
    assert_eq!(
        back.components[0].compliance.role,
        Some(PartRole::FlightController)
    );
    Ok(())
}

#[test]
fn compliance_bounds_refuse_oversized_declarations() {
    let mut compliance = Compliance::default();
    compliance.declarations = (0..65)
        .map(|i| Declaration {
            regime: format!("r{i}"),
            status: DeclarationStatus::Unknown,
            detail: String::new(),
            source: user_source(),
            as_of: String::new(),
        })
        .collect();
    assert!(compliance.validate().is_err());
}
