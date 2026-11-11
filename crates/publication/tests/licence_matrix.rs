use pcbcraft_publication::{LicenceVerdict, design_licence, is_restricted_source, licence_table};
use std::collections::BTreeMap;

const COMPAT_TOML: &str = include_str!("../data/licence_compat.toml");

fn expected_verdict(raw: &str) -> LicenceVerdict {
    match raw {
        "compatible" => LicenceVerdict::Compatible,
        "incompatible" => LicenceVerdict::Incompatible,
        _ => LicenceVerdict::Review,
    }
}

#[test]
fn licence_pair_table_has_a_test_for_every_cell() -> Result<(), String> {
    #[derive(serde::Deserialize)]
    struct Root {
        compat: BTreeMap<String, BTreeMap<String, String>>,
    }
    let root: Root =
        toml::from_str(COMPAT_TOML).map_err(|e| format!("parse licence table: {e}"))?;
    let table = licence_table();
    assert!(!table.licences.is_empty());
    for design in &table.licences {
        let row = root
            .compat
            .get(design)
            .ok_or_else(|| format!("missing design row {design}"))?;
        for part in &table.licences {
            let raw = row
                .get(part)
                .ok_or_else(|| format!("missing cell {design} x {part}"))?;
            assert_eq!(
                design_licence(&table, design, part),
                expected_verdict(raw),
                "cell {design} x {part}"
            );
        }
    }
    Ok(())
}

#[test]
fn unknown_licence_pair_goes_to_human_review() {
    let table = licence_table();
    assert_eq!(
        design_licence(&table, "MIT", "No-Such-Licence"),
        LicenceVerdict::Review
    );
}

#[test]
fn nd_part_in_open_design_is_incompatible() {
    let table = licence_table();
    assert_eq!(
        design_licence(&table, "MIT", "CC-BY-ND-4.0"),
        LicenceVerdict::Incompatible
    );
    assert_eq!(
        design_licence(&table, "MIT", "Proprietary-View"),
        LicenceVerdict::Incompatible
    );
}

#[test]
fn share_alike_part_in_permissive_design_needs_review() {
    let table = licence_table();
    assert_eq!(
        design_licence(&table, "MIT", "CC-BY-SA-4.0"),
        LicenceVerdict::Review
    );
}

#[test]
fn restricted_source_is_detected() {
    let table = licence_table();
    assert!(is_restricted_source(
        &table,
        "parts from view-only-catalog.example"
    ));
    assert!(!is_restricted_source(&table, "public-catalog.example"));
}
