use pcbcraft_geom::Point;
use pcbcraft_library::{SymbolDefinition, SymbolPin, infer_component_kind};

fn three_pin_symbol(keywords: &str) -> SymbolDefinition {
    SymbolDefinition {
        name: "PART".into(),
        reference: "U".into(),
        value: "PART".into(),
        description: String::new(),
        keywords: keywords.into(),
        footprint: String::new(),
        set: String::new(),
        pins: (1..=3)
            .map(|n| SymbolPin {
                number: n.to_string(),
                name: format!("P{n}"),
                position: Point::new(n as f64, 0.0),
                angle: 0.0,
                length: 2.54,
                electrical_type: "passive".into(),
                unit: 1,
                body_style: 0,
                graphical_style: "line".into(),
                visible: true,
            })
            .collect(),
        graphics: vec![],
        extends: None,
        raw_source: String::new(),
        unsupported: vec![],
    }
}

#[test]
fn ldo_keyword_maps_to_ldo_kind() {
    let sym = three_pin_symbol("LDO linear regulator");
    assert_eq!(infer_component_kind(&sym, None), "ldo");
}

#[test]
fn bare_three_pin_defaults_to_ic() {
    let sym = three_pin_symbol("");
    assert_eq!(infer_component_kind(&sym, None), "ic");
}
