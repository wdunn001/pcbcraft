//! Load and Save library on the Collections page: pick a file, run the command, say what happened.
//!
//! Context: owner bug "Load library does not work on Collections". The buttons used to run
//! `library.load` on a typed path that started as a file name which does not exist, and the
//! answer went to the footer, so nothing visible happened. Now each button opens the chooser
//! (desktop) or the browser file picker and download (browser build), and the engine's answer
//! shows on the page in the notice line under the buttons.
//! Local rules: the engine command does the work; this file only picks, calls and words the
//! result. A failed load changes nothing in the project.
//!
//! # Dependencies
//! - crates/ui-egui/src/path_chooser.rs (desktop choice)
//! - crates/ui-egui/src/webfs.rs (browser picker and download through the project host)
//! - crates/engine/src/authoring.rs (`library.load` takes a path or the file text)
//!
//! # Do not
//! - Read or write a file here; the engine and the host do.
//! - Branch on `App::browser`; ask the host with `takes_bytes`.
use crate::path_chooser::{LIBRARY_EXTENSION, Pick, Purpose};
use crate::{App, t};
use serde_json::{Value, json};

impl App {
    /// The Load library button: choose a collection file, or pick one in the browser.
    pub(crate) fn library_load_start(&mut self, _ctx: &egui::Context) {
        self.library.editor.notice = None;
        if self.webfs_pick_library() {
            self.library_say(t!("library.load.waiting"));
        } else {
            self.open_chooser(Purpose::LoadLibrary);
        }
    }

    /// The Save library button: choose where to write, or download in the browser.
    pub(crate) fn library_save_start(&mut self) {
        self.library.editor.notice = None;
        if !self.host.takes_bytes() {
            self.open_chooser(Purpose::SaveLibrary);
            return;
        }
        let name = format!("library.{LIBRARY_EXTENSION}");
        let files =
            pcbcraft_mcp::write_tool_files(&self.session, "library.save", &json!({"path": name}));
        let line = match files {
            Ok(files) => match files.first() {
                Some((file, bytes)) => match self.host.save_download(file, bytes) {
                    Ok(()) => t!("library.save.downloaded", name = file),
                    Err(e) => t!("library.save.failed", reason = e),
                },
                None => t!("library.save.failed", reason = t!("library.save.nothing")),
            },
            Err(e) => t!("library.save.failed", reason = e),
        };
        self.library_say(line);
    }

    /// Take a finished choice from the chooser window and run what it was for.
    pub(crate) fn chooser_done(&mut self, ctx: &egui::Context, pick: Pick) {
        match pick.purpose {
            Purpose::LoadLibrary => {
                let name = file_label(&pick.path);
                self.library_load_with(json!({"path": pick.path}), &name);
            }
            Purpose::SaveLibrary => {
                let ok = self.tool_call("library.save", json!({"path": pick.path}));
                let line = if ok {
                    t!("library.save.done", path = pick.path)
                } else {
                    t!("library.save.failed", reason = self.message.clone())
                };
                self.library_say(line);
            }
            Purpose::AddSource => {
                self.library
                    .request(ctx, "library.add_folder", json!({"path": pick.path}));
            }
            Purpose::PreviousPackage => {
                self.edit.dialog.previous_package = pick.path;
            }
        }
    }

    /// A collection file the browser picked: its bytes are the file text.
    pub(crate) fn library_load_text(&mut self, name: &str, bytes: &[u8]) {
        match String::from_utf8(bytes.to_vec()) {
            Ok(text) => self.library_load_with(json!({"text": text}), name),
            Err(_) => {
                let line = t!(
                    "library.load.failed",
                    name = name,
                    reason = t!("library.load.not_text")
                );
                self.library_say(line);
            }
        }
    }

    /// Show one line under the buttons on Collections and under the Sources card.
    fn library_say(&mut self, line: String) {
        self.library.error.clear();
        self.library.notice = line.clone();
        self.library.editor.notice = Some(line);
    }

    /// Run `library.load` and put its answer in the notice line.
    fn library_load_with(&mut self, args: Value, name: &str) {
        let result = self
            .session
            .lock()
            .map_err(|_| t!("dialogs.run.session_unavailable"))
            .and_then(|mut s| {
                s.execute("library.load", args, Some(self.revision))
                    .map_err(|e| e.to_string())
            });
        self.refresh();
        let line = match result {
            Ok(v) => t!(
                "library.load.done",
                name = name,
                symbols = v
                    .pointer("/result/symbols")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
                footprints = v
                    .pointer("/result/footprints")
                    .and_then(Value::as_u64)
                    .unwrap_or(0)
            ),
            Err(e) => t!("library.load.failed", name = name, reason = e),
        };
        self.library_say(line);
    }
}

/// The last part of a path, for the notice line.
fn file_label(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(path)
        .to_owned()
}
