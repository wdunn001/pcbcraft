//! Open, Save and Export through the project host instead of file paths.
//!
//! Context: the browser build has no disk, so File > Open picks a file with the
//! browser picker (or takes one dropped on the window), File > Save downloads the
//! design and an export downloads its files (several files become one zip). The
//! app never asks "am I in a browser": it asks `ProjectHost::takes_bytes`, so the
//! desktop provider keeps the path-based pages and the in-memory provider lets the
//! headless stories prove the same flow.
//!
//! # Rules
//! - Every action goes through `App::host`; nothing here reads or writes a path.
//! - A file that does not parse leaves the open design and its revision untouched.
//!
//! # Dependencies
//! - crates/mcp/src/project_host.rs (`ProjectHost`, `replace_session_from_open`)
//! - crates/io/src/lib.rs (`open_document`, `save_bytes`)
//! - crates/mcp/src/tool_files.rs (`write_tool_files`, reached through `pcbcraft_mcp`)
//! - crates/ui-egui/src/file_dialogs.rs (calls `webfs_export` from `export_output`)
//!
//! # Do not
//! - Branch on `App::browser` here; ask the host.
//! - Add a second open path: the picker, drag and drop all end in `webfs_open_bytes`.
use crate::{App, Tab, file_dialogs::DialogState, t};
use pcbcraft_io::formats::{FormatSpec, Run};
use serde_json::json;

/// How long a requested file picker is waited on before polling stops, in seconds.
const PICK_WAIT_SECONDS: f64 = 120.0;

/// What the app remembers about a picker it asked the host to show.
#[derive(Default)]
pub(crate) struct State {
    /// Set when the user asked for a picker; the next frame starts the wait.
    requested: bool,
    /// Frame time after which the app stops polling for the picked file.
    until: Option<f64>,
}

impl App {
    /// Plug in the provider that picks, saves and exports files.
    ///
    /// Why: the browser shell gives the app its file picker and downloads, and the
    /// stories give it an in-memory provider; without a call the native provider stays.
    ///
    /// # Rules
    /// - Call once, right after `App::new`.
    ///
    /// # Dependencies
    /// - crates/mcp/src/project_host.rs (`ProjectHost`)
    ///
    /// # Do not
    /// - Swap the provider while a picker is open; the pick is lost.
    pub fn with_project_host(mut self, host: Box<dyn pcbcraft_mcp::ProjectHost>) -> Self {
        self.host = host;
        self
    }

    /// Ask the host for a file to open; false when the host leaves open to the Files page.
    ///
    /// # Rules
    /// - The file arrives on a later frame through `webfs_frame`.
    pub(crate) fn webfs_open(&mut self) -> bool {
        if !self.host.takes_bytes() {
            return false;
        }
        self.host.request_open();
        self.webfs.requested = true;
        self.message = t!("webfs.pick_waiting");
        true
    }

    /// Per frame: collect a picked or dropped file from the host and open it.
    pub(crate) fn webfs_frame(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        if std::mem::take(&mut self.webfs.requested) {
            self.webfs.until = Some(now + PICK_WAIT_SECONDS);
        }
        if let Some((name, bytes)) = self.host.pick_open() {
            self.webfs.until = None;
            self.webfs_open_bytes(&name, &bytes);
        }
        if self.webfs.until.is_some_and(|until| now < until) {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        } else {
            self.webfs.until = None;
        }
    }

    /// Open a design from bytes, the one path the picker and drag and drop share.
    pub(crate) fn webfs_open_bytes(&mut self, name: &str, bytes: &[u8]) {
        let kind = match pcbcraft_io::open_document(bytes, name) {
            Ok(kind) => kind,
            Err(e) => {
                self.message = t!("webfs.open_failed", name = name, reason = e);
                return;
            }
        };
        if !matches!(kind, pcbcraft_io::OpenKind::Project(_)) {
            self.message = t!("webfs.not_a_design", name = name);
            return;
        }
        if let Err(e) = pcbcraft_mcp::replace_session_from_open(&self.session, kind) {
            self.message = t!("webfs.open_failed", name = name, reason = e);
            return;
        }
        self.refresh();
        self.select(None);
        self.path = name.to_owned();
        self.saved_revision = Some(self.revision);
        self.assistant_design_opened("open");
        self.needs_fit = true;
        self.files_home.error.clear();
        self.change_tab(Tab::Schematic);
        self.message = t!("webfs.opened", name = name);
    }

    /// Hand the design to the host as a download; false when the host saves through paths.
    pub(crate) fn webfs_save(&mut self) -> bool {
        if !self.host.takes_bytes() {
            return false;
        }
        let name = self.webfs_download_name();
        let bytes = match pcbcraft_io::save_bytes(&self.project) {
            Ok(bytes) => bytes,
            Err(e) => {
                self.message = t!("webfs.save_failed", reason = e);
                return true;
            }
        };
        match self.host.save_download(&name, &bytes) {
            Ok(()) => {
                self.saved_revision = Some(self.revision);
                self.message = t!("status.done.save", file = name);
            }
            Err(e) => self.message = t!("webfs.save_failed", reason = e),
        }
        true
    }

    /// The suggested file name: the open file's stem, or the design name, plus `.pcbcraft`.
    fn webfs_download_name(&self) -> String {
        let stem = std::path::Path::new(&self.path)
            .file_stem()
            .and_then(|s| s.to_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("design");
        format!("{stem}.pcbcraft")
    }

    /// Export the chosen format through the host; true when it was handed over.
    ///
    /// # Rules
    /// - One file downloads as itself, several files download as one zip.
    /// - A format with no byte writer says so and writes nothing.
    pub(crate) fn webfs_export(&mut self, f: &FormatSpec, d: &DialogState) -> bool {
        let stem = std::path::Path::new(&self.path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("design")
            .to_owned();
        let files = if f.id == "bom" {
            let o = crate::bom_export::Options::from_values(&self.project, &d.values, &d.only);
            crate::bom_export::table(&self.project, &o).map(|table| {
                let text = crate::bom_export::render(&table, &o.file_type);
                vec![(crate::bom_export::file_name(&o), text.into_bytes())]
            })
        } else {
            match f.export {
                Some(Run::Engine("gerber.files")) => {
                    pcbcraft_mcp::write_tool_files(&self.session, "gerber.export", &json!({}))
                }
                Some(Run::Tool("export")) => {
                    let ext = f.extensions.first().copied().unwrap_or("png");
                    let picture = f.id == "picture";
                    let mut params = json!({
                        "path": format!("{stem}.{ext}"),
                        "format": if picture { "png" } else { "kicad_pcb" },
                    });
                    if picture {
                        params["view"] = json!(d.values.get("View").map_or("pcb", String::as_str));
                    }
                    pcbcraft_mcp::write_tool_files(&self.session, "export", &params)
                }
                _ => Err(t!("webfs.export_unavailable", format = f.label)),
            }
        };
        let files = match files {
            Ok(files) if !files.is_empty() => files,
            Ok(_) => {
                self.message = t!("webfs.export_unavailable", format = f.label);
                return false;
            }
            Err(e) => {
                self.message = e;
                return false;
            }
        };
        let handed = if let [(name, bytes)] = files.as_slice() {
            self.host.save_download(name, bytes)
        } else {
            self.host.save_zip(&format!("{stem}-{}.zip", f.id), &files)
        };
        match handed {
            Ok(()) => {
                self.message = t!("webfs.exported", format = f.label);
                true
            }
            Err(e) => {
                self.message = e;
                false
            }
        }
    }
}
