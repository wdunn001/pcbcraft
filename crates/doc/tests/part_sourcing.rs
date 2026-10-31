//! Typed sourcing fields: round-trip, bounds and the free-form migration (#227, #804).
use pcbcraft_doc::{Component, MAX_SUPPLIERS, Project, Sourcing};
use serde_json::{Value, json};

type T = Result<(), String>;

fn part(extra: Value) -> Result<Component, String> {
    let mut v = json!({"id":1,"reference":"R1","value":"10k","kind":"resistor",
        "position":{"x":0.0,"y":0.0},"rotation":0.0,"pins":[]});
    if let (Some(base), Some(more)) = (v.as_object_mut(), extra.as_object()) {
        base.extend(more.clone());
    }
    serde_json::from_value(v).map_err(|e| e.to_string())
}

#[test]
fn typed_sourcing_round_trips_through_the_project_file() -> T {
    let mut c = part(json!({}))?;
    c.sourcing.manufacturer = "Yageo".into();
    c.sourcing.mpn = "RC0603FR-0710KL".into();
    c.sourcing.description = "10 k 1% 0603".into();
    c.sourcing.set_supplier(
        "Digi-Key",
        Some("311-10.0KHRCT-ND"),
        Some("https://example.test/p"),
    )?;
    c.sourcing.set_supplier("House", Some("C25804"), None)?;
    c.sourcing.moisture_level = Some(3);
    c.sourcing.rohs = Some(true);
    let mut p = Project::default();
    p.components.push(c.clone());
    let text = serde_json::to_string(&p).map_err(|e| e.to_string())?;
    let back: Project = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    assert_eq!(back.components[0].sourcing, c.sourcing);
    assert_eq!(back, p);
    Ok(())
}

#[test]
fn an_empty_record_is_not_written() -> T {
    let c = part(json!({}))?;
    let text = serde_json::to_string(&c).map_err(|e| e.to_string())?;
    assert!(!text.contains("sourcing"), "{text}");
    Ok(())
}

#[test]
fn an_old_component_loads_with_an_empty_record_and_its_fields_untouched() -> T {
    let c = part(json!({"fields":[{"key":"MPN","value":"X1"},{"key":"note","value":"hi"}]}))?;
    assert!(c.sourcing.is_empty());
    assert_eq!(c.field("MPN"), Some("X1"));
    assert_eq!(c.mpn(), "X1", "readers fall back to the free-form field");
    Ok(())
}

#[test]
fn free_form_names_are_adopted_and_the_originals_stay() -> T {
    let mut c = part(json!({"fields":[
        {"key":"mpn","value":"RC0603"},
        {"key":"Manufacturer","value":"Yageo"},
        {"key":"Description","value":"10 k resistor"},
        {"key":"Digi-Key PN","value":"311-ND"},
        {"key":"Mouser SKU","value":"603-RC"},
        {"key":"SKU","value":"no supplier named"},
        {"key":"Manufacturer Part Number","value":"IGNORED-SECOND"},
        {"key":"note","value":"keep"}
    ]}))?;
    c.adopt_free_form_sourcing();
    let s = &c.sourcing;
    assert_eq!(s.mpn, "RC0603", "the first match wins");
    assert_eq!(s.manufacturer, "Yageo");
    assert_eq!(s.description, "10 k resistor");
    assert_eq!(s.supplier_number("Digi-Key"), Some("311-ND"));
    assert_eq!(s.supplier_number("mouser"), Some("603-RC"));
    assert_eq!(s.suppliers.len(), 2, "a bare SKU names no supplier");
    assert_eq!(c.fields.len(), 8, "no original is removed");
    assert_eq!(c.field("Digi-Key PN"), Some("311-ND"));
    let again = c.clone();
    c.adopt_free_form_sourcing();
    assert_eq!(c, again, "a second run changes nothing");
    Ok(())
}

#[test]
fn a_typed_value_beats_a_free_form_one() -> T {
    let mut c = part(json!({"sourcing":{"mpn":"TYPED","suppliers":[
        {"supplier":"Digi-Key","part_number":"TYPED-DK"}]},
        "fields":[{"key":"MPN","value":"OLD"},{"key":"Digi-Key PN","value":"OLD-DK"}]}))?;
    c.adopt_free_form_sourcing();
    assert_eq!(c.sourcing.mpn, "TYPED");
    assert_eq!(c.sourcing.supplier_number("Digi-Key"), Some("TYPED-DK"));
    assert_eq!(c.mpn(), "TYPED");
    Ok(())
}

#[test]
fn moisture_level_outside_one_to_six_is_rejected() {
    for bad in [0u8, 7, 200] {
        let s = Sourcing {
            moisture_level: Some(bad),
            ..Sourcing::default()
        };
        assert!(s.validate().is_err(), "{bad}");
    }
    for good in 1u8..=6 {
        let s = Sourcing {
            moisture_level: Some(good),
            ..Sourcing::default()
        };
        assert!(s.validate().is_ok(), "{good}");
    }
}

#[test]
fn a_project_with_too_many_suppliers_does_not_validate() -> T {
    let many: Vec<Value> = (0..=MAX_SUPPLIERS)
        .map(|i| json!({"supplier":format!("S{i}"),"part_number":"1"}))
        .collect();
    let c = part(json!({"sourcing":{"suppliers":many}}))?;
    let mut p = Project::default();
    p.components.push(c);
    assert!(p.validate().is_err());
    Ok(())
}

#[test]
fn bom_values_read_typed_keys_then_free_form_keys() -> T {
    let mut c = part(json!({"fields":[{"key":"Tolerance","value":"1%"}]}))?;
    c.sourcing.mpn = "M".into();
    c.sourcing.set_supplier("A", Some("1"), None)?;
    c.sourcing.set_supplier("B", Some("2"), None)?;
    assert_eq!(c.bom_value("MPN"), "M");
    assert_eq!(c.bom_value("Suppliers"), "A: 1; B: 2");
    assert_eq!(c.bom_value("Supplier:b"), "2");
    assert_eq!(c.bom_value("Tolerance"), "1%");
    assert_eq!(c.bom_value("RoHS"), "");
    Ok(())
}
