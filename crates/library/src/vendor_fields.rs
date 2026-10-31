//! Table-driven mapping from distributor API field names to [`PartListing`].
//!
//! Recorded sample bodies live in `crates/library/tests/fixtures/sources/`.
//! No network calls here.

use crate::listing::{
    halogen_from, lifecycle_from, listing_from_result, moisture_level_from, reach_from, rohs_from,
    Lifecycle, PartListing,
};
use crate::sources::PartResult;
use serde_json::Value;
use std::collections::BTreeMap;

fn push_attr(map: &mut BTreeMap<String, String>, key: &str, value: &str) {
    let value = value.trim();
    if !value.is_empty() {
        map.insert(key.to_owned(), value.to_owned());
    }
}

fn text_field(record: &Value, names: &[&str]) -> String {
    names
        .iter()
        .find_map(|name| record.get(*name).and_then(Value::as_str))
        .unwrap_or("")
        .to_owned()
}

fn some_url(text: &str) -> Option<String> {
    let t = text.trim();
    (!t.is_empty()).then(|| t.to_owned())
}

/// Map a recorded Mouser part body plus a search [`PartResult`] to a listing.
///
/// Field names: `DataSheetUrl`, `LifecycleStatus`, `IsDiscontinued`, `ROHSStatus`,
/// `REACH-SVHC`, `ProductCompliance`, `Reeling`.
pub fn listing_from_mouser(result: &PartResult, record: &Value) -> PartListing {
    let mut attrs = BTreeMap::new();
    push_attr(&mut attrs, "RoHS", &text_field(record, &["ROHSStatus"]));
    push_attr(
        &mut attrs,
        "REACH",
        &text_field(record, &["REACH-SVHC"]),
    );
    push_attr(
        &mut attrs,
        "ProductCompliance",
        &text_field(record, &["ProductCompliance"]),
    );
    let reeling = text_field(record, &["Reeling"]);
    if !reeling.is_empty() {
        push_attr(&mut attrs, "Packaging", &reeling);
    }
    let mut listing = listing_from_result(result, &attrs);
    if let Some(url) = record.get("DataSheetUrl").and_then(Value::as_str) {
        listing.datasheet_url = some_url(url);
    }
    let lc = text_field(record, &["LifecycleStatus"]);
    if !lc.is_empty() {
        listing.lifecycle_raw = lc.clone();
        listing.lifecycle = lifecycle_from(&lc);
    }
    if record.get("IsDiscontinued").and_then(Value::as_bool) == Some(true) {
        listing.lifecycle = Lifecycle::Obsolete;
        if listing.lifecycle_raw.is_empty() {
            listing.lifecycle_raw = "Discontinued".into();
        }
    }
    if listing.rohs.is_none() {
        listing.rohs = record
            .get("ROHSStatus")
            .and_then(Value::as_str)
            .and_then(rohs_from);
    }
    if listing.reach.is_none() {
        listing.reach = record
            .get("REACH-SVHC")
            .and_then(Value::as_str)
            .and_then(reach_from);
    }
    if listing.halogen_free.is_none() {
        listing.halogen_free = record
            .get("ProductCompliance")
            .and_then(Value::as_str)
            .and_then(halogen_from);
    }
    listing
}

fn digikey_classifications(record: &Value) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let Some(obj) = record.get("Classifications").and_then(Value::as_object) else {
        return out;
    };
    for (k, v) in obj {
        if let Some(s) = v.as_str() {
            out.insert(k.clone(), s.to_owned());
        }
    }
    out
}

/// Map a recorded Digi-Key part body plus a search [`PartResult`] to a listing.
///
/// Field names: `DatasheetUrl`, `ProductStatus`, `Classifications`, `DateLastBuyChance`.
pub fn listing_from_digikey(result: &PartResult, record: &Value) -> PartListing {
    let classes = digikey_classifications(record);
    let mut attrs = BTreeMap::new();
    push_attr(
        &mut attrs,
        "RoHS",
        classes.get("RoHS").map(String::as_str).unwrap_or(""),
    );
    push_attr(
        &mut attrs,
        "REACH",
        classes.get("REACH").map(String::as_str).unwrap_or(""),
    );
    if let Some(msl) = classes.get("MoistureSensitivityLevel") {
        push_attr(&mut attrs, "Moisture Sensitivity Level (MSL)", msl);
    }
    let mut listing = listing_from_result(result, &attrs);
    if let Some(url) = record.get("DatasheetUrl").and_then(Value::as_str) {
        listing.datasheet_url = some_url(url);
    }
    let status = text_field(record, &["ProductStatus"]);
    if !status.is_empty() {
        listing.lifecycle_raw = status.clone();
        listing.lifecycle = lifecycle_from(&status);
    }
    let ltb = text_field(record, &["DateLastBuyChance"]);
    if !ltb.is_empty() {
        listing.lifecycle = Lifecycle::LastTimeBuy;
        listing.lifecycle_raw = format!("Last time buy {ltb}");
    }
    if listing.rohs.is_none() {
        listing.rohs = classes.get("RoHS").map(String::as_str).and_then(rohs_from);
    }
    if listing.reach.is_none() {
        listing.reach = classes
            .get("REACH")
            .map(String::as_str)
            .and_then(reach_from);
    }
    if listing.moisture_level.is_none() {
        listing.moisture_level = classes
            .get("MoistureSensitivityLevel")
            .map(String::as_str)
            .and_then(moisture_level_from);
    }
    listing
}
