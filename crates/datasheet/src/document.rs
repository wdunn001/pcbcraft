//! An opened PDF kept in memory for the reader pane.

use crate::render::{Bitmap, render_page};
use crate::search::{DocSearch, search_document};
use crate::text::{CharBox, open, page_has_text, plain_text, text_layer};
use hayro::RenderCache;
use hayro::hayro_interpret::InterpreterCache;
use hayro::hayro_syntax::Pdf;

/// An opened datasheet PDF.
pub struct Document {
    pdf: Pdf,
    page_count: usize,
}

impl Document {
    /// Parse bytes into a document handle.
    pub fn open(bytes: Vec<u8>) -> Result<Self, String> {
        let pdf = open(bytes)?;
        let page_count = count_pages(&pdf);
        if page_count == 0 {
            return Err("the file has no pages".into());
        }
        Ok(Self { pdf, page_count })
    }

    /// Number of pages.
    pub fn page_count(&self) -> usize {
        self.page_count
    }

    /// Page size in points for layout.
    pub fn page_size(&self, page: usize) -> Result<(f32, f32), String> {
        crate::text::page_size(&self.pdf, page).ok_or_else(|| "no such page".to_string())
    }

    /// Rasterise one page for display.
    pub fn render(&self, page: usize, dpi: f32) -> Result<Bitmap, String> {
        let cache = RenderCache::new();
        render_page(&self.pdf, page, dpi, &cache)
    }

    /// Text layer for one page, cached by the caller if needed.
    pub fn text_layer(&self, page: usize) -> Result<Vec<CharBox>, String> {
        let cache = InterpreterCache::new();
        text_layer(&self.pdf, page, &cache)
    }

    /// Whether the page exposes selectable text.
    pub fn page_has_text(&self, page: usize) -> Result<bool, String> {
        Ok(page_has_text(&self.text_layer(page)?))
    }

    /// Full page text for accessibility.
    pub fn page_plain_text(&self, page: usize) -> Result<String, String> {
        Ok(plain_text(&self.text_layer(page)?))
    }

    /// Search the whole file.
    pub fn search(&self, needle: &str) -> Result<DocSearch, String> {
        search_document(&self.pdf, needle)
    }
}

fn count_pages(pdf: &Pdf) -> usize {
    let pages = pdf.pages();
    (0..).take_while(|i| pages.get(*i).is_some()).count()
}
