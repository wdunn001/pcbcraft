//! Embedded Samtec connector series catalog for the first library slice.
//!
//! # Rules
//! - Records are hand-entered from public manufacturer pages; no vendor CAD files ship in the repo.
//! - Listings represent a configurable series, not a fully built part number.
//!
//! # Dependencies
//! - crates/library/data/connectors/catalog.toml
//! - crates/library/src/listing.rs
use crate::listing::{Category, Lifecycle, ListingSource, PartListing};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

const CATALOG_TOML: &str = include_str!("../data/connectors/catalog.toml");

/// One connector family offered as a per-series configurator entry.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectorSeriesRecord {
    pub id: String,
    pub prefix: String,
    pub description: String,
    pub family: String,
    pub pitch_mm: f64,
    pub drawing_url: String,
}

/// Samtec connector catalog embedded in the library crate.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectorCatalog {
    pub manufacturer: String,
    pub source_id: String,
    pub verify_note: String,
    pub permission_status: String,
    pub series: Vec<ConnectorSeriesRecord>,
}

impl ConnectorCatalog {
    fn parse(text: &str) -> Result<Self, String> {
        toml::from_str(text).map_err(|e| format!("connector catalog parse error: {e}"))
    }

    /// Load the embedded first-slice catalog.
    pub fn embedded() -> Result<&'static Self, String> {
        static CACHE: OnceLock<Result<ConnectorCatalog, String>> = OnceLock::new();
        match CACHE.get_or_init(|| Self::parse(CATALOG_TOML)) {
            Ok(catalog) => Ok(catalog),
            Err(message) => Err(message.clone()),
        }
    }

    /// Library listings for each configurable series.
    pub fn listings() -> Result<Vec<PartListing>, String> {
        let catalog = Self::embedded()?;
        Ok(catalog
            .series
            .iter()
            .map(|series| listing_for_series(catalog, series))
            .collect())
    }

    /// Summary for `library.catalog` and MCP clients.
    pub fn summary_json() -> Result<serde_json::Value, String> {
        let catalog = Self::embedded()?;
        serde_json::to_value(catalog).map_err(|e| e.to_string())
    }
}

fn listing_for_series(catalog: &ConnectorCatalog, series: &ConnectorSeriesRecord) -> PartListing {
    let description = join_description(&series.description, &catalog.verify_note);
    PartListing {
        source: ListingSource::Generated,
        source_id: catalog.source_id.clone(),
        source_ref: series.id.clone(),
        manufacturer: catalog.manufacturer.clone(),
        mpn: series.prefix.clone(),
        description,
        category: Category::Connector,
        source_category: "connector".into(),
        package_key: String::new(),
        package_text: format!("{} pitch", format_pitch(series.pitch_mm)),
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
        lifecycle: Lifecycle::Active,
        lifecycle_raw: String::new(),
        rohs: None,
        datasheet_url: Some(series.drawing_url.clone()),
        symbol: None,
        footprint: None,
        model3d: None,
        attribution: None,
    }
}

fn join_description(body: &str, verify_note: &str) -> String {
    let body = body.trim();
    let note = verify_note.trim();
    match (body.is_empty(), note.is_empty()) {
        (true, true) => String::new(),
        (true, false) => note.to_string(),
        (false, true) => body.to_string(),
        (false, false) => format!("{body} {note}"),
    }
}

fn format_pitch(pitch_mm: f64) -> String {
    if pitch_mm > 0.0 && (pitch_mm - 2.54).abs() < f64::EPSILON {
        "0.100 in".into()
    } else if pitch_mm > 0.0 {
        format!("{pitch_mm} mm")
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::listing::PartFilter;

    #[test]
    fn embedded_catalog_lists_three_first_slice_series() -> Result<(), String> {
        let catalog = ConnectorCatalog::embedded()?;
        assert_eq!(catalog.manufacturer, "Samtec");
        assert_eq!(catalog.series.len(), 3);
        let prefixes: Vec<_> = catalog.series.iter().map(|s| s.prefix.as_str()).collect();
        assert!(prefixes.contains(&"TSW"));
        assert!(prefixes.contains(&"SSW"));
        assert!(prefixes.contains(&"BTH"));
        Ok(())
    }

    #[test]
    fn series_listings_match_samtec_text_search() -> Result<(), String> {
        let listings = ConnectorCatalog::listings()?;
        let filter = PartFilter {
            text: Some("samtec".into()),
            ..PartFilter::default()
        };
        let matched: Vec<_> = listings
            .iter()
            .filter(|listing| filter.matches(listing))
            .collect();
        assert_eq!(matched.len(), 3);
        Ok(())
    }

    #[test]
    fn a_series_prefix_finds_one_configurable_entry() -> Result<(), String> {
        let listings = ConnectorCatalog::listings()?;
        let filter = PartFilter {
            text: Some("TSW".into()),
            ..PartFilter::default()
        };
        let matched: Vec<_> = listings
            .iter()
            .filter(|listing| filter.matches(listing))
            .collect();
        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0].mpn, "TSW");
        assert_eq!(matched[0].source, ListingSource::Generated);
        Ok(())
    }
}
