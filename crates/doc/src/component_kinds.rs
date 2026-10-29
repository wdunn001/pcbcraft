//! Closed component kind vocabulary for profile and companion rules.
//!
//! Kinds are lowercase slugs. Validation rejects unknown kinds with a plain
//! list of allowed slugs. Legacy loads with empty or unknown kinds are coerced
//! to `ic` on save.

use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

/// The complete set of recognized component kinds.
pub const KINDS: &[&str] = &[
    "resistor",
    "capacitor",
    "inductor",
    "led",
    "diode",
    "tvs",
    "mosfet",
    "relay",
    "fuse",
    "crystal",
    "oscillator",
    "ic",
    "mcu",
    "opamp",
    "ldo",
    "buck",
    "motor_driver",
    "sensor",
    "antenna",
    "test_point",
    "connector",
    "connector_power",
    "common_mode_choke",
];

/// Returns true if the given slug is a recognized component kind.
pub fn is_valid_kind(kind: &str) -> bool {
    KINDS.contains(&kind)
}

/// Returns the list of allowed kind slugs for error messages.
pub fn allowed_kinds() -> Vec<String> {
    KINDS.iter().map(|k| k.to_string()).collect()
}

/// Coerce a legacy or unknown kind to a recognized one.
/// Empty or unrecognized kinds become `ic`.
pub fn coerce_kind(kind: &str) -> String {
    if is_valid_kind(kind) {
        kind.to_string()
    } else {
        "ic".to_string()
    }
}

/// Check if a kind matches a rule alias.
/// `esd` matches `tvs`, `mcu` matches `ic`.
pub fn kind_matches_alias(kind: &str, alias: &str) -> bool {
    if kind == alias {
        return true;
    }
    match alias {
        "esd" => kind == "tvs",
        "mcu" => kind == "ic" || kind == "mcu",
        _ => false,
    }
}

/// Validate a component kind, returning an error message if invalid.
pub fn validate_kind(kind: &str) -> Option<String> {
    if kind.is_empty() {
        return Some("component kind is empty".to_string());
    }
    if !is_valid_kind(kind) {
        return Some(format!(
            "unknown component kind '{}'; allowed: {}",
            kind,
            allowed_kinds().join(", ")
        ));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_valid_kind() {
        assert!(is_valid_kind("resistor"));
        assert!(is_valid_kind("ldo"));
        assert!(is_valid_kind("mcu"));
        assert!(is_valid_kind("tvs"));
        assert!(!is_valid_kind("unknown"));
        assert!(!is_valid_kind(""));
    }

    #[test]
    fn test_coerce_kind() {
        assert_eq!(coerce_kind("resistor"), "resistor");
        assert_eq!(coerce_kind("ldo"), "ldo");
        assert_eq!(coerce_kind(""), "ic");
        assert_eq!(coerce_kind("unknown"), "ic");
    }

    #[test]
    fn test_kind_matches_alias() {
        assert!(kind_matches_alias("tvs", "esd"));
        assert!(kind_matches_alias("ic", "mcu"));
        assert!(kind_matches_alias("mcu", "mcu"));
        assert!(kind_matches_alias("resistor", "resistor"));
        assert!(!kind_matches_alias("diode", "esd"));
        assert!(!kind_matches_alias("opamp", "mcu"));
    }

    #[test]
    fn test_validate_kind() {
        assert!(validate_kind("resistor").is_none());
        assert!(validate_kind("ldo").is_none());
        assert!(validate_kind("").is_some());
        assert!(validate_kind("unknown").is_some());
    }
}
