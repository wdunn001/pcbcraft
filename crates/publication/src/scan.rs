//! Aggregate publication privacy, licence and bundle checks.
//!
//! # Rules
//! - Hard fails block upload; human-queue items need moderation.
//!
//! # Dependencies
//! - crates/publication/src/privacy.rs
//! - crates/publication/src/licence.rs
//! - crates/publication/src/datasheet.rs
//! - crates/publication/src/bundle.rs
//! - crates/publication/src/compliance.rs
//!
//! # Do not
//! - Mutate bundle bytes; callers remove findings and retry.

use crate::bundle::refused_path;
use crate::compliance::{ComplianceInput, compliance_notes};
use crate::datasheet::{DatasheetRef, check_datasheet_refs, check_embedded_datasheet_bytes};
use crate::licence::{bundled_part, licence_table};
use crate::privacy::{PrivacyOptions, scan_privacy_bytes, scan_privacy_text};
use base64::Engine as _;
use serde::Deserialize;

/// One file entry submitted to the scan.
#[derive(Clone, Debug)]
pub struct BundleFile {
    pub path: String,
    pub role: String,
    pub text: Option<String>,
    pub bytes_base64: Option<String>,
}

/// Licence and source for one bundled part.
#[derive(Clone, Debug)]
pub struct BundledPart {
    pub id: String,
    pub licence: String,
    pub source: String,
}

/// Full scan input for a publication upload.
#[derive(Clone, Debug, Default)]
pub struct PublicationScanInput {
    pub design_licence: String,
    pub author_emails: Vec<String>,
    pub parts: Vec<BundledPart>,
    pub datasheets: Vec<DatasheetRef>,
    pub compliance: ComplianceInput,
    pub files: Vec<BundleFile>,
}

/// One outcome from any check.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScanOutcome {
    HardFail {
        rule: &'static str,
        path: String,
        line: Option<u32>,
        column: Option<u32>,
        detail: String,
        action: &'static str,
    },
    HumanQueue {
        rule: &'static str,
        path: String,
        detail: String,
    },
    Note {
        rule: &'static str,
        detail: String,
    },
}

/// Combined scan result.
#[derive(Clone, Debug, Default)]
pub struct PublicationScanResult {
    pub hard_fails: Vec<ScanOutcome>,
    pub human_queue: Vec<ScanOutcome>,
    pub notes: Vec<ScanOutcome>,
}

impl PublicationScanResult {
    /// True when no hard fails are present.
    pub fn ok_to_upload(&self) -> bool {
        self.hard_fails.is_empty()
    }
}

/// Run every publication check on the input.
pub fn scan(input: &PublicationScanInput) -> PublicationScanResult {
    let table = licence_table();
    let mut hard = Vec::new();
    let mut queue = Vec::new();
    let mut notes = Vec::new();

    let mut privacy_opts = PrivacyOptions::default();
    for email in &input.author_emails {
        privacy_opts
            .allowed_emails
            .insert(email.to_ascii_lowercase());
    }

    for part in &input.parts {
        push_split(
            bundled_part(
                &table,
                &input.design_licence,
                &part.id,
                &part.licence,
                &part.source,
            ),
            &mut hard,
            &mut queue,
            &mut notes,
        );
    }

    push_split(check_datasheet_refs(&input.datasheets), &mut hard, &mut queue, &mut notes);
    push_split(
        compliance_notes(&input.compliance),
        &mut hard,
        &mut queue,
        &mut notes,
    );

    for file in &input.files {
        if refused_path(&file.path) {
            hard.push(ScanOutcome::HardFail {
                rule: "refused_file_type",
                path: file.path.clone(),
                line: None,
                column: None,
                detail: "executable or script file types are refused in publication bundles"
                    .into(),
                action: "remove_and_retry",
            });
            continue;
        }
        if let Some(text) = &file.text {
            push_split(
                scan_privacy_text(&file.path, text, &privacy_opts),
                &mut hard,
                &mut queue,
                &mut notes,
            );
            push_split(
                check_embedded_datasheet_bytes(&file.path, text.as_bytes(), &file.role),
                &mut hard,
                &mut queue,
                &mut notes,
            );
        } else if let Some(b64) = &file.bytes_base64 {
            if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(b64) {
                push_split(
                    scan_privacy_bytes(&file.path, &bytes, &privacy_opts),
                    &mut hard,
                    &mut queue,
                    &mut notes,
                );
                push_split(
                    check_embedded_datasheet_bytes(&file.path, &bytes, &file.role),
                    &mut hard,
                    &mut queue,
                    &mut notes,
                );
            } else {
                hard.push(ScanOutcome::HardFail {
                    rule: "invalid_base64",
                    path: file.path.clone(),
                    line: None,
                    column: None,
                    detail: "file bytes are not valid base64".into(),
                    action: "remove_and_retry",
                });
            }
        }
    }

    PublicationScanResult {
        hard_fails: hard,
        human_queue: queue,
        notes,
    }
}

fn push_split(
    items: Vec<ScanOutcome>,
    hard: &mut Vec<ScanOutcome>,
    queue: &mut Vec<ScanOutcome>,
    notes: &mut Vec<ScanOutcome>,
) {
    for item in items {
        match item {
            ScanOutcome::HardFail { .. } => hard.push(item),
            ScanOutcome::HumanQueue { .. } => queue.push(item),
            ScanOutcome::Note { .. } => notes.push(item),
        }
    }
}

/// JSON-friendly scan request for engine and CI callers.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScanRequest {
    pub design_licence: String,
    #[serde(default)]
    pub author_emails: Vec<String>,
    #[serde(default)]
    pub parts: Vec<ScanPart>,
    #[serde(default)]
    pub datasheets: Vec<ScanDatasheet>,
    #[serde(default)]
    pub compliance: ScanCompliance,
    #[serde(default)]
    pub files: Vec<ScanFile>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScanPart {
    pub id: String,
    pub licence: String,
    pub source: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScanDatasheet {
    pub url: String,
    pub title: String,
    pub sha256: String,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ScanCompliance {
    #[serde(default)]
    pub claims: Vec<String>,
    #[serde(default)]
    pub export_review_requested: bool,
    #[serde(default)]
    pub mains_power: bool,
    #[serde(default)]
    pub battery_power: bool,
    #[serde(default)]
    pub high_voltage: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScanFile {
    pub path: String,
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub bytes_base64: Option<String>,
}

impl ScanRequest {
    /// Convert into the typed scan input.
    pub fn into_input(self) -> PublicationScanInput {
        PublicationScanInput {
            design_licence: self.design_licence,
            author_emails: self.author_emails,
            parts: self
                .parts
                .into_iter()
                .map(|p| BundledPart {
                    id: p.id,
                    licence: p.licence,
                    source: p.source,
                })
                .collect(),
            datasheets: self
                .datasheets
                .into_iter()
                .map(|d| DatasheetRef {
                    url: d.url,
                    title: d.title,
                    sha256: d.sha256,
                })
                .collect(),
            compliance: ComplianceInput {
                claims: self.compliance.claims,
                export_review_requested: self.compliance.export_review_requested,
                mains_power: self.compliance.mains_power,
                battery_power: self.compliance.battery_power,
                high_voltage: self.compliance.high_voltage,
            },
            files: self
                .files
                .into_iter()
                .map(|f| BundleFile {
                    path: f.path,
                    role: f.role,
                    text: f.text,
                    bytes_base64: f.bytes_base64,
                })
                .collect(),
        }
    }
}
