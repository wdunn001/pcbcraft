//! Loads 3D model files off the UI thread and remembers the results for the
//! 3D View tab. One worker thread reads and parses; the frame only asks and
//! never waits. The in-memory key is path, byte length, modification time and
//! the loader version; converted models are also stored under the user cache
//! directory keyed by source hash, loader version and STEP tolerance. On the
//! browser build there are no threads and no files: every lookup says so.

use pcbcraft_io::{ImportCache, ModelCacheKey, ModelLookup};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

type JobSender = std::sync::mpsc::Sender<(ModelCacheKey, String)>;

enum Entry {
    Loading,
    Ready(pcbcraft_io::SharedModel),
    Failed(String),
}

#[derive(Default)]
struct Shared {
    entries: Mutex<HashMap<ModelCacheKey, Entry>>,
    /// Counts finished loads, so the view knows when to rebuild its mesh.
    generation: AtomicU64,
}

/// Handle to the shared cache. Cloning shares the same results.
///
/// # Rules
/// `lookup` never blocks on a file read or a parse.
/// # Dependencies
/// `crates/io/src/model_load.rs` for the reader and the cache key.
/// # Do not
/// Do not store loaded models in the project document here.
#[derive(Clone)]
pub(super) struct ModelCache {
    shared: Arc<Shared>,
    disk: ImportCache,
    #[cfg(not(target_arch = "wasm32"))]
    jobs: Arc<Mutex<Option<JobSender>>>,
}

impl Default for ModelCache {
    fn default() -> Self {
        Self {
            shared: Arc::new(Shared::default()),
            disk: ImportCache::default(),
            #[cfg(not(target_arch = "wasm32"))]
            jobs: Arc::new(Mutex::new(None)),
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
    /// Number of finished loads so far.
    pub(super) fn generation(&self) -> u64 {
        self.shared.generation.load(Ordering::Acquire)
    }
    /// How many files are still being read.
    pub(super) fn pending(&self) -> usize {
        self.shared.entries.lock().map_or(0, |e| {
            e.values().filter(|v| matches!(v, Entry::Loading)).count()
        })
    }

    /// The state of the model file at `path`, starting a load when new.
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn lookup(&self, path: &str) -> ModelLookup {
        let name = std::path::Path::new(path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(path);
        if !pcbcraft_io::model_extension_supported(path)
            && let Err(e) = pcbcraft_io::load_model(path, &[])
        {
            return ModelLookup::Failed(e.0);
        }
        let Ok(meta) = std::fs::metadata(path) else {
            return ModelLookup::Failed(format!("The model file {name} was not found."));
        };
        let key = pcbcraft_io::model_cache_key(path, meta.len(), meta.modified().ok());
        let Ok(mut entries) = self.shared.entries.lock() else {
            return ModelLookup::Failed("The model cache is unavailable.".into());
        };
        match entries.get(&key) {
            Some(Entry::Ready(m)) => return ModelLookup::Ready(m.clone()),
            Some(Entry::Failed(r)) => return ModelLookup::Failed(r.clone()),
            Some(Entry::Loading) => return ModelLookup::Loading,
            None => {}
        }
        // An older version of the same file is stale now.
        entries.retain(|k, _| k.path != key.path);
        entries.insert(key.clone(), Entry::Loading);
        drop(entries);
        if self.send(key.clone(), path.to_string()) {
            ModelLookup::Loading
        } else {
            self.finish(&key, Entry::Failed("Model loading could not start.".into()));
            ModelLookup::Failed("Model loading could not start.".into())
        }
    }
    /// The browser build has no files and no threads; the box is kept.
    #[cfg(target_arch = "wasm32")]
    pub(super) fn lookup(&self, _path: &str) -> ModelLookup {
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
    fn send(&self, key: ModelCacheKey, path: String) -> bool {
        let Ok(mut slot) = self.jobs.lock() else {
            return false;
        };
        if slot.is_none() {
            let (tx, rx) = std::sync::mpsc::channel::<(ModelCacheKey, String)>();
            let cache = self.clone();
            let spawned = std::thread::Builder::new()
                .name("model-loader".into())
                .spawn(move || {
                    while let Ok((key, path)) = rx.recv() {
                        let entry = match std::fs::read(&path) {
                            Err(_) => Entry::Failed("The model file could not be read.".into()),
                            Ok(bytes) => match cache.disk.load_or_convert(&path, &bytes) {
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
        slot.as_ref().is_some_and(|tx| tx.send((key, path)).is_ok())
    }
}
