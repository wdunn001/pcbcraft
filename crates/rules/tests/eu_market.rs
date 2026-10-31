use pcbcraft_doc::{
    Compliance, Declaration, DeclarationSource, DeclarationSourceKind, DeclarationStatus, PartRole,
    Project,
};
use pcbcraft_rules::compliance_check;

fn sample_project() -> Project {
    let mut p = Project::default();
    p.compliance_settings.rule_sets = vec!["eu_market".into()];
    let c = pcbcraft_doc::component_template(
        "resistor",
        1,
        "R1".into(),
        pcbcraft_geom::Point::default(),
    )
    .expect("template");
    p.components.push(c);
    p
}

fn user_source() -> DeclarationSource {
    DeclarationSource {
        kind: DeclarationSourceKind::User,
        label: "designer".into(),
        url: None,
        document_ref: None,
    }
}

#[test]
fn bom_parts_without_declarations_surface_unverified_eu_market_rows() {
    let p = sample_project();
    let bare = compliance_check(&p);
    let unverified: Vec<_> = bare.iter().filter(|f| f.state == "unverified").collect();
    assert!(
        unverified.len() >= 3,
        "missing declarations should surface grouped unverified rows: {:?}",
        bare
    );
    assert!(
        unverified.iter().all(|f| !f.citation.is_empty()),
        "every rule cites a source"
    );

    let mut declared = sample_project();
    declared.components[0].compliance = Compliance {
        declarations: vec![
            Declaration {
                regime: "rohs".into(),
                status: DeclarationStatus::DeclaredCompliant,
                detail: String::new(),
                source: user_source(),
                as_of: "2026-03-01".into(),
            },
            Declaration {
                regime: "reach".into(),
                status: DeclarationStatus::DeclaredCompliant,
                detail: "2026-03".into(),
                source: user_source(),
                as_of: "2026-03-01".into(),
            },
            Declaration {
                regime: "halogen_free".into(),
                status: DeclarationStatus::DeclaredCompliant,
                detail: String::new(),
                source: user_source(),
                as_of: "2026-03-01".into(),
            },
        ],
        ..Compliance::default()
    };
    let clean = compliance_check(&declared);
    assert!(
        !clean.iter().any(|f| f.state == "unverified"),
        "declared parts should not stay unverified: {:?}",
        clean
    );

    let mut stale = sample_project();
    stale.components[0].compliance = Compliance {
        declarations: vec![Declaration {
            regime: "rohs".into(),
            status: DeclarationStatus::DeclaredExempt,
            detail: "6(a)".into(),
            source: user_source(),
            as_of: "2026-03-01".into(),
        }],
        ..Compliance::default()
    };
    let stale_findings = compliance_check(&stale);
    assert!(
        stale_findings.iter().any(|f| f.state == "stale"),
        "expired exemption should read stale: {:?}",
        stale_findings
    );

    let mut leaded = sample_project();
    leaded.board.finish = "HASL leaded".into();
    let leaded_findings = compliance_check(&leaded);
    assert!(
        leaded_findings
            .iter()
            .any(|f| f.code == "board_lead_free_finish" && f.state == "hit"),
        "leaded finish should flag the board: {:?}",
        leaded_findings
    );

    let mut exempt_board = leaded.clone();
    exempt_board.compliance_settings.board_rohs_exemption = Some("7(a)-I".into());
    let exempt_findings = compliance_check(&exempt_board);
    assert!(
        !exempt_findings
            .iter()
            .any(|f| f.code == "board_lead_free_finish"),
        "board exemption should quiet the leaded finish hit"
    );
}

#[test]
fn bundled_list_metadata_is_present() {
    let reach = pcbcraft_rules::load_list_snapshot(include_str!(
        "../data/compliance/lists/reach_svhc_candidate.toml"
    ))
    .expect("reach snapshot");
    assert!(!reach.source_url.is_empty());
    assert!(!reach.licence.is_empty());
    assert!(pcbcraft_doc::valid_date(&reach.retrieved));
}

#[test]
fn compliance_settings_round_trip_on_the_project() {
    let mut p = sample_project();
    p.components[0].compliance.role = Some(PartRole::Battery);
    let json = serde_json::to_string(&p).expect("serialize");
    let back: Project = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.compliance_settings.rule_sets, vec!["eu_market"]);
    assert_eq!(
        back.components[0].compliance.role,
        Some(PartRole::Battery)
    );
}
