//! On-disk cache of converted 3D models under the user cache directory.
use crate::model_compact::{
    cache_entry_name, decode_compact, encode_compact, source_digest,
};
use crate::model_load::{
    load_model, LoadedModel, LOADER_VERSION, ModelError, MAX_MODEL_BYTES,
};
use crate::step_read::STEP_TOLERANCE_MM;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn bad(why: &str) -> ModelError {
    ModelError(why.to_string())
}

fn step_tolerance_for_path(path: &str) -> f64 {
    let lower = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    if matches!(lower.as_str(), "step" | "stp") {
        STEP_TOLERANCE_MM
    } else {
        0.0
    }
}

/// User cache folder for converted models: `pcbcraft/models`.
pub fn default_import_cache_dir_with(get: &dyn Fn(&str) -> Option<String>, os: &str) -> Option<PathBuf> {
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

/// [`default_import_cache_dir_with`] over the real environment.
pub fn default_import_cache_dir() -> Option<PathBuf> {
    default_import_cache_dir_with(&|n| std::env::var(n).ok(), std::env::consts::OS)
}

/// Disk cache of compact converted models keyed by source hash, loader version and tolerance.
#[derive(Clone)]
pub struct ImportCache {
    root: Option<PathBuf>,
}

impl Default for ImportCache {
    fn default() -> Self {
        Self {
            root: default_import_cache_dir(),
        }
    }
}

impl ImportCache {
    /// Cache rooted at `root`; the folder is created on first write.
    pub fn open(root: PathBuf) -> Self {
        Self { root: Some(root) }
    }

    /// A cache that never touches the disk (tests and wasm).
    pub fn in_memory() -> Self {
        Self { root: None }
    }

    fn entry_path(&self, path: &str, source: &[u8]) -> Option<PathBuf> {
        let root = self.root.as_ref()?;
        let digest = source_digest(source);
        let name = cache_entry_name(&digest, LOADER_VERSION, step_tolerance_for_path(path));
        let shard = digest.get(..2).unwrap_or("00");
        Some(root.join(shard).join(name))
    }

    /// Read a cached compact model when present and valid.
    pub fn get(&self, path: &str, source: &[u8]) -> Option<LoadedModel> {
        let file = self.entry_path(path, source)?;
        let bytes = fs::read(&file).ok()?;
        decode_compact(&bytes).ok()
    }

    /// Convert source bytes to a model, using the disk cache when possible.
    pub fn load_or_convert(&self, path: &str, source: &[u8]) -> Result<LoadedModel, ModelError> {
        if source.len() > MAX_MODEL_BYTES {
            return Err(bad("the model file is over the size limit"));
        }
        if let Some(m) = self.get(path, source) {
            return Ok(m);
        }
        let model = load_model(path, source)?;
        if let Some(file) = self.entry_path(path, source) {
            let digest = source_digest(source);
            let compact = encode_compact(
                &model,
                LOADER_VERSION,
                step_tolerance_for_path(path),
                &digest,
            )?;
            if let Some(parent) = file.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let _ = write_atomic(&file, &compact);
        }
        Ok(model)
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let n = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp = path.with_file_name(format!(".tmp-{}-{n}", std::process::id()));
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all().ok();
    }
    fs::rename(&tmp, path).inspect_err(|_| {
        let _ = fs::remove_file(&tmp);
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_load::load_model;

    #[test]
    fn second_load_hits_the_disk_cache() -> Result<(), String> {
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let cache = ImportCache::open(dir.path().to_path_buf());
        let source = include_bytes!("../tests/fixtures/box.step");
        let first = cache
            .load_or_convert("box.step", source)
            .map_err(|e| e.0)?;
        let digest = source_digest(source);
        let name = cache_entry_name(&digest, LOADER_VERSION, STEP_TOLERANCE_MM);
        let path = dir
            .path()
            .join(&digest[..2])
            .join(name);
        assert!(path.is_file(), "the cache file should exist");
        let mtime = path.metadata().map_err(|e| e.to_string())?.modified().ok();
        std::thread::sleep(std::time::Duration::from_millis(20));
        let second = cache
            .load_or_convert("box.step", source)
            .map_err(|e| e.0)?;
        assert_eq!(first, second);
        let mtime2 = path.metadata().map_err(|e| e.to_string())?.modified().ok();
        assert_eq!(mtime, mtime2, "a cache hit must not rewrite the file");
        Ok(())
    }

    #[test]
    fn cache_misses_when_source_bytes_change() -> Result<(), String> {
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let cache = ImportCache::open(dir.path().to_path_buf());
        let source = include_bytes!("../tests/fixtures/box.step");
        cache
            .load_or_convert("box.step", source)
            .map_err(|e| e.0)?;
        let mut changed = source.to_vec();
        changed.push(b'\n');
        let again = cache
            .load_or_convert("box.step", &changed)
            .map_err(|e| e.0)?;
        let direct = load_model("box.step", &changed).map_err(|e| e.0)?;
        assert_eq!(again.indices.len(), direct.indices.len());
        Ok(())
    }
}
