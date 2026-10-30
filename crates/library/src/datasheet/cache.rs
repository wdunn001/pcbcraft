//! The cache of datasheets a person fetched or chose, kept on this computer.
//!
//! Context: issue #678 and the datasheet reader note. The reader only ever opens what is here.
//! A PDF gets here by a user action: pressing Get datasheet (the app fetches it under a size
//! cap and a time limit) or choosing a file. The project file keeps only the link; the PDF
//! stays in this cache and is never uploaded, bundled or written into a shared library.
//!
//! Local rules: the key is the SHA-256 of the address; each entry is a `.pdf` and a `.json`
//! record with the address, the file's own SHA-256, the time and where it came from. Bytes are
//! checked (`check_pdf`) before they are stored. The total size is capped and the oldest files
//! go first. Reads never fetch.
//!
//! # Dependencies
//! - crates/library/src/datasheet/mod.rs (`check_pdf`, `DatasheetError`)
//!
//! # Do not
//! - Store a file that is not a PDF, or an address that is not http or https.
use super::{DatasheetError, check_pdf};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// Default total size of the cache, in bytes (500 MB).
pub const DEFAULT_CAP_BYTES: u64 = 500 * 1024 * 1024;

/// Where a cached PDF came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// Fetched from its address when the person asked.
    Downloaded,
    /// A file the person chose.
    UserFile,
}

/// The record kept beside each cached PDF.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CachedDoc {
    /// The address the datasheet is for.
    pub url: String,
    /// SHA-256 of the PDF bytes, in hex.
    pub sha256: String,
    /// Seconds since the Unix epoch when it was stored.
    pub stored_at: u64,
    /// Where it came from.
    pub source: Source,
    /// Size in bytes.
    pub bytes: u64,
}

/// A folder of cached datasheets.
#[derive(Clone, Debug)]
pub struct DatasheetCache {
    dir: PathBuf,
    cap: u64,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Accept only web addresses.
///
/// # Rules
/// - `http://` and `https://` with something after it; nothing else.
pub fn gate_url(url: &str) -> Result<&str, DatasheetError> {
    let u = url.trim();
    let rest = u
        .strip_prefix("https://")
        .or_else(|| u.strip_prefix("http://"))
        .ok_or(DatasheetError::BadAddress)?;
    if rest.is_empty() || u.chars().any(char::is_control) {
        return Err(DatasheetError::BadAddress);
    }
    Ok(u)
}

impl DatasheetCache {
    /// A cache in `dir` with the default size cap. The folder is made when first written.
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self {
            dir: dir.into(),
            cap: DEFAULT_CAP_BYTES,
        }
    }

    /// The same cache with another size cap.
    pub fn with_cap(mut self, cap: u64) -> Self {
        self.cap = cap;
        self
    }

    /// The usual place: `datasheets` beside the online cache folder.
    pub fn default_dir() -> Option<PathBuf> {
        super::super::cad_cache::default_cache_dir()
            .and_then(|d| d.parent().map(|p| p.join("datasheets")))
    }

    fn key(url: &str) -> String {
        hex(&Sha256::digest(url.trim().as_bytes()))
    }

    fn paths(&self, url: &str) -> (PathBuf, PathBuf) {
        let k = Self::key(url);
        (
            self.dir.join(format!("{k}.pdf")),
            self.dir.join(format!("{k}.json")),
        )
    }

    /// Store `bytes` as the datasheet for `url`.
    ///
    /// # Rules
    /// - The address and the bytes are checked first; a refusal changes nothing.
    /// - Older entries are removed when the total passes the cap.
    pub fn put(
        &self,
        url: &str,
        bytes: &[u8],
        source: Source,
        now: u64,
    ) -> Result<CachedDoc, DatasheetError> {
        let url = gate_url(url)?;
        check_pdf(bytes)?;
        let io = |e: std::io::Error| DatasheetError::Cache(e.to_string());
        std::fs::create_dir_all(&self.dir).map_err(io)?;
        let doc = CachedDoc {
            url: url.to_owned(),
            sha256: hex(&Sha256::digest(bytes)),
            stored_at: now,
            source,
            bytes: bytes.len() as u64,
        };
        let (pdf, meta) = self.paths(url);
        std::fs::write(&pdf, bytes).map_err(io)?;
        let record = serde_json::to_vec(&doc).map_err(|e| DatasheetError::Cache(e.to_string()))?;
        std::fs::write(&meta, record).map_err(io)?;
        self.evict(&pdf);
        Ok(doc)
    }

    /// The record for `url`, when one is cached and its PDF is still there.
    pub fn info(&self, url: &str) -> Option<CachedDoc> {
        let (pdf, meta) = self.paths(url);
        if !pdf.is_file() {
            return None;
        }
        serde_json::from_slice(&std::fs::read(meta).ok()?).ok()
    }

    /// The cached bytes for `url`, checked again against the recorded hash.
    pub fn get(&self, url: &str) -> Option<(CachedDoc, Vec<u8>)> {
        let doc = self.info(url)?;
        let (pdf, _) = self.paths(url);
        let bytes = std::fs::read(pdf).ok()?;
        (hex(&Sha256::digest(&bytes)) == doc.sha256).then_some((doc, bytes))
    }

    /// Remove the oldest files until the folder is within the cap, never the one just written.
    fn evict(&self, keep: &Path) {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return;
        };
        let mut files: Vec<(std::time::SystemTime, PathBuf, u64)> = entries
            .filter_map(Result::ok)
            .filter(|e| e.path().extension().is_some_and(|x| x == "pdf"))
            .filter_map(|e| {
                let m = e.metadata().ok()?;
                Some((m.modified().ok()?, e.path(), m.len()))
            })
            .collect();
        let mut total: u64 = files.iter().map(|f| f.2).sum();
        files.sort();
        for (_, path, len) in files {
            if total <= self.cap {
                break;
            }
            if path == keep {
                continue;
            }
            if std::fs::remove_file(&path).is_ok() {
                let _ = std::fs::remove_file(path.with_extension("json"));
                total = total.saturating_sub(len);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::datasheet::fixtures;

    fn pdf() -> Vec<u8> {
        fixtures::all()
            .into_iter()
            .next()
            .map(|f| f.bytes)
            .unwrap_or_default()
    }

    #[test]
    fn a_stored_datasheet_reads_back_with_its_record() -> Result<(), DatasheetError> {
        let dir = tempfile::tempdir().map_err(|e| DatasheetError::Cache(e.to_string()))?;
        let cache = DatasheetCache::new(dir.path());
        let url = "https://example.com/nr1206.pdf";
        assert!(cache.get(url).is_none());
        let doc = cache.put(url, &pdf(), Source::Downloaded, 1_792_540_800)?;
        let (back, bytes) = cache.get(url).ok_or(DatasheetError::NotOpen)?;
        assert_eq!(back, doc);
        assert_eq!(bytes, pdf());
        assert_eq!(back.stored_at, 1_792_540_800);
        Ok(())
    }

    #[test]
    fn a_file_that_is_not_a_pdf_or_an_address_that_is_not_web_is_refused() {
        let dir = tempfile::tempdir().map(|d| d.keep()).unwrap_or_default();
        let cache = DatasheetCache::new(&dir);
        assert_eq!(
            cache.put("https://x.test/a.pdf", b"<html>", Source::Downloaded, 0),
            Err(DatasheetError::NotPdf)
        );
        assert_eq!(
            cache.put("file:///etc/passwd", &pdf(), Source::UserFile, 0),
            Err(DatasheetError::BadAddress)
        );
        let big = vec![b'x'; super::super::MAX_PDF_BYTES + 1];
        assert_eq!(
            cache.put("https://x.test/a.pdf", &big, Source::Downloaded, 0),
            Err(DatasheetError::TooLarge)
        );
    }

    #[test]
    fn the_oldest_file_goes_when_the_cap_is_passed() -> Result<(), DatasheetError> {
        let dir = tempfile::tempdir().map_err(|e| DatasheetError::Cache(e.to_string()))?;
        let bytes = pdf();
        let cache = DatasheetCache::new(dir.path()).with_cap(bytes.len() as u64 + 10);
        cache.put("https://a.test/1.pdf", &bytes, Source::Downloaded, 1)?;
        std::thread::sleep(std::time::Duration::from_millis(20));
        cache.put("https://a.test/2.pdf", &bytes, Source::Downloaded, 2)?;
        assert!(
            cache.info("https://a.test/1.pdf").is_none(),
            "the older file was removed"
        );
        assert!(
            cache.info("https://a.test/2.pdf").is_some(),
            "the newer one stays"
        );
        Ok(())
    }
}
