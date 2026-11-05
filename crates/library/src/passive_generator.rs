//! Passive component generator: footprint, symbol and part number from a value.
//!
//! # Rules
//! - Package codes are imperial: 0402, 0603, 0805, 1206.
//! - Part numbers use the EIA value code with R/K/M or P/N/U markers.
//! - Resistors: 0 < value <= 1e12 ohms. Capacitors: 1e-13 <= value <= 1e-2 farads.
//! - Tolerance is an integer percentage from 1 to 20.
//!
//! # Dependencies
//! - crates/library/src/passive_series.rs (values_between, value_code, PassiveKind)
//! - crates/library/src/ipc7351.rs (generate_footprint_from_key)
//! - crates/library/src/online_import.rs (generic_symbol_for)
//!
//! # Do not
//! - Add package codes beyond the four standard chip sizes without IPC review.

use crate::ipc7351::generate_footprint_from_key;
use crate::online_import::generic_symbol_for;
use crate::passive_series::{PassiveKind, value_code};
use crate::{FootprintDefinition, PackageFamily, PackageKey, SymbolDefinition};

/// A generated passive component with footprint, symbol and part number.
///
/// # Rules
/// - `mpn` is unique within a series and package.
/// - `footprint` has exactly two pads numbered 1 and 2.
/// - `symbol` has exactly two pins matching the pad numbers.
///
/// # Dependencies
/// - crates/library/src/lib.rs (FootprintDefinition, SymbolDefinition)
///
/// # Do not
/// - Mutate any field after creation; this is a value object.
#[derive(Clone, Debug)]
pub struct GeneratedPassive {
    /// The part number, built from kind, package, value code and tolerance.
    pub mpn: String,
    /// The two pad land pattern for the package.
    pub footprint: FootprintDefinition,
    /// The two pin symbol that matches the pads.
    pub symbol: SymbolDefinition,
    /// The value in ohms for a resistor or farads for a capacitor.
    pub value: f64,
    /// The imperial package code such as 0603.
    pub package: String,
}

/// Generate a passive component from its value, package and tolerance.
///
/// # Rules
/// - Package must be one of 0402, 0603, 0805, 1206.
/// - Tolerance must be 1 to 20 percent.
/// - Returns Err for values outside the component's range.
///
/// # Dependencies
/// - crates/library/src/passive_series.rs (value_code)
/// - crates/library/src/ipc7351.rs (generate_footprint_from_key)
/// - crates/library/src/online_import.rs (generic_symbol_for)
///
/// # Do not
/// - Call with unbounded values; validate before generating.
pub fn generate_passive(
    kind: PassiveKind,
    value: f64,
    package: &str,
    tolerance_pct: u8,
) -> Result<GeneratedPassive, String> {
    if !(1..=20).contains(&tolerance_pct) {
        return Err("tolerance must be 1 to 20 percent".into());
    }
    validate_value(kind, value)?;
    let key = package_key_for_code(package)?;
    let code = value_code(value, kind)?;
    let prefix = match kind {
        PassiveKind::Resistor => "RES",
        PassiveKind::Capacitor => "CAP",
    };
    let mpn = format!("{}-{}-{}-{}", prefix, package, code, tolerance_pct);
    let footprint = generate_footprint_from_key(&key, crate::LandPatternDensity::Nominal, &mpn)?;
    let symbol = generic_symbol_for(&footprint).map_err(|e| e.to_string())?;
    Ok(GeneratedPassive {
        mpn,
        footprint,
        symbol,
        value,
        package: package.to_owned(),
    })
}

fn validate_value(kind: PassiveKind, value: f64) -> Result<(), String> {
    match kind {
        PassiveKind::Resistor => {
            if value <= 0.0 || value > 1e12 {
                return Err("resistor value must be 0 < value <= 1e12 ohms".into());
            }
        }
        PassiveKind::Capacitor => {
            if value < 1e-13 || value > 1e-2 {
                return Err("capacitor value must be 1e-13 <= value <= 1e-2 farads".into());
            }
        }
    }
    Ok(())
}

fn package_key_for_code(code: &str) -> Result<PackageKey, String> {
    let (length, width) = match code {
        "0402" => (1.0, 0.5),
        "0603" => (1.6, 0.8),
        "0805" => (2.0, 1.25),
        "1206" => (3.2, 1.6),
        _ => return Err(format!("unknown package code {code}")),
    };
    Ok(PackageKey {
        family: PackageFamily::Chip,
        pin_count: 2,
        pitch_mm: 0.0,
        body_length_mm: length,
        body_width_mm: width,
        lead_span_mm: 0.0,
        exposed_pad_mm: 0.0,
        height_mm: 0.0,
        courtyard_half_x: None,
        courtyard_half_y: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::passive_series::{ESeries, values_between};

    #[test]
    fn every_e24_resistor_from_10_ohms_to_1m_is_valid() -> Result<(), String> {
        let values = values_between(ESeries::E24, 10.0, 1e6)?;
        let mut mpns = Vec::new();
        for value in &values {
            let part = generate_passive(PassiveKind::Resistor, *value, "0603", 1)?;
            assert_eq!(part.footprint.pads.len(), 2, "footprint must have 2 pads");
            assert_eq!(part.symbol.pins.len(), 2, "symbol must have 2 pins");
            mpns.push(part.mpn);
        }
        assert_eq!(mpns.len(), values.len(), "all values must generate");
        let unique: std::collections::HashSet<_> = mpns.iter().collect();
        assert_eq!(unique.len(), mpns.len(), "all mpns must be unique");
        Ok(())
    }

    #[test]
    fn every_e6_capacitor_from_10_pf_to_10_uf_is_valid() -> Result<(), String> {
        let values = values_between(ESeries::E6, 1e-11, 1e-5)?;
        for value in &values {
            let part = generate_passive(PassiveKind::Capacitor, *value, "0805", 10)?;
            assert_eq!(part.footprint.pads.len(), 2);
            assert_eq!(part.symbol.pins.len(), 2);
        }
        Ok(())
    }

    #[test]
    fn a_ten_k_0603_resistor_is_named_res_0603_10k_1() -> Result<(), String> {
        let part = generate_passive(PassiveKind::Resistor, 10000.0, "0603", 1)?;
        assert_eq!(part.mpn, "RES-0603-10K-1");
        Ok(())
    }

    #[test]
    fn an_unknown_package_is_refused() {
        let result = generate_passive(PassiveKind::Resistor, 1000.0, "9999", 1);
        assert!(result.is_err());
    }
}
