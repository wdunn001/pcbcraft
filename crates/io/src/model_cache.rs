//! Loads 3D model files off the UI thread and remembers results in memory,
//! persisting parsed geometry through the shared import cache on disk.

use crate::board_mesh::ModelLookup;
use crate::import_cache::ImportCache;
use crate::model_load::{
    ModelCacheKey, load_model, model_cache_key_with_part, model_extension_supported,
};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

type JobSender = std::sync::mpsc::Sender<(ModelCacheKey, String, Option<String>)>;

enum Entry {
    Loading,
    Ready(std::sync::Arc<crate::model_load::LoadedModel>),
    Failed(String),
}

#[derive(Default)]
struct Shared {
    entries: Mutex<HashMap<ModelCacheKey, Entry>>,
    generation: AtomicU64,
    /// Paths already copied to the app status line.
    reported_failures: Mutex<HashSet<String>>,
    /// Failures waiting for the UI to read.
    pending_failures: Mutex<Vec<String>>,
}

/// Handle to the shared cache. Cloning shares the same results.
#[derive(Clone)]
pub struct ModelCache {
    shared: Arc<Shared>,
    #[cfg(not(target_arch = "wasm32"))]
    jobs: Arc<Mutex<Option<JobSender>>>,
    #[cfg(not(target_arch = "wasm32"))]
    import_cache: ImportCache,
}

impl Default for ModelCache {
    fn default() -> Self {
        Self {
            shared: Arc::new(Shared::default()),
            #[cfg(not(target_arch = "wasm32"))]
            jobs: Arc::new(Mutex::new(None)),
            #[cfg(not(target_arch = "wasm32"))]
            import_cache: ImportCache::in_memory(),
        }
    }
}

impl std::fmt::Debug for ModelCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModelCache")
            .field("pending", &self.pending())
            .finish()
    }
}

impl ModelCache {
    /// A cache that keeps parsed models in memory only and never touches the disk.
    ///
    /// Used where no user folder is known, such as a test or a window whose
    /// session carries no settings folder.
    pub fn memory_only() -> Self {
        Self {
            shared: Arc::new(Shared::default()),
            #[cfg(not(target_arch = "wasm32"))]
            jobs: Arc::new(Mutex::new(None)),
            #[cfg(not(target_arch = "wasm32"))]
            import_cache: ImportCache::in_memory(),
        }
    }

    /// Optional override for the on-disk import cache folder.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn with_disk_dir(dir: Option<std::path::PathBuf>) -> Self {
        let import_cache = match dir {
            Some(root) => ImportCache::open(root),
            None => ImportCache::in_memory(),
        };
        Self {
            shared: Arc::new(Shared::default()),
            jobs: Arc::new(Mutex::new(None)),
            import_cache,
        }
    }

    pub fn generation(&self) -> u64 {
        self.shared.generation.load(Ordering::Acquire)
    }

    pub fn pending(&self) -> usize {
        self.shared.entries.lock().map_or(0, |e| {
            e.values().filter(|v| matches!(v, Entry::Loading)).count()
        })
    }

    /// Status-line sentences for model files that failed since the last drain.
    pub fn drain_failure_messages(&self) -> Vec<String> {
        let Ok(mut pending) = self.shared.pending_failures.lock() else {
            return vec![];
        };
        std::mem::take(&mut *pending)
    }

    fn note_failure(&self, path: &str, reason: &str) {
        let Ok(mut reported) = self.shared.reported_failures.lock() else {
            return;
        };
        if !reported.insert(path.to_string()) {
            return;
        }
        drop(reported);
        let name = std::path::Path::new(path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(path);
        let line = format!("Model load failed. {name}: {reason}");
        if let Ok(mut pending) = self.shared.pending_failures.lock() {
            pending.push(line);
        }
    }

    /// Ask for one model file, or one named part inside it.
    ///
    /// # Rules
    /// - Two part names of one file are two cache entries, in memory and on disk.
    /// - An empty part name means the whole file.
    /// - Never waits: a file that is not ready answers [`ModelLookup::Loading`].
    ///
    /// # Dependencies
    /// `model_load::model_cache_key_with_part` for the key.
    ///
    /// # Do not
    /// Do not drop the part name; callers pass `ModelPlacement::cad_part`.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn lookup(&self, path: &str, cad_part: Option<&str>) -> ModelLookup {
        let cad_part = cad_part.filter(|p| !p.is_empty());
        let name = std::path::Path::new(path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(path);
        if !model_extension_supported(path)
            && let Err(e) = load_model(path, &[])
        {
            self.note_failure(path, &e.0);
            return ModelLookup::Failed(e.0);
        }
        let Ok(meta) = std::fs::metadata(path) else {
            let reason = format!("The model file {name} was not found.");
            self.note_failure(path, &reason);
            return ModelLookup::Failed(reason);
        };
        let key = model_cache_key_with_part(path, meta.len(), meta.modified().ok(), cad_part);
        let Ok(mut entries) = self.shared.entries.lock() else {
            return ModelLookup::Failed("The model cache is unavailable.".into());
        };
        match entries.get(&key) {
            Some(Entry::Ready(m)) => return ModelLookup::Ready(m.clone()),
            Some(Entry::Failed(r)) => {
                let reason = r.clone();
                drop(entries);
                self.note_failure(path, &reason);
                return ModelLookup::Failed(reason);
            }
            Some(Entry::Loading) => return ModelLookup::Loading,
            None => {}
        }
        entries.retain(|k, _| k.path != key.path || k.content_digest != key.content_digest);
        entries.insert(key.clone(), Entry::Loading);
        drop(entries);
        if self.send(key.clone(), path.to_string(), cad_part.map(str::to_owned)) {
            ModelLookup::Loading
        } else {
            let reason = "Model loading could not start.";
            self.finish(&key, Entry::Failed(reason.into()));
            self.note_failure(path, reason);
            ModelLookup::Failed(reason.into())
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub fn lookup(&self, path: &str, cad_part: Option<&str>) -> ModelLookup {
        let _ = (path, cad_part);
        ModelLookup::Failed("Model files load in the desktop app.".into())
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn finish(&self, key: &ModelCacheKey, entry: Entry) {
        if let Ok(mut e) = self.shared.entries.lock() {
            e.insert(key.clone(), entry);
        }
        self.shared.generation.fetch_add(1, Ordering::Release);
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn send(&self, key: ModelCacheKey, path: String, cad_part: Option<String>) -> bool {
        let Ok(mut slot) = self.jobs.lock() else {
            return false;
        };
        if slot.is_none() {
            let (tx, rx) = std::sync::mpsc::channel::<(ModelCacheKey, String, Option<String>)>();
            let cache = self.clone();
            let spawned = std::thread::Builder::new()
                .name("model-loader".into())
                .spawn(move || {
                    while let Ok((key, path, part)) = rx.recv() {
                        let entry = match std::fs::read(&path) {
                            Err(_) => Entry::Failed("The model file could not be read.".into()),
                            Ok(bytes) => match cache.import_cache.load_or_convert_part(
                                &path,
                                &bytes,
                                part.as_deref(),
                            ) {
                                Ok(m) => Entry::Ready(Arc::new(m)),
                                Err(e) => Entry::Failed(e.0),
                            },
                        };
                        cache.finish(&key, entry);
                    }
                });
            if spawned.is_err() {
                return false;
            }
            *slot = Some(tx);
        }
        slot.as_ref()
            .is_some_and(|tx| tx.send((key, path, cad_part)).is_ok())
    }
}
