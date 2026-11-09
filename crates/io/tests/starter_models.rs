#[cfg(not(target_arch = "wasm32"))]
#[test]
fn materialize_writes_bytes_and_resolve_loads_step() -> Result<(), String> {
    use pcbcraft_io::{load_model, materialize, resolve_path};
    use pcbcraft_library::starter_models;
    use tempfile::tempdir;

    let model = starter_models::by_id("chip_resistor_0603").ok_or("missing starter")?;
    let dir = tempdir().map_err(|e| e.to_string())?;
    let path = materialize(model, dir.path())?;
    let again = materialize(model, dir.path())?;
    assert_eq!(path, again);
    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    assert_eq!(bytes, model.bytes);
    let loaded = load_model(path.to_str().ok_or("path")?, &bytes).map_err(|e| e.0)?;
    assert!(loaded.indices.len() >= 3);

    let resolved = resolve_path(
        "missing/on/disk/R_0603_1608Metric.step",
        Some("R_0603_1608Metric"),
        dir.path(),
    )?;
    assert_eq!(resolved, path);
    Ok(())
}
