//! Per-user highlights, notes and bookmarks on datasheet PDFs, keyed by content
//! hash so a new download URL does not lose marks when the bytes match.
//!
//! # Rules
//! - Stored only on this computer under the user configuration folder.
//! - Coordinates are normalised to the page (0..=1), origin top-left.
//! - Writes use a temporary file and rename into place.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub const FILE_VERSION: u32 = 1;
pub const MAX_DOCUMENTS: usize = 512;
pub const MAX_ENTRIES_PER_DOCUMENT: usize = 256;
pub const MAX_NOTE_CHARS: usize = 2000;
pub const MAX_LABEL_CHARS: usize = 128;
pub const MAX_ENTRY_ID_CHARS: usize = 32;
pub const MAX_PAGE: u32 = 10_000;
pub const COPY_TO_FIELD_MAX: usize = 200;

static PATH_TEST: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Override path for tests.
pub fn set_store_path_for_test(path: Option<PathBuf>) {
    if let Ok(mut locked) = PATH_TEST.lock() {
        *locked = path;
    }
}

fn store_path() -> Result<PathBuf, String> {
    if let Ok(locked) = PATH_TEST.lock() {
        if let Some(path) = locked.as_ref() {
            return Ok(path.clone());
        }
    }
    let directory = |key: &str| {
        std::env::var(key)
            .ok()
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    };
    directory("XDG_CONFIG_HOME")
        .or_else(|| directory("APPDATA"))
        .or_else(|| directory("HOME").map(|home| home.join(".config")))
        .map(|base| base.join("pcbcraft").join("datasheet-annotations.json"))
        .ok_or_else(|| "no user configuration folder was found".into())
}

fn invalid(text: &str) -> String {
    text.to_owned()
}

/// Lowercase hex content hash, 8 to 64 digits.
pub fn check_document_hash(hash: &str) -> Result<(), String> {
    if hash.len() < 8
        || hash.len() > 64
        || !hash
            .chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
    {
        return Err(invalid(
            "document_hash must be 8 to 64 lowercase hex digits",
        ));
    }
    Ok(())
}

/// Normalised rectangle on one page; edges must be finite and within 0..=1.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PageRect {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

pub fn check_rect(rect: &PageRect) -> Result<(), String> {
    let in_unit = |v: f64| v.is_finite() && (0.0..=1.0).contains(&v);
    if !(in_unit(rect.x0) && in_unit(rect.y0) && in_unit(rect.x1) && in_unit(rect.y1)) {
        return Err(invalid("rect corners must be finite numbers from 0 to 1"));
    }
    if rect.x0 > rect.x1 || rect.y0 > rect.y1 {
        return Err(invalid("rect must have x0 <= x1 and y0 <= y1"));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryKind {
    Highlight,
    Note,
    Bookmark,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DatasheetEntry {
    pub id: String,
    pub kind: EntryKind,
    pub page: u32,
    pub rect: PageRect,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
}

pub fn check_entry(entry: &DatasheetEntry) -> Result<(), String> {
    if entry.id.is_empty() || entry.id.len() > MAX_ENTRY_ID_CHARS {
        return Err(invalid("entry id must be 1 to 32 bytes"));
    }
    if entry.id.chars().any(|c| c.is_control() || c == '/') {
        return Err(invalid(
            "entry id must not contain control characters or slashes",
        ));
    }
    if entry.page == 0 || entry.page > MAX_PAGE {
        return Err(invalid("page must be from 1 to 10000"));
    }
    check_rect(&entry.rect)?;
    if entry.note.chars().count() > MAX_NOTE_CHARS {
        return Err(invalid("note must be at most 2000 characters"));
    }
    if entry.label.chars().count() > MAX_LABEL_CHARS {
        return Err(invalid("label must be at most 128 characters"));
    }
    if entry.note.chars().any(char::is_control) || entry.label.chars().any(char::is_control) {
        return Err(invalid(
            "note and label must not contain control characters",
        ));
    }
    match entry.kind {
        EntryKind::Highlight => {}
        EntryKind::Note if entry.note.trim().is_empty() => {
            return Err(invalid("a note entry needs note text"));
        }
        EntryKind::Bookmark if entry.label.trim().is_empty() => {
            return Err(invalid("a bookmark entry needs a label"));
        }
        EntryKind::Note | EntryKind::Bookmark => {}
    }
    Ok(())
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DocumentAnnotations {
    #[serde(default)]
    pub entries: Vec<DatasheetEntry>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DatasheetAnnotationsFile {
    pub version: u32,
    #[serde(default)]
    pub documents: BTreeMap<String, DocumentAnnotations>,
}

impl Default for DatasheetAnnotationsFile {
    fn default() -> Self {
        Self {
            version: FILE_VERSION,
            documents: BTreeMap::new(),
        }
    }
}

pub fn load_file() -> Result<DatasheetAnnotationsFile, String> {
    let path = store_path()?;
    let Ok(text) = fs::read_to_string(&path) else {
        return Ok(DatasheetAnnotationsFile::default());
    };
    let parsed: DatasheetAnnotationsFile = serde_json::from_str(&text)
        .map_err(|_| invalid("datasheet annotations file could not be parsed"))?;
    if parsed.version != FILE_VERSION {
        return Err(invalid(
            "datasheet annotations file version is not supported",
        ));
    }
    Ok(parsed)
}

fn save_file(file: &DatasheetAnnotationsFile) -> Result<(), String> {
    if file.documents.len() > MAX_DOCUMENTS {
        return Err(invalid("too many documents in the profile store"));
    }
    for (hash, doc) in &file.documents {
        check_document_hash(hash)?;
        if doc.entries.len() > MAX_ENTRIES_PER_DOCUMENT {
            return Err(invalid("a document carries too many entries"));
        }
        for entry in &doc.entries {
            check_entry(entry)?;
        }
    }
    let path = store_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(file).map_err(|e| e.to_string())?;
    let temp = path.with_extension("tmp");
    let mut handle = fs::File::create(&temp).map_err(|e| e.to_string())?;
    handle
        .write_all(text.as_bytes())
        .map_err(|e| e.to_string())?;
    handle.sync_all().map_err(|e| e.to_string())?;
    fs::rename(&temp, &path).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn list_entries(document_hash: &str) -> Result<Vec<DatasheetEntry>, String> {
    check_document_hash(document_hash)?;
    let file = load_file()?;
    Ok(file
        .documents
        .get(document_hash)
        .map(|d| d.entries.clone())
        .unwrap_or_default())
}

pub fn upsert_entry(document_hash: &str, entry: DatasheetEntry) -> Result<DatasheetEntry, String> {
    check_document_hash(document_hash)?;
    check_entry(&entry)?;
    let mut file = load_file()?;
    let doc = file.documents.entry(document_hash.to_owned()).or_default();
    if doc.entries.len() >= MAX_ENTRIES_PER_DOCUMENT
        && !doc.entries.iter().any(|e| e.id == entry.id)
    {
        return Err(invalid(
            "this document already has the maximum number of entries",
        ));
    }
    if let Some(existing) = doc.entries.iter_mut().find(|e| e.id == entry.id) {
        *existing = entry.clone();
    } else {
        doc.entries.push(entry.clone());
    }
    save_file(&file)?;
    Ok(entry)
}

pub fn remove_entry(document_hash: &str, entry_id: &str) -> Result<bool, String> {
    check_document_hash(document_hash)?;
    if entry_id.is_empty() || entry_id.len() > MAX_ENTRY_ID_CHARS {
        return Err(invalid("entry id must be 1 to 32 bytes"));
    }
    let mut file = load_file()?;
    let Some(doc) = file.documents.get_mut(document_hash) else {
        return Ok(false);
    };
    let before = doc.entries.len();
    doc.entries.retain(|e| e.id != entry_id);
    if before == doc.entries.len() {
        return Ok(false);
    }
    if doc.entries.is_empty() {
        file.documents.remove(document_hash);
    }
    save_file(&file)?;
    Ok(true)
}

pub fn export_file(filter_hash: Option<&str>) -> Result<DatasheetAnnotationsFile, String> {
    let file = load_file()?;
    if let Some(hash) = filter_hash {
        check_document_hash(hash)?;
        let mut out = DatasheetAnnotationsFile::default();
        if let Some(doc) = file.documents.get(hash) {
            out.documents.insert(hash.to_owned(), doc.clone());
        }
        return Ok(out);
    }
    Ok(file)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportMode {
    Merge,
    ReplaceDocument,
    ReplaceAll,
}

pub fn import_file(payload: &DatasheetAnnotationsFile, mode: ImportMode) -> Result<usize, String> {
    if payload.version != FILE_VERSION {
        return Err(invalid("import payload version is not supported"));
    }
    for (hash, doc) in &payload.documents {
        check_document_hash(hash)?;
        if doc.entries.len() > MAX_ENTRIES_PER_DOCUMENT {
            return Err(invalid(
                "import payload has too many entries on one document",
            ));
        }
        for entry in &doc.entries {
            check_entry(entry)?;
        }
    }
    let mut file = match mode {
        ImportMode::ReplaceAll => DatasheetAnnotationsFile::default(),
        ImportMode::Merge | ImportMode::ReplaceDocument => load_file()?,
    };
    let mut count = 0usize;
    for (hash, doc) in &payload.documents {
        match mode {
            ImportMode::Merge => {
                let target = file.documents.entry(hash.clone()).or_default();
                for entry in &doc.entries {
                    if let Some(existing) = target.entries.iter_mut().find(|e| e.id == entry.id) {
                        *existing = entry.clone();
                    } else if target.entries.len() < MAX_ENTRIES_PER_DOCUMENT {
                        target.entries.push(entry.clone());
                        count += 1;
                    } else {
                        return Err(invalid(
                            "merge would exceed the entry limit on one document",
                        ));
                    }
                }
            }
            ImportMode::ReplaceDocument | ImportMode::ReplaceAll => {
                count += doc.entries.len();
                file.documents.insert(hash.clone(), doc.clone());
            }
        }
    }
    if file.documents.len() > MAX_DOCUMENTS {
        return Err(invalid("import would exceed the document limit"));
    }
    save_file(&file)?;
    Ok(count)
}

/// One line of text suitable for a component field, at most [`COPY_TO_FIELD_MAX`] characters.
pub fn entry_field_text(entry: &DatasheetEntry) -> String {
    let mut text = match entry.kind {
        EntryKind::Highlight => {
            if entry.note.trim().is_empty() {
                format!("Highlight on page {}", entry.page)
            } else {
                format!("p{}: {}", entry.page, entry.note.trim())
            }
        }
        EntryKind::Note => format!("p{}: {}", entry.page, entry.note.trim()),
        EntryKind::Bookmark => format!("p{} bookmark: {}", entry.page, entry.label.trim()),
    };
    if text.chars().count() > COPY_TO_FIELD_MAX {
        text = text.chars().take(COPY_TO_FIELD_MAX).collect();
    }
    text
}

pub fn entry_by_id(document_hash: &str, entry_id: &str) -> Result<Option<DatasheetEntry>, String> {
    check_document_hash(document_hash)?;
    let file = load_file()?;
    Ok(file
        .documents
        .get(document_hash)
        .and_then(|d| d.entries.iter().find(|e| e.id == entry_id).cloned()))
}

/// Path used on disk when not overridden; for tests and diagnostics only.
pub fn store_path_for_display() -> Result<PathBuf, String> {
    store_path()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, MutexGuard};

    fn lock_store(path: PathBuf) -> MutexGuard<'static, ()> {
        static LOCK: Mutex<()> = Mutex::new(());
        let guard = LOCK.lock().unwrap_or_else(|poison| poison.into_inner());
        set_store_path_for_test(Some(path));
        guard
    }

    #[test]
    fn annotations_survive_when_only_the_hash_is_used() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!("pcbcraft-ds-anno-{}", std::process::id()));
        let _guard = lock_store(dir.join("store.json"));
        let hash = "abc12345deadbeef";
        let entry = DatasheetEntry {
            id: "m1".into(),
            kind: EntryKind::Bookmark,
            page: 3,
            rect: PageRect {
                x0: 0.1,
                y0: 0.2,
                x1: 0.3,
                y1: 0.4,
            },
            note: String::new(),
            label: "Table 1".into(),
        };
        upsert_entry(hash, entry.clone())?;
        let listed = list_entries(hash)?;
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].label, "Table 1");
        let other_url_hash = hash;
        assert_eq!(list_entries(other_url_hash)?.len(), 1);
        Ok(())
    }

    #[test]
    fn export_and_import_round_trip() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!("pcbcraft-ds-anno-rt-{}", std::process::id()));
        let _guard = lock_store(dir.join("store.json"));
        let hash = "0011223344556677";
        upsert_entry(
            hash,
            DatasheetEntry {
                id: "n1".into(),
                kind: EntryKind::Note,
                page: 1,
                rect: PageRect {
                    x0: 0.0,
                    y0: 0.0,
                    x1: 1.0,
                    y1: 0.1,
                },
                note: "Max current 200 mA".into(),
                label: String::new(),
            },
        )?;
        let exported = export_file(None)?;
        set_store_path_for_test(Some(dir.join("other.json")));
        let imported = import_file(&exported, ImportMode::ReplaceAll)?;
        assert_eq!(imported, 1);
        let back = list_entries(hash)?;
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].note, "Max current 200 mA");
        Ok(())
    }

    #[test]
    fn field_text_stays_under_two_hundred_characters() {
        let long: String = "x".repeat(300);
        let text = entry_field_text(&DatasheetEntry {
            id: "a".into(),
            kind: EntryKind::Note,
            page: 9,
            rect: PageRect {
                x0: 0.0,
                y0: 0.0,
                x1: 1.0,
                y1: 1.0,
            },
            note: long,
            label: String::new(),
        });
        assert!(text.chars().count() <= COPY_TO_FIELD_MAX);
    }
}
