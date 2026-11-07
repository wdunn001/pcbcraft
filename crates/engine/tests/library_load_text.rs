//! library.load takes the file text as well as a path, so the browser build can load a picked file.
use pcbcraft_engine::{Error, Session};
use serde_json::{Value, json};

fn collection_text() -> Result<String, Error> {
    let mut s = Session::default();
    s.execute(
        "library.symbol.upsert",
        json!({"definition": {
            "name": "LoadTextSymbol", "reference": "R", "value": "R", "description": "",
            "keywords": "", "footprint": "",
            "pins": [{"number": "1", "name": "~", "position": {"x": 0.0, "y": 0.0}, "angle": 0.0,
                      "length": 2.54, "electrical_type": "passive", "unit": 1, "body_style": 1,
                      "graphical_style": "line", "visible": true}],
            "graphics": [], "raw_source": "", "unsupported": []
        }}),
        None,
    )?;
    let library =
        serde_json::to_value(&s.project.library).map_err(|e| Error::Invalid(e.to_string()))?;
    Ok(json!({"format": "pcbcraft-library", "version": 1, "library": library}).to_string())
}

#[test]
fn a_collection_loads_from_its_text() -> Result<(), Error> {
    let text = collection_text()?;
    let mut s = Session::default();
    let answer = s.execute("library.load", json!({"text": text}), None)?;
    let symbols = answer.pointer("/result/symbols").and_then(Value::as_u64);
    assert_eq!(symbols, Some(1), "one symbol came in: {answer}");
    assert_eq!(s.project.library.symbols.len(), 1);
    Ok(())
}

#[test]
fn text_that_is_not_a_collection_is_refused_and_changes_nothing() -> Result<(), Error> {
    let mut s = Session::default();
    let refused = s.execute("library.load", json!({"text": "not a collection"}), None);
    assert!(refused.is_err(), "garbage text is refused");
    assert!(
        s.project.library.symbols.is_empty(),
        "the project is untouched"
    );
    Ok(())
}

#[test]
fn a_load_with_neither_path_nor_text_says_what_it_needs() {
    let mut s = Session::default();
    let refused = s.execute("library.load", json!({}), None);
    assert!(
        refused.is_err_and(|e| e.to_string().contains("path of a collection file")),
        "the refusal names what to give"
    );
}
