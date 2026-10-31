use pcbcraft_engine::library::read_tool;
use serde_json::json;
use std::path::PathBuf;

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../library/tests/fixtures/static-registry")
}

#[test]
fn registry_read_tools_merge_instance_feed() -> Result<(), String> {
    let root = fixture_root();
    let path = root.to_string_lossy();
    let verify = read_tool("registry.verify", &json!({"path": path.as_ref()}))?;
    assert_eq!(verify["ok"], true);
    let items = read_tool(
        "registry.instance_items",
        &json!({
            "path": path.as_ref(),
            "instance_feed": root.join("instances/sample-instances.json").to_string_lossy()
        }),
    )?;
    assert_eq!(items["items"].as_array().map(Vec::len), Some(2));
    Ok(())
}
