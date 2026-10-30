//! Stage 0 rule checks for publication bundles.

use crate::hash::{bundle_root_hash, sha256_hex};
use crate::licence::{lineage_compatible, spdx_allowed};
use crate::manifest::{PublicationManifest, SCHEMA_VERSION};
use crate::zip_read::{entries_map, read_stored_zip, ZipEntry};
use pcbcraft_doc::Project;
use pcbcraft_library::{
    check_footprint, check_symbol, read_footprint, read_symbols, FindingSeverity,
};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// Outcome of one Stage 0 rule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CheckVerdict {
    Pass,
    Fail(Vec<String>),
    Flag(Vec<String>),
}

impl CheckVerdict {
    fn fail(reason: impl Into<String>) -> Self {
        Self::Fail(vec![reason.into()])
    }

    fn fail_many(reasons: Vec<String>) -> Self {
        if reasons.is_empty() {
            Self::Pass
        } else {
            Self::Fail(reasons)
        }
    }

    fn flag(reason: impl Into<String>) -> Self {
        Self::Flag(vec![reason.into()])
    }
}

/// One named rule result from Stage 0.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckResult {
    pub rule: &'static str,
    pub verdict: CheckVerdict,
}

/// Full Stage 0 report for one bundle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stage0Report {
    pub results: Vec<CheckResult>,
}

impl Stage0Report {
    /// True when no rule failed.
    pub fn passes(&self) -> bool {
        !self
            .results
            .iter()
            .any(|r| matches!(r.verdict, CheckVerdict::Fail(_)))
    }

    /// All failure reasons across rules.
    pub fn failure_reasons(&self) -> Vec<String> {
        let mut out = Vec::new();
        for result in &self.results {
            if let CheckVerdict::Fail(reasons) = &result.verdict {
                for reason in reasons {
                    out.push(format!("{}: {reason}", result.rule));
                }
            }
        }
        out
    }
}

/// Controlled tag vocabulary for publications.
pub const CONTROLLED_TAGS: &[&str] = &[
    "design",
    "part",
    "footprint",
    "symbol",
    "library",
    "reference",
    "breakout",
    "shield",
    "adapter",
    "module",
    "test",
    "education",
];

/// Controlled role strings for authorship metadata.
pub const CONTROLLED_ROLES: &[&str] = &["publisher", "maintainer", "contributor", "reviewer"];

/// Run every Stage 0 check on a stored ZIP bundle.
pub fn stage0_from_zip(bytes: &[u8]) -> Stage0Report {
    let mut results = Vec::new();
    let entries = match read_stored_zip(bytes) {
        Ok(entries) => entries,
        Err(reason) => {
            results.push(CheckResult {
                rule: "archive",
                verdict: CheckVerdict::fail(reason),
            });
            return Stage0Report { results };
        }
    };
    let map = match entries_map(&entries) {
        Ok(map) => map,
        Err(reason) => {
            results.push(CheckResult {
                rule: "archive",
                verdict: CheckVerdict::fail(reason),
            });
            return Stage0Report { results };
        }
    };
    stage0_from_entries(&entries, &map, results)
}

/// Run Stage 0 when the manifest JSON and file map are already extracted.
pub fn stage0_from_manifest_and_files(
    manifest_text: &str,
    files: &BTreeMap<String, Vec<u8>>,
) -> Stage0Report {
    let entries: Vec<ZipEntry> = files
        .iter()
        .map(|(name, data)| ZipEntry {
            name: name.clone(),
            data: data.clone(),
        })
        .collect();
    let mut results = Vec::new();
    let manifest = match PublicationManifest::from_json(manifest_text) {
        Ok(m) => m,
        Err(reason) => {
            results.push(CheckResult {
                rule: "schema",
                verdict: CheckVerdict::fail(reason),
            });
            return Stage0Report { results };
        }
    };
    run_checks(&manifest, manifest_text, files, &entries, results)
}

fn stage0_from_entries(
    entries: &[ZipEntry],
    map: &BTreeMap<String, Vec<u8>>,
    mut results: Vec<CheckResult>,
) -> Stage0Report {
    let manifest_text = match map.get("publication.json") {
        Some(bytes) => match std::str::from_utf8(bytes) {
            Ok(text) => text,
            Err(_) => {
                results.push(CheckResult {
                    rule: "manifest",
                    verdict: CheckVerdict::fail("publication.json is not UTF-8"),
                });
                return Stage0Report { results };
            }
        },
        None => {
            results.push(CheckResult {
                rule: "manifest",
                verdict: CheckVerdict::fail("bundle has no publication.json"),
            });
            return Stage0Report { results };
        }
    };

    let manifest = match PublicationManifest::from_json(manifest_text) {
        Ok(m) => m,
        Err(reason) => {
            results.push(CheckResult {
                rule: "schema",
                verdict: CheckVerdict::fail(reason),
            });
            return Stage0Report { results };
        }
    };
    run_checks(&manifest, manifest_text, map, entries, results)
}

fn run_checks(
    manifest: &PublicationManifest,
    _manifest_text: &str,
    map: &BTreeMap<String, Vec<u8>>,
    entries: &[ZipEntry],
    mut results: Vec<CheckResult>,
) -> Stage0Report {
    results.push(check_schema_version(manifest));
    results.push(check_licences(manifest));
    results.push(check_lineage(manifest));
    results.push(check_tags_and_roles(manifest));
    results.push(check_urls(manifest));
    results.push(check_names(manifest));
    results.push(check_files_listed(manifest, map));
    results.push(check_path_rules(manifest));
    results.push(check_bundle_hash(manifest));
    results.push(check_media_types(manifest, map));
    results.push(check_duplicates(manifest, map));
    results.push(check_near_duplicates(map));
    results.push(check_library_files(manifest, map));
    results.push(check_project_file(manifest, map));
    results.push(check_bom_summary(manifest, map));
    results.push(check_orphan_files(manifest, map));
    results.push(check_zip_bomb(entries));
    Stage0Report { results }
}

fn check_schema_version(m: &PublicationManifest) -> CheckResult {
    let verdict = if m.schema_version == SCHEMA_VERSION {
        CheckVerdict::Pass
    } else {
        CheckVerdict::fail(format!(
            "schema_version must be {SCHEMA_VERSION}, not {}",
            m.schema_version
        ))
    };
    CheckResult {
        rule: "schema",
        verdict,
    }
}

fn check_licences(m: &PublicationManifest) -> CheckResult {
    let mut reasons = Vec::new();
    for (layer, value) in [
        ("hardware", &m.licences.hardware),
        ("software", &m.licences.software),
        ("documentation", &m.licences.documentation),
        ("models_3d", &m.licences.models_3d),
    ] {
        if value.trim().is_empty() {
            reasons.push(format!("licence for {layer} is required"));
        } else if !spdx_allowed(value) {
            reasons.push(format!("licence for {layer} is not on the allowlist: {value}"));
        }
    }
    CheckResult {
        rule: "licence",
        verdict: CheckVerdict::fail_many(reasons),
    }
}

fn check_lineage(m: &PublicationManifest) -> CheckResult {
    let mut reasons = Vec::new();
    for entry in &m.derived_from {
        if !spdx_allowed(&entry.parent_licence) {
            reasons.push(format!(
                "parent licence for {} is not allowed",
                entry.parent_id
            ));
        }
        for (layer, value) in [
            ("hardware", &m.licences.hardware),
            ("software", &m.licences.software),
            ("documentation", &m.licences.documentation),
            ("models_3d", &m.licences.models_3d),
        ] {
            if !lineage_compatible(&entry.parent_licence, value) {
                reasons.push(format!(
                    "child {layer} licence {value} is incompatible with parent {} licence {}",
                    entry.parent_id, entry.parent_licence
                ));
            }
        }
    }
    CheckResult {
        rule: "lineage",
        verdict: CheckVerdict::fail_many(reasons),
    }
}

fn check_tags_and_roles(m: &PublicationManifest) -> CheckResult {
    let mut reasons = Vec::new();
    for tag in &m.tags {
        if !CONTROLLED_TAGS.contains(&tag.as_str()) {
            reasons.push(format!("tag {tag} is not in the controlled vocabulary"));
        }
    }
    for role in &m.roles {
        if !CONTROLLED_ROLES.contains(&role.as_str()) {
            reasons.push(format!("role {role} is not in the controlled vocabulary"));
        }
    }
    CheckResult {
        rule: "tags",
        verdict: CheckVerdict::fail_many(reasons),
    }
}

fn check_urls(m: &PublicationManifest) -> CheckResult {
    let mut reasons = Vec::new();
    if let Some(url) = &m.identity.homepage {
        if !url.starts_with("https://") {
            reasons.push("identity.homepage must use https".into());
        }
    }
    if let Some(bom) = &m.bom_summary {
        for line in &bom.lines {
            if let Some(url) = &line.datasheet_url
                && !url.starts_with("https://")
            {
                reasons.push(format!(
                    "datasheet_url for {} must use https",
                    line.designator
                ));
            }
        }
    }
    for claim in &m.compliance_claims {
        if let Some(url) = &claim.evidence_url
            && !url.starts_with("https://")
        {
            reasons.push(format!(
                "evidence_url for {} must use https",
                claim.scheme
            ));
        }
    }
    CheckResult {
        rule: "urls",
        verdict: CheckVerdict::fail_many(reasons),
    }
}

fn check_names(m: &PublicationManifest) -> CheckResult {
    let mut reasons = Vec::new();
    if confusable_with_existing(&m.identity.id, &m.identity.name) {
        reasons.push("identity.id and identity.name are confusable".into());
    }
    if m.identity.id.chars().any(|c| c.is_whitespace()) {
        reasons.push("identity.id must not contain whitespace".into());
    }
    CheckResult {
        rule: "names",
        verdict: CheckVerdict::fail_many(reasons),
    }
}

fn confusable_with_existing(id: &str, name: &str) -> bool {
    let norm_id = normalize_confusable(id);
    let norm_name = normalize_confusable(name);
    norm_id == norm_name && id != name
}

fn normalize_confusable(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '\u{0430}' | '\u{0410}' => 'a',
            '\u{0435}' | '\u{0415}' => 'e',
            '\u{043e}' | '\u{041e}' => 'o',
            '\u{0440}' | '\u{0420}' => 'p',
            '\u{0441}' | '\u{0421}' => 'c',
            '\u{0445}' | '\u{0425}' => 'x',
            other if other.is_ascii_alphanumeric() => other.to_ascii_lowercase(),
            _ => other,
        })
        .collect()
}

fn check_files_listed(m: &PublicationManifest, map: &BTreeMap<String, Vec<u8>>) -> CheckResult {
    let mut reasons = Vec::new();
    for file in &m.files {
        match map.get(&file.path) {
            None => reasons.push(format!("{} is listed but missing from the bundle", file.path)),
            Some(bytes) => {
                if bytes.len() as u64 != file.size {
                    reasons.push(format!(
                        "{} size is {} in the manifest but {} in the bundle",
                        file.path,
                        file.size,
                        bytes.len()
                    ));
                }
                let hash = sha256_hex(bytes);
                if !file.sha256.eq_ignore_ascii_case(&hash) {
                    reasons.push(format!("{} sha256 does not match the bundle bytes", file.path));
                }
            }
        }
    }
    CheckResult {
        rule: "files",
        verdict: CheckVerdict::fail_many(reasons),
    }
}

fn check_path_rules(m: &PublicationManifest) -> CheckResult {
    let mut reasons = Vec::new();
    let mut seen = BTreeSet::new();
    for file in &m.files {
        if file.path.contains("..") || file.path.contains('/') || file.path.contains('\\') {
            reasons.push(format!("{} is not a flat bundle path", file.path));
        }
        if !seen.insert(&file.path) {
            reasons.push(format!("{} is listed more than once", file.path));
        }
    }
    CheckResult {
        rule: "paths",
        verdict: CheckVerdict::fail_many(reasons),
    }
}

fn check_bundle_hash(m: &PublicationManifest) -> CheckResult {
    let pairs: Vec<(String, String)> = m
        .files
        .iter()
        .map(|f| (f.path.clone(), f.sha256.to_ascii_lowercase()))
        .collect();
    let want = bundle_root_hash(&pairs);
    let verdict = if m.bundle_root_hash.eq_ignore_ascii_case(&want) {
        CheckVerdict::Pass
    } else {
        CheckVerdict::fail("bundle_root_hash does not match the file list")
    };
    CheckResult {
        rule: "content_address",
        verdict,
    }
}

fn sniff_media_type(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some("image/png");
    }
    if bytes.starts_with(b"{") || bytes.starts_with(b"[") {
        return Some("application/json");
    }
    if bytes.starts_with(b"(kicad_") || bytes.starts_with(b"(footprint") || bytes.starts_with(b"(symbol") {
        return Some("text/x-kicad");
    }
    if bytes.starts_with(b"PK\x03\x04") {
        return Some("application/zip");
    }
    if bytes.iter().all(|b| b.is_ascii()) {
        return Some("text/plain");
    }
    None
}

fn check_media_types(m: &PublicationManifest, map: &BTreeMap<String, Vec<u8>>) -> CheckResult {
    let mut reasons = Vec::new();
    for file in &m.files {
        let Some(bytes) = map.get(&file.path) else {
            continue;
        };
        let Some(sniffed) = sniff_media_type(bytes) else {
            reasons.push(format!("{} media type could not be verified", file.path));
            continue;
        };
        if file.media_type != sniffed {
            reasons.push(format!(
                "{} declares media_type {} but content looks like {sniffed}",
                file.path, file.media_type
            ));
        }
    }
    CheckResult {
        rule: "media_type",
        verdict: CheckVerdict::fail_many(reasons),
    }
}

fn check_duplicates(m: &PublicationManifest, map: &BTreeMap<String, Vec<u8>>) -> CheckResult {
    let mut by_hash: HashMap<String, Vec<String>> = HashMap::new();
    for file in &m.files {
        if let Some(bytes) = map.get(&file.path) {
            by_hash
                .entry(sha256_hex(bytes))
                .or_default()
                .push(file.path.clone());
        }
    }
    let mut reasons = Vec::new();
    for (hash, paths) in by_hash {
        if paths.len() > 1 {
            reasons.push(format!(
                "duplicate content at {} (hash {hash})",
                paths.join(", ")
            ));
        }
    }
    CheckResult {
        rule: "duplicates",
        verdict: CheckVerdict::fail_many(reasons),
    }
}

fn check_near_duplicates(map: &BTreeMap<String, Vec<u8>>) -> CheckResult {
    let mut sizes: HashMap<u64, Vec<String>> = HashMap::new();
    for (path, bytes) in map {
        sizes.entry(bytes.len() as u64).or_default().push(path.clone());
    }
    let mut flags = Vec::new();
    for (size, paths) in sizes {
        if paths.len() > 1 && size > 1024 {
            flags.push(format!(
                "near-duplicate size {size} bytes: {}",
                paths.join(", ")
            ));
        }
    }
    let verdict = if flags.is_empty() {
        CheckVerdict::Pass
    } else {
        CheckVerdict::Flag(flags)
    };
    CheckResult {
        rule: "near_duplicate",
        verdict,
    }
}

fn check_library_files(m: &PublicationManifest, map: &BTreeMap<String, Vec<u8>>) -> CheckResult {
    let mut reasons = Vec::new();
    for file in &m.files {
        let Some(bytes) = map.get(&file.path) else {
            continue;
        };
        let text = match std::str::from_utf8(bytes) {
            Ok(t) => t,
            Err(_) => continue,
        };
        if file.role == "symbol" || file.path.ends_with(".kicad_sym") {
            match read_symbols(text) {
                Ok(symbols) => {
                    for symbol in symbols {
                        for finding in check_symbol(&symbol) {
                            if finding.severity == FindingSeverity::Error {
                                reasons.push(format!(
                                    "{} symbol {}: {}",
                                    file.path, symbol.name, finding.message
                                ));
                            }
                        }
                    }
                }
                Err(e) => reasons.push(format!("{} symbol parse: {e}", file.path)),
            }
        }
        if file.role == "footprint" || file.path.ends_with(".kicad_mod") {
            match read_footprint(text) {
                Ok(fp) => {
                    for finding in check_footprint(&fp) {
                        if finding.severity == FindingSeverity::Error {
                            reasons.push(format!(
                                "{} footprint {}: {}",
                                file.path, fp.name, finding.message
                            ));
                        }
                    }
                }
                Err(e) => reasons.push(format!("{} footprint parse: {e}", file.path)),
            }
        }
    }
    CheckResult {
        rule: "library_readers",
        verdict: CheckVerdict::fail_many(reasons),
    }
}

fn check_project_file(m: &PublicationManifest, map: &BTreeMap<String, Vec<u8>>) -> CheckResult {
    let project_paths: Vec<&str> = m
        .files
        .iter()
        .filter(|f| f.role == "project" || f.path == "design.json")
        .map(|f| f.path.as_str())
        .collect();
    if project_paths.is_empty() {
        return CheckResult {
            rule: "project",
            verdict: CheckVerdict::Pass,
        };
    }
    let mut reasons = Vec::new();
    for path in project_paths {
        let Some(bytes) = map.get(path) else {
            continue;
        };
        let text = match std::str::from_utf8(bytes) {
            Ok(t) => t,
            Err(_) => {
                reasons.push(format!("{path} is not UTF-8"));
                continue;
            }
        };
        match serde_json::from_str::<Project>(text) {
            Ok(project) => {
                if let Err(e) = project.validate() {
                    reasons.push(format!("{path} project validation: {e}"));
                }
            }
            Err(e) => reasons.push(format!("{path} is not a valid project: {e}")),
        }
    }
    CheckResult {
        rule: "project",
        verdict: CheckVerdict::fail_many(reasons),
    }
}

fn check_bom_summary(m: &PublicationManifest, map: &BTreeMap<String, Vec<u8>>) -> CheckResult {
    let Some(bom) = &m.bom_summary else {
        return CheckResult {
            rule: "bom",
            verdict: CheckVerdict::Pass,
        };
    };
    let project_path = m
        .files
        .iter()
        .find(|f| f.role == "project" || f.path == "design.json")
        .map(|f| f.path.as_str());
    let Some(path) = project_path else {
        return CheckResult {
            rule: "bom",
            verdict: CheckVerdict::Pass,
        };
    };
    let Some(bytes) = map.get(path) else {
        return CheckResult {
            rule: "bom",
            verdict: CheckVerdict::Pass,
        };
    };
    let text = match std::str::from_utf8(bytes) {
        Ok(t) => t,
        Err(_) => {
            return CheckResult {
                rule: "bom",
                verdict: CheckVerdict::fail(format!("{path} is not UTF-8")),
            };
        }
    };
    let project: Project = match serde_json::from_str(text) {
        Ok(p) => p,
        Err(e) => {
            return CheckResult {
                rule: "bom",
                verdict: CheckVerdict::fail(format!("{path} is not a valid project: {e}")),
            };
        }
    };
    let mut reasons = Vec::new();
    let mut designators: BTreeSet<String> = BTreeSet::new();
    for component in &project.components {
        if component.in_bom() {
            designators.insert(component.reference.clone());
        }
    }
    for line in &bom.lines {
        if !designators.contains(&line.designator) {
            reasons.push(format!(
                "bom_summary lists {} which is not a BOM component in the project",
                line.designator
            ));
        }
    }
    CheckResult {
        rule: "bom",
        verdict: CheckVerdict::fail_many(reasons),
    }
}

fn check_orphan_files(m: &PublicationManifest, map: &BTreeMap<String, Vec<u8>>) -> CheckResult {
    let listed: BTreeSet<&str> = m.files.iter().map(|f| f.path.as_str()).collect();
    let mut reasons = Vec::new();
    for path in map.keys() {
        if path == "publication.json" {
            continue;
        }
        if !listed.contains(path.as_str()) {
            reasons.push(format!("{path} is in the bundle but not listed in the manifest"));
        }
    }
    CheckResult {
        rule: "orphan_files",
        verdict: CheckVerdict::fail_many(reasons),
    }
}

fn check_zip_bomb(entries: &[ZipEntry]) -> CheckResult {
    let count = entries.len();
    let verdict = if count > crate::zip_read::MAX_BUNDLE_ENTRIES {
        CheckVerdict::fail("too many archive entries")
    } else {
        CheckVerdict::Pass
    };
    CheckResult {
        rule: "archive_limits",
        verdict,
    }
}
