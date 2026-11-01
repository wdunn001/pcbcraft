use pcbcraft_doc::{Component, ComponentField, MAX_SOURCING_TEXT, Sourcing};
use pcbcraft_library::listing::Lifecycle;

#[test]
fn an_old_component_with_free_form_fields_loads_unchanged() -> Result<(), String> {
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
    let c: Component = serde_json::from_str(json).map_err(|e| format!("{}: {e:?}", "load old component"))?;
    assert_eq!(c.field("MPN"), Some("OLD-MPN"));
    assert!(c.sourcing.is_empty());
    assert_eq!(c.mpn(), "OLD-MPN");
    Ok(())
}

#[test]
fn moisture_level_outside_one_to_six_is_rejected() {
    let mut s = Sourcing {
        moisture_level: Some(7),
        ..Sourcing::default()
    };
    assert!(s.validate().is_err(), "level 7 is refused");
    s.moisture_level = Some(0);
    assert!(s.validate().is_err(), "level 0 is refused");
}

#[test]
fn typed_sourcing_round_trips_without_empty_record() -> Result<(), String> {
    let c = Component {
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
        part: None,
        compliance: Default::default(),
        sourcing: Sourcing {
            mpn: "C-100N".into(),
            lifecycle: Some(Lifecycle::Nrnd),
            moisture_level: Some(2),
            ..Sourcing::default()
        },
        sheet: 0,
        instance: Default::default(),
        prototype: Default::default(),
        include_in_bom: true,
        include_in_placement: true,
    };
    c.sourcing.validate().map_err(|e| format!("{}: {e:?}", "valid"))?;
    let text = serde_json::to_string(&c).map_err(|e| format!("{}: {e:?}", "serialize"))?;
    assert!(
        !text.contains("\"sourcing\":{}"),
        "non-empty sourcing is written"
    );
    let back: Component = serde_json::from_str(&text).map_err(|e| format!("{}: {e:?}", "deserialize"))?;
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
        part: None,
        sourcing: Sourcing::default(),
        sheet: 0,
        compliance: Default::default(),
        instance: Default::default(),
        prototype: Default::default(),
        include_in_bom: true,
        include_in_placement: true,
    };
    let empty_json = serde_json::to_string(&empty).map_err(|e| format!("{}: {e:?}", "serialize empty"))?;
    assert!(
        !empty_json.contains("sourcing"),
        "empty sourcing is omitted"
    );
    let _ = MAX_SOURCING_TEXT;
    Ok(())
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
        compliance: Default::default(),
        part: None,
        instance: Default::default(),
        prototype: Default::default(),
        include_in_bom: true,
        include_in_placement: true,
    };
    c.adopt_free_form_sourcing();
    assert_eq!(c.sourcing.mpn, "FREE-1");
    assert_eq!(c.field("MPN"), Some("FREE-1"));
    c.adopt_free_form_sourcing();
    assert_eq!(c.sourcing.mpn, "FREE-1");
}
