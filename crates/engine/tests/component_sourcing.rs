use pcbcraft_engine::{Error, Session};
use pcbcraft_library::listing::{Lifecycle, ListingSource, PartListing};
use serde_json::json;

fn add_resistor(s: &mut Session) -> Result<u64, Error> {
    let id = s
        .execute(
            "component.add",
            json!({"kind":"resistor","position":{"x":10,"y":10}}),
            None,
        )?["result"]["id"]
        .as_u64()
        .ok_or_else(|| Error::Invalid("missing id".into()))?;
    Ok(id)
}

fn sample_listing() -> PartListing {
    let mut l = PartListing {
        source: ListingSource::Fixture,
        source_id: "Sample".into(),
        source_ref: "X".into(),
        manufacturer: "Acme".into(),
        mpn: "AC-1".into(),
        description: String::new(),
        category: pcbcraft_library::listing::Category::Resistor,
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
        lifecycle: Lifecycle::Active,
        lifecycle_raw: String::new(),
        rohs: Some(true),
        reach: Some(true),
        halogen_free: Some(true),
        moisture_level: Some(3),
        datasheet_url: Some("https://example.invalid/ds.pdf".into()),
        symbol: None,
        footprint: None,
        model3d: Some("https://example.invalid/model.step".into()),
        attribution: None,
    };
    l
}

#[test]
fn apply_listing_does_not_overwrite_a_user_value() -> Result<(), Error> {
    let mut s = Session::default();
    let id = add_resistor(&mut s)?;
    s.execute(
        "component.set_sourcing",
        json!({"id": id, "mpn": "USER-MPN"}),
        None,
    )?;
    let listing = sample_listing();
    s.execute(
        "component.apply_listing",
        json!({"id": id, "listing": listing, "overwrite": false}),
        None,
    )?;
    let component = s
        .project
        .components
        .iter()
        .find(|c| c.id == id)
        .ok_or_else(|| Error::Invalid("component".into()))?;
    assert_eq!(component.sourcing.mpn, "USER-MPN");
    assert_eq!(component.sourcing.manufacturer, "Acme");
    assert_eq!(component.sourcing.moisture_level, Some(3));
    Ok(())
}

#[test]
fn set_sourcing_refuses_bad_moisture_level() -> Result<(), Error> {
    let mut s = Session::default();
    let id = add_resistor(&mut s)?;
    assert!(
        s.execute(
            "component.set_sourcing",
            json!({"id": id, "moisture_level": 9}),
            None,
        )
        .is_err()
    );
    Ok(())
}
