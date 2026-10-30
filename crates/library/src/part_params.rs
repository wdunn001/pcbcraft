//! Normalised part parameters with units, per-source aliases and confidence.
//!
//! # Rules
//! - Empty means unknown; nothing is guessed.
//! - Raw source text stays beside every parsed value.
//! - Per-source alias JSON maps attribute names to [`ParamKey`] per category.
//!
//! # Dependencies
//! - crates/library/src/listing.rs (`parse_value`, `package_key`, categories)
//!
//! # Do not
//! - Fetch live catalogue data or write into project files from here.

use crate::listing::{
    Category, Unit, category_from, package_key, parse_temperature_range, parse_tolerance,
    parse_value, tolerance_from,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Closed parameter keys. Each maps to one SI unit where applicable.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParamKey {
    /// Capacitance in farads.
    Capacitance,
    /// Resistance in ohms.
    Resistance,
    /// Inductance in henries.
    Inductance,
    /// Maximum DC working voltage in volts.
    RatedVoltageDc,
    /// Maximum AC working voltage in volts.
    RatedVoltageAc,
    /// Maximum current in amperes.
    RatedCurrent,
    /// Power rating in watts.
    Power,
    /// Allowed value span as a percentage range.
    ToleranceRange,
    /// Operating temperature span in degrees Celsius.
    TemperatureRange,
    /// Capacitor dielectric name.
    Dielectric,
    /// Equivalent series resistance in ohms.
    Esr,
    /// Resonant or rated frequency in hertz.
    Frequency,
    /// Diode forward voltage in volts.
    ForwardVoltage,
    /// MOSFET drain-source voltage in volts.
    DrainSourceVoltage,
    /// MOSFET on-state resistance in ohms.
    OnResistance,
    /// Normalised package key from the source text.
    PackageKey,
    /// Pin or terminal count.
    PinCount,
    /// Pad pitch in millimetres.
    Pitch,
    /// Body height in millimetres.
    Height,
    /// Mount style text from the source.
    MountType,
    /// Temperature coefficient in ppm per degree Celsius.
    TemperatureCoefficient,
}

/// How much trust a merged parameter has.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParamConfidence {
    /// A person or policy marked this value as confirmed.
    Confirmed,
    /// Every source that gave this key agreed.
    Agreed,
    /// Only one source gave this key.
    Single,
    /// Sources disagreed; see [`ParamValue::conflicts`].
    Conflict,
}

/// A scalar value stored in SI base units unless the key says otherwise.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum ParamPayload {
    /// One numeric value in the key's SI unit.
    Number {
        /// Parsed magnitude.
        value: f64,
    },
    /// An inclusive numeric span.
    Range {
        /// Lower bound.
        low: f64,
        /// Upper bound.
        high: f64,
    },
    /// Free text that does not use SI units.
    Text {
        /// Source text or normalised token.
        value: String,
    },
}

/// One normalised parameter from one source answer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParamValue {
    /// Which closed key this row represents.
    pub key: ParamKey,
    /// Parsed value in SI units or text.
    pub payload: ParamPayload,
    /// SI unit for numeric keys; empty for text keys and temperature (Celsius).
    pub unit: Option<Unit>,
    /// Exact text the source returned.
    pub raw: String,
    /// Source id that supplied this row.
    pub source: String,
    /// When the source answer was recorded, Unix seconds.
    pub retrieved_at: u64,
    /// Trust label after merge, or single-source default.
    pub confidence: ParamConfidence,
    /// Other sources' values when [`ParamConfidence::Conflict`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conflicts: Vec<ParamConflictEntry>,
}

/// Another source's disagreeing value kept beside a conflict.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParamConflictEntry {
    /// Disagreeing parsed value.
    pub payload: ParamPayload,
    /// SI unit when the payload is numeric.
    pub unit: Option<Unit>,
    /// Exact text that source returned.
    pub raw: String,
    /// Source id for the disagreeing row.
    pub source: String,
    /// When that source answer was recorded, Unix seconds.
    pub retrieved_at: u64,
}

/// A source attribute that did not map to a closed key.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtraAttribute {
    /// Attribute name from the source.
    pub name: String,
    /// Attribute value text, unchanged.
    pub value: String,
}

/// Parsed parameters plus unmapped attributes for one part answer.
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartParams {
    /// Normalised rows sorted by key.
    pub params: Vec<ParamValue>,
    /// Unmapped attributes sorted by name.
    pub extras: Vec<ExtraAttribute>,
}

/// Per-source alias table: source id, category, attribute name to param key.
pub type ParamAliasFile = BTreeMap<String, BTreeMap<String, BTreeMap<String, ParamKey>>>;

/// Read alias JSON: top-level keys are source ids; then category snake names.
///
/// # Rules
/// - Files larger than two megabytes are refused.
/// # Do not
/// - Call with live network data; fixtures only.
pub fn load_param_aliases(json: &str) -> Result<ParamAliasFile, String> {
    if json.len() > 2_000_000 {
        return Err("the alias file is too large".into());
    }
    serde_json::from_str(json).map_err(|e| format!("the alias file is not valid: {e}"))
}

fn category_key(category: Category) -> &'static str {
    match category {
        Category::Resistor => "resistor",
        Category::Capacitor => "capacitor",
        Category::Inductor => "inductor",
        Category::Diode => "diode",
        Category::Led => "led",
        Category::Transistor => "transistor",
        Category::Mosfet => "mosfet",
        Category::IcRegulator => "ic_regulator",
        Category::IcMcu => "ic_mcu",
        Category::IcLogic => "ic_logic",
        Category::IcAnalog => "ic_analog",
        Category::Connector => "connector",
        Category::Crystal => "crystal",
        Category::Module => "module",
        Category::Mechanical => "mechanical",
        Category::Other => "other",
    }
}

fn allowed_keys(category: Category) -> &'static [ParamKey] {
    use ParamKey::*;
    match category {
        Category::Resistor => &[
            Resistance,
            ToleranceRange,
            RatedVoltageDc,
            RatedVoltageAc,
            Power,
            TemperatureRange,
            TemperatureCoefficient,
            PackageKey,
            PinCount,
            Pitch,
            Height,
            MountType,
        ],
        Category::Capacitor => &[
            Capacitance,
            ToleranceRange,
            RatedVoltageDc,
            RatedVoltageAc,
            Dielectric,
            Esr,
            TemperatureRange,
            TemperatureCoefficient,
            PackageKey,
            PinCount,
            Pitch,
            Height,
            MountType,
        ],
        Category::Inductor => &[
            Inductance,
            ToleranceRange,
            RatedCurrent,
            RatedVoltageDc,
            TemperatureRange,
            PackageKey,
            PinCount,
            Pitch,
            Height,
            MountType,
        ],
        Category::Crystal => &[
            Frequency,
            ToleranceRange,
            TemperatureRange,
            PackageKey,
            MountType,
        ],
        Category::Mosfet => &[
            DrainSourceVoltage,
            OnResistance,
            RatedCurrent,
            Power,
            TemperatureRange,
            PackageKey,
            PinCount,
            Pitch,
            MountType,
        ],
        Category::Diode | Category::Led => &[
            ForwardVoltage,
            RatedCurrent,
            Power,
            TemperatureRange,
            PackageKey,
            PinCount,
            MountType,
        ],
        _ => &[PackageKey, PinCount, Pitch, Height, MountType],
    }
}

fn key_allowed(category: Category, key: ParamKey) -> bool {
    allowed_keys(category).contains(&key)
}

fn unit_hint(key: ParamKey) -> Option<Unit> {
    use ParamKey::*;
    Some(match key {
        Resistance => Unit::Ohm,
        Capacitance => Unit::Farad,
        Inductance => Unit::Henry,
        Frequency => Unit::Hertz,
        RatedVoltageDc | RatedVoltageAc | ForwardVoltage | DrainSourceVoltage => Unit::Volt,
        RatedCurrent => Unit::Ampere,
        Power => Unit::Watt,
        OnResistance => Unit::Ohm,
        Esr => Unit::Ohm,
        _ => return None,
    })
}

fn parse_payload(
    key: ParamKey,
    category: Category,
    _attribute: &str,
    text: &str,
) -> Option<(ParamPayload, Option<Unit>)> {
    use ParamKey::*;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    match key {
        Resistance | Capacitance | Inductance | Frequency | RatedCurrent | Power | Esr
        | ForwardVoltage | DrainSourceVoltage | OnResistance => {
            let (v, u) = parse_value(trimmed, unit_hint(key))?;
            Some((ParamPayload::Number { value: v }, Some(u)))
        }
        RatedVoltageDc | RatedVoltageAc => {
            let (v, u) = parse_value(trimmed, Some(Unit::Volt))?;
            (u == Unit::Volt).then_some((ParamPayload::Number { value: v }, Some(Unit::Volt)))
        }
        ToleranceRange => {
            let (lo, hi) = tolerance_from(trimmed, category).or_else(|| parse_tolerance(trimmed))?;
            Some((
                ParamPayload::Range {
                    low: f64::from(lo),
                    high: f64::from(hi),
                },
                None,
            ))
        }
        TemperatureRange => {
            let (lo, hi) = parse_temperature_range(trimmed)?;
            Some((
                ParamPayload::Range {
                    low: f64::from(lo),
                    high: f64::from(hi),
                },
                None,
            ))
        }
        PackageKey => {
            let pkg = package_key(trimmed);
            let value = if pkg.key.is_empty() {
                pkg.text.clone()
            } else {
                pkg.key.clone()
            };
            Some((ParamPayload::Text { value }, None))
        }
        PinCount => {
            let digits: String = trimmed
                .chars()
                .filter(|c| c.is_ascii_digit())
                .take(4)
                .collect();
            let n: f64 = digits.parse().ok()?;
            Some((ParamPayload::Number { value: n }, None))
        }
        Pitch | Height => {
            if let Some((v, _)) = parse_value(trimmed, None) {
                return Some((ParamPayload::Number { value: v }, None));
            }
            let bare = trimmed
                .trim()
                .strip_suffix("mm")
                .map(str::trim)
                .or_else(|| trimmed.strip_suffix(" mm").map(str::trim))?;
            let v: f64 = bare.parse().ok()?;
            Some((ParamPayload::Number { value: v }, None))
        }
        MountType | Dielectric => Some((
            ParamPayload::Text {
                value: trimmed.to_string(),
            },
            None,
        )),
        TemperatureCoefficient => parse_temperature_coefficient(trimmed)
            .map(|value| (ParamPayload::Number { value }, None))
            .or_else(|| {
                Some((
                    ParamPayload::Text {
                        value: trimmed.to_string(),
                    },
                    None,
                ))
            }),
    }
}

fn rated_voltage_key(attribute: &str) -> ParamKey {
    let lower = attribute.to_lowercase();
    if lower.contains("ac") {
        ParamKey::RatedVoltageAc
    } else {
        ParamKey::RatedVoltageDc
    }
}

fn heuristic_key(category: Category, attribute: &str) -> Option<ParamKey> {
    let lower = attribute.to_lowercase();
    let key = if lower.contains("resistance") && !lower.contains("on") {
        ParamKey::Resistance
    } else if lower.contains("capacitance") {
        ParamKey::Capacitance
    } else if lower.contains("inductance") {
        ParamKey::Inductance
    } else if lower.contains("tolerance") {
        ParamKey::ToleranceRange
    } else if lower.contains("operating temperature") || lower.contains("temperature range") {
        ParamKey::TemperatureRange
    } else if lower.contains("voltage") && lower.contains("rated") {
        rated_voltage_key(attribute)
    } else if lower.contains("power") {
        ParamKey::Power
    } else if lower.contains("package") || lower.contains("case") {
        ParamKey::PackageKey
    } else if lower.contains("dielectric") {
        ParamKey::Dielectric
    } else if lower.contains("esr") {
        ParamKey::Esr
    } else if lower.contains("mount") {
        ParamKey::MountType
    } else {
        return None;
    };
    key_allowed(category, key).then_some(key)
}

fn lookup_key(
    table: Option<&BTreeMap<String, ParamKey>>,
    category: Category,
    attribute: &str,
) -> Option<ParamKey> {
    table
        .and_then(|aliases| {
            aliases
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(attribute))
                .map(|(_, v)| *v)
                .filter(|key| key_allowed(category, *key))
        })
        .or_else(|| heuristic_key(category, attribute))
}

/// Normalise one source attribute map using aliases and the part category.
///
/// # Rules
/// - Unmapped attribute names become [`ExtraAttribute`] rows.
/// - Each parsed row starts with [`ParamConfidence::Single`].
/// # Dependencies
/// - crates/library/src/part_params.rs#load_param_aliases
/// - crates/library/src/listing.rs#category_from
/// # Do not
/// - Invent a value when parsing fails; failed rows go to extras.
pub fn params_from_attributes(
    source_id: &str,
    source_category: &str,
    kind_text: &str,
    attributes: &BTreeMap<String, String>,
    aliases: &ParamAliasFile,
    retrieved_at: u64,
) -> PartParams {
    let category = category_from(source_category, kind_text);
    let table = aliases
        .get(source_id)
        .and_then(|by_cat| by_cat.get(category_key(category)));
    let mut params = Vec::new();
    let mut extras = Vec::new();
    for (name, value) in attributes {
        if name.eq_ignore_ascii_case("Category") {
            continue;
        }
        let Some(key) = lookup_key(table, category, name) else {
            if !value.trim().is_empty() {
                extras.push(ExtraAttribute {
                    name: name.clone(),
                    value: value.clone(),
                });
            }
            continue;
        };
        let Some((payload, unit)) = parse_payload(key, category, name, value) else {
            extras.push(ExtraAttribute {
                name: name.clone(),
                value: value.clone(),
            });
            continue;
        };
        params.push(ParamValue {
            key,
            payload,
            unit,
            raw: value.clone(),
            source: source_id.to_string(),
            retrieved_at,
            confidence: ParamConfidence::Single,
            conflicts: Vec::new(),
        });
    }
    params.sort_by_key(|p| p.key);
    extras.sort_by(|a, b| a.name.cmp(&b.name));
    PartParams { params, extras }
}

fn payloads_equal(a: &ParamPayload, b: &ParamPayload) -> bool {
    match (a, b) {
        (ParamPayload::Number { value: x }, ParamPayload::Number { value: y }) => {
            (x - y).abs() <= (x.abs().max(y.abs()) * 1e-9).max(1e-12)
        }
        (ParamPayload::Range { low: al, high: ah }, ParamPayload::Range { low: bl, high: bh }) => {
            (al - bl).abs() <= 1e-6 && (ah - bh).abs() <= 1e-6
        }
        (ParamPayload::Text { value: x }, ParamPayload::Text { value: y }) => {
            x.eq_ignore_ascii_case(y)
        }
        _ => false,
    }
}

/// Merge parameters from two sources for the same part. Nothing is guessed.
///
/// # Rules
/// - Equal payloads become [`ParamConfidence::Agreed`]; mismatches keep both
///   values with [`ParamConfidence::Conflict`].
/// # Do not
/// - Pick a winner when sources disagree.
pub fn merge_part_params(a: PartParams, b: PartParams) -> PartParams {
    let mut extras = a.extras;
    extras.extend(b.extras);
    let mut by_key: BTreeMap<ParamKey, Vec<ParamValue>> = BTreeMap::new();
    for p in a.params.into_iter().chain(b.params) {
        by_key.entry(p.key).or_default().push(p);
    }
    let mut params = Vec::new();
    for (_key, mut group) in by_key {
        group.sort_by(|x, y| x.source.cmp(&y.source));
        match group.len() {
            0 => {}
            1 => {
                if let Some(mut only) = group.pop() {
                    only.confidence = ParamConfidence::Single;
                    params.push(only);
                }
            }
            _ => {
                let same = group.first().is_some_and(|first| {
                    group
                        .iter()
                        .all(|p| payloads_equal(&p.payload, &first.payload))
                });
                if same {
                    if let Some(mut merged) = group.pop() {
                        merged.confidence = ParamConfidence::Agreed;
                        merged.conflicts.clear();
                        params.push(merged);
                    }
                } else if !group.is_empty() {
                    let mut primary = group.remove(0);
                    primary.confidence = ParamConfidence::Conflict;
                    primary.conflicts = group
                        .into_iter()
                        .map(|p| ParamConflictEntry {
                            payload: p.payload,
                            unit: p.unit,
                            raw: p.raw,
                            source: p.source,
                            retrieved_at: p.retrieved_at,
                        })
                        .collect();
                    params.push(primary);
                }
            }
        }
    }
    params.sort_by_key(|p| p.key);
    extras.sort_by(|x, y| x.name.cmp(&y.name));
    extras.dedup_by(|x, y| x.name.eq_ignore_ascii_case(&y.name) && x.value == y.value);
    PartParams { params, extras }
}

/// Parse a temperature coefficient string such as `±30 ppm/°C`.
///
/// # Rules
/// - The text must mention `ppm`; the numeric part is returned.
/// # Do not
/// - Guess when the unit is missing.
pub fn parse_temperature_coefficient(text: &str) -> Option<f64> {
    let lower = text.to_lowercase();
    if !lower.contains("ppm") {
        return None;
    }
    crate::listing::parse_tolerance(text)
        .map(|(a, _)| f64::from(a))
        .or_else(|| {
            text.trim()
                .trim_start_matches(['±', '+', '-'])
                .split_whitespace()
                .next()?
                .parse()
                .ok()
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temperature_range_parses_signed_span() {
        assert_eq!(
            parse_temperature_range("-55°C ~ 125°C"),
            Some((-55.0, 125.0))
        );
    }

    #[test]
    fn temperature_coefficient_reads_ppm() {
        assert_eq!(parse_temperature_coefficient("±30 ppm/°C"), Some(30.0));
    }

    #[test]
    fn rated_voltage_key_follows_attribute_wording() {
        assert_eq!(
            rated_voltage_key("Voltage - Rated AC"),
            ParamKey::RatedVoltageAc
        );
        assert_eq!(
            rated_voltage_key("Voltage - Rated"),
            ParamKey::RatedVoltageDc
        );
    }

    #[test]
    fn heuristics_parse_ac_rated_voltage_and_package() {
        let aliases = ParamAliasFile::new();
        let mut attrs = BTreeMap::from([
            (
                "Voltage - Rated AC".into(),
                "250 VAC".into(),
            ),
            (
                "Package / Case".into(),
                "0603 (1608 Metric)".into(),
            ),
        ]);
        let out = params_from_attributes(
            "Sample parts",
            "Chip Resistor - Surface Mount",
            "resistor",
            &attrs,
            &aliases,
            0,
        );
        assert_eq!(out.params.len(), 2);
        assert!(out.params.iter().any(|p| {
            p.key == ParamKey::RatedVoltageAc
                && matches!(p.payload, ParamPayload::Number { value } if (value - 250.0).abs() < 1e-6)
        }));
        assert!(out.params.iter().any(|p| {
            p.key == ParamKey::PackageKey
                && matches!(&p.payload, ParamPayload::Text { value } if value.contains("0603"))
        }));
    }

    #[test]
    fn pitch_height_and_pin_count_parse_from_aliases() {
        let aliases = load_param_aliases(
            r#"{
              "fixture": {
                "resistor": {
                  "Pitch": "pitch",
                  "Height": "height",
                  "Pin Count": "pin_count"
                }
              }
            }"#,
        )
        .expect("alias json");
        let attrs = BTreeMap::from([
            ("Pitch".into(), "1.6 mm".into()),
            ("Height".into(), "0.55mm".into()),
            ("Pin Count".into(), "2".into()),
        ]);
        let out = params_from_attributes("fixture", "Resistors", "resistor", &attrs, &aliases, 0);
        let pitch = out
            .params
            .iter()
            .find(|p| p.key == ParamKey::Pitch)
            .and_then(|p| match p.payload {
                ParamPayload::Number { value } => Some(value),
                _ => None,
            });
        let height = out
            .params
            .iter()
            .find(|p| p.key == ParamKey::Height)
            .and_then(|p| match p.payload {
                ParamPayload::Number { value } => Some(value),
                _ => None,
            });
        let pins = out
            .params
            .iter()
            .find(|p| p.key == ParamKey::PinCount)
            .and_then(|p| match p.payload {
                ParamPayload::Number { value } => Some(value),
                _ => None,
            });
        assert!((pitch.unwrap_or(0.0) - 1.6).abs() < 1e-6);
        assert!((height.unwrap_or(0.0) - 0.55).abs() < 1e-6);
        assert!((pins.unwrap_or(0.0) - 2.0).abs() < 1e-6);
    }

    #[test]
    fn temperature_coefficient_parses_ppm_or_keeps_text() {
        let aliases = load_param_aliases(
            r#"{
              "fixture": {
                "resistor": { "Temp Coeff": "temperature_coefficient" }
              }
            }"#,
        )
        .expect("alias json");
        let ppm = BTreeMap::from([("Temp Coeff".into(), "±30 ppm/°C".into())]);
        let parsed = params_from_attributes(
            "fixture",
            "Resistors",
            "resistor",
            &ppm,
            &aliases,
            0,
        );
        assert!(parsed.params.iter().any(|p| {
            p.key == ParamKey::TemperatureCoefficient
                && matches!(p.payload, ParamPayload::Number { value } if (value - 30.0).abs() < 1e-6)
        }));
        let raw = BTreeMap::from([("Temp Coeff".into(), "C0G".into())]);
        let text = params_from_attributes(
            "fixture",
            "Resistors",
            "resistor",
            &raw,
            &aliases,
            0,
        );
        assert!(text.params.iter().any(|p| {
            p.key == ParamKey::TemperatureCoefficient
                && matches!(&p.payload, ParamPayload::Text { value } if value == "C0G")
        }));
    }
}
