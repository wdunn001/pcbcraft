//! Shared vocabulary for online CAD sources: what a part reference looks like,
//! which licence and credit travel with a file, and the trait every source
//! implements. A CAD source supplies symbol, footprint and 3D model files, not
//! stock or prices (those stay on `PartSource` in `crates/library/src/sources.rs`).
//!
//! # Rules
//! - No network, disk or clock access here; implementations bring their own.
//! - Every text field is bounded and free of control characters before a source sees it.
//! - Nothing here changes `PartResult`; other branches build it with struct literals.
//!
//! # Dependencies
//! - crates/library/src/sources.rs
use crate::sources::{Attribution, SourceError};
use serde::{Deserialize, Serialize};

/// Longest library or part name a reference may carry.
pub const MAX_NAME_BYTES: usize = 128;
/// Longest upstream revision text (a blob or commit id) a reference may carry.
pub const MAX_REVISION_BYTES: usize = 64;
/// Largest file a source may hand back: 8 MiB.
pub const MAX_FILE_BYTES: usize = 8 * 1024 * 1024;
/// Longest piece of any one field shown inside a provenance notice.
const NOTICE_FIELD_CLIP: usize = 96;

/// [`Provenance::source_id`] for a file the user downloaded from a manufacturer
/// and dropped into the project. PCBCraft never redistributes these bytes.
pub const USER_MANUFACTURER_SOURCE_ID: &str = "user-manufacturer-download";

/// Which kind of file a reference points at.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CadKind {
    /// A symbol library file.
    Symbol,
    /// A footprint file.
    Footprint,
    /// A 3D model file.
    Model,
}

impl CadKind {
    /// Plain word for messages and notices.
    pub fn label(self) -> &'static str {
        match self {
            CadKind::Symbol => "symbol",
            CadKind::Footprint => "footprint",
            CadKind::Model => "3D model",
        }
    }
}

/// A part file named by library and part name, with the upstream revision when
/// the source has told us one.
///
/// # Rules
/// - Build it with [`CadRef::new`]; a source calls [`CadRef::check`] again on any
///   reference that arrived by another route (for example from JSON).
/// - `library` and `name` are single path pieces: no `/`, `\`, `..`, NUL or control characters.
///
/// # Do not
/// - Join `library` and `name` into a path without calling [`CadRef::check`] first.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CadRef {
    /// Which kind of file this names.
    pub kind: CadKind,
    /// Library (folder) the file lives in, without any folder suffix.
    pub library: String,
    /// Part name, without a file extension.
    pub name: String,
    /// Upstream revision of this one file (a content id); empty when unknown.
    #[serde(default)]
    pub revision: String,
}

fn check_piece(what: &str, text: &str) -> Result<(), SourceError> {
    if text.is_empty() || text.len() > MAX_NAME_BYTES {
        return Err(SourceError::Invalid(format!(
            "the {what} must be 1 to {MAX_NAME_BYTES} bytes"
        )));
    }
    if text == "." || text == ".." || text.contains("..") {
        return Err(SourceError::Invalid(format!(
            "the {what} may not contain two dots in a row"
        )));
    }
    if text
        .chars()
        .any(|c| c.is_control() || matches!(c, '/' | '\\' | '\0'))
    {
        return Err(SourceError::Invalid(format!(
            "the {what} may not contain slashes or control characters"
        )));
    }
    Ok(())
}

impl CadRef {
    /// A validated reference with no known revision.
    pub fn new(kind: CadKind, library: &str, name: &str) -> Result<Self, SourceError> {
        let r = Self {
            kind,
            library: library.to_owned(),
            name: name.to_owned(),
            revision: String::new(),
        };
        r.check()?;
        Ok(r)
    }

    /// The same reference with an upstream revision attached.
    pub fn with_revision(mut self, revision: &str) -> Result<Self, SourceError> {
        self.revision = revision.to_owned();
        self.check()?;
        Ok(self)
    }

    /// Re-validate every field.
    pub fn check(&self) -> Result<(), SourceError> {
        check_piece("library name", &self.library)?;
        check_piece("part name", &self.name)?;
        if self.revision.len() > MAX_REVISION_BYTES
            || !self.revision.chars().all(|c| c.is_ascii_alphanumeric())
        {
            return Err(SourceError::Invalid(
                "the revision must be letters and digits, at most 64 bytes".into(),
            ));
        }
        Ok(())
    }
}

/// The licence a source's files come under, shown on cards and in the import step.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LicenseInfo {
    /// Short name such as "CC-BY-SA 4.0".
    pub name: String,
    /// Link to the licence text.
    pub url: String,
    /// One plain sentence on what the licence asks of a person who keeps the
    /// files, for example the design-output exception.
    pub terms_note: String,
}

/// Where one imported file came from. [`Provenance::notice`] is the text that
/// lands in the project library's notices and doubles as the credit the
/// licence asks for.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Provenance {
    /// Stable identifier of the source.
    pub source_id: String,
    /// Name of the source as shown to people.
    pub source_name: String,
    /// Which kind of file this was.
    pub kind: CadKind,
    /// Path of the file inside the upstream repository.
    pub upstream_path: String,
    /// Upstream revision of this file; empty when unknown.
    pub blob_id: String,
    /// Upstream repository revision the answer was read at; empty when unknown.
    pub commit_id: String,
    /// Short licence name.
    pub license_name: String,
    /// Link to the licence text.
    pub license_url: String,
    /// Credit line, for example the author group.
    pub attribution: String,
}

fn clip(text: &str) -> String {
    let flat: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    if flat.len() <= NOTICE_FIELD_CLIP {
        return flat;
    }
    let mut end = NOTICE_FIELD_CLIP;
    while !flat.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &flat[..end])
}

/// Fields recovered from [`Provenance::notice`] text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedProvenanceNotice {
    /// Which kind of file the notice describes.
    pub kind: CadKind,
    /// Source name as shown in the notice.
    pub source_name: String,
    /// Stable source identifier when the notice was written with one; empty for older notices.
    pub source_id: String,
    /// Path of the file inside the upstream repository.
    pub upstream_path: String,
    /// Stored upstream file revision; empty when the notice named none.
    pub blob_id: String,
    /// Stored upstream repository revision; empty when the notice named none.
    pub commit_id: String,
}

impl Provenance {
    /// Provenance for one symbol, footprint or model file the user downloaded
    /// from a manufacturer page and imported locally.
    pub fn user_manufacturer_download(
        manufacturer: &str,
        mpn: &str,
        kind: CadKind,
        filename: &str,
        page_url: &str,
    ) -> Result<Self, SourceError> {
        let manufacturer = manufacturer.trim();
        let mpn = mpn.trim();
        let filename = filename.trim();
        if manufacturer.is_empty() || manufacturer.len() > MAX_NAME_BYTES {
            return Err(SourceError::Invalid(
                "the manufacturer name must be 1 to 128 bytes".into(),
            ));
        }
        if mpn.is_empty() || mpn.len() > MAX_REVISION_BYTES {
            return Err(SourceError::Invalid(
                "the part number must be 1 to 64 bytes".into(),
            ));
        }
        if filename.is_empty() || filename.len() > MAX_NAME_BYTES {
            return Err(SourceError::Invalid(
                "the file name must be 1 to 128 bytes".into(),
            ));
        }
        if page_url.is_empty() || page_url.len() > 512 {
            return Err(SourceError::Invalid(
                "the manufacturer page link must be 1 to 512 bytes".into(),
            ));
        }
        Ok(Self {
            source_id: USER_MANUFACTURER_SOURCE_ID.into(),
            source_name: manufacturer.to_owned(),
            kind,
            upstream_path: filename.to_owned(),
            blob_id: String::new(),
            commit_id: String::new(),
            license_name: "User download (not redistributed)".into(),
            license_url: page_url.to_owned(),
            attribution: mpn.to_owned(),
        })
    }

    /// One plain sentence naming the source, the file, its upstream revision
    /// and the licence with its link. Stable for equal input, so the library's
    /// notice list de-duplicates it, and under one KiB for any input.
    pub fn notice(&self) -> String {
        if self.source_id == USER_MANUFACTURER_SOURCE_ID {
            return format!(
                "Imported {} for part {} from a file downloaded by the user from {}; not redistributed. Source file: {}.",
                self.kind.label(),
                clip(&self.attribution),
                clip(&self.source_name),
                clip(&self.upstream_path)
            );
        }
        let mut text = format!(
            "Imported {} from {} ({}): {}",
            self.kind.label(),
            clip(&self.source_name),
            clip(&self.source_id),
            clip(&self.upstream_path)
        );
        if !self.blob_id.is_empty() {
            text.push_str(&format!(", file revision {}", clip(&self.blob_id)));
        }
        if !self.commit_id.is_empty() {
            text.push_str(&format!(", repository revision {}", clip(&self.commit_id)));
        }
        text.push_str(&format!(
            ". Licence {} ({})",
            clip(&self.license_name),
            clip(&self.license_url)
        ));
        if !self.attribution.is_empty() {
            text.push_str(&format!(". Credit: {}", clip(&self.attribution)));
        }
        text.push('.');
        text
    }

    /// Reads back the import fields that [`Self::notice`] writes.
    ///
    /// # Rules
    /// - Accepts notices with or without a parenthesised source id.
    /// - Clipped fields in the notice round-trip to the clipped text, not the full value.
    ///
    /// # Dependencies
    /// - crates/library/src/cad.rs `Provenance::notice`
    ///
    /// # Do not
    /// - Treat licence or credit lines as part of the upstream path.
    pub fn parse_notice(text: &str) -> Result<ParsedProvenanceNotice, SourceError> {
        let rest = text
            .strip_prefix("Imported ")
            .ok_or_else(|| invalid_notice("the notice must start with Imported"))?;
        let (kind, rest) = parse_notice_kind(rest)?;
        let rest = rest
            .strip_prefix(" from ")
            .ok_or_else(|| invalid_notice("the notice must say from after the file kind"))?;
        let (source_name, source_id, rest) = parse_notice_source(rest)?;
        let rest = rest.strip_prefix(": ").ok_or_else(|| {
            invalid_notice("the notice must name the upstream path after the source")
        })?;
        let (upstream_path, rest) = split_notice_field(
            rest,
            &[", file revision ", ", repository revision ", ". Licence "],
        )?;
        if upstream_path.is_empty() {
            return Err(invalid_notice("the upstream path must not be empty"));
        }
        let (blob_id, rest) = parse_notice_optional_prefix(rest, ", file revision ")?;
        let (commit_id, rest) = parse_notice_optional_prefix(rest, ", repository revision ")?;
        let rest = rest
            .strip_prefix(". Licence ")
            .ok_or_else(|| invalid_notice("the notice must end with a licence line"))?;
        if rest.is_empty() {
            return Err(invalid_notice("the licence line must not be empty"));
        }
        Ok(ParsedProvenanceNotice {
            kind,
            source_name,
            source_id,
            upstream_path,
            blob_id,
            commit_id,
        })
    }
}

fn invalid_notice(text: &str) -> SourceError {
    SourceError::Invalid(text.into())
}

fn parse_notice_kind(text: &str) -> Result<(CadKind, &str), SourceError> {
    for (label, kind) in [
        ("3D model", CadKind::Model),
        ("footprint", CadKind::Footprint),
        ("symbol", CadKind::Symbol),
    ] {
        if let Some(rest) = text.strip_prefix(label) {
            return Ok((kind, rest));
        }
    }
    Err(invalid_notice(
        "the notice must name symbol, footprint or 3D model",
    ))
}

fn parse_notice_source(text: &str) -> Result<(String, String, &str), SourceError> {
    if let Some(open) = text.find(" (") {
        let close = text[open..]
            .find("): ")
            .ok_or_else(|| invalid_notice("a source id must be closed before the path"))?;
        let source_name = text[..open].to_owned();
        let source_id = text[open + 2..open + close].to_owned();
        if source_name.is_empty() || source_id.is_empty() {
            return Err(invalid_notice("the source name and id must not be empty"));
        }
        return Ok((source_name, source_id, &text[open + close + 3..]));
    }
    let colon = text
        .find(": ")
        .ok_or_else(|| invalid_notice("the notice must name the upstream path after the source"))?;
    let source_name = text[..colon].to_owned();
    if source_name.is_empty() {
        return Err(invalid_notice("the source name must not be empty"));
    }
    Ok((source_name, String::new(), &text[colon + 2..]))
}

fn split_notice_field<'a>(text: &'a str, stops: &[&str]) -> Result<(String, &'a str), SourceError> {
    let mut end = text.len();
    for stop in stops {
        if let Some(at) = text.find(stop) {
            end = end.min(at);
        }
    }
    let field = text[..end].to_owned();
    Ok((field, &text[end..]))
}

fn parse_notice_optional_prefix<'a>(
    text: &'a str,
    prefix: &str,
) -> Result<(String, &'a str), SourceError> {
    if let Some(rest) = text.strip_prefix(prefix) {
        let (value, rest) = split_notice_field(
            rest,
            &[", file revision ", ", repository revision ", ". Licence "],
        )?;
        return Ok((value, rest));
    }
    Ok((String::new(), text))
}

/// One fetched file: its bytes and where they came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CadFile {
    /// The reference that was fetched, with its upstream revision filled in.
    pub cad_ref: CadRef,
    /// The file's bytes.
    pub bytes: Vec<u8>,
    /// Where the file came from.
    pub provenance: Provenance,
}

impl CadFile {
    /// The bytes as text, or a plain error when the file is not UTF-8.
    pub fn text(&self) -> Result<&str, SourceError> {
        std::str::from_utf8(&self.bytes)
            .map_err(|_| SourceError::Invalid("the file is not text".into()))
    }
}

/// One online supplier of symbol, footprint and 3D model files.
///
/// # Rules
/// - A source never writes to the project; it only lists and returns files.
/// - It calls [`CadRef::check`] before building a path or URL from a reference.
/// - A file over [`MAX_FILE_BYTES`] is refused with [`SourceError::Invalid`].
/// - Offline, it answers from its own cache, else [`SourceError::Unavailable`].
///
/// # Dependencies
/// - crates/library/src/sources.rs
///
/// # Do not
/// - Add a method that needs the network to the default implementations.
/// - Put keys or account data in a reference or a provenance record.
///
/// # Implementations
/// - crates/library/src/open_library.rs
pub trait CadSource: Send + Sync {
    /// Short stable identifier used in cache keys and notices.
    fn id(&self) -> &str;
    /// Name shown on cards.
    fn name(&self) -> &str;
    /// The licence this source's files come under.
    fn license(&self) -> LicenseInfo;
    /// The credit the licence asks to see next to the data.
    fn attribution(&self) -> Attribution;
    /// Names of the libraries (folders) that hold files of this kind.
    fn list_libraries(&self, kind: CadKind) -> Result<Vec<String>, SourceError>;
    /// Every file of this kind in one library.
    fn list_entries(&self, kind: CadKind, library: &str) -> Result<Vec<CadRef>, SourceError>;
    /// Files of this kind whose library or name matches the words in `query`.
    fn search(&self, kind: CadKind, query: &str) -> Result<Vec<CadRef>, SourceError>;
    /// Fetch one file.
    fn fetch(&self, cad_ref: &CadRef) -> Result<CadFile, SourceError>;

    /// Where the file for this reference is kept on this computer, when the
    /// source keeps files on disk and has fetched it. Used to point a
    /// footprint's 3D model reference at the saved copy.
    fn cached_path(&self, _cad_ref: &CadRef) -> Option<std::path::PathBuf> {
        None
    }

    /// Fetch a symbol library file.
    fn fetch_symbol(&self, cad_ref: &CadRef) -> Result<CadFile, SourceError> {
        want(cad_ref, CadKind::Symbol)?;
        self.fetch(cad_ref)
    }
    /// Fetch a footprint file.
    fn fetch_footprint(&self, cad_ref: &CadRef) -> Result<CadFile, SourceError> {
        want(cad_ref, CadKind::Footprint)?;
        self.fetch(cad_ref)
    }
    /// Fetch the bytes of a 3D model file.
    fn fetch_model(&self, cad_ref: &CadRef) -> Result<CadFile, SourceError> {
        want(cad_ref, CadKind::Model)?;
        self.fetch(cad_ref)
    }
}

fn want(cad_ref: &CadRef, kind: CadKind) -> Result<(), SourceError> {
    cad_ref.check()?;
    if cad_ref.kind != kind {
        return Err(SourceError::Invalid(format!(
            "expected a {} reference, got a {}",
            kind.label(),
            cad_ref.kind.label()
        )));
    }
    Ok(())
}
