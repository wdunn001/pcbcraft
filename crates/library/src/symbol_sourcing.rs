//! Sourcing fields a library symbol already states: maker, part number,
//! datasheet link and description.
//!
//! # Rules
//! - Only what the symbol file says is copied; a maker is never inferred.
//! - The part number is the symbol's own property when it has one; otherwise the symbol
//!   value, and only when the symbol also links a datasheet (a named chip, not a generic
//!   resistor or connector).
//! - Values are at most 256 bytes without control characters, so the engine accepts them.
//!
//! # Dependencies
//! - crates/library/src/part_fields.rs (the field names)
//! - crates/library/src/sexpr.rs (reads the symbol's properties)
use crate::SymbolDefinition;
use crate::part_fields::{DESCRIPTION, MANUFACTURER, MPN};
use crate::sexpr;

const MAX_VALUE: usize = 256;

fn clean(text: &str) -> String {
    let mut out: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .trim()
        .to_owned();
    while out.len() > MAX_VALUE {
        out.pop();
    }
    out
}

fn is_link(text: &str) -> bool {
    let t = text.trim();
    (t.starts_with("https://") || t.starts_with("http://")) && t.len() <= MAX_VALUE
}

/// The key and value pairs to put on a placed part, in the order Manufacturer,
/// MPN, Datasheet, Description; fields the symbol does not state are left out.
///
/// # Rules
/// - Works from the symbol's source text; a symbol built in memory has no properties
///   and yields only its description.
/// - A datasheet is kept only when it is an http or https link.
///
/// # Dependencies
/// - crates/library/src/part_fields.rs
///
/// # Do not
/// - Guess a manufacturer from the description or the datasheet address.
pub fn sourcing_fields(symbol: &SymbolDefinition) -> Vec<(String, String)> {
    let mut properties: Vec<(String, String)> = Vec::new();
    if let Ok(root) = sexpr::parse(&symbol.raw_source) {
        for child in &root.children {
            if child.name() == "property" {
                properties.push((child.value(1).to_owned(), child.value(2).to_owned()));
            }
        }
    }
    let property = |names: &[&str]| -> String {
        names
            .iter()
            .find_map(|n| {
                properties
                    .iter()
                    .find(|(k, v)| k.eq_ignore_ascii_case(n) && !v.trim().is_empty())
                    .map(|(_, v)| clean(v))
            })
            .unwrap_or_default()
    };
    let datasheet = property(&["Datasheet"]);
    let datasheet = if is_link(&datasheet) {
        datasheet
    } else {
        String::new()
    };
    let mut mpn = property(&[
        "MPN",
        "Manufacturer_Part_Number",
        "Manufacturer Part Number",
    ]);
    let value = clean(&symbol.value);
    if mpn.is_empty() && !datasheet.is_empty() && !value.is_empty() && value != "~" {
        mpn = value;
    }
    let mut out = Vec::new();
    for (key, value) in [
        (MANUFACTURER, property(&["Manufacturer", "MFR"])),
        (MPN, mpn),
        ("Datasheet", datasheet),
        (DESCRIPTION, clean(&symbol.description)),
    ] {
        if !value.is_empty() {
            out.push((key.to_owned(), value));
        }
    }
    out
}
