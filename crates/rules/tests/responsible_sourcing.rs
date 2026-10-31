use pcbcraft_doc::{
    Compliance, ComplianceDeclaration, ComplianceSource, ComplianceSourceKind, ComplianceStatus,
    DeclaredSubstance, ManufacturerRecord, MineralsReportOnFile, Project,
};
use pcbcraft_rules::compliance_check;

fn part(id: u64, reference: &str, manufacturer: &str) -> pcbcraft_doc::Component {
    let mut c = pcbcraft_doc::component_template(
        "resistor",
        id,
        reference.into(),
        pcbcraft_geom::Point::default(),
    )
    .expect("template");
    c.fields.push(pcbcraft_doc::ComponentField {
        key: "Manufacturer".into(),
        value: manufacturer.into(),
    });
    c
}

#[test]
fn a_newcomer_sees_which_suppliers_owe_a_minerals_report() {
    let mut p = Project::default();
    p.compliance_settings.rule_sets = vec!["responsible_sourcing".into()];
    p.components.push(part(1, "R1", "Alpha Parts"));
    p.components.push(part(2, "R2", "Beta Supply"));
    p.manufacturers.push(ManufacturerRecord {
        id: 10,
        name: "Alpha Parts".into(),
        aliases: vec![],
        country: None,
        parent: None,
        minerals_report: Some(MineralsReportOnFile {
            template: "cmrt".into(),
            version: "6.22".into(),
            as_of: "2026-03-01".into(),
        }),
    });

    let findings = compliance_check(&p);
    let minerals: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == "minerals_report_on_file")
        .collect();
    assert_eq!(minerals.len(), 1, "only Beta Supply lacks a CMRT on file: {findings:?}");
    assert!(
        minerals[0].evidence.contains("Beta Supply"),
        "finding names the manufacturer: {}",
        minerals[0].evidence
    );
    assert!(
        minerals.iter().all(|f| f.severity == "warning" || f.severity == "info"),
        "severity stays at information or warning"
    );
    assert!(
        !findings.iter().any(|f| f.severity == "error"),
        "no error-level compliance rows"
    );
}

#[test]
fn a_listed_prop65_substance_cites_the_bundled_snapshot() {
    let mut p = Project::default();
    p.compliance_settings.rule_sets = vec!["responsible_sourcing".into()];
    p.components.push(part(1, "C1", "Example Co"));
    p.compliance_by_component.insert(
        1,
        Compliance {
            substances: vec![DeclaredSubstance {
                name: "Lead".into(),
                cas: Some("7439-92-1".into()),
                weight_percent: 0.0,
            }],
            ..Compliance::default()
        },
    );

    let findings = compliance_check(&p);
    let prop65: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == "prop65_listed_substance")
        .collect();
    assert_eq!(prop65.len(), 1);
    assert!(
        prop65[0].evidence.contains("2026-01-03"),
        "evidence cites the list snapshot version: {}",
        prop65[0].evidence
    );
    assert_eq!(prop65[0].severity, "info");
}

#[test]
fn fluoropolymer_parts_surface_unknown_pfas_status_as_information() {
    let mut p = Project::default();
    p.compliance_settings.rule_sets = vec!["responsible_sourcing".into()];
    p.components.push(part(1, "W1", "Wire Co"));
    p.compliance_by_component.insert(
        1,
        Compliance {
            role: Some("fluoropolymer".into()),
            declarations: vec![ComplianceDeclaration {
                regime: "us.tsca.pfas".into(),
                status: ComplianceStatus::Unknown,
                detail: String::new(),
                source: ComplianceSource {
                    kind: ComplianceSourceKind::User,
                    label: "lab".into(),
                    url: None,
                    document_ref: None,
                },
                as_of: None,
            }],
            ..Compliance::default()
        },
    );

    let findings = compliance_check(&p);
    let pfas: Vec<_> = findings
        .iter()
        .filter(|f| f.rule == "pfas_fluoropolymer_declaration")
        .collect();
    assert_eq!(pfas.len(), 1);
    assert_eq!(pfas[0].severity, "info");
    assert!(
        pfas[0].evidence.contains("2026-03-28"),
        "rule file date appears in evidence: {}",
        pfas[0].evidence
    );
}
