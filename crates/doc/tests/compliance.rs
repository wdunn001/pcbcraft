use pcbcraft_doc::{
    ComplianceDeclaration, ComplianceRole, ComplianceSettings, Component, ComponentCompliance,
    CountryOfOrigin, DeclarationStatus, Project, validate_component_compliance,
};

#[test]
fn compliance_fields_round_trip_in_project_json() -> Result<(), String> {
    let mut project = Project::default();
    project.compliance.rule_sets.push("blue_uas_style_drone".into());
    project.compliance.target_markets.push("EU".into());
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
        compliance: Some(ComponentCompliance {
            role: ComplianceRole::FlightController,
            country_of_origin: Some(CountryOfOrigin {
                code: "US".into(),
                basis: "label".into(),
                as_of: "2026-01-01".into(),
            }),
            declarations: vec![ComplianceDeclaration {
                regime: "ndaa_848".into(),
                status: DeclarationStatus::DeclaredCompliant,
                as_of: "2026-01-01".into(),
                ..Default::default()
            }],
            ..Default::default()
        }),
    });
    project.validate()?;
    let json = serde_json::to_string(&project).map_err(|e| e.to_string())?;
    let back: Project = serde_json::from_str(&json).map_err(|e| e.to_string())?;
    back.validate()?;
    Ok(())
}

#[test]
fn compliance_bounds_refuse_oversized_declarations() {
    let mut compliance = ComponentCompliance::default();
    compliance.declarations = (0..65)
        .map(|i| ComplianceDeclaration {
            regime: format!("r{i}"),
            ..Default::default()
        })
        .collect();
    assert!(validate_component_compliance(&compliance).is_err());
}
