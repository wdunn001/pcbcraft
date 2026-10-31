//! Part-level checks: pin pad agreement, catalog fields, 3D body fit, replacement pinout.
//!
//! # Dependencies
//! - crates/library/src/pin_pad_map.rs
//! - crates/library/src/listing.rs (`package_key`, `Category`)
//! - crates/library/src/footprint_check.rs (`courtyard_half_extents`)

use crate::listing::{Category, package_key};
use crate::pin_pad_map::{PinPadFinding, PinPadMap, validate_pin_pad_map};
use crate::{FootprintDefinition, SymbolDefinition};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Closed parameter keys (snake_case names). Matches the part-parameters epic list.
///
/// # Rules
/// - Keys are snake_case and category-specific via `param_key_allowed`.
///
/// # Dependencies
/// - crates/library/src/part_validation.rs `validate_catalog_fields`
///
/// # Do not
/// - Add keys without updating category allowlists in the same commit.
pub const PARAM_KEYS: &[&str] = &[
    "capacitance",
    "resistance",
    "inductance",
    "rated_voltage_dc",
    "rated_voltage_ac",
    "rated_current",
    "power",
    "tolerance_range",
    "temperature_range",
    "dielectric",
    "esr",
    "frequency",
    "forward_voltage",
    "drain_source_voltage",
    "on_resistance",
    "package_key",
    "pin_count",
    "pitch",
    "height",
    "mount_type",
    "temperature_coefficient",
];

fn param_key_allowed(category: Category, key: &str) -> bool {
    if !PARAM_KEYS.contains(&key) {
        return false;
    }
    match category {
        Category::Resistor => matches!(
            key,
            "resistance"
                | "tolerance_range"
                | "rated_voltage_dc"
                | "rated_voltage_ac"
                | "power"
                | "temperature_range"
                | "temperature_coefficient"
                | "package_key"
                | "pin_count"
                | "pitch"
                | "height"
                | "mount_type"
        ),
        Category::Capacitor => matches!(
            key,
            "capacitance"
                | "tolerance_range"
                | "rated_voltage_dc"
                | "rated_voltage_ac"
                | "dielectric"
                | "esr"
                | "temperature_range"
                | "temperature_coefficient"
                | "package_key"
                | "pin_count"
                | "pitch"
                | "height"
                | "mount_type"
        ),
        Category::Inductor => matches!(
            key,
            "inductance"
                | "tolerance_range"
                | "rated_current"
                | "rated_voltage_dc"
                | "temperature_range"
                | "package_key"
                | "pin_count"
                | "pitch"
                | "height"
                | "mount_type"
        ),
        Category::Crystal => matches!(
            key,
            "frequency"
                | "tolerance_range"
                | "temperature_range"
                | "package_key"
                | "mount_type"
        ),
        Category::Mosfet => matches!(
            key,
            "drain_source_voltage"
                | "on_resistance"
                | "rated_current"
                | "power"
                | "temperature_range"
                | "package_key"
                | "pin_count"
                | "pitch"
                | "mount_type"
        ),
        Category::Diode | Category::Led => matches!(
            key,
            "forward_voltage"
                | "rated_current"
                | "power"
                | "temperature_range"
                | "package_key"
                | "pin_count"
                | "mount_type"
        ),
        _ => matches!(
            key,
            "package_key" | "pin_count" | "pitch" | "height" | "mount_type"
        ),
    }
}

/// Catalog and geometry fields checked beside the pin pad map.
///
/// # Rules
/// - `package_key` must match `package_key(package_text)` when text is present.
/// - `body_half_mm` is compared to the footprint courtyard when set.
///
/// # Dependencies
/// - crates/library/src/listing.rs `package_key`, `Category`
///
/// # Do not
/// - Parse parameter values here; only keys are validated.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartCatalogFields {
    #[serde(default)]
    pub manufacturer: String,
    #[serde(default)]
    pub mpn: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub datasheet_url: Option<String>,
    /// Canonical package key from `package_key`.
    #[serde(default)]
    pub package_key: String,
    /// Source or user package text used to compute the key.
    #[serde(default)]
    pub package_text: String,
    #[serde(default)]
    pub category: Category,
    /// Normalised parameter keys with values (values are not parsed here).
    #[serde(default)]
    pub parameters: Vec<(String, String)>,
    /// Optional axis-aligned body half sizes in millimetres for courtyard fit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body_half_mm: Option<[f64; 3]>,
}

/// One part validation finding; pin and pad mirror pin pad map localisation.
///
/// # Rules
/// - `code` groups outcomes for the Verified badge and UI filters.
///
/// # Dependencies
/// - crates/library/src/part_validation.rs `validate_part`
///
/// # Do not
/// - Emit findings without a stable `code` string.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartValidationFinding {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pin: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pad: Option<String>,
}

impl From<PinPadFinding> for PartValidationFinding {
    fn from(f: PinPadFinding) -> Self {
        Self {
            code: f.code,
            message: f.message,
            pin: f.pin,
            pad: f.pad,
        }
    }
}

/// Aggregated part validation outcome for the Verified badge.
///
/// # Rules
/// - `verified` is true only when every boolean flag is true and findings are empty.
///
/// # Dependencies
/// - crates/library/src/part_validation.rs `validate_part`
///
/// # Do not
/// - Set `verified` without running both pin pad and catalog checks.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartValidationResult {
    pub verified: bool,
    #[serde(default)]
    pub pin_pad_ok: bool,
    #[serde(default)]
    pub package_key_ok: bool,
    #[serde(default)]
    pub datasheet_ok: bool,
    #[serde(default)]
    pub identity_ok: bool,
    #[serde(default)]
    pub parameters_ok: bool,
    #[serde(default)]
    pub body_fits_ok: bool,
    #[serde(default)]
    pub findings: Vec<PartValidationFinding>,
}

/// Validate manufacturer identity, datasheet, package key, parameters and 3D body fit.
///
/// # Rules
/// - Returns `(ok, findings)` where `ok` is false when any finding is emitted.
///
/// # Dependencies
/// - crates/library/src/footprint_check.rs `courtyard_half_extents`
///
/// # Do not
/// - Skip footprint package comparison when a footprint is supplied.
pub fn validate_catalog_fields(
    fields: &PartCatalogFields,
    footprint: Option<&FootprintDefinition>,
) -> (bool, Vec<PartValidationFinding>) {
    let mut findings = Vec::new();
    let mut ok = true;

    if fields.manufacturer.trim().is_empty() {
        ok = false;
        findings.push(PartValidationFinding {
            code: "manufacturer_missing".into(),
            message: "manufacturer is not set".into(),
            pin: None,
            pad: None,
        });
    }
    if fields.mpn.trim().is_empty() {
        ok = false;
        findings.push(PartValidationFinding {
            code: "mpn_missing".into(),
            message: "manufacturer part number is not set".into(),
            pin: None,
            pad: None,
        });
    }

    let ds_ok = fields
        .datasheet_url
        .as_ref()
        .is_some_and(|u| u.starts_with("http://") || u.starts_with("https://"));
    if !ds_ok {
        ok = false;
        findings.push(PartValidationFinding {
            code: "datasheet_missing".into(),
            message: "datasheet URL must be http or https".into(),
            pin: None,
            pad: None,
        });
    }

    let computed = package_key(&fields.package_text);
    if fields.package_key.is_empty() {
        ok = false;
        findings.push(PartValidationFinding {
            code: "package_key_missing".into(),
            message: "package key is not set".into(),
            pin: None,
            pad: None,
        });
    } else if !computed.key.is_empty() && computed.key != fields.package_key {
        ok = false;
        findings.push(PartValidationFinding {
            code: "package_key_mismatch".into(),
            message: format!(
                "typed package {:?} normalises to {} but the record has {}",
                fields.package_text, computed.key, fields.package_key
            ),
            pin: None,
            pad: None,
        });
    }

    if let Some(fp) = footprint {
        let name_key = package_key(&fp.name);
        if !fields.package_key.is_empty()
            && !name_key.key.is_empty()
            && name_key.key != fields.package_key
        {
            ok = false;
            findings.push(PartValidationFinding {
                code: "footprint_package_mismatch".into(),
                message: format!(
                    "footprint {} package key {} does not match part package key {}",
                    fp.name, name_key.key, fields.package_key
                ),
                pin: None,
                pad: None,
            });
        }
    }

    for (key, _value) in &fields.parameters {
        if !param_key_allowed(fields.category, key) {
            ok = false;
            findings.push(PartValidationFinding {
                code: "parameter_key_invalid".into(),
                message: format!("parameter key {key} is not allowed for this category"),
                pin: None,
                pad: None,
            });
        }
    }

    let body_ok = match (fields.body_half_mm, footprint) {
        (Some(half), Some(fp)) => body_fits_courtyard(half, fp, &mut findings),
        (Some(_), None) => {
            findings.push(PartValidationFinding {
                code: "body_no_footprint".into(),
                message: "3D body extents need a footprint courtyard".into(),
                pin: None,
                pad: None,
            });
            false
        }
        (None, _) => {
            findings.push(PartValidationFinding {
                code: "body_missing".into(),
                message: "3D body half extents are not set".into(),
                pin: None,
                pad: None,
            });
            false
        }
    };
    if !body_ok {
        ok = false;
    }

    (ok, findings)
}

fn body_fits_courtyard(
    half: [f64; 3],
    footprint: &FootprintDefinition,
    findings: &mut Vec<PartValidationFinding>,
) -> bool {
    let Some((hx, hy)) = crate::courtyard_half_extents(footprint) else {
        findings.push(PartValidationFinding {
            code: "courtyard_missing".into(),
            message: format!("footprint {} has no courtyard rectangle", footprint.name),
            pin: None,
            pad: None,
        });
        return false;
    };
    let tol = 0.2;
    if half[0] > hx + tol || half[1] > hy + tol {
        findings.push(PartValidationFinding {
            code: "body_outside_courtyard".into(),
            message: format!(
                "3D body half size {:.3} x {:.3} mm exceeds courtyard {:.3} x {:.3} mm on footprint {}",
                half[0], half[1], hx, hy, footprint.name
            ),
            pin: None,
            pad: None,
        });
        return false;
    }
    true
}

/// Run pin pad and catalog checks; sets `verified` when every group passes.
pub fn validate_part(
    symbol: &SymbolDefinition,
    footprint: &FootprintDefinition,
    map: &PinPadMap,
    fields: &PartCatalogFields,
) -> PartValidationResult {
    let pin_findings = validate_pin_pad_map(symbol, footprint, map);
    let pin_pad_ok = pin_findings.is_empty();
    let (catalog_ok, mut catalog_findings) = validate_catalog_fields(fields, Some(footprint));

    let mut findings: Vec<PartValidationFinding> = pin_findings.into_iter().map(Into::into).collect();
    findings.append(&mut catalog_findings);

    let package_key_ok = !findings.iter().any(|f| {
        f.code.starts_with("package_key") || f.code == "footprint_package_mismatch"
    });
    let datasheet_ok = !findings.iter().any(|f| f.code == "datasheet_missing");
    let identity_ok = !findings
        .iter()
        .any(|f| f.code == "manufacturer_missing" || f.code == "mpn_missing");
    let parameters_ok = !findings
        .iter()
        .any(|f| f.code == "parameter_key_invalid");
    let body_fits_ok = !findings.iter().any(|f| {
        f.code == "body_outside_courtyard"
            || f.code == "body_missing"
            || f.code == "courtyard_missing"
            || f.code == "body_no_footprint"
    });

    let verified = pin_pad_ok
        && catalog_ok
        && package_key_ok
        && datasheet_ok
        && identity_ok
        && parameters_ok
        && body_fits_ok;

    PartValidationResult {
        verified,
        pin_pad_ok,
        package_key_ok,
        datasheet_ok,
        identity_ok,
        parameters_ok,
        body_fits_ok,
        findings,
    }
}

/// Outcome of a drop-in pinout comparison between two pin pad maps.
///
/// # Rules
/// - `drop_in` is true when every pin maps to the same pad set on both maps.
///
/// # Dependencies
/// - crates/library/src/part_validation.rs `replacement_check`
///
/// # Do not
/// - Compare catalog fields here; pin pad agreement only.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplacementCheckResult {
    pub drop_in: bool,
    #[serde(default)]
    pub findings: Vec<PartValidationFinding>,
}

/// Compare candidate pin pad mapping to a reference for replacement suggestions.
///
/// # Rules
/// - Findings name the pin whose pad set differs or is missing.
///
/// # Dependencies
/// - crates/library/src/pin_pad_map.rs `PinPadMap`
///
/// # Do not
/// - Treat pad role differences as drop-in failures; only mapped pad sets are compared.
pub fn replacement_check(reference: &PinPadMap, candidate: &PinPadMap) -> ReplacementCheckResult {
    let mut findings = Vec::new();
    let ref_pins: BTreeMap<_, _> = reference
        .rows
        .iter()
        .map(|r| (r.pin.as_str(), r.pads.as_slice()))
        .collect();
    let cand_pins: BTreeMap<_, _> = candidate
        .rows
        .iter()
        .map(|r| (r.pin.as_str(), r.pads.as_slice()))
        .collect();

    for (pin, ref_pads) in &ref_pins {
        match cand_pins.get(pin) {
            Some(cand_pads) if ref_pads == cand_pads => {}
            Some(cand_pads) => {
                findings.push(PartValidationFinding {
                    code: "replacement_pin_pads".into(),
                    message: format!(
                        "pin {pin} maps to pads {cand_pads:?} but the reference maps to {ref_pads:?}"
                    ),
                    pin: Some((*pin).to_owned()),
                    pad: None,
                });
            }
            None => {
                findings.push(PartValidationFinding {
                    code: "replacement_pin_missing".into(),
                    message: format!("candidate has no mapping for pin {pin}"),
                    pin: Some((*pin).to_owned()),
                    pad: None,
                });
            }
        }
    }
    for pin in cand_pins.keys() {
        if !ref_pins.contains_key(pin) {
            findings.push(PartValidationFinding {
                code: "replacement_pin_extra".into(),
                message: format!("candidate adds pin {pin} not on the reference"),
                pin: Some((*pin).to_owned()),
                pad: None,
            });
        }
    }

    ReplacementCheckResult {
        drop_in: findings.is_empty(),
        findings,
    }
}
