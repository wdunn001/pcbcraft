use pcbcraft_engine::Session;
use serde_json::json;

#[test]
fn a_newcomer_searches_the_glossary_for_a_term() -> Result<(), String> {
    let mut session = Session::default();
    let out = session
        .execute(
            "help.glossary",
            json!({"query": "thicker track", "limit": 40}),
            None,
        )
        .map_err(|e| e.to_string())?;
    let hits = out["result"]["hits"].as_array().ok_or("hits array")?;
    let ids: Vec<&str> = hits.iter().filter_map(|h| h["id"].as_str()).collect();
    assert!(
        ids.contains(&"track_width"),
        "track_width appears in results"
    );
    assert!(
        ids.contains(&"track.set_width"),
        "the related command appears in results"
    );
    Ok(())
}

#[test]
fn a_newcomer_looks_up_one_term_by_id() -> Result<(), String> {
    let mut session = Session::default();
    let out = session
        .execute("help.term", json!({"id": "voltage"}), None)
        .map_err(|e| e.to_string())?;
    assert_eq!(
        out["result"]["id"].as_str(),
        Some("voltage"),
        "the term id is returned"
    );
    assert!(
        out["result"]["short"].as_str().is_some(),
        "the short definition is returned"
    );
    Ok(())
}

#[test]
fn a_newcomer_gets_an_error_for_an_unknown_term_id() -> Result<(), String> {
    let mut session = Session::default();
    let err = session
        .execute("help.term", json!({"id": "not_a_real_term_id"}), None)
        .map_err(|e| e.to_string());
    assert!(
        err.is_err_and(|e| e.contains("not_a_real_term_id")),
        "the error names the unknown id"
    );
    Ok(())
}
