//! Rasterise one PDF page for display behind the text layer.

use hayro::hayro_syntax::Pdf;
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::{PixmapSettings, RenderCache, RenderSettings, render};

/// Longest page side the renderer accepts, in device pixels.
pub const MAX_SIDE_PX: f32 = 16000.0;

/// An RGBA bitmap with an opaque white background.
#[derive(Clone, Debug)]
pub struct Bitmap {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Premultiplied RGBA bytes, row-major.
    pub rgba: Vec<u8>,
}

/// Render one page at `dpi` (72 dpi = one pixel per point).
pub fn render_page<'a>(
    pdf: &'a Pdf,
    index: usize,
    dpi: f32,
    cache: &RenderCache<'a>,
) -> Result<Bitmap, String> {
    let pages = pdf.pages();
    let page = pages.get(index).ok_or_else(|| "no such page".to_string())?;
    let (w, h) = page.render_dimensions();
    let mut scale = dpi / 72.0;
    let longest = w.max(h) * scale;
    if longest > MAX_SIDE_PX {
        scale *= MAX_SIDE_PX / longest;
    }
    if !(w > 0.0 && h > 0.0) {
        return Err("empty page box".into());
    }
    let pix = render(
        page,
        cache,
        &hayro::hayro_interpret::InterpreterSettings::default(),
        &RenderSettings::default(),
        &PixmapSettings {
            x_scale: scale,
            y_scale: scale,
            bg_color: WHITE,
        },
    );
    Ok(Bitmap {
        width: u32::from(pix.width()),
        height: u32::from(pix.height()),
        rgba: pix.data_as_u8_slice().to_vec(),
    })
}
