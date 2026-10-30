//! Datasheet reading: render PDF pages and read their text, behind two small traits.
//!
//! Context: issues #695 and #678 (datasheet reader), spike #723 (renderer choice). The Library
//! tab reads a cached PDF. This module has no UI dependency: `PageRenderer` turns a page into
//! pixels, `TextLayer` gives each character a box in page points so the reader can search,
//! highlight and copy. `HayroBackend` implements both with one parse. A cache module keeps the
//! PDFs a person fetched or chose.
//!
//! Local rules: page numbers are 0-based; boxes use page points with the origin at the top left
//! and y down; nothing here panics on input from a file; every failure is a `DatasheetError`
//! with a plain sentence. A PDF is untrusted input: size and page count are capped before
//! parsing, and no script or external action is ever run.
//!
//! # Dependencies
//! - crates/library/src/datasheet/hayro_backend.rs (the renderer)
//! - crates/library/src/datasheet/cache.rs (files on this computer)
//! - crates/library/src/datasheet/search.rs (text search)
//!
//! # Implementations
//! - crates/library/src/datasheet/hayro_backend.rs (`PageRenderer` and `TextLayer`)
//!
//! # Do not
//! - Bundle or commit a vendor datasheet; tests use the generated PDFs in `fixtures.rs`.
//! - Fetch from the network here; the app fetches on a user action and hands over bytes.
pub mod cache;
pub mod fixtures;
mod hayro_backend;
pub mod search;

pub use hayro_backend::HayroBackend;

/// Largest PDF the reader opens, in bytes.
pub const MAX_PDF_BYTES: usize = 25 * 1024 * 1024;
/// Most pages the reader opens.
pub const MAX_PAGES: usize = 2000;
/// Longest side of a rendered bitmap, in pixels.
pub const MAX_SIDE_PX: f32 = 8192.0;

/// Why a datasheet could not be read.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DatasheetError {
    /// The bytes are not a PDF (no `%PDF-` near the start).
    #[error("That file is not a PDF.")]
    NotPdf,
    /// The file is over the size cap.
    #[error("That PDF is larger than {} MB, so it was not opened.", MAX_PDF_BYTES / (1024 * 1024))]
    TooLarge,
    /// The PDF has more pages than the cap.
    #[error("That PDF has more than {MAX_PAGES} pages, so it was not opened.")]
    TooManyPages,
    /// The PDF is damaged or needs a password.
    #[error("That PDF could not be opened: {0}")]
    Open(String),
    /// One page could not be read.
    #[error("That page could not be drawn: {0}")]
    Page(String),
    /// A page number past the end.
    #[error("That PDF has no such page.")]
    NoSuchPage,
    /// No document is open.
    #[error("No PDF is open.")]
    NotOpen,
    /// The address is not a web address.
    #[error("That is not a web address.")]
    BadAddress,
    /// The cache could not be read or written.
    #[error("The datasheet cache could not be used: {0}")]
    Cache(String),
}

/// What opening a document tells the reader.
#[derive(Clone, Debug, PartialEq)]
pub struct DocInfo {
    /// Page sizes in points, after rotation, in page order.
    pub sizes: Vec<(f32, f32)>,
}

/// A rectangle of pixels, for cutting a tile out of a page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PxRect {
    /// Left edge.
    pub x: u32,
    /// Top edge.
    pub y: u32,
    /// Width.
    pub w: u32,
    /// Height.
    pub h: u32,
}

/// An RGBA bitmap on an opaque white page.
#[derive(Clone, Debug)]
pub struct Bitmap {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Straight RGBA bytes, row by row.
    pub rgba: Vec<u8>,
}

/// One character with its box in page points.
#[derive(Clone, Debug, PartialEq)]
pub struct CharBox {
    /// The character; spaces and line breaks are synthesised from geometry.
    pub ch: char,
    /// Left edge.
    pub x0: f32,
    /// Top edge.
    pub y0: f32,
    /// Right edge.
    pub x1: f32,
    /// Bottom edge.
    pub y1: f32,
}

/// A search hit as a rectangle in page points, with the character range it covers.
#[derive(Clone, Debug, PartialEq)]
pub struct Hit {
    /// Left edge.
    pub x0: f32,
    /// Top edge.
    pub y0: f32,
    /// Right edge.
    pub x1: f32,
    /// Bottom edge.
    pub y1: f32,
    /// Index of the first matched character in the page's character list.
    pub first: usize,
    /// Index one past the last matched character.
    pub last: usize,
}

/// Turns pages into pixels.
///
/// # Rules
/// - `scale` is pixels per point; the result is capped at [`MAX_SIDE_PX`] on its longest side.
/// - `tile` cuts a rectangle of that bitmap; `None` returns the whole page.
///
/// # Implementations
/// - crates/library/src/datasheet/hayro_backend.rs
///
/// # Do not
/// - Keep a page rendered for the caller; the caller caches what it needs.
pub trait PageRenderer {
    /// Open a document from its bytes.
    fn open(&mut self, bytes: Vec<u8>) -> Result<DocInfo, DatasheetError>;
    /// Render one page, or a tile of it.
    fn render(
        &self,
        page: usize,
        scale: f32,
        tile: Option<PxRect>,
    ) -> Result<Bitmap, DatasheetError>;
}

/// Gives pages a text layer.
///
/// # Implementations
/// - crates/library/src/datasheet/hayro_backend.rs
pub trait TextLayer {
    /// The characters of a page in content order, with boxes in page points.
    fn chars(&self, page: usize) -> Result<Vec<CharBox>, DatasheetError>;
}

/// Check bytes before parsing: size cap and the `%PDF-` header in the first kilobyte.
///
/// # Rules
/// - Pure; reads no file.
pub fn check_pdf(bytes: &[u8]) -> Result<(), DatasheetError> {
    if bytes.len() > MAX_PDF_BYTES {
        return Err(DatasheetError::TooLarge);
    }
    let head = &bytes[..bytes.len().min(1024)];
    if !head.windows(5).any(|w| w == b"%PDF-") {
        return Err(DatasheetError::NotPdf);
    }
    Ok(())
}
