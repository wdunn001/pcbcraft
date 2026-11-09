use pcbcraft_library::listing::{
    Category, Lifecycle, ListingSource, PartFilter, PartListing, Unit, category_from,
    lifecycle_from, listing_from_result, package_key, parse_tolerance, parse_value,
};
use pcbcraft_library::listing_adapters::{listing_from_symbol, listings_from_symbols};
use pcbcraft_library::sources::{FixtureSource, PartResult, PartSource, PriceBreak, SourceError};
use pcbcraft_library::{SymbolDefinition, read_symbols};

const SAMPLE: &str = include_str!("fixtures/sources/sample.json");
const ATTRIBUTES: &str = include_str!("fixtures/sources/sample-attributes.json");

type Case = (&'static str, Option<Unit>, Option<(f64, Unit)>);

fn source() -> Result<FixtureSource, SourceError> {
    FixtureSource::from_json(SAMPLE)?.with_attributes(ATTRIBUTES)
}

fn result(mpn: &str, kind: &str) -> PartResult {
    PartResult {
        source: "Sample parts".into(),
        mpn: mpn.into(),
        manufacturer: "Example Components".into(),
        description: String::new(),
        kind: kind.into(),
        distributor: String::new(),
        sku: String::new(),
        stock: 0,
        price_breaks: vec![],
        lifecycle: String::new(),
        datasheet_url: String::new(),
        product_url: String::new(),
        retrieved_at: 1_790_000_000,
    }
}

fn listing_for(src: &FixtureSource, part: &PartResult) -> PartListing {
    listing_from_result(part, &src.attributes(&part.mpn))
}

#[test]
fn the_same_part_in_two_spellings_normalizes_to_equal_filter_fields() -> Result<(), SourceError> {
    let src = source()?;
    let a = listing_for(&src, &src.fetch("SAMPLE-R-10K")?);
    let b = listing_for(&src, &result("ALT-R-10K", "resistor"));
    assert_eq!(a.value, Some(10_000.0));
    assert_eq!(a.value, b.value);
    assert_eq!(a.value_unit, Some(Unit::Ohm));
    assert_eq!(a.value_unit, b.value_unit);
    assert_eq!(a.package_key, "chip-0603");
    assert_eq!(a.package_key, b.package_key);
    assert_ne!(a.package_text, b.package_text, "the raw spellings differ");
    assert_eq!(a.tolerance_pct, Some((1.0, 1.0)));
    assert_eq!(a.tolerance_pct, b.tolerance_pct);
    assert_eq!(a.rated_voltage_v, b.rated_voltage_v);
    assert_eq!(a.category, Category::Resistor);
    assert_eq!(a.category, b.category);
    assert_eq!((a.temp_min_c, a.temp_max_c), (Some(-55.0), Some(155.0)));
    assert_eq!(a.power_w, Some(0.1));
    assert_eq!(a.rohs, Some(true));
    Ok(())
}

#[test]
fn value_parsing_handles_prefixes_micro_sign_and_letter_notation() {
    let ohm = Some(Unit::Ohm);
    let farad = Some(Unit::Farad);
    let table: [Case; 9] = [
        ("10k", ohm, Some((10_000.0, Unit::Ohm))),
        ("10k\u{3a9}", None, Some((10_000.0, Unit::Ohm))),
        ("10 kohms", None, Some((10_000.0, Unit::Ohm))),
        ("4R7", None, Some((4.7, Unit::Ohm))),
        ("4k7", ohm, Some((4_700.0, Unit::Ohm))),
        ("2u2", farad, Some((2.2e-6, Unit::Farad))),
        ("100nF", None, Some((1.0e-7, Unit::Farad))),
        ("100 \u{b5}F", None, Some((1.0e-4, Unit::Farad))),
        ("abc", ohm, None),
    ];
    for (text, hint, want) in table {
        assert_eq!(parse_value(text, hint), want, "{text}");
    }
}

#[test]
fn tolerance_parsing_handles_plus_minus_and_asymmetric_forms() {
    let table = [
        ("±1%", Some((1.0, 1.0))),
        ("+/-1 %", Some((1.0, 1.0))),
        ("1%", Some((1.0, 1.0))),
        ("±0.5 %", Some((0.5, 0.5))),
        ("+80%, -20%", Some((20.0, 80.0))),
        ("-20%, +80%", Some((20.0, 80.0))),
        ("tight", None),
        ("1", None),
    ];
    for (text, want) in table {
        assert_eq!(parse_tolerance(text), want, "{text}");
    }
}

#[test]
fn imperial_and_metric_chip_codes_are_never_mixed_up() {
    let bare = package_key("0402");
    let metric = package_key("0402 metric");
    assert_eq!(bare.key, "chip-0402");
    assert_eq!(metric.key, "chip-01005");
    assert_ne!(bare.key, metric.key);
    assert_eq!(package_key("0603").key, package_key("1608 Metric").key);
    assert_eq!(package_key("0603 (1608 Metric)").key, "chip-0603");
    assert_eq!(
        package_key("1608").key,
        "",
        "an unmarked metric-looking code is not guessed"
    );
}

#[test]
fn missing_fields_stay_empty_and_are_never_guessed() -> Result<(), SourceError> {
    let src = source()?;
    let bare = listing_for(&src, &result("BARE-PART", ""));
    assert_eq!(bare.category, Category::Other);
    assert_eq!(bare.source_category, "Mystery parts");
    assert_eq!(bare.value, None);
    assert_eq!(bare.value_unit, None);
    assert_eq!(bare.tolerance_pct, None);
    assert_eq!(bare.rated_voltage_v, None);
    assert_eq!((bare.temp_min_c, bare.temp_max_c), (None, None));
    assert_eq!(bare.power_w, None);
    assert_eq!(bare.package_key, "");
    assert_eq!(bare.datasheet_url, None);
    assert_eq!(bare.rohs, None);
    assert_eq!(bare.packaging, None);
    assert_eq!(bare.lifecycle, Lifecycle::Unknown);
    let cap = listing_for(&src, &result("SAMPLE-C-100N", "capacitor"));
    assert_eq!(
        cap.stock, None,
        "a Stock entry with no count leaves stock empty"
    );
    assert_eq!(cap.stock_at, None);
    let known_zero = listing_for(&src, &result("BARE-PART", ""));
    assert_eq!(known_zero.stock, Some(0), "a recorded 0 is a known 0");
    Ok(())
}

#[test]
fn lifecycle_text_maps_to_the_fixed_list_and_keeps_the_raw_text() -> Result<(), SourceError> {
    let table = [
        ("Active", Lifecycle::Active),
        ("Not recommended for new designs", Lifecycle::Nrnd),
        ("Last Time Buy", Lifecycle::LastTimeBuy),
        ("Obsolete", Lifecycle::Obsolete),
        ("Discontinued at Example Shop", Lifecycle::Obsolete),
        ("Inactive", Lifecycle::Obsolete),
        ("", Lifecycle::Unknown),
        ("Preliminary", Lifecycle::Unknown),
    ];
    for (text, want) in table {
        assert_eq!(lifecycle_from(text), want, "{text}");
    }
    let src = source()?;
    let listing = listing_for(&src, &src.fetch("SAMPLE-C-100N")?);
    assert_eq!(listing.lifecycle, Lifecycle::Nrnd);
    assert_eq!(listing.lifecycle_raw, "Not recommended for new designs");
    assert_eq!(listing.category, Category::Capacitor);
    assert_eq!(listing.package_key, "chip-01005");
    assert_eq!(category_from("", "resistor"), Category::Resistor);
    Ok(())
}

#[test]
fn price_breaks_are_sorted_and_keep_their_currency() {
    let mut part = result("ANY", "resistor");
    part.price_breaks = vec![
        PriceBreak {
            quantity: 100,
            unit_price: 0.02,
            currency: "EUR".into(),
        },
        PriceBreak {
            quantity: 1,
            unit_price: 0.10,
            currency: "USD".into(),
        },
        PriceBreak {
            quantity: 10,
            unit_price: 0.05,
            currency: "GBP".into(),
        },
    ];
    let listing = listing_from_result(&part, &Default::default());
    let seen: Vec<(u32, &str)> = listing
        .price_breaks
        .iter()
        .map(|b| (b.quantity, b.currency.as_str()))
        .collect();
    assert_eq!(seen, vec![(1, "USD"), (10, "GBP"), (100, "EUR")]);
    assert_eq!(listing.price_breaks[0].unit_price, 0.10, "no conversion");
}

#[test]
fn a_listing_round_trips_through_serde_json() -> Result<(), String> {
    let src = source().map_err(|e| e.to_string())?;
    let mut listing = listing_for(&src, &result("ALT-R-10K", "resistor"));
    listing.source = ListingSource::Fixture;
    listing.price_breaks.push(PriceBreak {
        quantity: 5,
        unit_price: 0.3,
        currency: "USD".into(),
    });
    let text = serde_json::to_string(&listing).map_err(|e| e.to_string())?;
    let back: PartListing = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    assert_eq!(back, listing);
    Ok(())
}

fn filter_listing(mpn: &str, category: Category, source: ListingSource, id: &str) -> PartListing {
    let mut l = listing_from_result(&result(mpn, "resistor"), &Default::default());
    l.category = category;
    l.source = source;
    l.source_id = id.into();
    l
}

fn full(mpn: &str, source: ListingSource) -> PartListing {
    let mut l = filter_listing(mpn, Category::Resistor, source, "Sample parts");
    l.package_key = "chip-0603".into();
    l.package_text = "0603".into();
    l.value = Some(10_000.0);
    l.value_unit = Some(Unit::Ohm);
    l.rated_voltage_v = Some(50.0);
    l.temp_min_c = Some(-55.0);
    l.temp_max_c = Some(155.0);
    l.stock = Some(500);
    l.lifecycle = Lifecycle::Active;
    l.rohs = Some(true);
    l
}

#[test]
fn a_filter_on_category_package_value_range_voltage_stock_and_source_returns_only_matching_listings_across_mixed_sources()
 {
    let mut cap = full("C1", ListingSource::Installed);
    cap.category = Category::Capacitor;
    let mut other_pkg = full("R2", ListingSource::Installed);
    other_pkg.package_key = "chip-0805".into();
    let mut big = full("R3", ListingSource::Fixture);
    big.value = Some(1.0e6);
    let mut weak = full("R4", ListingSource::Project);
    weak.rated_voltage_v = Some(10.0);
    let mut low = full("R5", ListingSource::Distributor);
    low.stock = Some(3);
    let mut other_source = full("R6", ListingSource::Distributor);
    other_source.source = ListingSource::Distributor;
    let keep_a = full("R7", ListingSource::Fixture);
    let keep_b = full("R8", ListingSource::Installed);
    let filter = PartFilter {
        categories: vec![Category::Resistor],
        packages: vec!["0603".into()],
        value_min: Some(1_000.0),
        value_max: Some(100_000.0),
        voltage_min: Some(25.0),
        stock_min: Some(100),
        sources: vec![ListingSource::Fixture, ListingSource::Installed],
        temp_range: Some((-40.0, 85.0)),
        lifecycle: vec![Lifecycle::Active],
        rohs: Some(true),
        ..Default::default()
    };
    assert_eq!(filter.validate(), Ok(()));
    let all = [cap, other_pkg, big, weak, low, other_source, keep_a, keep_b];
    let found: Vec<&str> = all
        .iter()
        .filter(|l| filter.matches(l))
        .map(|l| l.mpn.as_str())
        .collect();
    assert_eq!(found, ["R7", "R8"]);
}

#[test]
fn missing_fields_never_match_a_numeric_filter() {
    let mut blank = filter_listing("X", Category::Resistor, ListingSource::Installed, "Lib");
    blank.stock = None;
    let numeric = [
        PartFilter {
            value_min: Some(0.0),
            ..Default::default()
        },
        PartFilter {
            value_max: Some(1.0e12),
            ..Default::default()
        },
        PartFilter {
            voltage_min: Some(0.0),
            ..Default::default()
        },
        PartFilter {
            temp_range: Some((0.0, 1.0)),
            ..Default::default()
        },
        PartFilter {
            stock_min: Some(0),
            ..Default::default()
        },
    ];
    for f in &numeric {
        assert!(!f.matches(&blank), "{f:?} matched an empty listing");
    }
    assert!(
        PartFilter::default().matches(&blank),
        "an empty filter matches all"
    );
    let mut diode = full("D1", ListingSource::Installed);
    diode.category = Category::Diode;
    assert!(!numeric[0].matches(&diode), "no value unit, no value match");
}

#[test]
fn a_filter_with_more_than_sixteen_categories_is_refused() {
    let f = PartFilter {
        categories: vec![Category::Led; 17],
        ..Default::default()
    };
    assert!(f.validate().is_err_and(|e| e.contains("16 categories")));
    let bad = PartFilter {
        value_min: Some(2.0),
        value_max: Some(1.0),
        ..Default::default()
    };
    assert!(bad.validate().is_err());
}

const DEVICE: &str = include_str!("fixtures/device.kicad_sym");
const OPAMP: &str = include_str!("fixtures/lm358.kicad_sym");

fn symbol(text: &str, name: &str) -> Result<SymbolDefinition, String> {
    read_symbols(text)
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|s| s.name == name)
        .ok_or_else(|| format!("{name} not in fixture"))
}

#[test]
fn a_project_symbol_with_a_value_and_footprint_becomes_a_listing_with_package_and_value()
-> Result<(), String> {
    let mut r = symbol(DEVICE, "R")?;
    r.value = "10k".into();
    r.footprint = "Resistor_SMD:R_0805_2012Metric".into();
    let l = listing_from_symbol(&r, ListingSource::Project, "My board");
    assert_eq!(l.source, ListingSource::Project);
    assert_eq!(l.source_id, "My board");
    assert_eq!(l.source_ref, "R");
    assert_eq!(l.category, Category::Resistor);
    assert_eq!((l.value, l.value_unit), (Some(10_000.0), Some(Unit::Ohm)));
    assert_eq!(l.package_key, "chip-0805");
    assert_eq!(
        l.footprint.as_deref(),
        Some("Resistor_SMD:R_0805_2012Metric")
    );
    assert_eq!(l.description, "Resistor");
    assert_eq!(l.stock, None);
    assert_eq!(l.manufacturer, "");
    Ok(())
}

#[test]
fn an_installed_symbol_without_a_value_leaves_value_empty() -> Result<(), String> {
    let mut op = symbol(OPAMP, "LM358")?;
    op.value = String::new();
    let l = listing_from_symbol(&op, ListingSource::Installed, "/lib/lm358.kicad_sym");
    assert_eq!(l.source, ListingSource::Installed);
    assert_eq!((l.value, l.value_unit), (None, None));
    assert_eq!(l.footprint, None);
    assert_eq!(l.package_key, "");
    assert_eq!(
        l.datasheet_url.as_deref(),
        Some("http://www.ti.com/lit/ds/symlink/lm2904-n.pdf")
    );
    Ok(())
}

#[test]
fn more_than_the_listing_cap_is_clipped_with_total_known_set() -> Result<(), String> {
    let r = symbol(DEVICE, "R")?;
    let many = vec![r; 5003];
    let out = listings_from_symbols(&many, ListingSource::Installed, "lib", 10_000);
    assert_eq!(out.items.len(), 5000);
    assert_eq!(out.total_known, 5003);
    let small = listings_from_symbols(&many, ListingSource::Installed, "lib", 3);
    assert_eq!((small.items.len(), small.total_known), (3, 5003));
    Ok(())
}
