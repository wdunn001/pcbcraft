use pcbcraft_engine::Session;
use serde_json::json;

fn file_names(session: &Session) -> Vec<String> {
    let inspect = session.inspect();
    let files = inspect
        .get("project")
        .and_then(|p| p.get("files"))
        .cloned()
        .unwrap_or(json!([]));
    let names = files.as_array().cloned().unwrap_or_default();
    names
        .iter()
        .filter_map(|f| {
            f.get("name")
                .and_then(|n| n.as_str())
                .map(|s| s.to_string())
        })
        .collect()
}

#[test]
fn datasheet_extract_stores_the_pin_table_with_page_three() -> Result<(), String> {
    let mut session = Session::default();
    let params = json!({
        "part": "U1",
        "document": {
            "pages": [
                {
                    "page_index": 0,
                    "page_label": 3,
                    "cells": [
                        {"text": "Pin", "page_index": 0, "page_label": 3, "rect": {"x0": 10.0, "y0": 50.0, "x1": 30.0, "y1": 60.0}},
                        {"text": "Description", "page_index": 0, "page_label": 3, "rect": {"x0": 40.0, "y0": 50.0, "x1": 120.0, "y1": 60.0}},
                        {"text": "1", "page_index": 0, "page_label": 3, "rect": {"x0": 10.0, "y0": 70.0, "x1": 20.0, "y1": 80.0}},
                        {"text": "IN+", "page_index": 0, "page_label": 3, "rect": {"x0": 30.0, "y0": 70.0, "x1": 60.0, "y1": 80.0}},
                        {"text": "2", "page_index": 0, "page_label": 3, "rect": {"x0": 10.0, "y0": 90.0, "x1": 20.0, "y1": 100.0}},
                        {"text": "IN-", "page_index": 0, "page_label": 3, "rect": {"x0": 30.0, "y0": 90.0, "x1": 60.0, "y1": 100.0}}
                    ]
                }
            ]
        }
    });
    let result = session
        .execute("datasheet.extract", params, None)
        .map_err(|e| e.to_string())?;
    assert_eq!(result["result"]["part"], "U1");
    assert_eq!(result["result"]["pins"], 2);
    let names = file_names(&session);
    assert!(
        names.contains(&"datasheet-extract-U1.json".to_string()),
        "datasheet-extract-U1.json not found in files"
    );
    Ok(())
}

#[test]
fn datasheet_extract_refuses_a_bad_part_name() {
    let mut session = Session::default();
    let params = json!({
        "part": "a b",
        "document": {
            "pages": []
        }
    });
    let result = session.execute("datasheet.extract", params, None);
    assert!(result.is_err(), "expected an error for invalid part name");
}

#[test]
fn datasheet_extract_undo_restores_the_project() -> Result<(), String> {
    let mut session = Session::default();
    let params = json!({
        "part": "U1",
        "document": {
            "pages": []
        }
    });
    session
        .execute("datasheet.extract", params, None)
        .map_err(|e| e.to_string())?;
    assert!(session.can_undo());
    session.undo().map_err(|e| e.to_string())?;
    let names = file_names(&session);
    assert!(
        !names.contains(&"datasheet-extract-U1.json".to_string()),
        "extract attachment should be gone after undo"
    );
    Ok(())
}
