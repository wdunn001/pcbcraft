//! Trait sketch for the Library tab datasheet page (spike #723, parent #695).
//!
//! This file is documentation only. It is not wired into the app workspace. When the reader lands,
//! move these traits behind a small crate under `crates/` and implement `HayroBackend` by default,
//! with `PdfiumBackend` behind feature `pdfium` on desktop only.

/// Document metadata after a successful open.
pub struct DocInfo {
    /// Number of pages.
    pub page_count: usize,
}

/// A page region in device pixels for tiled rendering.
pub struct PxRect {
    /// Left edge in pixels.
    pub x: u32,
    /// Top edge in pixels.
    pub y: u32,
    /// Width in pixels.
    pub w: u32,
    /// Height in pixels.
    pub h: u32,
}

/// One character box in page points (top-left origin, y down).
pub struct CharBox {
    /// Character.
    pub ch: char,
    /// Left edge in points.
    pub x0: f32,
    /// Top edge in points.
    pub y0: f32,
    /// Right edge in points.
    pub x1: f32,
    /// Bottom edge in points.
    pub y1: f32,
}

/// A search hit inside one page.
pub struct Hit {
    /// Matched substring.
    pub needle: String,
    /// Bounding box in page points.
    pub bbox: CharBox,
}

/// Errors from open, render or text extraction (no panics on bad input).
pub enum DatasheetError {
    /// Human-readable reason.
    Message(String),
}

/// RGBA page bitmap for egui textures.
pub struct Bitmap {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Straight RGBA bytes, row-major.
    pub rgba: Vec<u8>,
}

/// Rasterise PDF pages for display.
pub trait PageRenderer {
    /// Parse bytes and return page count.
    fn open(&mut self, bytes: Vec<u8>) -> Result<DocInfo, DatasheetError>;
    /// Render one page at `scale` (1.0 = 72 dpi). Optional `tile` limits the pixmap.
    fn render(
        &self,
        page: usize,
        scale: f32,
        tile: Option<PxRect>,
    ) -> Result<Bitmap, DatasheetError>;
}

/// Positioned text for search, highlights and extraction assist.
pub trait TextLayer {
    /// All characters on one page in reading order.
    fn chars(&self, page: usize) -> Result<Vec<CharBox>, DatasheetError>;
    /// Case-insensitive substring search on one page.
    fn search(&self, page: usize, needle: &str) -> Vec<Hit>;
}
