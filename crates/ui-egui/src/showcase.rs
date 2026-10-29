//! Start-page support for the showcase sample board.
//!
//! The finished board ships inside the program, so the Start page can open it
//! from one click on any machine. It is written to the temporary folder and
//! opened like any other design file, which keeps undo, recents and the
//! assistant notice working exactly as for a file the person opened.
//!
//! # Rules
//! - The text is the generated file in `examples/`; never edit a copy.
//!
//! # Dependencies
//! - examples/showcase.pcbcraft (produced by `cargo xtask showcase`)
//! - crates/ui-egui/src/lib.rs (`App::tool_call`)
//!
//! # Do not
//! - Parse or change the design here; the engine's `open` command does that.
use super::*;

/// The generated showcase design, embedded at build time.
const SHOWCASE: &str = include_str!("../../../examples/showcase.pcbcraft");

/// A temporary file name no other window or test shares: the process, a
/// counter and the clock keep two opens apart.
fn unique_showcase_path() -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    std::env::temp_dir().join(format!(
        "pcbcraft-showcase-sensor-node-{}-{}-{stamp}.pcbcraft",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}

impl App {
    /// Open the showcase board as the current design.
    ///
    /// Returns true when the design opened. On failure the status message
    /// already says why, so the caller only decides whether to leave the
    /// Start page.
    ///
    /// # Rules
    /// - The design replaces the current one through the engine's `open`
    ///   command, so the change is undoable like any other open.
    ///
    /// # Dependencies
    /// - examples/showcase.pcbcraft
    ///
    /// # Do not
    /// - Keep the temporary file open; it is only read once.
    /// - Reuse a fixed file name; two opens at once would overwrite each other.
    pub(crate) fn open_showcase(&mut self) -> bool {
        let path = unique_showcase_path();
        if let Err(error) = std::fs::write(&path, SHOWCASE) {
            self.message = error.to_string();
            return false;
        }
        let Some(path) = path.to_str() else {
            return false;
        };
        self.tool_call("open", json!({ "path": path }))
    }
}
