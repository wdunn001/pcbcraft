//! Fabrication package: one stored ZIP holding every file an export run
//! produced, with a manifest that names each one.
//!
//! The manifest is `manifest.json`, always the first entry in the archive. It
//! records the project name, the design revision, the unit and the application
//! version once, then one record per file: its name, size in bytes, CRC-32,
//! the export kind and the output job that produced it. A Gerber or drill file
//! also records the file function and polarity it declares in its own header,
//! so the manifest repeats the layer map rather than keeping a second list
//! that could drift from the files.
//!
//! Nothing in the package is stamped with the time it was built. The job file
//! is written with a fixed creation date and every ZIP entry carries the
//! format's minimum stamp, so the same design always yields the same bytes.
use crate::gerber_job;
use pcbcraft_doc::Project;
use serde_json::{Value, json};
use std::collections::BTreeSet;

/// Name of the manifest entry, first in every package.
pub const MANIFEST_NAME: &str = "manifest.json";
/// Plain-text revision notes written into every fabrication package.
pub const REVISION_NOTES_NAME: &str = "REVISION-NOTES.txt";
/// Most files one package may hold, the manifest included.
pub const MAX_PACKAGE_FILES: usize = 256;
/// Creation date recorded in the Gerber job file of a package. A package
/// carries no build timestamp, so two runs over the same design produce the
/// same bytes; the value is the earliest date a ZIP entry can hold.
pub const PACKAGE_CREATION_DATE: &str = "1980-01-01T00:00:00+00:00";

/// One rendered file waiting to be packaged, with the export kind that
/// produced it.
#[derive(Clone)]
pub struct PackageEntry {
    pub name: String,
    pub kind: String,
    pub bytes: Vec<u8>,
}

/// The manifest of a package, as JSON text. `job` is the output job the files
/// came from and `variant` the bill-of-materials variant, when one was chosen.
pub fn package_manifest(
    p: &Project,
    job: &str,
    variant: Option<&str>,
    entries: &[PackageEntry],
) -> Result<String, String> {
    p.validate()?;
    let job = job.trim();
    if job.is_empty() || job.len() > 64 {
        return Err("a package names its output job in 1 to 64 bytes".into());
    }
    if let Some(name) = variant
        && (name.is_empty() || name.len() > 64)
    {
        return Err("a package names its variant in 1 to 64 bytes".into());
    }
    check_entries(entries)?;
    // The drill files name the copper span they drill, so the copper count is
    // read from the layers before any file is described.
    let copper = entries
        .iter()
        .filter(|e| {
            e.name.ends_with(".gbr")
                && gerber_job::file_function(&e.name, &text_of(e), 1)
                    .0
                    .starts_with("Copper,")
        })
        .count()
        .max(1);
    let mut files: Vec<Value> = Vec::new();
    let mut total = 0usize;
    for entry in entries.iter().filter(|e| e.name != MANIFEST_NAME) {
        total += entry.bytes.len();
        let mut record = json!({
            "name": entry.name,
            "bytes": entry.bytes.len(),
            "crc32": format!("{:08x}", crate::crc32(&entry.bytes)),
            "kind": entry.kind,
            "job": job,
        });
        // Mixed-case extensions are not written today, but a record is only
        // useful when it matches the file, so compare case-insensitively.
        let lower = entry.name.to_ascii_lowercase();
        if lower.ends_with(".gbr") || lower.ends_with(".drl") {
            let (function, polarity) =
                gerber_job::file_function(&entry.name, &text_of(entry), copper);
            if let Some(map) = record.as_object_mut() {
                map.insert("function".into(), json!(function));
                map.insert("polarity".into(), json!(polarity));
            }
        }
        files.push(record);
    }
    let manifest = json!({
        "format": "pcbcraft-fabrication-package",
        "version": 1,
        "application": {
            "name": "PCBCraft",
            "version": env!("CARGO_PKG_VERSION"),
        },
        "project": {
            "name": p.name,
            "document_format": p.format,
            "document_version": p.version,
            "revision": p.meta.get("revision"),
            "unit": p.unit,
        },
        "job": job,
        "variant": variant,
        "files": files,
        "total_bytes": total,
    });
    serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())
}

/// A whole package: the manifest first, then every file sorted by name, in one
/// stored ZIP with no subfolders.
pub fn package_zip(
    p: &Project,
    job: &str,
    variant: Option<&str>,
    entries: &[PackageEntry],
) -> Result<Vec<u8>, String> {
    let mut sorted: Vec<PackageEntry> = entries
        .iter()
        .filter(|e| e.name != MANIFEST_NAME)
        .cloned()
        .collect();
    sorted.sort_by(|a, b| a.name.cmp(&b.name));
    let manifest = package_manifest(p, job, variant, &sorted)?;
    let mut files: Vec<(String, Vec<u8>)> =
        vec![(MANIFEST_NAME.to_string(), manifest.into_bytes())];
    for entry in sorted {
        files.push((entry.name, entry.bytes));
    }
    crate::zip_entries(&files)
}

/// Text of one entry for the attribute reader. Packaging only reads files
/// this crate writes, so a lossy read of a non-text entry is harmless.
fn text_of(entry: &PackageEntry) -> String {
    String::from_utf8_lossy(&entry.bytes).into_owned()
}

/// Refuse a package whose file list could not be read back: no entries, too
/// many entries, names out of range, names with a folder in them, or two files
/// of the same name.
fn check_entries(entries: &[PackageEntry]) -> Result<(), String> {
    if entries.is_empty() {
        return Err("a package holds at least one file".into());
    }
    if entries.len() + 1 > MAX_PACKAGE_FILES {
        return Err(format!(
            "a package holds at most {} files, the manifest included",
            MAX_PACKAGE_FILES
        ));
    }
    let mut seen: Vec<&str> = Vec::with_capacity(entries.len());
    for entry in entries {
        if entry.name.is_empty() || entry.name.len() > 255 {
            return Err("package file names are 1 to 255 bytes".into());
        }
        if entry.name.contains('/') || entry.name.contains('\\') {
            return Err(format!("{} carries a folder in its name", entry.name));
        }
        if entry.name == MANIFEST_NAME {
            return Err(format!("{MANIFEST_NAME} is written by the package itself"));
        }
        if entry.kind.is_empty() || entry.kind.len() > 64 {
            return Err("package entries name their export kind in 1 to 64 bytes".into());
        }
        if seen.contains(&entry.name.as_str()) {
            return Err(format!("two exports write {}", entry.name));
        }
        seen.push(&entry.name);
    }
    Ok(())
}

/// Every entry of a stored ZIP this crate writes, in archive order.
pub fn package_stored_zip_entries(zip: &[u8]) -> Result<Vec<(String, Vec<u8>)>, String> {
    let mut entries = Vec::new();
    let mut at = 0usize;
    while at + 30 <= zip.len() && &zip[at..at + 4] == b"PK\x03\x04" {
        let read = |offset: usize| -> usize {
            u32::from_le_bytes([
                zip[at + offset],
                zip[at + offset + 1],
                zip[at + offset + 2],
                zip[at + offset + 3],
            ]) as usize
        };
        let (size, name_len, extra_len) = (read(18), read(26) & 0xffff, read(28) & 0xffff);
        let name_at = at + 30;
        if name_at + name_len > zip.len() {
            return Err("a package entry name runs past the archive".into());
        }
        let name = String::from_utf8_lossy(&zip[name_at..name_at + name_len]).into_owned();
        let data_at = name_at + name_len + extra_len;
        if data_at + size > zip.len() {
            return Err(format!("{name} runs past the archive"));
        }
        entries.push((name, zip[data_at..data_at + size].to_vec()));
        at = data_at + size;
    }
    if entries.is_empty() {
        return Err("the archive holds no entries".into());
    }
    Ok(entries)
}

/// Read the bill of materials CSV from a prior package zip, when one was included.
pub fn previous_bom_from_package(zip: &[u8]) -> Result<Option<String>, String> {
    for (name, bytes) in package_stored_zip_entries(zip)? {
        if name == "bom.csv" {
            return Ok(Some(String::from_utf8_lossy(&bytes).into_owned()));
        }
    }
    Ok(None)
}

fn csv_row_fields(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        if in_quotes {
            if ch == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    in_quotes = false;
                }
            } else {
                field.push(ch);
            }
        } else if ch == '"' {
            in_quotes = true;
        } else if ch == ',' {
            fields.push(field.clone());
            field.clear();
        } else {
            field.push(ch);
        }
    }
    fields.push(field);
    fields
}

/// Designator references listed in a bill of materials CSV this crate writes.
pub fn bom_references_from_csv(csv: &str) -> Result<BTreeSet<String>, String> {
    let mut lines = csv.lines();
    let header = lines.next().ok_or("a bill of materials needs a header row")?;
    let columns = csv_row_fields(header);
    let references_at = columns
        .iter()
        .position(|c| c == "references")
        .ok_or("a bill of materials needs a references column")?;
    let mut refs = BTreeSet::new();
    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        let fields = csv_row_fields(line);
        let Some(cell) = fields.get(references_at) else {
            continue;
        };
        for reference in cell.split(',') {
            let reference = reference.trim();
            if !reference.is_empty() {
                refs.insert(reference.to_string());
            }
        }
    }
    Ok(refs)
}

fn copper_layer_count(p: &Project) -> usize {
    let recorded = p
        .board
        .layer_records
        .iter()
        .filter(|l| l.kind == "copper")
        .count();
    if recorded > 0 {
        recorded
    } else {
        2
    }
}

fn meta_or_not_recorded(p: &Project, key: &str) -> String {
    p.meta
        .get(key)
        .filter(|v| !v.is_empty())
        .cloned()
        .unwrap_or_else(|| "(not recorded)".into())
}

/// Deterministic revision notes for a fabrication package.
pub fn package_revision_notes(
    p: &Project,
    job: &str,
    variant: Option<&str>,
    file_names: &[String],
    previous_bom_csv: Option<Option<&str>>,
) -> Result<String, String> {
    p.validate()?;
    let title = if p.name.trim().is_empty() {
        "untitled board".to_string()
    } else {
        p.name.trim().to_string()
    };
    let variant_label = variant
        .filter(|v| !v.is_empty())
        .unwrap_or("(default)");
    let mut out = format!(
        "Project: {title}\nRevision: {}\nDate: {}\nVariant: {variant_label}\nJob: {job}\nLayer count: {}\nBoard thickness: {:.3} mm\nSurface finish: {}\n\nFiles:\n",
        meta_or_not_recorded(p, "revision"),
        meta_or_not_recorded(p, "date"),
        copper_layer_count(p),
        p.board.thickness,
        p.board.finish,
    );
    for name in file_names {
        out.push_str(name);
        out.push('\n');
    }
    out.push_str("\nChanges since last export:\n");
    let previous_refs = match previous_bom_csv {
        None => {
            out.push_str("  (no previous export was supplied)\n");
            return Ok(out);
        }
        Some(None) => BTreeSet::new(),
        Some(Some(csv)) => bom_references_from_csv(csv)?,
    };
    let current = crate::bom_references(p, variant)?;
    let added: Vec<&str> = current
        .difference(&previous_refs)
        .map(String::as_str)
        .collect();
    let removed: Vec<&str> = previous_refs
        .difference(&current)
        .map(String::as_str)
        .collect();
    if added.is_empty() && removed.is_empty() {
        out.push_str("  (none)\n");
    } else {
        if !added.is_empty() {
            out.push_str("  Added parts: ");
            out.push_str(&added.join(", "));
            out.push('\n');
        }
        if !removed.is_empty() {
            out.push_str("  Removed parts: ");
            out.push_str(&removed.join(", "));
            out.push('\n');
        }
    }
    Ok(out)
}

/// Append revision notes to the entries about to be zipped.
pub fn attach_revision_notes_entry(
    p: &Project,
    job: &str,
    variant: Option<&str>,
    entries: &mut Vec<PackageEntry>,
    previous_bom_csv: Option<Option<&str>>,
) -> Result<(), String> {
    if entries.iter().any(|e| e.name == REVISION_NOTES_NAME) {
        return Err(format!("{REVISION_NOTES_NAME} is written by the package itself"));
    }
    let mut names: Vec<String> = entries.iter().map(|e| e.name.clone()).collect();
    names.push(REVISION_NOTES_NAME.into());
    names.sort();
    let text = package_revision_notes(p, job, variant, &names, previous_bom_csv)?;
    entries.push(PackageEntry {
        name: REVISION_NOTES_NAME.into(),
        kind: "notes".into(),
        bytes: text.into_bytes(),
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn board() -> Project {
        Project {
            name: "Package test".into(),
            library_table: Vec::new(),
            padstacks: Vec::new(),
            ..Project::default()
        }
    }

    fn entry(name: &str, kind: &str, text: &str) -> PackageEntry {
        PackageEntry {
            name: name.into(),
            kind: kind.into(),
            bytes: text.as_bytes().to_vec(),
        }
    }

    #[test]
    fn manifest_names_every_file_with_its_size_crc_kind_and_job() -> Result<(), String> {
        let project = board();
        let entries = vec![
            entry(
                "pcb-F.Cu.gbr",
                "gerbers",
                "%TF.FileFunction,Copper,L1,Top*%\n%TF.FilePolarity,Positive*%\nM02*\n",
            ),
            entry(
                "pcb-PTH.drl",
                "gerbers",
                "; #@! TF.FileFunction,Plated,1,2,PTH\nM48\n",
            ),
            entry("bom.csv", "bom", "Comment,Designator\n"),
        ];
        let manifest: Value =
            serde_json::from_str(&package_manifest(&project, "fab", None, &entries)?)
                .map_err(|e| e.to_string())?;
        let files = manifest["files"]
            .as_array()
            .ok_or("the manifest lists its files")?;
        assert_eq!(files.len(), 3, "every file is listed once");
        assert_eq!(manifest["project"]["name"], "Package test");
        assert_eq!(manifest["job"], "fab");
        let copper = files
            .iter()
            .find(|f| f["name"] == "pcb-F.Cu.gbr")
            .ok_or("the copper layer is listed")?;
        assert_eq!(copper["kind"], "gerbers");
        assert_eq!(copper["job"], "fab");
        assert_eq!(copper["function"], "Copper,L1,Top");
        assert_eq!(copper["polarity"], "Positive");
        assert_eq!(
            copper["crc32"],
            format!(
                "{:08x}",
                crate::crc32(
                    b"%TF.FileFunction,Copper,L1,Top*%\n%TF.FilePolarity,Positive*%\nM02*\n"
                )
            )
        );
        let drill = files
            .iter()
            .find(|f| f["name"] == "pcb-PTH.drl")
            .ok_or("the drill file is listed")?;
        assert_eq!(
            drill["function"], "Plated,1,2,PTH",
            "the drill span follows the copper count"
        );
        assert_eq!(
            drill["bytes"],
            entries[1].bytes.len(),
            "the size is the file size"
        );
        let bom = files
            .iter()
            .find(|f| f["name"] == "bom.csv")
            .ok_or("the bill is listed")?;
        assert!(
            bom.get("function").is_none(),
            "a bill of materials declares no fabrication function"
        );
        Ok(())
    }

    #[test]
    fn package_zip_puts_the_manifest_first_and_the_files_in_name_order() -> Result<(), String> {
        let project = board();
        let entries = vec![
            entry(
                "pcb-Edge.gbr",
                "gerbers",
                "%TF.FileFunction,Profile,NP*%\nM02*\n",
            ),
            entry("bom.csv", "bom", "Comment\n"),
        ];
        let zip = package_zip(&project, "fab", Some("lite"), &entries)?;
        assert!(zip.starts_with(b"PK\x03\x04"), "a stored ZIP");
        let text = String::from_utf8_lossy(&zip).into_owned();
        let manifest_at = text.find(MANIFEST_NAME).ok_or("the manifest is present")?;
        let bom_at = text.find("bom.csv").ok_or("the bill is present")?;
        let edge_at = text.find("pcb-Edge.gbr").ok_or("the outline is present")?;
        assert!(
            manifest_at < bom_at && bom_at < edge_at,
            "the manifest comes first, then the files by name"
        );
        assert!(
            text.contains("\"variant\": \"lite\""),
            "the variant is named"
        );
        assert!(
            !text.contains("1970") && !text.contains("2026-"),
            "a package carries no build timestamp"
        );
        Ok(())
    }

    #[test]
    fn revision_notes_are_stable_and_list_part_changes_from_a_prior_bill() -> Result<(), String> {
        let mut project = board();
        project.meta.insert("revision".into(), "B".into());
        project.meta.insert("date".into(), "2026-03-01".into());
        let part = |id: u64, reference: &str, value: &str, kind: &str| pcbcraft_doc::Component {
            sheet: 0,
            id,
            reference: reference.into(),
            value: value.into(),
            kind: kind.into(),
            position: pcbcraft_geom::Point::new(1.0, 1.0),
            rotation: 0.0,
            pins: Vec::new(),
            ..Default::default()
        };
        project.components.push(part(1, "R1", "10k", "resistor"));
        project.components.push(part(2, "C1", "100n", "capacitor"));
        project.validate()?;
        let previous_bom = crate::bom_csv(&project, &[], None)?;
        project.components.push(part(3, "R2", "1k", "resistor"));
        project.components.retain(|c| c.reference != "C1");
        project.validate()?;
        let names = vec!["bom.csv".into(), REVISION_NOTES_NAME.into()];
        let once = package_revision_notes(&project, "fab", None, &names, Some(Some(&previous_bom)))?;
        let again =
            package_revision_notes(&project, "fab", None, &names, Some(Some(&previous_bom)))?;
        assert_eq!(once, again, "identical input gives identical notes");
        assert!(once.contains("Revision: B"));
        assert!(once.contains("Date: 2026-03-01"));
        assert!(once.contains("Added parts: R2"));
        assert!(once.contains("Removed parts: C1"));
        let no_previous =
            package_revision_notes(&project, "fab", None, &names, None)?;
        assert!(no_previous.contains("(no previous export was supplied)"));
        Ok(())
    }

    #[test]
    fn package_refuses_an_empty_list_a_duplicate_name_and_a_folder() -> Result<(), String> {
        let project = board();
        assert!(package_manifest(&project, "fab", None, &[]).is_err());
        let twice = vec![entry("bom.csv", "bom", "a"), entry("bom.csv", "bom", "b")];
        assert!(
            package_manifest(&project, "fab", None, &twice).is_err(),
            "two exports cannot write one name"
        );
        let folder = vec![entry("files/bom.csv", "bom", "a")];
        assert!(package_manifest(&project, "fab", None, &folder).is_err());
        let many: Vec<PackageEntry> = (0..MAX_PACKAGE_FILES)
            .map(|n| entry(&format!("f{n}.txt"), "report", "x"))
            .collect();
        assert!(
            package_manifest(&project, "fab", None, &many).is_err(),
            "the manifest leaves room inside the file limit"
        );
        Ok(())
    }
}
