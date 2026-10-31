//! Privacy, licence and bundle checks for shared publications.
//!
//! One library for the app upload step, CI on a static registry, and the portal
//! service. Hard failures block upload; review outcomes go to the human queue.
//!
//! # Rules
//! - A privacy hit is a hard fail with a remove-and-retry action.
//! - Licence pairs come from `data/licence_compat.toml`; unknown pairs are review.
//! - Datasheets are link, title and hash only; copied PDF bytes are refused.
//!
//! # Dependencies
//! - crates/publication/data/*.toml
//! - crates/engine/src/publication.rs
//!
//! # Do not
//! - Certify compliance claims; they are publisher statements only.
//! - Copy datasheet PDFs into bundles when terms do not allow it.

mod bundle;
mod compliance;
mod datasheet;
mod licence;
mod privacy;
mod scan;

pub use bundle::refused_path;
pub use compliance::{ComplianceInput, compliance_notes};
pub use datasheet::{DatasheetRef, check_datasheet_refs, check_embedded_datasheet_bytes};
pub use licence::{
    LicenceVerdict, bundled_part, design_licence, is_restricted_source, licence_table,
};
pub use privacy::{PrivacyOptions, scan_privacy_text, scan_privacy_bytes};
pub use scan::{
    BundleFile, BundledPart, PublicationScanInput, PublicationScanResult, ScanOutcome,
    ScanRequest, scan,
};
