use pcbcraft_doc::{
    Compliance, ComplianceFindingMute, ComplianceOrgRole, ComplianceProjectSettings, CountryOfOrigin,
    Declaration, DeclarationSource, DeclarationSourceKind, DeclarationStatus, ManufacturerRecord,
    PartRole, Project, Sourcing, apply_listing_declarations, normalize_manufacturer_name,
    MAX_COMPLIANCE_TEXT, MAX_DECLARATIONS,
};

#[test]
fn compliance_round_trips_and_old_files_load_without_compliance_fields() -> Result<(), String> {
    let mut p = Project::default();
    p.components.push(pcbcraft_doc::Component {
        id: 1,
        reference: "U1".into(),
        value: "MCU".into(),
        kind: "ic".into(),
        position: pcbcraft_geom::Point::new(0.0, 0.0),
        rotation: 0.0,
        pins: vec![],
        units: vec![],
        library_symbol: None,
        library_footprint: None,
        fields: vec![],
        sourcing: Default::default(),
        compliance: Compliance {
            country_of_origin: Some(CountryOfOrigin {
                code: "us".into(),
                basis: "manufactured".into(),
                as_of: "2026-01-15".into(),
            }),
            role: Some(PartRole::FlightController),
            declarations: vec![Declaration {
                regime: "rohs".into(),
                status: DeclarationStatus::DeclaredCompliant,
                detail: String::new(),
                source: DeclarationSource {
                    kind: DeclarationSourceKind::User,
                    label: "lab check".into(),
                    url: None,
                    document_ref: None,
                },
                as_of: "2026-01-15".into(),
            }],
            substances: vec![],
        },
        sheet: 0,
    });
    p.validate()?;
    let text = serde_json::to_string(&p).map_err(|e| e.to_string())?;
    assert!(text.contains("\"flight_controller\"") && text.contains("\"declared_compliant\""));
    let back: Project = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    assert_eq!(back.components[0].compliance.role, Some(PartRole::FlightController));
    let mut value: serde_json::Value = serde_json::to_value(&p).map_err(|e| e.to_string())?;
    let comp = value["components"][0]
        .as_object_mut()
        .ok_or_else(|| "component missing".to_string())?;
    comp.remove("compliance");
    comp.remove("sourcing");
    let loaded: Project = serde_json::from_value(value).map_err(|e| e.to_string())?;
    assert!(loaded.components[0].compliance.is_empty());
    loaded.validate()?;
    Ok(())
}

#[test]
fn compliance_bounds_refuse_oversized_payloads() -> Result<(), String> {
    let big = "x".repeat(MAX_COMPLIANCE_TEXT + 1);
    let mut compliance = Compliance::default();
    compliance.declarations.push(Declaration {
        regime: big,
        status: DeclarationStatus::Unknown,
        detail: String::new(),
        source: DeclarationSource {
            kind: DeclarationSourceKind::User,
            label: "x".into(),
            url: None,
            document_ref: None,
        },
        as_of: String::new(),
    });
    assert!(compliance.validate().is_err());
    compliance = Compliance::default();
    compliance.declarations = (0..=MAX_DECLARATIONS)
        .map(|i| Declaration {
            regime: format!("regime{i}"),
            status: DeclarationStatus::Unknown,
            detail: String::new(),
            source: DeclarationSource {
                kind: DeclarationSourceKind::User,
                label: "x".into(),
                url: None,
                document_ref: None,
            },
            as_of: String::new(),
        })
        .collect();
    assert!(compliance.validate().is_err());
    Ok(())
}

#[test]
fn manufacturer_alias_normalisation_resolves_names() -> Result<(), String> {
    assert_eq!(
        normalize_manufacturer_name("Acme Corp."),
        normalize_manufacturer_name("acme corp")
    );
    let mut p = Project::default();
    p.upsert_manufacturer(ManufacturerRecord {
        name: "Acme Corp".into(),
        aliases: vec!["ACME International".into()],
        country: "US".into(),
        parent: None,
        supplier_responsibility_on_file: true,
    })?;
    assert!(p.resolve_manufacturer("acme corp.").is_some());
    assert!(p.resolve_manufacturer("ACME International").is_some());
    assert!(p.resolve_manufacturer("Other").is_none());
    Ok(())
}

#[test]
fn quick_sourcing_flags_become_declarations_without_overriding_explicit_ones() -> Result<(), String> {
    let mut c = pcbcraft_doc::Component {
        id: 1,
        reference: "R1".into(),
        value: "10k".into(),
        kind: "resistor".into(),
        position: pcbcraft_geom::Point::new(0.0, 0.0),
        rotation: 0.0,
        pins: vec![],
        units: vec![],
        library_symbol: None,
        library_footprint: None,
        fields: vec![],
        sourcing: Sourcing {
            rohs: Some(true),
            reach: Some(false),
            ..Default::default()
        },
        compliance: Compliance {
            declarations: vec![Declaration {
                regime: "rohs".into(),
                status: DeclarationStatus::DeclaredExempt,
                detail: "legacy".into(),
                source: DeclarationSource {
                    kind: DeclarationSourceKind::Manufacturer,
                    label: "maker".into(),
                    url: None,
                    document_ref: None,
                },
                as_of: "2025-06-01".into(),
            }],
            ..Default::default()
        },
        sheet: 0,
    };
    let effective = c.effective_declarations();
    assert_eq!(effective.len(), 2);
    assert_eq!(effective[0].status, DeclarationStatus::DeclaredExempt);
    assert_eq!(effective[1].regime, "reach");
    let mut compliance = Compliance::default();
    apply_listing_declarations(
        &mut compliance,
        "2026-02-01",
        "Sample Shop",
        Some(true),
        None,
        Some(true),
    )?;
    assert_eq!(compliance.declarations.len(), 2);
    assert_eq!(
        compliance.declarations[0].source.kind,
        DeclarationSourceKind::Distributor
    );
    Ok(())
}

#[test]
fn compliance_settings_and_mutes_round_trip() -> Result<(), String> {
    let mut p = Project::default();
    p.compliance_settings = ComplianceProjectSettings {
        rule_sets: vec!["synthetic_demo".into()],
        org_role: Some(ComplianceOrgRole::Buyer),
        target_markets: vec!["demo-market".into()],
    };
    p.compliance_mutes.push(ComplianceFindingMute {
        rule: "demo.declaration_required".into(),
        component: Some(1),
        manufacturer: None,
        reason: "prototype waiver".into(),
    });
    p.components.push(pcbcraft_doc::Component {
        id: 1,
        reference: "R1".into(),
        value: "1k".into(),
        kind: "resistor".into(),
        position: pcbcraft_geom::Point::default(),
        rotation: 0.0,
        pins: vec![],
        units: vec![],
        library_symbol: None,
        library_footprint: None,
        fields: vec![],
        sourcing: Default::default(),
        compliance: Default::default(),
        sheet: 0,
    });
    p.validate()?;
    let text = serde_json::to_string(&p).map_err(|e| e.to_string())?;
    let back: Project = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    assert_eq!(back.compliance_settings.org_role, Some(ComplianceOrgRole::Buyer));
    assert_eq!(back.compliance_mutes[0].reason, "prototype waiver");
    Ok(())
}
