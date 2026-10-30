//! The datasheet reader: a cached PDF drawn page by page, with search, zoom and selectable text.
//!
//! Context: issues #695 and #678, spike #723. The pixels and the text come from the library
//! crate's `PageRenderer` and `TextLayer` (hayro). Work runs on one worker thread per open
//! document (in the browser build, where threads are not available, it runs in the frame).
//! The frame only draws: it asks the worker for a page at a scale and for the text layer, and
//! shows what has arrived.
//!
//! Local rules: textures are made lazily per page and per zoom step, cut into tiles of at most
//! `TILE_PX` pixels so no texture passes the GPU limit, and dropped least recently used first
//! when they pass `TEXTURE_BUDGET_BYTES`. A request for a page is replaced by a newer one for the
//! same page. The text of the first `INDEX_PAGES` pages is kept for search. Nothing here
//! reads the network or a file; the bytes of a cached PDF are handed in.
//!
//! # Dependencies
//! - crates/library/src/datasheet/mod.rs (`PageRenderer`, `TextLayer`, `Bitmap`, `CharBox`, `Hit`)
//! - crates/library/src/datasheet/search.rs (`find`, `text_of`)
//!
//! # Do not
//! - Render on the frame thread on desktop.
//! - Keep a texture for every page of a long manual.
use super::*;
use pcbcraft_library::datasheet::{
    Bitmap, CharBox, DocInfo, HayroBackend, Hit, PageRenderer, TextLayer, search,
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::mpsc::{Receiver, Sender};

/// Largest texture side in pixels; bigger pages are cut into tiles.
pub(crate) const TILE_PX: u32 = 2048;
/// Texture memory kept for pages, in bytes.
pub(crate) const TEXTURE_BUDGET_BYTES: usize = 64 * 1024 * 1024;
/// Pages whose text is indexed for search.
pub(crate) const INDEX_PAGES: usize = 300;
/// Zoom steps in percent of fit-to-width.
pub(crate) const ZOOMS: [u32; 8] = [50, 75, 100, 125, 150, 200, 300, 400];

/// A request to the worker.
enum Req {
    Render { page: usize, step: u32 },
    Text { page: usize },
}

/// What the worker answers.
enum Reply {
    Opened(Result<DocInfo, String>),
    Page {
        page: usize,
        step: u32,
        bitmap: Bitmap,
    },
    PageFailed {
        page: usize,
        why: String,
    },
    Text {
        page: usize,
        chars: Vec<CharBox>,
    },
}

/// Where the renderer runs.
enum Worker {
    #[cfg(not(target_arch = "wasm32"))]
    Thread {
        tx: Sender<Req>,
        rx: Receiver<Reply>,
    },
    /// No threads (the browser build): requests run at once, in the frame.
    Inline {
        backend: Box<HayroBackend>,
        queue: VecDeque<Reply>,
    },
}

impl Worker {
    #[cfg(not(target_arch = "wasm32"))]
    fn spawn(ctx: &egui::Context, bytes: Vec<u8>) -> Worker {
        let (tx, req_rx) = std::sync::mpsc::channel::<Req>();
        let (reply_tx, rx) = std::sync::mpsc::channel::<Reply>();
        let ctx = ctx.clone();
        let started = std::thread::Builder::new()
            .name("pcbcraft-datasheet".into())
            .spawn(move || {
                let mut backend = HayroBackend::new();
                let say = |r: Reply| {
                    let _ = reply_tx.send(r);
                    ctx.request_repaint();
                };
                match backend.open(bytes) {
                    Ok(info) => say(Reply::Opened(Ok(info))),
                    Err(e) => {
                        say(Reply::Opened(Err(e.to_string())));
                        return;
                    }
                }
                while let Ok(first) = req_rx.recv() {
                    // A newer request for the same page replaces an older one.
                    let mut batch = vec![first];
                    batch.extend(req_rx.try_iter());
                    let mut renders: BTreeMap<usize, u32> = BTreeMap::new();
                    let mut texts: Vec<usize> = Vec::new();
                    for r in batch {
                        match r {
                            Req::Render { page, step } => {
                                renders.insert(page, step);
                            }
                            Req::Text { page } => texts.push(page),
                        }
                    }
                    for (page, step) in renders {
                        say(run_render(&backend, page, step));
                    }
                    for page in texts {
                        say(run_text(&backend, page));
                    }
                }
            });
        match started {
            Ok(_) => Worker::Thread { tx, rx },
            Err(_) => {
                let mut queue = VecDeque::new();
                queue.push_back(Reply::Opened(Err(t!("libws.ds.worker_failed"))));
                Worker::Inline {
                    backend: Box::new(HayroBackend::new()),
                    queue,
                }
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn open_inline(bytes: Vec<u8>) -> Worker {
        let mut backend = HayroBackend::new();
        let mut queue = VecDeque::new();
        queue.push_back(Reply::Opened(
            backend.open(bytes).map_err(|e| e.to_string()),
        ));
        Worker::Inline {
            backend: Box::new(backend),
            queue,
        }
    }

    fn send(&mut self, req: Req) {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            Worker::Thread { tx, .. } => {
                let _ = tx.send(req);
            }
            Worker::Inline { backend, queue } => queue.push_back(match req {
                Req::Render { page, step } => run_render(backend, page, step),
                Req::Text { page } => run_text(backend, page),
            }),
        }
    }

    fn poll(&mut self) -> Vec<Reply> {
        match self {
            #[cfg(not(target_arch = "wasm32"))]
            Worker::Thread { rx, .. } => rx.try_iter().collect(),
            Worker::Inline { queue, .. } => queue.drain(..).collect(),
        }
    }
}

/// Pixels per point at a zoom step: the step number over four.
fn step_scale(step: u32) -> f32 {
    step as f32 / 4.0
}

fn run_render(b: &HayroBackend, page: usize, step: u32) -> Reply {
    match b.render(page, step_scale(step), None) {
        Ok(bitmap) => Reply::Page { page, step, bitmap },
        Err(e) => Reply::PageFailed {
            page,
            why: e.to_string(),
        },
    }
}

fn run_text(b: &HayroBackend, page: usize) -> Reply {
    Reply::Text {
        page,
        chars: b.chars(page).unwrap_or_default(),
    }
}

/// The tiles of one rendered page.
struct PageTex {
    tiles: Vec<(egui::Rect, egui::TextureHandle)>,
    bytes: usize,
}

/// A selected run of characters on one page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Selection {
    pub(crate) page: usize,
    pub(crate) anchor: usize,
    pub(crate) head: usize,
}

/// One open datasheet.
pub(crate) struct Reader {
    /// The address the document is for; the reader is for one address at a time.
    pub(crate) url: String,
    worker: Worker,
    /// Page sizes in points once the file opened.
    pub(crate) info: Option<DocInfo>,
    /// Why the file could not be opened.
    pub(crate) failed: Option<String>,
    /// The page showing, 0-based.
    pub(crate) page: usize,
    /// Zoom in percent of fit-to-width.
    pub(crate) zoom: u32,
    textures: BTreeMap<(usize, u32), PageTex>,
    lru: VecDeque<(usize, u32)>,
    asked: BTreeSet<(usize, u32)>,
    pub(crate) text: BTreeMap<usize, Vec<CharBox>>,
    /// The search box.
    pub(crate) query: String,
    /// Matches over the indexed pages, in page order.
    pub(crate) hits: Vec<(usize, Hit)>,
    /// The match the page jumped to.
    pub(crate) hit_at: usize,
    hits_for: (String, usize),
    /// What is selected, if anything.
    pub(crate) selection: Option<Selection>,
}

impl Reader {
    /// Open `bytes` in a worker and ask for the first page's text.
    pub(crate) fn open(ctx: &egui::Context, url: &str, bytes: Vec<u8>) -> Reader {
        #[cfg(not(target_arch = "wasm32"))]
        let worker = Worker::spawn(ctx, bytes);
        #[cfg(target_arch = "wasm32")]
        let worker = {
            let _ = ctx;
            Worker::open_inline(bytes)
        };
        Reader {
            url: url.to_owned(),
            worker,
            info: None,
            failed: None,
            page: 0,
            zoom: 100,
            textures: BTreeMap::new(),
            lru: VecDeque::new(),
            asked: BTreeSet::new(),
            text: BTreeMap::new(),
            query: String::new(),
            hits: Vec::new(),
            hit_at: 0,
            hits_for: (String::new(), 0),
            selection: None,
        }
    }

    /// Take what the worker has finished: the open result, pages as textures, text layers.
    pub(crate) fn poll(&mut self, ctx: &egui::Context) {
        for reply in self.worker.poll() {
            match reply {
                Reply::Opened(Ok(info)) => {
                    let n = info.sizes.len();
                    self.info = Some(info);
                    self.worker.send(Req::Text { page: 0 });
                    for page in 1..n.min(INDEX_PAGES) {
                        self.worker.send(Req::Text { page });
                    }
                }
                Reply::Opened(Err(why)) => self.failed = Some(why),
                Reply::Page { page, step, bitmap } => {
                    self.asked.remove(&(page, step));
                    self.store(ctx, page, step, &bitmap);
                }
                Reply::PageFailed { page, why } => {
                    self.asked.retain(|(p, _)| *p != page);
                    self.failed = Some(why);
                }
                Reply::Text { page, chars } => {
                    self.text.insert(page, chars);
                }
            }
        }
    }

    /// Cut a bitmap into tiles of at most `TILE_PX`, upload them and enforce the texture budget.
    fn store(&mut self, ctx: &egui::Context, page: usize, step: u32, b: &Bitmap) {
        let mut tiles = Vec::new();
        let mut bytes = 0usize;
        let mut y = 0;
        while y < b.height {
            let h = TILE_PX.min(b.height - y);
            let mut x = 0;
            while x < b.width {
                let w = TILE_PX.min(b.width - x);
                let mut rgba = Vec::with_capacity((w * h * 4) as usize);
                for row in y..y + h {
                    let s = ((row * b.width + x) * 4) as usize;
                    if let Some(slice) = b.rgba.get(s..s + (w * 4) as usize) {
                        rgba.extend_from_slice(slice);
                    }
                }
                if rgba.len() == (w * h * 4) as usize {
                    let image =
                        egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba);
                    let handle = ctx.load_texture(
                        format!("datasheet:{page}:{step}:{x}:{y}"),
                        image,
                        egui::TextureOptions::LINEAR,
                    );
                    bytes += rgba.len();
                    // Position as a fraction of the page so any display size can place it.
                    let rect = egui::Rect::from_min_size(
                        egui::pos2(x as f32 / b.width as f32, y as f32 / b.height as f32),
                        egui::vec2(w as f32 / b.width as f32, h as f32 / b.height as f32),
                    );
                    tiles.push((rect, handle));
                }
                x += w;
            }
            y += h;
        }
        self.textures.insert((page, step), PageTex { tiles, bytes });
        self.lru.retain(|k| *k != (page, step));
        self.lru.push_back((page, step));
        let mut total: usize = self.textures.values().map(|t| t.bytes).sum();
        while total > TEXTURE_BUDGET_BYTES && self.lru.len() > 1 {
            if let Some(old) = self.lru.pop_front()
                && old.0 != self.page
                && let Some(t) = self.textures.remove(&old)
            {
                total = total.saturating_sub(t.bytes);
            }
        }
    }

    /// The zoom step (quarters of a pixel per point) for a page drawn `points_wide` wide that is
    /// `page_w` points: pixels per point times the device scale, rounded to a step.
    pub(crate) fn step_for(points_wide: f32, page_w: f32, ppp: f32) -> u32 {
        ((points_wide / page_w.max(1.0)) * ppp * 4.0)
            .round()
            .clamp(1.0, 32.0) as u32
    }

    /// The best texture for `page` at `step` or the nearest one already made.
    fn texture_for(&mut self, page: usize, step: u32) -> Option<&PageTex> {
        if !self.textures.contains_key(&(page, step)) && self.asked.insert((page, step)) {
            self.worker.send(Req::Render { page, step });
        }
        let key = self
            .textures
            .keys()
            .filter(|(p, _)| *p == page)
            .min_by_key(|(_, s)| s.abs_diff(step))
            .copied()?;
        self.lru.retain(|k| *k != key);
        self.lru.push_back(key);
        self.textures.get(&key)
    }

    /// True when the page showing has been drawn and the text layer has arrived, so a capture
    /// image can be taken.
    pub(crate) fn settled(&self) -> bool {
        match &self.info {
            None => self.failed.is_some(),
            Some(i) => {
                self.textures.keys().any(|(p, _)| *p == self.page)
                    && self.text.len() >= i.sizes.len().min(INDEX_PAGES)
            }
        }
    }

    /// Recompute matches when the query or the amount of indexed text changed.
    pub(crate) fn refresh_hits(&mut self) {
        let sig = (self.query.clone(), self.text.len());
        if sig == self.hits_for {
            return;
        }
        let first = self.hits.get(self.hit_at).map(|(p, h)| (*p, h.first));
        self.hits = self
            .text
            .iter()
            .flat_map(|(p, chars)| {
                search::find(chars, &self.query)
                    .into_iter()
                    .map(|h| (*p, h))
            })
            .collect();
        self.hit_at = first
            .and_then(|f| self.hits.iter().position(|(p, h)| (*p, h.first) == f))
            .unwrap_or(0);
        self.hits_for = sig;
    }

    /// Go to the next (or previous) match and show its page.
    pub(crate) fn step_hit(&mut self, forward: bool) {
        if self.hits.is_empty() {
            return;
        }
        let n = self.hits.len();
        self.hit_at = if forward {
            (self.hit_at + 1) % n
        } else {
            (self.hit_at + n - 1) % n
        };
        self.page = self.hits[self.hit_at].0;
    }

    /// Show page `page` (0-based), kept inside the document.
    pub(crate) fn go_to(&mut self, page: usize) {
        let n = self.info.as_ref().map_or(1, |i| i.sizes.len());
        self.page = page.min(n.saturating_sub(1));
        self.selection = None;
    }

    /// The selected text, for copying.
    pub(crate) fn selected_text(&self) -> Option<String> {
        let s = self.selection?;
        let chars = self.text.get(&s.page)?;
        let (a, b) = (s.anchor.min(s.head), s.anchor.max(s.head) + 1);
        Some(search::text_of(chars, a, b))
    }

    /// The index of the character nearest to a point in page points.
    fn char_at(&self, page: usize, x: f32, y: f32) -> Option<usize> {
        let chars = self.text.get(&page)?;
        chars
            .iter()
            .enumerate()
            .filter(|(_, c)| !c.ch.is_whitespace() || c.x1 > c.x0)
            .map(|(i, c)| {
                let dx = (c.x0 - x).max(0.0).max(x - c.x1);
                let dy = (c.y0 - y).max(0.0).max(y - c.y1);
                (i, dx * dx + dy * dy)
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    }

    /// Draw the page in `ui`: texture tiles, match highlights and the selection; handle drags.
    ///
    /// # Rules
    /// - The page is drawn `zoom` percent of the width available.
    /// - A drag over the page selects text; a plain click clears the selection.
    pub(crate) fn show_page(&mut self, ui: &mut egui::Ui, tokens: &crate::theme::Tokens) {
        let Some((pw, ph)) = self
            .info
            .as_ref()
            .and_then(|i| i.sizes.get(self.page).copied())
        else {
            return;
        };
        let width = ui.available_width() * self.zoom as f32 / 100.0;
        let scale = width / pw.max(1.0);
        let size = egui::vec2(width, ph * scale);
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
        let step = Self::step_for(width, pw, ui.ctx().pixels_per_point());
        let page = self.page;
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, egui::Color32::WHITE);
        if let Some(tex) = self.texture_for(page, step) {
            for (frac, handle) in &tex.tiles {
                let at = egui::Rect::from_min_size(
                    rect.min + egui::vec2(frac.min.x * size.x, frac.min.y * size.y),
                    egui::vec2(frac.width() * size.x, frac.height() * size.y),
                );
                painter.image(
                    handle.id(),
                    at,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
            }
        } else {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                t!("libws.ds.drawing"),
                crate::fonts::Role::Body.font(),
                egui::Color32::DARK_GRAY,
            );
        }
        let to_screen = |x: f32, y: f32| rect.min + egui::vec2(x * scale, y * scale);
        for (i, (p, h)) in self.hits.iter().enumerate() {
            if *p != page {
                continue;
            }
            let r = egui::Rect::from_two_pos(to_screen(h.x0, h.y0), to_screen(h.x1, h.y1));
            let fill = if i == self.hit_at {
                tokens.copper.gamma_multiply(0.55)
            } else {
                tokens.gold.gamma_multiply(0.35)
            };
            painter.rect_filled(r, 2.0, fill);
        }
        if let (Some(sel), Some(chars)) = (self.selection, self.text.get(&page))
            && sel.page == page
        {
            for c in chars
                .iter()
                .take(sel.anchor.max(sel.head) + 1)
                .skip(sel.anchor.min(sel.head))
            {
                let r = egui::Rect::from_two_pos(to_screen(c.x0, c.y0), to_screen(c.x1, c.y1));
                painter.rect_filled(r, 0.0, tokens.cyan.gamma_multiply(0.35));
            }
        }
        let to_page =
            |pos: egui::Pos2| ((pos.x - rect.min.x) / scale, (pos.y - rect.min.y) / scale);
        if response.drag_started()
            && let Some(pos) = response.interact_pointer_pos()
        {
            let (x, y) = to_page(pos);
            self.selection = self.char_at(page, x, y).map(|i| Selection {
                page,
                anchor: i,
                head: i,
            });
        } else if response.dragged()
            && let Some(pos) = response.interact_pointer_pos()
        {
            let (x, y) = to_page(pos);
            if let (Some(i), Some(sel)) = (self.char_at(page, x, y), self.selection.as_mut()) {
                sel.head = i;
            }
        } else if response.clicked() {
            self.selection = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_drawn_wider_gets_a_finer_step_and_the_step_is_bounded() {
        assert_eq!(Reader::step_for(612.0, 612.0, 1.0), 4);
        assert_eq!(Reader::step_for(1224.0, 612.0, 1.0), 8);
        assert_eq!(Reader::step_for(10.0, 612.0, 1.0), 1);
        assert_eq!(Reader::step_for(100_000.0, 612.0, 2.0), 32);
    }
}
