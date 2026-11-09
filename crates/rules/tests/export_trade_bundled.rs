use pcbcraft_rules::compliance::{
    EntityListSnapshot, bundled_export_and_trade, bundled_list_tokens,
};

#[test]
fn bundled_export_and_trade_rule_set_loads() -> Result<(), String> {
    let set = bundled_export_and_trade()?;
    assert_eq!(set.id, "export_and_trade");
    assert!(set.rules.iter().any(|r| r.id == "party_screening"));
    Ok(())
}

#[test]
fn bundled_party_screening_fixture_has_source_metadata() -> Result<(), String> {
    let snap = EntityListSnapshot::parse(include_str!(
        "../data/compliance/lists/party_screening_fixture.toml"
    ))?;
    assert!(!snap.source_url.is_empty());
    assert!(!snap.retrieved.is_empty());
    assert!(!snap.licence.is_empty());
    Ok(())
}

#[test]
fn bundled_list_tokens_are_non_empty() {
    assert!(!bundled_list_tokens().is_empty());
}
