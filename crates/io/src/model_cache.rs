//! Loads 3D model files off the UI thread and remembers results in memory and
//! on disk. One worker thread reads and parses; callers only ask and never
//! wait. Failures can be drained once per file for the status message strip.

use crate::board_mesh::ModelLookup;
use crate::model_disk_cache::{read_cached_model, write_cached_model};
use crate::model_load::{ModelCacheKey, load_model, model_cache_key, model_extension_supported};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

type JobSender = std::sync::mpsc::Sender<(ModelCacheKey, String)>;

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
    disk_dir: Option<std::path::PathBuf>,
}

impl Default for ModelCache {
    fn default() -> Self {
        Self {
            shared: Arc::new(Shared::default()),
            #[cfg(not(target_arch = "wasm32"))]
            jobs: Arc::new(Mutex::new(None)),
            #[cfg(not(target_arch = "wasm32"))]
            disk_dir: None,
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
    /// Optional override for the on-disk cache folder; `None` uses the platform default.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn with_disk_dir(dir: Option<std::path::PathBuf>) -> Self {
        Self {
            shared: Arc::new(Shared::default()),
            jobs: Arc::new(Mutex::new(None)),
            disk_dir: dir,
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
        let out = std::mem::take(&mut *pending);
        out
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

    #[cfg(not(target_arch = "wasm32"))]
    pub fn lookup(&self, path: &str) -> ModelLookup {
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
        let key = model_cache_key(path, meta.len(), meta.modified().ok());
        let Ok(mut entries) = self.shared.entries.lock() else {
            return ModelLookup::Failed("The model cache is unavailable.".into());
        };
        match entries.get(&key) {
            Some(Entry::Ready(m)) => return ModelLookup::Ready(m.clone()),
            Some(Entry::Failed(r)) => return ModelLookup::Failed(r.clone()),
            Some(Entry::Loading) => return ModelLookup::Loading,
            None => {}
        }
        if let Some(model) = read_cached_model(&key, self.disk_dir.as_deref()) {
            let shared = Arc::new(model);
            entries.insert(key.clone(), Entry::Ready(shared.clone()));
            drop(entries);
            self.shared.generation.fetch_add(1, Ordering::Release);
            return ModelLookup::Ready(shared);
        }
        entries.retain(|k, _| k.path != key.path);
        entries.insert(key.clone(), Entry::Loading);
        drop(entries);
        if self.send(key.clone(), path.to_string()) {
            ModelLookup::Loading
        } else {
            let reason = "Model loading could not start.";
            self.finish(&key, Entry::Failed(reason.into()));
            self.note_failure(path, reason);
            ModelLookup::Failed(reason.into())
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub fn lookup(&self, path: &str) -> ModelLookup {
        let _ = path;
        ModelLookup::Failed("Model files load in the desktop app.".into())
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn finish(&self, key: &ModelCacheKey, entry: Entry) {
        if let Entry::Failed(reason) = &entry {
            self.note_failure(&key.path, reason);
        }
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
            let disk = self.disk_dir.clone();
            let spawned = std::thread::Builder::new()
                .name("model-loader".into())
                .spawn(move || {
                    while let Ok((key, path)) = rx.recv() {
                        let entry = match std::fs::read(&path) {
                            Err(_) => Entry::Failed("The model file could not be read.".into()),
                            Ok(bytes) => match load_model(&path, &bytes) {
                                Ok(m) => {
                                    let _ = write_cached_model(
                                        &key,
                                        &m,
                                        disk.as_deref(),
                                    );
                                    Entry::Ready(Arc::new(m))
                                }
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
