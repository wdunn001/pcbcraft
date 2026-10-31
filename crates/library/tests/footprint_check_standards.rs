//! Standards-rule coverage for footprint and symbol checkers.
use pcbcraft_geom::Point;
use pcbcraft_library::{
    FootprintDefinition, FootprintPad, Graphic, Model3D, SymbolDefinition, SymbolPin,
    check_footprint, check_symbol,
};

fn smd_pad(number: &str, x: f64, y: f64) -> FootprintPad {
    FootprintPad {
        number: number.into(),
        pad_type: "smd".into(),
        shape: "rect".into(),
        position: Point::new(x, y),
        rotation: 0.0,
        size: Point::new(1.0, 1.0),
        layers: vec!["F.Cu".into(), "F.Mask".into(), "F.Paste".into()],
        drill: None,
        roundrect_ratio: None,
        chamfer: None,
        polygon: vec![],
        raw_source: String::new(),
    }
}

fn bare_footprint(pads: Vec<FootprintPad>, graphics: Vec<Graphic>) -> FootprintDefinition {
    FootprintDefinition {
        name: "RESC2012X70N".into(),
        description: String::new(),
        keywords: String::new(),
        pads,
        graphics,
        model: None,
        rules: None,
        raw_source: String::new(),
        unsupported: vec![],
        retained: vec![],
    }
}

fn has_code(findings: &[pcbcraft_library::FootprintFinding], code: &str) -> bool {
    findings.iter().any(|f| f.code == code)
}

#[test]
fn pad_number_gap_is_reported() {
    let def = bare_footprint(
        vec![smd_pad("1", -1.0, 0.0), smd_pad("3", 1.0, 0.0)],
        vec![],
    );
    assert!(has_code(&check_footprint(&def), "lib_pad_number_missing"));
}

#[test]
fn courtyard_off_grid_is_reported() {
    let def = bare_footprint(
        vec![smd_pad("1", 0.0, 0.0)],
        vec![Graphic::Rectangle {
            start: Point::new(-1.001, -1.0),
            end: Point::new(1.0, 1.0),
            width: 0.12,
            fill: "none".into(),
            layer: "pcbcraft.courtyard".into(),
        }],
    );
    assert!(has_code(&check_footprint(&def), "lib_courtyard_off_grid"));
}

#[test]
fn smd_missing_paste_is_reported() {
    let mut pad = smd_pad("1", 0.0, 0.0);
    pad.layers = vec!["F.Cu".into(), "F.Mask".into()];
    let def = bare_footprint(vec![pad], vec![]);
    assert!(has_code(
        &check_footprint(&def),
        "lib_smd_pad_missing_mask_or_paste"
    ));
}

#[test]
fn npth_with_copper_is_reported() {
    let mut pad = smd_pad("1", 0.0, 0.0);
    pad.pad_type = "np_thru_hole".into();
    let def = bare_footprint(vec![pad], vec![]);
    assert!(has_code(&check_footprint(&def), "lib_npth_has_copper"));
}

#[test]
fn symbol_duplicate_position_is_reported() {
    let pin = SymbolPin {
        number: "1".into(),
        name: "A".into(),
        position: Point::new(0.0, 2.54),
        angle: 0.0,
        length: 2.54,
        electrical_type: "passive".into(),
        unit: 1,
        body_style: 1,
        graphical_style: "line".into(),
        visible: true,
    };
    let def = SymbolDefinition {
        name: "U".into(),
        reference: "U".into(),
        value: "U".into(),
        description: String::new(),
        keywords: String::new(),
        footprint: String::new(),
        set: String::new(),
        pins: vec![
            pin.clone(),
            SymbolPin {
                number: "2".into(),
                ..pin
            },
        ],
        graphics: vec![],
        extends: None,
        raw_source: String::new(),
        unsupported: vec![],
        retained: vec![],
    };
    assert!(has_code(&check_symbol(&def), "lib_pin_duplicate_position"));
}

#[test]
fn symbol_side_warning_for_top_pins() {
    let def = SymbolDefinition {
        name: "U".into(),
        reference: "U".into(),
        value: "U".into(),
        description: String::new(),
        keywords: String::new(),
        footprint: String::new(),
        set: String::new(),
        pins: (1..=4)
            .map(|n| SymbolPin {
                number: n.to_string(),
                name: n.to_string(),
                position: Point::new(0.0, n as f64 * 2.54),
                angle: 270.0,
                length: 2.54,
                electrical_type: "passive".into(),
                unit: 1,
                body_style: 1,
                graphical_style: "line".into(),
                visible: true,
            })
            .collect(),
        graphics: vec![],
        extends: None,
        raw_source: String::new(),
        unsupported: vec![],
        retained: vec![],
    };
    assert!(has_code(&check_symbol(&def), "lib_symbol_pins_top_bottom"));
}

#[test]
fn model_missing_warns_for_smd() {
    let def = bare_footprint(vec![smd_pad("1", 0.0, 0.0)], vec![]);
    assert!(has_code(&check_footprint(&def), "lib_model_missing"));
}

#[test]
fn thin_silk_is_reported() {
    let def = bare_footprint(
        vec![smd_pad("1", 0.0, 0.0)],
        vec![Graphic::Line {
            start: Point::new(-1.0, -1.0),
            end: Point::new(1.0, -1.0),
            width: 0.05,
            layer: "F.SilkS".into(),
        }],
    );
    assert!(has_code(&check_footprint(&def), "lib_silk_too_thin"));
}

#[test]
fn refdes_text_height_is_checked() {
    let def = bare_footprint(
        vec![smd_pad("1", 0.0, 0.0)],
        vec![Graphic::Text {
            position: Point::new(0.0, 0.0),
            angle: 0.0,
            text: "REF**".into(),
            size: Point::new(0.5, 0.5),
            layer: "F.Fab".into(),
        }],
    );
    assert!(has_code(&check_footprint(&def), "lib_refdes_text_small"));
}

#[test]
fn model_offset_unit_mismatch_is_reported() {
    let mut def = bare_footprint(vec![smd_pad("1", 0.0, 0.0)], vec![]);
    def.model = Some(Model3D {
        path: "pkg.step".into(),
        offset: [50.8, 0.0, 0.0],
        scale: [1.0, 1.0, 1.0],
        rotate: [0.0, 0.0, 0.0],
        raw_source: "(offset (xyz 50.8 0 0))".into(),
    });
    assert!(has_code(
        &check_footprint(&def),
        "lib_model_origin_mismatch"
    ));
}
