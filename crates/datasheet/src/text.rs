//! Extract per-character boxes from a PDF page.

use hayro::hayro_interpret::font::{Glyph, GlyphRun};
use hayro::hayro_interpret::{
    BlendMode, ClipPath, Context, Device, DrawMode, DrawProps, Image, ImageDrawProps,
    InterpreterCache, InterpreterSettings, SoftMask, interpret_page,
};
use hayro::hayro_syntax::Pdf;
use hayro::hayro_syntax::page::Page;
use hayro::kurbo::{Affine, BezPath, Rect, Shape};
use std::sync::Arc;

/// One extracted character with its box in page points (top-left origin, y down).
#[derive(Clone, Debug, PartialEq)]
pub struct CharBox {
    /// The character (a space may be synthesised between words).
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

/// Parse a PDF from bytes. Encrypted or corrupt files return `Err`.
pub fn open(bytes: Vec<u8>) -> Result<Pdf, String> {
    Pdf::new(Arc::new(bytes)).map_err(|e| format!("open: {e:?}"))
}

/// Page size in points after rotation, without rendering.
pub fn page_size(pdf: &Pdf, index: usize) -> Option<(f32, f32)> {
    pdf.pages().get(index).map(|p| p.render_dimensions())
}

struct TextDevice {
    out: Vec<CharBox>,
}

impl<'a> Device<'a> for TextDevice {
    fn draw_path(&mut self, _: &BezPath, _: DrawProps<'a>, _: &DrawMode) {}
    fn push_clip_path(&mut self, _: &ClipPath) {}
    fn push_transparency_group(&mut self, _: f32, _: Option<SoftMask<'a>>, _: BlendMode) {}
    fn draw_glyph_run(&mut self, run: &GlyphRun<'_, 'a>, props: DrawProps<'a>, mode: &DrawMode) {
        let _ = mode;
        for g in run.glyphs() {
            let Some(text) = g.as_unicode() else {
                continue;
            };
            let text = match text {
                hayro::hayro_interpret::hayro_cmap::BfString::Char(c) => c.to_string(),
                hayro::hayro_interpret::hayro_cmap::BfString::String(s) => s,
            };
            let m = props.transform * g.transform();
            let local = match &**g {
                Glyph::Outline(o) => {
                    let b = o.outline().bounding_box();
                    if b.width() > 0.0 && b.height() > 0.0 {
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

/// Extract the text layer of one page in content order with boxes in page points.
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

/// Plain text for accessibility: every character on the page in reading order.
pub fn plain_text(layer: &[CharBox]) -> String {
    layer.iter().map(|c| c.ch).collect()
}

/// True when the page has no characters a person could select or search.
pub fn page_has_text(layer: &[CharBox]) -> bool {
    layer.iter().any(|c| !c.ch.is_whitespace())
}
