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
fn load_bundle(session: &Session) -> Result<serde_json::Value, String> {
    let inspect = session.inspect();
    let files = inspect
        .get("project")
        .and_then(|p| p.get("files"))
        .cloned()
        .unwrap_or(json!([]));
    for f in files.as_array().cloned().unwrap_or_default() {
        if f.get("name").and_then(|n| n.as_str()) == Some("datasheet-extract-U1.json") {
            let data = f.get("data").and_then(|d| d.as_str()).unwrap_or("");
            return serde_json::from_str(data).map_err(|e| e.to_string());
        }
    }
    Err("bundle not found".to_string())
}

#[test]
fn datasheet_review_records_each_decision() -> Result<(), String> {
    let mut session = Session::default();
    // Extract with two pin rows (IN+ and IN-)
    let extract_params = json!({
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
    session
        .execute("datasheet.extract", extract_params, None)
        .map_err(|e| e.to_string())?;

    // Reject pin 0
    let reject_params = json!({
        "part": "U1",
        "table": "pin_table",
        "index": 0,
        "decision": "reject"
    });
    session
        .execute("datasheet.review", reject_params, None)
        .map_err(|e| e.to_string())?;

    // Accept pin 1
    let accept_params = json!({
        "part": "U1",
        "table": "pin_table",
        "index": 1,
        "decision": "accept"
    });
    session
        .execute("datasheet.review", accept_params, None)
        .map_err(|e| e.to_string())?;

    // Edit pin 1: change pin_name to VIN
    let edit_params = json!({
        "part": "U1",
        "table": "pin_table",
        "index": 1,
        "decision": "edit",
        "edited": {
            "pin_number": "2",
            "pin_name": "VIN",
            "source": {
                "page_index": 0,
                "page_label": 99,
                "rect": {"x0": 0.0, "y0": 0.0, "x1": 1.0, "y1": 1.0},
                "quote": "dummy"
            }
        }
    });
    session
        .execute("datasheet.review", edit_params, None)
        .map_err(|e| e.to_string())?;

    // Reload and verify
    let bundle = load_bundle(&session)?;
    let pin_table = bundle["pin_table"]
        .as_array()
        .ok_or("the bundle has no pin table list")?;

    // Pin 0 should be rejected
    assert_eq!(pin_table[0]["decision"], "rejected");

    // Pin 1 should be edited
    assert_eq!(pin_table[1]["decision"], "edited");
    assert_eq!(pin_table[1]["edited"]["pin_name"], "VIN");
    // Source should still be from original extraction (page 3)
    assert_eq!(pin_table[1]["edited"]["source"]["page_label"], 3);

    Ok(())
}

#[test]
fn datasheet_review_refuses_edit_without_a_value() -> Result<(), String> {
    let mut session = Session::default();
    let extract_params = json!({
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
                        {"text": "IN+", "page_index": 0, "page_label": 3, "rect": {"x0": 30.0, "y0": 70.0, "x1": 60.0, "y1": 80.0}}
                    ]
                }
            ]
        }
    });
    session
        .execute("datasheet.extract", extract_params, None)
        .map_err(|e| format!("extract should succeed: {e}"))?;

    // Try to edit without providing an edited value
    let edit_params = json!({
        "part": "U1",
        "table": "pin_table",
        "index": 0,
        "decision": "edit"
    });
    let result = session.execute("datasheet.review", edit_params, None);
    let err = result
        .err()
        .ok_or("edit without value should fail")?
        .to_string();
    assert!(
        err.contains("edited value required"),
        "error should mention edited value: {}",
        err
    );
    Ok(())
}

#[test]
fn datasheet_review_refuses_index_out_of_range() -> Result<(), String> {
    let mut session = Session::default();
    let extract_params = json!({
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
                        {"text": "IN+", "page_index": 0, "page_label": 3, "rect": {"x0": 30.0, "y0": 70.0, "x1": 60.0, "y1": 80.0}}
                    ]
                }
            ]
        }
    });
    session
        .execute("datasheet.extract", extract_params, None)
        .map_err(|e| format!("extract should succeed: {e}"))?;

    // Try to access index 10 (only 1 pin exists)
    let review_params = json!({
        "part": "U1",
        "table": "pin_table",
        "index": 10,
        "decision": "accept"
    });
    let result = session.execute("datasheet.review", review_params, None);
    let err = result
        .err()
        .ok_or("out of range index should fail")?
        .to_string();
    assert!(
        err.contains("out of range"),
        "error should mention out of range: {}",
        err
    );
    Ok(())
}
