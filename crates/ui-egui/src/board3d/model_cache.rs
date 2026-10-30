//! Loads 3D model files off the UI thread and remembers the results for the
//! 3D View tab. One worker thread reads and parses; the frame only asks and
//! never waits. The cache key is path, byte length, modification time and the
//! loader version, so a changed file reloads and an unchanged one is served
//! from memory. Companion CAD projects key on the file's content hash instead
//! of mtime. Nothing here enters the project document. On the browser
//! build there are no threads and no files: every lookup says so.

use pcbcraft_io::{ModelCacheKey, ModelLookup, ModelPlacement};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

type JobSender = std::sync::mpsc::Sender<(ModelCacheKey, ModelJob)>;
#[derive(Clone, Debug)]
struct ModelJob {
    path: String,
    cad_part: Option<String>,
    cache_root: Option<std::path::PathBuf>,
}

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
/// Do not store loaded models in the project document or on disk here.
#[derive(Clone, Default)]
pub(super) struct ModelCache {
    shared: Arc<Shared>,
    #[cfg(not(target_arch = "wasm32"))]
    jobs: Arc<Mutex<Option<JobSender>>>,
    #[cfg(not(target_arch = "wasm32"))]
    cache_root: Arc<Mutex<Option<std::path::PathBuf>>>,
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

    /// Where companion CAD STEP sidecars may be cached for the open project.
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn set_cache_root(&self, root: Option<std::path::PathBuf>) {
        if let Ok(mut slot) = self.cache_root.lock() {
            *slot = root;
        }
    }

    /// Drop cached entries for one path so the next lookup re-reads the file.
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn invalidate_path(&self, path: &str) {
        if let Ok(mut entries) = self.shared.entries.lock() {
            entries.retain(|k, _| k.path != path && !k.path.starts_with(&format!("{path}#")));
        }
        self.shared.generation.fetch_add(1, Ordering::Release);
    }

    /// The state of the model file at `placement`, starting a load when new.
    #[cfg(not(target_arch = "wasm32"))]
    pub(super) fn lookup(&self, placement: &ModelPlacement) -> ModelLookup {
        let path = &placement.path;
        let name = std::path::Path::new(path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(path);
        if !pcbcraft_io::model_extension_supported(path)
            && let Err(e) = pcbcraft_io::load_model(path, &[])
        {
            return ModelLookup::Failed(e.0);
        }
        let cache_root = self
            .cache_root
            .lock()
            .ok()
            .and_then(|g| g.clone());
        let key = if pcbcraft_io::is_companion_project_path(path) {
            let Ok(bytes) = std::fs::read(path) else {
                return ModelLookup::Failed(format!("The model file {name} was not found."));
            };
            let hash = pcbcraft_io::content_sha256_hex(&bytes);
            let token = pcbcraft_io::companion_cache_token(
                &hash,
                placement.cad_part.as_deref(),
            );
            ModelCacheKey {
                path: format!("{path}#{token}"),
                len: bytes.len() as u64,
                mtime_nanos: None,
                loader_version: pcbcraft_io::LOADER_VERSION,
            }
        } else {
            let Ok(meta) = std::fs::metadata(path) else {
                return ModelLookup::Failed(format!("The model file {name} was not found."));
            };
            pcbcraft_io::model_cache_key(path, meta.len(), meta.modified().ok())
        };
        let Ok(mut entries) = self.shared.entries.lock() else {
            return ModelLookup::Failed("The model cache is unavailable.".into());
        };
        match entries.get(&key) {
            Some(Entry::Ready(m)) => return ModelLookup::Ready(m.clone()),
            Some(Entry::Failed(r)) => return ModelLookup::Failed(r.clone()),
            Some(Entry::Loading) => return ModelLookup::Loading,
            None => {}
        }
        entries.retain(|k, _| !k.path.starts_with(path));
        entries.insert(key.clone(), Entry::Loading);
        drop(entries);
        let job = ModelJob {
            path: path.clone(),
            cad_part: placement.cad_part.clone(),
            cache_root,
        };
        if self.send(key.clone(), job) {
            ModelLookup::Loading
        } else {
            self.finish(&key, Entry::Failed("Model loading could not start.".into()));
            ModelLookup::Failed("Model loading could not start.".into())
        }
    }
    /// The browser build has no files and no threads; the box is kept.
    #[cfg(target_arch = "wasm32")]
    pub(super) fn lookup(&self, _placement: &ModelPlacement) -> ModelLookup {
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
    fn send(&self, key: ModelCacheKey, job: ModelJob) -> bool {
        let Ok(mut slot) = self.jobs.lock() else {
            return false;
        };
        if slot.is_none() {
            let (tx, rx) = std::sync::mpsc::channel::<(ModelCacheKey, ModelJob)>();
            let cache = self.clone();
            let spawned = std::thread::Builder::new()
                .name("model-loader".into())
                .spawn(move || {
                    while let Ok((key, job)) = rx.recv() {
                        let entry = match std::fs::read(&job.path) {
                            Err(_) => Entry::Failed("The model file could not be read.".into()),
                            Ok(bytes) => {
                                match pcbcraft_io::load_model_with_part(
                                    &job.path,
                                    &bytes,
                                    job.cad_part.as_deref(),
                                    job.cache_root.as_deref(),
                                ) {
                                    Ok(m) => Entry::Ready(std::sync::Arc::new(m)),
                                    Err(e) => Entry::Failed(e.0),
                                }
                            }
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
            .is_some_and(|tx| tx.send((key, job)).is_ok())
    }
}
