use pcbcraft_engine::Session;
use pcbcraft_library::datasheet_annotations::set_store_path_for_test;
use serde_json::json;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

fn with_store(path: PathBuf) -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    let guard = LOCK
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    set_store_path_for_test(Some(path));
    guard
}

#[test]
fn profile_commands_round_trip_without_bumping_revision() -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!("pcbcraft-eng-ds-{}", std::process::id()));
    let _guard = with_store(dir.join("store.json"));
    let mut session = Session::default();
    let rev = session.revision;
    let hash = "aabbccddeeff0011";
    session
        .execute(
            "profile.datasheet_upsert",
            json!({
                "document_hash": hash,
                "entry": {
                    "id": "b1",
                    "kind": "bookmark",
                    "page": 2,
                    "rect": {"x0": 0.1, "y0": 0.1, "x1": 0.2, "y1": 0.2},
                    "label": "Pinout"
                }
            }),
            None,
        )
        .map_err(|e| e.to_string())?;
    assert_eq!(session.revision, rev);
    let listed = session
        .execute(
            "profile.datasheet_list",
            json!({"document_hash": hash}),
            None,
        )
        .map_err(|e| e.to_string())?;
    assert_eq!(
        listed["result"]["entries"].as_array().map(|a| a.len()),
        Some(1)
    );
    let exported = session
        .execute("profile.datasheet_export", json!({}), None)
        .map_err(|e| e.to_string())?;
    set_store_path_for_test(Some(dir.join("store2.json")));
    session
        .execute(
            "profile.datasheet_import",
            json!({
                "mode": "replace_all",
                "payload": exported["result"]["profile"]
            }),
            None,
        )
        .map_err(|e| e.to_string())?;
    let again = session
        .execute(
            "profile.datasheet_list",
            json!({"document_hash": hash}),
            None,
        )
        .map_err(|e| e.to_string())?;
    assert_eq!(
        again["result"]["entries"][0]["label"].as_str(),
        Some("Pinout")
    );
    Ok(())
}

#[test]
fn copy_annotation_to_field_updates_the_component() -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!("pcbcraft-eng-ds-copy-{}", std::process::id()));
    let _guard = with_store(dir.join("store.json"));
    let mut session = Session::default();
    let id = session
        .execute(
            "component.add",
            json!({
                "kind": "ic",
                "reference": "U1",
                "value": "Part",
                "position": {"x": 10, "y": 10}
            }),
            None,
        )
        .map_err(|e| e.to_string())?["result"]["id"]
        .as_u64()
        .ok_or("missing component id")?;
    let hash = "0011223344556677";
    session
        .execute(
            "profile.datasheet_upsert",
            json!({
                "document_hash": hash,
                "entry": {
                    "id": "n1",
                    "kind": "note",
                    "page": 4,
                    "rect": {"x0": 0.0, "y0": 0.0, "x1": 1.0, "y1": 0.5},
                    "note": "Supply 3.3 V only"
                }
            }),
            None,
        )
        .map_err(|e| e.to_string())?;
    let out = session
        .execute(
            "datasheet.copy_annotation_to_field",
            json!({"id": id, "document_hash": hash, "entry_id": "n1", "key": "Notes"}),
            None,
        )
        .map_err(|e| e.to_string())?;
    assert_eq!(
        out["result"]["text"].as_str(),
        Some("p4: Supply 3.3 V only")
    );
    assert_eq!(out["result"]["set"].as_bool(), Some(true));
    Ok(())
}
