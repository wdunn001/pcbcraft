//! A newcomer opens an interchange schematic and sees its parts and nets.
use pcbcraft_engine::Session;
use serde_json::json;

#[test]
fn a_newcomer_opens_a_schematic_and_sees_components_and_nets() -> Result<(), String> {
    let mut session = Session::default();
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../io/tests/fixtures/schematic_import_flat.kicad_sch");
    let path = fixture.to_string_lossy().to_string();
    let result = session
        .execute(
            "project.import",
            json!({"path": path, "format": "schematic"}),
            None,
        )
        .map_err(|e| format!("import failed: {e}"))?;
    let components = result["result"]["components"]
        .as_u64()
        .ok_or("no components field")?;
    let nets = result["result"]["nets"].as_u64().ok_or("no nets field")?;
    assert!(
        components >= 2,
        "the flat fixture has two components, got {components}"
    );
    assert!(
        nets >= 1,
        "the flat fixture has at least one net, got {nets}"
    );
    let inspect = session.inspect();
    assert!(
        inspect["project"]["name"].as_str().is_some(),
        "project has a name"
    );
    Ok(())
}
