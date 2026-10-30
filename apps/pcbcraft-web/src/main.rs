//! Browser entry point: starts the egui app on the page's canvas.
//!
//! Local rules: the page text that appears before the wasm loads lives in
//! `index.html` and `webgpu-check.js`; its wording must equal the catalog key
//! `web.needs_webgpu` (test `the_page_message_matches_the_catalog`). The wasm
//! build needs a browser with WebGPU; a failed start shows the same message.
//!
//! # Dependencies
//! - `crates/text/strings/en.txt` (message text)
//! - `apps/pcbcraft-web/index.html` and `apps/pcbcraft-web/webgpu-check.js`
//! - `apps/pcbcraft-web/src/transport.rs` (online library fetch queue)
//!
//! # Do not
//! - Add text literals here; read them from the catalog.

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
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;

    use pcbcraft_web::transport::{
        cross_origin_status, BrowserTransport, CrossOriginStatus, PollingFetcher,
    };
    use wasm_bindgen::JsCast;

    let document = web_sys::window()
        .and_then(|window| window.document())
        .ok_or_else(|| wasm_bindgen::JsValue::from_str("browser document unavailable"))?;
    let canvas = document
        .get_element_by_id("pcbcraft")
        .ok_or_else(|| wasm_bindgen::JsValue::from_str("missing pcbcraft canvas"))?
        .dyn_into::<web_sys::HtmlCanvasElement>()?;
    let session = pcbcraft_mcp::shared(demo_session());
    let transport = Arc::new(BrowserTransport::live());
    let offline = Arc::new(AtomicBool::new(false));
    eframe::WebRunner::new()
        .start(
            canvas,
            eframe::WebOptions::default(),
            Box::new(move |cc| {
                let mut app =
                    pcbcraft_ui_egui::App::new(cc, session, None, None, false);
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
            }),
        )
        .await
}

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
