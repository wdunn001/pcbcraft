//! Datasheet reference rules for publications.
//!
//! # Rules
//! - Bundles store link, title and content hash only.
//! - Embedded PDF or binary datasheet copies are refused unless explicitly allowed.
//!
//! # Dependencies
//! - crates/publication/src/scan.rs
//!
//! # Do not
//! - Host datasheet PDF bytes in the bundle by default.

use crate::ScanOutcome;

/// A datasheet pointer carried in publication metadata.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatasheetRef {
    pub url: String,
    pub title: String,
    pub sha256: String,
}

/// Validate metadata-only datasheet references.
pub fn check_datasheet_refs(refs: &[DatasheetRef]) -> Vec<ScanOutcome> {
    let mut out = Vec::new();
    for ds in refs {
        if !ds.url.starts_with("https://") {
            out.push(ScanOutcome::HardFail {
                rule: "datasheet_url",
                path: ds.title.clone(),
                line: None,
                column: None,
                detail: "datasheet links must use https".into(),
                action: "remove_and_retry",
            });
        }
        if ds.title.is_empty() || ds.sha256.len() != 64 || !ds.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
            out.push(ScanOutcome::HardFail {
                rule: "datasheet_metadata",
                path: ds.title.clone(),
                line: None,
                column: None,
                detail: "datasheet entries need a title and a 64-character sha256 hash".into(),
                action: "remove_and_retry",
            });
        }
    }
    out
}

/// Refuse raw PDF bytes masquerading as bundle content.
pub fn check_embedded_datasheet_bytes(path: &str, bytes: &[u8], role: &str) -> Vec<ScanOutcome> {
    if role == "datasheet_copy" {
        return vec![ScanOutcome::HardFail {
            rule: "datasheet_copy",
            path: path.into(),
            line: None,
            column: None,
            detail: "copied datasheet files are not allowed; store link, title and hash only".into(),
            action: "remove_and_retry",
        }];
    }
    if bytes.starts_with(b"%PDF-") && role != "allowed_datasheet_pdf" {
        return vec![ScanOutcome::HardFail {
            rule: "datasheet_pdf_bytes",
            path: path.into(),
            line: None,
            column: None,
            detail: "PDF bytes in the bundle need explicit terms that allow copying".into(),
            action: "remove_and_retry",
        }];
    }
    Vec::new()
}
