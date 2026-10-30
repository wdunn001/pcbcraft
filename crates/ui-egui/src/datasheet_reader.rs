//! Browser datasheet reader: drop, file pick, cache record and page 1 preview.
//!
//! Context: issue #720. The desktop build still opens http(s) links in the system
//! browser; the wasm build shows this window instead.
//!
//! Rules: cross-origin fetch is attempted once, then a plain blocked message with
//! the link; bytes never leave the device. No unwrap on user input.
//!
//! # Dependencies
//! - `pcbcraft-datasheet` for gate, cache record, LRU and render
//!
//! # Do not
//! - Proxy manufacturer PDFs through the lab; link-out stays the default.

use egui::{ColorImage, Context, TextureHandle, TextureOptions};
use crate::datasheet_browser_cache;
use pcbcraft_datasheet::gate::DEFAULT_CACHE_CAP_BYTES;
use pcbcraft_datasheet::{ingest_pdf, lru, record::CacheRecord, render};
use pcbcraft_text::t;
use std::collections::HashMap;

const FETCH_SLOT: egui::Id = egui::Id::new("datasheet_fetch");
const BYTES_SLOT: egui::Id = egui::Id::new("datasheet_bytes");

/// Fixed clock for tests so records stay deterministic.
#[cfg(test)]
const TEST_NOW: &str = "2026-01-01T12:00:00Z";

#[cfg(not(test))]
fn now_rfc3339() -> String {
    format!(
        "unix-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    )
}

#[cfg(test)]
fn now_rfc3339() -> String {
    TEST_NOW.to_owned()
}

/// Result of a background PDF fetch (wasm only).
#[derive(Clone)]
enum FetchDone {
    Ok(Vec<u8>),
    Err,
}

/// UI state for the browser datasheet pane.
#[derive(Default)]
pub(crate) struct State {
    open: bool,
    source_url: Option<String>,
    status: String,
    blocked_url: Option<String>,
    page_texture: Option<TextureHandle>,
    texture_serial: u64,
    bytes_by_sha: HashMap<String, Vec<u8>>,
    index: Vec<CacheRecord>,
    cap_bytes: u64,
    fetch_busy: bool,
    pending_sha: Option<String>,
}

impl State {
    pub(crate) fn new_browser() -> Self {
        Self {
            cap_bytes: DEFAULT_CACHE_CAP_BYTES,
            index: datasheet_browser_cache::load_index(),
            ..Default::default()
        }
    }

    /// Open the reader for a part link.
    pub(crate) fn open_for_url(&mut self, ctx: &Context, url: String) {
        self.open = true;
        self.source_url = Some(url.clone());
        self.blocked_url = None;
        self.status = String::new();
        if let Some(bytes) = self.bytes_for_url(&url) {
            let _ = self.show_bytes(ctx, bytes);
            return;
        }
        if let Some(record) = self
            .index
            .iter()
            .find(|r| r.source_url.as_deref() == Some(url.as_str()))
        {
            self.pending_sha = Some(record.sha256.clone());
            datasheet_browser_cache::request_bytes(record.sha256.clone(), BYTES_SLOT, ctx);
            return;
        }
        #[cfg(target_arch = "wasm32")]
        self.start_fetch(ctx, url);
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.blocked_url = Some(url);
            self.status = t!("web.datasheet.blocked");
        }
    }

    /// Apply async fetch results and accept dropped files.
    pub(crate) fn poll(&mut self, ctx: &Context, browser: bool) {
        if browser {
            self.poll_drops(ctx);
            self.poll_fetch(ctx);
            self.poll_picker(ctx);
            self.poll_storage(ctx);
        }
    }

    fn bytes_for_url(&self, url: &str) -> Option<Vec<u8>> {
        let record = self
            .index
            .iter()
            .find(|r| r.source_url.as_deref() == Some(url))?;
        self.bytes_by_sha.get(&record.sha256).cloned()
    }

    fn poll_drops(&mut self, ctx: &Context) {
        let dropped: Vec<Vec<u8>> = ctx.input(|input| {
            input
                .raw
                .dropped_files
                .iter()
                .filter_map(|f| f.bytes.clone())
                .collect()
        });
        for bytes in dropped {
            let _ = self.store_and_render(ctx, bytes, self.source_url.clone(), None);
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn start_fetch(&mut self, ctx: &Context, url: String) {
        if self.fetch_busy {
            return;
        }
        self.fetch_busy = true;
        self.status = t!("web.datasheet.loading");
        let ctx = ctx.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let done = match fetch_pdf_bytes(&url).await {
                Ok(bytes) => FetchDone::Ok(bytes),
                Err(_) => FetchDone::Err,
            };
            ctx.data_mut(|d| d.insert_temp(FETCH_SLOT, done));
            ctx.request_repaint();
        });
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn poll_fetch(&mut self, _ctx: &Context) {}

    #[cfg(target_arch = "wasm32")]
    fn poll_fetch(&mut self, ctx: &Context) {
        let Some(done) = ctx.data_mut(|d| d.get_temp::<FetchDone>(FETCH_SLOT)) else {
            return;
        };
        ctx.data_mut(|d| d.remove::<FetchDone>(FETCH_SLOT));
        self.fetch_busy = false;
        match done {
            FetchDone::Ok(bytes) => {
                let url = self.source_url.clone();
                let _ = self.store_and_render(ctx, bytes, url, None);
            }
            FetchDone::Err => {
                if let Some(url) = self.source_url.clone() {
                    self.blocked_url = Some(url);
                }
                self.status = t!("web.datasheet.blocked");
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn poll_picker(&mut self, ctx: &Context) {
        let Some(bytes) = ctx.data_mut(|d| d.get_temp::<Vec<u8>>(egui::Id::new("datasheet_pick"))) else {
            return;
        };
        ctx.data_mut(|d| d.remove::<Vec<u8>>(egui::Id::new("datasheet_pick")));
        let _ = self.store_and_render(ctx, bytes, self.source_url.clone(), None);
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn poll_picker(&mut self, _ctx: &Context) {}

    fn poll_storage(&mut self, ctx: &Context) {
        let Some(bytes) = ctx.data_mut(|d| d.get_temp::<Vec<u8>>(BYTES_SLOT)) else {
            return;
        };
        ctx.data_mut(|d| d.remove::<Vec<u8>>(BYTES_SLOT));
        if let Some(sha) = self.pending_sha.take() {
            self.bytes_by_sha.insert(sha, bytes.clone());
        }
        let _ = self.show_bytes(ctx, bytes);
    }

    pub(crate) fn window(&mut self, ctx: &Context) {
        if !self.open {
            return;
        }
        let mut open = self.open;
        egui::Window::new(t!("web.datasheet.title"))
            .open(&mut open)
            .default_width(480.0)
            .show(ctx, |ui| {
                ui.label(t!("web.datasheet.drop"));
                ui.horizontal(|ui| {
                    if ui
                        .button(t!("web.datasheet.choose"))
                        .on_hover_text(t!("web.datasheet.choose.tip"))
                        .clicked()
                    {
                        #[cfg(target_arch = "wasm32")]
                        request_pdf_picker(ctx);
                    }
                    if let Some(url) = self.blocked_url.as_ref() {
                        if ui
                            .button(t!("web.datasheet.open_link"))
                            .on_hover_text(t!("web.datasheet.open_link.tip"))
                            .clicked()
                        {
                            ctx.open_url(egui::OpenUrl::new_tab(url.clone()));
                        }
                    }
                });
                if !self.status.is_empty() {
                    ui.label(&self.status);
                }
                if let Some(url) = self.blocked_url.as_ref() {
                    ui.hyperlink_to(url, url);
                }
                if self.page_texture.is_some() {
                    ui.label(t!("web.datasheet.page"));
                }
                if let Some(tex) = &self.page_texture {
                    ui.image(tex);
                }
            });
        self.open = open;
    }

    fn store_and_render(
        &mut self,
        ctx: &Context,
        bytes: Vec<u8>,
        source_url: Option<String>,
        etag: Option<String>,
    ) -> Result<(), String> {
        let (mut record, bytes) = ingest_pdf(bytes, source_url, etag, &now_rfc3339())?;
        record.touch(&now_rfc3339());
        let key = record.sha256.clone();
        self.bytes_by_sha.insert(key.clone(), bytes.clone());
        self.index.retain(|r| r.sha256 != key);
        self.index.push(record);
        let dropped = lru::evict_for_cap(&mut self.index, self.cap_bytes);
        for sha in dropped {
            self.bytes_by_sha.remove(&sha);
        }
        self.blocked_url = None;
        self.status = String::new();
        datasheet_browser_cache::save_index(&self.index);
        datasheet_browser_cache::store_bytes(&key, &bytes);
        self.show_bytes(ctx, bytes)
    }

    fn show_bytes(&mut self, ctx: &Context, bytes: Vec<u8>) -> Result<(), String> {
        let page = render::render_page(&bytes, 0, 96.0)?;
        let image = ColorImage::from_rgba_unmultiplied(
            [page.width as usize, page.height as usize],
            &page.rgba,
        );
        self.texture_serial = self.texture_serial.wrapping_add(1);
        self.page_texture = Some(ctx.load_texture(
            format!("datasheet-page-{}", self.texture_serial),
            image,
            TextureOptions::LINEAR,
        ));
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn test_show_pdf(&mut self, ctx: &Context, bytes: Vec<u8>) -> Result<(), String> {
        self.open = true;
        self.store_and_render(ctx, bytes, None, None)
    }
}

#[cfg(target_arch = "wasm32")]
async fn fetch_pdf_bytes(url: &str) -> Result<Vec<u8>, String> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;
    use web_sys::{Request, RequestInit, RequestMode, Response};

    let gate_url = pcbcraft_datasheet::gate::http_url(Some(url))
        .ok_or_else(|| "bad url".to_string())?;
    let opts = RequestInit::new();
    opts.set_method("GET");
    opts.set_mode(RequestMode::Cors);
    let request = Request::new_with_str_and_init(&gate_url, &opts)
        .map_err(|_| "request failed".to_string())?;
    let window = web_sys::window().ok_or_else(|| "no window".to_string())?;
    let resp_value = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|_| "fetch blocked".to_string())?;
    let resp: Response = resp_value
        .dyn_into()
        .map_err(|_| "bad response".to_string())?;
    if !resp.ok() {
        return Err(format!("http {}", resp.status()));
    }
    let buf = JsFuture::from(
        resp.array_buffer()
            .map_err(|_| "no body".to_string())?,
    )
    .await
    .map_err(|_| "read failed".to_string())?;
    let u8arr = js_sys::Uint8Array::new(&buf);
    let mut bytes = vec![0u8; u8arr.length() as usize];
    u8arr.copy_to(&mut bytes);
    pcbcraft_datasheet::gate::validate_pdf_bytes(&bytes)?;
    Ok(bytes)
}

#[cfg(target_arch = "wasm32")]
fn request_pdf_picker(ctx: &Context) {
    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::JsCast;
    let Some(window) = web_sys::window() else {
        return;
    };
    let Some(document) = window.document() else {
        return;
    };
    let Ok(input) = document.create_element("input") else {
        return;
    };
    let Ok(input) = input.dyn_into::<web_sys::HtmlInputElement>() else {
        return;
    };
    let _ = input.set_attribute("type", "file");
    let _ = input.set_attribute("accept", "application/pdf,.pdf");
    let ctx = ctx.clone();
    let closure = Closure::wrap(Box::new(move |event: web_sys::Event| {
        let Some(target) = event.target() else {
            return;
        };
        let Ok(input) = target.dyn_into::<web_sys::HtmlInputElement>() else {
            return;
        };
        let Some(files) = input.files() else {
            return;
        };
        if files.length() == 0 {
            return;
        }
        let Some(file) = files.get(0) else {
            return;
        };
        let ctx = ctx.clone();
        wasm_bindgen_futures::spawn_local(async move {
            if let Ok(bytes) = read_file_as_bytes(&file).await {
                ctx.data_mut(|d| d.insert_temp(egui::Id::new("datasheet_pick"), bytes));
                ctx.request_repaint();
            }
        });
    }) as Box<dyn FnMut(_)>);
    input.set_onchange(Some(closure.as_ref().unchecked_ref()));
    closure.forget();
    let _ = input.click();
}

#[cfg(target_arch = "wasm32")]
async fn read_file_as_bytes(file: &web_sys::File) -> Result<Vec<u8>, String> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;
    let reader = web_sys::FileReader::new().map_err(|_| "reader".to_string())?;
    reader.read_as_array_buffer(file).map_err(|_| "read".to_string())?;
    let promise = reader.array_buffer().map_err(|_| "buffer".to_string())?;
    let buf = JsFuture::from(promise)
        .await
        .map_err(|_| "wait".to_string())?;
    let u8arr = js_sys::Uint8Array::new(&buf);
    let mut bytes = vec![0u8; u8arr.length() as usize];
    u8arr.copy_to(&mut bytes);
    Ok(bytes)
}
