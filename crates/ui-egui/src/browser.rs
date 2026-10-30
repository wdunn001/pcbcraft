//! What the browser build cannot do, and how it says so.
//!
//! The wasm build keeps the design in memory only: there is no disk to open
//! from or save to. Instead of failing inside the engine, those commands are
//! stopped here and the status strip says they need the desktop app.
//!
//! Local rules: the wording lives in the text catalog (`web.*` keys in
//! `../../text/strings/en.txt`). The flag is `App::browser`, true on wasm32
//! builds and settable in tests.

/// True for the engine tool names that read or write files on disk.
///
/// # Rules
/// - Only names that touch the disk belong here; edits stay available.
///
/// # Dependencies
/// - `crates/mcp/src/lib.rs` tool names (`open`, `save`, `export`)
///
/// # Do not
/// - Add a tool that only changes the in-memory design.
pub(crate) fn needs_desktop(tool: &str) -> bool {
    matches!(tool, "open" | "save" | "export")
}

/// The status line shown when a disk action is refused in the browser.
pub(crate) fn refusal() -> String {
    pcbcraft_text::Catalog::english().get("web.needs_desktop")
}

/// The status line shown when the browser build starts.
pub(crate) fn start_message() -> String {
    pcbcraft_text::Catalog::english().get("web.limits")
}
