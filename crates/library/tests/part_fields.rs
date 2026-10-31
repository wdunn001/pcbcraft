use pcbcraft_library::{
    DESCRIPTION, MANUFACTURER, MPN, PARTS_TABLE_SUGGESTED_KEYS, STANDARD_SOURCING_KEYS,
    SUPPLIER_KEY_PREFIX, is_standard_sourcing_key,
};

#[test]
fn standard_keys_are_reserved_for_sourcing_columns() {
    assert!(STANDARD_SOURCING_KEYS.contains(&MANUFACTURER));
    assert!(STANDARD_SOURCING_KEYS.contains(&MPN));
    assert!(STANDARD_SOURCING_KEYS.contains(&DESCRIPTION));
    assert_eq!(PARTS_TABLE_SUGGESTED_KEYS, &[MANUFACTURER, MPN, DESCRIPTION]);
    assert!(is_standard_sourcing_key(MPN));
    assert!(is_standard_sourcing_key(&format!("{SUPPLIER_KEY_PREFIX}Digi-Key")));
    assert!(!is_standard_sourcing_key("Rating"));
}
