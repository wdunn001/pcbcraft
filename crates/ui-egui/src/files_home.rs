//! The Files workspace: a command list on the left, the chosen command's page in the
//! centre, and the details of a recent project on the right.
//!
//! Context: the Files tab is a workspace (`workspace.rs`, the Workspace contract), so it
//! defines its own left aside instead of inheriting Layers and Library. The left aside
//! lists New project, Open, Save, Save as, Import, Export, Project settings, Preferences
//! and Close project. A command that needs room opens a page in the centre (Recent
//! projects is the default, then Open, Import, Export, Project settings, Close); the rest
//! act at once. The right aside shows the project under the pointer or selected in the
//! list, or the current project when no recent one is picked.
//!
//! Local rules:
//! - Import and Export pages list the same registry entries as the File menu (Recent,
//!   Common, More) and open the same dialog (`file_dialogs.rs`); no format is named here.
//! - Recent projects come from `pcbcraft_engine::recents`; pins and last-opened times
//!   live in `Preferences`. Pictures and file facts are read on a worker thread and never
//!   on the window thread.
//! - Every string comes from the catalog (`crates/text/strings/en.txt`, keys `files.*`).
//!
//! # Dependencies
//! - crates/ui-egui/src/workspace.rs (`FilesWorkspace` calls `left_body`, `center`)
//! - crates/ui-egui/src/file_dialogs.rs (`App::open_dialog`, `Recents`)
//! - crates/engine/src/recents.rs (the list of recent paths)
//! - crates/io/src/raster.rs (the board picture)
//! - crates/ui-egui/src/ui_kit/rows.rs, button.rs, empty_state.rs
//!
//! # Do not
//! - Run a dialog's work here; open the dialog.
//! - Read a file or its metadata on the window thread.
//! - Touch the real recents file in a test; set `RECENTS_PATH_TEST` to a temp dir.

use super::*;
use crate::ui_kit::button::{self, Size};
use crate::ui_kit::icons::{self, Icon};
use pcbcraft_io::formats::{self, Direction, Scope};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Pictures are this many points across at their largest.
const THUMB: [usize; 2] = [168, 112];
/// Pinned projects kept.
const MAX_PINNED: usize = 12;
/// Entries a folder listing shows.
const MAX_ENTRIES: usize = 400;
/// Height of one recent project row.
const ROW_HEIGHT: f32 = 76.0;
/// Width of the picture in a row.
const ROW_PICTURE: f32 = 84.0;

/// The page in the centre.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Page {
    /// Recent projects, the default.
    #[default]
    Recent,
    /// The folder browser.
    Open,
    /// Formats to bring in.
    Import,
    /// Formats to make.
    Export,
    /// Where the project's rules and setup live.
    ProjectSettings,
    /// Asks before closing a project with changes.
    Close,
}

/// A left aside command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Command {
    Recent,
    New,
    Open,
    Save,
    SaveAs,
    Import,
    Export,
    ProjectSettings,
    Preferences,
    Close,
}

/// What the worker found out about one file.
struct Found {
    path: String,
    exists: bool,
    modified: Option<u64>,
    bytes: u64,
    image: Option<egui::ColorImage>,
}

/// What is known about one recent project's file.
struct Probe {
    exists: bool,
    modified: Option<u64>,
    bytes: u64,
    picture: Option<egui::TextureHandle>,
}

/// One line of a folder listing.
#[derive(Clone, Debug)]
struct Entry {
    name: String,
    folder: bool,
}

/// The Open page's folder browser.
#[derive(Default)]
struct Picker {
    dir: PathBuf,
    entries: Vec<Entry>,
    loaded: bool,
    selected: Option<usize>,
    /// The typed path, which is also where Enter goes.
    typed: String,
    error: String,
}

/// What the Files workspace remembers between frames.
#[derive(Default)]
pub(crate) struct State {
    pub(crate) page: Page,
    /// The recent project picked in the list, by path.
    selected: Option<String>,
    /// The recent project under the pointer last frame.
    hovered: Option<String>,
    probes: HashMap<String, Probe>,
    asked: HashSet<String>,
    #[cfg(not(target_arch = "wasm32"))]
    found: Option<std::sync::mpsc::Receiver<Found>>,
    picker: Picker,
    /// Scroll the selected row into view once.
    reveal: bool,
    /// Ids of the list rows drawn last frame; focus on one of them still lets the
    /// arrow keys and Enter work the list.
    rows: Vec<egui::Id>,
    /// What the last Files action said when it failed.
    pub(crate) error: String,
}

impl State {
    /// Forget what was read about `path` so the next frame reads it again (after a
    /// save, which changes the file and its picture).
    pub(crate) fn forget_probe(&mut self, path: &str) {
        self.probes.remove(path);
        self.asked.remove(path);
    }

    /// The picked recent project (tests read it).
    #[cfg(test)]
    pub(crate) fn selected(&self) -> Option<&str> {
        self.selected.as_deref()
    }

    /// Type a path into the Open page (tests only).
    #[cfg(test)]
    pub(crate) fn set_typed_for_test(&mut self, path: &str) {
        self.picker.typed = path.to_owned();
        self.picker.loaded = true;
    }

    /// True once every listed project has been looked at (screenshots wait for it).
    pub(crate) fn settled(&self, paths: &[String]) -> bool {
        paths.iter().all(|p| self.probes.contains_key(p))
    }
}

/// Seconds since 1970, or 0 where the platform has no clock.
fn now_secs() -> u64 {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }
    #[cfg(target_arch = "wasm32")]
    {
        0
    }
}

/// A picture of the board in a file: top copper over the board colour.
///
/// Why: a recent project is recognised by its shape faster than by its name.
///
/// # Rules
/// - Reads and rasterises on the calling thread, so call it from a worker.
/// - Returns `None` for a file that does not load or has no drawable board.
///
/// # Dependencies
/// - crates/io/src/raster.rs (`layer_raster`)
///
/// # Do not
/// - Call it from the window thread.
pub(crate) fn board_picture(path: &Path) -> Option<egui::ColorImage> {
    use pcbcraft_io::{FabLayer, RasterDepth, RasterOptions, RasterPixels};
    let project = pcbcraft_io::load(path).ok()?;
    let options = RasterOptions {
        layer: FabLayer::CopperTop,
        dpi: 300,
        mirror: false,
        invert: false,
        outline: true,
        registration: None,
        drill_marks: None,
        margin_mm: 1.0,
        depth: RasterDepth::Grey8,
    };
    let raster = pcbcraft_io::layer_raster(&project, &options).ok()?;
    let RasterPixels::Grey(grey) = raster.pixels else {
        return None;
    };
    let (w, h) = (grey.width() as usize, grey.height() as usize);
    if w == 0 || h == 0 {
        return None;
    }
    let scale = (THUMB[0] as f32 / w as f32).min(THUMB[1] as f32 / h as f32);
    let (tw, th) = (
        ((w as f32 * scale).round() as usize).clamp(1, THUMB[0]),
        ((h as f32 * scale).round() as usize).clamp(1, THUMB[1]),
    );
    let board = egui::Color32::from_rgb(0x0E, 0x3B, 0x2D);
    let copper = egui::Color32::from_rgb(0xE8, 0x83, 0x4A);
    let mut pixels = Vec::with_capacity(tw * th);
    for ty in 0..th {
        let y0 = ty * h / th;
        let y1 = ((ty + 1) * h / th).max(y0 + 1).min(h);
        for tx in 0..tw {
            let x0 = tx * w / tw;
            let x1 = ((tx + 1) * w / tw).max(x0 + 1).min(w);
            let mut dark = 0u32;
            let mut count = 0u32;
            for y in y0..y1 {
                for x in x0..x1 {
                    dark += 255 - u32::from(grey.get_pixel(x as u32, y as u32).0[0]);
                    count += 1;
                }
            }
            let cover = (dark as f32 / (count.max(1) as f32 * 255.0) * 1.6).min(1.0);
            pixels.push(board.lerp_to_gamma(copper, cover));
        }
    }
    Some(egui::ColorImage::new([tw, th], pixels))
}

/// The file name of a project without its extension.
fn project_name(path: &str) -> String {
    let name = Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_owned());
    for suffix in [".pcbcraft", ".board.json", ".json"] {
        if let Some(stem) = name.strip_suffix(suffix)
            && !stem.is_empty()
        {
            return stem.to_owned();
        }
    }
    name
}

/// The folder of a path with the home folder shortened to `~`, for display only.
fn short_folder(path: &str) -> String {
    let folder = Path::new(path)
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default();
    let shown = if !home.is_empty()
        && let Some(rest) = folder.strip_prefix(&home)
        && (rest.is_empty() || rest.starts_with(['/', '\\']))
    {
        format!("~{rest}")
    } else {
        folder
    };
    let parts: Vec<&str> = shown.split(['/', '\\']).filter(|p| !p.is_empty()).collect();
    if parts.len() > 4 {
        let lead = if shown.starts_with('~') { "~" } else { "" };
        format!("{lead}/…/{}", parts[parts.len() - 3..].join("/"))
    } else {
        shown
    }
}

impl App {
    /// The projects the list shows: pinned first, then the rest by recency.
    ///
    /// # Rules
    /// - A pinned project stays listed even after it falls off the recent list.
    pub(crate) fn files_listed(&self) -> Vec<String> {
        let pinned = &self.preferences.pinned_projects;
        let mut out: Vec<String> = pinned.clone();
        for path in &self.recent_designs {
            if !pinned.contains(path) {
                out.push(path.clone());
            }
        }
        out
    }

    /// True when the project has changes that are not on disk.
    pub(crate) fn project_has_unsaved_changes(&self) -> bool {
        !self.project.components.is_empty() && self.saved_revision != Some(self.revision)
    }

    /// True when the project was never written to the path it would save to.
    fn project_never_saved(&self) -> bool {
        self.saved_revision.is_none() && !Path::new(&self.path).exists()
    }

    /// Remember that a project was opened or saved now.
    pub(crate) fn files_note_opened(&mut self, path: &str) {
        self.preferences
            .project_opened
            .insert(path.to_owned(), now_secs());
        while self.preferences.project_opened.len() > 64 {
            let Some(oldest) = self
                .preferences
                .project_opened
                .iter()
                .min_by_key(|(_, t)| **t)
                .map(|(p, _)| p.clone())
            else {
                break;
            };
            self.preferences.project_opened.remove(&oldest);
        }
        self.files_home.forget_probe(path);
        self.files_home.error.clear();
    }

    /// Go to the Files tab on `page`.
    pub(crate) fn show_files(&mut self, page: Page) {
        if self.tab != Tab::Files {
            self.change_tab(Tab::Files);
        }
        self.files_home.page = page;
        self.files_home.error.clear();
        if page == Page::Open {
            self.files_home.picker.loaded = false;
        }
    }

    /// Save to the current path, or ask where when the project has no file yet.
    ///
    /// Why: Ctrl+S, the File menu and the Files page all mean the same thing.
    ///
    /// # Do not
    /// - Write a file named `design.pcbcraft` in the working folder without asking.
    pub(crate) fn save_or_ask(&mut self) {
        if self.project_never_saved() {
            self.open_dialog(file_dialogs::Mode::SaveAs, None, None, vec![]);
        } else {
            let path = self.path.clone();
            self.tool_call("save", json!({"path": path}));
        }
    }

    /// Keys that work on every tab: Open, Save, Save as.
    ///
    /// # Rules
    /// - Runs before the text-field check, so a focused field does not swallow them.
    pub(crate) fn file_shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.current_pass_index() > 0 || ui_kit::menu::any_open(ctx) {
            return;
        }
        let save_as = egui::KeyboardShortcut::new(
            egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
            egui::Key::S,
        );
        let save = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::S);
        let open = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::O);
        if ctx.input_mut(|i| i.consume_shortcut(&save_as)) {
            self.open_dialog(file_dialogs::Mode::SaveAs, None, None, vec![]);
        } else if ctx.input_mut(|i| i.consume_shortcut(&save)) {
            self.save_or_ask();
        } else if ctx.input_mut(|i| i.consume_shortcut(&open)) {
            self.show_files(Page::Open);
        }
    }

    /// Open a project file and, when it opens, go to the schematic.
    fn files_open_path(&mut self, path: &str) -> bool {
        let path = path.trim().to_owned();
        if path.is_empty() {
            self.files_home.error = t!("files.open.nothing_typed");
            return false;
        }
        self.path = path.clone();
        if self.tool_call("open", json!({"path": path})) {
            self.select(None);
            self.files_home.error.clear();
            self.change_tab(Tab::Schematic);
            true
        } else {
            self.files_home.error = self.message.clone();
            false
        }
    }

    /// Close the project: a blank one takes its place.
    fn files_close_now(&mut self) {
        self.tool_call("new_design", json!({}));
        self.select(None);
        self.path = "design.pcbcraft".into();
        self.saved_revision = None;
        self.files_home.page = Page::Recent;
        self.message = t!("files.close.done");
    }

    /// Do a left aside command.
    pub(crate) fn files_run(&mut self, command: Command) {
        match command {
            Command::Recent => self.files_home.page = Page::Recent,
            Command::New => {
                self.tool_call("new_design", json!({}));
                self.select(None);
                self.change_tab(Tab::Schematic);
            }
            Command::Open => self.show_files(Page::Open),
            Command::Save => self.save_or_ask(),
            Command::SaveAs => self.open_dialog(file_dialogs::Mode::SaveAs, None, None, vec![]),
            Command::Import => self.files_home.page = Page::Import,
            Command::Export => self.files_home.page = Page::Export,
            Command::ProjectSettings => self.files_home.page = Page::ProjectSettings,
            Command::Preferences => self.change_tab(Tab::Settings),
            Command::Close => {
                if self.project_has_unsaved_changes() {
                    self.files_home.page = Page::Close;
                } else {
                    self.files_close_now();
                }
            }
        }
    }

    /// The left aside of the Files tab: the command list.
    ///
    /// Why: the Files tab is where a project is begun, opened, saved and sent out, so its
    /// aside lists those commands instead of the drawing tools' Layers and Library.
    ///
    /// # Rules
    /// - A page command highlights while its page shows; an action command never does.
    /// - Close project is dimmed while the project is empty, and its tip says so.
    ///
    /// # Dependencies
    /// - crates/ui-egui/src/ui_kit/rows.rs (`list_row`)
    ///
    /// # Do not
    /// - Add a tab strip or the Library here; the workspace lists one tab only.
    pub(crate) fn files_commands(&mut self, ui: &mut egui::Ui) {
        // Selectable text would take the click that belongs to the row.
        ui.style_mut().interaction.selectable_labels = false;
        let ctx = ui.ctx().clone();
        let tokens = theme::tokens(&ctx);
        let primary = |key: egui::Key, shift: bool| {
            let mods = if shift {
                egui::Modifiers::COMMAND | egui::Modifiers::SHIFT
            } else {
                egui::Modifiers::COMMAND
            };
            ctx.format_shortcut(&egui::KeyboardShortcut::new(mods, key))
        };
        let can_close = !self.project.components.is_empty() || self.project_has_unsaved_changes();
        let commands: [(Command, String, String, String, bool, bool); 10] = [
            (
                Command::Recent,
                t!("files.nav.recent"),
                String::new(),
                t!("files.nav.recent.tip"),
                self.files_home.page == Page::Recent,
                true,
            ),
            (
                Command::New,
                t!("files.nav.new"),
                String::new(),
                t!("files.nav.new.tip"),
                false,
                true,
            ),
            (
                Command::Open,
                t!("files.nav.open"),
                primary(egui::Key::O, false),
                t!("files.nav.open.tip"),
                self.files_home.page == Page::Open,
                true,
            ),
            (
                Command::Save,
                t!("files.nav.save"),
                primary(egui::Key::S, false),
                t!("files.nav.save.tip"),
                false,
                true,
            ),
            (
                Command::SaveAs,
                t!("files.nav.save_as"),
                primary(egui::Key::S, true),
                t!("files.nav.save_as.tip"),
                false,
                true,
            ),
            (
                Command::Import,
                t!("files.nav.import"),
                String::new(),
                t!("files.nav.import.tip"),
                self.files_home.page == Page::Import,
                true,
            ),
            (
                Command::Export,
                t!("files.nav.export"),
                String::new(),
                t!("files.nav.export.tip"),
                self.files_home.page == Page::Export,
                true,
            ),
            (
                Command::ProjectSettings,
                t!("files.nav.project_settings"),
                String::new(),
                t!("files.nav.project_settings.tip"),
                self.files_home.page == Page::ProjectSettings,
                true,
            ),
            (
                Command::Preferences,
                t!("files.nav.preferences"),
                String::new(),
                t!("files.nav.preferences.tip"),
                false,
                true,
            ),
            (
                Command::Close,
                t!("files.nav.close"),
                String::new(),
                if can_close {
                    t!("files.nav.close.tip")
                } else {
                    t!("files.nav.close.empty")
                },
                self.files_home.page == Page::Close,
                can_close,
            ),
        ];
        let mut picked = None;
        egui::ScrollArea::vertical()
            .id_salt("files-commands")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add_space(theme::SPACE[1]);
                for (index, (command, label, key, tip, selected, enabled)) in
                    commands.iter().enumerate()
                {
                    if matches!(index, 1 | 5 | 8) {
                        ui.add_space(theme::SPACE[1]);
                    }
                    let pages = matches!(
                        command,
                        Command::Import | Command::Export | Command::ProjectSettings
                    );
                    let response = ui_kit::rows::list_row(ui, *selected, label, tip, |ui| {
                        let colour = if *enabled { tokens.text } else { tokens.muted };
                        ui.label(fonts::text(fonts::Role::Body, label).color(colour));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if pages {
                                let (rect, _) = ui.allocate_exact_size(
                                    Vec2::splat(icons::ICON_SIZE),
                                    egui::Sense::hover(),
                                );
                                (icons::CHEVRON_RIGHT.0)(ui.painter(), rect, tokens.muted);
                            } else if !key.is_empty() {
                                ui.label(fonts::caption(key).color(tokens.muted));
                            }
                        });
                    });
                    if response.clicked() && *enabled {
                        picked = Some(*command);
                    }
                }
            });
        if let Some(command) = picked {
            self.files_run(command);
        }
    }

    /// Ask the worker thread for what is not known yet about the listed projects.
    fn files_probe(&mut self, ctx: &egui::Context) {
        let listed = self.files_listed();
        #[cfg(not(target_arch = "wasm32"))]
        {
            let mut keys: Vec<String> = Vec::new();
            for path in &listed {
                if !self.files_home.probes.contains_key(path)
                    && !self.files_home.asked.contains(path)
                {
                    keys.push(path.clone());
                }
            }
            if !keys.is_empty() && self.files_home.found.is_none() {
                let (tx, rx) = std::sync::mpsc::channel::<Found>();
                let sent = keys.clone();
                let spawned = std::thread::Builder::new()
                    .name("files-pictures".into())
                    .spawn(move || {
                        for path in keys {
                            let meta = std::fs::metadata(&path).ok();
                            let modified = meta
                                .as_ref()
                                .and_then(|m| m.modified().ok())
                                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                                .map(|d| d.as_secs());
                            let image = meta
                                .as_ref()
                                .filter(|m| m.is_file())
                                .and_then(|_| board_picture(Path::new(&path)));
                            let found = Found {
                                exists: meta.as_ref().is_some_and(|m| m.is_file()),
                                modified,
                                bytes: meta.as_ref().map_or(0, |m| m.len()),
                                image,
                                path,
                            };
                            if tx.send(found).is_err() {
                                return;
                            }
                        }
                    });
                if spawned.is_ok() {
                    self.files_home.asked.extend(sent);
                    self.files_home.found = Some(rx);
                }
            }
            let mut finished = false;
            if let Some(rx) = &self.files_home.found {
                loop {
                    match rx.try_recv() {
                        Ok(found) => {
                            let picture = found.image.map(|image| {
                                ctx.load_texture(
                                    format!("files-picture:{}", found.path),
                                    image,
                                    egui::TextureOptions::LINEAR,
                                )
                            });
                            self.files_home.probes.insert(
                                found.path,
                                Probe {
                                    exists: found.exists,
                                    modified: found.modified,
                                    bytes: found.bytes,
                                    picture,
                                },
                            );
                        }
                        Err(std::sync::mpsc::TryRecvError::Empty) => {
                            ctx.request_repaint_after(std::time::Duration::from_millis(50));
                            break;
                        }
                        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                            finished = true;
                            break;
                        }
                    }
                }
            }
            if finished {
                self.files_home.found = None;
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = ctx;
            for path in listed {
                self.files_home.probes.entry(path).or_insert(Probe {
                    exists: true,
                    modified: None,
                    bytes: 0,
                    picture: None,
                });
            }
        }
    }

    /// "3 hours ago" in the catalog's words; empty when the clock is unknown.
    fn files_ago(&self, then: u64) -> String {
        let now = now_secs();
        if now == 0 || then == 0 {
            return String::new();
        }
        let secs = now.saturating_sub(then);
        let (key, n) = match secs {
            0..=59 => return t!("files.ago.now"),
            60..=3599 => ("files.ago.minutes", secs / 60),
            3600..=86_399 => ("files.ago.hours", secs / 3600),
            _ => ("files.ago.days", secs / 86_400),
        };
        self.catalog.plural(key, n)
    }

    /// The words under a project's name: when it was opened, or that it is gone.
    fn files_when(&self, path: &str) -> String {
        let Some(probe) = self.files_home.probes.get(path) else {
            return String::new();
        };
        if !probe.exists {
            return t!("files.missing");
        }
        if let Some(opened) = self.preferences.project_opened.get(path) {
            return t!("files.opened_ago", when = self.files_ago(*opened));
        }
        match probe.modified {
            Some(m) => t!("files.saved_ago", when = self.files_ago(m)),
            None => String::new(),
        }
    }

    /// The centre of the Files tab.
    ///
    /// Why: one place decides which page shows, so the left aside and the keyboard
    /// agree with it.
    ///
    /// # Rules
    /// - Arrow keys and Enter act on the list only while nothing else holds the focus.
    ///
    /// # Dependencies
    /// - crates/ui-egui/src/workspace.rs (`FilesWorkspace::center`)
    ///
    /// # Do not
    /// - Draw the Library or Layers here.
    pub(crate) fn files_center(&mut self, ui: &mut egui::Ui) {
        ui.style_mut().interaction.selectable_labels = false;
        let ctx = ui.ctx().clone();
        self.files_probe(&ctx);
        self.files_page_keys(&ctx);
        self.files_home.rows.clear();
        let narrow = Self::narrow_window(&ctx);
        egui::ScrollArea::vertical()
            .id_salt("files-page")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                Self::padded(ui, |ui| {
                    ui.set_max_width(ui.available_width().min(860.0));
                    if narrow {
                        self.files_narrow_commands(ui);
                    }
                    let (title, subtitle) = match self.files_home.page {
                        Page::Recent => (t!("files.recent.title"), t!("files.recent.subtitle")),
                        Page::Open => (t!("files.open.title"), t!("files.open.subtitle")),
                        Page::Import => (t!("files.import.title"), t!("files.import.subtitle")),
                        Page::Export => (t!("files.export.title"), t!("files.export.subtitle")),
                        Page::ProjectSettings => {
                            (t!("files.settings.title"), t!("files.settings.subtitle"))
                        }
                        Page::Close => (t!("files.close.title"), t!("files.close.subtitle")),
                    };
                    let muted = theme::tokens(&ctx).muted;
                    ui.label(fonts::title(title));
                    ui.add_space(theme::SPACE[0]);
                    ui.label(fonts::text(fonts::Role::Body, subtitle).color(muted));
                    ui.add_space(theme::SPACE[3]);
                    if !self.files_home.error.is_empty() {
                        let red = theme::tokens(&ctx).red;
                        ui.label(fonts::text(fonts::Role::Body, &self.files_home.error).color(red));
                        ui.add_space(theme::SPACE[1]);
                    }
                    match self.files_home.page {
                        Page::Recent => self.files_recent_page(ui),
                        Page::Open => self.files_open_page(ui),
                        Page::Import => self.files_formats_page(ui, file_dialogs::Mode::Import),
                        Page::Export => self.files_formats_page(ui, file_dialogs::Mode::Export),
                        Page::ProjectSettings => self.files_settings_page(ui),
                        Page::Close => self.files_close_page(ui),
                    }
                });
            });
    }

    /// In a window too narrow for the left aside the commands sit in a row on the page.
    fn files_narrow_commands(&mut self, ui: &mut egui::Ui) {
        let mut picked = None;
        ui.horizontal_wrapped(|ui| {
            for (command, label, tip) in [
                (Command::New, t!("files.nav.new"), t!("files.nav.new.tip")),
                (
                    Command::Open,
                    t!("files.nav.open"),
                    t!("files.nav.open.tip"),
                ),
                (
                    Command::Save,
                    t!("files.nav.save"),
                    t!("files.nav.save.tip"),
                ),
                (
                    Command::SaveAs,
                    t!("files.nav.save_as"),
                    t!("files.nav.save_as.tip"),
                ),
                (
                    Command::Import,
                    t!("files.nav.import"),
                    t!("files.nav.import.tip"),
                ),
                (
                    Command::Export,
                    t!("files.nav.export"),
                    t!("files.nav.export.tip"),
                ),
            ] {
                if button::secondary_button(ui, None, &label, &tip, Size::Normal, true).clicked() {
                    picked = Some(command);
                }
            }
        });
        ui.add_space(theme::SPACE[2]);
        if let Some(command) = picked {
            self.files_run(command);
        }
    }

    /// Arrows and Enter in the list on the page that shows one.
    fn files_page_keys(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input()
            || ui_kit::menu::any_open(ctx)
            || self.edit.dialog.open
            || ctx.memory(|m| {
                m.focused()
                    .is_some_and(|id| !self.files_home.rows.contains(&id))
            })
        {
            return;
        }
        let up = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp));
        let down = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown));
        let enter = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
        match self.files_home.page {
            Page::Recent => {
                let list = self.files_listed();
                if list.is_empty() {
                    return;
                }
                let at = self
                    .files_home
                    .selected
                    .as_ref()
                    .and_then(|s| list.iter().position(|p| p == s));
                let next = match (at, up, down) {
                    (None, true, _) | (None, _, true) => Some(0),
                    (Some(i), true, _) => Some(i.saturating_sub(1)),
                    (Some(i), _, true) => Some((i + 1).min(list.len() - 1)),
                    _ => None,
                };
                if let Some(i) = next {
                    self.files_home.selected = list.get(i).cloned();
                    self.files_home.reveal = true;
                }
                if enter && let Some(path) = self.files_home.selected.clone() {
                    self.files_open_path(&path);
                }
            }
            Page::Open => {
                let count = self.files_home.picker.entries.len();
                if count == 0 {
                    return;
                }
                let at = self.files_home.picker.selected;
                let next = match (at, up, down) {
                    (None, true, _) | (None, _, true) => Some(0),
                    (Some(i), true, _) => Some(i.saturating_sub(1)),
                    (Some(i), _, true) => Some((i + 1).min(count - 1)),
                    _ => None,
                };
                if let Some(i) = next {
                    self.files_home.picker.selected = Some(i);
                    self.files_home.reveal = true;
                }
                if enter && let Some(i) = self.files_home.picker.selected {
                    self.files_picker_activate(i);
                }
            }
            _ => {}
        }
    }

    /// The Recent projects page.
    fn files_recent_page(&mut self, ui: &mut egui::Ui) {
        let tokens = theme::tokens(ui.ctx());
        self.files_current_card(ui);
        ui.add_space(theme::SPACE[3]);
        ui.label(fonts::section_caption(t!("files.recent.heading")).color(tokens.muted));
        ui.add_space(theme::SPACE[1]);
        let list = self.files_listed();
        if list.is_empty() {
            let clicked = ui_kit::empty_state::empty_state(
                ui,
                icons::PLUS,
                &t!("files.recent.empty.title"),
                &t!("files.recent.empty.message"),
                &t!("files.recent.empty.button"),
                &t!("files.recent.empty.tip"),
            )
            .clicked();
            if clicked {
                self.show_files(Page::Open);
            }
            self.files_home.hovered = None;
            return;
        }
        let mut hovered = None;
        let mut open = None;
        let mut pin = None;
        let mut remove = None;
        let mut select = None;
        for path in &list {
            let row = self.files_row(ui, path);
            if row.response.hovered() {
                hovered = Some(path.clone());
            }
            if row.response.clicked() {
                select = Some(path.clone());
            }
            if row.response.double_clicked() {
                open = Some(path.clone());
            }
            if row.pin {
                pin = Some(path.clone());
            }
            if row.remove {
                remove = Some(path.clone());
            }
        }
        if self.files_home.hovered != hovered {
            ui.ctx().request_repaint();
        }
        self.files_home.hovered = hovered;
        self.files_home.reveal = false;
        if let Some(path) = select {
            self.files_home.selected = Some(path);
        }
        if let Some(path) = pin {
            self.files_toggle_pin(&path);
        }
        if let Some(path) = remove {
            self.files_remove(&path);
        }
        if let Some(path) = open {
            self.files_open_path(&path);
        }
    }

    /// A card for the project that is open now.
    fn files_current_card(&mut self, ui: &mut egui::Ui) {
        let tokens = theme::tokens(ui.ctx());
        ui.label(fonts::section_caption(t!("files.current.heading")).color(tokens.muted));
        ui.add_space(theme::SPACE[1]);
        let name = project_name(&self.path);
        let parts = self.project.components.len();
        let state = if self.project_has_unsaved_changes() {
            t!("files.current.unsaved")
        } else if parts == 0 {
            t!("files.current.empty")
        } else {
            t!("files.current.saved")
        };
        egui::Frame::NONE
            .fill(tokens.panel)
            .stroke(egui::Stroke::new(theme::BORDER, tokens.line))
            .corner_radius(theme::RADIUS_CONTROL)
            .inner_margin(egui::Margin::symmetric(16, 12))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(fonts::strong(name).color(tokens.text));
                    ui.label(
                        fonts::caption(self.catalog.plural("cmp.count", parts as u64))
                            .color(tokens.muted),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(fonts::caption(state).color(tokens.muted));
                    });
                });
                let folder = short_folder(&self.path);
                if !folder.is_empty() {
                    ui.add(egui::Label::new(fonts::mono(folder).color(tokens.muted)).truncate());
                }
            });
    }

    /// One recent project row; tells the page what was pressed.
    fn files_row(&mut self, ui: &mut egui::Ui, path: &str) -> RowOutcome {
        let tokens = theme::tokens(ui.ctx());
        let selected = self.files_home.selected.as_deref() == Some(path);
        let pinned = self.preferences.pinned_projects.iter().any(|p| p == path);
        let name = project_name(path);
        let (rect, response) = ui.allocate_exact_size(
            Vec2::new(ui.available_width(), ROW_HEIGHT),
            egui::Sense::click(),
        );
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, selected, &name)
        });
        if selected && self.files_home.reveal {
            response.scroll_to_me(None);
        }
        self.keep_list_focus(ui, &response);
        if selected {
            ui.painter().rect_filled(rect, 0.0, tokens.copper_tint);
            ui.painter().rect_filled(
                egui::Rect::from_min_size(rect.min, Vec2::new(3.0, rect.height())),
                0.0,
                tokens.copper,
            );
        } else if response.hovered() {
            ui.painter().rect_filled(rect, 0.0, tokens.raised);
        }
        ui.painter().hline(
            rect.x_range(),
            rect.max.y - 0.5,
            egui::Stroke::new(theme::BORDER, tokens.line.gamma_multiply(0.55)),
        );
        let probe = self.files_home.probes.get(path);
        let missing = probe.is_some_and(|p| !p.exists);
        let picture = egui::Rect::from_min_size(
            rect.min + Vec2::new(12.0, 10.0),
            Vec2::new(ROW_PICTURE, ROW_HEIGHT - 20.0),
        );
        paint_picture(ui, picture, probe.and_then(|p| p.picture.as_ref()), &tokens);
        let buttons_width = 2.0 * 28.0 + theme::SPACE[0] + 12.0;
        let text_rect = egui::Rect::from_min_max(
            egui::pos2(picture.right() + 12.0, rect.top() + 12.0),
            egui::pos2(rect.right() - buttons_width - 8.0, rect.bottom() - 8.0),
        );
        let when = self.files_when(path);
        let mut text = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(text_rect)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        text.horizontal(|ui| {
            if pinned {
                let (r, _) =
                    ui.allocate_exact_size(Vec2::splat(icons::ICON_SIZE), egui::Sense::hover());
                (icons::PIN.0)(ui.painter(), r, tokens.copper);
            }
            let colour = if missing { tokens.muted } else { tokens.text };
            ui.add(egui::Label::new(fonts::strong(&name).color(colour)).truncate());
        });
        text.add(egui::Label::new(fonts::mono(short_folder(path)).color(tokens.muted)).truncate());
        let when_colour = if missing { tokens.gold } else { tokens.muted };
        text.add(egui::Label::new(fonts::caption(when).color(when_colour)).truncate());
        let mut pin = false;
        let mut remove = false;
        let mut buttons = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(egui::Rect::from_min_max(
                    egui::pos2(rect.right() - buttons_width, rect.top()),
                    egui::pos2(rect.right() - 12.0, rect.bottom()),
                ))
                .layout(egui::Layout::right_to_left(egui::Align::Center)),
        );
        let (remove_tip, pin_name, pin_tip) = if pinned {
            (
                t!("files.row.remove.tip"),
                t!("files.row.unpin"),
                t!("files.row.unpin.tip"),
            )
        } else {
            (
                t!("files.row.remove.tip"),
                t!("files.row.pin"),
                t!("files.row.pin.tip"),
            )
        };
        if button::icon_button(
            &mut buttons,
            icons::CLOSE,
            &t!("files.row.remove"),
            &remove_tip,
            Size::Normal,
            true,
        )
        .clicked()
        {
            remove = true;
        }
        if button::icon_button(
            &mut buttons,
            icons::PIN,
            &pin_name,
            &pin_tip,
            Size::Normal,
            true,
        )
        .clicked()
        {
            pin = true;
        }
        ui_kit::focus::focus_ring(ui, &response, 0);
        let response = response.on_hover_text(t!("files.row.tip"));
        RowOutcome {
            response,
            pin,
            remove,
        }
    }

    /// Note a list row and, while it has the focus, keep the arrow keys for the list.
    fn keep_list_focus(&mut self, ui: &egui::Ui, response: &egui::Response) {
        self.files_home.rows.push(response.id);
        if response.has_focus() {
            ui.memory_mut(|m| {
                m.set_focus_lock_filter(
                    response.id,
                    egui::EventFilter {
                        vertical_arrows: true,
                        ..Default::default()
                    },
                )
            });
        }
    }

    fn files_toggle_pin(&mut self, path: &str) {
        let pinned = &mut self.preferences.pinned_projects;
        if let Some(at) = pinned.iter().position(|p| p == path) {
            pinned.remove(at);
        } else {
            pinned.insert(0, path.to_owned());
            pinned.truncate(MAX_PINNED);
        }
    }

    /// Take a project off the list; the file stays where it is.
    fn files_remove(&mut self, path: &str) {
        pcbcraft_engine::recents::forget_recent_path(path);
        self.recent_designs = pcbcraft_engine::recents::load_recent_paths();
        self.preferences.pinned_projects.retain(|p| p != path);
        self.preferences.project_opened.remove(path);
        if self.files_home.selected.as_deref() == Some(path) {
            self.files_home.selected = None;
        }
        self.files_home.forget_probe(path);
    }

    /// The right aside of the Files tab: details of the hovered or selected project.
    ///
    /// Why: a name alone does not tell two copies of a board apart; the path, the
    /// picture and the dates do.
    ///
    /// # Rules
    /// - Shows the project under the pointer first, then the selected one, then the
    ///   project that is open now.
    ///
    /// # Dependencies
    /// - crates/ui-egui/src/lib.rs (`App::inspector` calls this on the Files tab)
    pub(crate) fn files_details(&mut self, ui: &mut egui::Ui) {
        ui.style_mut().interaction.selectable_labels = false;
        let tokens = theme::tokens(ui.ctx());
        let label = t!("files.details.tab");
        let tip = t!("files.details.tab.tip");
        let items = [ui_kit::tab_bar::TabItem::new(&label, &tip)];
        let mut index = 0;
        ui_kit::tab_bar::tab_bar(
            ui,
            "files-details-tabs",
            &mut index,
            &items,
            ui_kit::tab_bar::TabSize::Aside,
        );
        let shown = if self.files_home.page == Page::Recent {
            self.files_home
                .hovered
                .clone()
                .or_else(|| self.files_home.selected.clone())
        } else {
            None
        };
        egui::ScrollArea::vertical()
            .id_salt("files-details")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::Frame::NONE
                    .inner_margin(egui::Margin::symmetric(16, 12))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        match shown {
                            Some(path) => self.files_recent_details(ui, &path),
                            None => self.files_current_details(ui),
                        }
                    });
            });
        let _ = tokens;
    }

    fn files_recent_details(&mut self, ui: &mut egui::Ui, path: &str) {
        let tokens = theme::tokens(ui.ctx());
        let probe = self.files_home.probes.get(path);
        let exists = probe.is_none_or(|p| p.exists);
        let width = ui.available_width();
        let (rect, _) = ui.allocate_exact_size(
            Vec2::new(width, (width * 0.62).min(190.0)),
            egui::Sense::hover(),
        );
        paint_picture(ui, rect, probe.and_then(|p| p.picture.as_ref()), &tokens);
        ui.add_space(theme::SPACE[2]);
        ui.label(fonts::title(project_name(path)).color(tokens.text));
        ui.add_space(theme::SPACE[0]);
        ui.add(
            egui::Label::new(fonts::mono(path).color(tokens.muted))
                .wrap_mode(egui::TextWrapMode::Wrap),
        );
        ui.add_space(theme::SPACE[2]);
        let opened = self
            .preferences
            .project_opened
            .get(path)
            .map(|t| self.files_ago(*t))
            .unwrap_or_default();
        let modified = probe
            .and_then(|p| p.modified)
            .map(|t| self.files_ago(t))
            .unwrap_or_default();
        let size = probe.map(|p| p.bytes).filter(|b| *b > 0).map(size_text);
        let status = if exists {
            t!("files.details.there")
        } else {
            t!("files.missing")
        };
        for (key, value) in [
            (t!("files.details.opened"), opened),
            (t!("files.details.modified"), modified),
            (t!("files.details.size"), size.unwrap_or_default()),
            (t!("files.details.status"), status),
        ] {
            if value.is_empty() {
                continue;
            }
            detail_row(ui, &key, &value, &tokens);
        }
        ui.add_space(theme::SPACE[2]);
        let pinned = self.preferences.pinned_projects.iter().any(|p| p == path);
        let open_tip = if exists {
            t!("files.details.open.tip")
        } else {
            t!("files.details.open.missing")
        };
        if button::primary_button(
            ui,
            None,
            &t!("files.details.open"),
            &open_tip,
            Size::Normal,
            exists,
        )
        .clicked()
        {
            self.files_open_path(path);
        }
        ui.add_space(theme::SPACE[1]);
        ui.horizontal(|ui| {
            let (label, tip) = if pinned {
                (t!("files.row.unpin"), t!("files.row.unpin.tip"))
            } else {
                (t!("files.row.pin"), t!("files.row.pin.tip"))
            };
            if button::secondary_button(ui, Some(icons::PIN), &label, &tip, Size::Normal, true)
                .clicked()
            {
                self.files_toggle_pin(path);
            }
            if button::secondary_button(
                ui,
                None,
                &t!("files.details.remove"),
                &t!("files.row.remove.tip"),
                Size::Normal,
                true,
            )
            .clicked()
            {
                self.files_remove(path);
            }
        });
    }

    fn files_current_details(&mut self, ui: &mut egui::Ui) {
        let tokens = theme::tokens(ui.ctx());
        ui.label(fonts::section_caption(t!("files.current.heading")).color(tokens.muted));
        ui.add_space(theme::SPACE[1]);
        ui.label(fonts::title(project_name(&self.path)).color(tokens.text));
        ui.add_space(theme::SPACE[0]);
        ui.add(
            egui::Label::new(fonts::mono(&self.path).color(tokens.muted))
                .wrap_mode(egui::TextWrapMode::Wrap),
        );
        ui.add_space(theme::SPACE[2]);
        let state = if self.project_has_unsaved_changes() {
            t!("files.current.unsaved")
        } else if self.project.components.is_empty() {
            t!("files.current.empty")
        } else {
            t!("files.current.saved")
        };
        let board = format!(
            "{} × {} {}",
            self.project.board.width,
            self.project.board.height,
            t!("unit.mm")
        );
        for (key, value) in [
            (t!("files.details.status"), state),
            (
                t!("files.details.parts"),
                self.project.components.len().to_string(),
            ),
            (
                t!("files.details.nets"),
                self.project.nets.len().to_string(),
            ),
            (t!("files.details.board"), board),
        ] {
            detail_row(ui, &key, &value, &tokens);
        }
        ui.add_space(theme::SPACE[2]);
        ui.label(fonts::text(fonts::Role::Body, t!("files.details.hint")).color(tokens.muted));
    }

    /// The Open page: a folder browser with a typed path above it.
    fn files_open_page(&mut self, ui: &mut egui::Ui) {
        let tokens = theme::tokens(ui.ctx());
        if !self.files_home.picker.loaded {
            self.files_picker_load();
        }
        let mut go = false;
        ui.horizontal(|ui| {
            let field = ui
                .add(
                    crate::targets::text_field(&mut self.files_home.picker.typed)
                        .hint_text(t!("files.open.path_hint"))
                        .desired_width((ui.available_width() - 100.0).max(160.0)),
                )
                .named(&t!("files.open.path_name"))
                .on_hover_text(t!("files.open.path.tip"));
            if field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                go = true;
            }
            if button::primary_button(
                ui,
                None,
                &t!("files.open.go"),
                &t!("files.open.go.tip"),
                Size::Normal,
                true,
            )
            .clicked()
            {
                go = true;
            }
        });
        ui.add_space(theme::SPACE[1]);
        let mut up = false;
        let mut home = false;
        ui.horizontal(|ui| {
            let has_parent = self.files_home.picker.dir.parent().is_some();
            up = button::secondary_button(
                ui,
                None,
                &t!("files.open.up"),
                &t!("files.open.up.tip"),
                Size::Small,
                has_parent,
            )
            .clicked();
            home = button::secondary_button(
                ui,
                None,
                &t!("files.open.home"),
                &t!("files.open.home.tip"),
                Size::Small,
                true,
            )
            .clicked();
            ui.add(
                egui::Label::new(
                    fonts::mono(self.files_home.picker.dir.to_string_lossy()).color(tokens.muted),
                )
                .truncate(),
            );
        });
        ui.add_space(theme::SPACE[1]);
        if up && let Some(parent) = self.files_home.picker.dir.parent() {
            self.files_picker_enter(parent.to_path_buf());
        }
        if home {
            let dir = std::env::var("HOME")
                .or_else(|_| std::env::var("USERPROFILE"))
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from("."));
            self.files_picker_enter(dir);
        }
        let entries = self.files_home.picker.entries.clone();
        if entries.is_empty() {
            ui.label(fonts::text(fonts::Role::Body, t!("files.open.empty")).color(tokens.muted));
        }
        let mut activate = None;
        let mut select = None;
        for (index, entry) in entries.iter().enumerate() {
            let selected = self.files_home.picker.selected == Some(index);
            let label = if entry.folder {
                format!("{}/", entry.name)
            } else {
                entry.name.clone()
            };
            let tip = if entry.folder {
                t!("files.open.folder.tip")
            } else {
                t!("files.open.file.tip")
            };
            let response = ui_kit::rows::list_row(ui, selected, &label, &tip, |ui| {
                let colour = if entry.folder {
                    tokens.cyan
                } else {
                    tokens.text
                };
                ui.label(fonts::text(fonts::Role::Body, &label).color(colour));
            });
            if selected && self.files_home.reveal {
                response.scroll_to_me(None);
            }
            self.keep_list_focus(ui, &response);
            if response.double_clicked() {
                activate = Some(index);
            } else if response.clicked() {
                select = Some(index);
            }
        }
        self.files_home.reveal = false;
        if let Some(index) = select {
            self.files_home.picker.selected = Some(index);
            if let Some(entry) = entries.get(index)
                && !entry.folder
            {
                self.files_home.picker.typed = self
                    .files_home
                    .picker
                    .dir
                    .join(&entry.name)
                    .to_string_lossy()
                    .into_owned();
            }
        }
        if let Some(index) = activate {
            self.files_picker_activate(index);
        }
        if go {
            let typed = self.files_home.picker.typed.clone();
            let path = Path::new(typed.trim());
            if path.is_dir() {
                self.files_picker_enter(path.to_path_buf());
            } else {
                self.files_open_path(&typed);
            }
        }
        ui.add_space(theme::SPACE[2]);
        ui.label(fonts::caption(t!("files.open.hint")).color(tokens.muted));
    }

    /// Fill the folder listing, starting beside the current project.
    fn files_picker_load(&mut self) {
        if self.files_home.picker.dir.as_os_str().is_empty() {
            let start = Path::new(&self.path)
                .parent()
                .filter(|p| p.is_dir())
                .map(Path::to_path_buf)
                .or_else(|| std::env::current_dir().ok())
                .unwrap_or_else(|| PathBuf::from("."));
            self.files_home.picker.dir = start;
        }
        let dir = self.files_home.picker.dir.clone();
        self.files_picker_enter(dir);
    }

    /// Show the contents of a folder: folders first, then project files.
    fn files_picker_enter(&mut self, dir: PathBuf) {
        let mut entries: Vec<Entry> = Vec::new();
        let mut error = String::new();
        match std::fs::read_dir(&dir) {
            Ok(read) => {
                for item in read.flatten().take(MAX_ENTRIES * 4) {
                    let name = item.file_name().to_string_lossy().into_owned();
                    if name.starts_with('.') {
                        continue;
                    }
                    let folder = item.file_type().map(|t| t.is_dir()).unwrap_or(false);
                    let project = name.ends_with(".pcbcraft") || name.ends_with(".json");
                    if folder || project {
                        entries.push(Entry { name, folder });
                    }
                }
            }
            Err(_) => error = t!("files.open.unreadable"),
        }
        entries.sort_by(|a, b| {
            b.folder
                .cmp(&a.folder)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        entries.truncate(MAX_ENTRIES);
        let picker = &mut self.files_home.picker;
        picker.typed = dir.to_string_lossy().into_owned();
        picker.dir = dir;
        picker.entries = entries;
        picker.selected = None;
        picker.loaded = true;
        picker.error = error.clone();
        self.files_home.error = error;
    }

    /// Enter a folder or open a file of the listing.
    fn files_picker_activate(&mut self, index: usize) {
        let Some(entry) = self.files_home.picker.entries.get(index).cloned() else {
            return;
        };
        let path = self.files_home.picker.dir.join(&entry.name);
        if entry.folder {
            self.files_picker_enter(path);
        } else {
            self.files_open_path(&path.to_string_lossy());
        }
    }

    /// The Import and Export pages: Recent, Common, More, as in the File menu.
    fn files_formats_page(&mut self, ui: &mut egui::Ui, mode: file_dialogs::Mode) {
        let tokens = theme::tokens(ui.ctx());
        let recent = match mode {
            file_dialogs::Mode::Import => self.preferences.recent_files.imports.clone(),
            _ => self.preferences.recent_files.exports.clone(),
        };
        let direction = if mode == file_dialogs::Mode::Import {
            Direction::Import
        } else {
            Direction::Export
        };
        let scope = if mode == file_dialogs::Mode::Import {
            Scope::Parts
        } else {
            Scope::Output
        };
        let mut picked: Option<(Option<String>, Option<file_dialogs::Values>)> = None;
        if !recent.is_empty() {
            ui.label(fonts::section_caption(t!("files.formats.recent")).color(tokens.muted));
            ui.add_space(theme::SPACE[1]);
            for r in &recent {
                let response = ui_kit::rows::list_row(
                    ui,
                    false,
                    &r.summary,
                    &t!("files.formats.recent.tip"),
                    |ui| {
                        ui.label(fonts::text(fonts::Role::Body, &r.summary).color(tokens.text));
                    },
                );
                if response.clicked() {
                    picked = Some((Some(r.format.clone()), Some(r.values.clone())));
                }
            }
            ui.add_space(theme::SPACE[2]);
        }
        ui.label(fonts::section_caption(t!("files.formats.common")).color(tokens.muted));
        ui.add_space(theme::SPACE[1]);
        for f in formats::listed(direction, scope)
            .into_iter()
            .filter(|f| f.common)
        {
            if recent.iter().any(|r| r.format == f.id) {
                continue;
            }
            let response = ui_kit::rows::list_row(ui, false, f.label, f.tip, |ui| {
                ui.label(fonts::text(fonts::Role::Body, f.label).color(tokens.text));
                ui.add(egui::Label::new(fonts::caption(f.tip).color(tokens.muted)).truncate());
            });
            if response.clicked() {
                picked = Some((Some(f.id.to_owned()), None));
            }
        }
        ui.add_space(theme::SPACE[2]);
        if button::secondary_button(
            ui,
            None,
            &t!("files.formats.more"),
            &t!("files.formats.more.tip"),
            Size::Normal,
            true,
        )
        .clicked()
        {
            picked = Some((None, None));
        }
        if let Some((format, values)) = picked {
            self.open_dialog(mode, format.as_deref(), values, vec![]);
        }
    }

    /// The Project settings page: links until the tabbed dialog exists.
    fn files_settings_page(&mut self, ui: &mut egui::Ui) {
        let tokens = theme::tokens(ui.ctx());
        ui.label(fonts::text(fonts::Role::Body, t!("files.settings.note")).color(tokens.muted));
        ui.add_space(theme::SPACE[2]);
        for (label, tip, tab) in [
            (
                t!("files.settings.rules"),
                t!("files.settings.rules.tip"),
                Tab::Rules,
            ),
            (
                t!("files.settings.app"),
                t!("files.settings.app.tip"),
                Tab::Settings,
            ),
        ] {
            if button::secondary_button(ui, None, &label, &tip, Size::Normal, true).clicked() {
                self.change_tab(tab);
            }
            ui.add_space(theme::SPACE[1]);
        }
        ui.add_space(theme::SPACE[2]);
        self.files_tab(ui);
    }

    /// The Close page: only shown when the project has changes that are not saved.
    fn files_close_page(&mut self, ui: &mut egui::Ui) {
        let tokens = theme::tokens(ui.ctx());
        ui.label(
            fonts::text(
                fonts::Role::Body,
                t!("files.close.warning", name = project_name(&self.path)),
            )
            .color(tokens.text),
        );
        ui.add_space(theme::SPACE[2]);
        ui.horizontal(|ui| {
            if button::primary_button(
                ui,
                None,
                &t!("files.close.save"),
                &t!("files.close.save.tip"),
                Size::Normal,
                true,
            )
            .clicked()
            {
                if self.project_never_saved() {
                    self.open_dialog(file_dialogs::Mode::SaveAs, None, None, vec![]);
                } else {
                    let path = self.path.clone();
                    if self.tool_call("save", json!({"path": path})) {
                        self.files_close_now();
                    }
                }
            }
            if button::secondary_button(
                ui,
                None,
                &t!("files.close.discard"),
                &t!("files.close.discard.tip"),
                Size::Normal,
                true,
            )
            .clicked()
            {
                self.files_close_now();
            }
            if button::ghost_button(
                ui,
                None,
                &t!("files.close.cancel"),
                &t!("files.close.cancel.tip"),
                Size::Normal,
                true,
            )
            .clicked()
            {
                self.files_home.page = Page::Recent;
            }
        });
    }

    /// Capture scenes: `files-open` and `files-export` pick the page for a screenshot.
    pub(crate) fn files_capture_scene(&mut self, scene: &str) {
        match scene {
            "files-open" => self.show_files(Page::Open),
            "files-export" => self.show_files(Page::Export),
            "files-import" => self.show_files(Page::Import),
            _ => {}
        }
    }
}

/// What a recent project row reported.
struct RowOutcome {
    response: egui::Response,
    pin: bool,
    remove: bool,
}

/// A picture in a box, or the empty-board mark when there is none.
fn paint_picture(
    ui: &egui::Ui,
    rect: egui::Rect,
    texture: Option<&egui::TextureHandle>,
    tokens: &theme::Tokens,
) {
    ui.painter()
        .rect_filled(rect, theme::RADIUS_CONTROL, tokens.raised);
    ui.painter().rect_stroke(
        rect,
        theme::RADIUS_CONTROL,
        egui::Stroke::new(theme::BORDER, tokens.line),
        egui::StrokeKind::Inside,
    );
    match texture {
        Some(texture) => {
            let size = texture.size_vec2();
            let scale = (rect.width() / size.x).min(rect.height() / size.y);
            let fitted = egui::Rect::from_center_size(rect.center(), size * scale);
            ui.painter().image(
                texture.id(),
                fitted,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        None => {
            let icon: Icon = icons::FIT;
            let side = icons::ICON_SIZE.min(rect.height() * 0.5);
            let r = egui::Rect::from_center_size(rect.center(), Vec2::splat(side));
            (icon.0)(ui.painter(), r, tokens.muted);
        }
    }
}

/// A label and its value on one line of the details.
fn detail_row(ui: &mut egui::Ui, key: &str, value: &str, tokens: &theme::Tokens) {
    ui.horizontal_wrapped(|ui| {
        ui.label(fonts::caption(key).color(tokens.muted));
        ui.label(fonts::text(fonts::Role::Body, value).color(tokens.text));
    });
}

/// File size in kilobytes or megabytes.
fn size_text(bytes: u64) -> String {
    if bytes >= 1_048_576 {
        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
    } else {
        format!("{} KB", bytes.div_ceil(1024).max(1))
    }
}
