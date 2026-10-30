//! On-disk cache of parsed 3D models so a restart does not re-read large files.
//! Entries are keyed by path, byte length, modification time and loader version.
use crate::model_load::{LoadedModel, LOADER_VERSION, ModelCacheKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Serialize, Deserialize)]
struct DiskEntry {
    key: ModelCacheKey,
    model: LoadedModel,
}

/// User cache directory for parsed models: `…/pcbcraft/models`, or `None`.
pub fn default_model_cache_dir_with(get: &dyn Fn(&str) -> Option<String>, os: &str) -> Option<PathBuf> {
    let absolute = |name: &str| {
        get(name)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
    };
    let base = match os {
        "windows" => absolute("LOCALAPPDATA")?,
        "macos" => absolute("HOME")?.join("Library").join("Caches"),
        _ => absolute("XDG_CACHE_HOME").or_else(|| absolute("HOME").map(|h| h.join(".cache")))?,
    };
    Some(base.join("pcbcraft").join("models"))
}

#[cfg(not(target_arch = "wasm32"))]
pub fn default_model_cache_dir() -> Option<PathBuf> {
    default_model_cache_dir_with(&|n| std::env::var(n).ok(), std::env::consts::OS)
}

fn cache_file_name(key: &ModelCacheKey) -> String {
    let mut h = Sha256::new();
    h.update(key.path.as_bytes());
    h.update(key.len.to_le_bytes());
    if let Some(n) = key.mtime_nanos {
        h.update(n.to_le_bytes());
    }
    h.update(key.loader_version.to_le_bytes());
    h.update(LOADER_VERSION.to_le_bytes());
    format!("{:x}.json", h.finalize())
}

fn cache_root(dir: Option<&Path>) -> Option<PathBuf> {
    dir.map(Path::to_path_buf)
        .or_else(default_model_cache_dir)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn read_cached_model(key: &ModelCacheKey, dir: Option<&Path>) -> Option<LoadedModel> {
    let root = cache_root(dir)?;
    let file = root.join(cache_file_name(key));
    let text = fs::read_to_string(&file).ok()?;
    let entry: DiskEntry = serde_json::from_str(&text).ok()?;
    if entry.key == *key {
        Some(entry.model)
    } else {
        let _ = fs::remove_file(&file);
        None
    }
}

#[cfg(target_arch = "wasm32")]
pub fn read_cached_model(_key: &ModelCacheKey, _dir: Option<&Path>) -> Option<LoadedModel> {
    None
}

#[cfg(not(target_arch = "wasm32"))]
pub fn write_cached_model(key: &ModelCacheKey, model: &LoadedModel, dir: Option<&Path>) -> Result<(), String> {
    let Some(root) = cache_root(dir) else {
        return Ok(());
    };
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let file = root.join(cache_file_name(key));
    let entry = DiskEntry {
        key: key.clone(),
        model: model.clone(),
    };
    let text = serde_json::to_string(&entry).map_err(|e| e.to_string())?;
    let id = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp = root.join(format!(".model-{id}.tmp"));
    {
        let mut out = fs::File::create(&tmp).map_err(|e| e.to_string())?;
        out.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
    }
    fs::rename(&tmp, &file).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(target_arch = "wasm32")]
pub fn write_cached_model(
    _key: &ModelCacheKey,
    _model: &LoadedModel,
    _dir: Option<&Path>,
) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, UNIX_EPOCH};

    type Check = Result<(), String>;

    #[test]
    fn disk_entry_round_trips_through_json() -> Check {
        let key = ModelCacheKey {
            path: "a.glb".into(),
            len: 10,
            mtime_nanos: Some(42),
            loader_version: LOADER_VERSION,
        };
        let model = LoadedModel {
            positions: vec![[1.0, 0.0, 0.0]],
            indices: vec![0, 0, 0],
            color: None,
        };
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        write_cached_model(&key, &model, Some(dir.path()))?;
        let back = read_cached_model(&key, Some(dir.path())).ok_or("miss")?;
        assert_eq!(back, model);
        Ok(())
    }

    #[test]
    fn stale_mtime_is_a_miss() -> Check {
        let t0 = UNIX_EPOCH + Duration::from_secs(1);
        let key = crate::model_load::model_cache_key("a.glb", 5, Some(t0));
        let model = LoadedModel {
            positions: vec![[0.0; 3]],
            indices: vec![0, 0, 0],
            color: None,
        };
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        write_cached_model(&key, &model, Some(dir.path()))?;
        let t1 = t0 + Duration::from_secs(1);
        let other = crate::model_load::model_cache_key("a.glb", 5, Some(t1));
        assert!(read_cached_model(&other, Some(dir.path())).is_none());
        Ok(())
    }
}
