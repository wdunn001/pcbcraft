//! Stories for filling a drop-in part from a symbol and a footprint (issue 716).
//!
//! # Dependencies
//! - crates/library/src/drop_in_check.rs
//! - crates/library/src/pin_pad_map.rs
use pcbcraft_geom::Point;
use pcbcraft_library::{
    DropInPart, FootprintDefinition, LandPatternDensity, LandPatternParams, SymbolDefinition,
    SymbolPin, drop_in_check, generate_footprint,
};

fn pin(number: &str, name: &str) -> SymbolPin {
    SymbolPin {
        number: number.into(),
        name: name.into(),
        position: Point::new(0.0, 0.0),
        angle: 0.0,
        length: 2.54,
        electrical_type: "passive".into(),
        unit: 1,
        body_style: 1,
        graphical_style: "line".into(),
        visible: true,
    }
}

fn symbol_with(pins: Vec<SymbolPin>) -> SymbolDefinition {
    SymbolDefinition {
        name: "QFN16".into(),
        reference: "U".into(),
        value: "QFN16".into(),
        description: String::new(),
        keywords: String::new(),
        footprint: String::new(),
        set: String::new(),
        pins,
        graphics: vec![],
        extends: None,
        raw_source: String::new(),
        unsupported: Vec::new(),
        retained: vec![],
    }
}

/// A generated 16 pin QFN footprint: pads 1 to 16 and the exposed pad EP.
fn qfn16() -> Result<FootprintDefinition, String> {
    let p = LandPatternParams {
        name: "QFN16_test".into(),
        description: String::new(),
        keywords: String::new(),
        pitch: 0.5,
        pin_count: 16,
        body_length: 3.0,
        body_width: 3.0,
        row_spacing: 0.0,
        lead_width: 0.0,
        lead_diameter: 0.0,
        grid_rows: 0,
        grid_cols: 0,
        thermal_size: 1.5,
    };
    generate_footprint("qfn", LandPatternDensity::Nominal, &p)
}

/// The symbol that matches `qfn16`: pins 1 to 16 and an EP pin.
fn qfn16_pins() -> Vec<SymbolPin> {
    let mut pins: Vec<SymbolPin> = (1..=16)
        .map(|n| pin(&n.to_string(), &format!("P{n}")))
        .collect();
    pins.push(pin("EP", "GND"));
    pins
}

fn blank() -> Result<DropInPart, String> {
    serde_json::from_value(serde_json::json!({"footprint": "unfilled", "lifecycle": "active"}))
        .map_err(|e| e.to_string())
}

#[test]
fn a_matching_symbol_and_footprint_fill_the_pads_pins_and_map() -> Result<(), String> {
    let fp = qfn16()?;
    let part = blank()?.with_geometry(&symbol_with(qfn16_pins()), &fp);
    assert_eq!(part.footprint, "QFN16_test");
    assert_eq!(part.pads.len(), 17, "pads 1 to 16 and EP: {:?}", part.pads);
    assert!(part.pads.iter().any(|p| p == "EP"));
    assert_eq!(part.pins.len(), 17);
    assert_eq!(part.pin_to_pad.get("EP"), Some(&vec!["EP".to_string()]));
    assert_eq!(part.pin_to_pad.get("7"), Some(&vec!["7".to_string()]));
    let report = drop_in_check(&part, &part)?;
    let pinout = report
        .findings
        .iter()
        .find(|f| f.code == "pinout")
        .ok_or("pinout finding missing")?;
    assert!(pinout.pass, "{}", pinout.message);
    Ok(())
}

#[test]
fn a_symbol_with_a_missing_pin_leaves_that_pad_unmapped() -> Result<(), String> {
    let fp = qfn16()?;
    let full = blank()?.with_geometry(&symbol_with(qfn16_pins()), &fp);
    let mut pins = qfn16_pins();
    pins.retain(|p| p.number != "5");
    let short = blank()?.with_geometry(&symbol_with(pins), &fp);
    assert!(
        short.pads.iter().any(|p| p == "5"),
        "the pad is still there"
    );
    assert!(short.pins.iter().all(|p| p.number != "5"));
    assert!(
        !short.pin_to_pad.contains_key("5"),
        "nothing maps to pad 5: {:?}",
        short.pin_to_pad
    );
    let report = drop_in_check(&full, &short)?;
    assert!(
        !report.drop_in,
        "a part with a missing pin is not a drop-in"
    );
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.code == "pinout" && !f.pass)
    );
    Ok(())
}

#[test]
fn a_pin_without_a_pad_gets_no_empty_map_entry() -> Result<(), String> {
    let fp = qfn16()?;
    let mut pins = qfn16_pins();
    pins.push(pin("99", "NC"));
    let part = blank()?.with_geometry(&symbol_with(pins), &fp);
    assert!(part.pins.iter().any(|p| p.number == "99"));
    assert!(!part.pin_to_pad.contains_key("99"));
    // The part must still be accepted by the check (an empty entry would be refused).
    drop_in_check(&part, &part)?;
    Ok(())
}

#[test]
fn a_list_over_the_cap_returns_the_part_unchanged() -> Result<(), String> {
    let fp = qfn16()?;
    let before = blank()?;
    // More than 512 pins.
    let many_pins: Vec<SymbolPin> = (1..=513).map(|n| pin(&n.to_string(), "X")).collect();
    let after = before.clone().with_geometry(&symbol_with(many_pins), &fp);
    assert_eq!(after, before, "too many pins leaves the part as it was");
    // More than 512 pads.
    let mut big = fp.clone();
    let template = big.pads[0].clone();
    big.pads = (1..=513)
        .map(|n| pcbcraft_library::FootprintPad {
            number: n.to_string(),
            ..template.clone()
        })
        .collect();
    let after = before
        .clone()
        .with_geometry(&symbol_with(qfn16_pins()), &big);
    assert_eq!(after, before, "too many pads leaves the part as it was");
    Ok(())
}
