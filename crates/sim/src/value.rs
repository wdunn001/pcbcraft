//! Component value parser. Reads the free text people type into a value field.
//!
//! # Rules
//! - Scale letters: `T G meg k m u n p` (any case except `M`), plus the micro sign.
//! - A bare `M` is refused because SPICE reads it as milli; the message asks for `meg` or `m`.
//! - `R`, `k`, `m`, `u`, `n`, `p`, `G`, `T` between digits act as the decimal point (`4k7`, `1R0`).
//! - A trailing unit (`F H V A Ω ohm`) is ignored; `F` is a farad, never a femtofarad.
//!
//! # Dependencies
//! - docs/adr-0001-simulation.md section 3
//!
//! # Do not
//! - Do not accept a value it cannot read; every refusal echoes the input.

/// Longest value text accepted, in bytes.
const MAX_LEN: usize = 64;

/// Reads a component value such as `4k7`, `100nF` or `330 Ω` into a plain number.
///
/// Returns the value in base units (ohms, farads, volts, amps). The result is always finite.
///
/// # Rules
/// - Refuses empty text, text over 64 bytes, a bare `M`, unknown letters and out-of-range numbers.
///
/// # Dependencies
/// - docs/adr-0001-simulation.md section 3
///
/// # Do not
/// - Do not write the input text into a netlist; write the number this returns.
pub fn parse_value(input: &str) -> Result<f64, String> {
    let fail = |why: &str| format!("cannot read the value \"{input}\": {why}");
    let text = input.trim().replace(['µ', 'μ'], "u");
    if text.is_empty() {
        return Err(fail("it is empty"));
    }
    if text.len() > MAX_LEN {
        return Err(fail("it is too long"));
    }
    let lead_len = text
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(text.len());
    let lead = &text[..lead_len];
    let mut rest = &text[lead_len..];
    if !lead.chars().any(|c| c.is_ascii_digit()) {
        return Err(fail("it does not start with a number"));
    }
    if lead.matches('.').count() > 1 {
        return Err(fail("it has more than one decimal point"));
    }
    let mut number = lead.to_string();
    let mut has_exponent = false;
    if let Some(after) = exponent_len(rest) {
        number.push_str(&rest[..after]);
        rest = &rest[after..];
        has_exponent = true;
    }
    rest = rest.trim_start();
    let (scale, used) = scale_of(rest).map_err(|why| fail(&why))?;
    rest = &rest[used..];
    if used > 0 && !has_exponent && !lead.contains('.') {
        let digits = rest
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(rest.len());
        if digits > 0 {
            number.push('.');
            number.push_str(&rest[..digits]);
            rest = &rest[digits..];
        }
    }
    let unit = rest.trim().to_lowercase();
    if !matches!(
        unit.as_str(),
        "" | "f" | "h" | "v" | "a" | "ω" | "ohm" | "ohms"
    ) {
        return Err(fail(&format!(
            "\"{}\" is not a unit or scale letter",
            rest.trim()
        )));
    }
    let base: f64 = number
        .parse()
        .map_err(|_| fail("the number is malformed"))?;
    let value = base * scale;
    if !value.is_finite() {
        return Err(fail("the number is out of range"));
    }
    Ok(value)
}

/// Length of an exponent such as `e-3` at the start of `rest`, if there is one.
fn exponent_len(rest: &str) -> Option<usize> {
    let bytes = rest.as_bytes();
    if !matches!(bytes.first(), Some(b'e' | b'E')) {
        return None;
    }
    let mut at = 1;
    if matches!(bytes.get(at), Some(b'+' | b'-')) {
        at += 1;
    }
    let start = at;
    while bytes.get(at).is_some_and(u8::is_ascii_digit) {
        at += 1;
    }
    (at > start).then_some(at)
}

/// The scale factor at the start of `rest` and how many bytes it used.
fn scale_of(rest: &str) -> Result<(f64, usize), String> {
    if rest.len() >= 3 && rest.is_char_boundary(3) && rest[..3].eq_ignore_ascii_case("meg") {
        return Ok((1e6, 3));
    }
    let Some(first) = rest.chars().next() else {
        return Ok((1.0, 0));
    };
    match first {
        'M' => Err("a bare M is ambiguous; write meg for mega or m for milli".to_string()),
        'm' => Ok((1e-3, 1)),
        'k' | 'K' => Ok((1e3, 1)),
        'u' | 'U' => Ok((1e-6, 1)),
        'n' | 'N' => Ok((1e-9, 1)),
        'p' | 'P' => Ok((1e-12, 1)),
        'g' | 'G' => Ok((1e9, 1)),
        't' | 'T' => Ok((1e12, 1)),
        'r' | 'R' => Ok((1.0, 1)),
        _ => Ok((1.0, 0)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() <= 1e-12 * b.abs().max(1e-30)
    }

    #[test]
    fn value_table_accepts() -> Result<(), String> {
        let table = [
            ("4k7", 4700.0),
            ("1R0", 1.0),
            ("330", 330.0),
            ("330 Ω", 330.0),
            ("100nF", 1e-7),
            ("2.2u", 2.2e-6),
            ("1meg", 1e6),
            ("1Meg", 1e6),
            ("1m", 1e-3),
            ("4.7µF", 4.7e-6),
            ("10k", 1e4),
            ("2k2Ω", 2200.0),
            ("1e3", 1e3),
            ("5V", 5.0),
            ("  47  ", 47.0),
        ];
        for (text, want) in table {
            let got = parse_value(text)?;
            if !close(got, want) {
                return Err(format!("{text} read as {got}, wanted {want}"));
            }
        }
        Ok(())
    }

    #[test]
    fn value_table_refuses() -> Result<(), String> {
        for text in [
            "1M", "10MΩ", "", "   ", "abc", "1e999", "NaN", "nan", "inf", "-5", "1.2.3", "10x",
            "1mil",
        ] {
            match parse_value(text) {
                Ok(v) => return Err(format!("{text:?} should be refused, got {v}")),
                Err(message) => {
                    if !message.contains(&format!("\"{text}\"")) {
                        return Err(format!("refusal for {text:?} does not echo it: {message}"));
                    }
                }
            }
        }
        let bare = parse_value("1M").err().unwrap_or_default();
        if !bare.contains("meg") || !bare.contains("m ") {
            return Err(format!("bare M message does not offer meg or m: {bare}"));
        }
        Ok(())
    }

    #[test]
    fn one_farad_is_not_read_as_a_femtofarad() -> Result<(), String> {
        let farad = parse_value("1F")?;
        if close(farad, 1.0) {
            Ok(())
        } else {
            Err(format!("1F read as {farad}"))
        }
    }

    #[test]
    fn one_meg_resistor_is_one_million_ohms() -> Result<(), String> {
        let ohms = parse_value("1meg")?;
        if close(ohms, 1e6) {
            Ok(())
        } else {
            Err(format!("1meg read as {ohms}"))
        }
    }

    #[test]
    fn four_k_seven_is_4700() -> Result<(), String> {
        let ohms = parse_value("4k7")?;
        if close(ohms, 4700.0) {
            Ok(())
        } else {
            Err(format!("4k7 read as {ohms}"))
        }
    }
}
