//! Read-only drop-in replacement check: footprint, pinout, ratings and lifecycle.
use crate::listing::{Category, Lifecycle, Unit};
use serde::Deserialize;
use std::collections::BTreeMap;

const MAX_PINS: usize = 512;
const MAX_PADS: usize = 512;
const MAX_PIN_TO_PAD: usize = 512;
const MAX_STRING: usize = 256;

fn default_category() -> Category {
    Category::Other
}

/// One schematic pin in a drop-in comparison snapshot.
///
/// # Rules
/// - `number` is required and must match between incumbent and candidate.
/// - `name` and `electrical_type` must match when both sides supply them.
///
/// # Dependencies
/// - crates/library/src/drop_in_check.rs `DropInPart`
///
/// # Do not
/// - Treat an empty pin number as a wildcard.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DropInPin {
    pub number: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub electrical_type: String,
}

/// Part snapshot for drop-in comparison.
///
/// # Rules
/// - Optional numeric and tolerance fields that are missing are treated as unknown.
/// - Passives require every electrical rating to be known on both parts.
/// - `lifecycle` must be present in JSON input.
///
/// # Dependencies
/// - crates/library/src/listing.rs `Category`, `Lifecycle`, `Unit`
///
/// # Do not
/// - Infer footprint match from empty `footprint` and `package_key`.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DropInPart {
    #[serde(default)]
    pub footprint: String,
    #[serde(default)]
    pub package_key: String,
    #[serde(default)]
    pub pads: Vec<String>,
    #[serde(default)]
    pub pins: Vec<DropInPin>,
    #[serde(default)]
    pub pin_to_pad: BTreeMap<String, Vec<String>>,
    #[serde(default = "default_category")]
    pub category: Category,
    pub lifecycle: Lifecycle,
    pub value: Option<f64>,
    pub value_unit: Option<Unit>,
    pub tolerance_pct: Option<(f32, f32)>,
    pub rated_voltage_v: Option<f64>,
    pub temp_min_c: Option<f32>,
    pub temp_max_c: Option<f32>,
    pub power_w: Option<f64>,
    pub height_mm: Option<f64>,
}

/// One pass or fail line in a drop-in report.
///
/// # Rules
/// - `code` is a stable machine-readable key such as `footprint` or `rated_voltage_v`.
/// - `pass` is false when the candidate fails that check.
///
/// # Dependencies
/// - crates/library/src/drop_in_check.rs `DropInReport`
///
/// # Do not
/// - Change `code` values without updating callers and tests.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct DropInFinding {
    pub code: String,
    pub pass: bool,
    pub message: String,
}

/// Overall answer for whether the candidate may replace the incumbent on the board.
///
/// # Rules
/// - `drop_in` is true only when every finding passes.
/// - Findings are ordered footprint, pinout, ratings, lifecycle, then height.
///
/// # Dependencies
/// - crates/library/src/drop_in_check.rs `DropInFinding`
///
/// # Do not
/// - Treat `drop_in` as true when any finding failed.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct DropInReport {
    pub drop_in: bool,
    pub findings: Vec<DropInFinding>,
}

impl DropInPart {
    fn validate(&self) -> Result<(), String> {
        if self.footprint.len() > MAX_STRING || self.package_key.len() > MAX_STRING {
            return Err("footprint and package_key may be at most 256 bytes".into());
        }
        if self.pins.len() > MAX_PINS {
            return Err(format!("at most {MAX_PINS} pins may be compared"));
        }
        if self.pads.len() > MAX_PADS {
            return Err(format!("at most {MAX_PADS} pads may be compared"));
        }
        if self.pin_to_pad.len() > MAX_PIN_TO_PAD {
            return Err(format!("at most {MAX_PIN_TO_PAD} pin_to_pad entries may be compared"));
        }
        for pin in &self.pins {
            if pin.number.is_empty() || pin.number.len() > 32 {
                return Err("each pin needs a number up to 32 bytes".into());
            }
            if pin.name.len() > MAX_STRING || pin.electrical_type.len() > MAX_STRING {
                return Err("pin name and electrical_type may be at most 256 bytes".into());
            }
        }
        for pad in &self.pads {
            if pad.is_empty() || pad.len() > 32 {
                return Err("each pad number may be up to 32 bytes and must not be empty".into());
            }
        }
        for (pin, pads) in &self.pin_to_pad {
            if pin.is_empty() || pin.len() > 32 {
                return Err("pin_to_pad keys may be up to 32 bytes and must not be empty".into());
            }
            if pads.is_empty() || pads.len() > MAX_PADS {
                return Err("each pin_to_pad entry needs one to 512 pad numbers".into());
            }
            for pad in pads {
                if pad.is_empty() || pad.len() > 32 {
                    return Err("pad numbers in pin_to_pad may be up to 32 bytes".into());
                }
            }
        }
        for value in [
            self.value,
            self.rated_voltage_v,
            self.power_w,
            self.height_mm,
        ]
        .into_iter()
        .flatten()
        {
            if !value.is_finite() || value < 0.0 {
                return Err("numeric ratings must be finite and not negative".into());
            }
        }
        if let Some((a, b)) = self.tolerance_pct
            && (!a.is_finite() || !b.is_finite() || a < 0.0 || b < 0.0 || a > 150.0 || b > 150.0)
        {
            return Err("tolerance must be two percentages from 0 to 150".into());
        }
        if let (Some(min), Some(max)) = (self.temp_min_c, self.temp_max_c)
            && (!min.is_finite() || !max.is_finite() || min > max)
        {
            return Err("temperature limits must be finite with min not above max".into());
        }
        Ok(())
    }
}

fn finding(code: &str, pass: bool, message: impl Into<String>) -> DropInFinding {
    DropInFinding {
        code: code.into(),
        pass,
        message: message.into(),
    }
}

fn lifecycle_rank(l: Lifecycle) -> u8 {
    match l {
        Lifecycle::Active => 0,
        Lifecycle::Nrnd => 1,
        Lifecycle::LastTimeBuy => 2,
        Lifecycle::Obsolete => 3,
        Lifecycle::Unknown => 4,
    }
}

fn footprint_matches(inc: &DropInPart, cand: &DropInPart) -> DropInFinding {
    let (inc_key, cand_key) = (
        inc.package_key.trim(),
        cand.package_key.trim(),
    );
    if !inc_key.is_empty() && !cand_key.is_empty() {
        let pass = inc_key == cand_key;
        return finding(
            "footprint",
            pass,
            if pass {
                "Package keys match".into()
            } else {
                format!("Package keys differ ({inc_key} vs {cand_key})")
            },
        );
    }
    let (inc_fp, cand_fp) = (inc.footprint.trim(), cand.footprint.trim());
    if inc_fp.is_empty() || cand_fp.is_empty() {
        return finding(
            "footprint",
            false,
            "Footprint or package is unknown on at least one part".into(),
        );
    }
    let pass = inc_fp.eq_ignore_ascii_case(cand_fp);
    finding(
        "footprint",
        pass,
        if pass {
            "Footprint references match".into()
        } else {
            format!("Footprint references differ ({inc_fp} vs {cand_fp})")
        },
    )
}

fn sorted_numbers(items: &[String]) -> Vec<String> {
    let mut out = items.to_vec();
    out.sort();
    out
}

fn pinout_matches(inc: &DropInPart, cand: &DropInPart) -> Vec<DropInFinding> {
    let mut out = vec![];
    let inc_nums: Vec<_> = inc.pins.iter().map(|p| p.number.as_str()).collect();
    let cand_nums: Vec<_> = cand.pins.iter().map(|p| p.number.as_str()).collect();
    if inc_nums.is_empty() && cand_nums.is_empty() {
        out.push(finding(
            "pinout",
            false,
            "Neither part lists schematic pins".into(),
        ));
        return out;
    }
    let mut inc_sorted = inc_nums.clone();
    inc_sorted.sort();
    let mut cand_sorted = cand_nums.clone();
    cand_sorted.sort();
    if inc_sorted != cand_sorted {
        out.push(finding(
            "pinout",
            false,
            "Pin numbers differ between the two parts".into(),
        ));
        return out;
    }
    out.push(finding("pinout", true, "Pin numbers match".into()));
    let mut type_ok = true;
    let mut name_ok = true;
    for number in inc_sorted {
        let inc_pin = inc
            .pins
            .iter()
            .find(|p| p.number == number)
            .map(|p| (p.electrical_type.as_str(), p.name.as_str()));
        let cand_pin = cand
            .pins
            .iter()
            .find(|p| p.number == number)
            .map(|p| (p.electrical_type.as_str(), p.name.as_str()));
        if let (Some((it, iname)), Some((ct, cname))) = (inc_pin, cand_pin) {
            if !it.eq_ignore_ascii_case(ct) {
                type_ok = false;
            }
            if iname != cname {
                name_ok = false;
            }
        }
    }
    out.push(finding(
        "pin_electrical_type",
        type_ok,
        if type_ok {
            "Electrical types match on every pin".into()
        } else {
            "At least one pin has a different electrical type".into()
        },
    ));
    out.push(finding(
        "pin_name",
        name_ok,
        if name_ok {
            "Pin names match on every pin".into()
        } else {
            "At least one pin has a different name".into()
        },
    ));
    if !inc.pads.is_empty() || !cand.pads.is_empty() {
        let inc_pads = sorted_numbers(&inc.pads);
        let cand_pads = sorted_numbers(&cand.pads);
        out.push(finding(
            "pads",
            inc_pads == cand_pads,
            if inc_pads == cand_pads {
                "Footprint pad numbers match".into()
            } else {
                "Footprint pad numbers differ".into()
            },
        ));
    }
    if !inc.pin_to_pad.is_empty() || !cand.pin_to_pad.is_empty() {
        let pass = inc.pin_to_pad == cand.pin_to_pad;
        out.push(finding(
            "pin_to_pad",
            pass,
            if pass {
                "Pin to pad maps match".into()
            } else {
                "Pin to pad maps differ".into()
            },
        ));
    }
    out
}

fn passive_category(category: Category) -> bool {
    matches!(
        category,
        Category::Resistor | Category::Capacitor | Category::Inductor | Category::Crystal
    )
}

fn needs_power(category: Category) -> bool {
    matches!(category, Category::Resistor | Category::Inductor)
}

fn value_close(a: f64, b: f64) -> bool {
    let scale = a.abs().max(b.abs());
    (a - b).abs() <= scale * 1e-6 + 1e-12
}

fn tolerance_no_worse(inc: (f32, f32), cand: (f32, f32)) -> bool {
    cand.0 <= inc.0 + 1e-6 && cand.1 <= inc.1 + 1e-6
}

fn compare_optional_f64(field: &str, inc: Option<f64>, cand: Option<f64>, at_least: bool) -> DropInFinding {
    match (inc, cand) {
        (None, None) => finding(field, true, format!("No {field} on either part")),
        (Some(_), None) | (None, Some(_)) => finding(
            field,
            false,
            format!("{field} is unknown on one part"),
        ),
        (Some(i), Some(c)) => {
            let pass = if at_least { c + 1e-12 >= i } else { value_close(i, c) };
            finding(
                field,
                pass,
                if pass {
                    format!("{field} is acceptable")
                } else if at_least {
                    format!("{field} on the candidate is below the incumbent")
                } else {
                    format!("{field} differs between the parts")
                },
            )
        }
    }
}

fn compare_temp(inc: &DropInPart, cand: &DropInPart) -> DropInFinding {
    match (
        (inc.temp_min_c, inc.temp_max_c),
        (cand.temp_min_c, cand.temp_max_c),
    ) {
        ((None, None), (None, None)) => {
            finding("temperature", true, "No temperature span on either part".into())
        }
        ((Some(_), None) | (None, Some(_)), _) | (_, (Some(_), None) | (None, Some(_))) => finding(
            "temperature",
            false,
            "Temperature span is incomplete on at least one part".into(),
        ),
        ((Some(i_min), Some(i_max)), (Some(c_min), Some(c_max))) => {
            let pass = c_min <= i_min + 1e-6 && c_max + 1e-6 >= i_max;
            finding(
                "temperature",
                pass,
                if pass {
                    "Operating temperature span is at least as wide".into()
                } else {
                    "Operating temperature span is narrower on the candidate".into()
                },
            )
        }
    }
}

fn compare_tolerance(inc: &DropInPart, cand: &DropInPart) -> DropInFinding {
    match (inc.tolerance_pct, cand.tolerance_pct) {
        (None, None) => finding("tolerance", true, "No tolerance on either part".into()),
        (Some(_), None) | (None, Some(_)) => {
            finding("tolerance", false, "Tolerance is unknown on one part".into())
        }
        (Some(i), Some(c)) => {
            let pass = tolerance_no_worse(i, c);
            finding(
                "tolerance",
                pass,
                if pass {
                    "Tolerance is the same or tighter".into()
                } else {
                    "Tolerance is looser on the candidate".into()
                },
            )
        }
    }
}

fn compare_value(inc: &DropInPart, cand: &DropInPart) -> Vec<DropInFinding> {
    let mut out = vec![];
    match (inc.value, cand.value) {
        (None, None) => out.push(finding("value", true, "No value on either part".into())),
        (Some(_), None) | (None, Some(_)) => {
            out.push(finding("value", false, "Value is unknown on one part".into()));
        }
        (Some(i), Some(c)) => {
            out.push(finding(
                "value",
                value_close(i, c),
                if value_close(i, c) {
                    "Nominal values match".into()
                } else {
                    "Nominal values differ".into()
                },
            ));
        }
    }
    match (inc.value_unit, cand.value_unit) {
        (None, None) => out.push(finding("value_unit", true, "No value unit on either part".into())),
        (Some(_), None) | (None, Some(_)) => {
            out.push(finding(
                "value_unit",
                false,
                "Value unit is unknown on one part".into(),
            ));
        }
        (Some(i), Some(c)) => out.push(finding(
            "value_unit",
            i == c,
            if i == c {
                "Value units match".into()
            } else {
                "Value units differ".into()
            },
        )),
    }
    out
}

fn require_known(field: &str, inc: bool, cand: bool) -> Option<DropInFinding> {
    if inc && cand {
        None
    } else {
        Some(finding(
            field,
            false,
            format!("{field} must be known on both parts"),
        ))
    }
}

fn rating_findings(inc: &DropInPart, cand: &DropInPart) -> Vec<DropInFinding> {
    let mut out = vec![];
    let category = if passive_category(inc.category) {
        inc.category
    } else if passive_category(cand.category) {
        cand.category
    } else {
        inc.category
    };
    if passive_category(category) {
        if let Some(f) = require_known("value", inc.value.is_some(), cand.value.is_some()) {
            out.push(f);
        }
        if let Some(f) =
            require_known("value_unit", inc.value_unit.is_some(), cand.value_unit.is_some())
        {
            out.push(f);
        }
        if let Some(f) = require_known(
            "tolerance",
            inc.tolerance_pct.is_some(),
            cand.tolerance_pct.is_some(),
        ) {
            out.push(f);
        }
        if let Some(f) = require_known(
            "rated_voltage_v",
            inc.rated_voltage_v.is_some(),
            cand.rated_voltage_v.is_some(),
        ) {
            out.push(f);
        }
        if let Some(f) = require_known(
            "temperature",
            inc.temp_min_c.is_some() && inc.temp_max_c.is_some(),
            cand.temp_min_c.is_some() && cand.temp_max_c.is_some(),
        ) {
            out.push(f);
        }
        if needs_power(category)
            && let Some(f) = require_known(
                "power_w",
                inc.power_w.is_some(),
                cand.power_w.is_some(),
            )
        {
            out.push(f);
        }
        let ratings_ready = inc.value.is_some()
            && cand.value.is_some()
            && inc.value_unit.is_some()
            && cand.value_unit.is_some()
            && inc.tolerance_pct.is_some()
            && cand.tolerance_pct.is_some()
            && inc.rated_voltage_v.is_some()
            && cand.rated_voltage_v.is_some()
            && inc.temp_min_c.is_some()
            && inc.temp_max_c.is_some()
            && cand.temp_min_c.is_some()
            && cand.temp_max_c.is_some()
            && (!needs_power(category) || (inc.power_w.is_some() && cand.power_w.is_some()));
        if ratings_ready {
            out.extend(compare_value(inc, cand));
            out.push(compare_tolerance(inc, cand));
            out.push(compare_optional_f64(
                "rated_voltage_v",
                inc.rated_voltage_v,
                cand.rated_voltage_v,
                true,
            ));
            out.push(compare_temp(inc, cand));
            if needs_power(category) {
                out.push(compare_optional_f64(
                    "power_w",
                    inc.power_w,
                    cand.power_w,
                    true,
                ));
            }
        }
        return out;
    }
    if inc.value.is_some() || cand.value.is_some() {
        out.extend(compare_value(inc, cand));
    }
    if inc.tolerance_pct.is_some() || cand.tolerance_pct.is_some() {
        out.push(compare_tolerance(inc, cand));
    }
    if inc.rated_voltage_v.is_some() || cand.rated_voltage_v.is_some() {
        out.push(compare_optional_f64(
            "rated_voltage_v",
            inc.rated_voltage_v,
            cand.rated_voltage_v,
            true,
        ));
    }
    if inc.power_w.is_some() || cand.power_w.is_some() {
        out.push(compare_optional_f64("power_w", inc.power_w, cand.power_w, true));
    }
    if inc.temp_min_c.is_some()
        || inc.temp_max_c.is_some()
        || cand.temp_min_c.is_some()
        || cand.temp_max_c.is_some()
    {
        out.push(compare_temp(inc, cand));
    }
    out
}

fn lifecycle_finding(inc: &DropInPart, cand: &DropInPart) -> DropInFinding {
    if cand.lifecycle == Lifecycle::Unknown {
        return finding(
            "lifecycle",
            false,
            "Candidate lifecycle is unknown".into(),
        );
    }
    if inc.lifecycle != Lifecycle::Unknown && lifecycle_rank(cand.lifecycle) > lifecycle_rank(inc.lifecycle)
    {
        return finding(
            "lifecycle",
            false,
            "Candidate lifecycle is worse than the incumbent".into(),
        );
    }
    finding("lifecycle", true, "Lifecycle is acceptable".into())
}

fn height_finding(inc: &DropInPart, cand: &DropInPart) -> DropInFinding {
    match (inc.height_mm, cand.height_mm) {
        (None, None) => finding("height_mm", true, "No height on either part".into()),
        (Some(_), None) | (None, Some(_)) => {
            finding("height_mm", false, "Height is unknown on one part".into())
        }
        (Some(i), Some(c)) => finding(
            "height_mm",
            c <= i + 1e-9,
            if c <= i + 1e-9 {
                "Candidate height is not taller".into()
            } else {
                "Candidate is taller than the incumbent".into()
            },
        ),
    }
}

/// Compare a candidate part against the incumbent already on the board.
///
/// # Rules
/// Drop-in requires matching footprint, matching pinout, ratings at least as
/// good where both are known, lifecycle not worse, and height not greater.
/// For passives every electrical field must be known on both parts. Unknown
/// lifecycle on the candidate always fails.
/// # Dependencies
/// None.
/// # Do not
/// Guess missing ratings or treat empty footprint and package as a match.
pub fn drop_in_check(incumbent: &DropInPart, candidate: &DropInPart) -> Result<DropInReport, String> {
    incumbent.validate()?;
    candidate.validate()?;
    let mut findings = vec![footprint_matches(incumbent, candidate)];
    findings.extend(pinout_matches(incumbent, candidate));
    findings.extend(rating_findings(incumbent, candidate));
    findings.push(lifecycle_finding(incumbent, candidate));
    findings.push(height_finding(incumbent, candidate));
    let drop_in = findings.iter().all(|f| f.pass);
    Ok(DropInReport { drop_in, findings })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resistor_10k() -> DropInPart {
        DropInPart {
            footprint: "lib/resistor-0603".into(),
            package_key: "chip-0603".into(),
            pads: vec!["1".into(), "2".into()],
            pins: vec![
                DropInPin {
                    number: "1".into(),
                    name: "~".into(),
                    electrical_type: "passive".into(),
                },
                DropInPin {
                    number: "2".into(),
                    name: "~".into(),
                    electrical_type: "passive".into(),
                },
            ],
            pin_to_pad: BTreeMap::new(),
            category: Category::Resistor,
            lifecycle: Lifecycle::Active,
            value: Some(10_000.0),
            value_unit: Some(Unit::Ohm),
            tolerance_pct: Some((1.0, 1.0)),
            rated_voltage_v: Some(50.0),
            temp_min_c: Some(-55.0),
            temp_max_c: Some(155.0),
            power_w: Some(0.1),
            height_mm: Some(0.55),
        }
    }

    #[test]
    fn identical_passives_are_drop_in() -> Result<(), String> {
        let inc = resistor_10k();
        let cand = inc.clone();
        let report = drop_in_check(&inc, &cand)?;
        assert!(report.drop_in);
        Ok(())
    }

    #[test]
    fn a_better_rating_with_the_same_footprint_passes() -> Result<(), String> {
        let inc = resistor_10k();
        let mut cand = inc.clone();
        cand.rated_voltage_v = Some(75.0);
        cand.power_w = Some(0.125);
        assert!(drop_in_check(&inc, &cand)?.drop_in);
        Ok(())
    }

    #[test]
    fn unknown_voltage_on_the_candidate_fails() -> Result<(), String> {
        let inc = resistor_10k();
        let mut cand = inc.clone();
        cand.rated_voltage_v = None;
        let report = drop_in_check(&inc, &cand)?;
        assert!(!report.drop_in);
        assert!(report.findings.iter().any(|f| f.code == "rated_voltage_v" && !f.pass));
        Ok(())
    }

    #[test]
    fn a_different_package_fails() -> Result<(), String> {
        let inc = resistor_10k();
        let mut cand = inc.clone();
        cand.package_key = "chip-0805".into();
        assert!(!drop_in_check(&inc, &cand)?.drop_in);
        Ok(())
    }

    #[test]
    fn a_taller_part_fails() -> Result<(), String> {
        let inc = resistor_10k();
        let mut cand = inc.clone();
        cand.height_mm = Some(0.8);
        assert!(!drop_in_check(&inc, &cand)?.drop_in);
        Ok(())
    }
}
