use crate::error::{OpenError, RenderError};
use image::RgbaImage;

/// Pixel rectangle inside a scaled page bitmap.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TileRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// One render job for a page at a given scale, optionally cropped to a tile.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RenderRequest {
    pub page: u32,
    /// Pixels per PDF point (1/72 inch).
    pub scale: f32,
    pub tile: Option<TileRect>,
}

/// Size of one page in PDF points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageSizePt {
    pub width: f32,
    pub height: f32,
}

/// Opens documents and renders pages; alternate backends implement this trait.
pub trait PageRenderer: Send + Sync {
    type Document: PageDocument + Send;

    fn open(&self, bytes: &[u8]) -> Result<Self::Document, OpenError>;
}

/// Page metadata and rasterization for one open document.
pub trait PageDocument: Send {
    fn page_count(&self) -> u32;
    fn page_size_pt(&self, page: u32) -> Result<PageSizePt, RenderError>;
    fn render(&self, request: RenderRequest) -> Result<RgbaImage, RenderError>;
}
