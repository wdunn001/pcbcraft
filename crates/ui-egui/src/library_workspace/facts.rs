//! What the Library tab knows about one part, as plain data with no drawing code.
//!
//! Context: issues #695 (Library tab) and #699 (part data and compare). The page of one part,
//! the vendor table and the compare table all read one `PartFacts`, so they can never
//! disagree about a value. Everything here is pure and unit-tested.
//!
//! Local rules: a value the sources did not give stays `None`; nothing is guessed and money
//! is never converted between currencies. Prices are cached answers and always carry the
//! date they were taken.
//!
//! # Dependencies
//! - crates/library/src/sources.rs (`PartResult`, `PriceBreak`)
//! - crates/library/src/listing.rs (`PartListing`, `Lifecycle`)
//! - crates/doc/src/sourcing.rs (`SupplierPart`, re-exported by the doc crate)
//!
//! # Do not
//! - Read the network or the disk here.
use pcbcraft_doc::SupplierPart;
use pcbcraft_library::listing::{Lifecycle, PartListing};
use pcbcraft_library::sources::{PartResult, PriceBreak};

/// Everything the Library tab shows about one part.
#[derive(Clone, Debug, Default)]
pub(crate) struct PartFacts {
    /// The part's name as the browser card shows it.
    pub(crate) name: String,
    /// Where the part comes from, in words (built in, a library, the project, an online source).
    pub(crate) origin: String,
    /// What the part is.
    pub(crate) description: String,
    /// The package or footprint name, when known.
    pub(crate) package: Option<String>,
    /// How many pins the symbol has, when known.
    pub(crate) pins: Option<usize>,
    /// Maker of the part.
    pub(crate) manufacturer: String,
    /// The maker's part number.
    pub(crate) mpn: String,
    /// Link to the datasheet.
    pub(crate) datasheet: Option<String>,
    /// The normalised listing, when a source gave one.
    pub(crate) listing: Option<PartListing>,
    /// Cached supplier answers for this part number.
    pub(crate) quotes: Vec<PartResult>,
    /// Supplier numbers typed on the project part.
    pub(crate) suppliers: Vec<SupplierPart>,
    /// Lifecycle typed on the project part.
    pub(crate) lifecycle: Option<Lifecycle>,
    /// RoHS typed on the project part.
    pub(crate) rohs: Option<bool>,
    /// REACH typed on the project part.
    pub(crate) reach: Option<bool>,
    /// Moisture sensitivity level typed on the project part.
    pub(crate) moisture: Option<u8>,
    /// The placed part in this project that carries these facts, when there is one.
    pub(crate) component: Option<u64>,
}

/// One line of the vendor table.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct VendorRow {
    /// Who sells it.
    pub(crate) supplier: String,
    /// The supplier's own number.
    pub(crate) sku: String,
    /// Units in stock when the cached answer said so.
    pub(crate) stock: Option<u64>,
    /// Unit price at each break, in the currency the supplier returned.
    pub(crate) breaks: Vec<PriceBreak>,
    /// Seconds since the Unix epoch of the cached answer; `None` when no answer is cached.
    pub(crate) as_of: Option<u64>,
}

impl PartFacts {
    /// The vendor table rows: every cached answer, then each typed supplier number that has
    /// no cached answer yet (shown with no price and no date).
    ///
    /// # Rules
    /// - Suppliers match ignoring case; a typed number never replaces a cached SKU.
    pub(crate) fn vendor_rows(&self) -> Vec<VendorRow> {
        let mut rows: Vec<VendorRow> = self
            .quotes
            .iter()
            .map(|q| VendorRow {
                supplier: if q.distributor.is_empty() {
                    q.source.clone()
                } else {
                    q.distributor.clone()
                },
                sku: q.sku.clone(),
                stock: if q.stock == 0 && q.price_breaks.is_empty() {
                    None
                } else {
                    Some(q.stock)
                },
                breaks: q.price_breaks.clone(),
                as_of: Some(q.retrieved_at),
            })
            .collect();
        for s in &self.suppliers {
            if !rows
                .iter()
                .any(|r| r.supplier.eq_ignore_ascii_case(&s.supplier))
            {
                rows.push(VendorRow {
                    supplier: s.supplier.clone(),
                    sku: s.part_number.clone(),
                    stock: None,
                    breaks: Vec::new(),
                    as_of: None,
                });
            }
        }
        rows
    }

    /// The best cached unit price for `quantity` pieces: for each supplier the break with the
    /// largest quantity not above `quantity`, then the cheapest of those. Returns the price and
    /// its currency, or `None` when no supplier has a break that small.
    ///
    /// # Rules
    /// - Currencies are never converted; the cheapest is picked among the first currency seen.
    pub(crate) fn unit_price_at(&self, quantity: u32) -> Option<(f64, String)> {
        let mut best: Option<(f64, String)> = None;
        for row in self.vendor_rows() {
            let Some(b) = row
                .breaks
                .iter()
                .filter(|b| b.quantity <= quantity)
                .max_by_key(|b| b.quantity)
            else {
                continue;
            };
            match &best {
                Some((_, cur)) if *cur != b.currency => {}
                Some((p, _)) if *p <= b.unit_price => {}
                _ => best = Some((b.unit_price, b.currency.clone())),
            }
        }
        best
    }

    /// Units in stock across the cached answers that gave a count, or `None` when none did.
    pub(crate) fn stock_total(&self) -> Option<u64> {
        let counts: Vec<u64> = self.vendor_rows().iter().filter_map(|r| r.stock).collect();
        if counts.is_empty() {
            None
        } else {
            Some(counts.iter().sum())
        }
    }
}

/// Format a price: up to four decimals, trailing zeros kept to three so `0.012` reads as written.
pub(crate) fn price_text(price: f64, currency: &str) -> String {
    format!("{price:.3} {currency}")
}

/// The calendar date of a Unix time in seconds, as `YYYY-MM-DD` (UTC).
///
/// # Rules
/// - Pure integer arithmetic; valid for any date from 1970 on.
pub(crate) fn date_text(epoch_secs: u64) -> String {
    let days = (epoch_secs / 86_400) as i64;
    // Civil-from-days (proleptic Gregorian), after H. Hinnant.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quote(distributor: &str, sku: &str, stock: u64, breaks: &[(u32, f64)]) -> PartResult {
        PartResult {
            source: "recorded".into(),
            mpn: "NR1206-330R-F".into(),
            manufacturer: "Northfield".into(),
            description: String::new(),
            kind: String::new(),
            distributor: distributor.into(),
            sku: sku.into(),
            stock,
            price_breaks: breaks
                .iter()
                .map(|(q, p)| PriceBreak {
                    quantity: *q,
                    unit_price: *p,
                    currency: "USD".into(),
                })
                .collect(),
            lifecycle: String::new(),
            datasheet_url: String::new(),
            product_url: String::new(),
            retrieved_at: 1_792_540_800,
        }
    }

    #[test]
    fn a_unix_time_becomes_its_calendar_date() {
        assert_eq!(date_text(0), "1970-01-01");
        assert_eq!(date_text(1_792_540_800), "2026-10-21");
        assert_eq!(date_text(951_782_400), "2000-02-29");
    }

    #[test]
    fn the_cheapest_cached_supplier_wins_at_the_asked_quantity() {
        let facts = PartFacts {
            quotes: vec![
                quote("Supplier A", "A-1", 100, &[(1, 0.1), (100, 0.012)]),
                quote("Supplier C", "C-1", 5, &[(1, 0.09), (100, 0.011)]),
            ],
            ..Default::default()
        };
        assert_eq!(facts.unit_price_at(100), Some((0.011, "USD".into())));
        assert_eq!(facts.unit_price_at(1), Some((0.09, "USD".into())));
        assert_eq!(facts.unit_price_at(0), None, "no break is that small");
        assert_eq!(facts.stock_total(), Some(105));
    }

    #[test]
    fn a_typed_supplier_number_with_no_cached_answer_has_no_price_and_no_date() {
        let facts = PartFacts {
            quotes: vec![quote("Supplier A", "A-1", 1, &[(1, 0.1)])],
            suppliers: vec![
                SupplierPart {
                    supplier: "supplier a".into(),
                    part_number: "ignored".into(),
                    url: None,
                },
                SupplierPart {
                    supplier: "Shop Z".into(),
                    part_number: "Z-9".into(),
                    url: None,
                },
            ],
            ..Default::default()
        };
        let rows = facts.vendor_rows();
        assert_eq!(rows.len(), 2);
        assert_eq!(
            rows[0].sku, "A-1",
            "a typed number never replaces a cached SKU"
        );
        assert_eq!(rows[1].supplier, "Shop Z");
        assert!(rows[1].breaks.is_empty() && rows[1].as_of.is_none());
    }
}
