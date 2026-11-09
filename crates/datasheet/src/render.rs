//! Rasterise the first page of a PDF for the browser reader.

use hayro::hayro_syntax::Pdf;
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::{PixmapSettings, RenderCache, RenderSettings, render};
use std::sync::Arc;

/// Longest page side rendered in device pixels.
const MAX_SIDE_PX: f32 = 4096.0;

/// One RGBA page bitmap (straight alpha, opaque white background).
pub struct PageBitmap {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Render page `index` (0-based) at `dpi` (72 = one pixel per point).
pub fn render_page(bytes: &[u8], index: usize, dpi: f32) -> Result<PageBitmap, String> {
    let pdf = Pdf::new(Arc::new(bytes.to_vec())).map_err(|e| format!("open: {e:?}"))?;
    let pages = pdf.pages();
    let page = pages
        .get(index)
        .ok_or_else(|| "That PDF has no pages.".to_string())?;
    let (w, h) = page.render_dimensions();
    if !(w > 0.0 && h > 0.0) {
        return Err("That page has no size.".into());
    }
    let mut scale = dpi / 72.0;
    let longest = w.max(h) * scale;
    if longest > MAX_SIDE_PX {
        scale *= MAX_SIDE_PX / longest;
    }
    let cache = RenderCache::new();
    let pix = render(
        page,
        &cache,
        &hayro::hayro_interpret::InterpreterSettings::default(),
        &RenderSettings::default(),
        &PixmapSettings {
            x_scale: scale,
            y_scale: scale,
            bg_color: WHITE,
        },
    );
    Ok(PageBitmap {
        width: u32::from(pix.width()),
        height: u32::from(pix.height()),
        rgba: pix.data_as_u8_slice().to_vec(),
    })
}
