use crate::error::{check_range, CalcError};
use crate::Accuracy;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UnitConvertOut {
    pub result: f64,
    pub accuracy: Accuracy,
}

fn factor(from: &str, to: &str) -> Result<f64, CalcError> {
    let table: &[(&str, &str, f64)] = &[
        ("mm", "mil", 39.370_078_7),
        ("mil", "mm", 0.025_4),
        ("inch", "mm", 25.4),
        ("mm", "inch", 1.0 / 25.4),
        ("f", "uf", 1e6),
        ("uf", "nf", 1000.0),
        ("hz", "khz", 0.001),
        ("khz", "mhz", 0.001),
        ("w", "mw", 1000.0),
        ("v", "mv", 1000.0),
        ("a", "ma", 1000.0),
    ];
    for (a, b, k) in table {
        if from.eq_ignore_ascii_case(a) && to.eq_ignore_ascii_case(b) {
            return Ok(*k);
        }
    }
    Err(CalcError::Domain {
        message: "unsupported unit pair",
    })
}

pub fn unit_converter(value: f64, from_unit: &str, to_unit: &str) -> Result<UnitConvertOut, CalcError> {
    check_range("value", value, -1e12, 1e12)?;
    let k = factor(from_unit, to_unit)?;
    Ok(UnitConvertOut {
        result: value * k,
        accuracy: Accuracy::Exact,
    })
}
