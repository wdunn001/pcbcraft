//! Lazy PDF page rendering behind a small trait surface for the datasheet reader widget.

mod error;
mod hayro_backend;
mod layout;
mod load;
mod traits;

pub use error::{LoadError, OpenError, RenderError};
pub use hayro_backend::{HayroDocument, HayroRenderer};
pub use layout::{
    MAX_TEXTURE_SIDE, PAGE_GAP_PX, page_height_px, page_offsets, page_width_px, scroll_height,
    tile_grid, visible_pages,
};
pub use load::load_bytes;
pub use traits::{PageDocument, PageRenderer, PageSizePt, RenderRequest, TileRect};
