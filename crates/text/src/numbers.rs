//! Locale-aware numbers and dates for text a person reads, plus entry parsing.
//!
//! Local rules: the table below is vendored from Unicode CLDR (read
//! 2026-10-29) for nine languages and uses Latin digits only. Files, logs and
//! machine output never go through the language functions: they use
//! [`fmt_machine`] and [`fmt_date_iso`]. An unknown tag uses English.
//! Dependencies: none beyond `std`; the catalog reaches this module through
//! `Catalog::number_tag` in `./lib.rs`.

/// Longest text [`parse_number`] looks at.
pub const MAX_NUMBER_CHARS: usize = 64;
/// Most decimal places any formatter prints.
pub const MAX_PLACES: usize = 12;
const NO_BREAK_SPACE: char = '\u{a0}';
const NARROW_NO_BREAK_SPACE: char = '\u{202f}';
const REFUSAL: &str = "Type a number such as 0,2";

/// How one language writes numbers and short dates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NumberStyle {
    /// Marker between whole and fractional digits.
    pub decimal: char,
    /// Marker between groups of thousands.
    pub group: char,
    /// Smallest group count a number needs before grouping starts: 1 groups
    /// from four digits, 2 groups from five.
    pub min_grouping_digits: u8,
    /// Short date pattern using `d`, `dd`, `M`, `MM`, `yy`, `y`.
    pub date_pattern: &'static str,
}

const fn style_row(
    decimal: char,
    group: char,
    min_grouping_digits: u8,
    date_pattern: &'static str,
) -> NumberStyle {
    NumberStyle {
        decimal,
        group,
        min_grouping_digits,
        date_pattern,
    }
}

/// The style for a language tag such as `de` or `de-DE`. Unknown tags give
/// English.
///
/// # Rules
/// Only the primary subtag counts, compared case-insensitively.
/// # Dependencies
/// None.
/// # Do not
/// Add a locale crate for these nine languages; extend the table instead.
pub fn style(tag: &str) -> NumberStyle {
    let primary: String = tag
        .chars()
        .take_while(|c| c.is_ascii_alphabetic())
        .collect::<String>()
        .to_ascii_lowercase();
    match primary.as_str() {
        "de" => style_row(',', '.', 1, "dd.MM.yy"),
        "fr" => style_row(',', NARROW_NO_BREAK_SPACE, 1, "dd/MM/y"),
        "es" => style_row(',', '.', 2, "d/M/yy"),
        "pt" => style_row(',', '.', 1, "dd/MM/y"),
        "ru" => style_row(',', NO_BREAK_SPACE, 1, "dd.MM.y"),
        "pl" => style_row(',', NO_BREAK_SPACE, 2, "d.MM.y"),
        "ja" => style_row('.', ',', 1, "y/MM/dd"),
        "zh" => style_row('.', ',', 1, "y/M/d"),
        _ => style_row('.', ',', 1, "M/d/yy"),
    }
}

/// Format `value` with `places` decimals and no grouping, in the machine
/// form: a point, never localized. Not a finite number prints as zero.
///
/// # Rules
/// `places` is capped at [`MAX_PLACES`]; negative zero prints without a sign.
/// # Dependencies
/// None.
/// # Do not
/// Use this for text a person reads; use [`fmt_ui`].
pub fn fmt_machine(value: f64, places: usize) -> String {
    let places = places.min(MAX_PLACES);
    let value = if value.is_finite() { value } else { 0.0 };
    let text = format!("{value:.places$}");
    match text.strip_prefix('-') {
        Some(rest) if rest.bytes().all(|b| b == b'0' || b == b'.') => rest.to_string(),
        _ => text,
    }
}

/// Format `value` for the language: its decimal marker, a leading zero for
/// values between -1 and 1, and no grouping (engineering values stay whole).
///
/// # Rules
/// Same bounds as [`fmt_machine`].
/// # Dependencies
/// [`style`].
/// # Do not
/// Group digits here; counts use [`fmt_count_ui`].
pub fn fmt_ui(tag: &str, value: f64, places: usize) -> String {
    let marker = style(tag).decimal;
    fmt_machine(value, places).replace('.', &marker.to_string())
}

/// Format a count with the language's grouping marker. Spanish and Polish
/// leave a four-digit number whole and group from five digits.
///
/// # Rules
/// Groups of three from the right.
/// # Dependencies
/// [`style`].
/// # Do not
/// Use for measured values; those stay ungrouped via [`fmt_ui`].
pub fn fmt_count_ui(tag: &str, n: u64) -> String {
    let st = style(tag);
    let digits = n.to_string();
    if digits.len() < 3 + usize::from(st.min_grouping_digits) {
        return digits;
    }
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(st.group);
        }
        out.push(ch);
    }
    out
}

/// A number and its unit joined by a no-break space so the unit never wraps
/// onto its own line.
///
/// # Rules
/// The separator is U+00A0.
/// # Dependencies
/// [`fmt_ui`].
/// # Do not
/// Put a plain space between number and unit.
pub fn fmt_with_unit(tag: &str, value: f64, places: usize, unit: &str) -> String {
    format!("{}{NO_BREAK_SPACE}{unit}", fmt_ui(tag, value, places))
}

/// Read a number a person typed. Either `,` or `.` is the decimal marker in
/// every language; a sign and a missing leading zero (`,2`) are fine.
///
/// # Rules
/// Longer than [`MAX_NUMBER_CHARS`], a grouping separator, a second marker,
/// letters and spaces are refused with "Type a number such as 0,2".
/// # Dependencies
/// None.
/// # Do not
/// Guess that `1,234` means a thousand; it reads as 1.234.
pub fn parse_number(text: &str) -> Result<f64, String> {
    let refuse = || REFUSAL.to_string();
    let text = text.trim();
    if text.chars().count() > MAX_NUMBER_CHARS {
        return Err(refuse());
    }
    let (negative, body) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.strip_prefix('+').unwrap_or(text)),
    };
    let mut markers = 0;
    let mut digits = 0;
    let mut normal = String::with_capacity(body.len() + 1);
    for ch in body.chars() {
        match ch {
            '0'..='9' => {
                digits += 1;
                normal.push(ch);
            }
            ',' | '.' => {
                markers += 1;
                normal.push('.');
            }
            _ => return Err(refuse()),
        }
    }
    if digits == 0 || markers > 1 {
        return Err(refuse());
    }
    let magnitude: f64 = normal.parse().map_err(|_| refuse())?;
    Ok(if negative { -magnitude } else { magnitude })
}

/// A short date in the language's pattern. Month 1 to 12, day 1 to 31, year
/// 0 to 9999; anything else gives an empty string.
///
/// # Rules
/// Pattern letters: `d`, `dd`, `M`, `MM`, `yy` (two digits), `y` (full year).
/// # Dependencies
/// [`style`].
/// # Do not
/// Use for files or logs; those use [`fmt_date_iso`].
pub fn fmt_date_ui(tag: &str, y: i32, m: u32, d: u32) -> String {
    if !valid_date(y, m, d) {
        return String::new();
    }
    let pattern = style(tag).date_pattern;
    let mut out = String::new();
    let mut chars = pattern.chars().peekable();
    while let Some(c) = chars.next() {
        let mut run = 1;
        while chars.peek() == Some(&c) {
            chars.next();
            run += 1;
        }
        match (c, run) {
            ('d', 1) => out.push_str(&d.to_string()),
            ('d', _) => out.push_str(&format!("{d:02}")),
            ('M', 1) => out.push_str(&m.to_string()),
            ('M', _) => out.push_str(&format!("{m:02}")),
            ('y', 2) => out.push_str(&format!("{:02}", y % 100)),
            ('y', _) => out.push_str(&y.to_string()),
            (other, n) => out.extend(std::iter::repeat_n(other, n)),
        }
    }
    out
}

/// The year-month-day form for files and logs. It never changes with the
/// language. Out of range values give an empty string.
///
/// # Rules
/// Same bounds as [`fmt_date_ui`]; four-digit year.
/// # Dependencies
/// None.
/// # Do not
/// Localize this.
pub fn fmt_date_iso(y: i32, m: u32, d: u32) -> String {
    if valid_date(y, m, d) {
        format!("{y:04}-{m:02}-{d:02}")
    } else {
        String::new()
    }
}

fn valid_date(y: i32, m: u32, d: u32) -> bool {
    (0..=9999).contains(&y) && (1..=12).contains(&m) && (1..=31).contains(&d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_and_parse_round_trip_for_en_de_fr_es() -> Result<(), String> {
        for tag in ["en", "de", "fr", "es"] {
            for v in [0.0, -3.0, 0.2, 1234.5, 1_000_000.0] {
                let shown = fmt_ui(tag, v, 1);
                assert_eq!(parse_number(&shown)?, v, "{tag} {shown}");
            }
        }
        assert_eq!(fmt_ui("de", 0.2, 2), "0,20");
        assert_eq!(fmt_ui("en", 1234.5, 1), "1234.5");
        assert_eq!(fmt_ui("fr", -0.04, 1), "0,0");
        Ok(())
    }

    #[test]
    fn german_decimal_comma_parses_to_a_point_value() -> Result<(), String> {
        assert_eq!(parse_number("0,2")?, 0.2);
        assert_eq!(parse_number(",2")?, 0.2);
        assert_eq!(parse_number("0.2")?, 0.2);
        assert_eq!(parse_number("-,5")?, -0.5);
        assert_eq!(parse_number("+3")?, 3.0);
        Ok(())
    }

    #[test]
    fn a_grouping_separator_or_a_second_marker_is_refused_with_the_plain_message() {
        let long = "1".repeat(65);
        for bad in [
            "1.234,5",
            "1,234.5",
            "1,2,3",
            "1 234",
            "1\u{a0}234",
            "1\u{202f}234",
            "",
            "-",
            ",",
            "abc",
            "1e5",
            &long,
        ] {
            assert_eq!(
                parse_number(bad),
                Err("Type a number such as 0,2".to_string()),
                "{bad}"
            );
        }
    }

    #[test]
    fn es_and_pl_do_not_group_four_digit_counts_but_group_five() {
        assert_eq!(fmt_count_ui("es", 1234), "1234");
        assert_eq!(fmt_count_ui("es", 12345), "12.345");
        assert_eq!(fmt_count_ui("pl", 1234), "1234");
        assert_eq!(fmt_count_ui("pl", 12345), "12\u{a0}345");
        assert_eq!(fmt_count_ui("en", 1234), "1,234");
        assert_eq!(fmt_count_ui("de", 1234567), "1.234.567");
        assert_eq!(fmt_count_ui("fr", 1234), "1\u{202f}234");
        assert_eq!(fmt_count_ui("en", 999), "999");
        assert_eq!(fmt_count_ui("en", 0), "0");
    }

    #[test]
    fn a_unit_never_wraps_off_its_number() {
        assert_eq!(fmt_with_unit("de", 0.2, 2, "mm"), "0,20\u{a0}mm");
        assert_eq!(fmt_with_unit("en", 10.0, 0, "mil"), "10\u{a0}mil");
        assert!(!fmt_with_unit("fr", 1.0, 1, "mm").contains(' '));
    }

    #[test]
    fn machine_format_ignores_the_language() {
        assert_eq!(fmt_machine(1234.5, 2), "1234.50");
        assert_eq!(fmt_machine(-0.0, 2), "0.00");
        assert_eq!(fmt_machine(f64::NAN, 1), "0.0");
        assert_eq!(fmt_machine(0.5, 99).len(), 2 + MAX_PLACES);
    }

    #[test]
    fn dates_follow_the_short_pattern_and_iso_never_changes() {
        let want = [
            ("en", "3/7/26"),
            ("de", "07.03.26"),
            ("fr", "07/03/2026"),
            ("es", "7/3/26"),
            ("pl", "7.03.2026"),
            ("ja", "2026/03/07"),
            ("zh", "2026/3/7"),
            ("xx", "3/7/26"),
        ];
        for (tag, text) in want {
            assert_eq!(fmt_date_ui(tag, 2026, 3, 7), text, "{tag}");
        }
        assert_eq!(fmt_date_iso(2026, 3, 7), "2026-03-07");
        assert_eq!(fmt_date_ui("de", 2026, 13, 1), "");
        assert_eq!(fmt_date_iso(2026, 0, 1), "");
    }

    #[test]
    fn region_tags_and_unknown_tags_pick_a_style() {
        assert_eq!(style("de-DE").decimal, ',');
        assert_eq!(style("DE_at").group, '.');
        assert_eq!(style("zz").decimal, '.');
    }
}
