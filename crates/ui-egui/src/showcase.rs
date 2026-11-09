//! Start-page support for the showcase sample board.
//!
//! The finished board ships inside the program, so the Start page can open it
//! from one click on any machine. It opens from its embedded text through the
//! session, with no file, so the browser build can open it too. Undo and the
//! assistant notice work as for a file the person opened.
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
    /// - Write a file or ask for a temporary folder; the browser build has neither.
    pub(crate) fn open_showcase(&mut self) -> bool {
        // The design opens from its embedded text: the browser build has no file system, and
        // asking it for a temporary folder stops the whole page.
        let project = match pcbcraft_io::load_text_for_open(SHOWCASE) {
            Ok(project) => project,
            Err(error) => {
                self.message = error;
                return false;
            }
        };
        let opened = match self.session.lock() {
            Ok(mut session) => session.open_project(project).map_err(|e| e.to_string()),
            Err(_) => Err("session unavailable after internal failure".to_string()),
        };
        if let Err(error) = opened {
            self.message = error;
            return false;
        }
        self.message = t!("status.done.sample_design");
        self.path = "showcase.pcbcraft".to_string();
        self.refresh();
        self.saved_revision = Some(self.revision);
        self.assistant_design_opened("open");
        self.needs_fit = true;
        true
    }
}
