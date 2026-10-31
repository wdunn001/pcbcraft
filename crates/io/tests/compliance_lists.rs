use pcbcraft_io::{
    EntityMatchConfidence, builtin_compliance_list, compliance_list_from_toml,
    match_entity_name, snapshot_staleness_note,
};

#[test]
fn alias_and_near_name_matching_use_synthetic_names() -> Result<(), String> {
    let snapshot = builtin_compliance_list("synthetic-demo-entities")?;
    let exact = match_entity_name(&snapshot, "SynAlpha Co").ok_or("alias should match exactly")?;
    assert_eq!(exact.confidence, EntityMatchConfidence::Exact);
    let possible = match_entity_name(&snapshot, "Synthetic Entty Alpha")
        .ok_or("near name should be a possible match")?;
    assert_eq!(possible.confidence, EntityMatchConfidence::Possible);
    assert!(match_entity_name(&snapshot, "Unrelated Maker ZZZ").is_none());
    Ok(())
}

#[test]
fn staleness_note_appears_after_cadence() -> Result<(), String> {
    let snapshot = builtin_compliance_list("synthetic-demo-entities")?;
    assert!(snapshot_staleness_note(&snapshot, "2026-06-01").is_none());
    let note = snapshot_staleness_note(&snapshot, "2026-10-01").ok_or("expected staleness note")?;
    assert!(note.contains("synthetic-demo-entities"));
    Ok(())
}

#[test]
fn malformed_and_oversized_imports_are_refused() -> Result<(), String> {
    assert!(compliance_list_from_toml("not valid toml {{{").is_err());
    let mut huge = String::from("schema = 1\nid = \"x\"\ntitle = \"t\"\nretrieved = \"2026-01-01\"\nlicence = \"CC0-1.0\"\n");
    huge.push_str(&"a".repeat(2 * 1024 * 1024 + 1));
    assert!(compliance_list_from_toml(&huge).is_err());
    let dup = include_str!("fixtures/compliance-lists/duplicate-entity.toml");
    assert!(compliance_list_from_toml(dup).is_err());
    Ok(())
}
