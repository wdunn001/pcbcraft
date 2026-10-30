//! Browser entry point: starts the egui app on the page's canvas.
//!
//! Local rules: the page text that appears around the wasm lives in
//! `index.html` and `webgpu-check.js`; its wording must equal the catalog keys
//! `web.needs_webgpu`, `web.crashed`, `web.reload` and `web.reload.tip` (test
//! `the_page_message_matches_the_catalog`). The page script decides before the
//! wasm downloads whether the browser has WebGPU or WebGL 2; a failed start or
//! a panic after load shows the crash notice with a reload button.
//!
//! # Dependencies
//! - `crates/text/strings/en.txt` (message text)
//! - `apps/pcbcraft-web/index.html`
//! - `apps/pcbcraft-web/webgpu-check.js`
//! - `apps/pcbcraft-web/Trunk.toml` (the loader that waits for the check)
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
            show_crash_notice();
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
    let runner = eframe::WebRunner::new();
    // eframe installs its own panic hook when the runner is made; chain after
    // it so a panic in any frame also puts the notice on the page.
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        previous(info);
        show_crash_notice();
    }));
    runner
        .start(
            canvas,
            eframe::WebOptions::default(),
            Box::new(move |cc| {
                Ok(Box::new(pcbcraft_ui_egui::App::new(
                    cc, session, None, None, false,
                )))
            }),
        )
        .await
}

/// Show the "PCBCraft stopped" notice with its reload button, hide the canvas.
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
