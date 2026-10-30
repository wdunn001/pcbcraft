//! Typed part source fields on components (#324).
use pcbcraft_doc::{Component, Project};
use serde_json::json;

type T = Result<(), String>;

fn part(extra: serde_json::Value) -> Result<Component, String> {
    let mut v = json!({"id":1,"reference":"R1","value":"10k","kind":"resistor",
        "position":{"x":0.0,"y":0.0},"rotation":0.0,"pins":[]});
    if let (Some(base), Some(more)) = (v.as_object_mut(), extra.as_object()) {
        base.extend(more.clone());
    }
    serde_json::from_value(v).map_err(|e| e.to_string())
}

#[test]
fn typed_part_source_round_trips_through_the_project_file() -> T {
    let mut c = part(json!({}))?;
    c.part_source.manufacturer = "Yageo".into();
    c.part_source.mpn = "RC0603FR-0710KL".into();
    c.part_source.orderables.push(pcbcraft_doc::SourceOrderable {
        distributor: "Digi-Key".into(),
        sku: "311-10.0KHRCT-ND".into(),
        url: "https://example.test/p".into(),
        stock: "5000".into(),
    });
    c.part_source.datasheet = Some(pcbcraft_doc::SourceDatasheet {
        url: "https://example.test/d.pdf".into(),
        retrieved_at: 1_790_000_000,
    });
    c.part_source.lifecycle = Some(pcbcraft_doc::SourceLifecycle {
        status: "active".into(),
        checked_at: 1_790_000_100,
    });
    let mut p = Project::default();
    p.components.push(c.clone());
    let text = serde_json::to_string(&p).map_err(|e| e.to_string())?;
    let back: Project = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    assert_eq!(back.components[0].part_source, c.part_source);
    Ok(())
}

#[test]
fn an_empty_part_source_is_not_written() -> T {
    let c = part(json!({}))?;
    let text = serde_json::to_string(&c).map_err(|e| e.to_string())?;
    assert!(!text.contains("part_source"), "{text}");
    Ok(())
}

#[test]
fn old_property_style_projects_load_into_typed_fields() -> T {
    let mut c = part(json!({"fields":[
        {"key":"Manufacturer","value":"Legacy Maker"},
        {"key":"MPN","value":"LEG-1"},
        {"key":"Datasheet","value":"https://example.invalid/legacy.pdf"},
        {"key":"Shop SKU","value":"SH-9"}
    ]}))?;
    assert!(c.part_source.is_empty());
    c.adopt_property_part_source();
    assert_eq!(c.part_source.manufacturer, "Legacy Maker");
    assert_eq!(c.part_source.mpn, "LEG-1");
    assert_eq!(
        c.part_source.datasheet.as_ref().map(|d| d.url.as_str()),
        Some("https://example.invalid/legacy.pdf")
    );
    assert_eq!(c.part_source.orderables[0].distributor, "Shop");
    assert_eq!(c.part_source.orderables[0].sku, "SH-9");
    assert_eq!(c.fields.len(), 4);
    let again = c.clone();
    c.adopt_property_part_source();
    assert_eq!(c, again, "migration is idempotent when typed fields are set");
    Ok(())
}

#[test]
fn a_typed_part_source_beats_free_form_properties() -> T {
    let mut c = part(json!({"part_source":{"manufacturer":"Typed","mpn":"T-1"},
        "fields":[{"key":"Manufacturer","value":"OLD"},{"key":"MPN","value":"OLD-M"}]}))?;
    c.adopt_property_part_source();
    assert_eq!(c.part_source.manufacturer, "Typed");
    assert_eq!(c.part_source.mpn, "T-1");
    Ok(())
}

#[test]
fn project_migration_lifts_properties() -> T {
    let mut p = Project::default();
    p.components.push(part(json!({"fields":[
        {"key":"MPN","value":"X1"}
    ]}))?);
    p.migrate_component_part_sources();
    assert_eq!(p.components[0].part_source.mpn, "X1");
    Ok(())
}
