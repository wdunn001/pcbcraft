//! One normalized, read-only record for a part, whatever source it came from,
//! and the pure functions that fill it from a source's own names and units.
//!
//! Local rules: empty means unknown and nothing is guessed. A bare four-digit
//! chip code is imperial; metric only when the text says so. Money stays in
//! the currency the source returned. The record keeps the source's raw text
//! beside every normalized form. No network, no keys, no UI here.
//! Dependencies: `./sources.rs` for `PartResult`, `PriceBreak`, `Attribution`.
//! `PartFilter` below selects listings; `library.list` in
//! `crates/engine/src/library.rs` serves it.
use crate::sources::{Attribution, PartResult, PriceBreak};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Which kind of source a listing came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ListingSource {
    /// Recorded sample data.
    Fixture,
    /// Built from an embedded series table; configure before place.
    Generated,
    /// A part in the open project.
    Project,
    /// A part from an installed library.
    Installed,
    /// A symbol or footprint from the open online library.
    OpenLibrary,
    /// A part from an online distributor.
    Distributor,
}

/// The closed list of 16 part categories.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    /// Resistors.
    Resistor,
    /// Capacitors.
    Capacitor,
    /// Inductors and ferrite beads.
    Inductor,
    /// Diodes other than LEDs.
    Diode,
    /// Light emitting diodes.
    Led,
    /// Bipolar transistors.
    Transistor,
    /// Field effect transistors.
    Mosfet,
    /// Voltage regulators and converters.
    IcRegulator,
    /// Microcontrollers.
    IcMcu,
    /// Logic ICs.
    IcLogic,
    /// Analog ICs.
    IcAnalog,
    /// Connectors and headers.
    Connector,
    /// Crystals and oscillators.
    Crystal,
    /// Ready-made modules.
    Module,
    /// Mechanical parts.
    Mechanical,
    /// Anything the table does not map.
    Other,
}

/// Lifecycle of a part. `LastTimeBuy` is kept apart because a designer must
/// see it; the source's own text stays in `PartListing::lifecycle_raw`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Lifecycle {
    /// In production.
    Active,
    /// Not recommended for new designs.
    Nrnd,
    /// Last time buy offered.
    LastTimeBuy,
    /// Discontinued or obsolete.
    Obsolete,
    /// The source did not say, or said something unmapped.
    #[default]
    Unknown,
}

/// How a part is supplied.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Packaging {
    /// On a reel.
    Reel,
    /// Cut tape.
    CutTape,
    /// In a tray.
    Tray,
    /// In a tube.
    Tube,
    /// Loose in bulk.
    Bulk,
    /// Anything the table does not map.
    Other,
}

/// The unit a value is stored in (SI base units).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Unit {
    /// Ohm.
    Ohm,
    /// Farad.
    Farad,
    /// Henry.
    Henry,
    /// Hertz.
    Hertz,
    /// Volt.
    Volt,
    /// Ampere.
    Ampere,
    /// Watt.
    Watt,
}

/// A package key plus the source's own text. `key` is empty when the package
/// is not recognised; chip sizes are always keyed in their imperial code so
/// two spellings of one package compare equal.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageKey {
    /// Canonical package key; empty when not recognised.
    pub key: String,
    /// The source's own package text.
    pub text: String,
}

/// One part as every source shows it. Empty (`None`, empty string) is unknown.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartListing {
    /// Kind of source this listing came from.
    pub source: ListingSource,
    /// Name of the library or distributor.
    pub source_id: String,
    /// The source's own key, such as a SKU.
    pub source_ref: String,
    /// Manufacturer name.
    pub manufacturer: String,
    /// Manufacturer part number.
    pub mpn: String,
    /// The source's description text.
    pub description: String,
    /// Category from the closed list.
    pub category: Category,
    /// The source's own category text, kept for display.
    pub source_category: String,
    /// Canonical package key; empty when unknown.
    pub package_key: String,
    /// The source's own package text.
    pub package_text: String,
    /// SI base units; see `value_unit`.
    pub value: Option<f64>,
    /// Unit of `value`.
    pub value_unit: Option<Unit>,
    /// Percent below and above nominal, so +80/-20 is `(20.0, 80.0)`.
    pub tolerance_pct: Option<(f32, f32)>,
    /// Rated voltage in volts.
    pub rated_voltage_v: Option<f64>,
    /// Lowest operating temperature in degrees Celsius.
    pub temp_min_c: Option<f32>,
    /// Highest operating temperature in degrees Celsius.
    pub temp_max_c: Option<f32>,
    /// Rated power in watts.
    pub power_w: Option<f64>,
    /// Units in stock; empty when the source gave no count.
    pub stock: Option<u64>,
    /// Seconds since the Unix epoch of the answer that gave `stock`.
    pub stock_at: Option<u64>,
    /// Sorted by quantity, each in the currency the source returned.
    pub price_breaks: Vec<PriceBreak>,
    /// Smallest order quantity.
    pub min_order_qty: Option<u32>,
    /// Orders come in multiples of this.
    pub order_multiple: Option<u32>,
    /// How the part is supplied.
    pub packaging: Option<Packaging>,
    /// Lifecycle from the fixed list.
    pub lifecycle: Lifecycle,
    /// The source's own lifecycle text.
    pub lifecycle_raw: String,
    /// Whether the part is RoHS compliant.
    pub rohs: Option<bool>,
    /// Whether the part has no REACH substance of very high concern.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reach: Option<bool>,
    /// True when the part is halogen free.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub halogen_free: Option<bool>,
    /// Moisture sensitivity level, 1 to 6.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub moisture_level: Option<u8>,
    /// Link to the datasheet.
    pub datasheet_url: Option<String>,
    /// Link to a schematic symbol.
    pub symbol: Option<String>,
    /// Link to a footprint.
    pub footprint: Option<String>,
    /// Link to a 3D model.
    pub model3d: Option<String>,
    /// Credit the source asks to see.
    pub attribution: Option<Attribution>,
}

/// Power of ten for an SI prefix letter. Case matters: `m` milli, `M` mega.
fn prefix_exp(c: char) -> Option<i32> {
    Some(match c {
        'p' => -12,
        'n' => -9,
        'u' => -6,
        'm' => -3,
        'k' | 'K' => 3,
        'M' => 6,
        'G' => 9,
        _ => return None,
    })
}

fn unit_word(word: &str) -> Option<Unit> {
    Some(match word {
        "ohm" | "ohms" | "r" => Unit::Ohm,
        "f" | "farad" | "farads" => Unit::Farad,
        "h" | "henry" | "henries" => Unit::Henry,
        "hz" | "hertz" => Unit::Hertz,
        "v" | "volt" | "volts" => Unit::Volt,
        "a" | "amp" | "amps" | "ampere" | "amperes" => Unit::Ampere,
        "w" | "watt" | "watts" => Unit::Watt,
        _ => return None,
    })
}

/// Build `mantissa * 10^exp` through the text parser so the result is the
/// correctly rounded double (4.7 with exp 3 is exactly 4700).
fn scaled(mantissa: &str, exp: i32) -> Option<f64> {
    let v: f64 = format!("{mantissa}e{exp}").parse().ok()?;
    v.is_finite().then_some(v)
}

/// Read a component value such as `10k`, `10 kOhm`, `4R7`, `2u2`, `100 µF`.
///
/// # Rules
/// Prefixes `p n u µ μ m k K M G`; units ohm, ohms, Ω, F, H, Hz, V, A, W.
/// A unit in the text wins over `unit_hint`; with neither, nothing is
/// returned. Letter-as-decimal (`4R7`, `4k7`) puts the letter at the point.
/// # Dependencies
/// None.
/// # Do not
/// Guess a unit or accept text that is not wholly a value.
pub fn parse_value(text: &str, unit_hint: Option<Unit>) -> Option<(f64, Unit)> {
    let text: String = text
        .trim()
        .replace(['\u{b5}', '\u{3bc}'], "u")
        .replace(['\u{3a9}', '\u{2126}'], "ohm");
    if text.is_empty() || text.len() > 40 {
        return None;
    }
    let chars: Vec<char> = text.chars().collect();
    let int_end = chars.iter().take_while(|c| c.is_ascii_digit()).count();
    if int_end == 0 {
        return None;
    }
    // Letter-as-decimal notation: digits, one letter, digits.
    if let Some(&mid) = chars.get(int_end)
        && matches!(
            mid,
            'R' | 'r' | 'k' | 'K' | 'M' | 'u' | 'n' | 'p' | 'm' | 'G'
        )
    {
        let frac_end = int_end
            + 1
            + chars[int_end + 1..]
                .iter()
                .take_while(|c| c.is_ascii_digit())
                .count();
        if frac_end > int_end + 1 {
            let whole: String = chars[..int_end].iter().collect();
            let frac: String = chars[int_end + 1..frac_end].iter().collect();
            let rest: String = chars[frac_end..].iter().collect::<String>();
            let rest = rest.trim().to_lowercase();
            let (exp, implied) = match mid {
                'R' | 'r' => (0, Some(Unit::Ohm)),
                other => (prefix_exp(other)?, None),
            };
            let unit = if rest.is_empty() {
                implied.or(unit_hint)?
            } else {
                unit_word(&rest)?
            };
            return Some((scaled(&format!("{whole}.{frac}"), exp)?, unit));
        }
    }
    // Plain number with an optional point.
    let mut end = int_end;
    if chars.get(end) == Some(&'.') {
        let frac = chars[end + 1..]
            .iter()
            .take_while(|c| c.is_ascii_digit())
            .count();
        if frac == 0 {
            return None;
        }
        end += 1 + frac;
    }
    let number: String = chars[..end].iter().collect();
    let rest: String = chars[end..].iter().collect::<String>().trim().to_string();
    if rest.is_empty() {
        return Some((scaled(&number, 0)?, unit_hint?));
    }
    if let Some(unit) = unit_word(&rest.to_lowercase()) {
        return Some((scaled(&number, 0)?, unit));
    }
    let mut it = rest.chars();
    let exp = prefix_exp(it.next()?)?;
    let tail: String = it.collect::<String>().trim().to_lowercase();
    let unit = if tail.is_empty() {
        unit_hint?
    } else {
        unit_word(&tail)?
    };
    Some((scaled(&number, exp)?, unit))
}

/// Numbers in `text` with the sign written just before each one.
fn signed_numbers(text: &str) -> Vec<(Option<char>, f32)> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut sign = None;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_ascii_digit() || (c == '.' && chars.get(i + 1).is_some_and(char::is_ascii_digit)) {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                i += 1;
            }
            let s: String = chars[start..i].iter().collect();
            if let Ok(v) = s.parse::<f32>() {
                out.push((sign.take(), v));
            }
            continue;
        }
        match c {
            '+' | '-' | '\u{b1}' => sign = Some(c),
            c if c.is_whitespace() => {}
            _ => sign = None,
        }
        i += 1;
    }
    out
}

/// Read a tolerance such as `±1%`, `1 %`, `+/-1 %`, `+80%, -20%`.
///
/// # Rules
/// A percent sign is required. The result is `(below, above)` in percent, so
/// `+80%, -20%` is `(20.0, 80.0)`. Values over 100 are refused.
/// # Dependencies
/// None.
/// # Do not
/// Read letter codes here; they need a category, see [`tolerance_from`].
pub fn parse_tolerance(text: &str) -> Option<(f32, f32)> {
    if !text.contains('%') || text.len() > 60 {
        return None;
    }
    let text = text.replace("+/-", "\u{b1}").replace("+-", "\u{b1}");
    let nums = signed_numbers(&text);
    let pair = if text.contains('\u{b1}') {
        let v = nums.first()?.1;
        (v, v)
    } else if let [(None, v)] = nums.as_slice() {
        (*v, *v)
    } else if let [(Some(a), x), (Some(b), y)] = nums.as_slice() {
        match (a, b) {
            ('+', '-') => (*y, *x),
            ('-', '+') => (*x, *y),
            _ => return None,
        }
    } else {
        return None;
    };
    (pair.0 <= 100.0 && pair.1 <= 100.0).then_some(pair)
}

/// A tolerance from text, also reading the letter codes F G J K M, but only
/// for resistors and capacitors.
///
/// # Rules
/// F is 1 percent, G 2, J 5, K 10, M 20.
/// # Dependencies
/// [`parse_tolerance`].
/// # Do not
/// Expand a letter for any other category.
pub fn tolerance_from(text: &str, category: Category) -> Option<(f32, f32)> {
    if let Some(found) = parse_tolerance(text) {
        return Some(found);
    }
    if !matches!(category, Category::Resistor | Category::Capacitor) {
        return None;
    }
    let pct = match text.trim() {
        "F" => 1.0,
        "G" => 2.0,
        "J" => 5.0,
        "K" => 10.0,
        "M" => 20.0,
        _ => return None,
    };
    Some((pct, pct))
}

/// Metric chip code to its imperial code.
const METRIC_TO_IMPERIAL: [(&str, &str); 11] = [
    ("0402", "01005"),
    ("0603", "0201"),
    ("1005", "0402"),
    ("1608", "0603"),
    ("2012", "0805"),
    ("3216", "1206"),
    ("3225", "1210"),
    ("4532", "1812"),
    ("5025", "2010"),
    ("6332", "2512"),
    ("0201", "008004"),
];
const IMPERIAL_CHIPS: [&str; 11] = [
    "008004", "01005", "0201", "0402", "0603", "0805", "1206", "1210", "1812", "2010", "2512",
];

/// Families whose first number names the variant, not the pin count.
const VARIANT_FAMILIES: [&str; 5] = ["sot", "to", "sod", "sc", "sma"];
const PIN_FAMILIES: [&str; 14] = [
    "soic", "tssop", "ssop", "msop", "qfn", "dfn", "qfp", "lqfp", "tqfp", "dip", "bga", "lga",
    "wlcsp", "sop",
];

/// Turn package text into a key. Chip sizes key as `chip-<imperial>`.
///
/// # Rules
/// A bare chip code is imperial. Metric is read only when the word `metric`
/// sits next to the code (`1608 Metric`, `0402 metric`), then converted, so
/// `0603 (1608 Metric)` and `1608 Metric` are one key and `0402 metric`
/// differs from bare `0402`. Other packages key by family, pin count, `ep`
/// and pitch (`soic-8`, `qfn-32-ep`, `to-220`). Unknown text gives an empty
/// key and keeps the text.
/// # Dependencies
/// None.
/// # Do not
/// Convert a chip code without the word `metric`.
pub fn package_key(text: &str) -> PackageKey {
    let lower = text.trim().to_lowercase();
    let tokens: Vec<&str> = lower
        .split(|c: char| c.is_whitespace() || matches!(c, '-' | '_' | ',' | '(' | ')' | '/' | ';'))
        .filter(|t| !t.is_empty())
        .collect();
    let key = chip_key(&tokens).or_else(|| family_key(&tokens));
    PackageKey {
        key: key.unwrap_or_default(),
        text: text.trim().to_string(),
    }
}

fn is_code(t: &str) -> bool {
    (4..=6).contains(&t.len()) && t.bytes().all(|b| b.is_ascii_digit())
}

fn chip_key(tokens: &[&str]) -> Option<String> {
    if let Some(at) = tokens.iter().position(|t| *t == "metric") {
        let code = at
            .checked_sub(1)
            .and_then(|i| tokens.get(i))
            .filter(|t| is_code(t))
            .or_else(|| tokens.get(at + 1).filter(|t| is_code(t)))?;
        return Some(match METRIC_TO_IMPERIAL.iter().find(|(m, _)| m == code) {
            Some((_, imperial)) => format!("chip-{imperial}"),
            None => format!("chip-metric-{code}"),
        });
    }
    tokens
        .iter()
        .find(|t| IMPERIAL_CHIPS.contains(t))
        .map(|t| format!("chip-{t}"))
}

fn family_key(tokens: &[&str]) -> Option<String> {
    let (at, family) = tokens.iter().enumerate().find_map(|(i, t)| {
        let name = t.trim_end_matches(|c: char| c.is_ascii_digit());
        let known = PIN_FAMILIES.contains(&name) || VARIANT_FAMILIES.contains(&name);
        known.then_some((i, name))
    })?;
    let glued = tokens[at].strip_prefix(family).unwrap_or("");
    let mut numbers: Vec<&str> = Vec::new();
    if !glued.is_empty() {
        numbers.push(glued);
    } else if let Some(before) = at.checked_sub(1).and_then(|i| tokens.get(i))
        && before.bytes().all(|b| b.is_ascii_digit())
        && !is_code(before)
    {
        numbers.push(before);
    }
    let mut key = family.to_string();
    let mut exposed = false;
    let mut pitch = None;
    for (i, t) in tokens.iter().enumerate().skip(at + 1) {
        if t.bytes().all(|b| b.is_ascii_digit()) && numbers.len() < 2 {
            numbers.push(t);
        } else if *t == "ep" || *t == "exposed" {
            exposed = true;
        } else if let Some(p) = t.strip_suffix("mm")
            && p.parse::<f32>().is_ok()
            && !matches!(tokens.get(i + 1), Some(&"width" | &"wide" | &"body"))
        {
            pitch = Some(p.to_string());
        }
    }
    for n in &numbers {
        key.push('-');
        key.push_str(n);
    }
    if exposed {
        key.push_str("-ep");
    }
    if let Some(p) = pitch {
        key.push_str("-p");
        key.push_str(&p);
    }
    Some(key)
}

/// Map a source's lifecycle text to the fixed list.
///
/// # Rules
/// Unrecognised or empty text is `Unknown`. The raw text is kept by the
/// caller in `lifecycle_raw`.
/// # Dependencies
/// None.
/// # Do not
/// Treat a missing status as `Active`.
pub fn lifecycle_from(text: &str) -> Lifecycle {
    let t = text.trim().to_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| t.contains(w));
    if has(&["not recommended", "nrnd", "not for new"]) {
        Lifecycle::Nrnd
    } else if has(&["last time buy", "last-time-buy", "last time", "ltb"]) {
        Lifecycle::LastTimeBuy
    } else if has(&[
        "obsolete",
        "discontinued",
        "end of life",
        "eol",
        "no longer manufactured",
        "inactive",
    ]) {
        Lifecycle::Obsolete
    } else if has(&[
        "active",
        "in production",
        "production",
        "preferred",
        "new product",
    ]) {
        Lifecycle::Active
    } else {
        Lifecycle::Unknown
    }
}

/// Keyword table, first match wins, so the specific kinds come first.
const CATEGORY_WORDS: [(Category, &[&str]); 15] = [
    (Category::Mosfet, &["mosfet", "fet"]),
    (Category::Led, &["led", "light emitting"]),
    (Category::Diode, &["diode", "rectifier", "zener"]),
    (Category::Resistor, &["resistor"]),
    (Category::Capacitor, &["capacitor"]),
    (Category::Inductor, &["inductor", "ferrite", "choke"]),
    (Category::Crystal, &["crystal", "oscillator", "resonator"]),
    (Category::IcRegulator, &["regulator", "ldo", "converter"]),
    (Category::IcMcu, &["microcontroller", "mcu"]),
    (
        Category::IcLogic,
        &["logic", "gate", "flip flop", "latch", "buffer"],
    ),
    (
        Category::IcAnalog,
        &["amplifier", "op amp", "comparator", "adc", "dac", "analog"],
    ),
    (Category::Transistor, &["transistor", "bjt"]),
    (
        Category::Connector,
        &["connector", "header", "socket", "terminal"],
    ),
    (Category::Module, &["module"]),
    (
        Category::Mechanical,
        &[
            "screw",
            "standoff",
            "spacer",
            "enclosure",
            "fastener",
            "hardware",
            "mechanical",
        ],
    ),
];

fn category_of(text: &str) -> Option<Category> {
    let lower: String = text
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect();
    let words: Vec<&str> = lower.split_whitespace().collect();
    let padded = format!(" {} ", words.join(" "));
    CATEGORY_WORDS.iter().find_map(|(cat, keys)| {
        keys.iter()
            .any(|k| {
                if k.contains(' ') {
                    padded.contains(&format!(" {k}"))
                } else {
                    words.iter().any(|w| w.starts_with(k))
                }
            })
            .then_some(*cat)
    })
}

/// Map the source's generic kind and its own category text to the closed
/// list. The kind text is tried first. Unmapped text gives `Other`.
///
/// # Rules
/// Keywords match at the start of a word, so `Resistors` and `LEDs` map.
/// # Dependencies
/// None.
/// # Do not
/// Invent a category for text the table does not know.
pub fn category_from(source_category: &str, kind_text: &str) -> Category {
    category_of(kind_text)
        .or_else(|| category_of(source_category))
        .unwrap_or(Category::Other)
}

/// A stock count as given. `None` stays `None`; a recorded 0 is a known 0.
pub fn stock_from(count: Option<u64>) -> Option<u64> {
    count
}

fn packaging_from(text: &str) -> Option<Packaging> {
    let t = text.trim().to_lowercase();
    if t.is_empty() {
        return None;
    }
    Some(if t.contains("cut tape") {
        Packaging::CutTape
    } else if t.contains("reel") || t.contains("tape") {
        Packaging::Reel
    } else if t.contains("tray") {
        Packaging::Tray
    } else if t.contains("tube") {
        Packaging::Tube
    } else if t.contains("bulk") {
        Packaging::Bulk
    } else {
        Packaging::Other
    })
}

/// The first non-empty attribute among `names`, matched ignoring case.
fn attr<'a>(attrs: &'a BTreeMap<String, String>, names: &[&str]) -> Option<&'a str> {
    names.iter().find_map(|name| {
        attrs
            .iter()
            .find(|(k, v)| k.eq_ignore_ascii_case(name) && !v.trim().is_empty())
            .map(|(_, v)| v.trim())
    })
}

/// Read an operating temperature span such as `-55°C ~ 125°C`.
///
/// # Rules
/// Two signed numbers in order; returns `(low, high)` in degrees Celsius.
/// # Dependencies
/// None.
/// # Do not
/// Guess when fewer or more than two numbers appear.
pub fn parse_temperature_range(text: &str) -> Option<(f32, f32)> {
    let chars: Vec<char> = text.chars().collect();
    let mut found = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let negative = chars[i] == '-'
            && chars.get(i + 1).is_some_and(char::is_ascii_digit)
            && (i == 0 || !chars[i - 1].is_alphanumeric());
        if negative || chars[i].is_ascii_digit() {
            let start = if negative { i + 1 } else { i };
            let mut end = start;
            while end < chars.len() && (chars[end].is_ascii_digit() || chars[end] == '.') {
                end += 1;
            }
            let s: String = chars[start..end].iter().collect();
            let v = s.parse::<f32>().ok()?;
            found.push(if negative { -v } else { v });
            i = end;
        } else {
            i += 1;
        }
    }
    match found.as_slice() {
        [lo, hi] if lo <= hi => Some((*lo, *hi)),
        _ => None,
    }
}

fn leading_count(text: &str) -> Option<u64> {
    let digits: String = text
        .trim()
        .chars()
        .filter(|c| *c != ',')
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().ok()
}

pub fn rohs_from(text: &str) -> Option<bool> {
    let t = text.trim().to_lowercase();
    if t.contains("non") || t.contains("not") || t == "no" || t == "false" {
        Some(false)
    } else if t.contains("compliant") || t == "yes" || t == "true" {
        Some(true)
    } else {
        None
    }
}

/// Read a REACH answer from distributor text.
pub fn reach_from(text: &str) -> Option<bool> {
    let t = text.trim().to_lowercase();
    if t.contains("svhc") && (t.contains("no") || t.contains("none") || t.contains("free")) {
        Some(true)
    } else if t.contains("affected") || t.contains("contains") {
        Some(false)
    } else if t.contains("unaffected") || t.contains("compliant") {
        Some(true)
    } else {
        None
    }
}

/// Read a halogen-free claim from compliance text.
pub fn halogen_from(text: &str) -> Option<bool> {
    let t = text.trim().to_lowercase();
    if t.contains("halogen") && (t.contains("free") || t.contains("no halogen")) {
        Some(true)
    } else if t.contains("contains halogen") {
        Some(false)
    } else {
        None
    }
}

/// Read a moisture sensitivity level between 1 and 6.
pub fn moisture_level_from(text: &str) -> Option<u8> {
    let t = text.trim().to_lowercase();
    for level in (1..=6).rev() {
        let needle = level.to_string();
        if t.contains(&format!("level {needle}"))
            || t.contains(&format!("msl {needle}"))
            || t == needle
            || t.ends_with(&format!(" {needle}"))
        {
            return Some(level);
        }
    }
    t.chars()
        .filter(|c| c.is_ascii_digit())
        .collect::<String>()
        .parse::<u8>()
        .ok()
        .filter(|m| (1..=6).contains(m))
}

fn some_text(text: &str) -> Option<String> {
    let t = text.trim();
    (!t.is_empty()).then(|| t.to_string())
}

/// Fill a listing from a source's answer and its attribute map.
///
/// # Rules
/// Attributes are the source's own names, for example `Resistance`,
/// `Tolerance`, `Package / Case`, `Voltage - Rated`, `Operating Temperature`,
/// `Power`, `Packaging`, `RoHS`, `Stock`. A missing or unreadable one leaves
/// its field empty. `PartResult::stock` is a plain count, so 0 reads as a
/// known 0; unknown stock is a `Stock` attribute with no number. Money is
/// never converted.
/// # Dependencies
/// [`crate::sources::PartResult`]; the attribute map comes from
/// `PartSource::attributes` in `./sources.rs`.
/// # Do not
/// Guess a field the source did not give.
pub fn listing_from_result(
    result: &PartResult,
    attributes: &BTreeMap<String, String>,
) -> PartListing {
    let source_category = attr(attributes, &["Category"]).unwrap_or("").to_string();
    let category = category_from(&source_category, &result.kind);
    let hint = match category {
        Category::Resistor => Some(Unit::Ohm),
        Category::Capacitor => Some(Unit::Farad),
        Category::Inductor => Some(Unit::Henry),
        Category::Crystal => Some(Unit::Hertz),
        _ => None,
    };
    let (value, value_unit) = attr(
        attributes,
        &[
            "Resistance",
            "Capacitance",
            "Inductance",
            "Frequency",
            "Value",
        ],
    )
    .and_then(|t| parse_value(t, hint))
    .map_or((None, None), |(v, u)| (Some(v), Some(u)));
    let package_text = attr(attributes, &["Package / Case", "Package", "Case"]).unwrap_or("");
    let package = package_key(package_text);
    let (stock, stock_at) = {
        let count = match attr(attributes, &["Stock"]) {
            Some(text) => leading_count(text),
            None => Some(result.stock),
        };
        let stock = stock_from(count);
        (stock, stock.map(|_| result.retrieved_at))
    };
    let mut price_breaks = result.price_breaks.clone();
    price_breaks.sort_by_key(|b| b.quantity);
    let lifecycle_raw = if result.lifecycle.trim().is_empty() {
        attr(attributes, &["Lifecycle", "Product Status"])
            .unwrap_or("")
            .to_string()
    } else {
        result.lifecycle.trim().to_string()
    };
    let temp = attr(attributes, &["Operating Temperature"]).and_then(parse_temperature_range);
    PartListing {
        source: ListingSource::Fixture,
        source_id: result.source.clone(),
        source_ref: if result.sku.is_empty() {
            result.mpn.clone()
        } else {
            result.sku.clone()
        },
        manufacturer: result.manufacturer.clone(),
        mpn: result.mpn.clone(),
        description: result.description.clone(),
        category,
        source_category,
        package_key: package.key,
        package_text: package.text,
        value,
        value_unit,
        tolerance_pct: attr(attributes, &["Tolerance"]).and_then(|t| tolerance_from(t, category)),
        rated_voltage_v: attr(
            attributes,
            &["Voltage - Rated", "Voltage Rating", "Rated Voltage"],
        )
        .and_then(|t| parse_value(t, Some(Unit::Volt)))
        .filter(|(_, u)| *u == Unit::Volt)
        .map(|(v, _)| v),
        temp_min_c: temp.map(|t| t.0),
        temp_max_c: temp.map(|t| t.1),
        power_w: attr(attributes, &["Power (Watts)", "Power"])
            .and_then(|t| parse_value(t, Some(Unit::Watt)))
            .filter(|(_, u)| *u == Unit::Watt)
            .map(|(v, _)| v),
        stock,
        stock_at,
        price_breaks,
        min_order_qty: attr(attributes, &["Minimum Order Quantity"])
            .and_then(leading_count)
            .and_then(|n| u32::try_from(n).ok()),
        order_multiple: attr(attributes, &["Order Multiple"])
            .and_then(leading_count)
            .and_then(|n| u32::try_from(n).ok()),
        packaging: attr(attributes, &["Packaging"]).and_then(packaging_from),
        lifecycle: lifecycle_from(&lifecycle_raw),
        lifecycle_raw,
        rohs: attr(attributes, &["RoHS", "RoHS Status"]).and_then(rohs_from),
        reach: attr(attributes, &["REACH", "REACH-SVHC", "REACH SVHC"]).and_then(reach_from),
        halogen_free: attr(
            attributes,
            &["Halogen free", "Halogen Free", "ProductCompliance"],
        )
        .and_then(halogen_from),
        moisture_level: attr(
            attributes,
            &[
                "Moisture Sensitivity Level (MSL)",
                "Moisture Sensitivity Level",
                "MSL",
            ],
        )
        .and_then(moisture_level_from),
        datasheet_url: some_text(&result.datasheet_url),
        symbol: None,
        footprint: None,
        model3d: None,
        attribution: None,
    }
}

/// Most categories one filter may name.
pub const MAX_FILTER_CATEGORIES: usize = 16;
/// Most entries in any other list field of a filter.
pub const MAX_FILTER_LIST: usize = 64;
/// Longest text a filter may search for, in bytes.
pub const MAX_FILTER_TEXT: usize = 256;

/// One query over listings from any source. Every field that is set must
/// match; a field left out does not restrict.
///
/// # Rules
/// A numeric field that is empty on the listing never matches a set numeric
/// filter. The value range applies to categories that have a value unit
/// (resistor, capacitor, inductor, crystal); a listing in another category
/// never matches it. `temp_range` is `(low, high)` and the part must cover
/// all of it. Stock given only as text is an empty count.
/// # Dependencies
/// [`PartListing`] above; `package_key` for package spellings.
/// # Do not
/// Rank, sort or merge results here, and never treat a missing field as a match.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PartFilter {
    /// Words to find in the part number, maker, description or source key.
    pub text: Option<String>,
    /// Allowed categories; at most [`MAX_FILTER_CATEGORIES`].
    pub categories: Vec<Category>,
    /// Allowed packages, in any spelling `package_key` reads.
    pub packages: Vec<String>,
    /// Lowest value in SI base units.
    pub value_min: Option<f64>,
    /// Highest value in SI base units.
    pub value_max: Option<f64>,
    /// Lowest acceptable rated voltage in volts.
    pub voltage_min: Option<f64>,
    /// Temperature span in degrees Celsius the part must cover, low then high.
    pub temp_range: Option<(f32, f32)>,
    /// Lowest acceptable stock count.
    pub stock_min: Option<u64>,
    /// Allowed lifecycles.
    pub lifecycle: Vec<Lifecycle>,
    /// Allowed kinds of source.
    pub sources: Vec<ListingSource>,
    /// Required RoHS answer.
    pub rohs: Option<bool>,
}

fn has_value_unit(category: Category) -> bool {
    matches!(
        category,
        Category::Resistor | Category::Capacitor | Category::Inductor | Category::Crystal
    )
}

impl PartFilter {
    /// Refuse a filter that is out of bounds or contradicts itself.
    ///
    /// # Rules
    /// At most [`MAX_FILTER_CATEGORIES`] categories, [`MAX_FILTER_LIST`]
    /// entries elsewhere, text up to [`MAX_FILTER_TEXT`] bytes, finite numbers,
    /// ranges in order. The error is a plain sentence.
    /// # Dependencies
    /// None.
    /// # Do not
    /// Repair a bad filter silently.
    pub fn validate(&self) -> Result<(), String> {
        if self.categories.len() > MAX_FILTER_CATEGORIES {
            return Err(format!(
                "a filter may name at most {MAX_FILTER_CATEGORIES} categories"
            ));
        }
        if self.packages.len() > MAX_FILTER_LIST
            || self.lifecycle.len() > MAX_FILTER_LIST
            || self.sources.len() > MAX_FILTER_LIST
        {
            return Err(format!(
                "a filter list may hold at most {MAX_FILTER_LIST} entries"
            ));
        }
        if self
            .text
            .as_ref()
            .is_some_and(|t| t.len() > MAX_FILTER_TEXT)
            || self.packages.iter().any(|p| p.len() > MAX_FILTER_TEXT)
        {
            return Err(format!(
                "filter text may be at most {MAX_FILTER_TEXT} bytes"
            ));
        }
        let numbers = [self.value_min, self.value_max, self.voltage_min];
        if numbers.iter().flatten().any(|n| !n.is_finite()) {
            return Err("filter numbers must be finite".into());
        }
        if let (Some(lo), Some(hi)) = (self.value_min, self.value_max)
            && lo > hi
        {
            return Err("value_min must not exceed value_max".into());
        }
        if let Some((lo, hi)) = self.temp_range
            && (!lo.is_finite() || !hi.is_finite() || lo > hi)
        {
            return Err("temp_range must be a low then a high temperature".into());
        }
        Ok(())
    }

    /// Whether one listing passes every field that is set.
    ///
    /// # Rules
    /// See the type: empty numeric fields on the listing never match.
    /// # Dependencies
    /// `package_key` in this file.
    /// # Do not
    /// Call this on a filter that failed `validate`.
    pub fn matches(&self, listing: &PartListing) -> bool {
        if let Some(text) = self
            .text
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
        {
            let hay = format!(
                "{} {} {} {}",
                listing.mpn, listing.manufacturer, listing.description, listing.source_ref
            )
            .to_lowercase();
            if !text
                .to_lowercase()
                .split_whitespace()
                .all(|word| hay.contains(word))
            {
                return false;
            }
        }
        if !self.categories.is_empty() && !self.categories.contains(&listing.category) {
            return false;
        }
        if !self.packages.is_empty() && !self.packages.iter().any(|p| package_matches(p, listing)) {
            return false;
        }
        if (self.value_min.is_some() || self.value_max.is_some())
            && !(has_value_unit(listing.category)
                && listing.value.is_some_and(|v| {
                    self.value_min.is_none_or(|lo| v >= lo)
                        && self.value_max.is_none_or(|hi| v <= hi)
                }))
        {
            return false;
        }
        if let Some(min) = self.voltage_min
            && !listing.rated_voltage_v.is_some_and(|v| v >= min)
        {
            return false;
        }
        if let Some((lo, hi)) = self.temp_range
            && !(listing.temp_min_c.is_some_and(|t| t <= lo)
                && listing.temp_max_c.is_some_and(|t| t >= hi))
        {
            return false;
        }
        if let Some(min) = self.stock_min
            && !listing.stock.is_some_and(|s| s >= min)
        {
            return false;
        }
        if !self.lifecycle.is_empty() && !self.lifecycle.contains(&listing.lifecycle) {
            return false;
        }
        if !self.sources.is_empty() && !self.sources.contains(&listing.source) {
            return false;
        }
        if let Some(want) = self.rohs
            && listing.rohs != Some(want)
        {
            return false;
        }
        true
    }
}

fn package_matches(wanted: &str, listing: &PartListing) -> bool {
    let wanted = package_key(wanted);
    if !wanted.key.is_empty() {
        return listing.package_key == wanted.key;
    }
    let text = wanted.text.trim().to_lowercase();
    !text.is_empty() && listing.package_text.trim().to_lowercase() == text
}

#[cfg(test)]
mod tests {
    use super::*;

    type Case = (&'static str, Option<Unit>, Option<(f64, Unit)>);

    #[test]
    fn values_parse_by_table() {
        let cases: [Case; 8] = [
            ("10k", Some(Unit::Ohm), Some((10_000.0, Unit::Ohm))),
            ("4R7", None, Some((4.7, Unit::Ohm))),
            ("2u2", Some(Unit::Farad), Some((2.2e-6, Unit::Farad))),
            ("3.3 V", None, Some((3.3, Unit::Volt))),
            ("8 MHz", None, Some((8.0e6, Unit::Hertz))),
            ("10k", None, None),
            ("1.", Some(Unit::Ohm), None),
            ("", Some(Unit::Ohm), None),
        ];
        for (text, hint, want) in cases {
            assert_eq!(parse_value(text, hint), want, "{text}");
        }
    }

    #[test]
    fn tolerance_letters_need_a_resistor_or_capacitor() {
        assert_eq!(tolerance_from("J", Category::Resistor), Some((5.0, 5.0)));
        assert_eq!(tolerance_from("J", Category::Diode), None);
        assert_eq!(tolerance_from("1.5%", Category::Diode), Some((1.5, 1.5)));
        assert_eq!(parse_tolerance("+10%"), None);
        assert_eq!(parse_tolerance("150%"), None);
    }

    #[test]
    fn packages_key_by_family_pins_and_pitch() {
        let keys = [
            ("SOIC-8", "soic-8"),
            ("QFN-32-EP", "qfn-32-ep"),
            ("TSSOP-16", "tssop-16"),
            ("SOT-23-5", "sot-23-5"),
            ("TO-220", "to-220"),
            ("8-SOIC (3.9mm Width)", "soic-8"),
            ("Mystery", ""),
        ];
        for (text, key) in keys {
            assert_eq!(package_key(text).key, key, "{text}");
        }
        assert_eq!(package_key(" Mystery ").text, "Mystery");
    }

    #[test]
    fn categories_map_by_keyword() {
        assert_eq!(
            category_from("Chip Resistor - Surface Mount", ""),
            Category::Resistor
        );
        assert_eq!(
            category_from("Transistors - FETs, MOSFETs - Single", ""),
            Category::Mosfet
        );
        assert_eq!(category_from("LEDs", ""), Category::Led);
        assert_eq!(category_from("Flux Capacitor", ""), Category::Capacitor);
        assert_eq!(category_from("Widgets", ""), Category::Other);
        assert_eq!(category_from("Widgets", "capacitor"), Category::Capacitor);
    }
}
