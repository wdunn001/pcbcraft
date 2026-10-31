//! Side-by-side diff rows for two to five normalized part listings.
//!
//! Context: issue #710. The Library tab UI builds the same table from richer
//! facts; this module is the pure listing layer MCP and the engine call.
//!
//! Local rules: a row differs when its cells are not all equal, and a missing
//! value counts as different from a value. A row where every part lacks the
//! value does not differ. Prices use cached breaks at the chosen quantity and
//! are never converted between currencies.
//!
//! # Dependencies
//! - `crates/library/src/listing.rs` for `PartListing` and `Unit`
//!
//! # Do not
//! - Convert money between currencies or guess missing fields.
use crate::listing::{Lifecycle, PartListing, Unit};
use crate::sources::PriceBreak;
use serde::{Deserialize, Serialize};

/// At least this many parts are required.
pub const MIN_COMPARE_PARTS: usize = 2;
/// At most this many parts are allowed.
pub const MAX_COMPARE_PARTS: usize = 5;
/// Order quantities the compare table may use for the price row.
pub const COMPARE_QUANTITIES: [u32; 3] = [1, 100, 1000];

/// One part column in the compare table.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComparePartInput {
    /// Column title; when empty the listing MPN or source ref is used.
    #[serde(default)]
    pub label: String,
    /// Normalized listing for this column.
    pub listing: PartListing,
    /// Pin count when the caller knows it and the listing does not carry it.
    #[serde(default)]
    pub pins: Option<u32>,
    /// REACH flag when known outside the listing record.
    #[serde(default)]
    pub reach: Option<bool>,
}

/// One row of the compare table.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompareRow {
    /// What the row measures.
    pub label: String,
    /// One cell per part; `None` when no source gave a value.
    pub values: Vec<Option<String>>,
    /// True when the cells are not all equal.
    pub differs: bool,
}

/// Summary counts for the whole table.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompareTable {
    /// The order quantity used for the price row.
    pub quantity: u32,
    /// Column labels in part order.
    pub labels: Vec<String>,
    /// Rows in display order.
    pub rows: Vec<CompareRow>,
    /// How many rows have `differs` true.
    pub differing_rows: usize,
    /// Missing cells among rows that differ.
    pub missing_cells: usize,
}

/// Build the compare table for `parts` at `quantity` pieces.
///
/// # Rules
/// - `parts` must hold between [`MIN_COMPARE_PARTS`] and [`MAX_COMPARE_PARTS`] entries.
/// - `quantity` must be one of [`COMPARE_QUANTITIES`].
pub fn compare_table(parts: &[ComparePartInput], quantity: u32) -> Result<CompareTable, String> {
    if parts.len() < MIN_COMPARE_PARTS || parts.len() > MAX_COMPARE_PARTS {
        return Err(format!(
            "parts must hold between {MIN_COMPARE_PARTS} and {MAX_COMPARE_PARTS} entries"
        ));
    }
    if !COMPARE_QUANTITIES.contains(&quantity) {
        return Err("quantity must be 1, 100, or 1000".into());
    }
    let labels: Vec<String> = parts.iter().map(part_label).collect();
    let mut table: Vec<(String, Vec<Option<String>>)> = key_parameter_rows(parts);
    let each =
        |f: &dyn Fn(&ComparePartInput) -> Option<String>| -> Vec<Option<String>> {
            parts.iter().map(f).collect()
        };
    table.push(("Lifecycle".into(), each(&lifecycle_cell)));
    table.push((
        "RoHS".into(),
        each(&|p| flag_cell(p.listing.rohs)),
    ));
    table.push(("REACH".into(), each(&|p| flag_cell(p.reach))));
    table.push((
        format!("Price at {quantity}"),
        each(&|p| unit_price_cell(&p.listing, quantity)),
    ));
    table.push(("Stock".into(), each(&|p| p.listing.stock.map(|n| n.to_string()))));
    table.push((
        "Datasheet".into(),
        each(&|p| p.listing.datasheet_url.clone()),
    ));
    let rows: Vec<CompareRow> = table
        .into_iter()
        .map(|(label, values)| {
            let differs = values.iter().any(|v| *v != values[0]);
            CompareRow {
                label,
                values,
                differs,
            }
        })
        .collect();
    let differing_rows = rows.iter().filter(|r| r.differs).count();
    let missing_cells = rows
        .iter()
        .filter(|r| r.differs)
        .flat_map(|r| r.values.iter())
        .filter(|v| v.is_none())
        .count();
    Ok(CompareTable {
        quantity,
        labels,
        rows,
        differing_rows,
        missing_cells,
    })
}

fn part_label(part: &ComparePartInput) -> String {
    let trimmed = part.label.trim();
    if !trimmed.is_empty() {
        return trimmed.to_string();
    }
    if !part.listing.mpn.is_empty() {
        return part.listing.mpn.clone();
    }
    part.listing.source_ref.clone()
}

fn key_parameter_rows(parts: &[ComparePartInput]) -> Vec<(String, Vec<Option<String>>)> {
    let params: Vec<Vec<(String, Option<String>)>> = parts.iter().map(key_parameters).collect();
    let Some(first) = params.first() else {
        return vec![];
    };
    first
        .iter()
        .enumerate()
        .map(|(i, (label, _))| {
            (
                label.clone(),
                params.iter().map(|p| p[i].1.clone()).collect(),
            )
        })
        .collect()
}

fn key_parameters(part: &ComparePartInput) -> Vec<(String, Option<String>)> {
    let l = &part.listing;
    let range = match (l.temp_min_c, l.temp_max_c) {
        (Some(a), Some(b)) => Some(format!("{a} to {b} °C")),
        _ => None,
    };
    let package = if l.package_text.is_empty() {
        None
    } else {
        Some(l.package_text.clone())
    };
    vec![
        ("Package".into(), package),
        ("Pin count".into(), part.pins.map(|n| n.to_string())),
        (
            "Value".into(),
            l.value
                .zip(l.value_unit)
                .map(|(v, u)| format_si(v, u)),
        ),
        (
            "Tolerance".into(),
            l.tolerance_pct.map(|(lo, hi)| {
                if (lo - hi).abs() < f32::EPSILON {
                    format!("{lo} %")
                } else {
                    format!("-{lo} / +{hi} %")
                }
            }),
        ),
        (
            "Rated voltage".into(),
            l.rated_voltage_v.map(|v| format!("{v} V")),
        ),
        ("Power".into(), l.power_w.map(|w| format!("{w} W"))),
        ("Temperature range".into(), range),
    ]
}

fn lifecycle_cell(part: &ComparePartInput) -> Option<String> {
    let l = part.listing.lifecycle;
    if l == Lifecycle::Unknown {
        None
    } else {
        Some(lifecycle_label(l))
    }
}

fn lifecycle_label(l: Lifecycle) -> String {
    match l {
        Lifecycle::Active => "Active".into(),
        Lifecycle::Nrnd => "Not recommended for new designs".into(),
        Lifecycle::LastTimeBuy => "Last time buy".into(),
        Lifecycle::Obsolete => "Obsolete".into(),
        Lifecycle::Unknown => "Unknown".into(),
    }
}

fn flag_cell(v: Option<bool>) -> Option<String> {
    v.map(|b| if b { "Yes" } else { "No" }.to_string())
}

fn unit_price_cell(listing: &PartListing, quantity: u32) -> Option<String> {
    unit_price_at(&listing.price_breaks, quantity).map(|(price, currency)| price_text(price, &currency))
}

/// Best unit price at `quantity` from one supplier's breaks.
fn unit_price_at(breaks: &[PriceBreak], quantity: u32) -> Option<(f64, String)> {
    breaks
        .iter()
        .filter(|b| b.quantity <= quantity)
        .max_by_key(|b| b.quantity)
        .map(|b| (b.unit_price, b.currency.clone()))
}

fn price_text(price: f64, currency: &str) -> String {
    format!("{price:.3} {currency}")
}

fn format_si(value: f64, unit: Unit) -> String {
    let symbol = match unit {
        Unit::Ohm => "\u{3a9}",
        Unit::Farad => "F",
        Unit::Henry => "H",
        Unit::Hertz => "Hz",
        Unit::Volt => "V",
        Unit::Ampere => "A",
        Unit::Watt => "W",
    };
    let (scaled, prefix) = [
        (1e9, "G"),
        (1e6, "M"),
        (1e3, "k"),
        (1.0, ""),
        (1e-3, "m"),
        (1e-6, "\u{b5}"),
        (1e-9, "n"),
        (1e-12, "p"),
    ]
    .iter()
    .find(|(f, _)| value.abs() >= *f)
    .map_or((value, ""), |(f, p)| (value / f, *p));
    let digits = format!("{scaled:.3}");
    let digits = digits.trim_end_matches('0').trim_end_matches('.');
    format!("{digits} {prefix}{symbol}").trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::listing::{Category, ListingSource};
    use crate::sources::PriceBreak;

    fn blank_listing(name: &str) -> PartListing {
        PartListing {
            source: ListingSource::Fixture,
            source_id: "sample".into(),
            source_ref: name.into(),
            manufacturer: String::new(),
            mpn: name.into(),
            description: String::new(),
            category: Category::Other,
            source_category: String::new(),
            package_key: String::new(),
            package_text: String::new(),
            value: None,
            value_unit: None,
            tolerance_pct: None,
            rated_voltage_v: None,
            temp_min_c: None,
            temp_max_c: None,
            power_w: None,
            stock: None,
            stock_at: None,
            price_breaks: vec![],
            min_order_qty: None,
            order_multiple: None,
            packaging: None,
            lifecycle: Lifecycle::Unknown,
            lifecycle_raw: String::new(),
            rohs: None,
            datasheet_url: None,
            symbol: None,
            footprint: None,
            model3d: None,
            attribution: None,
        }
    }

    fn sample_part(name: &str, package: &str, price: f64, stock: u64) -> ComparePartInput {
        let mut listing = blank_listing(name);
        listing.category = Category::Resistor;
        listing.package_text = package.into();
        listing.stock = Some(stock);
        listing.price_breaks = vec![PriceBreak {
            quantity: 1,
            unit_price: price,
            currency: "USD".into(),
        }];
        ComparePartInput {
            label: name.into(),
            listing,
            pins: Some(2),
            reach: None,
        }
    }

    fn row<'a>(table: &'a CompareTable, label: &str) -> &'a CompareRow {
        table
            .rows
            .iter()
            .find(|r| r.label == label)
            .unwrap_or(&table.rows[0])
    }

    #[test]
    fn rows_differ_when_cells_are_not_all_equal() {
        let table = compare_table(
            &[
                sample_part("A", "1206", 0.1, 5),
                sample_part("B", "0603", 0.1, 9),
            ],
            1,
        )
        .expect("valid table");
        assert!(row(&table, "Package").differs);
        assert!(!row(&table, "Pin count").differs);
        assert!(!row(&table, "Price at 1").differs);
        assert!(row(&table, "Stock").differs);
    }

    #[test]
    fn missing_value_differs_from_a_value_and_all_missing_is_equal() {
        let mut b = sample_part("B", "1206", 0.1, 5);
        b.pins = None;
        let table = compare_table(&[sample_part("A", "1206", 0.1, 5), b], 1).expect("valid table");
        let pins = row(&table, "Pin count");
        assert!(pins.differs && pins.values[1].is_none());
        let empty = ComparePartInput {
            label: "X".into(),
            listing: blank_listing("X"),
            pins: None,
            reach: None,
        };
        let both = compare_table(&[empty.clone(), empty], 1).expect("valid table");
        assert!(
            both.rows.iter().all(|r| !r.differs),
            "two parts that both lack a value do not differ on it"
        );
    }

    #[test]
    fn the_price_row_follows_the_asked_quantity() {
        let part = sample_part("A", "1206", 0.1, 5);
        let table = compare_table(std::slice::from_ref(&part), 1000);
        assert!(table.is_err(), "one part is below the minimum");
        let table = compare_table(
            &[sample_part("A", "1206", 0.1, 5), sample_part("B", "1206", 0.2, 5)],
            1000,
        )
        .expect("valid table");
        assert_eq!(
            row(&table, "Price at 1000").values[0].as_deref(),
            Some("0.100 USD")
        );
    }

    #[test]
    fn part_count_and_quantity_are_bounded() {
        let one = sample_part("A", "1206", 0.1, 5);
        assert!(compare_table(std::slice::from_ref(&one), 1).is_err());
        let many: Vec<_> = (0..6)
            .map(|i| sample_part(&format!("P{i}"), "1206", 0.1, 5))
            .collect();
        assert!(compare_table(&many, 1).is_err());
        let pair = [sample_part("A", "1206", 0.1, 5), sample_part("B", "1206", 0.1, 5)];
        assert!(compare_table(&pair, 50).is_err());
    }
}
