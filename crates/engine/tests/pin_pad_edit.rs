//! library.pin_pad_assign and library.pin_pad_role: a person fixes which pad a pin drives (#759).
use pcbcraft_engine::{Error, Session};
use serde_json::{Value, json};

fn pad(number: &str, x: f64) -> Value {
    json!({
        "number": number,
        "pad_type": "smd",
        "shape": "rect",
        "position": {"x": x, "y": 0.0},
        "rotation": 0.0,
        "size": {"x": 1.0, "y": 0.6},
        "layers": ["F.Cu", "F.Paste", "F.Mask"],
        "drill": null,
        "roundrect_ratio": null,
        "raw_source": ""
    })
}

fn pin(number: &str) -> Value {
    json!({
        "number": number,
        "name": "~",
        "position": {"x": 0.0, "y": 0.0},
        "angle": 0.0,
        "length": 2.54,
        "electrical_type": "passive",
        "unit": 1,
        "body_style": 1,
        "graphical_style": "line",
        "visible": true
    })
}

/// A part named `Duo` with pins 1 and 2 and a footprint `Duo_FP` with pads 1 and 2.
fn session_with_part() -> Result<Session, Error> {
    let mut s = Session::default();
    s.execute(
        "library.footprint.upsert",
        json!({"definition": {
            "name": "Duo_FP",
            "description": "",
            "keywords": "",
            "pads": [pad("1", -1.0), pad("2", 1.0)],
            "graphics": [],
            "raw_source": "",
            "unsupported": [],
            "retained": []
        }}),
        None,
    )?;
    s.execute(
        "library.symbol.upsert",
        json!({"definition": {
            "name": "Duo",
            "reference": "U",
            "value": "Duo",
            "description": "",
            "keywords": "",
            "footprint": "Duo_FP",
            "pins": [pin("1"), pin("2")],
            "graphics": [],
            "raw_source": "",
            "unsupported": []
        }}),
        None,
    )?;
    Ok(s)
}

fn codes(reply: &Value) -> Vec<String> {
    reply["result"]["findings"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|f| f["code"].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

fn rows(s: &Session) -> Vec<(String, Vec<String>)> {
    s.project
        .library
        .part_pin_maps
        .get("Duo")
        .map(|m| {
            m.rows
                .iter()
                .map(|r| (r.pin.clone(), r.pads.clone()))
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn assigning_a_pad_moves_it_to_the_pin_and_undo_restores_the_map() -> Result<(), Error> {
    let mut s = session_with_part()?;
    let reply = s.execute(
        "library.pin_pad_assign",
        json!({"name": "Duo", "pin": "1", "pad": "2"}),
        None,
    )?;
    assert_eq!(
        rows(&s),
        vec![
            ("1".to_string(), vec!["1".to_string(), "2".to_string()]),
            ("2".to_string(), vec![])
        ],
        "pad 2 left pin 2 for pin 1, and pin 1 still keeps its own pad"
    );
    assert!(
        codes(&reply).contains(&"pin_unmapped".to_string()),
        "pin 2 now has no pad: {reply}"
    );
    s.undo()?;
    assert!(
        s.project.library.part_pin_maps.is_empty(),
        "one undo removes the stored map"
    );
    Ok(())
}

#[test]
fn giving_a_pad_to_a_second_pin_is_reported_and_reassigning_clears_it() -> Result<(), Error> {
    let mut s = session_with_part()?;
    let reply = s.execute(
        "library.pin_pad_assign",
        json!({"name": "Duo", "pin": "2", "pad": "1", "only": true}),
        None,
    )?;
    let found = codes(&reply);
    assert!(
        found.contains(&"pad_duplicate".to_string()),
        "pad 1 is driven by two pins: {reply}"
    );
    assert!(
        found.contains(&"pad_unclaimed".to_string()),
        "pad 2 is now driven by no pin: {reply}"
    );
    let reply = s.execute(
        "library.pin_pad_assign",
        json!({"name": "Duo", "pin": "2", "pad": "2", "only": true}),
        None,
    )?;
    assert!(codes(&reply).is_empty(), "the findings clear: {reply}");
    Ok(())
}

#[test]
fn an_unknown_pin_pad_part_or_role_is_refused_and_writes_nothing() -> Result<(), Error> {
    let mut s = session_with_part()?;
    let undo_before = s.inspect()["history"]["undo"].as_u64();
    let refused = [
        (
            "library.pin_pad_assign",
            json!({"name": "Duo", "pin": "9", "pad": "1"}),
            "has no pin 9",
        ),
        (
            "library.pin_pad_assign",
            json!({"name": "Duo", "pin": "1", "pad": "9"}),
            "has no pad 9",
        ),
        (
            "library.pin_pad_assign",
            json!({"name": "Nope", "pin": "1", "pad": "1"}),
            "no project-library symbol named Nope",
        ),
        (
            "library.pin_pad_role",
            json!({"name": "Duo", "pad": "9", "role": "thermal"}),
            "has no pad 9",
        ),
        (
            "library.pin_pad_role",
            json!({"name": "Duo", "pad": "1", "role": "shiny"}),
            "the role must be",
        ),
    ];
    for (command, params, words) in refused {
        let outcome = s.execute(command, params, None);
        let message = match outcome {
            Err(e) => e.to_string(),
            Ok(v) => return Err(Error::Invalid(format!("{command} was accepted: {v}"))),
        };
        assert!(message.contains(words), "{command} said: {message}");
    }
    assert!(
        s.project.library.part_pin_maps.is_empty(),
        "a refusal stores nothing"
    );
    assert_eq!(
        s.inspect()["history"]["undo"].as_u64(),
        undo_before,
        "a refusal adds no undo step"
    );
    Ok(())
}

#[test]
fn a_pad_role_change_removes_the_unclaimed_finding_and_undo_restores_it() -> Result<(), Error> {
    let mut s = session_with_part()?;
    s.execute(
        "library.pin_pad_assign",
        json!({"name": "Duo", "pin": "2", "pad": "1", "only": true}),
        None,
    )?;
    let reply = s.execute(
        "library.pin_pad_role",
        json!({"name": "Duo", "pad": "2", "role": "thermal"}),
        None,
    )?;
    assert!(
        !codes(&reply).contains(&"pad_unclaimed".to_string()),
        "a thermal pad needs no pin: {reply}"
    );
    let stored = s
        .project
        .library
        .part_pin_maps
        .get("Duo")
        .map(|m| m.pad_role("2"));
    assert_eq!(stored, Some(pcbcraft_library::PadRole::Thermal));
    s.undo()?;
    let stored = s
        .project
        .library
        .part_pin_maps
        .get("Duo")
        .map(|m| m.pad_role("2"));
    assert_eq!(
        stored,
        Some(pcbcraft_library::PadRole::Electrical),
        "undo puts the role back"
    );
    Ok(())
}
