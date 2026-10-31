//! Datasheet renderer spike (issue #723, parent #695). Not wired into the app.
//!
//! Context: the Library tab Datasheet page needs a renderer decision. This crate wraps hayro (pure
//! Rust PDF rasteriser) behind two small functions, `render_page` and `text_layer`, so one parse
//! feeds the pixels and the per-character boxes. It is a standalone crate with its own lockfile and
//! is not a member of the app workspace.
//!
//! Rules: page index is 0-based; the text layer is in page points with the origin at the top left of
//! the rendered page; no unwrap, expect or panic on input-derived values; every failure is a
//! `String` the caller records, never a crash.
//!
//! Dependencies: `hayro` (render, Apache-2.0 or MIT), `src/bin/spike.rs` (measurement CLI),
//! `src/fixtures.rs` (generated CI fixtures), `scripts/run-corpus.py` (corpus driver).
//!
//! Do not: commit downloaded datasheets; call this from `crates/` (the app keeps the renderer behind
//! its own trait once the decision lands).

pub mod fixtures;
pub mod search;
#[cfg(target_arch = "wasm32")]
pub mod wasm;

use hayro::hayro_interpret::font::{Glyph, GlyphRun};
use hayro::hayro_interpret::{
    BlendMode, ClipPath, Context, Device, DrawMode, DrawProps, Image, ImageDrawProps,
    InterpreterCache, InterpreterSettings, SoftMask, interpret_page,
};
use hayro::hayro_syntax::Pdf;
use hayro::hayro_syntax::page::Page;
use hayro::kurbo::{Affine, BezPath, Rect, Shape};
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::{PixmapSettings, RenderCache, RenderSettings, render};
use std::sync::Arc;

/// Longest page side the spike renders, in device pixels (the pixmap type is 16-bit).
pub const MAX_SIDE_PX: f32 = 16000.0;

/// An RGBA bitmap with an opaque white background.
pub struct Bitmap {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Premultiplied RGBA bytes, row-major (equal to straight RGBA: the page is opaque).
    pub rgba: Vec<u8>,
}

/// One extracted character with its box in page points (top-left origin, y down).
#[derive(Clone, Debug)]
pub struct CharBox {
    /// The character (a space is synthesised between words).
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

/// Parse a PDF from bytes. Encrypted-with-password and corrupt files return `Err`.
pub fn open(bytes: Vec<u8>) -> Result<Pdf, String> {
    Pdf::new(Arc::new(bytes)).map_err(|e| format!("open: {e:?}"))
}

/// Page size in points after rotation, without rendering.
pub fn page_size(pdf: &Pdf, index: usize) -> Option<(f32, f32)> {
    pdf.pages().get(index).map(|p| p.render_dimensions())
}

/// Render one page at `dpi` (72 dpi = 1 pixel per point). Reuse `cache` across pages of one file.
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
        &InterpreterSettings::default(),
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

/// A device that drops all painting and records glyph boxes for the text layer.
struct TextDevice {
    out: Vec<CharBox>,
}

impl<'a> Device<'a> for TextDevice {
    fn draw_path(&mut self, _: &BezPath, _: DrawProps<'a>, _: &DrawMode) {}
    fn push_clip_path(&mut self, _: &ClipPath) {}
    fn push_transparency_group(&mut self, _: f32, _: Option<SoftMask<'a>>, _: BlendMode) {}
    fn draw_glyph_run(&mut self, run: &GlyphRun<'_, 'a>, props: DrawProps<'a>, mode: &DrawMode) {
        if matches!(mode, DrawMode::Invisible) {
            // Invisible text (scanned pages with an OCR layer) still counts as text.
        }
        for g in run.glyphs() {
            let Some(text) = g.as_unicode() else { continue };
            let text = match text {
                hayro::hayro_interpret::hayro_cmap::BfString::Char(c) => c.to_string(),
                hayro::hayro_interpret::hayro_cmap::BfString::String(s) => s,
            };
            let m = props.transform * g.transform();
            let local = match &**g {
                Glyph::Outline(o) => {
                    let b = o.outline().bounding_box();
                    if b.width() > 0.0 && b.height() > 0.0 {
                        // Use the em box vertically so a glyph box has the line height, and the
                        // outline extent horizontally.
                        Rect::new(b.x0, -200.0, b.x1, 800.0)
                    } else {
                        Rect::new(0.0, -200.0, 0.0, 800.0)
                    }
                }
                Glyph::Type3(_) => Rect::new(0.0, 0.0, 1000.0, 1000.0),
            };
            let bb = (m * local.to_path(0.1)).bounding_box();
            let n = text.chars().count().max(1) as f32;
            for (i, ch) in text.chars().enumerate() {
                let w = (bb.x1 - bb.x0) as f32 / n;
                self.out.push(CharBox {
                    ch,
                    x0: bb.x0 as f32 + w * i as f32,
                    x1: bb.x0 as f32 + w * (i as f32 + 1.0),
                    y0: bb.y0 as f32,
                    y1: bb.y1 as f32,
                });
            }
        }
    }
    fn draw_image(&mut self, _: Image<'a, '_>, _: ImageDrawProps<'a>) {}
    fn pop_clip(&mut self) {}
    fn pop_transparency_group(&mut self) {}
}

/// Extract the text layer of one page: characters in content order with boxes in page points.
/// Spaces and line breaks are synthesised from geometry (a gap wider than a quarter em, or a baseline
/// jump), because most datasheet generators position words instead of emitting space glyphs.
pub fn text_layer<'a>(
    pdf: &'a Pdf,
    index: usize,
    cache: &InterpreterCache<'a>,
) -> Result<Vec<CharBox>, String> {
    let pages = pdf.pages();
    let page: &Page<'a> = pages.get(index).ok_or_else(|| "no such page".to_string())?;
    let initial: Affine = {
        use hayro::hayro_interpret::util::TransformExt;
        page.initial_transform(true).to_kurbo()
    };
    let (w, h) = page.render_dimensions();
    let mut ctx = Context::new(
        initial,
        Rect::new(0.0, 0.0, f64::from(w), f64::from(h)),
        cache,
        page.xref(),
        InterpreterSettings::default(),
    );
    let mut dev = TextDevice { out: Vec::new() };
    interpret_page(page, &mut ctx, &mut dev);
    Ok(add_spaces(dev.out))
}

/// Insert spaces and newlines between glyph boxes using simple geometry.
fn add_spaces(chars: Vec<CharBox>) -> Vec<CharBox> {
    let mut out: Vec<CharBox> = Vec::with_capacity(chars.len() + chars.len() / 4);
    for c in chars {
        if let Some(prev) = out.last().cloned() {
            let h = (prev.y1 - prev.y0).abs().max(1.0);
            let dy = (c.y0 - prev.y0).abs();
            let gap = c.x0 - prev.x1;
            let sep = if dy > 0.5 * h {
                Some('\n')
            } else if gap > 0.25 * h || gap < -2.0 * h {
                Some(' ')
            } else {
                None
            };
            if let Some(s) = sep
                && prev.ch != ' '
                && prev.ch != '\n'
                && !c.ch.is_whitespace()
            {
                out.push(CharBox {
                    ch: s,
                    x0: prev.x1,
                    x1: c.x0.max(prev.x1),
                    y0: prev.y0,
                    y1: prev.y1,
                });
            }
        }
        out.push(c);
    }
    out
}
