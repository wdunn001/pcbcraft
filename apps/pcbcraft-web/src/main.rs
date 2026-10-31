//! Browser entry point: starts the egui app on the page's canvas.
//!
//! Local rules: the page text that appears before the wasm loads lives in
//! `index.html` and `webgpu-check.js`; its wording must equal the catalog key
//! `web.needs_webgpu` (test `the_page_message_matches_the_catalog`). The wasm
//! build needs a browser with WebGPU; a failed start shows the same message.
//!
//! The online part source runs through the browser's own fetch
//! (`browser_fetch.rs`), started by `crates/library/src/web_transport.rs`.
//!
//! # Dependencies
//! - `apps/pcbcraft-web/src/browser_fetch.rs` (the browser transport, wasm only)
//! - `crates/text/strings/en.txt` (message text)
//! - `apps/pcbcraft-web/index.html` and `apps/pcbcraft-web/webgpu-check.js`
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
            show_start_failure();
        }
    });
}
#[cfg(target_arch = "wasm32")]
async fn start() -> Result<(), wasm_bindgen::JsValue> {
    use wasm_bindgen::JsCast;
    let document = web_sys::window()
        .and_then(|window| window.document())
        .ok_or_else(|| wasm_bindgen::JsValue::from_str("browser document unavailable"))?;
    let canvas = document
        .get_element_by_id("pcbcraft")
        .ok_or_else(|| wasm_bindgen::JsValue::from_str("missing pcbcraft canvas"))?
        .dyn_into::<web_sys::HtmlCanvasElement>()?;
    let session = pcbcraft_mcp::shared(demo_session());
    let offline = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let transport = browser_fetch::transport(offline.clone());
    eframe::WebRunner::new()
        .start(
            canvas,
            eframe::WebOptions::default(),
            Box::new(move |cc| {
Ok(Box::new(
                    pcbcraft_ui_egui::App::new(cc, session, None, None, false)
                        .with_web_transport(transport, offline)
                ))
            }),
        )
        .await
}
            }),
        )
        .await
}

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
    }
}

/// Show the "needs WebGPU" notice that `index.html` carries, hide the canvas.
///
/// Called when the app fails to start after load, which is what a browser
/// without a usable graphics adapter produces. The text is the page's own
/// paragraph, kept equal to catalog key `web.needs_webgpu` by a test.
///
/// # Rules
/// - Best effort: a missing element is ignored, the console already has the error.
///
/// # Dependencies
/// - `apps/pcbcraft-web/index.html` (`#needs-webgpu`, `#pcbcraft`)
///
/// # Do not
/// - Panic or return an error; this runs on the failure path.
#[cfg(target_arch = "wasm32")]
fn show_start_failure() {
    let Some(document) = web_sys::window().and_then(|window| window.document()) else {
        return;
    };
    if let Some(notice) = document.get_element_by_id("needs-webgpu") {
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
    }
}
