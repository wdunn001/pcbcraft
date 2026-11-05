//! E-series passive value tables and EIA value codes.
//!
//! # Rules
//! - Values are returned as f64 mantissas in the range [1.0, 10.0).
//! - value_code uses the R/K/M and P/N/U marker system.
//!
//! # Dependencies
//! - None (pure computation).

/// Standard E-series used for passive components.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ESeries {
    /// 6 values per decade.
    E6,
    /// 12 values per decade.
    E12,
    /// 24 values per decade.
    E24,
    /// 96 values per decade.
    E96,
}

/// Kind of passive component for value code formatting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PassiveKind {
    Resistor,
    Capacitor,
}

/// Return the decade values for an E-series (1.0 to 9.x).
pub fn decade(series: ESeries) -> &'static [f64] {
    match series {
        ESeries::E6 => E6_VALUES,
        ESeries::E12 => E12_VALUES,
        ESeries::E24 => E24_VALUES,
        ESeries::E96 => E96_VALUES,
    }
}

/// Return every E-series value in [min, max] sorted ascending.
///
/// # Rules
/// - min must be positive, max must be >= min.
/// - At most 2000 values are returned.
/// - Values are rounded to 12 significant digits.
pub fn values_between(series: ESeries, min: f64, max: f64) -> Result<Vec<f64>, String> {
    if min <= 0.0 {
        return Err("min must be positive".into());
    }
    if max < min {
        return Err("max must be >= min".into());
    }
    let base = decade(series);
    let mut results = Vec::new();
    let min_exp = (min.log10()).floor();
    let max_exp = (max.log10()).ceil();
    for exp in (min_exp..=max_exp).map(|e| e as i32) {
        let scale = 10.0f64.powi(exp);
        for &v in base {
            let val = v * scale;
            if val >= min && val <= max {
                results.push(round_sig(val, 12));
            }
        }
    }
    results.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    if results.len() > 2000 {
        return Err("more than 2000 values".into());
    }
    Ok(results)
}

/// Format a passive value as an EIA value code string.
///
/// # Rules
/// - Resistors use R/K/M markers.
/// - Capacitors use P/N/U markers.
/// - Mantissa is scaled to [1, 1000) with at most 3 significant digits.
pub fn value_code(value: f64, kind: PassiveKind) -> Result<String, String> {
    if value <= 0.0 {
        return Err("value must be positive".into());
    }
    match kind {
        PassiveKind::Resistor => resistor_code(value),
        PassiveKind::Capacitor => capacitor_code(value),
    }
}

fn resistor_code(value: f64) -> Result<String, String> {
    let (mantissa, exp) = scale_value(value);
    let marker = match exp {
        0 => 'R',
        3 => 'K',
        6 => 'M',
        _ => return Err(format!("unsupported exponent {}", exp)),
    };
    format_code(mantissa, marker)
}

fn capacitor_code(value: f64) -> Result<String, String> {
    let (mantissa, exp) = scale_value(value);
    let marker = match exp {
        -12 => 'P',
        -9 => 'N',
        -6 => 'U',
        _ => return Err(format!("unsupported exponent {}", exp)),
    };
    format_code(mantissa, marker)
}

fn scale_value(value: f64) -> (f64, i32) {
    let log10 = value.log10();
    let exp = ((log10 - 1.0) / 3.0).floor() * 3.0;
    let mantissa = value / 10.0f64.powi(exp as i32);
    (mantissa, exp as i32)
}

fn format_code(mantissa: f64, marker: char) -> Result<String, String> {
    let mantissa = round_sig(mantissa, 3);
    if mantissa < 1.0 || mantissa >= 1000.0 {
        return Err("mantissa out of range".into());
    }
    let s = format!("{}", mantissa);
    if s.contains('.') {
        let parts: Vec<&str> = s.split('.').collect();
        let whole = parts[0];
        let frac = parts[1];
        if frac.is_empty() {
            return Ok(format!("{}{}", whole, marker));
        }
        let digit = frac.chars().next().unwrap_or('0');
        return Ok(format!("{}{}{}", whole, digit, marker));
    }
    if s.len() == 1 {
        return Ok(format!("{}0{}", s, marker));
    }
    Ok(format!("{}{}", s, marker))
}

fn round_sig(value: f64, digits: u32) -> f64 {
    if value == 0.0 {
        return 0.0;
    }
    let log10 = value.abs().log10();
    let magnitude = log10.floor() as i32;
    let scale = 10.0f64.powi((digits as i32) - 1 - magnitude);
    (value * scale).round() / scale
}

static E6_VALUES: [f64; 6] = [1.0, 1.5, 2.2, 3.3, 4.7, 6.8];

static E12_VALUES: [f64; 12] = [1.0, 1.2, 1.5, 1.8, 2.2, 2.7, 3.3, 3.9, 4.7, 5.6, 6.8, 8.2];

static E24_VALUES: [f64; 24] = [
    1.0, 1.1, 1.2, 1.3, 1.5, 1.6, 1.8, 2.0, 2.2, 2.4, 2.7, 3.0, 3.3, 3.6, 3.9, 4.3, 4.7, 5.1, 5.6,
    6.2, 6.8, 7.5, 8.2, 9.1,
];

static E96_VALUES: [f64; 96] = [
    1.00, 1.02, 1.05, 1.07, 1.10, 1.13, 1.15, 1.18, 1.21, 1.24, 1.27, 1.30, 1.33, 1.37, 1.40, 1.43,
    1.47, 1.50, 1.54, 1.58, 1.62, 1.65, 1.69, 1.74, 1.78, 1.82, 1.87, 1.91, 1.96, 2.00, 2.05, 2.10,
    2.15, 2.21, 2.26, 2.32, 2.37, 2.43, 2.49, 2.55, 2.61, 2.67, 2.74, 2.80, 2.87, 2.94, 3.01, 3.09,
    3.16, 3.24, 3.32, 3.40, 3.48, 3.57, 3.65, 3.74, 3.83, 3.92, 4.02, 4.12, 4.22, 4.32, 4.42, 4.53,
    4.64, 4.75, 4.87, 4.99, 5.11, 5.23, 5.36, 5.49, 5.62, 5.76, 5.90, 6.04, 6.19, 6.34, 6.49, 6.65,
    6.81, 6.98, 7.15, 7.32, 7.50, 7.68, 7.87, 8.06, 8.25, 8.45, 8.66, 8.87, 9.09, 9.31, 9.53, 9.76,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn e24_decade_has_24_values_and_ends_at_nine_point_one() {
        let values = decade(ESeries::E24);
        assert_eq!(values.len(), 24);
        assert_eq!(values.last().unwrap_or(&0.0), &9.1);
    }

    #[test]
    fn e96_decade_has_96_values() {
        let values = decade(ESeries::E96);
        assert_eq!(values.len(), 96);
    }

    #[test]
    fn values_between_lists_the_e12_values_from_100_to_1000() {
        let expected = vec![
            100.0, 120.0, 150.0, 180.0, 220.0, 270.0, 330.0, 390.0, 470.0, 560.0, 680.0, 820.0,
            1000.0,
        ];
        let got = values_between(ESeries::E12, 100.0, 1000.0).unwrap_or_default();
        assert_eq!(got, expected);
    }

    #[test]
    fn value_code_of_4700_ohms_is_4k7() {
        let code = value_code(4700.0, PassiveKind::Resistor).unwrap_or_default();
        assert_eq!(code, "4K7");
    }

    #[test]
    fn value_code_of_100_nanofarads_is_100n() {
        let code = value_code(100e-9, PassiveKind::Capacitor).unwrap_or_default();
        assert_eq!(code, "100N");
    }

    #[test]
    fn value_code_of_one_megohm_is_1m0() {
        let code = value_code(1e6, PassiveKind::Resistor).unwrap_or_default();
        assert_eq!(code, "1M0");
    }

    #[test]
    fn value_code_refuses_zero() {
        let result = value_code(0.0, PassiveKind::Resistor);
        assert!(result.is_err());
    }
}
