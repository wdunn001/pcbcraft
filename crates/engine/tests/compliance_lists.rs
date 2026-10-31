use pcbcraft_engine::Session;
use serde_json::json;
use std::path::PathBuf;
use std::sync::Mutex;

fn with_lists_dir<T>(run: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    let _lock = pcbcraft_engine::compliance_lists::LISTS_TEST_LOCK
        .lock()
        .map_err(|_| "lock poisoned")?;
    let directory =
        std::env::temp_dir().join(format!("pcbcraft-compliance-lists-{}", std::process::id()));
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    *pcbcraft_engine::compliance_lists::LISTS_DIR_TEST
        .lock()
        .map_err(|_| "lock poisoned")? = Some(PathBuf::from(&directory));
    let result = run();
    let _ = std::fs::remove_dir_all(&directory);
    *pcbcraft_engine::compliance_lists::LISTS_DIR_TEST
        .lock()
        .map_err(|_| "lock poisoned")? = None;
    result
}

#[test]
fn a_newcomer_imports_a_newer_list_and_sees_what_changed() -> Result<(), String> {
    with_lists_dir(|| {
        let mut session = Session::default();
        let before = session
            .execute("compliance.lists", json!({}), None)
            .map_err(|e| e.to_string())?;
        assert!(
            before["result"]["count"].as_u64().unwrap_or(0) >= 1,
            "bundled lists are visible before import"
        );
        let newer = include_str!("../../io/tests/fixtures/compliance-lists/synthetic-demo-entities-v2.toml");
        let updated = session
            .execute(
                "compliance.lists_update",
                json!({"text": newer}),
                None,
            )
            .map_err(|e| e.to_string())?;
        let added = updated["result"]["entities_added"]
            .as_array()
            .ok_or("entities_added array")?;
        assert!(
            added.iter().any(|v| v.as_str() == Some("Synthetic Entity Delta")),
            "the update summary lists new entities"
        );
        let stale = session
            .execute(
                "compliance.list_staleness",
                json!({"list_id": "synthetic-demo-entities", "today": "2026-10-01"}),
                None,
            )
            .map_err(|e| e.to_string())?;
        assert!(
            stale["result"]["stale"].as_bool() == Some(true),
            "staleness is reported after cadence"
        );
        Ok(())
    })
}
