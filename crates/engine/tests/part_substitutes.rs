//! library.part.set_substitute: bounds, undo and validation (#719).
use pcbcraft_engine::{Error, Session};
use pcbcraft_library::PartSubstitutes;
use serde_json::json;

fn missing() -> Error {
    Error::Invalid("missing field".into())
}

fn upsert_symbol(s: &mut Session, name: &str) -> Result<(), Error> {
    s.execute(
        "library.symbol.upsert",
        json!({
            "definition": {
                "name": name,
                "reference": "R",
                "value": "R",
                "description": "",
                "keywords": "",
                "footprint": "",
                "pins": [{
                    "number": "1",
                    "name": "~",
                    "position": {"x": 0.0, "y": 0.0},
                    "angle": 0.0,
                    "length": 2.54,
                    "electrical_type": "passive",
                    "unit": 1,
                    "body_style": 1,
                    "graphical_style": "line",
                    "visible": true
                }],
                "graphics": [],
                "raw_source": "",
                "unsupported": []
            }
        }),
        None,
    )?;
    Ok(())
}

fn substitutes(s: &Session, name: &str) -> Result<PartSubstitutes, Error> {
    s.project
        .library
        .part_substitutes
        .get(name)
        .cloned()
        .ok_or_else(missing)
}

#[test]
fn set_substitute_adds_second_sources_and_alternates_with_undo() -> Result<(), Error> {
    let mut s = Session::default();
    upsert_symbol(&mut s, "Primary")?;
    upsert_symbol(&mut s, "Alt")?;
    s.execute(
        "library.part.set_substitute",
        json!({"name":"Primary","kind":"second_source","manufacturer":"Acme","mpn":"111"}),
        None,
    )?;
    s.execute(
        "library.part.set_substitute",
        json!({"name":"Primary","kind":"alternate","symbol":"Alt","note":"drop-in"}),
        None,
    )?;
    let record = substitutes(&s, "Primary")?;
    assert_eq!(record.second_sources.len(), 1);
    assert_eq!(record.alternates.len(), 1);
    assert_eq!(record.alternates[0].note, "drop-in");
    s.undo()?;
    assert_eq!(substitutes(&s, "Primary")?.alternates.len(), 0);
    s.undo()?;
    assert!(!s.project.library.part_substitutes.contains_key("Primary"));
    Ok(())
}

#[test]
fn bad_substitute_is_refused_and_changes_nothing() -> Result<(), Error> {
    let mut s = Session::default();
    upsert_symbol(&mut s, "Primary")?;
    s.execute(
        "library.part.set_substitute",
        json!({"name":"Primary","kind":"second_source","manufacturer":"Acme","mpn":"1"}),
        None,
    )?;
    let before = substitutes(&s, "Primary")?;
    for bad in [
        json!({"name":"Primary","kind":"second_source","manufacturer":"x".repeat(300),"mpn":"1"}),
        json!({"name":"Primary","kind":"alternate","symbol":"Ghost"}),
        json!({"name":"Primary","kind":"alternate","symbol":"Primary"}),
        json!({"name":"Missing","kind":"second_source","manufacturer":"A","mpn":"1"}),
    ] {
        assert!(
            s.execute("library.part.set_substitute", bad, None).is_err(),
            "bad params must be refused"
        );
    }
    assert_eq!(substitutes(&s, "Primary")?, before);
    Ok(())
}

#[test]
fn footprint_rename_updates_alternate_footprint_override() -> Result<(), Error> {
    let mut s = Session::default();
    upsert_symbol(&mut s, "Primary")?;
    upsert_symbol(&mut s, "Alt")?;
    s.execute(
        "library.footprint.upsert",
        json!({
            "definition": {
                "name": "FP_A",
                "description": "",
                "keywords": "",
                "pads": [],
                "graphics": [],
                "raw_source": "",
                "unsupported": []
            }
        }),
        None,
    )?;
    s.execute(
        "library.part.set_substitute",
        json!({"name":"Primary","kind":"alternate","symbol":"Alt","footprint":"FP_A"}),
        None,
    )?;
    s.execute(
        "library.footprint.rename",
        json!({"name":"FP_A","new_name":"FP_C"}),
        None,
    )?;
    let alt = substitutes(&s, "Primary")?.alternates[0].footprint.clone();
    assert_eq!(alt, "FP_C");
    s.execute(
        "library.footprint.remove",
        json!({"name":"FP_C"}),
        None,
    )?;
    assert!(substitutes(&s, "Primary")?.alternates[0].footprint.is_empty());
    s.project.validate()?;
    Ok(())
}

#[test]
fn remove_second_source_and_alternate() -> Result<(), Error> {
    let mut s = Session::default();
    upsert_symbol(&mut s, "Primary")?;
    upsert_symbol(&mut s, "Alt")?;
    s.execute(
        "library.part.set_substitute",
        json!({"name":"Primary","kind":"second_source","manufacturer":"Acme","mpn":"1"}),
        None,
    )?;
    s.execute(
        "library.part.set_substitute",
        json!({"name":"Primary","kind":"alternate","symbol":"Alt"}),
        None,
    )?;
    s.execute(
        "library.part.set_substitute",
        json!({"name":"Primary","kind":"second_source","manufacturer":"Acme","mpn":null}),
        None,
    )?;
    assert_eq!(substitutes(&s, "Primary")?.second_sources.len(), 0);
    s.execute(
        "library.part.set_substitute",
        json!({"name":"Primary","kind":"alternate","symbol":"Alt","clear":true}),
        None,
    )?;
    assert!(!s.project.library.part_substitutes.contains_key("Primary"));
    Ok(())
}
