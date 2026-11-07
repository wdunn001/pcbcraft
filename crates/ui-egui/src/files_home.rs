//! The Files workspace: a tool bar, the project at a glance in the centre and the
//! project's own properties on the right.
//!
//! Context: the Files tab is a workspace (`workspace.rs`, the Workspace contract). The
//! approved mockup `files` has a tool bar (New, Open, Save, Import, Export, Open sample
//! project) and the project files tree in the left aside. The centre shows the open project's
//! name and its saved state, four cards (Parts, Nets, Board shape, Checks), the Shape of
//! each layer row, Next and Recent projects. The right aside holds Properties: name, units,
//! author, notes, the read only shape, layers and Project settings. There is no board width
//! or height to edit here. `home.rs`, `shape_cards.rs`, `tree.rs` and `props.rs` draw all of
//! that. This file holds the state, the worker that looks at files, the Open, Import, Export
//! and Close pages that the tool bar leads to, and the commands.
//!
//! Local rules:
//! - Import and Export pages list the same registry entries as the File menu (Recent,
//!   Common, More) and open the same dialog (`file_dialogs.rs`); no format is named here.
//! - Recent projects come from `pcbcraft_engine::recents`; pins and last-opened times
//!   live in `Preferences`. File facts are read on a worker thread and never on the
//!   window thread.
//! - Project settings is the dialog of `project_settings/`, opened from here and from the
//!   File menu; this tab has no page of its own for it.
//! - Every string comes from the catalog (`crates/text/strings/en.txt`, keys `files.*`).
//!
//! # Dependencies
//! - crates/ui-egui/src/workspace.rs (`FilesWorkspace` calls `files_toolbar`, `files_center`)
//! - crates/ui-egui/src/files_home/home.rs (header and Recent projects), shape_cards.rs, tree.rs, props.rs
//! - crates/ui-egui/src/file_dialogs.rs (`App::open_dialog`, `Recents`)
//! - crates/engine/src/recents.rs (the list of recent paths)
//! - crates/ui-egui/src/ui_kit/rows.rs, button.rs, empty_state.rs
//!
//! # Do not
//! - Run a dialog's work here; open the dialog.
//! - Read a file or its metadata on the window thread.
//! - Touch the real recents file in a test; set `RECENTS_PATH_TEST` to a temp dir.

mod home;
mod props;
mod shape_cards;
mod tree;

use super::*;
use crate::ui_kit::button::{self, Size};
use crate::ui_kit::icons;
use pcbcraft_io::formats::{self, Direction, Scope};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

/// The name a design has before it is saved anywhere.
pub(crate) const DEFAULT_PROJECT_PATH: &str = "design.pcbcraft";

/// Windows narrower than this give the page 16 points at the sides instead of 32.
const PAGE_NARROW_BELOW: f32 = 600.0;
/// Pinned projects kept.
const MAX_PINNED: usize = 12;
/// Entries a folder listing shows.
const MAX_ENTRIES: usize = 400;
/// The page in the centre.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Page {
    /// The project at a glance, the default.
    #[default]
    Home,
    /// The folder browser.
    Open,
    /// Formats to bring in.
    Import,
    /// Formats to make.
    Export,
    /// Asks before closing a project with changes.
    Close,
}

/// A command of the tool bar or the File menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Command {
    New,
    Open,
    Save,
    Import,
    Export,
    Sample,
    Close,
}

/// What the worker found out about one file.
struct Found {
    path: String,
    exists: bool,
    modified: Option<u64>,
    bytes: u64,
}

/// What is known about one recent project's file.
struct Probe {
    exists: bool,
    modified: Option<u64>,
    bytes: u64,
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
    /// The words typed in the project files tree filter.
    pub(crate) filter: String,
    /// Tree rows the person flipped from their first state (top sections start open,
    /// groups start folded).
    tree_flipped: BTreeSet<String>,
    /// The Start from a rectangle dialog while it is open.
    pub(crate) rect_draft: Option<RectDraft>,
}

/// The two numbers of the Start from a rectangle dialog, in millimetres.
///
/// Why: a rectangle is only a template for the free-form shape, so these two numbers
/// exist in this dialog and nowhere else on the Files page.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RectDraft {
    /// How far the template reaches across.
    pub(crate) width: f64,
    /// How far the template reaches down.
    pub(crate) height: f64,
}

impl State {
    /// Forget what was read about `path` so the next frame reads it again (after a
    /// save, which changes the file).
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
    pcbcraft_clock::unix_secs()
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

    /// True when the project has no file of its own yet: it was never saved
    /// and its path is still the placeholder name, so Save has to ask where.
    /// A path the person chose (a command-line file, a typed name) is written to directly.
    fn project_never_saved(&self) -> bool {
        self.saved_revision.is_none()
            && self.path == DEFAULT_PROJECT_PATH
            && !Path::new(&self.path).exists()
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
        if self.webfs_save() {
            return;
        }
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
            if !self.webfs_save() {
                self.open_dialog(file_dialogs::Mode::SaveAs, None, None, vec![]);
            }
        } else if ctx.input_mut(|i| i.consume_shortcut(&save)) {
            self.save_or_ask();
        } else if ctx.input_mut(|i| i.consume_shortcut(&open)) && !self.webfs_open() {
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
        self.path = DEFAULT_PROJECT_PATH.into();
        self.saved_revision = None;
        self.files_home.page = Page::Home;
        self.message = t!("files.close.done");
    }

    /// Do a tool bar or File menu command.
    pub(crate) fn files_run(&mut self, command: Command) {
        match command {
            Command::New => {
                self.tool_call("new_design", json!({}));
                self.select(None);
                self.change_tab(Tab::Schematic);
            }
            Command::Open => {
                if !self.webfs_open() {
                    self.show_files(Page::Open);
                }
            }
            Command::Save => self.save_or_ask(),
            Command::Import => self.show_files(Page::Import),
            Command::Export => self.show_files(Page::Export),
            Command::Sample => {
                self.tool_call("sample_design", json!({}));
                self.select(None);
                self.show_files(Page::Home);
            }
            Command::Close => {
                if self.project_has_unsaved_changes() {
                    self.show_files(Page::Close);
                } else {
                    self.files_close_now();
                }
            }
        }
    }

    /// The paths the worker looks at: the listed projects and the open one.
    pub(crate) fn files_probe_paths(&self) -> Vec<String> {
        let mut paths = self.files_listed();
        if !paths.contains(&self.path) {
            paths.push(self.path.clone());
        }
        paths
    }

    /// Ask the worker thread for what is not known yet about the listed projects.
    fn files_probe(&mut self, ctx: &egui::Context) {
        let listed = self.files_probe_paths();
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
                    .name("files-facts".into())
                    .spawn(move || {
                        for path in keys {
                            let meta = std::fs::metadata(&path).ok();
                            let modified = meta
                                .as_ref()
                                .and_then(|m| m.modified().ok())
                                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                                .map(|d| d.as_secs());
                            let found = Found {
                                exists: meta.as_ref().is_some_and(|m| m.is_file()),
                                modified,
                                bytes: meta.as_ref().map_or(0, |m| m.len()),
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
                            self.files_home.probes.insert(
                                found.path,
                                Probe {
                                    exists: found.exists,
                                    modified: found.modified,
                                    bytes: found.bytes,
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
    /// Why: one place decides which page shows. The tool bar and the keyboard
    /// agree with it.
    ///
    /// # Rules
    /// - The page sits on the `ground` colour with 32 side and 24 top padding.
    /// - Arrow keys and Enter act on the list only while nothing else holds the focus.
    ///
    /// # Dependencies
    /// - crates/ui-egui/src/workspace.rs (`FilesWorkspace::center`)
    /// - crates/ui-egui/src/files_home/home.rs (`files_home_page`)
    ///
    /// # Do not
    /// - Draw the Library or Layers here.
    pub(crate) fn files_center(&mut self, ui: &mut egui::Ui) {
        ui.style_mut().interaction.selectable_labels = false;
        let ctx = ui.ctx().clone();
        let tokens = theme::tokens(&ctx);
        ui.painter().rect_filled(ui.max_rect(), 0.0, tokens.ground);
        self.files_probe(&ctx);
        self.files_page_keys(&ctx);
        self.files_home.rows.clear();
        egui::ScrollArea::vertical()
            .id_salt("files-page")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let side = if ctx.content_rect().width() < PAGE_NARROW_BELOW {
                    theme::SPACE[3]
                } else {
                    theme::SPACE[5]
                };
                egui::Frame::NONE
                    .inner_margin(egui::Margin::symmetric(side as i8, theme::SPACE[4] as i8))
                    .show(ui, |ui| {
                        if self.files_home.page == Page::Home {
                            self.files_home_page(ui);
                        } else {
                            self.files_subpage(ui);
                        }
                    });
            });
    }

    /// A page behind a tool bar button: Open, Import, Export or Close.
    fn files_subpage(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        ui.set_max_width(ui.available_width().min(860.0));
        if button::ghost_button(
            ui,
            Some(icons::CHEVRON_LEFT),
            &t!("files.back"),
            &t!("files.back.tip"),
            Size::Normal,
            true,
        )
        .clicked()
        {
            self.files_home.page = Page::Home;
            self.files_home.error.clear();
        }
        ui.add_space(theme::SPACE[1]);
        let (title, subtitle) = match self.files_home.page {
            Page::Home | Page::Open => (t!("files.open.title"), t!("files.open.subtitle")),
            Page::Import => (t!("files.import.title"), t!("files.import.subtitle")),
            Page::Export => (t!("files.export.title"), t!("files.export.subtitle")),
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
            Page::Home | Page::Open => self.files_open_page(ui),
            Page::Import => self.files_formats_page(ui, file_dialogs::Mode::Import),
            Page::Export => self.files_formats_page(ui, file_dialogs::Mode::Export),
            Page::Close => self.files_close_page(ui),
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
            Page::Home => {
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
        let config = self.user_config();
        pcbcraft_engine::recents::forget_recent_path(&config, path);
        self.recent_designs = pcbcraft_engine::recents::load_recent_paths(&config);
        self.preferences.pinned_projects.retain(|p| p != path);
        self.preferences.project_opened.remove(path);
        if self.files_home.selected.as_deref() == Some(path) {
            self.files_home.selected = None;
        }
        self.files_home.forget_probe(path);
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
                self.files_home.page = Page::Home;
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

/// File size in kilobytes or megabytes.
fn size_text(bytes: u64) -> String {
    if bytes >= 1_048_576 {
        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
    } else {
        format!("{} KB", bytes.div_ceil(1024).max(1))
    }
}

impl App {
    /// Tell the user which nets an opened design had folded together because
    /// they shared a name. Nothing is said when no name repeated.
    ///
    /// # Rules
    /// - Reads the `merged` list the `open` tool returns and writes one plain
    ///   sentence to the status line that names each net and how many were joined.
    ///
    /// # Dependencies
    /// - crates/engine/src/net_names.rs (`NetMerge` is the shape of each entry)
    /// - crates/text/strings/en.txt (`schematic.merge.notice`)
    ///
    /// # Do not
    /// - Merge anything here: the engine already did, as one undo step.
    pub(crate) fn note_merged_nets(&mut self, result: &serde_json::Value) {
        let Some(rows) = result
            .pointer("/structuredContent/merged")
            .and_then(serde_json::Value::as_array)
        else {
            return;
        };
        let items: Vec<String> = rows
            .iter()
            .filter_map(|row| {
                let name = row.get("name")?.as_str()?;
                let count = row.get("merged")?.as_array()?.len() + 1;
                Some(t!("schematic.merge.item", name = name, count = count))
            })
            .collect();
        if !items.is_empty() {
            self.message = t!("schematic.merge.notice", list = items.join(", "));
        }
    }
}
