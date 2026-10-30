//! Publication manifest schema and Stage 0 checks for shared designs and parts.
//!
//! Bundles carry a versioned `publication.json`, a flat ZIP of content files,
//! and a content address over the sorted file hash list. Stage 0 validates
//! structure, licences, paths, and library content before a human queue or
//! registry ingest runs later stages.

mod check;
mod hash;
mod licence;
mod manifest;
mod zip_read;

pub use check::{
    CheckResult, CheckVerdict, Stage0Report, stage0_from_manifest_and_files, stage0_from_zip,
    CONTROLLED_ROLES, CONTROLLED_TAGS,
};
pub use hash::{bundle_root_hash, sha256_hex};
pub use licence::{lineage_compatible, spdx_allowed, ALLOWED_SPDX};
pub use manifest::{
    Author, BomLine, BomSummary, ComplianceClaim, FileEntry, Identity, LayerLicences, LineageEntry,
    LineageRelation, PreviewEntry, ProvenanceBlock, PublicationManifest, PublisherSignature,
    VersionInfo, MAX_CHANGELOG_LEN, MAX_ID_LEN, MAX_MANIFEST_FILES, MAX_NAME_LEN, SCHEMA_VERSION,
};
pub use zip_read::{read_stored_zip, ZipEntry, MAX_BUNDLE_ENTRIES, MAX_BUNDLE_UNCOMPRESSED};

/// JSON Schema for [`PublicationManifest`] at [`SCHEMA_VERSION`].
pub const PUBLICATION_SCHEMA_V1: &str = include_str!("../schema/publication.v1.schema.json");
