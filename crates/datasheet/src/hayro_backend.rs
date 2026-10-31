use crate::error::{OpenError, RenderError};
use crate::traits::{PageDocument, PageRenderer, PageSizePt, RenderRequest, TileRect};
use hayro::hayro_interpret::InterpreterSettings;
use hayro::hayro_syntax::Pdf;
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::{PixmapSettings, RenderCache, RenderSettings, render};
use image::RgbaImage;
use std::sync::Arc;

/// Longest rendered side in pixels; matches the datasheet spike guard.
const MAX_SIDE_PX: f32 = 16_000.0;

/// Default pure-Rust PDF backend (hayro).
#[derive(Default)]
pub struct HayroRenderer;

impl PageRenderer for HayroRenderer {
    type Document = HayroDocument;

    fn open(&self, bytes: &[u8]) -> Result<Self::Document, OpenError> {
        HayroDocument::open(bytes)
    }
}

pub struct HayroDocument {
    pdf: Pdf,
    settings: InterpreterSettings,
}

impl HayroDocument {
    pub fn open(bytes: &[u8]) -> Result<Self, OpenError> {
        let pdf = Pdf::new(Arc::new(bytes.to_vec()))
            .map_err(|e| OpenError::Message(format!("{e:?}")))?;
        Ok(Self {
            pdf,
            settings: InterpreterSettings::default(),
        })
    }

    fn page(&self, index: u32) -> Result<hayro::hayro_syntax::page::Page<'_>, RenderError> {
        self.pdf
            .pages()
            .get(index as usize)
            .ok_or(RenderError::PageOutOfRange)
    }

    fn scale_for_page(&self, page: u32, scale: f32) -> Result<f32, RenderError> {
        let page = self.page(page)?;
        let (w, h) = page.render_dimensions();
        if !(w > 0.0 && h > 0.0) {
            return Err(RenderError::Failed("The page has no drawable area.".into()));
        }
        let mut out = scale.max(0.05).min(8.0);
        let longest = w.max(h) * out;
        if longest > MAX_SIDE_PX {
            out *= MAX_SIDE_PX / longest;
        }
        Ok(out)
    }

    fn crop(image: RgbaImage, tile: TileRect) -> Result<RgbaImage, RenderError> {
        let full_w = image.width();
        let full_h = image.height();
        if tile.x >= full_w || tile.y >= full_h {
            return Err(RenderError::Failed("The tile lies outside the page.".into()));
        }
        let w = tile.width.min(full_w - tile.x);
        let h = tile.height.min(full_h - tile.y);
        if w == 0 || h == 0 {
            return Err(RenderError::Failed("The tile has no pixels.".into()));
        }
        let mut out = RgbaImage::new(w, h);
        for y in 0..h {
            for x in 0..w {
                let px = image.get_pixel(tile.x + x, tile.y + y);
                out.put_pixel(x, y, *px);
            }
        }
        Ok(out)
    }
}

impl PageDocument for HayroDocument {
    fn page_count(&self) -> u32 {
        self.pdf.pages().len().min(u32::MAX as usize) as u32
    }

    fn page_size_pt(&self, page: u32) -> Result<PageSizePt, RenderError> {
        let page = self.page(page)?;
        let (w, h) = page.render_dimensions();
        Ok(PageSizePt {
            width: w,
            height: h,
        })
    }

    fn render(&self, request: RenderRequest) -> Result<RgbaImage, RenderError> {
        let page = self.page(request.page)?;
        let scale = self.scale_for_page(request.page, request.scale)?;
        let cache = RenderCache::new();
        let rendered = render(
            page,
            &cache,
            &self.settings,
            &RenderSettings::default(),
            &PixmapSettings {
                x_scale: scale,
                y_scale: scale,
                bg_color: WHITE,
            },
        );
        let rgba = rendered.data_as_u8_slice();
        let w = u32::from(rendered.width());
        let h = u32::from(rendered.height());
        let image = RgbaImage::from_raw(w, h, rgba.to_vec()).ok_or(RenderError::Failed(
            "The page bitmap could not be built.".into(),
        ))?;
        match request.tile {
            None => Ok(image),
            Some(tile) => Self::crop(image, tile),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::PageRenderer;

    #[test]
    fn hayro_opens_fixture() -> Result<(), Box<dyn std::error::Error>> {
        let bytes = include_bytes!("../tests/fixtures/one_page.pdf");
        let doc = HayroRenderer.open(bytes)?;
        assert_eq!(doc.page_count(), 1);
        let size = doc.page_size_pt(0)?;
        assert!(size.width > 0.0 && size.height > 0.0);
        let bmp = doc.render(RenderRequest {
            page: 0,
            scale: 1.0,
            tile: None,
        })?;
        assert!(bmp.width() > 10 && bmp.height() > 10);
        Ok(())
    }
}
