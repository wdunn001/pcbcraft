use pcbcraft_library::listing::{Category, Lifecycle, ListingSource, PartListing, Unit};
use pcbcraft_library::records::{
    component_from_links, orderable_from_listing, provenance_from_listing, validate_store,
};

#[test]
fn project_library_store_validates_cross_references() {
    let component = component_from_links(
        Some(("Device".into(), "R".into())),
        None,
        None,
        Default::default(),
        "resistor",
        "10k",
        "",
        Default::default(),
        None,
    );
    let listing = PartListing {
        source: ListingSource::Project,
        source_id: "proj".into(),
        source_ref: "MPN-1".into(),
        manufacturer: "Acme".into(),
        mpn: "MPN-1".into(),
        description: String::new(),
        category: Category::Resistor,
        source_category: "resistor".into(),
        package_key: String::new(),
        package_text: String::new(),
        value: Some(10_000.0),
        value_unit: Some(Unit::Ohm),
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
        lifecycle_raw: "Active".into(),
        rohs: None,
        datasheet_url: None,
        symbol: None,
        footprint: None,
        model3d: None,
        attribution: None,
    };
    let orderable = orderable_from_listing(
        &listing,
        &component.id,
        provenance_from_listing(&listing, 1),
    );
    validate_store(&[component], &[orderable]).expect("valid store");
}
