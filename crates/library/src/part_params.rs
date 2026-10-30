//! Normalised part parameters with units, per-source aliases and confidence.
//!
//! Local rules match [`crate::listing`]: empty means unknown and nothing is
//! guessed. Raw source text is kept beside every parsed value. No network here.
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
    Capacitance,
    Resistance,
    Inductance,
    RatedVoltageDc,
    RatedVoltageAc,
    RatedCurrent,
    Power,
    ToleranceRange,
    TemperatureRange,
    Dielectric,
    Esr,
    Frequency,
    ForwardVoltage,
    DrainSourceVoltage,
    OnResistance,
    PackageKey,
    PinCount,
    Pitch,
    Height,
    MountType,
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
    Number { value: f64 },
    Range { low: f64, high: f64 },
    Text { value: String },
}

/// One normalised parameter from one source answer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParamValue {
    pub key: ParamKey,
    pub payload: ParamPayload,
    /// SI unit for numeric keys; empty for text keys and temperature (Celsius).
    pub unit: Option<Unit>,
    pub raw: String,
    pub source: String,
    pub retrieved_at: u64,
    pub confidence: ParamConfidence,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conflicts: Vec<ParamConflictEntry>,
}

/// Another source's disagreeing value kept beside a conflict.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParamConflictEntry {
    pub payload: ParamPayload,
    pub unit: Option<Unit>,
    pub raw: String,
    pub source: String,
    pub retrieved_at: u64,
}

/// A source attribute that did not map to a closed key.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtraAttribute {
    pub name: String,
    pub value: String,
}

/// Parsed parameters plus unmapped attributes for one part answer.
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartParams {
    pub params: Vec<ParamValue>,
    pub extras: Vec<ExtraAttribute>,
}

/// Per-source alias table: category name, then attribute name to param key.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParamAliasFile {
    #[serde(flatten)]
    pub sources: BTreeMap<String, BTreeMap<String, BTreeMap<String, ParamKey>>>,
}

/// Read alias JSON: top-level keys are source ids; then category snake names.
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
        Category::Crystal => &[Frequency, ToleranceRange, TemperatureRange, PackageKey, MountType],
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
            (u == Unit::Volt).then_some((
                ParamPayload::Number { value: v },
                Some(Unit::Volt),
            ))
        }
        ToleranceRange => {
            let (lo, hi) = tolerance_from(trimmed, category)
                .or_else(|| parse_tolerance(trimmed).map(|(a, b)| (a, b)))?;
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
            Some((
                ParamPayload::Text { value },
                None,
            ))
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
        MountType | Dielectric | TemperatureCoefficient => Some((
            ParamPayload::Text {
                value: trimmed.to_string(),
            },
            None,
        )),
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
    if let Some(aliases) = table {
        if let Some(key) = aliases
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(attribute))
            .map(|(_, v)| *v)
            .filter(|key| key_allowed(category, *key))
        {
            return Some(key);
        }
    }
    heuristic_key(category, attribute)
}

/// Normalise one source attribute map using aliases and the part category.
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
        .sources
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
    params.sort_by(|a, b| a.key.cmp(&b.key));
    extras.sort_by(|a, b| a.name.cmp(&b.name));
    PartParams { params, extras }
}

fn payloads_equal(a: &ParamPayload, b: &ParamPayload) -> bool {
    match (a, b) {
        (ParamPayload::Number { value: x }, ParamPayload::Number { value: y }) => {
            (x - y).abs() <= (x.abs().max(y.abs()) * 1e-9).max(1e-12)
        }
        (
            ParamPayload::Range {
                low: al,
                high: ah,
            },
            ParamPayload::Range {
                low: bl,
                high: bh,
            },
        ) => (al - bl).abs() <= 1e-6 && (ah - bh).abs() <= 1e-6,
        (ParamPayload::Text { value: x }, ParamPayload::Text { value: y }) => {
            x.eq_ignore_ascii_case(y)
        }
        _ => false,
    }
}

/// Merge parameters from two sources for the same part. Nothing is guessed.
pub fn merge_part_params(mut a: PartParams, mut b: PartParams) -> PartParams {
    let mut extras = std::mem::take(&mut a.extras);
    extras.append(&mut b.extras);
    let mut by_key: BTreeMap<ParamKey, Vec<ParamValue>> = BTreeMap::new();
    for p in a.params.drain(..).chain(b.params.drain(..)) {
        by_key.entry(p.key).or_default().push(p);
    }
    let mut params = Vec::new();
    for (_key, mut group) in by_key {
        group.sort_by(|x, y| x.source.cmp(&y.source));
        if group.len() == 1 {
            let mut only = group.pop().expect("one");
            only.confidence = ParamConfidence::Single;
            params.push(only);
            continue;
        }
        let first = &group[0];
        if group.iter().all(|p| payloads_equal(&p.payload, &first.payload)) {
            let mut merged = group.pop().expect("non-empty");
            merged.confidence = ParamConfidence::Agreed;
            merged.conflicts.clear();
            params.push(merged);
            continue;
        }
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
    params.sort_by(|a, b| a.key.cmp(&b.key));
    extras.sort_by(|x, y| x.name.cmp(&y.name));
    extras.dedup_by(|x, y| x.name.eq_ignore_ascii_case(&y.name) && x.value == y.value);
    PartParams { params, extras }
}

/// Parse a temperature coefficient string such as `±30 ppm/°C`.
pub fn parse_temperature_coefficient(text: &str) -> Option<f64> {
    let lower = text.to_lowercase();
    if !lower.contains("ppm") {
        return None;
    }
    let nums = crate::listing::parse_tolerance(text)
        .map(|(a, _)| f64::from(a))
        .or_else(|| {
            text.trim()
                .trim_start_matches(['±', '+', '-'])
                .split_whitespace()
                .next()?
                .parse()
                .ok()
        });
    nums
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
}
