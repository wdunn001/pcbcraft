//! Compliance report writers: CSV, Markdown, HTML and a plain-text PDF.
//!
//! Context: issue #704. The report summarises part-level compliance data and
//! optional rule-set choices. It is a data-quality aid, not a product
//! certification. The documentation export package (issue #214) may include
//! the `compliance_report` kind as an optional file once package records ship.
//!
//! # Rules
//! - Identical project data and options yield identical bytes (sorted rows, LF).
//! - Prose never states that the product is compliant.
//!
//! # Dependencies
//! - `crates/doc/src/compliance.rs` for declarations and manufacturers.
//! - `crates/io/src/lib.rs` for `pdf_plain_text_document`.
//!
//! # Do not
//! - Embed timestamps or host names in the output.

use pcbcraft_doc::{
    Component, Declaration, DeclarationStatus, DeclarationSourceKind, Project,
};
use std::collections::{BTreeMap, BTreeSet};

/// Most rule set ids named in one export.
pub const MAX_RULE_SETS: usize = 8;
/// Most regime columns in the matrix.
pub const MAX_REGIMES: usize = 32;
/// Most list snapshots listed in one report.
pub const MAX_SNAPSHOTS: usize = 32;
/// Most waived findings printed.
pub const MAX_WAIVERS: usize = 256;
/// Most BOM parts in one report.
pub const MAX_REPORT_PARTS: usize = 4096;

const DISCLAIMER: &str = "This report is a data-quality aid. It summarises declarations and \
                          snapshots stored in the project. It is not a legal opinion and does \
                          not certify the product for any market.";

/// Output format for `compliance.report` and the `compliance_report` export kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComplianceReportFormat {
    Csv,
    Markdown,
    Html,
    Pdf,
}

impl ComplianceReportFormat {
    pub fn parse(text: &str) -> Result<Self, String> {
        match text.trim().to_ascii_lowercase().as_str() {
            "csv" => Ok(Self::Csv),
            "markdown" | "md" => Ok(Self::Markdown),
            "html" | "htm" => Ok(Self::Html),
            "pdf" => Ok(Self::Pdf),
            other => Err(format!("compliance report format must be csv, markdown, html or pdf, not {other}")),
        }
    }

    pub fn file_name(self) -> &'static str {
        match self {
            Self::Csv => "compliance-report.csv",
            Self::Markdown => "compliance-report.md",
            Self::Html => "compliance-report.html",
            Self::Pdf => "compliance-report.pdf",
        }
    }
}

/// One chosen rule set line in the report header.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuleSetLine {
    pub id: String,
    pub version: String,
}

/// One list snapshot named in the report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnapshotLine {
    pub id: String,
    pub as_of: String,
    pub licence: String,
    pub source: String,
}

/// A waived or muted finding with the reason stored on the project.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WaiverLine {
    pub rule: String,
    pub subject: String,
    pub reason: String,
}

/// Options that pin rule sets, regimes, snapshots and waivers for one run.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ComplianceReportOptions {
    pub rule_sets: Vec<RuleSetLine>,
    pub snapshots: Vec<SnapshotLine>,
    pub regimes: Vec<String>,
    pub waivers: Vec<WaiverLine>,
}

impl ComplianceReportOptions {
    pub fn validate(&self) -> Result<(), String> {
        if self.rule_sets.len() > MAX_RULE_SETS {
            return Err(format!("a compliance report names at most {MAX_RULE_SETS} rule sets"));
        }
        for rs in &self.rule_sets {
            bounded("rule set id", &rs.id, 64)?;
            bounded("rule set version", &rs.version, 64)?;
        }
        if self.regimes.len() > MAX_REGIMES {
            return Err(format!("a compliance report lists at most {MAX_REGIMES} regimes"));
        }
        for regime in &self.regimes {
            bounded("regime", regime, 256)?;
        }
        if self.snapshots.len() > MAX_SNAPSHOTS {
            return Err(format!("a compliance report lists at most {MAX_SNAPSHOTS} snapshots"));
        }
        for snap in &self.snapshots {
            bounded("snapshot id", &snap.id, 128)?;
            bounded("snapshot date", &snap.as_of, 32)?;
            bounded("snapshot licence", &snap.licence, 256)?;
            bounded("snapshot source", &snap.source, 512)?;
        }
        if self.waivers.len() > MAX_WAIVERS {
            return Err(format!("a compliance report lists at most {MAX_WAIVERS} waivers"));
        }
        for w in &self.waivers {
            bounded("waiver rule", &w.rule, 128)?;
            bounded("waiver subject", &w.subject, 256)?;
            bounded("waiver reason", &w.reason, 512)?;
        }
        Ok(())
    }
}

fn bounded(label: &str, text: &str, max: usize) -> Result<(), String> {
    if text.len() > max || text.chars().any(char::is_control) {
        return Err(format!("{label} must be at most {max} bytes without control characters"));
    }
    Ok(())
}

fn csv_cell(text: &str) -> String {
    if text.contains([',', '"', '\n']) {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text.to_string()
    }
}

fn html_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(ch),
        }
    }
    out
}

fn project_revision(p: &Project) -> String {
    p.meta
        .get("revision")
        .cloned()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "unspecified".into())
}

fn project_title(p: &Project) -> String {
    let title = p.name.trim();
    if title.is_empty() {
        "untitled design".into()
    } else {
        title.to_owned()
    }
}

fn manufacturer_of(c: &Component) -> String {
    let m = c.bom_value("Manufacturer");
    if m.trim().is_empty() {
        "unknown maker".into()
    } else {
        m
    }
}

fn mpn_of(c: &Component) -> String {
    c.bom_value("MPN")
}

fn status_label(status: DeclarationStatus) -> &'static str {
    match status {
        DeclarationStatus::DeclaredCompliant => "declared",
        DeclarationStatus::DeclaredExempt => "exempt",
        DeclarationStatus::DeclaredNoncompliant => "declared_not_met",
        DeclarationStatus::NotApplicable => "not_applicable",
        DeclarationStatus::Unknown => "unknown",
    }
}

fn regime_cell(c: &Component, regime: &str) -> String {
    let key = regime.trim().to_ascii_lowercase();
    for d in c.effective_declarations() {
        if d.regime.trim().eq_ignore_ascii_case(&key) {
            return status_label(d.status).to_owned();
        }
    }
    "unverified".into()
}

fn collect_regimes(p: &Project, options: &ComplianceReportOptions) -> Result<Vec<String>, String> {
    let mut set: BTreeSet<String> = BTreeSet::new();
    for r in &options.regimes {
        let trimmed = r.trim();
        if !trimmed.is_empty() {
            set.insert(trimmed.to_ascii_lowercase());
        }
    }
    if set.is_empty() {
        for c in &p.components {
            if !c.in_bom() {
                continue;
            }
            for d in c.effective_declarations() {
                let regime = d.regime.trim();
                if !regime.is_empty() {
                    set.insert(regime.to_ascii_lowercase());
                }
            }
        }
    }
    if set.is_empty() {
        set.insert("rohs".into());
        set.insert("reach".into());
    }
    Ok(set.into_iter().collect())
}

struct MatrixRow {
    reference: String,
    manufacturer: String,
    mpn: String,
    cells: Vec<String>,
}

fn build_matrix(
    p: &Project,
    regimes: &[String],
) -> Result<Vec<MatrixRow>, String> {
    let mut rows = Vec::new();
    let mut parts = 0usize;
    let mut refs: Vec<&Component> = p.components.iter().filter(|c| c.in_bom()).collect();
    refs.sort_by(|a, b| a.reference.cmp(&b.reference));
    for c in refs {
        parts += 1;
        if parts > MAX_REPORT_PARTS {
            return Err(format!("compliance report exceeds {MAX_REPORT_PARTS} parts"));
        }
        let cells: Vec<String> = regimes.iter().map(|r| regime_cell(c, r)).collect();
        rows.push(MatrixRow {
            reference: c.reference.clone(),
            manufacturer: manufacturer_of(c),
            mpn: mpn_of(c),
            cells,
        });
    }
    Ok(rows)
}

struct EvidenceRow {
    reference: String,
    regime: String,
    status: String,
    as_of: String,
    source: String,
    document: String,
}

fn build_evidence(p: &Project) -> Result<Vec<EvidenceRow>, String> {
    let mut out = Vec::new();
    let mut refs: Vec<&Component> = p.components.iter().filter(|c| c.in_bom()).collect();
    refs.sort_by(|a, b| a.reference.cmp(&b.reference));
    for c in refs {
        for d in c.effective_declarations() {
            out.push(evidence_from_declaration(&c.reference, d));
        }
    }
    Ok(out)
}

fn evidence_from_declaration(reference: &str, d: Declaration) -> EvidenceRow {
    let source_kind = match d.source.kind {
        DeclarationSourceKind::Manufacturer => "manufacturer",
        DeclarationSourceKind::Distributor => "distributor",
        DeclarationSourceKind::User => "user",
        DeclarationSourceKind::Imported => "imported",
    };
    let source = if d.source.label.trim().is_empty() {
        source_kind.to_owned()
    } else {
        format!("{source_kind}: {}", d.source.label.trim())
    };
    let document = d
        .source
        .document_ref
        .clone()
        .or(d.source.url.clone())
        .unwrap_or_default();
    EvidenceRow {
        reference: reference.to_owned(),
        regime: d.regime.trim().to_ascii_lowercase(),
        status: status_label(d.status).to_owned(),
        as_of: d.as_of.clone(),
        source,
        document,
    }
}

struct SupplierBlock {
    manufacturer: String,
    lines: Vec<String>,
}

fn build_supplier_questions(
    p: &Project,
    regimes: &[String],
) -> Result<Vec<SupplierBlock>, String> {
    let mut by_maker: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for c in p.components.iter().filter(|c| c.in_bom()) {
        let maker = manufacturer_of(c);
        let mut needs: BTreeSet<String> = BTreeSet::new();
        for regime in regimes {
            if regime_cell(c, regime) == "unverified" {
                needs.insert(format!("declaration for regime {regime}"));
            }
            if regime.contains("mineral") && regime_cell(c, regime) == "unverified" {
                needs.insert("minerals reporting template".into());
            }
            if regime.contains("export") && regime_cell(c, regime) == "unverified" {
                needs.insert("export classification".into());
            }
        }
        if c.compliance.country_of_origin.is_none() {
            needs.insert("country of origin".into());
        }
        if needs.is_empty() {
            continue;
        }
        by_maker.entry(maker).or_default().extend(needs);
    }
    Ok(by_maker
        .into_iter()
        .map(|(manufacturer, lines)| SupplierBlock {
            manufacturer,
            lines: lines.into_iter().collect(),
        })
        .collect())
}

fn render_markdown(
    p: &Project,
    options: &ComplianceReportOptions,
    regimes: &[String],
    matrix: &[MatrixRow],
    evidence: &[EvidenceRow],
    suppliers: &[SupplierBlock],
) -> String {
    let mut out = String::new();
    out.push_str("# Compliance data report\n\n");
    out.push_str(&format!("Project: {}\n", project_title(p)));
    out.push_str(&format!("Revision: {}\n\n", project_revision(p)));
    out.push_str("## Rule sets\n\n");
    if options.rule_sets.is_empty() {
        out.push_str("No rule sets were named for this export.\n\n");
    } else {
        for rs in &options.rule_sets {
            let version = if rs.version.trim().is_empty() {
                "version not recorded"
            } else {
                rs.version.trim()
            };
            out.push_str(&format!("- {} ({})\n", rs.id.trim(), version));
        }
        out.push('\n');
    }
    out.push_str("## List snapshots\n\n");
    if options.snapshots.is_empty() {
        out.push_str("No list snapshots were supplied for this export.\n\n");
    } else {
        for snap in &options.snapshots {
            out.push_str(&format!(
                "- {} dated {} licence {} source {}\n",
                snap.id.trim(),
                if snap.as_of.trim().is_empty() {
                    "unknown"
                } else {
                    snap.as_of.trim()
                },
                if snap.licence.trim().is_empty() {
                    "unspecified"
                } else {
                    snap.licence.trim()
                },
                if snap.source.trim().is_empty() {
                    "unspecified"
                } else {
                    snap.source.trim()
                },
            ));
        }
        out.push('\n');
    }
    out.push_str("## Part by regime matrix\n\n");
    out.push_str("| Reference | Manufacturer | MPN |");
    for regime in regimes {
        out.push('|');
        out.push_str(regime);
    }
    out.push_str("|\n| --- | --- | --- |");
    for _ in regimes {
        out.push_str(" --- |");
    }
    out.push('\n');
    for row in matrix {
        out.push('|');
        out.push_str(&row.reference);
        out.push('|');
        out.push_str(&row.manufacturer);
        out.push('|');
        out.push_str(&row.mpn);
        out.push('|');
        for cell in &row.cells {
            out.push_str(cell);
            out.push('|');
        }
        out.push('\n');
    }
    out.push('\n');
    out.push_str("## Evidence appendix\n\n");
    if evidence.is_empty() {
        out.push_str("No declarations were recorded on the bill of materials.\n\n");
    } else {
        for e in evidence {
            out.push_str(&format!(
                "- {} / {} / {} / dated {} / {} / document {}\n",
                e.reference,
                e.regime,
                e.status,
                if e.as_of.trim().is_empty() {
                    "unknown"
                } else {
                    e.as_of.trim()
                },
                e.source,
                if e.document.trim().is_empty() {
                    "none"
                } else {
                    e.document.trim()
                },
            ));
        }
        out.push('\n');
    }
    out.push_str("## Waivers\n\n");
    if options.waivers.is_empty() {
        out.push_str("No waived findings were recorded.\n\n");
    } else {
        for w in &options.waivers {
            out.push_str(&format!(
                "- {} on {} reason {}\n",
                w.rule.trim(),
                w.subject.trim(),
                w.reason.trim()
            ));
        }
        out.push('\n');
    }
    out.push_str("## Supplier questions (unverified data)\n\n");
    if suppliers.is_empty() {
        out.push_str("Every listed part had the requested regime data.\n\n");
    } else {
        for block in suppliers {
            out.push_str(&format!("### {}\n\n", block.manufacturer));
            for line in &block.lines {
                out.push_str(&format!("- {line}\n"));
            }
            out.push('\n');
        }
    }
    out.push_str("## Disclaimer\n\n");
    out.push_str(DISCLAIMER);
    out.push('\n');
    out
}

fn render_csv(
    p: &Project,
    options: &ComplianceReportOptions,
    regimes: &[String],
    matrix: &[MatrixRow],
    evidence: &[EvidenceRow],
    suppliers: &[SupplierBlock],
) -> String {
    let mut out = String::new();
    out.push_str("section,key,value\n");
    out.push_str(&format!(
        "project,name,{}\n",
        csv_cell(&project_title(p))
    ));
    out.push_str(&format!(
        "project,revision,{}\n",
        csv_cell(&project_revision(p))
    ));
    for rs in &options.rule_sets {
        out.push_str(&format!(
            "rule_set,id,{}\n",
            csv_cell(rs.id.trim())
        ));
        out.push_str(&format!(
            "rule_set,version,{}\n",
            csv_cell(rs.version.trim())
        ));
    }
    for snap in &options.snapshots {
        out.push_str(&format!("snapshot,id,{}\n", csv_cell(snap.id.trim())));
        out.push_str(&format!("snapshot,as_of,{}\n", csv_cell(snap.as_of.trim())));
        out.push_str(&format!(
            "snapshot,licence,{}\n",
            csv_cell(snap.licence.trim())
        ));
        out.push_str(&format!(
            "snapshot,source,{}\n",
            csv_cell(snap.source.trim())
        ));
    }
    out.push_str("matrix,header,reference,manufacturer,mpn");
    for regime in regimes {
        out.push(',');
        out.push_str(&csv_cell(regime));
    }
    out.push('\n');
    for row in matrix {
        out.push_str("matrix,row,");
        out.push_str(&csv_cell(&row.reference));
        out.push(',');
        out.push_str(&csv_cell(&row.manufacturer));
        out.push(',');
        out.push_str(&csv_cell(&row.mpn));
        for cell in &row.cells {
            out.push(',');
            out.push_str(&csv_cell(cell));
        }
        out.push('\n');
    }
    for e in evidence {
        out.push_str(&format!(
            "evidence,{},{},{},{},{},{}\n",
            csv_cell(&e.reference),
            csv_cell(&e.regime),
            csv_cell(&e.status),
            csv_cell(&e.as_of),
            csv_cell(&e.source),
            csv_cell(&e.document),
        ));
    }
    for w in &options.waivers {
        out.push_str(&format!(
            "waiver,{},{},{}\n",
            csv_cell(w.rule.trim()),
            csv_cell(w.subject.trim()),
            csv_cell(w.reason.trim())
        ));
    }
    for block in suppliers {
        for line in &block.lines {
            out.push_str(&format!(
                "supplier_question,{},{}\n",
                csv_cell(&block.manufacturer),
                csv_cell(line)
            ));
        }
    }
    out.push_str(&format!("disclaimer,text,{}\n", csv_cell(DISCLAIMER)));
    let _ = p;
    out
}

fn render_html(body_md: &str) -> String {
    let mut out = String::from(
        "<!DOCTYPE html><html lang=\"en\"><head><meta charset=\"utf-8\"><title>Compliance data report</title></head><body><pre>",
    );
    out.push_str(&html_escape(body_md));
    out.push_str("</pre></body></html>\n");
    out
}

/// Build the compliance report bytes for one format.
pub fn compliance_report_bytes(
    p: &Project,
    format: ComplianceReportFormat,
    options: &ComplianceReportOptions,
) -> Result<Vec<u8>, String> {
    p.validate()?;
    options.validate()?;
    let regimes = collect_regimes(p, options)?;
    let matrix = build_matrix(p, &regimes)?;
    let evidence = build_evidence(p)?;
    let suppliers = build_supplier_questions(p, &regimes)?;
    let markdown = render_markdown(
        p,
        options,
        &regimes,
        &matrix,
        &evidence,
        &suppliers,
    );
    match format {
        ComplianceReportFormat::Markdown => Ok(markdown.into_bytes()),
        ComplianceReportFormat::Csv => Ok(render_csv(
            p,
            options,
            &regimes,
            &matrix,
            &evidence,
            &suppliers,
        )
        .into_bytes()),
        ComplianceReportFormat::Html => Ok(render_html(&markdown).into_bytes()),
        ComplianceReportFormat::Pdf => crate::pdf_plain_text_document(&markdown),
    }
}

/// Markdown body for tests and the default export kind file.
pub fn compliance_report_markdown(
    p: &Project,
    options: &ComplianceReportOptions,
) -> Result<String, String> {
    p.validate()?;
    options.validate()?;
    let regimes = collect_regimes(p, options)?;
    let matrix = build_matrix(p, &regimes)?;
    let evidence = build_evidence(p)?;
    let suppliers = build_supplier_questions(p, &regimes)?;
    Ok(render_markdown(
        p,
        options,
        &regimes,
        &matrix,
        &evidence,
        &suppliers,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pcbcraft_doc::{
        Compliance, CountryOfOrigin, Declaration, DeclarationSource, DeclarationStatus, PartRole,
    };

    fn sample_project() -> Project {
        let mut p = Project::default();
        p.name = "Drone FC".into();
        p.meta.insert("revision".into(), "rev-a".into());
        let mut c = pcbcraft_doc::Component {
            id: 1,
            reference: "U1".into(),
            value: "MCU".into(),
            kind: "ic".into(),
            ..Default::default()
        };
        c.fields.push(pcbcraft_doc::ComponentField {
            key: "Manufacturer".into(),
            value: "Example Components".into(),
        });
        c.fields.push(pcbcraft_doc::ComponentField {
            key: "MPN".into(),
            value: "EX-1".into(),
        });
        c.compliance = Compliance {
            role: Some(PartRole::FlightController),
            country_of_origin: Some(CountryOfOrigin {
                code: "US".into(),
                basis: String::new(),
                as_of: "2024-01-15".into(),
            }),
            declarations: vec![Declaration {
                regime: "rohs".into(),
                status: DeclarationStatus::DeclaredCompliant,
                detail: String::new(),
                source: DeclarationSource {
                    kind: DeclarationSourceKind::Manufacturer,
                    label: "maker portal".into(),
                    url: None,
                    document_ref: Some("DOC-9".into()),
                },
                as_of: "2024-01-15".into(),
            }],
            ..Default::default()
        };
        p.components.push(c);
        p
    }

    #[test]
    fn identical_inputs_yield_identical_markdown_bytes() -> Result<(), String> {
        let p = sample_project();
        let options = ComplianceReportOptions {
            rule_sets: vec![RuleSetLine {
                id: "eu_market".into(),
                version: "1".into(),
            }],
            ..Default::default()
        };
        let a = compliance_report_markdown(&p, &options)?;
        let b = compliance_report_markdown(&p, &options)?;
        assert_eq!(a, b);
        assert!(!a.to_ascii_lowercase().contains("product is compliant"));
        assert!(a.contains("rev-a"));
        assert!(a.contains("DOC-9"));
        Ok(())
    }

    #[test]
    fn waivers_print_with_reasons() -> Result<(), String> {
        let p = sample_project();
        let options = ComplianceReportOptions {
            waivers: vec![WaiverLine {
                rule: "origin_check".into(),
                subject: "U1".into(),
                reason: "contract covers origin".into(),
            }],
            ..Default::default()
        };
        let text = compliance_report_markdown(&p, &options)?;
        assert!(text.contains("contract covers origin"));
        Ok(())
    }
}
