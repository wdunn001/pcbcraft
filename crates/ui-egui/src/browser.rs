//! What the browser build cannot do, and how it says so.
//!
//! The wasm build has no disk to open from or save to. The design lives in memory and a
//! copy of it is written to page storage (`store_recovery` in `lib.rs`), which the web
//! entry point reads back after a reload. Instead of failing inside the engine, the
//! disk commands are stopped here and the status strip says they need the desktop app.
//! Open and Save are not refused: they go through the project host (`webfs.rs`).
//!
//! Local rules: the wording lives in the text catalog (`web.*` keys in
//! `../../text/strings/en.txt`). The flag is `App::browser`, true on wasm32
//! builds and settable in tests.

/// True for the engine tool names that read or write files on disk.
///
/// # Rules
/// - Only names that touch the disk belong here; edits stay available.
/// - Open and Save are not listed: the browser reaches them through the project host
///   (`webfs.rs`), never through a path.
///
/// # Dependencies
/// - `crates/mcp/src/lib.rs` tool names (`export`)
///
/// # Do not
/// - Add a tool that only changes the in-memory design.
pub(crate) fn needs_desktop(tool: &str) -> bool {
    matches!(tool, "export")
}

/// The status line shown when a disk action is refused in the browser.
pub(crate) fn refusal() -> String {
    pcbcraft_text::Catalog::english().get("web.needs_desktop")
}

/// The status line shown when the browser build starts.
pub(crate) fn start_message() -> String {
    pcbcraft_text::Catalog::english().get("web.limits")
}
