//! The default renderer and text layer: hayro, a pure-Rust PDF rasteriser.
//!
//! Context: spike #723 measured it on 52 public datasheets (580 pages): no real faults, text
//! layer word coverage 99.6 percent, wasm builds. One parse feeds the pixels and the text.
//!
//! Local rules: the page box used is the CropBox when the file has one, as viewers do. Spaces
//! and line breaks are synthesised from glyph geometry because most datasheet generators place
//! words instead of emitting space glyphs. Caches live for one call, so a `HayroBackend` holds
//! only the parsed document and can be shared between threads behind a lock.
//!
//! # Dependencies
//! - crates/library/src/datasheet/mod.rs (the traits and types)
//!
//! # Do not
//! - Run a PDF script or follow an external link; hayro does neither.
use super::{
    Bitmap, CharBox, DatasheetError, DocInfo, MAX_PAGES, MAX_SIDE_PX, PageRenderer, PxRect,
    TextLayer, check_pdf,
};
use hayro::hayro_interpret::font::{Glyph, GlyphRun};
use hayro::hayro_interpret::{
    BlendMode, ClipPath, Context, Device, DrawMode, DrawProps, Image, ImageDrawProps,
    InterpreterCache, InterpreterSettings, SoftMask, interpret_page,
};
use hayro::hayro_syntax::Pdf;
use hayro::kurbo::{Affine, BezPath, Rect, Shape};
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::{PixmapSettings, RenderCache, RenderSettings, render};
use std::sync::Arc;

/// The hayro backend. Holds the opened document.
#[derive(Default)]
pub struct HayroBackend {
    pdf: Option<Pdf>,
}

impl HayroBackend {
    /// An empty backend; call [`PageRenderer::open`] before anything else.
    pub fn new() -> Self {
        Self::default()
    }

    fn doc(&self) -> Result<&Pdf, DatasheetError> {
        self.pdf.as_ref().ok_or(DatasheetError::NotOpen)
    }
}

/// Contract: crates/library/src/datasheet/mod.rs
impl PageRenderer for HayroBackend {
    fn open(&mut self, bytes: Vec<u8>) -> Result<DocInfo, DatasheetError> {
        check_pdf(&bytes)?;
        let pdf = Pdf::new(Arc::new(bytes)).map_err(|e| DatasheetError::Open(format!("{e:?}")))?;
        let pages = pdf.pages();
        if pages.len() > MAX_PAGES {
            return Err(DatasheetError::TooManyPages);
        }
        let sizes: Vec<(f32, f32)> = pages.iter().map(|p| p.render_dimensions()).collect();
        if sizes.is_empty() {
            return Err(DatasheetError::Open("it has no pages".into()));
        }
        self.pdf = Some(pdf);
        Ok(DocInfo { sizes })
    }

    fn render(
        &self,
        page: usize,
        scale: f32,
        tile: Option<PxRect>,
    ) -> Result<Bitmap, DatasheetError> {
        let pdf = self.doc()?;
        let pages = pdf.pages();
        let p = pages.get(page).ok_or(DatasheetError::NoSuchPage)?;
        let (w, h) = p.render_dimensions();
        if !(w > 0.0 && h > 0.0 && scale > 0.0 && scale.is_finite()) {
            return Err(DatasheetError::Page("empty page box".into()));
        }
        let mut s = scale;
        let longest = w.max(h) * s;
        if longest > MAX_SIDE_PX {
            s *= MAX_SIDE_PX / longest;
        }
        let cache = RenderCache::new();
        let pix = render(
            p,
            &cache,
            &InterpreterSettings::default(),
            &RenderSettings::default(),
            &PixmapSettings {
                x_scale: s,
                y_scale: s,
                bg_color: WHITE,
            },
        );
        let (pw, ph) = (u32::from(pix.width()), u32::from(pix.height()));
        let data = pix.data_as_u8_slice();
        let Some(t) = tile else {
            return Ok(Bitmap {
                width: pw,
                height: ph,
                rgba: data.to_vec(),
            });
        };
        let (x1, y1) = (
            t.x.saturating_add(t.w).min(pw),
            t.y.saturating_add(t.h).min(ph),
        );
        if t.x >= x1 || t.y >= y1 {
            return Err(DatasheetError::Page("the tile is outside the page".into()));
        }
        let mut rgba = Vec::with_capacity(((x1 - t.x) * (y1 - t.y) * 4) as usize);
        for row in t.y..y1 {
            let start = ((row * pw + t.x) * 4) as usize;
            let end = ((row * pw + x1) * 4) as usize;
            match data.get(start..end) {
                Some(slice) => rgba.extend_from_slice(slice),
                None => return Err(DatasheetError::Page("the bitmap is short".into())),
            }
        }
        Ok(Bitmap {
            width: x1 - t.x,
            height: y1 - t.y,
            rgba,
        })
    }
}

/// Records glyph boxes and drops all painting.
struct TextDevice {
    out: Vec<CharBox>,
}

impl<'a> Device<'a> for TextDevice {
    fn draw_path(&mut self, _: &BezPath, _: DrawProps<'a>, _: &DrawMode) {}
    fn push_clip_path(&mut self, _: &ClipPath) {}
    fn push_transparency_group(&mut self, _: f32, _: Option<SoftMask<'a>>, _: BlendMode) {}
    fn draw_glyph_run(&mut self, run: &GlyphRun<'_, 'a>, props: DrawProps<'a>, _mode: &DrawMode) {
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
                        // The em box vertically gives a glyph the line height; the outline
                        // extent horizontally keeps narrow letters narrow.
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

/// Contract: crates/library/src/datasheet/mod.rs
impl TextLayer for HayroBackend {
    fn chars(&self, page: usize) -> Result<Vec<CharBox>, DatasheetError> {
        let pdf = self.doc()?;
        let pages = pdf.pages();
        let p = pages.get(page).ok_or(DatasheetError::NoSuchPage)?;
        let initial: Affine = {
            use hayro::hayro_interpret::util::TransformExt;
            p.initial_transform(true).to_kurbo()
        };
        let (w, h) = p.render_dimensions();
        let cache = InterpreterCache::new();
        let mut ctx = Context::new(
            initial,
            Rect::new(0.0, 0.0, f64::from(w), f64::from(h)),
            &cache,
            p.xref(),
            InterpreterSettings::default(),
        );
        let mut dev = TextDevice { out: Vec::new() };
        interpret_page(p, &mut ctx, &mut dev);
        Ok(add_spaces(dev.out))
    }
}

/// Insert spaces and newlines between glyph boxes from simple geometry.
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

#[cfg(test)]
mod tests {
    use super::super::{fixtures, search};
    use super::*;

    #[test]
    fn every_fixture_renders_with_ink_and_its_strings_are_found() -> Result<(), DatasheetError> {
        for f in fixtures::all() {
            let mut b = HayroBackend::new();
            let info = b.open(f.bytes.clone())?;
            let bmp = b.render(0, 1.5, None)?;
            let inked = bmp
                .rgba
                .chunks(4)
                .any(|p| p[0] < 200 || p[1] < 200 || p[2] < 200);
            assert!(inked, "{} has ink", f.name);
            for (page, needle) in &f.expect {
                let layer = b.chars(*page)?;
                let hits = search::find(&layer, needle);
                assert!(!hits.is_empty(), "{}: {needle} on page {page}", f.name);
                let (w, h) = info.sizes[*page];
                let hit = &hits[0];
                assert!(
                    hit.x0 >= -1.0 && hit.x1 <= w + 1.0 && hit.y0 >= -1.0 && hit.y1 <= h + 1.0,
                    "{}: the box lies on the page",
                    f.name
                );
            }
        }
        Ok(())
    }

    #[test]
    fn a_tile_is_cut_from_the_page_and_a_rotated_page_swaps_its_size() -> Result<(), DatasheetError>
    {
        let all = fixtures::all();
        let two = all
            .iter()
            .find(|f| f.name == "two-pages.pdf")
            .map(|f| f.bytes.clone());
        let mut b = HayroBackend::new();
        let info = b.open(two.unwrap_or_default())?;
        assert_eq!(info.sizes.len(), 2);
        let tile = b.render(
            0,
            1.0,
            Some(PxRect {
                x: 10,
                y: 20,
                w: 100,
                h: 50,
            }),
        )?;
        assert_eq!((tile.width, tile.height), (100, 50));
        let rot = all
            .iter()
            .find(|f| f.name == "rotated-90.pdf")
            .map(|f| f.bytes.clone());
        let info = HayroBackend::new().open(rot.unwrap_or_default())?;
        assert!(
            info.sizes[0].0 > info.sizes[0].1,
            "landscape after rotation"
        );
        Ok(())
    }

    #[test]
    fn bad_input_is_refused_in_plain_words() {
        let mut b = HayroBackend::new();
        assert_eq!(b.open(b"not a pdf".to_vec()), Err(DatasheetError::NotPdf));
        assert!(matches!(
            b.render(0, 1.0, None),
            Err(DatasheetError::NotOpen)
        ));
    }
}
