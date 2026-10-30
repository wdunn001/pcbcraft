use pcbcraft_library::listing::{Lifecycle, Packaging};
use pcbcraft_library::sources::PartResult;
use pcbcraft_library::vendor_fields::{listing_from_digikey, listing_from_mouser};
use serde_json::Value;
use std::fs;

fn sample_result(mpn: &str) -> PartResult {
    PartResult {
        source: "Sample".into(),
        mpn: mpn.into(),
        manufacturer: "Example Corp".into(),
        description: "Sample part".into(),
        kind: "resistor".into(),
        distributor: String::new(),
        sku: String::new(),
        stock: 100,
        price_breaks: vec![],
        lifecycle: String::new(),
        datasheet_url: String::new(),
        product_url: String::new(),
        retrieved_at: 1_790_000_000,
    }
}

fn load_fixture(name: &str) -> Value {
    let path = format!("{}/fixtures/sources/{name}", env!("CARGO_MANIFEST_DIR"));
    let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("read {name}: {e}"));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse {name}: {e}"))
}

#[test]
fn mouser_sample_maps_to_the_typed_fields() {
    let record = load_fixture("mouser-sample.json");
    let listing = listing_from_mouser(&sample_result("MOUS-1"), &record);
    assert_eq!(listing.lifecycle, Lifecycle::Active);
    assert_eq!(listing.rohs, Some(true));
    assert_eq!(listing.reach, Some(true));
    assert_eq!(listing.halogen_free, Some(true));
    assert_eq!(listing.packaging, Some(Packaging::CutTape));
    assert_eq!(
        listing.datasheet_url.as_deref(),
        Some("https://example.invalid/mouser-datasheet.pdf")
    );
}

#[test]
fn digikey_sample_maps_to_the_typed_fields() {
    let record = load_fixture("digikey-sample.json");
    let listing = listing_from_digikey(&sample_result("DK-1"), &record);
    assert_eq!(listing.rohs, Some(true));
    assert_eq!(listing.reach, Some(true));
    assert_eq!(listing.moisture_level, Some(3));
    assert_eq!(
        listing.datasheet_url.as_deref(),
        Some("https://example.invalid/digikey-datasheet.pdf")
    );
}

#[test]
fn a_last_time_buy_date_sets_the_lifecycle() {
    let record = load_fixture("digikey-sample.json");
    let listing = listing_from_digikey(&sample_result("DK-LTB"), &record);
    assert_eq!(listing.lifecycle, Lifecycle::LastTimeBuy);
    assert!(
        listing.lifecycle_raw.contains("2027-06-15"),
        "raw text keeps the date"
    );
}
