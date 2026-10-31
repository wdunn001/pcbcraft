use pcbcraft_doc::{Component, ComponentField, Sourcing, MAX_SOURCING_TEXT};
use pcbcraft_library::listing::Lifecycle;

#[test]
fn an_old_component_with_free_form_fields_loads_unchanged() {
    let json = r#"{
        "id": 1,
        "reference": "R1",
        "value": "10k",
        "kind": "resistor",
        "position": {"x": 0, "y": 0},
        "rotation": 0,
        "pins": [],
        "fields": [{"key": "MPN", "value": "OLD-MPN"}]
    }"#;
    let c: Component = serde_json::from_str(json).expect("load old component");
    assert_eq!(c.field("MPN"), Some("OLD-MPN"));
    assert!(c.sourcing.is_empty());
    assert_eq!(c.mpn(), "OLD-MPN");
}

#[test]
fn moisture_level_outside_one_to_six_is_rejected() {
    let mut s = Sourcing::default();
    s.moisture_level = Some(7);
    assert!(
        s.validate().is_err(),
        "level 7 is refused"
    );
    s.moisture_level = Some(0);
    assert!(s.validate().is_err(), "level 0 is refused");
}

#[test]
fn typed_sourcing_round_trips_without_empty_record() {
    let mut c = Component {
        id: 2,
        reference: "C1".into(),
        value: "100n".into(),
        kind: "capacitor".into(),
        position: pcbcraft_geom::Point::default(),
        rotation: 0.0,
        pins: vec![],
        units: vec![],
        library_symbol: None,
        library_footprint: None,
        fields: vec![],
        sourcing: Sourcing {
            mpn: "C-100N".into(),
            lifecycle: Some(Lifecycle::Nrnd),
            moisture_level: Some(2),
            ..Sourcing::default()
        },
        sheet: 0,
    };
    c.sourcing.validate().expect("valid");
    let text = serde_json::to_string(&c).expect("serialize");
    assert!(
        !text.contains("\"sourcing\":{}"),
        "non-empty sourcing is written"
    );
    let back: Component = serde_json::from_str(&text).expect("deserialize");
    assert_eq!(back.sourcing.mpn, "C-100N");
    assert_eq!(back.sourcing.moisture_level, Some(2));
    let empty = Component {
        id: 3,
        reference: "R2".into(),
        value: "1k".into(),
        kind: "resistor".into(),
        position: pcbcraft_geom::Point::default(),
        rotation: 0.0,
        pins: vec![],
        units: vec![],
        library_symbol: None,
        library_footprint: None,
        fields: vec![],
        sourcing: Sourcing::default(),
        sheet: 0,
    };
    let empty_json = serde_json::to_string(&empty).expect("serialize empty");
    assert!(
        !empty_json.contains("sourcing"),
        "empty sourcing is omitted"
    );
    let _ = MAX_SOURCING_TEXT;
}

#[test]
fn adopt_free_form_copies_mpn_without_removing_the_field() {
    let mut c = Component {
        id: 4,
        reference: "R3".into(),
        value: "10k".into(),
        kind: "resistor".into(),
        position: pcbcraft_geom::Point::default(),
        rotation: 0.0,
        pins: vec![],
        units: vec![],
        library_symbol: None,
        library_footprint: None,
        fields: vec![ComponentField {
            key: "MPN".into(),
            value: "FREE-1".into(),
        }],
        sourcing: Sourcing::default(),
        sheet: 0,
    };
    c.adopt_free_form_sourcing();
    assert_eq!(c.sourcing.mpn, "FREE-1");
    assert_eq!(c.field("MPN"), Some("FREE-1"));
    c.adopt_free_form_sourcing();
    assert_eq!(c.sourcing.mpn, "FREE-1");
}
