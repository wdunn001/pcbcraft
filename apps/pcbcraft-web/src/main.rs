//! Browser entry point: starts the egui app on the page's canvas.
//!
//! Local rules: the page text that appears around the wasm lives in
//! `index.html` and `webgpu-check.js`; its wording must equal the catalog keys
//! `web.needs_webgpu`, `web.crashed`, `web.reload` and `web.reload.tip` (test
//! `the_page_message_matches_the_catalog`). The page script decides before the
//! wasm downloads whether the browser has WebGPU or WebGL 2; a failed start or
//! a panic after load shows the crash notice with a reload button.
//!
//! The online part source runs through the browser's own fetch
//! (`browser_fetch.rs`), started by `crates/library/src/web_transport.rs`.
//!
//! # Dependencies
//! - `apps/pcbcraft-web/src/browser_fetch.rs` (the browser transport, wasm only)
//! - `crates/text/strings/en.txt` (message text)
<<<<<<< ours
//! - `apps/pcbcraft-web/index.html`
//! - `apps/pcbcraft-web/webgpu-check.js`
//! - `apps/pcbcraft-web/Trunk.toml` (the loader that waits for the check)
=======
//! - `apps/pcbcraft-web/index.html` and `apps/pcbcraft-web/webgpu-check.js`
//! - `apps/pcbcraft-web/src/transport.rs` (online library fetch queue)
>>>>>>> theirs
//!
//! # Do not
//! - Add text literals here; read them from the catalog.

#[cfg(target_arch = "wasm32")]
mod browser_fetch;

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!("Build the web app with trunk in apps/pcbcraft-web.");
}

#[cfg(target_arch = "wasm32")]
fn main() {
    wasm_bindgen_futures::spawn_local(async {
        if let Err(error) = start().await {
            web_sys::console::error_1(&error);
            show_crash_notice();
        }
    });
}

#[cfg(target_arch = "wasm32")]
mod wasm_host;

#[cfg(target_arch = "wasm32")]
async fn start() -> Result<(), wasm_bindgen::JsValue> {
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;

    use pcbcraft_web::transport::{
        BrowserTransport, CrossOriginStatus, PollingFetcher, cross_origin_status,
    };
    use wasm_bindgen::JsCast;

    let document = web_sys::window()
        .and_then(|window| window.document())
        .ok_or_else(|| wasm_bindgen::JsValue::from_str("browser document unavailable"))?;
    let _wasm_host = wasm_host::WasmHost::mount(&document)?;
    let canvas = document
        .get_element_by_id("pcbcraft")
        .ok_or_else(|| wasm_bindgen::JsValue::from_str("missing pcbcraft canvas"))?
        .dyn_into::<web_sys::HtmlCanvasElement>()?;
    let session = pcbcraft_mcp::shared(demo_session());
<<<<<<< ours
<<<<<<< ours
<<<<<<< ours
    let offline = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let transport = browser_fetch::transport(offline.clone());
=======
    let transport = Arc::new(BrowserTransport::live());
    let offline = Arc::new(AtomicBool::new(false));
>>>>>>> theirs
=======
    let offline = Arc::new(AtomicBool::new(false));
    let transport = Arc::new(BrowserTransport::live(Arc::clone(&offline)));
>>>>>>> theirs
    eframe::WebRunner::new()
=======
    let runner = eframe::WebRunner::new();
    // eframe installs its own panic hook when the runner is made; chain after
    // it so a panic in any frame also puts the notice on the page.
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        previous(info);
        show_crash_notice();
    }));
    runner
>>>>>>> theirs
        .start(
            canvas,
            eframe::WebOptions::default(),
            Box::new(move |cc| {
<<<<<<< ours
<<<<<<< ours
<<<<<<< ours
                Ok(Box::new(WithFonts::new(pcbcraft_ui_egui::App::new(
                    cc, session, None, None, false,
                ))))
=======
                Ok(Box::new(
                    pcbcraft_ui_egui::App::new(cc, session, None, None, false)
                        .with_web_transport(transport, offline),
                ))
>>>>>>> theirs
=======
                let mut app =
                    pcbcraft_ui_egui::App::new(cc, session, None, None, false);
=======
                let mut app = pcbcraft_ui_egui::App::new(cc, session, None, None, false);
>>>>>>> theirs
                if cross_origin_status() != CrossOriginStatus::Refused {
                    app = app.with_online_fetcher(
                        Box::new(PollingFetcher::new(Arc::clone(&transport))),
                        offline,
                    );
                }
                Ok(Box::new(WebShell {
                    inner: app,
                    transport,
                }))
>>>>>>> theirs
            }),
        )
        .await
}

<<<<<<< ours
<<<<<<< ours
/// Wraps the app so selecting Japanese or Chinese fetches that language's
/// font file over HTTP, exactly once, and installs it as a fallback. The
/// font lives beside the site as `fonts/<file>` (brotli-compressed by the
/// build), never inside the wasm. A failed fetch keeps the default fonts
/// and says so in the status line.
#[cfg(target_arch = "wasm32")]
struct WithFonts {
    inner: pcbcraft_ui_egui::App,
    /// File already fetched or being fetched, so one download per file.
    fetching: Option<String>,
    /// File whose fetch failed; the fetch task parks it here and the next
    /// frame turns it into a status-line sentence.
    failed: std::rc::Rc<std::cell::Cell<Option<String>>>,
}

#[cfg(target_arch = "wasm32")]
impl WithFonts {
    fn new(inner: pcbcraft_ui_egui::App) -> Self {
        Self {
            inner,
            fetching: None,
            failed: std::rc::Rc::new(std::cell::Cell::new(None)),
        }
    }

    /// Start the fetch for the current language's font file, if any.
    fn start_fetch(&mut self, ctx: &eframe::egui::Context) {
        let tag = self.inner.language();
        let Some(file) = pcbcraft_ui_egui::fonts::font_file_for(&tag) else {
            return;
        };
        if self.fetching.as_deref() == Some(file) {
            return;
        }
        self.fetching = Some(file.to_string());
        let ctx = ctx.clone();
        let failed = self.failed.clone();
        wasm_bindgen_futures::spawn_local(async move {
            match load_font_file(file).await {
                Ok(bytes) => {
                    pcbcraft_ui_egui::fonts::add_fallback(
                        &ctx,
                        pcbcraft_ui_egui::fonts::font_stem(file),
                        bytes,
                    );
                    ctx.request_repaint();
                }
                Err(_) => failed.set(Some(file.to_string())),
            }
        });
    }

    /// Turn a parked fetch failure into the status-line sentence. The
    /// app keeps working with the default fonts.
    fn report_failures(&mut self) {
        if let Some(file) = self.failed.take() {
            self.inner.report_missing_font(&file);
        }
    }
}

/// Fetch `fonts/<file>` and return its bytes.
#[cfg(target_arch = "wasm32")]
async fn load_font_file(file: &str) -> Result<Vec<u8>, wasm_bindgen::JsValue> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;
    let window = web_sys::window().ok_or_else(|| wasm_bindgen::JsValue::from_str("no window"))?;
    let response = JsFuture::from(window.fetch_with_str(&format!("fonts/{file}"))).await?;
    let response: web_sys::Response = response.dyn_into()?;
    if !response.ok() {
        return Err(wasm_bindgen::JsValue::from_str("font file not found"));
    }
    let body = response.array_buffer()?;
    let buffer = JsFuture::from(body).await?;
    let buffer: js_sys::ArrayBuffer = buffer.dyn_into()?;
    Ok(js_sys::Uint8Array::new(&buffer).to_vec())
}

#[cfg(target_arch = "wasm32")]
impl eframe::App for WithFonts {
    fn ui(&mut self, ui: &mut eframe::egui::Ui, frame: &mut eframe::Frame) {
        self.start_fetch(ui.ctx());
        self.report_failures();
        self.inner.ui(ui, frame);
    }
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        self.inner.save(storage);
=======
/// Runs the inner app and drains the online fetch queue each frame.
#[cfg(target_arch = "wasm32")]
struct WebShell {
    inner: pcbcraft_ui_egui::App,
    transport: std::sync::Arc<pcbcraft_web::transport::BrowserTransport>,
}

#[cfg(target_arch = "wasm32")]
impl eframe::App for WebShell {
    fn update(&mut self, ctx: &eframe::egui::Context, frame: &mut eframe::Frame) {
        self.transport.poll();
        self.inner.update(ctx, frame);
>>>>>>> theirs
    }
}

/// Show the "needs WebGPU" notice that `index.html` carries, hide the canvas.
=======
/// Show the "PCBCraft stopped" notice with its reload button, hide the canvas.
>>>>>>> theirs
///
/// Called when the app fails to start after load and from the panic hook, so a
/// crash in any frame leaves a readable page instead of a blank canvas. The
/// text is the page's own `#web-crashed` block, kept equal to catalog key
/// `web.crashed` by a test.
///
/// # Rules
/// - Best effort: a missing element is ignored, the console already has the error.
///
/// # Dependencies
/// - `apps/pcbcraft-web/index.html` (`#web-crashed`, `#pcbcraft`)
///
/// # Do not
/// - Panic or return an error; this runs on the failure path and inside the panic hook.
#[cfg(target_arch = "wasm32")]
fn show_crash_notice() {
    let Some(document) = web_sys::window().and_then(|window| window.document()) else {
        return;
    };
    if let Some(notice) = document.get_element_by_id("web-crashed") {
        let _ = notice.remove_attribute("hidden");
    }
    if let Some(canvas) = document.get_element_by_id("pcbcraft") {
        let _ = canvas.set_attribute("hidden", "");
    }
}

/// The session the demo opens with: the built-in sample board.
///
/// The desktop app's `--sample` flag builds the same project, so checks and
/// tabs behave identically. If the sample cannot be built the demo starts
/// empty rather than failing.
///
/// # Rules
/// - Never panic; fall back to an empty session.
///
/// # Dependencies
/// - `crates/engine/src/lib.rs` (`Session::sample`)
///
/// # Do not
/// - Read files; the browser has none.
#[cfg(any(target_arch = "wasm32", test))]
fn demo_session() -> pcbcraft_engine::Session {
    pcbcraft_engine::Session::sample().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    /// The demo opens on a board with parts and nets.
    #[test]
    fn the_demo_starts_with_a_sample() {
        let session = super::demo_session();
        assert!(!session.project.components.is_empty(), "no parts");
        assert!(!session.project.nets.is_empty(), "no nets");
    }

    /// The paragraph in index.html says what the catalog says.
    #[test]
    fn the_page_message_matches_the_catalog() {
        let html = include_str!("../index.html");
        let catalog = pcbcraft_text::Catalog::english();
        for key in [
            "web.needs_webgpu",
            "web.crashed",
            "web.reload",
            "web.reload.tip",
            "web.download_desktop",
            "web.download_desktop.tip",
        ] {
            assert!(catalog.defines(key), "{key} missing from the catalog");
            assert!(
                html.contains(&catalog.get(key)),
                "index.html does not carry the catalog text of {key}"
            );
        }
        let script = include_str!("../webgpu-check.js");
        assert!(script.contains("needs-webgpu"));
        assert!(script.contains("web-crashed"));
        assert!(html.contains("id=\"web-reload\""));
    }

    /// The trunk config gates the wasm on the page check and drops the preloads
    /// that would download it regardless.
    #[test]
    fn the_loader_waits_for_the_graphics_check() {
        let config = include_str!("../Trunk.toml");
        assert!(config.contains("pattern_preload = \"\""));
        assert!(config.contains("window.pcbcraftGraphics"));
    }
}
