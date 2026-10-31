//! In-app datasheet reader for the Library tab: lazy page textures, zoom, scroll, jump.

use crate::a11y::Named;
use egui::{Color32, Context, Rect, TextureHandle, TextureOptions, Ui, Vec2};
use image::RgbaImage;
use pcbcraft_datasheet::{
    HayroDocument, HayroRenderer, MAX_TEXTURE_SIDE, PAGE_GAP_PX, PageDocument, PageRenderer,
    RenderRequest, TileRect, load_bytes, page_height_px, page_offsets, page_width_px,
    scroll_height, tile_grid, visible_pages,
};
use pcbcraft_text::t;
use std::collections::{BTreeSet, HashMap, VecDeque};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};

const TEXTURE_BUDGET: usize = 32;
const PREVIEW_SCALE: f32 = 0.35;

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
struct TileKey {
    page: u32,
    col: u32,
    row: u32,
    preview: bool,
}

pub struct State {
    pub open: bool,
    source: String,
    status: String,
    error: String,
    document: Option<Arc<HayroDocument>>,
    load_rx: Option<Receiver<Result<Arc<HayroDocument>, String>>>,
    scale: f32,
    fit_width: bool,
    jump_draft: String,
    pending_scroll_y: Option<f32>,
    textures: HashMap<TileKey, TextureHandle>,
    lru: VecDeque<TileKey>,
    inflight: BTreeSet<TileKey>,
    render_rx: Vec<(TileKey, Receiver<Result<RgbaImage, String>>)>,
    /// Test hook: bytes loader replaces HTTP for e2e.
    #[cfg(test)]
    pub test_loader: Option<Arc<dyn Fn(&str) -> Result<Vec<u8>, String> + Send + Sync>>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            open: false,
            source: String::new(),
            status: String::new(),
            error: String::new(),
            document: None,
            load_rx: None,
            scale: 1.25,
            fit_width: true,
            jump_draft: String::new(),
            pending_scroll_y: None,
            textures: HashMap::new(),
            lru: VecDeque::new(),
            inflight: BTreeSet::new(),
            render_rx: Vec::new(),
            #[cfg(test)]
            test_loader: None,
        }
    }
}

impl State {
    pub fn open_source(&mut self, ctx: &Context, source: String) {
        self.open = true;
        self.source = source.clone();
        self.error.clear();
        self.status = t!("library.datasheet_reader.loading");
        self.document = None;
        self.textures.clear();
        self.lru.clear();
        self.inflight.clear();
        self.render_rx.clear();
        self.jump_draft = "1".into();
        self.pending_scroll_y = Some(0.0);
        #[cfg(test)]
        let test_loader = self.test_loader.clone();
        let source_for_load = source.clone();
        let load = move || -> Result<Arc<HayroDocument>, String> {
            #[cfg(test)]
            if let Some(load) = &test_loader {
                let bytes = load(&source_for_load)?;
                let doc = HayroRenderer.open(&bytes).map_err(|e| e.to_string())?;
                return Ok(Arc::new(doc));
            }
            let bytes = load_bytes(&source_for_load).map_err(|e| e.to_string())?;
            let doc = HayroRenderer.open(&bytes).map_err(|e| e.to_string())?;
            Ok(Arc::new(doc))
        };
        #[cfg(not(target_arch = "wasm32"))]
        {
            let (tx, rx) = channel();
            self.load_rx = Some(rx);
            let ctx = ctx.clone();
            let _ = std::thread::Builder::new()
                .name("pcbcraft-datasheet-load".into())
                .spawn(move || {
                    let result = load();
                    let _ = tx.send(result);
                    ctx.request_repaint();
                });
        }
        #[cfg(target_arch = "wasm32")]
        {
            match load() {
                Ok(doc) => {
                    self.document = Some(doc);
                    self.status = t!("library.datasheet_reader.ready");
                }
                Err(e) => self.error = e,
            }
        }
    }

    pub fn close(&mut self) {
        self.open = false;
        self.document = None;
        self.load_rx = None;
        self.textures.clear();
        self.lru.clear();
        self.error.clear();
        self.status.clear();
    }

    fn poll_load(&mut self) {
        let msg = self.load_rx.as_ref().and_then(|rx| rx.try_recv().ok());
        if let Some(result) = msg {
            self.load_rx = None;
            match result {
                Ok(doc) => {
                    self.document = Some(doc);
                    self.status = t!("library.datasheet_reader.ready");
                    self.error.clear();
                }
                Err(e) => {
                    self.error = e;
                    self.status.clear();
                }
            }
        }
    }

    fn poll_renders(&mut self, ctx: &Context) {
        let mut done = Vec::new();
        for (i, (key, rx)) in self.render_rx.iter().enumerate() {
            if let Ok(result) = rx.try_recv() {
                done.push(i);
                self.inflight.remove(key);
                match result {
                    Ok(image) => self.store_texture(ctx, *key, image),
                    Err(e) => {
                        if self.error.is_empty() {
                            self.error = e;
                        }
                    }
                }
            }
        }
        for i in done.into_iter().rev() {
            self.render_rx.remove(i);
        }
    }

    fn store_texture(&mut self, ctx: &Context, key: TileKey, image: RgbaImage) {
        if let Some(old) = self.textures.insert(
            key,
            ctx.load_texture(
                format!("datasheet-{key:?}"),
                egui::ColorImage::from_rgba_unmultiplied(
                    [image.width() as usize, image.height() as usize],
                    image.as_raw(),
                ),
                TextureOptions::LINEAR,
            ),
        ) {
            old.destroy(ctx);
        }
        self.lru.retain(|k| k != &key);
        self.lru.push_back(key);
        while self.lru.len() > TEXTURE_BUDGET {
            if let Some(evict) = self.lru.pop_front() {
                if let Some(tex) = self.textures.remove(&evict) {
                    tex.destroy(ctx);
                }
            }
        }
    }

    fn page_heights(&self, scale: f32) -> Vec<f32> {
        let Some(doc) = &self.document else {
            return Vec::new();
        };
        let mut out = Vec::with_capacity(doc.page_count() as usize);
        for page in 0..doc.page_count() {
            if let Ok(size) = doc.page_size_pt(page) {
                out.push(page_height_px(size, scale));
            }
        }
        out
    }

    fn queue_tile(
        &mut self,
        ctx: &Context,
        doc: &Arc<HayroDocument>,
        key: TileKey,
        scale: f32,
        page_w: f32,
        page_h: f32,
    ) {
        if self.textures.contains_key(&key) || self.inflight.contains(&key) {
            return;
        }
        self.inflight.insert(key);
        let render_scale = if key.preview { PREVIEW_SCALE } else { scale };
        let tile = tile_for(key, page_w, page_h, render_scale);
        #[cfg(not(target_arch = "wasm32"))]
        {
            let (tx, rx) = channel();
            self.render_rx.push((key, rx));
            let doc = Arc::clone(doc);
            let ctx = ctx.clone();
            let _ = std::thread::Builder::new()
                .name("pcbcraft-datasheet-render".into())
                .spawn(move || {
                    let result = doc
                        .render(RenderRequest {
                            page: key.page,
                            scale: render_scale,
                            tile,
                        })
                        .map_err(|e| e.to_string());
                    let _ = tx.send(result);
                    ctx.request_repaint();
                });
        }
        #[cfg(target_arch = "wasm32")]
        {
            match doc.render(RenderRequest {
                page: key.page,
                scale: render_scale,
                tile,
            }) {
                Ok(image) => self.store_texture(ctx, key, image),
                Err(e) => self.error = e.to_string(),
            }
            self.inflight.remove(&key);
        }
    }

    fn ensure_tiles(&mut self, ctx: &Context, viewport: Rect, scale: f32) {
        let Some(doc) = self.document.clone() else {
            return;
        };
        let heights = self.page_heights(scale);
        if heights.is_empty() {
            return;
        }
        let scroll_y = self.pending_scroll_y.unwrap_or(0.0);
        let pages = visible_pages(scroll_y, viewport.height(), &heights, 1);
        for page in pages {
            let Ok(size) = doc.page_size_pt(page) else {
                continue;
            };
            let page_w = page_width_px(size, scale);
            let page_h = page_height_px(size, scale);
            let full_w = page_width_px(size, if scale > PREVIEW_SCALE { scale } else { PREVIEW_SCALE });
            let full_h = page_height_px(size, if scale > PREVIEW_SCALE { scale } else { PREVIEW_SCALE });
            let (cols, rows) = tile_grid(full_w as u32, full_h as u32);
            for row in 0..rows {
                for col in 0..cols {
                    let preview_key = TileKey {
                        page,
                        col,
                        row,
                        preview: true,
                    };
                    self.queue_tile(ctx, &doc, preview_key, scale, page_w, page_h);
                    let sharp_key = TileKey {
                        page,
                        col,
                        row,
                        preview: false,
                    };
                    self.queue_tile(ctx, &doc, sharp_key, scale, page_w, page_h);
                }
            }
        }
    }

    pub fn ui(&mut self, ui: &mut Ui, accent: Color32, muted: Color32, library_mode: &mut u8) {
        self.poll_load();
        self.poll_renders(ui.ctx());
        ui.horizontal(|ui| {
            if ui
                .button(t!("library.datasheet_reader.close"))
                .on_hover_text(t!("library.datasheet_reader.close.tip"))
                .clicked()
            {
                self.close();
                *library_mode = 0;
            }
            ui.separator();
            if ui
                .button(t!("library.datasheet_reader.zoom_out"))
                .on_hover_text(t!("library.datasheet_reader.zoom_out.tip"))
                .clicked()
            {
                self.fit_width = false;
                self.scale = (self.scale * 0.85).max(0.2);
            }
            if ui
                .button(t!("library.datasheet_reader.zoom_in"))
                .on_hover_text(t!("library.datasheet_reader.zoom_in.tip"))
                .clicked()
            {
                self.fit_width = false;
                self.scale = (self.scale * 1.15).min(6.0);
            }
            if ui
                .button(t!("library.datasheet_reader.fit_width"))
                .on_hover_text(t!("library.datasheet_reader.fit_width.tip"))
                .clicked()
            {
                self.fit_width = true;
            }
            ui.separator();
            ui.label(t!("library.datasheet_reader.page"));
            ui.add(
                egui::TextEdit::singleline(&mut self.jump_draft)
                    .desired_width(48.0)
                    .named(&t!("library.datasheet_reader.page")),
            )
            .on_hover_text(t!("library.datasheet_reader.page.tip"));
            if ui
                .button(t!("library.datasheet_reader.go"))
                .on_hover_text(t!("library.datasheet_reader.go.tip"))
                .clicked()
            {
                self.jump_to_page();
            }
            if let Some(doc) = &self.document {
                ui.label(
                    egui::RichText::new(t!(
                        "library.datasheet_reader.of_pages",
                        pages = doc.page_count()
                    ))
                    .small()
                    .color(muted),
                )
                .on_hover_text(t!("library.datasheet_reader.of_pages.tip"));
            }
        });
        if !self.source.is_empty() {
            ui.label(
                egui::RichText::new(&self.source)
                    .small()
                    .color(muted),
            )
            .on_hover_text(t!("library.datasheet_reader.source.tip"));
        }
        if !self.status.is_empty() {
            ui.label(&self.status);
        }
        if !self.error.is_empty() {
            ui.colored_label(Color32::from_rgb(200, 80, 80), &self.error);
        }
        let Some(doc) = self.document.clone() else {
            return;
        };
        let viewport = ui.available_rect_before_wrap();
        let mut scale = self.scale;
        if self.fit_width {
            if let Ok(size) = doc.page_size_pt(0) {
                scale = (viewport.width() / page_width_px(size, 1.0)).clamp(0.2, 6.0);
            }
        }
        self.scale = scale;
        let heights = self.page_heights(scale);
        let total_h = scroll_height(&heights);
        self.ensure_tiles(ui.ctx(), viewport, scale);
        egui::ScrollArea::vertical()
            .id_salt("library_datasheet_reader")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if let Some(y) = self.pending_scroll_y.take() {
                    ui.scroll_to_rect(
                        Rect::from_min_size(egui::pos2(0.0, y), Vec2::ZERO),
                        Some(egui::Align::TOP),
                    );
                }
                ui.set_min_height(total_h);
                let offsets = page_offsets(&heights);
                let scroll_y = (-ui.min_rect().top()).max(0.0);
                self.pending_scroll_y = Some(scroll_y);
                for (page, (&top, &height)) in offsets.iter().zip(heights.iter()).enumerate() {
                    let page = page as u32;
                    if !visible_pages(scroll_y, viewport.height(), &heights, 1).contains(&page) {
                        ui.add_space(height + PAGE_GAP_PX);
                        continue;
                    }
                    let Ok(size) = doc.page_size_pt(page) else {
                        continue;
                    };
                    let page_w = page_width_px(size, scale);
                    ui.horizontal(|ui| {
                        ui.add_space(((viewport.width() - page_w) * 0.5).max(0.0));
                        let (rect, _) =
                            ui.allocate_exact_size(Vec2::new(page_w, height), egui::Sense::hover());
                        paint_page(ui, rect, page, &self.textures, accent);
                    });
                    if page + 1 < doc.page_count() {
                        ui.add_space(PAGE_GAP_PX);
                    }
                }
            });
    }

    fn jump_to_page(&mut self) {
        let Some(doc) = &self.document else {
            return;
        };
        let Ok(mut one_based) = self.jump_draft.trim().parse::<u32>() else {
            self.error = t!("library.datasheet_reader.bad_page");
            return;
        };
        if one_based == 0 {
            one_based = 1;
        }
        if one_based > doc.page_count() {
            self.error = t!("library.datasheet_reader.bad_page");
            return;
        }
        self.error.clear();
        let scale = self.scale;
        let heights = self.page_heights(scale);
        let idx = (one_based - 1) as usize;
        if let Some(top) = page_offsets(&heights).get(idx) {
            self.pending_scroll_y = Some(*top);
        }
    }
}

fn tile_for(key: TileKey, page_w: f32, page_h: f32, scale: f32) -> Option<TileRect> {
    let w = page_w as u32;
    let h = page_h as u32;
    if w <= MAX_TEXTURE_SIDE && h <= MAX_TEXTURE_SIDE {
        return None;
    }
    let side = MAX_TEXTURE_SIDE;
    Some(TileRect {
        x: key.col * side,
        y: key.row * side,
        width: side.min(w.saturating_sub(key.col * side)),
        height: side.min(h.saturating_sub(key.row * side)),
    })
}

fn paint_page(ui: &mut Ui, rect: Rect, page: u32, textures: &HashMap<TileKey, TextureHandle>, accent: Color32) {
    ui.painter().rect_stroke(rect, 0.0, egui::Stroke::new(1.0, accent), egui::StrokeKind::Inside);
    let (cols, rows) = tile_grid(rect.width() as u32, rect.height() as u32);
    for row in 0..rows {
        for col in 0..cols {
            let sharp = TileKey {
                page,
                col,
                row,
                preview: false,
            };
            let preview = TileKey {
                page,
                col,
                row,
                preview: true,
            };
            let key = if textures.contains_key(&sharp) {
                sharp
            } else {
                preview
            };
            if let Some(tex) = textures.get(&key) {
                let side = MAX_TEXTURE_SIDE as f32;
                let tile_rect = Rect::from_min_size(
                    rect.min + Vec2::new(col as f32 * side, row as f32 * side),
                    Vec2::new(side.min(rect.width()), side.min(rect.height())),
                );
                ui.painter().image(
                    tex.id(),
                    tile_rect,
                    Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
        }
    }
}

#[cfg(test)]
pub fn fixture_pdf_path() -> String {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .join("../datasheet/tests/fixtures/one_page.pdf")
        .to_string_lossy()
        .into_owned()
}
