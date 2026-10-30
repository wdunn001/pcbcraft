use pcbcraft_doc::{
    Compliance, DeclaredSubstance, Declaration, DeclarationSource, DeclarationSourceKind,
    DeclarationStatus, PartRole, Project,
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

fn set_compliance(project: &mut Project, compliance: Compliance) {
    if let Some(c) = project.components.iter_mut().find(|c| c.id == 1) {
        c.compliance = compliance;
    }
}

#[test]
fn a_newcomer_checks_a_board_against_the_eu_market_rule_set() {
    let p = sample_project();
    let bare = compliance_check(&p);
    let unverified: Vec<_> = bare
        .iter()
        .filter(|f| f.state == "unverified")
        .collect();
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
    set_compliance(
        &mut declared,
        Compliance {
            declarations: vec![
                Declaration {
                    regime: "eu.rohs".into(),
                    status: DeclarationStatus::DeclaredCompliant,
                    detail: String::new(),
                    source: user_source(),
                    as_of: "2026-03-01".into(),
                },
                Declaration {
                    regime: "eu.reach.svhc".into(),
                    status: DeclarationStatus::DeclaredCompliant,
                    detail: "2026-03".into(),
                    source: user_source(),
                    as_of: "2026-03-01".into(),
                },
                Declaration {
                    regime: "eu.halogen_free".into(),
                    status: DeclarationStatus::DeclaredCompliant,
                    detail: String::new(),
                    source: user_source(),
                    as_of: "2026-03-01".into(),
                },
            ],
            ..Compliance::default()
        },
    );
    let clean = compliance_check(&declared);
    assert!(
        !clean.iter().any(|f| f.state == "unverified"),
        "declared parts should not stay unverified: {:?}",
        clean
    );

    let mut stale = sample_project();
    set_compliance(
        &mut stale,
        Compliance {
            declarations: vec![Declaration {
                regime: "eu.rohs".into(),
                status: DeclarationStatus::DeclaredExempt,
                detail: "6(a)".into(),
                source: user_source(),
                as_of: "2026-03-01".into(),
            }],
            ..Compliance::default()
        },
    );
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

    let mut reach_hit = sample_project();
    set_compliance(
        &mut reach_hit,
        Compliance {
            declarations: vec![Declaration {
                regime: "eu.reach.svhc".into(),
                status: DeclarationStatus::DeclaredCompliant,
                detail: "2026-03".into(),
                source: user_source(),
                as_of: "2026-03-01".into(),
            }],
            substances: vec![DeclaredSubstance {
                name: "Synthetic SVHC Alpha".into(),
                note: String::new(),
                weight_percent: 0.2,
            }],
            ..Compliance::default()
        },
    );
    let reach = compliance_check(&reach_hit);
    assert!(
        reach.iter().any(|f| f.code == "reach_article_33" && f.state == "hit"),
        "SVHC above 0.1% should Article 33 finding: {:?}",
        reach
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
fn compliance_project_round_trips() {
    let mut p = sample_project();
    set_compliance(
        &mut p,
        Compliance {
            role: Some(PartRole::Battery),
            ..Compliance::default()
        },
    );
    let json = serde_json::to_string(&p).expect("serialize");
    let back: Project = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.compliance_settings.rule_sets, vec!["eu_market"]);
    assert_eq!(
        back.components
            .iter()
            .find(|c| c.id == 1)
            .and_then(|c| c.compliance.role),
        Some(PartRole::Battery)
    );
}
