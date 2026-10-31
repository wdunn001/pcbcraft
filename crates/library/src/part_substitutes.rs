//! Alternates and second sources for one library part.
//!
//! Context: issue #719 (child of #699). Symbol and footprint files have no
//! room for substitute lists, so they live in the project library and in an
//! optional sidecar beside an on-disk symbol file.
//!
//! Local rules: bounded text, at most eight of each kind, no control
//! characters. A second source is a maker and part number pair. An alternate
//! is another project-library symbol name, optionally with a footprint name.
//!
//! # Dependencies
//! - `crates/doc/src/lib.rs` stores [`PartSubstitutes`] on [`ProjectLibrary`].
//!
//! # Do not
//! - Resolve or fetch parts here; callers validate names against the library.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Longest maker name, part number, symbol name, footprint name or note.
pub const MAX_SUBSTITUTE_TEXT: usize = 256;
/// Most second sources or alternates on one part.
pub const MAX_SUBSTITUTES_EACH: usize = 8;
/// Sidecar format token written as JSON.
pub const PART_SIDECAR_FORMAT: &str = "pcbcraft-part-sidecar";
/// Sidecar schema version.
pub const PART_SIDECAR_VERSION: u32 = 1;
/// Suffix for a sidecar file beside a symbol library file.
pub const PART_SIDECAR_SUFFIX: &str = ".pcbcraft-part.json";

fn bounded(label: &str, text: &str, max: usize) -> Result<(), String> {
    if text.is_empty() {
        return Err(format!("{label} must not be empty"));
    }
    if text.len() > max || text.chars().any(char::is_control) {
        return Err(format!("{label} is out of bounds"));
    }
    Ok(())
}

/// Another maker's number for the same design slot.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SecondSource {
    /// Maker of the substitute part.
    pub manufacturer: String,
    /// The maker's part number.
    pub mpn: String,
    /// Why this source is listed, in plain words.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
}

impl SecondSource {
    /// Check field bounds.
    pub fn validate(&self) -> Result<(), String> {
        bounded("manufacturer", &self.manufacturer, MAX_SUBSTITUTE_TEXT)?;
        bounded("mpn", &self.mpn, MAX_SUBSTITUTE_TEXT)?;
        if !self.note.is_empty() {
            bounded("note", &self.note, MAX_SUBSTITUTE_TEXT)?;
        }
        Ok(())
    }
}

/// Another library part that may be used instead of the primary one.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AlternatePart {
    /// Project-library symbol name of the substitute part.
    pub symbol: String,
    /// Footprint name when it differs from the alternate symbol's default.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub footprint: String,
    /// Why this alternate is listed, in plain words.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
}

impl AlternatePart {
    /// Check field bounds.
    pub fn validate(&self) -> Result<(), String> {
        bounded("symbol", &self.symbol, MAX_SUBSTITUTE_TEXT)?;
        if !self.footprint.is_empty() {
            bounded("footprint", &self.footprint, MAX_SUBSTITUTE_TEXT)?;
        }
        if !self.note.is_empty() {
            bounded("note", &self.note, MAX_SUBSTITUTE_TEXT)?;
        }
        Ok(())
    }
}

/// Second sources and drop-in alternates for one primary part.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PartSubstitutes {
    /// Other maker numbers for the same slot, in priority order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub second_sources: Vec<SecondSource>,
    /// Other library parts that may replace this one, in priority order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alternates: Vec<AlternatePart>,
}

impl PartSubstitutes {
    /// True when nothing is stored.
    pub fn is_empty(&self) -> bool {
        self.second_sources.is_empty() && self.alternates.is_empty()
    }

    /// Check counts and every entry.
    pub fn validate(&self) -> Result<(), String> {
        if self.second_sources.len() > MAX_SUBSTITUTES_EACH {
            return Err("a part may list at most eight second sources".into());
        }
        if self.alternates.len() > MAX_SUBSTITUTES_EACH {
            return Err("a part may list at most eight alternates".into());
        }
        for entry in &self.second_sources {
            entry.validate()?;
        }
        for entry in &self.alternates {
            entry.validate()?;
        }
        Ok(())
    }

    /// Add or replace a second source keyed by manufacturer, ignoring case.
    pub fn set_second_source(
        &mut self,
        manufacturer: &str,
        mpn: Option<&str>,
        note: Option<&str>,
    ) -> Result<(), String> {
        bounded("manufacturer", manufacturer, MAX_SUBSTITUTE_TEXT)?;
        if let Some(note) = note.filter(|s| !s.is_empty()) {
            bounded("note", note, MAX_SUBSTITUTE_TEXT)?;
        }
        let Some(mpn) = mpn else {
            let upper = manufacturer.to_ascii_uppercase();
            let before = self.second_sources.len();
            self.second_sources
                .retain(|s| s.manufacturer.to_ascii_uppercase() != upper);
            if before == self.second_sources.len() {
                return Err("that second source is not on the list".into());
            }
            return Ok(());
        };
        bounded("mpn", mpn, MAX_SUBSTITUTE_TEXT)?;
        let entry = SecondSource {
            manufacturer: manufacturer.to_string(),
            mpn: mpn.to_string(),
            note: note.unwrap_or("").to_string(),
        };
        entry.validate()?;
        if let Some(existing) = self
            .second_sources
            .iter_mut()
            .find(|s| s.manufacturer.eq_ignore_ascii_case(manufacturer))
        {
            *existing = entry;
        } else {
            if self.second_sources.len() >= MAX_SUBSTITUTES_EACH {
                return Err("a part may list at most eight second sources".into());
            }
            self.second_sources.push(entry);
        }
        Ok(())
    }

    /// Add or replace an alternate keyed by symbol name.
    pub fn set_alternate(
        &mut self,
        symbol: &str,
        footprint: Option<&str>,
        note: Option<&str>,
        symbol_exists: impl Fn(&str) -> bool,
        footprint_exists: impl Fn(&str) -> bool,
    ) -> Result<(), String> {
        bounded("symbol", symbol, MAX_SUBSTITUTE_TEXT)?;
        if let Some(note) = note.filter(|s| !s.is_empty()) {
            bounded("note", note, MAX_SUBSTITUTE_TEXT)?;
        }
        if footprint.is_some_and(|s| s.is_empty()) {
            return Err("footprint must not be empty when given".into());
        }
        if let Some(footprint) = footprint.filter(|s| !s.is_empty()) {
            bounded("footprint", footprint, MAX_SUBSTITUTE_TEXT)?;
            if !footprint_exists(footprint) {
                return Err(format!("no footprint named {footprint} in the project library"));
            }
        }
        if !symbol_exists(symbol) {
            return Err(format!("no symbol named {symbol} in the project library"));
        }
        let entry = AlternatePart {
            symbol: symbol.to_string(),
            footprint: footprint.unwrap_or("").to_string(),
            note: note.unwrap_or("").to_string(),
        };
        entry.validate()?;
        if let Some(existing) = self.alternates.iter_mut().find(|a| a.symbol == symbol) {
            *existing = entry;
        } else {
            if self.alternates.len() >= MAX_SUBSTITUTES_EACH {
                return Err("a part may list at most eight alternates".into());
            }
            self.alternates.push(entry);
        }
        Ok(())
    }

    /// Drop an alternate by symbol name.
    pub fn remove_alternate(&mut self, symbol: &str) -> Result<(), String> {
        let before = self.alternates.len();
        self.alternates.retain(|a| a.symbol != symbol);
        if before == self.alternates.len() {
            return Err("that alternate is not on the list".into());
        }
        Ok(())
    }
}

/// On-disk sidecar envelope.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct PartSidecarFile {
    format: String,
    version: u32,
    #[serde(flatten)]
    substitutes: PartSubstitutes,
}

/// Path of the sidecar file for a symbol library file path.
pub fn sidecar_path_for_symbol(symbol_library_path: &Path) -> PathBuf {
    let file_name = symbol_library_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("part");
    let stem = Path::new(file_name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(file_name);
    symbol_library_path
        .with_file_name(format!("{stem}{PART_SIDECAR_SUFFIX}"))
}

/// Read a sidecar file from disk.
pub fn read_sidecar(path: &Path) -> Result<PartSubstitutes, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if text.len() > 256 * 1024 {
        return Err("part sidecar exceeds 256 KiB".into());
    }
    let file: PartSidecarFile =
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    if file.format != PART_SIDECAR_FORMAT {
        return Err("part sidecar has the wrong format token".into());
    }
    if file.version != PART_SIDECAR_VERSION {
        return Err("part sidecar version is not supported".into());
    }
    file.substitutes.validate()?;
    Ok(file.substitutes)
}

/// Write a sidecar file; removes the file when the record is empty.
pub fn write_sidecar(path: &Path, substitutes: &PartSubstitutes) -> Result<(), String> {
    substitutes.validate()?;
    if substitutes.is_empty() {
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("{}: {e}", path.display())),
        }
    } else {
        let file = PartSidecarFile {
            format: PART_SIDECAR_FORMAT.into(),
            version: PART_SIDECAR_VERSION,
            substitutes: substitutes.clone(),
        };
        let text = serde_json::to_string_pretty(&file).map_err(|e| e.to_string())?;
        if text.len() > 256 * 1024 {
            return Err("part sidecar exceeds 256 KiB".into());
        }
        std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn second_sources_replace_by_manufacturer_ignoring_case() -> Result<(), String> {
        let mut subs = PartSubstitutes::default();
        subs.set_second_source("Yageo", Some("A"), None)?;
        subs.set_second_source("YAGEO", Some("B"), Some("preferred"))?;
        assert_eq!(subs.second_sources.len(), 1);
        assert_eq!(subs.second_sources[0].mpn, "B");
        subs.set_second_source("Yageo", None, None)?;
        assert!(subs.is_empty());
        Ok(())
    }

    #[test]
    fn sidecar_roundtrip_and_empty_removes_file() -> Result<(), String> {
        let dir = tempdir().map_err(|e| e.to_string())?;
        let symbol = dir.path().join("parts.libsym");
        let sidecar = sidecar_path_for_symbol(&symbol);
        let mut subs = PartSubstitutes::default();
        subs.set_second_source("Acme", Some("123"), None)?;
        write_sidecar(&sidecar, &subs)?;
        let loaded = read_sidecar(&sidecar)?;
        assert_eq!(loaded, subs);
        write_sidecar(&sidecar, &PartSubstitutes::default())?;
        assert!(
            !sidecar.exists(),
            "an empty record deletes the sidecar file"
        );
        Ok(())
    }
}
