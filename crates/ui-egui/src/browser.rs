//! What the browser build cannot do, and how it says so.
//!
//! The wasm build keeps the design in memory and in browser storage. Disk
//! open and export are refused here; save downloads a copy. Wording lives in
//! the text catalog (`web.*` keys in `../../text/strings/en.txt`). The flag is
//! `App::browser`, true on wasm32 builds and settable in tests.

use pcbcraft_doc::Project;
use pcbcraft_engine::Session;
use pcbcraft_mcp::SharedSession;

/// True for the engine tool names that read or write files on disk (not save in
/// the browser build, which downloads instead).
///
/// # Rules
/// - Only names that touch the disk belong here; in-memory edits stay available.
///
/// # Dependencies
/// - `crates/mcp/src/lib.rs` tool names (`open`, `save`, `export`)
///
/// # Do not
/// - Add a tool that only changes the in-memory design.
pub(crate) fn needs_desktop(tool: &str) -> bool {
    matches!(tool, "open" | "export")
}

/// The status line shown when a disk action is refused in the browser.
pub(crate) fn refusal() -> String {
    pcbcraft_text::Catalog::english().get("web.needs_desktop")
}

/// The status line shown when the browser build starts.
pub(crate) fn start_message() -> String {
    pcbcraft_text::Catalog::english().get("web.limits")
}

/// Status after a successful browser download from File > Save.
pub(crate) fn save_downloaded(path: &str) -> String {
    pcbcraft_text::Catalog::english().format("web.save_downloaded", &[("path", path)])
}

/// Short note for panels that only work with a local disk.
pub(crate) fn desktop_only_panel() -> String {
    pcbcraft_text::Catalog::english().get("web.desktop_only_panel")
}

/// Replace the session project when browser storage holds a saved recovery
/// snapshot and the session has not been edited yet (`revision == 0`).
///
/// The web demo starts from the sample board, which has parts, so an empty
/// session check would skip recovery on every reload.
pub(crate) fn apply_recovery(storage: Option<&dyn eframe::Storage>, session: &SharedSession) {
    let Some(text) = storage.and_then(|storage| storage.get_string("recovery")) else {
        return;
    };
    if text.len() > pcbcraft_io_limit() {
        return;
    }
    let Ok(project) = serde_json::from_str::<Project>(&text) else {
        return;
    };
    if project.validate().is_err() {
        return;
    }
    let Ok(mut current) = session.lock() else {
        return;
    };
    if current.revision != 0 {
        return;
    }
    let _ = current.replace(project);
}

/// Download the current project as a `.pcbcraft` file in the browser.
#[cfg(target_arch = "wasm32")]
pub(crate) fn download_project(project: &Project, filename: &str) -> Result<(), String> {
    use wasm_bindgen::JsCast;

    project.validate()?;
    let bytes = serde_json::to_vec_pretty(project).map_err(|e| e.to_string())?;
    if bytes.len() > pcbcraft_io_limit() {
        return Err("project exceeds 8 MiB".into());
    }
    let window = web_sys::window().ok_or_else(|| "browser window unavailable".to_string())?;
    let document = window
        .document()
        .ok_or_else(|| "browser document unavailable".to_string())?;
    let array = js_sys::Uint8Array::from(bytes.as_slice());
    let parts = js_sys::Array::new();
    parts.push(&array);
    let blob = web_sys::Blob::new_with_u8_array_sequence(&parts)
        .map_err(|_| "could not build download".to_string())?;
    let url = web_sys::Url::create_object_url_with_blob(&blob)
        .map_err(|_| "could not prepare download".to_string())?;
    let anchor = document
        .create_element("a")
        .map_err(|_| "could not start download".to_string())?
        .dyn_into::<web_sys::HtmlAnchorElement>()
        .map_err(|_| "could not start download".to_string())?;
    anchor.set_href(&url);
    anchor.set_download(filename);
    anchor.click();
    web_sys::Url::revoke_object_url(&url).map_err(|_| "could not finish download".to_string())?;
    Ok(())
}

/// Native builds and unit tests: saving through the browser flag does not write disk.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn download_project(project: &Project, _filename: &str) -> Result<(), String> {
    project.validate()?;
    Ok(())
}

fn pcbcraft_io_limit() -> usize {
    8 * 1024 * 1024
}

#[cfg(test)]
mod tests {
    use super::*;
    use pcbcraft_mcp::shared;
    use std::collections::BTreeMap;

    struct MapStorage(BTreeMap<String, String>);

    impl eframe::Storage for MapStorage {
        fn get_string(&self, key: &str) -> Option<String> {
            self.0.get(key).cloned()
        }
        fn set_string(&mut self, key: &str, value: String) {
            self.0.insert(key.to_owned(), value);
        }
    }

    #[test]
    fn reload_restores_a_saved_project_over_the_demo_board() {
        let demo = Session::sample().expect("sample board");
        let shared = shared(demo);
        let saved = {
            let mut session = shared.lock().expect("lock");
            session.execute(
                "component.add",
                serde_json::json!({"kind":"resistor","position":{"x":99,"y":88}}),
                None,
            )
            .expect("add part");
            session.project.clone()
        };
        let recovery =
            serde_json::to_string(&saved).expect("serialize recovery");
        let storage = MapStorage(BTreeMap::from([("recovery".into(), recovery)]));
        apply_recovery(Some(&storage), &shared);
        let after = shared.lock().expect("lock");
        assert!(
            after
                .project
                .components
                .iter()
                .any(|c| (c.position.x - 99.0).abs() < 0.01),
            "recovery should replace the demo board"
        );
    }
}
