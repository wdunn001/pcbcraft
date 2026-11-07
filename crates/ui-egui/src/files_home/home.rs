//! The Files tab home: tool bar, project header, Recent projects.
//!
//! Context: the approved mockup `files` (design folder, page layouts section 2). The tool
//! bar carries New, Open, Save, Import, Export and Open sample project. The header names the
//! project and says whether it is saved. The cards live in `shape_cards.rs`, the aside tree in
//! `tree.rs` and the Properties aside in `props.rs`.
//! Local rules: every edit is an engine command through `App::run` with the board checks
//! deferred to the worker (`with_deferred_checks`); a text field writes when it loses focus
//! so a name typed key by key is one undo step; widths, heights and gaps come from `theme`.
//!
//! # Dependencies
//! - crates/ui-egui/src/files_home.rs (state, probes, commands)
//! - crates/ui-egui/src/ui_kit/ (card, rows, button, menu, empty_state)
//!
//! # Do not
//! - Read a file here; the worker in `files_home.rs` does that.
//! - Draw a button, row or card by hand; use the kit.
use super::*;
use crate::ui_kit::card::card_flush;
use crate::ui_kit::empty_state::empty_state;
use crate::ui_kit::menu::{Entry, Item, Menu};
use crate::ui_kit::rows::list_row_high;

/// Height of a recent project row: a name and a line under it.
const RECENT_ROW: f32 = crate::ui_kit::rows::FINDING_ROW_HEIGHT;
/// Narrowest tool bar that still shows a label beside each icon.
const LABELS_FROM: f32 = 720.0;
/// Space reserved at the right of a recent row for its Open button.
const OPEN_BUTTON: f32 = 64.0;

/// What a right click on a recent row can do.
#[derive(Clone, Copy)]
enum RecentAct {
    Pin,
    Remove,
}

/// The file name at the end of a path.
pub(super) fn file_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map_or_else(|| path.to_owned(), |n| n.to_string_lossy().into_owned())
}

impl App {
    /// The tool bar of the Files tab: New, Open, Save | Import, Export | Open sample project.
    ///
    /// # Rules
    /// - Buttons are ghost buttons with an icon and a one to three word label.
    /// - Open sample project sits at the right edge.
    ///
    /// # Dependencies
    /// - crates/ui-egui/src/ui_kit/button.rs
    ///
    /// # Do not
    /// - Add Save as or Close project here; the File menu has them.
    pub(crate) fn files_toolbar(&mut self, ui: &mut egui::Ui) {
        let tokens = theme::tokens(ui.ctx());
        let labels = ui.available_width() >= LABELS_FROM;
        let mut picked = None;
        let tool = |ui: &mut egui::Ui, icon: icons::Icon, label: &str, tip: &str| {
            if labels {
                button::ghost_button(ui, Some(icon), label, tip, Size::Normal, true).clicked()
            } else {
                button::icon_button(ui, icon, label, tip, Size::Normal, true).clicked()
            }
        };
        ui.horizontal_centered(|ui| {
            ui.add_space(Self::PAGE_GUTTER - theme::SPACE[1]);
            ui.spacing_mut().item_spacing.x = theme::SPACE[0];
            for (command, icon, label, tip) in [
                (
                    Command::New,
                    icons::FILE_PLUS,
                    t!("files.tool.new"),
                    t!("files.tool.new.tip"),
                ),
                (
                    Command::Open,
                    icons::FOLDER,
                    t!("files.tool.open"),
                    t!("files.tool.open.tip"),
                ),
                (
                    Command::Save,
                    icons::SAVE,
                    t!("files.tool.save"),
                    t!("files.tool.save.tip"),
                ),
            ] {
                if tool(ui, icon, &label, &tip) {
                    picked = Some(command);
                }
            }
            Self::toolbar_divider(ui, &tokens);
            for (command, icon, label, tip) in [
                (
                    Command::Import,
                    icons::DOWNLOAD,
                    t!("files.tool.import"),
                    t!("files.tool.import.tip"),
                ),
                (
                    Command::Export,
                    icons::UPLOAD,
                    t!("files.tool.export"),
                    t!("files.tool.export.tip"),
                ),
            ] {
                if tool(ui, icon, &label, &tip) {
                    picked = Some(command);
                }
            }
            let sample = (t!("files.tool.sample"), t!("files.tool.sample.tip"));
            if labels {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(theme::SPACE[3] - theme::SPACE[1]);
                    if button::secondary_button(
                        ui,
                        Some(icons::FOLDER),
                        &sample.0,
                        &sample.1,
                        Size::Normal,
                        true,
                    )
                    .clicked()
                    {
                        picked = Some(Command::Sample);
                    }
                });
            } else if tool(ui, icons::FOLDER, &sample.0, &sample.1) {
                picked = Some(Command::Sample);
            }
        });
        if let Some(command) = picked {
            self.files_run(command);
        }
    }

    /// The project name at display size and its state. Saving and opening live in the tool bar.
    pub(super) fn files_header(&mut self, ui: &mut egui::Ui) {
        let tokens = theme::tokens(ui.ctx());
        let name = self.footer_project_name();
        let file = file_name(&self.path);
        let state = if self.project_has_unsaved_changes() {
            t!("files.home.unsaved")
        } else {
            match self.files_home.probes.get(&self.path) {
                Some(p) if p.exists => match p.modified {
                    Some(m) => t!("files.saved_ago", when = self.files_ago(m)),
                    None => t!("files.current.saved"),
                },
                _ => t!("files.home.new"),
            }
        };
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.label(fonts::section_caption(t!("files.home.caption")).color(tokens.muted));
                ui.add(egui::Label::new(fonts::display(&name).color(tokens.text)).truncate());
                ui.add(
                    egui::Label::new(
                        fonts::text(
                            fonts::Role::Body,
                            t!("files.home.subtitle", state = state, file = file),
                        )
                        .color(tokens.muted),
                    )
                    .truncate(),
                );
            });
        });
    }

    /// The Recent projects card: one row per project with its Open button.
    pub(super) fn files_recent_card(&mut self, ui: &mut egui::Ui) {
        let mut open = None;
        let mut act: Option<(String, RecentAct)> = None;
        let mut select = None;
        let list = self.files_listed();
        card_flush(
            ui,
            &t!("files.recent.heading"),
            None,
            |_| {},
            |ui| {
                if list.is_empty() {
                    ui.add_space(theme::SPACE[4]);
                    if empty_state(
                        ui,
                        icons::FOLDER,
                        &t!("files.recent.empty.title"),
                        &t!("files.recent.empty.message"),
                        &t!("files.recent.empty.button"),
                        &t!("files.recent.empty.tip"),
                    )
                    .clicked()
                    {
                        open = Some(String::new());
                    }
                    ui.add_space(theme::SPACE[4]);
                    return;
                }
                for path in &list {
                    self.files_recent_row(ui, path, &mut open, &mut select, &mut act);
                }
                self.files_home.reveal = false;
            },
        );
        if let Some(path) = select {
            self.files_home.selected = Some(path);
        }
        if let Some((path, what)) = act {
            match what {
                RecentAct::Pin => self.files_toggle_pin(&path),
                RecentAct::Remove => self.files_remove(&path),
            }
        }
        match open {
            Some(path) if path.is_empty() => self.show_files(Page::Open),
            Some(path) => {
                self.files_open_path(&path);
            }
            None => {}
        }
    }

    /// One recent project: folder icon, name, when it was opened, and Open.
    fn files_recent_row(
        &mut self,
        ui: &mut egui::Ui,
        path: &str,
        open: &mut Option<String>,
        select: &mut Option<String>,
        act: &mut Option<(String, RecentAct)>,
    ) {
        let tokens = theme::tokens(ui.ctx());
        let selected = self.files_home.selected.as_deref() == Some(path);
        let pinned = self.preferences.pinned_projects.iter().any(|p| p == path);
        let name = project_name(path);
        let missing = self.files_home.probes.get(path).is_some_and(|p| !p.exists);
        let when = self.files_when(path);
        let mut pressed_open = false;
        let response = list_row_high(
            ui,
            RECENT_ROW,
            selected,
            &name,
            &t!("files.row.tip"),
            |ui| {
                let (rect, _) = ui
                    .allocate_exact_size(egui::Vec2::splat(icons::ICON_SIZE), egui::Sense::hover());
                let icon = if pinned { icons::PIN } else { icons::FOLDER };
                (icon.0)(ui.painter(), rect, tokens.muted);
                ui.add_space(theme::SPACE[2]);
                let text_width = (ui.available_width() - OPEN_BUTTON - theme::SPACE[1]).max(0.0);
                ui.allocate_ui_with_layout(
                    egui::vec2(text_width, RECENT_ROW),
                    egui::Layout::top_down(egui::Align::Min).with_main_align(egui::Align::Center),
                    |ui| {
                        ui.set_width(text_width);
                        ui.spacing_mut().item_spacing.y = 0.0;
                        let colour = if missing { tokens.muted } else { tokens.text };
                        ui.add(egui::Label::new(fonts::strong(&name).color(colour)).truncate());
                        let when_colour = if missing { tokens.gold } else { tokens.muted };
                        ui.add(
                            egui::Label::new(fonts::caption(&when).color(when_colour)).truncate(),
                        );
                    },
                );
                let tip = if missing {
                    t!("files.details.open.missing")
                } else {
                    t!("files.details.open.tip")
                };
                pressed_open = button::secondary_button(
                    ui,
                    None,
                    &t!("files.details.open"),
                    &tip,
                    Size::Small,
                    !missing,
                )
                .clicked();
            },
        );
        self.files_home.rows.push(response.id);
        if selected && self.files_home.reveal {
            response.scroll_to_me(None);
        }
        self.keep_list_focus(ui, &response);
        if response.clicked() {
            *select = Some(path.to_owned());
        }
        if (response.double_clicked() || pressed_open) && !missing {
            *open = Some(path.to_owned());
        }
        let picked = crate::ui_kit::menu::context(&response, "files-recent", || {
            let (pin_label, pin_tip) = if pinned {
                (t!("files.row.unpin"), t!("files.row.unpin.tip"))
            } else {
                (t!("files.row.pin"), t!("files.row.pin.tip"))
            };
            Menu::new()
                .group([
                    Entry::from(Item::new(RecentAct::Pin, pin_label, pin_tip).icon(icons::PIN)),
                    Entry::from(
                        Item::new(
                            RecentAct::Remove,
                            t!("files.row.remove"),
                            t!("files.row.remove.tip"),
                        )
                        .icon(icons::CLOSE),
                    ),
                ])
                .entries()
        });
        if let Some(what) = picked {
            *act = Some((path.to_owned(), what));
        }
    }

    /// Run a command from the Properties aside with the board checks on the worker.
    pub(super) fn files_edit(&mut self, command: &str, params: serde_json::Value) {
        self.with_deferred_checks(|app| app.run(command, params));
    }

    /// Record or clear one metadata entry of the project.
    pub(super) fn files_meta(&mut self, key: &str, text: &str) {
        if text.is_empty() {
            if self.project.meta.contains_key(key) {
                self.files_edit("project.clear_meta", json!({"key": key}));
            }
        } else {
            self.files_edit("project.set_meta", json!({"key": key, "value": text}));
        }
    }
}
