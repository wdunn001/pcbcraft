//! Shared test helpers: a transport that replays the recorded answers in
//! tests/fixtures/open_library/ and remembers what was asked.
#![allow(dead_code)]
use pcbcraft_library::open_library::{FetchError, Fetcher};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

pub type Boxed = Box<dyn std::error::Error>;

pub fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/open_library")
}

/// Serves the recorded index; remembers every URL asked.
#[derive(Clone)]
pub struct Recorded {
    pub index: Arc<BTreeMap<String, String>>,
    pub asked: Arc<Mutex<Vec<String>>>,
    pub offline: Arc<AtomicBool>,
    pub limited: Arc<AtomicU64>,
}

impl Recorded {
    pub fn new() -> Result<Self, Boxed> {
        let text = std::fs::read_to_string(fixtures().join("index.json"))?;
        Ok(Self {
            index: Arc::new(serde_json::from_str(&text)?),
            asked: Arc::new(Mutex::new(Vec::new())),
            offline: Arc::new(AtomicBool::new(false)),
            limited: Arc::new(AtomicU64::new(0)),
        })
    }
    pub fn count(&self) -> usize {
        self.asked.lock().map(|a| a.len()).unwrap_or(0)
    }
    pub fn urls(&self) -> Vec<String> {
        self.asked.lock().map(|a| a.clone()).unwrap_or_default()
    }
}

impl Fetcher for Recorded {
    fn get(&self, url: &str) -> Result<Vec<u8>, FetchError> {
        if self.offline.load(Ordering::SeqCst) {
            return Err(FetchError::Offline("test is offline".into()));
        }
        let wait = self.limited.load(Ordering::SeqCst);
        if wait > 0 {
            return Err(FetchError::RateLimited {
                retry_after_secs: wait,
            });
        }
        if let Ok(mut a) = self.asked.lock() {
            a.push(url.to_owned());
        }
        let file = self
            .index
            .get(url)
            .ok_or_else(|| FetchError::Failed(format!("not recorded: {url}")))?;
        std::fs::read(fixtures().join(file)).map_err(|e| FetchError::Failed(e.to_string()))
    }
}
