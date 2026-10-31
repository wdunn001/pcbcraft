//! Standard keys for manufacturer, part number, description and supplier numbers on a part.
//!
//! Bills of materials, schematic parts tables and the typed sourcing record agree on these
//! names so columns stay stable across tools.

/// Maker of the part.
pub const MANUFACTURER: &str = "Manufacturer";
/// The maker's part number.
pub const MPN: &str = "MPN";
/// What the part is, in words.
pub const DESCRIPTION: &str = "Description";
/// Every supplier on the part, formatted as `Name: number; ...` in export columns.
pub const SUPPLIERS: &str = "Suppliers";
/// Prefix for one supplier's number: `Supplier:<name>`.
pub const SUPPLIER_KEY_PREFIX: &str = "Supplier:";

/// Keys reserved for structured sourcing in BOM field lists and parts table columns.
pub const STANDARD_SOURCING_KEYS: &[&str] = &[MANUFACTURER, MPN, DESCRIPTION, SUPPLIERS];

/// Keys suggested first when picking parts table columns (issue #227).
pub const PARTS_TABLE_SUGGESTED_KEYS: &[&str] = &[MANUFACTURER, MPN, DESCRIPTION];

/// True when `key` is a standard sourcing column name or `Supplier:<name>`.
pub fn is_standard_sourcing_key(key: &str) -> bool {
    STANDARD_SOURCING_KEYS.contains(&key)
        || key.strip_prefix(SUPPLIER_KEY_PREFIX)
            .is_some_and(|name| !name.trim().is_empty())
}
