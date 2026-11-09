use pcbcraft_publication::{LicenceVerdict, design_licence, licence_table};

#[test]
fn licence_pair_table_has_every_cell() -> Result<(), String> {
    let table = licence_table();
    assert!(!table.licences.is_empty());
    for design in &table.licences {
        let row = table
            .compat
            .get(design)
            .ok_or_else(|| format!("missing design row {design}"))?;
        for part in &table.licences {
            assert!(row.contains_key(part), "missing cell {design} x {part}");
            let _ = design_licence(&table, design, part);
        }
    }
    Ok(())
}

#[test]
fn nd_part_in_open_design_is_incompatible() {
    let table = licence_table();
    assert_eq!(
        design_licence(&table, "MIT", "CC-BY-ND-4.0"),
        LicenceVerdict::Incompatible
    );
}

#[test]
fn restricted_source_is_detected() {
    let table = licence_table();
    assert!(pcbcraft_publication::is_restricted_source(
        &table,
        "parts from view-only-catalog.example"
    ));
}
