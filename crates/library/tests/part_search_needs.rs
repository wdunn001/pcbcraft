use std::collections::BTreeMap;

use pcbcraft_library::listing::{Category, Lifecycle, ListingSource, PartListing};
use pcbcraft_library::part_search::{PartSearchIndex, PartSearchQuery, recall_at_k};

/// Build a fixture part listing with the given MPN, description and category.
fn part(
    mpn: &str,
    description: &str,
    category: Category,
    package_key: &str,
    voltage: f64,
) -> PartListing {
    PartListing {
        source: ListingSource::Fixture,
        source_id: "test".into(),
        source_ref: mpn.into(),
        manufacturer: "Example".into(),
        mpn: mpn.into(),
        description: description.into(),
        category,
        source_category: String::new(),
        package_key: package_key.into(),
        package_text: package_key.into(),
        value: None,
        value_unit: None,
        tolerance_pct: None,
        rated_voltage_v: Some(voltage),
        temp_min_c: None,
        temp_max_c: None,
        power_w: None,
        stock: Some(100),
        stock_at: None,
        price_breaks: vec![],
        min_order_qty: None,
        order_multiple: None,
        packaging: None,
        lifecycle: Lifecycle::Active,
        lifecycle_raw: String::new(),
        rohs: Some(true),
        reach: None,
        halogen_free: None,
        moisture_level: None,
        datasheet_url: None,
        symbol: None,
        footprint: None,
        model3d: None,
        attribution: None,
    }
}

/// The fifteen stated needs with their expected parts.
fn needs() -> Vec<(&'static str, &'static str)> {
    vec![
        ("low noise 3.3 V regulator in a small package", "EX-LDO-33"),
        ("10 kilohm resistor 0603", "EX-R-10K"),
        ("100 nanofarad capacitor 16 volt", "EX-C-100N"),
        ("USB C receptacle", "EX-USBC"),
        (
            "microcontroller with I2C and 64 kilobyte flash",
            "EX-MCU-64",
        ),
        ("10 microhenry inductor", "EX-L-10UH"),
        ("4.7 kilohm pull-up resistor 0402", "EX-R-4K7"),
        ("green LED 0805", "EX-LED-G"),
        ("12 volt to 5 volt buck converter", "EX-BUCK-5"),
        ("1 megohm trim pot", "EX-POT-1M"),
        ("100 ohm current sense resistor", "EX-R-100"),
        ("2.2 microfarad tantalum capacitor", "EX-C-2U2"),
        ("10 MHz crystal oscillator", "EX-XO-10M"),
        ("N channel MOSFET logic level", "EX-MOS-N"),
        ("16 pin DIP header", "EX-HDR-16"),
        ("fast switching signal diode", "EX-D-1N4148"),
        ("red LED in 0603 package", "EX-LED-R"),
        ("8 MHz quartz crystal", "EX-CRYSTAL-8M"),
        ("JST PH two pin connector", "EX-CONN-JST"),
        ("temperature sensor I2C digital output", "EX-SENSOR-TEMP"),
        ("three axis accelerometer digital", "EX-SENOR-ACCEL"),
        ("SPI serial flash memory chip", "EX-MEM-FLASH"),
        ("dual op amp in small package", "EX-OPAMP-DUAL"),
        ("P channel MOSFET logic level", "EX-MOS-P"),
        ("resettable polyfuse 500 mA", "EX-FUSE-POLY"),
        ("47 microhenry power inductor", "EX-IND-47UH"),
        ("5 V LDO 500 mA regulator", "EX-LDO-5V"),
        ("3.3 V LDO 500 mA regulator", "EX-LDO-3V3"),
        ("220 ohm resistor 0402", "EX-RES-220R"),
        ("1 microfarad ceramic capacitor 50 volt", "EX-CAP-1UF"),
    ]
}

/// The parts that satisfy the fifteen needs.
fn listings() -> Vec<PartListing> {
    vec![
        part(
            "EX-LDO-33",
            "Low noise 3.3 V LDO linear regulator small package",
            Category::IcRegulator,
            "sot-23",
            3.3,
        ),
        part(
            "EX-R-10K",
            "10 kilohm resistor 0603 1 percent",
            Category::Resistor,
            "chip-0603",
            50.0,
        ),
        part(
            "EX-C-100N",
            "100 nanofarad ceramic capacitor 16 volt 0603",
            Category::Capacitor,
            "chip-0603",
            16.0,
        ),
        part(
            "EX-USBC",
            "USB C receptacle 16 pin surface mount",
            Category::Connector,
            "usb-c",
            5.0,
        ),
        part(
            "EX-MCU-64",
            "Microcontroller I2C SPI 64 kilobyte flash 3.3 V",
            Category::IcMcu,
            "lqfp-48",
            3.3,
        ),
        part(
            "EX-L-10UH",
            "10 microhenry inductor 0603",
            Category::Inductor,
            "chip-0603",
            50.0,
        ),
        part(
            "EX-R-4K7",
            "4.7 kilohm resistor 0402 1 percent pull-up",
            Category::Resistor,
            "chip-0402",
            50.0,
        ),
        part(
            "EX-LED-G",
            "Green LED 0805 20 mA",
            Category::Led,
            "chip-0805",
            2.0,
        ),
        part(
            "EX-BUCK-5",
            "12 volt to 5 volt buck converter step-down",
            Category::IcRegulator,
            "sot-23",
            12.0,
        ),
        part(
            "EX-POT-1M",
            "1 megohm trim pot variable resistor",
            Category::Resistor,
            "pot-3296",
            50.0,
        ),
        part(
            "EX-R-100",
            "100 ohm resistor 0805 current sense",
            Category::Resistor,
            "chip-0805",
            50.0,
        ),
        part(
            "EX-C-2U2",
            "2.2 microfarad tantalum capacitor 16 volt",
            Category::Capacitor,
            "tantalum-a",
            16.0,
        ),
        part(
            "EX-XO-10M",
            "10 MHz crystal oscillator surface mount",
            Category::Crystal,
            "xo-3225",
            3.3,
        ),
        part(
            "EX-MOS-N",
            "N channel MOSFET logic level 30 volt",
            Category::Mosfet,
            "sot-23",
            30.0,
        ),
        part(
            "EX-HDR-16",
            "16 pin DIP header male 2.54 mm",
            Category::Connector,
            "dip-16",
            50.0,
        ),
        part(
            "EX-D-1N4148",
            "1N4148 fast switching signal diode",
            Category::Diode,
            "do-35",
            100.0,
        ),
        part(
            "EX-LED-R",
            "Red LED 0603 20 mA",
            Category::Led,
            "chip-0603",
            2.0,
        ),
        part(
            "EX-CRYSTAL-8M",
            "8 MHz quartz crystal oscillator",
            Category::Crystal,
            "hc-49",
            3.3,
        ),
        part(
            "EX-CONN-JST",
            "JST PH 2 pin connector",
            Category::Connector,
            "jst-ph-2",
            50.0,
        ),
        part(
            "EX-SENSOR-TEMP",
            "Temperature sensor I2C digital output",
            Category::IcAnalog,
            "sot-23",
            3.3,
        ),
        part(
            "EX-SENOR-ACCEL",
            "Three axis accelerometer digital I2C",
            Category::IcAnalog,
            "lga-14",
            3.3,
        ),
        part(
            "EX-MEM-FLASH",
            "SPI serial flash memory chip 2 megabit",
            Category::IcAnalog,
            "soic-8",
            3.3,
        ),
        part(
            "EX-OPAMP-DUAL",
            "Dual op amp in small package rail to rail",
            Category::IcAnalog,
            "sot-23",
            5.0,
        ),
        part(
            "EX-MOS-P",
            "P channel MOSFET logic level 30 volt",
            Category::Mosfet,
            "sot-23",
            30.0,
        ),
        part(
            "EX-FUSE-POLY",
            "Resettable polyfuse 500 mA",
            Category::Other,
            "radial-6",
            6.0,
        ),
        part(
            "EX-IND-47UH",
            "47 microhenry power inductor shielded",
            Category::Inductor,
            "smd-1210",
            50.0,
        ),
        part(
            "EX-LDO-5V",
            "5 V LDO linear regulator 500 mA low dropout",
            Category::IcRegulator,
            "sot-223",
            5.0,
        ),
        part(
            "EX-LDO-3V3",
            "3.3 V LDO linear regulator 500 mA low dropout",
            Category::IcRegulator,
            "sot-223",
            3.3,
        ),
        part(
            "EX-RES-220R",
            "220 ohm resistor 0402 1 percent",
            Category::Resistor,
            "chip-0402",
            50.0,
        ),
        part(
            "EX-CAP-1UF",
            "1 microfarad ceramic capacitor 50 volt 0805",
            Category::Capacitor,
            "chip-0805",
            50.0,
        ),
    ]
}

#[test]
fn fifteen_stated_needs_find_their_part_with_no_model() -> Result<(), String> {
    let listings = listings();
    let index = PartSearchIndex::build(&listings, &BTreeMap::new())?;
    let cases = needs();
    let recall = recall_at_k(&index, &cases, 3);
    assert!(
        recall >= 0.9,
        "recall@3 should be at least 0.9 on fifteen stated needs, got {recall}"
    );
    Ok(())
}

#[test]
fn each_hit_carries_reasons() -> Result<(), String> {
    let listings = listings();
    let index = PartSearchIndex::build(&listings, &BTreeMap::new())?;
    let query = PartSearchQuery {
        need: "low noise 3.3 V regulator in a small package".into(),
        ..PartSearchQuery::default()
    };
    let hits = index.search(&query, 3)?;
    let top = hits.first().ok_or("regulator search returned no hits")?;
    assert!(
        !top.reasons.is_empty(),
        "top hit should carry match reasons"
    );
    assert_eq!(top.listing.mpn, "EX-LDO-33");
    Ok(())
}

#[test]
fn the_index_reports_no_vectors() -> Result<(), String> {
    let listings = listings();
    let index = PartSearchIndex::build(&listings, &BTreeMap::new())?;
    let json = index.to_json()?;
    assert!(
        !json.contains("\"vector\""),
        "index with no vectors should not contain vector fields"
    );
    Ok(())
}

#[test]
fn thirty_stated_needs_meet_the_recall_floor_with_no_model() -> Result<(), String> {
    let listings = listings();
    let index = PartSearchIndex::build(&listings, &BTreeMap::new())?;
    let cases = needs();
    let recall_at_3 = recall_at_k(&index, &cases, 3);
    let recall_at_1 = recall_at_k(&index, &cases, 1);
    assert!(
        recall_at_3 >= 0.9,
        "recall@3 should be at least 0.9 on thirty stated needs, got {recall_at_3}"
    );
    assert!(
        recall_at_1 >= 0.7,
        "recall@1 should be at least 0.7 on thirty stated needs, got {recall_at_1}"
    );
    Ok(())
}

#[test]
fn near_neighbour_needs_pick_the_right_voltage() -> Result<(), String> {
    let listings = listings();
    let index = PartSearchIndex::build(&listings, &BTreeMap::new())?;

    let query_5v = PartSearchQuery {
        need: "5 V LDO 500 mA regulator".into(),
        ..PartSearchQuery::default()
    };
    let hits_5v = index.search(&query_5v, 5)?;
    let top_5v = hits_5v.first().ok_or("5 V LDO search returned no hits")?;
    assert_eq!(
        top_5v.listing.mpn, "EX-LDO-5V",
        "top hit for 5 V LDO need must be the 5 V part, got {}",
        top_5v.listing.mpn
    );

    let query_3v3 = PartSearchQuery {
        need: "3.3 V LDO 500 mA regulator".into(),
        ..PartSearchQuery::default()
    };
    let hits_3v3 = index.search(&query_3v3, 5)?;
    let top_3v3 = hits_3v3
        .first()
        .ok_or("3.3 V LDO search returned no hits")?;
    assert_eq!(
        top_3v3.listing.mpn, "EX-LDO-3V3",
        "top hit for 3.3 V LDO need must be the 3.3 V part, got {}",
        top_3v3.listing.mpn
    );
    Ok(())
}
