use pcbcraft_library::TriangleMesh;

#[cfg(not(target_arch = "wasm32"))]
pub fn gltf_bounds(path: &str, scale: [f64; 3]) -> Result<TriangleMesh, String> {
    if path.len() > 4096 {
        return Err("model path too long".into());
    }
    let (doc, buffers, _) = gltf::import(path).map_err(|e| format!("{path}: {e}"))?;
    let mut vertices = Vec::new();
    let mut triangles = Vec::new();
    for mesh in doc.meshes() {
        for primitive in mesh.primitives() {
            let reader = primitive.reader(|buffer| Some(&buffers[buffer.index()]));
            let base = vertices.len() as u32;
            if let Some(iter) = reader.read_positions() {
                for v in iter {
                    vertices.push([
                        f64::from(v[0]) * scale[0],
                        f64::from(v[1]) * scale[1],
                        f64::from(v[2]) * scale[2],
                    ]);
                }
            }
            if let Some(iter) = reader.read_indices() {
                let mut idx: Vec<u32> = iter.map(|i| base + i as u32).collect();
                for tri in idx.chunks(3) {
                    if tri.len() == 3 {
                        triangles.push([tri[0], tri[1], tri[2]]);
                    }
                }
            }
        }
    }
    let mesh = TriangleMesh {
        vertices,
        triangles,
    };
    mesh.validate().map_err(|e| e.to_string())
}

#[cfg(target_arch = "wasm32")]
pub fn gltf_bounds(_path: &str, _scale: [f64; 3]) -> Result<TriangleMesh, String> {
    Err("glTF loading is not available on wasm builds".into())
}
