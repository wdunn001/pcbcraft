//! Story from issue #727: a newcomer checks a symbol against a footprint and is told whether the pins match the pads.
use pcbcraft_engine::Session;
use pcbcraft_geom::Point;
use pcbcraft_library::{
    FootprintDefinition, Graphic, LandPatternDensity, LandPatternParams, SymbolDefinition,
    SymbolPin, UnitGraphic, generate_footprint,
};
use serde_json::json;

fn pin(number: &str) -> SymbolPin {
    SymbolPin {
        number: number.into(),
        name: number.into(),
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

/// A QFN16 footprint and a symbol with pins 1 to `pin_count` plus the exposed pad pin `EP`.
fn symbol_and_footprint(pin_count: u32) -> Result<(SymbolDefinition, FootprintDefinition), String> {
    let params = LandPatternParams {
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
        thermal_size: 0.0,
    };
    let footprint = generate_footprint("qfn", LandPatternDensity::Nominal, &params)?;
    let symbol = SymbolDefinition {
        name: "QFN16".into(),
        reference: "U".into(),
        value: "QFN16".into(),
        description: String::new(),
        keywords: String::new(),
        footprint: footprint.name.clone(),
        set: String::new(),
        pins: (1..=pin_count)
            .map(|n| pin(&n.to_string()))
            .chain(std::iter::once(pin("EP")))
            .collect(),
        graphics: vec![UnitGraphic {
            unit: 1,
            body_style: 1,
            primitive: Graphic::Rectangle {
                start: Point::new(-1.0, -1.0),
                end: Point::new(1.0, 1.0),
                width: 0.1,
                fill: "none".into(),
                layer: String::new(),
            },
        }],
        extends: None,
        raw_source: String::new(),
        unsupported: Vec::new(),
        retained: vec![],
    };
    Ok((symbol, footprint))
}

fn validate(symbol: &SymbolDefinition, footprint: &FootprintDefinition) -> Result<bool, String> {
    let mut session = Session::default();
    let reply = session
        .execute(
            "library.pinpad.validate",
            json!({"symbol_definition": symbol, "footprint_definition": footprint}),
            None,
        )
        .map_err(|e| e.to_string())?;
    reply["result"]["verified"]
        .as_bool()
        .ok_or_else(|| "the reply has no verified flag".to_string())
}

#[test]
fn a_newcomer_is_told_a_matching_symbol_and_footprint_are_verified() -> Result<(), String> {
    let (symbol, footprint) = symbol_and_footprint(16)?;
    assert!(
        validate(&symbol, &footprint)?,
        "16 pins and the exposed pad match 17 pads"
    );
    Ok(())
}

#[test]
fn a_newcomer_is_told_a_symbol_missing_a_pin_is_not_verified() -> Result<(), String> {
    let (symbol, footprint) = symbol_and_footprint(15)?;
    assert!(
        !validate(&symbol, &footprint)?,
        "a numbered pad with no pin must not verify"
    );
    Ok(())
}

#[test]
fn a_newcomer_is_asked_for_a_symbol_and_a_footprint() -> Result<(), String> {
    let mut session = Session::default();
    let refused = session.execute("library.pinpad.validate", json!({}), None);
    assert!(refused.is_err(), "no symbol or footprint is refused");
    Ok(())
}
