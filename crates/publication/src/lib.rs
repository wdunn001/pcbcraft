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
mod check;
mod compliance;
mod datasheet;
mod hash;
mod licence;
mod manifest;
mod privacy;
mod scan;
mod spdx;
mod zip_read;

pub use bundle::refused_path;
pub use compliance::{ComplianceInput, compliance_notes};
pub use datasheet::{DatasheetRef, check_datasheet_refs, check_embedded_datasheet_bytes};
pub use licence::{
    LicenceVerdict, bundled_part, design_licence, is_restricted_source, licence_table,
};
pub use privacy::{PrivacyOptions, scan_privacy_bytes, scan_privacy_text};
pub use scan::{
    BundleFile, BundledPart, PublicationScanInput, PublicationScanResult, ScanOutcome, ScanRequest,
    scan,
};

pub use check::{
    CONTROLLED_ROLES, CONTROLLED_TAGS, CheckResult, CheckVerdict, Stage0Report,
    stage0_from_manifest_and_files, stage0_from_zip,
};
pub use hash::{bundle_root_hash, sha256_hex};
pub use manifest::{
    Author, BomLine, BomSummary, ComplianceClaim, FileEntry, Identity, LayerLicences, LineageEntry,
    LineageRelation, MAX_CHANGELOG_LEN, MAX_ID_LEN, MAX_MANIFEST_FILES, MAX_NAME_LEN, PreviewEntry,
    ProvenanceBlock, PublicationManifest, PublisherSignature, SCHEMA_VERSION, VersionInfo,
};
pub use spdx::{ALLOWED_SPDX, lineage_compatible, spdx_allowed};
pub use zip_read::{MAX_BUNDLE_ENTRIES, MAX_BUNDLE_UNCOMPRESSED, ZipEntry, read_stored_zip};

/// JSON Schema for [`PublicationManifest`] at [`SCHEMA_VERSION`].
pub const PUBLICATION_SCHEMA_V1: &str = include_str!("../schema/publication.v1.schema.json");
