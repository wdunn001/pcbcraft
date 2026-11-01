use pcbcraft_library::TriangleMesh;

#[cfg(not(target_arch = "wasm32"))]
pub fn gltf_bounds(path: &str, scale: [f64; 3]) -> Result<TriangleMesh, String> {
    if path.len() > 4096 {
        return Err("model path too long".into());
    }
    let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    let model = crate::model_load::load_model(path, &bytes).map_err(|e| format!("{path}: {e}"))?;
    let vertices: Vec<[f64; 3]> = model
        .positions
        .iter()
        .map(|v| [v[0] * scale[0], v[1] * scale[1], v[2] * scale[2]])
        .collect();
    let triangles: Vec<[u32; 3]> = model
        .indices
        .as_chunks::<3>().0.iter()
        .map(|tri| [tri[0], tri[1], tri[2]])
        .collect();
    let mesh = TriangleMesh {
        vertices,
        triangles,
    };
    mesh.validate().map_err(|e| e.to_string())?;
    Ok(mesh)
}
