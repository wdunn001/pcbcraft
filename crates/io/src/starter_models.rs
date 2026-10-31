//! Materialize bundled starter 3D models into the user cache and resolve
//! footprint model paths when the original file is missing.

use pcbcraft_library::starter_models::StarterModel;
use std::path::{Path, PathBuf};

/// Directory under the host cache root where starter models are written once.
pub fn cache_subdir(cache_root: &Path) -> PathBuf {
    cache_root.join("starter-models")
}

/// Write one starter model to the cache if needed and return its path.
#[cfg(not(target_arch = "wasm32"))]
pub fn materialize(model: &StarterModel, cache_root: &Path) -> Result<PathBuf, String> {
    let dir = cache_subdir(cache_root);
    std::fs::create_dir_all(&dir).map_err(|e| format!("starter model cache: {e}"))?;
    let path = dir.join(model.file_name);
    if path.is_file() {
        let on_disk = std::fs::read(&path).map_err(|e| format!("starter model cache: {e}"))?;
        if on_disk == model.bytes {
            return Ok(path);
        }
    }
    std::fs::write(&path, model.bytes).map_err(|e| format!("starter model cache: {e}"))?;
    Ok(path)
}

#[cfg(target_arch = "wasm32")]
pub fn materialize(_model: &StarterModel, _cache_root: &Path) -> Result<PathBuf, String> {
    Err("Starter 3D models materialize in the desktop app.".into())
}

/// If `path` exists on disk, return it. Otherwise try a bundled starter model.
#[cfg(not(target_arch = "wasm32"))]
pub fn resolve_path(
    path: &str,
    footprint_name: Option<&str>,
    cache_root: &Path,
) -> Result<PathBuf, String> {
    let candidate = Path::new(path);
    if candidate.is_file() {
        return Ok(candidate.to_path_buf());
    }
    let model = footprint_name
        .and_then(pcbcraft_library::starter_models::for_footprint)
        .or_else(|| pcbcraft_library::starter_models::for_model_path(path));
    let Some(model) = model else {
        return Err(format!(
            "The model file {} was not found.",
            candidate
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(path)
        ));
    };
    materialize(model, cache_root)
}

#[cfg(target_arch = "wasm32")]
pub fn resolve_path(path: &str, _footprint_name: Option<&str>, _cache_root: &Path) -> Result<PathBuf, String> {
    let _ = path;
    Err("Starter 3D models resolve in the desktop app.".into())
}
