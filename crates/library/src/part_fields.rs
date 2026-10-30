//! Standard keys for manufacturer, part number and sourcing columns on a part.
//!
//! Bills of materials and the typed sourcing record agree on these names so
//! columns stay stable across tools.

/// Maker of the part.
pub const MANUFACTURER: &str = "Manufacturer";
/// The maker's part number.
pub const MPN: &str = "MPN";
/// Where the part is in its life.
pub const LIFECYCLE: &str = "Lifecycle";
/// Moisture sensitivity level, 1 to 6.
pub const MOISTURE_LEVEL: &str = "Moisture level";
/// RoHS compliance.
pub const ROHS: &str = "RoHS";
/// REACH compliance.
pub const REACH: &str = "REACH";
/// Halogen-free claim.
pub const HALOGEN_FREE: &str = "Halogen free";
/// How the part is supplied.
pub const PACKAGING: &str = "Packaging";
/// Link to the datasheet.
pub const DATASHEET: &str = "Datasheet";
/// Link to a 3D model.
pub const MODEL3D: &str = "3D model";
/// Alternate parts as `MFR: MPN; ...`.
pub const ALTERNATES: &str = "Alternates";

/// Keys that read the typed sourcing record in BOM export.
pub const TYPED_SOURCING_KEYS: &[&str] = &[
    MANUFACTURER,
    MPN,
    LIFECYCLE,
    MOISTURE_LEVEL,
    ROHS,
    REACH,
    HALOGEN_FREE,
    PACKAGING,
    DATASHEET,
    MODEL3D,
    ALTERNATES,
];

/// True when `key` is a typed sourcing column name.
pub fn is_typed_sourcing_key(key: &str) -> bool {
    TYPED_SOURCING_KEYS.contains(&key)
}
