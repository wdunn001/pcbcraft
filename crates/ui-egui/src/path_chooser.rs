//! A small in-app chooser for one folder, one file to read or one file to write.
//!
//! Context: owner bugs "Load library does not work" and "can't add folder on sources". Both
//! asked for a typed path, which gives no result when it is empty or wrong. The app has no
//! operating system dialog; the Open page already browses folders inside the window, and this
//! window is the same idea for the Library page: Up, Home, a typed path and a list, then one
//! button that takes the choice.
//! Local rules: the chooser never reads or writes a file; it only lists folders and hands one
//! path back. The caller runs the engine command and shows its answer. In the browser build
//! there is no disk, so callers use the project host (`webfs.rs`) and never open this window.
//!
//! # Dependencies
//! - crates/ui-egui/src/ui_kit (buttons, list rows)
//! - crates/ui-egui/src/library.rs and authoring.rs (callers: Load library, Save library, Add folder)
//!
//! # Do not
//! - Read a file here, or run a command; return the path through `Pick`.
//! - Open this window in the browser build.
use crate::a11y::Named;
use crate::ui_kit::button::{self, Size};
use crate::ui_kit::rows::list_row;
use crate::{App, fonts, theme};
use pcbcraft_text::t;
use std::path::{Path, PathBuf};

/// Most rows the list shows for one folder.
const MAX_ROWS: usize = 400;

/// Why the chooser is open, so the answer goes to the right place.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Purpose {
    /// Read a collection file into the project library.
    LoadLibrary,
    /// Write the project library to a collection file.
    SaveLibrary,
    /// Add a folder of collections as a library source.
    AddSource,
}

/// What the chooser takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// A folder: the one shown.
    Folder,
    /// An existing file with this extension.
    File,
    /// A file name to write in the folder shown.
    Save,
}

impl Purpose {
    fn kind(self) -> Kind {
        match self {
            Self::LoadLibrary => Kind::File,
            Self::SaveLibrary => Kind::Save,
            Self::AddSource => Kind::Folder,
        }
    }
    fn title(self) -> String {
        match self {
            Self::LoadLibrary => t!("chooser.title.load_library"),
            Self::SaveLibrary => t!("chooser.title.save_library"),
            Self::AddSource => t!("chooser.title.add_source"),
        }
    }
}

/// The file name ending a collection file carries.
pub(crate) const LIBRARY_EXTENSION: &str = "pcbcraft-lib";

/// One row of the folder listing.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Entry {
    name: String,
    folder: bool,
}

/// What the chooser remembers between frames.
#[derive(Default)]
pub(crate) struct State {
    purpose: Option<Purpose>,
    dir: PathBuf,
    typed: String,
    name: String,
    entries: Vec<Entry>,
    selected: Option<usize>,
    note: String,
}

/// A finished choice, taken once by the caller.
pub(crate) struct Pick {
    pub(crate) purpose: Purpose,
    pub(crate) path: String,
}

/// List a folder: folders first, then the files this kind shows, both sorted by name.
fn list(dir: &Path, kind: Kind) -> Result<Vec<Entry>, String> {
    let read = std::fs::read_dir(dir).map_err(|e| e.to_string())?;
    let mut folders = Vec::new();
    let mut files = Vec::new();
    for item in read.flatten() {
        let name = item.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let is_dir = item.file_type().map(|k| k.is_dir()).unwrap_or(false);
        if is_dir {
            folders.push(Entry { name, folder: true });
        } else if kind != Kind::Folder
            && Path::new(&name)
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case(LIBRARY_EXTENSION))
        {
            files.push(Entry {
                name,
                folder: false,
            });
        }
    }
    folders.sort_by_key(|e| e.name.to_lowercase());
    files.sort_by_key(|e| e.name.to_lowercase());
    folders.extend(files);
    folders.truncate(MAX_ROWS);
    Ok(folders)
}

fn home_dir() -> PathBuf {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

impl State {
    /// Show a folder and its rows; an unreadable folder keeps the old one and says why.
    fn enter(&mut self, dir: PathBuf) {
        let kind = self.purpose.map_or(Kind::Folder, Purpose::kind);
        match list(&dir, kind) {
            Ok(entries) => {
                self.typed = dir.to_string_lossy().into_owned();
                self.dir = dir;
                self.entries = entries;
                self.selected = None;
                self.note.clear();
            }
            Err(_) => self.note = t!("chooser.unreadable"),
        }
    }
}

impl App {
    /// Open the chooser for one purpose, starting beside the current design or in the home folder.
    ///
    /// # Rules
    /// - Desktop only in effect: the browser build picks through the project host instead.
    ///
    /// # Dependencies
    /// - crates/ui-egui/src/path_chooser.rs (`State`)
    pub(crate) fn open_chooser(&mut self, purpose: Purpose) {
        let start = Path::new(&self.path)
            .parent()
            .filter(|p| p.is_dir())
            .map(Path::to_path_buf)
            .unwrap_or_else(home_dir);
        let chooser = &mut self.chooser;
        chooser.purpose = Some(purpose);
        chooser.name = format!("library.{LIBRARY_EXTENSION}");
        chooser.enter(start);
    }

    /// True while the chooser is on screen.
    #[cfg(test)]
    pub(crate) fn chooser_open(&self) -> bool {
        self.chooser.purpose.is_some()
    }

    /// Draw the chooser and return the choice once the person confirms it.
    ///
    /// # Rules
    /// - Cancel and the window's close button drop the request and run nothing.
    ///
    /// # Do not
    /// - Run the engine command here; the caller does, so its answer shows on its own page.
    pub(crate) fn chooser_window(&mut self, ctx: &egui::Context) -> Option<Pick> {
        let purpose = self.chooser.purpose?;
        let kind = purpose.kind();
        let tokens = theme::tokens(ctx);
        let mut open = true;
        let mut confirm: Option<String> = None;
        let mut cancel = false;
        egui::Window::new(purpose.title())
            .id(egui::Id::new("path-chooser"))
            .collapsible(false)
            .resizable(true)
            .default_width(520.0)
            .open(&mut open)
            .show(ctx, |ui| {
                let s = &mut self.chooser;
                let mut go = false;
                ui.horizontal(|ui| {
                    let field = ui
                        .add(
                            crate::targets::text_field(&mut s.typed)
                                .hint_text(t!("chooser.path_hint"))
                                .desired_width((ui.available_width() - 80.0).max(160.0)),
                        )
                        .named(&t!("chooser.path_name"))
                        .on_hover_text(t!("chooser.path.tip"));
                    if field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        go = true;
                    }
                    if button::secondary_button(
                        ui,
                        None,
                        &t!("chooser.go"),
                        &t!("chooser.go.tip"),
                        Size::Small,
                        true,
                    )
                    .clicked()
                    {
                        go = true;
                    }
                });
                let (mut up, mut home) = (false, false);
                ui.horizontal(|ui| {
                    up = button::secondary_button(
                        ui,
                        None,
                        &t!("chooser.up"),
                        &t!("chooser.up.tip"),
                        Size::Small,
                        s.dir.parent().is_some(),
                    )
                    .clicked();
                    home = button::secondary_button(
                        ui,
                        None,
                        &t!("chooser.home"),
                        &t!("chooser.home.tip"),
                        Size::Small,
                        true,
                    )
                    .clicked();
                });
                if up && let Some(parent) = s.dir.parent() {
                    let parent = parent.to_path_buf();
                    s.enter(parent);
                }
                if home {
                    s.enter(home_dir());
                }
                if go {
                    let typed = PathBuf::from(s.typed.trim());
                    if typed.is_dir() {
                        s.enter(typed);
                    } else {
                        s.note = t!("chooser.not_a_folder");
                    }
                }
                ui.add_space(theme::SPACE[1]);
                if s.entries.is_empty() {
                    ui.label(
                        fonts::text(fonts::Role::Body, t!("chooser.empty")).color(tokens.muted),
                    );
                }
                let (mut enter, mut pick) = (None, None);
                egui::ScrollArea::vertical()
                    .id_salt("path-chooser-list")
                    .max_height(280.0)
                    .show(ui, |ui| {
                        for (index, entry) in s.entries.iter().enumerate() {
                            let label = if entry.folder {
                                format!("{}/", entry.name)
                            } else {
                                entry.name.clone()
                            };
                            let tip = if entry.folder {
                                t!("chooser.folder.tip")
                            } else {
                                t!("chooser.file.tip")
                            };
                            let response =
                                list_row(ui, s.selected == Some(index), &label, &tip, |ui| {
                                    let colour = if entry.folder {
                                        tokens.cyan
                                    } else {
                                        tokens.text
                                    };
                                    ui.label(fonts::text(fonts::Role::Body, &label).color(colour));
                                });
                            if response.double_clicked() {
                                if entry.folder {
                                    enter = Some(index);
                                } else {
                                    pick = Some(index);
                                }
                            } else if response.clicked() {
                                s.selected = Some(index);
                            }
                        }
                    });
                if let Some(index) = enter {
                    let next = s.dir.join(&s.entries[index].name);
                    s.enter(next);
                }
                if kind == Kind::Save {
                    ui.add_space(theme::SPACE[1]);
                    ui.horizontal(|ui| {
                        ui.label(fonts::caption(t!("chooser.file_name")).color(tokens.muted));
                        ui.add(
                            crate::targets::text_field(&mut s.name)
                                .desired_width((ui.available_width() - 8.0).max(120.0)),
                        )
                        .named(&t!("chooser.file_name"))
                        .on_hover_text(t!("chooser.file_name.tip"));
                    });
                }
                if !s.note.is_empty() {
                    ui.label(fonts::caption(&s.note).color(tokens.red));
                }
                ui.add_space(theme::SPACE[1]);
                let chosen_file = s
                    .selected
                    .and_then(|i| s.entries.get(i))
                    .filter(|e| !e.folder)
                    .map(|e| s.dir.join(&e.name));
                let save_name = s.name.trim().to_owned();
                let ok = match kind {
                    Kind::Folder => true,
                    Kind::File => chosen_file.is_some() || pick.is_some(),
                    Kind::Save => !save_name.is_empty(),
                };
                ui.horizontal(|ui| {
                    let (label, tip) = match kind {
                        Kind::Folder => {
                            (t!("chooser.choose_folder"), t!("chooser.choose_folder.tip"))
                        }
                        Kind::File => (t!("chooser.choose_file"), t!("chooser.choose_file.tip")),
                        Kind::Save => (t!("chooser.save_here"), t!("chooser.save_here.tip")),
                    };
                    let accepted =
                        button::primary_button(ui, None, &label, &tip, Size::Normal, ok).clicked();
                    if accepted || (pick.is_some() && ok) {
                        let path = match kind {
                            Kind::Folder => Some(s.dir.clone()),
                            Kind::File => pick
                                .and_then(|i| s.entries.get(i))
                                .map(|e| s.dir.join(&e.name))
                                .or(chosen_file.clone()),
                            Kind::Save => Some(s.dir.join(&save_name)),
                        };
                        confirm = path.map(|p| p.to_string_lossy().into_owned());
                    }
                    if button::secondary_button(
                        ui,
                        None,
                        &t!("chooser.cancel"),
                        &t!("chooser.cancel.tip"),
                        Size::Normal,
                        true,
                    )
                    .clicked()
                    {
                        cancel = true;
                    }
                });
            });
        if let Some(path) = confirm {
            self.chooser.purpose = None;
            return Some(Pick { purpose, path });
        }
        if cancel || !open {
            self.chooser.purpose = None;
        }
        None
    }
}
