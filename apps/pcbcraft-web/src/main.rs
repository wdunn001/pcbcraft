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
    use wasm_bindgen::JsCast;
    let document = web_sys::window()
        .and_then(|window| window.document())
        .ok_or_else(|| wasm_bindgen::JsValue::from_str("browser document unavailable"))?;
    let canvas = document
        .get_element_by_id("pcbcraft")
        .ok_or_else(|| wasm_bindgen::JsValue::from_str("missing pcbcraft canvas"))?
        .dyn_into::<web_sys::HtmlCanvasElement>()?;
    eframe::WebRunner::new()
        .start(
            canvas,
            eframe::WebOptions::default(),
            Box::new(move |cc| {
                let session = pcbcraft_mcp::shared(opening_session(cc.storage));
                Ok(Box::new(pcbcraft_ui_egui::App::new(
                    cc, session, None, None, false,
                )))
            }),
        )
        .await
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

/// Session passed into the UI on first paint: recovery snapshot or the demo board.
///
/// The demo always starts from a sample board with parts. `App::new` only
/// restores browser storage when the session is still empty, so recovery must
/// happen here before the sample session is installed.
///
/// # Rules
/// - Never panic; fall back to the sample board, then to an empty session.
///
/// # Dependencies
/// - `crates/engine/src/lib.rs` (`Session::sample`)
/// - `crates/ui-egui` autosave key `recovery` in `eframe::Storage`
///
/// # Do not
/// - Read files from disk; the browser has none.
#[cfg(any(target_arch = "wasm32", test))]
fn opening_session(storage: Option<&dyn eframe::Storage>) -> pcbcraft_engine::Session {
    if let Some(text) = storage.and_then(|storage| storage.get_string("recovery")) {
        if text.len() <= pcbcraft_io_limit() {
            if let Ok(project) = serde_json::from_str::<pcbcraft_doc::Project>(&text) {
                if project.validate().is_ok() {
                    let mut session = pcbcraft_engine::Session::default();
                    if session.replace(project).is_ok() {
                        return session;
                    }
                }
            }
        }
    }
    demo_session()
}

/// The session the demo opens with when nothing is stored yet.
#[cfg(any(target_arch = "wasm32", test))]
fn demo_session() -> pcbcraft_engine::Session {
    pcbcraft_engine::Session::sample().unwrap_or_default()
}

fn pcbcraft_io_limit() -> usize {
    8 * 1024 * 1024
}

#[cfg(test)]
mod tests {
    use super::*;
    use pcbcraft_engine::Session;
    use std::collections::BTreeMap;

    struct MapStorage(BTreeMap<String, String>);

    impl eframe::Storage for MapStorage {
        fn get_string(&self, key: &str) -> Option<String> {
            self.0.get(key).cloned()
        }
        fn set_string(&mut self, key: &str, value: String) {
            self.0.insert(key.to_owned(), value);
        }
        fn remove_string(&mut self, key: &str) {
            self.0.remove(key);
        }
        fn flush(&mut self) {}
    }

    /// The demo opens on a board with parts and nets.
    #[test]
    fn the_demo_starts_with_a_sample() {
        let session = super::demo_session();
        assert!(!session.project.components.is_empty(), "no parts");
        assert!(!session.project.nets.is_empty(), "no nets");
    }

    #[test]
    fn reload_restores_a_saved_project_over_the_demo_board() {
        let demo = Session::sample().unwrap_or_default();
        let shared = pcbcraft_mcp::shared(demo);
        let Ok(mut session) = shared.lock() else {
            assert!(false, "lock session");
            return;
        };
        assert!(
            session
                .execute(
                    "component.add",
                    serde_json::json!({"kind":"resistor","position":{"x":99,"y":88}}),
                    None,
                )
                .is_ok(),
            "add part"
        );
        let saved = session.project.clone();
        drop(session);
        let Ok(recovery) = serde_json::to_string(&saved) else {
            assert!(false, "serialize recovery");
            return;
        };
        let storage = MapStorage(BTreeMap::from([("recovery".into(), recovery)]));
        let opened = opening_session(Some(&storage));
        assert!(
            opened
                .project
                .components
                .iter()
                .any(|c| (c.position.x - 99.0).abs() < 0.01),
            "recovery should replace the demo board"
        );
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
