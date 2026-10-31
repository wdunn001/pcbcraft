//! Datasheet PDF gate, cache records, LRU and page rendering (no egui).
//!
//! Context: issue #695 children share one cache record shape. The browser build
//! (#720) stores bytes in browser storage; the desktop fetch module (#701) writes
//! the same metadata beside files on disk.
//!
//! Rules: no unwrap on input-derived values; failures are `String` messages.
//!
//! # Dependencies
//! - `hayro` for page rasterisation
//!
//! # Do not
//! - Pull in egui or the engine; UI lives in `pcbcraft-ui-egui`.

pub mod gate;
pub mod lru;
pub mod record;
pub mod render;

#[cfg(any(test, feature = "fixtures"))]
pub mod fixture_pdf;

/// Ingest PDF bytes after validation; returns the cache record and the digest key.
pub fn ingest_pdf(
    bytes: Vec<u8>,
    source_url: Option<String>,
    etag: Option<String>,
    now: &str,
) -> Result<(record::CacheRecord, Vec<u8>), String> {
    gate::validate_pdf_bytes(&bytes)?;
    let sha256 = gate::sha256_hex(&bytes);
    let record = record::CacheRecord::new(sha256, source_url, etag, bytes.len() as u64, now);
    Ok((record, bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_minimal_pdf_renders_page_one() -> Result<(), String> {
        let bytes = fixture_pdf::one_page_text();
        let (record, _) = ingest_pdf(bytes.clone(), None, None, "2026-01-01T00:00:00Z")?;
        assert_eq!(record.byte_len, bytes.len() as u64);
        let page = render::render_page(&bytes, 0, 96.0)?;
        assert!(page.width > 0 && page.height > 0);
        assert_eq!(page.rgba.len(), (page.width * page.height * 4) as usize);
        Ok(())
    }
}
