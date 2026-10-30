//! Export package records: a package is data, not code.
//!
//! Context: export packages epic (#214); the record shape and validation live here (#224).
//! A package names the
//! files one recipient asks for (a board maker, an assembler, a single-file exchange
//! format) as a list of contents, each with a writer format, a few options and a file
//! name template. The records ship as a bundled data file (`crates/io/data/packages.json`)
//! and a project may save its own in `Project::packages`.
//!
//! Local rules:
//! - Records carry no behaviour. Which `format` ids exist, and which are not written yet,
//!   is decided in `crates/io/src/export_package.rs`.
//! - A template is a relative file name with tokens in braces; it never holds a folder,
//!   `..`, a drive or a dot-first name.
//!
//! # Dependencies
//! - crates/doc/src/lib.rs (`OUTPUT_KINDS`, `Project::validate`)
//! - crates/io/src/export_package.rs (token values, writers)
//!
//! # Do not
//! - Add a field a writer cannot honour; an option the writer does not know is refused there.
use crate::OUTPUT_KINDS;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Most contents one package lists.
pub const MAX_PACKAGE_CONTENTS: usize = 64;
/// Most packages a project saves.
pub const MAX_PROJECT_PACKAGES: usize = 64;
/// Tokens a name template may hold.
pub const PACKAGE_TOKENS: [&str; 7] = ["project", "rev", "date", "variant", "side", "layer", "ext"];
/// The containers a package may write: one ZIP, a plain folder, or one file.
pub const PACKAGE_CONTAINERS: [&str; 3] = ["zip", "folder", "file"];

/// One content of a package: a writer format, its options and the file name template.
///
/// # Rules
/// - `kind` is one of `OUTPUT_KINDS`; `format` is checked against the writer table in io.
///
/// # Do not
/// - Put a folder in `name`; the package container owns the folder.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PackageContent {
    /// The existing output kind this content belongs to.
    pub kind: String,
    /// The writer format id, for example `gerber_layers` or `bom_csv`.
    pub format: String,
    /// Writer options as text, for example `naming` = `protel`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub options: BTreeMap<String, String>,
    /// File name template. One content may write several files (one per layer).
    pub name: String,
}

/// A named bundle of outputs for one recipient.
///
/// # Rules
/// - `container` is `zip` (needs `archive`), `folder` or `file` (one content only);
///   `archive` is empty for the last two.
/// - Ids are lower case letters, digits and underscores.
///
/// # Dependencies
/// - crates/io/src/export_package.rs expands the templates and writes it.
///
/// # Do not
/// - Store a path or a host name; the destination is chosen when it runs.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PackageDef {
    /// Stable id.
    pub id: String,
    /// Short English name shown in Export as.
    pub name: String,
    /// Who it is for, in plain words (the tooltip).
    pub audience: String,
    /// `zip`, `folder` or `file`.
    pub container: String,
    /// ZIP file name template; empty for `folder` and `file`.
    #[serde(default)]
    pub archive: String,
    /// True for a package whose writer is not validated against a viewer or schema yet.
    #[serde(default, skip_serializing_if = "is_false")]
    pub experimental: bool,
    /// What it writes, in order.
    pub files: Vec<PackageContent>,
}

fn is_false(v: &bool) -> bool {
    !*v
}

/// What one package export last wrote: the design revision and a hash of the bytes, so the
/// Manufacture list can show when it is stale.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PackageRun {
    /// The package id.
    pub package: String,
    /// Design revision at export.
    pub revision: u64,
    /// SHA-256 of the written bytes, 64 hex digits.
    pub hash: String,
}

/// Checks a name template: bounded, braces balanced, tokens known, and a plain relative
/// file name (no folder, no `..`, no drive, no leading dot, no control characters).
pub fn check_template(template: &str) -> Result<(), String> {
    if template.is_empty() || template.len() > 128 {
        return Err("a name template holds 1 to 128 bytes".into());
    }
    if template.starts_with('.') || template.contains("..") {
        return Err(format!(
            "{template}: a name may not start with a dot or hold .."
        ));
    }
    if template.chars().any(|c| {
        c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
    }) {
        return Err(format!(
            "{template}: a name holds a character files may not"
        ));
    }
    let mut rest = template;
    while let Some(open) = rest.find(['{', '}']) {
        if rest.as_bytes()[open] == b'}' {
            return Err(format!("{template}: a closing brace has no opening brace"));
        }
        let after = &rest[open + 1..];
        let close = after
            .find('}')
            .ok_or_else(|| format!("{template}: a brace is not closed"))?;
        let token = &after[..close];
        if !PACKAGE_TOKENS.contains(&token) {
            return Err(format!("{template}: unknown token {{{token}}}"));
        }
        rest = &after[close + 1..];
    }
    Ok(())
}

impl PackageDef {
    /// Bounded validation of the record itself (writer ids are checked in io).
    pub fn validate(&self) -> Result<(), String> {
        let id_ok = !self.id.is_empty()
            && self.id.len() <= 32
            && self
                .id
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
        if !id_ok {
            return Err("a package id is 1 to 32 lower case letters, digits or underscores".into());
        }
        if self.name.trim().is_empty() || self.name.len() > 64 {
            return Err("a package name holds 1 to 64 bytes".into());
        }
        if self.audience.len() > 256 {
            return Err("a package audience holds at most 256 bytes".into());
        }
        if !PACKAGE_CONTAINERS.contains(&self.container.as_str()) {
            return Err(format!(
                "{}: container must be zip, folder or file",
                self.id
            ));
        }
        if self.files.is_empty() || self.files.len() > MAX_PACKAGE_CONTENTS {
            return Err(format!(
                "{}: a package lists 1 to {MAX_PACKAGE_CONTENTS} contents",
                self.id
            ));
        }
        match self.container.as_str() {
            "zip" => check_template(&self.archive)?,
            _ if !self.archive.is_empty() => {
                return Err(format!(
                    "{}: only a zip package has an archive name",
                    self.id
                ));
            }
            _ => {}
        }
        if self.container == "file" && self.files.len() != 1 {
            return Err(format!(
                "{}: a file package holds exactly one content",
                self.id
            ));
        }
        for c in &self.files {
            if !OUTPUT_KINDS.contains(&c.kind.as_str()) {
                return Err(format!("{}: unknown output kind {}", self.id, c.kind));
            }
            if c.format.is_empty() || c.format.len() > 48 {
                return Err(format!("{}: a format id holds 1 to 48 bytes", self.id));
            }
            if c.options.len() > 16
                || c.options
                    .iter()
                    .any(|(k, v)| k.is_empty() || k.len() > 32 || v.len() > 64)
            {
                return Err(format!("{}: options are out of range", self.id));
            }
            check_template(&c.name)?;
        }
        Ok(())
    }
}

/// Validates a list of packages saved in a project: bounded, valid, unique ids.
pub fn validate_packages(list: &[PackageDef]) -> Result<(), String> {
    if list.len() > MAX_PROJECT_PACKAGES {
        return Err(format!(
            "a project saves at most {MAX_PROJECT_PACKAGES} packages"
        ));
    }
    let mut seen = std::collections::BTreeSet::new();
    for d in list {
        d.validate()?;
        if !seen.insert(d.id.as_str()) {
            return Err(format!("two packages share the id {}", d.id));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def() -> PackageDef {
        PackageDef {
            id: "mine".into(),
            name: "Mine".into(),
            audience: "me".into(),
            container: "zip".into(),
            archive: "{project}-{rev}.zip".into(),
            experimental: false,
            files: vec![PackageContent {
                kind: "gerbers".into(),
                format: "gerber_layers".into(),
                options: BTreeMap::new(),
                name: "{project}.{ext}".into(),
            }],
        }
    }

    #[test]
    fn templates_refuse_folders_dots_and_unknown_tokens() {
        assert!(check_template("{project}-{rev}.zip").is_ok());
        for bad in [
            "", "a/b", "../x", "/abs", "C:x", ".hid", "{nope}", "{project", "x}", "a\\b",
        ] {
            assert!(check_template(bad).is_err(), "{bad} should be refused");
        }
    }

    #[test]
    fn a_record_is_checked_and_round_trips() -> Result<(), String> {
        let d = def();
        d.validate()?;
        let text = serde_json::to_string(&d).map_err(|e| e.to_string())?;
        let back: PackageDef = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        assert_eq!(d, back);
        let mut bad = def();
        bad.files[0].kind = "nope".into();
        assert!(bad.validate().is_err());
        let mut bad = def();
        bad.container = "folder".into();
        assert!(
            bad.validate().is_err(),
            "a folder package has no archive name"
        );
        let mut dup = vec![def(), def()];
        assert!(validate_packages(&dup).is_err());
        dup.pop();
        validate_packages(&dup)
    }
}
