//! Typed view of `publication.json` for a shared design or part bundle.

use serde::{Deserialize, Serialize};

/// Current schema version written into every manifest.
pub const SCHEMA_VERSION: u32 = 1;

/// Maximum length of a publication display name.
pub const MAX_NAME_LEN: usize = 128;
/// Maximum length of a publication slug or identifier string.
pub const MAX_ID_LEN: usize = 64;
/// Maximum changelog length in bytes.
pub const MAX_CHANGELOG_LEN: usize = 16_384;
/// Maximum number of files listed in the manifest.
pub const MAX_MANIFEST_FILES: usize = 512;
/// Maximum number of controlled tags on one publication.
pub const MAX_TAGS: usize = 32;
/// Maximum number of lineage records.
pub const MAX_LINEAGE: usize = 16;
/// Maximum preview entries.
pub const MAX_PREVIEWS: usize = 8;
/// Maximum BOM line items in the summary block.
pub const MAX_BOM_LINES: usize = 4_096;

/// Root document stored as `publication.json` inside a publication bundle.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationManifest {
    pub schema_version: u32,
    pub identity: Identity,
    pub licences: LayerLicences,
    pub version: VersionInfo,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub derived_from: Vec<LineageEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub roles: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bom_summary: Option<BomSummary>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub compliance_claims: Vec<ComplianceClaim>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previews: Vec<PreviewEntry>,
    pub files: Vec<FileEntry>,
    pub provenance: ProvenanceBlock,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publisher_signature: Option<PublisherSignature>,
    pub bundle_root_hash: String,
}

/// Stable identity and authorship for one publication.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Identity {
    pub id: String,
    pub name: String,
    pub authors: Vec<Author>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
}

/// One credited author.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Author {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contact: Option<String>,
}

/// Required SPDX licence id per content layer. Every field must be set explicitly.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LayerLicences {
    pub hardware: String,
    pub software: String,
    pub documentation: String,
    pub models_3d: String,
}

/// Version metadata and optional link to the previous immutable revision.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VersionInfo {
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_version_hash: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub changelog: String,
}

/// Relation to an upstream publication this item derives from.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LineageEntry {
    pub parent_id: String,
    pub relation: LineageRelation,
    pub parent_licence: String,
}

/// How this publication relates to its parent.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LineageRelation {
    Fork,
    Remix,
    Adaptation,
    UsesPart,
}

/// BOM and sourcing summary without prices or stock levels.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BomSummary {
    pub lines: Vec<BomLine>,
}

/// One BOM row in the publication summary.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BomLine {
    pub designator: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub mpn: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub manufacturer: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub datasheet_url: Option<String>,
}

/// Optional compliance statement supplied by the publisher, never certified here.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComplianceClaim {
    pub scheme: String,
    pub claim: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_url: Option<String>,
}

/// Preview image or render shipped with the bundle.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreviewEntry {
    pub path: String,
    pub role: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
}

/// One file in the bundle, named by path relative to the bundle root.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileEntry {
    pub path: String,
    pub sha256: String,
    pub size: u64,
    pub media_type: String,
    pub role: String,
    pub format: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format_version: Option<String>,
}

/// Where the bundle was built and by whom.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProvenanceBlock {
    pub publisher: String,
    pub tool: String,
    pub tool_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_revision: Option<String>,
}

/// Detached signature over the manifest bytes, when the publisher signs bundles.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublisherSignature {
    pub algorithm: String,
    pub public_key_id: String,
    pub signature: String,
}

impl PublicationManifest {
    /// Parse and validate structural bounds on a manifest value.
    pub fn from_json(text: &str) -> Result<Self, String> {
        let manifest: Self =
            serde_json::from_str(text).map_err(|e| format!("publication.json is not valid JSON: {e}"))?;
        manifest.validate_bounds()?;
        Ok(manifest)
    }

    /// Serialize to pretty JSON for round-trip tests and golden fixtures.
    pub fn to_json_pretty(&self) -> Result<String, String> {
        serde_json::to_string_pretty(self).map_err(|e| e.to_string())
    }

    /// Field and collection size limits independent of bundle bytes.
    pub fn validate_bounds(&self) -> Result<(), String> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(format!(
                "schema_version must be {SCHEMA_VERSION}, not {}",
                self.schema_version
            ));
        }
        if self.identity.id.is_empty() || self.identity.id.len() > MAX_ID_LEN {
            return Err("identity.id must be 1 to 64 bytes".into());
        }
        if self.identity.name.is_empty() || self.identity.name.len() > MAX_NAME_LEN {
            return Err("identity.name must be 1 to 128 bytes".into());
        }
        if self.identity.authors.is_empty() || self.identity.authors.len() > 32 {
            return Err("identity.authors must list 1 to 32 authors".into());
        }
        for author in &self.identity.authors {
            if author.name.is_empty() || author.name.len() > MAX_NAME_LEN {
                return Err("each author name must be 1 to 128 bytes".into());
            }
        }
        if self.version.version.is_empty() || self.version.version.len() > 32 {
            return Err("version.version must be 1 to 32 bytes".into());
        }
        if self.version.changelog.len() > MAX_CHANGELOG_LEN {
            return Err("version.changelog exceeds the 16 KiB limit".into());
        }
        if self.derived_from.len() > MAX_LINEAGE {
            return Err("derived_from exceeds the lineage limit".into());
        }
        if self.tags.len() > MAX_TAGS {
            return Err("tags exceed the tag limit".into());
        }
        if self.previews.len() > MAX_PREVIEWS {
            return Err("previews exceed the preview limit".into());
        }
        if self.files.is_empty() || self.files.len() > MAX_MANIFEST_FILES {
            return Err("files must list 1 to 512 entries".into());
        }
        if let Some(bom) = &self.bom_summary
            && bom.lines.len() > MAX_BOM_LINES
        {
            return Err("bom_summary exceeds the line limit".into());
        }
        if self.bundle_root_hash.len() != 64
            || !self
                .bundle_root_hash
                .chars()
                .all(|c| c.is_ascii_hexdigit())
        {
            return Err("bundle_root_hash must be 64 lowercase hex digits".into());
        }
        for file in &self.files {
            if file.path.is_empty() || file.path.len() > 255 {
                return Err("each file path must be 1 to 255 bytes".into());
            }
            if file.sha256.len() != 64
                || !file.sha256.chars().all(|c| c.is_ascii_hexdigit())
            {
                return Err(format!("{} has an invalid sha256 field", file.path));
            }
        }
        Ok(())
    }
}
