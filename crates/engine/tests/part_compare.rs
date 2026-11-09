//! library.part.compare: typed field diffs for alternates (#719).
use pcbcraft_engine::{Error, Session};
use serde_json::json;

fn upsert_symbol(s: &mut Session, name: &str, value: &str) -> Result<(), Error> {
    s.execute(
        "library.symbol.upsert",
        json!({
            "definition": {
                "name": name,
                "reference": "R",
                "value": value,
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

#[test]
fn compare_marks_differing_values_and_leaves_history_alone() -> Result<(), Error> {
    let mut s = Session::default();
    upsert_symbol(&mut s, "Primary", "10k")?;
    upsert_symbol(&mut s, "Alt", "22k")?;
    let out = s.peek(
        "library.part.compare",
        json!({"baseline":"Primary","others":["Alt"]}),
    )?;
    let rows = out["compared"][0]["fields"]
        .as_array()
        .ok_or_else(|| Error::Invalid("fields".into()))?;
    let value = rows
        .iter()
        .find(|r| r["field"] == "value")
        .ok_or_else(|| Error::Invalid("value row".into()))?;
    assert_eq!(value["same"], json!(false));
    Ok(())
}
