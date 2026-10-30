//! Datasheet PDF text layer: glyph boxes, search, and selection in page coordinates.
//!
//! Context: the Library tab reader (#695) needs selectable, searchable text drawn over a
//! rendered page. This crate holds the pure logic; egui paints highlights and forwards
//! copy and accessibility.
//!
//! Rules: page index is 0-based; boxes use page points with the origin at the top left;
//! failures are `String`, never panics on bad input.
//!
//! Dependencies: `hayro` for parse, render, and glyph extraction.
//!
//! Do not: fetch URLs or touch the network from here.

mod document;
mod fixtures;
mod render;
mod search;
mod selection;
mod text;

pub use document::Document;
pub use fixtures::Fixture;
pub use render::{Bitmap, MAX_SIDE_PX, render_page};
pub use search::{DocHit, DocSearch, Hit, find, search_document};
pub use selection::{Selection, char_at_point, selected_text};
pub use text::{CharBox, open, page_has_text, page_size, plain_text, text_layer};
