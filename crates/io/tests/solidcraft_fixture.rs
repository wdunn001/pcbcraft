//! Golden SolidCraft export file checks the writer's JSON subset without re-running export.
//!
//! # Dependencies
//! - crates/io/tests/fixtures/solidcraft/board_two_parts.solidcraft
use serde_json::Value;

#[test]
fn fixture_has_the_board_and_one_component_per_part() -> Result<(), String> {
    let text = include_str!("fixtures/solidcraft/board_two_parts.solidcraft");
    let doc: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    assert_eq!(doc["format"], "solidcraft/22");
    assert_eq!(doc["components"].as_array().map(Vec::len), Some(2));
    assert_eq!(doc["occurrences"].as_array().map(Vec::len), Some(2));
    assert_eq!(doc["metadata"]["org.pcbcraft"]["schema"], "pcbcraft-interop/1");
    assert_eq!(doc["metadata"]["org.pcbcraft"]["bodies"]["R2"]["component"], "R2");
    Ok(())
}
