//! The Files tab home: tool bar, project summary, Project files, Made files, Recent
//! projects and the Properties aside.
//!
//! Context: the approved mockup `files` (design folder, page layouts section 2). The tool
//! bar carries New, Open, Save, Import, Export and Open sample project. The centre says
//! what is in the open project: its name, four stat cards (Parts, Nets, Board, Checks),
//! the Project files table, an empty Made files card that points to Manufacture, and the
//! Recent projects list. The right aside edits the project's name, units, author and
//! board size.
//! Local rules: every edit is an engine command through `App::run` with the board checks
//! deferred to the worker (`with_deferred_checks`); a text field writes when it loses focus
//! so a name typed key by key is one undo step; widths, heights and gaps come from `theme`.
//!
//! # Dependencies
//! - crates/ui-egui/src/files_home.rs (state, probes, commands)
//! - crates/ui-egui/src/ui_kit/ (card, stat_card, rows, button, form_row, section, segmented)
//! - crates/ui-egui/src/project_settings/mod.rs (`text_field`)
//!
//! # Do not
//! - Read a file here; the worker in `files_home.rs` does that.
//! - Draw a button, row or card by hand; use the kit.
use super::*;
use crate::ui_kit::card::{card, card_flush};
use crate::ui_kit::empty_state::empty_state;
use crate::ui_kit::form_row::{FormWidth, form_row};
use crate::ui_kit::menu::{Entry, Item, Menu};
use crate::ui_kit::number_field::{NumberSpec, number_field};
use crate::ui_kit::rows::{list_row_high, table_head, table_row};
use crate::ui_kit::section::section;
use crate::ui_kit::segmented::{Segment, segmented};
use crate::ui_kit::stat_card::stat_card;

/// Share of the width each column of the Project files table takes.
const FILE_COLUMNS: [f32; 4] = [3.0, 2.0, 1.2, 1.8];
/// Height of a recent project row: a name and a line under it.
const RECENT_ROW: f32 = crate::ui_kit::rows::FINDING_ROW_HEIGHT;
/// Narrowest tool bar that still shows a label beside each icon.
const LABELS_FROM: f32 = 720.0;
/// Narrowest centre that still puts all four stat cards in one row.
const STATS_ONE_ROW_FROM: f32 = 720.0;
/// Narrowest centre that still puts two stat cards side by side.
const STATS_TWO_ROWS_FROM: f32 = 400.0;
/// Narrowest centre that still puts the project name beside Save and Open folder.
const HEADER_ROW_FROM: f32 = 560.0;
/// Narrowest centre that still puts Project files beside Recent projects.
const TWO_COLUMNS_FROM: f32 = 640.0;
/// Space reserved at the right of a recent row for its Open button.
const OPEN_BUTTON: f32 = 64.0;
/// How many part references or net names a stat card lists before "+N more".
const STAT_NAMES: usize = 4;
/// The units the Properties aside offers: catalog stem and the value the engine takes.
const UNITS: [(&str, &str); 2] = [("mm", "mm"), ("mil", "mil")];

/// What a right click on a recent row can do.
#[derive(Clone, Copy)]
enum RecentAct {
    Pin,
    Remove,
}

/// The file name at the end of a path.
fn file_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map_or_else(|| path.to_owned(), |n| n.to_string_lossy().into_owned())
}

/// A board size number: whole numbers without decimals, others to one place.
fn dimension(value: f64) -> String {
    if (value - value.round()).abs() < 1e-9 {
        format!("{value:.0}")
    } else {
        format!("{value:.1}")
    }
}

/// Up to [`STAT_NAMES`] names joined with commas, then "+N more".
fn name_list(names: &[String]) -> String {
    if names.is_empty() {
        return t!("files.stat.none");
    }
    let shown = names
        .iter()
        .take(STAT_NAMES)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    if names.len() > STAT_NAMES {
        format!(
            "{shown} {}",
            t!("files.stat.more", count = names.len() - STAT_NAMES)
        )
    } else {
        shown
    }
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

    /// The home page: header, stat cards, then Project files and Made files beside
    /// Recent projects.
    ///
    /// # Rules
    /// - Below [`TWO_COLUMNS_FROM`] the two columns stack.
    ///
    /// # Dependencies
    /// - crates/ui-egui/src/ui_kit/card.rs, stat_card.rs, rows.rs
    pub(crate) fn files_home_page(&mut self, ui: &mut egui::Ui) {
        let gap = theme::SPACE[3];
        ui.spacing_mut().item_spacing = egui::vec2(gap, gap);
        if !self.files_home.error.is_empty() {
            let red = theme::tokens(ui.ctx()).red;
            ui.label(fonts::text(fonts::Role::Body, &self.files_home.error).color(red));
        }
        self.files_header(ui);
        self.files_stats(ui);
        let total = ui.available_width();
        if total < TWO_COLUMNS_FROM {
            self.files_project_card(ui);
            self.files_recent_card(ui);
            self.files_made_card(ui);
            return;
        }
        let right = (total * 0.36).clamp(300.0, 440.0);
        let left = total - gap - right;
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            ui.allocate_ui_with_layout(
                egui::vec2(left, 0.0),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.set_width(left);
                    self.files_project_card(ui);
                    ui.add_space(gap);
                    self.files_made_card(ui);
                },
            );
            ui.allocate_ui_with_layout(
                egui::vec2(right, 0.0),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.set_width(right);
                    self.files_recent_card(ui);
                },
            );
        });
    }

    /// The project name at display size, its state, and Save and Open folder.
    fn files_header(&mut self, ui: &mut egui::Ui) {
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
        let (mut save, mut folder) = (false, false);
        let side_by_side = ui.available_width() >= HEADER_ROW_FROM;
        let buttons = |ui: &mut egui::Ui| -> (bool, bool) {
            let folder = button::secondary_button(
                ui,
                Some(icons::FOLDER),
                &t!("files.home.folder"),
                &t!("files.home.folder.tip"),
                Size::Normal,
                true,
            )
            .clicked();
            let save = button::primary_button(
                ui,
                Some(icons::SAVE),
                &t!("files.tool.save"),
                &t!("files.tool.save.tip"),
                Size::Normal,
                true,
            )
            .clicked();
            (save, folder)
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
            if side_by_side {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                    (save, folder) = buttons(ui);
                });
            }
        });
        if !side_by_side {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                (save, folder) = buttons(ui);
            });
        }
        if save {
            self.save_or_ask();
        }
        if folder {
            self.show_files(Page::Open);
        }
    }

    /// Parts, Nets, Board and Checks as four equal cards.
    fn files_stats(&mut self, ui: &mut egui::Ui) {
        let gap = theme::SPACE[3];
        let total = ui.available_width();
        let per_row = if total >= STATS_ONE_ROW_FROM {
            4
        } else if total >= STATS_TWO_ROWS_FROM {
            2
        } else {
            1
        };
        let width = ((total - (per_row as f32 - 1.0) * gap) / per_row as f32).max(0.0);
        let refs: Vec<String> = self
            .project
            .components
            .iter()
            .map(|c| c.reference.clone())
            .collect();
        let nets: Vec<String> = self.project.nets.iter().map(|n| n.name.clone()).collect();
        let board = &self.project.board;
        let size = format!("{} × {}", dimension(board.width), dimension(board.height));
        let layers = t!(
            "files.stat.board.detail",
            unit = t!("unit.mm"),
            count = self.project.copper_layer_count()
        );
        let (errors, warnings) = self.findings().counts().strip();
        let (checks, checks_detail) = if self.checking() {
            ("…".to_owned(), t!("files.stat.checks.running"))
        } else if errors > 0 {
            (
                errors.to_string(),
                t!("files.stat.checks.errors", warnings = warnings),
            )
        } else if warnings > 0 {
            (warnings.to_string(), t!("files.stat.checks.review"))
        } else {
            ("0".to_owned(), t!("files.stat.checks.clear"))
        };
        let cards = [
            (
                t!("files.stat.parts"),
                refs.len().to_string(),
                name_list(&refs),
                t!("files.stat.parts.tip"),
            ),
            (
                t!("files.stat.nets"),
                nets.len().to_string(),
                name_list(&nets),
                t!("files.stat.nets.tip"),
            ),
            (
                t!("files.stat.board"),
                size,
                layers,
                t!("files.stat.board.tip"),
            ),
            (
                t!("files.stat.checks"),
                checks,
                checks_detail,
                t!("files.stat.checks.tip"),
            ),
        ];
        for row in cards.chunks(per_row) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for (label, value, detail, tip) in row {
                    stat_card(ui, width, label, value, detail, tip);
                }
            });
        }
    }

    /// The Project files card: a table of the files that make up the open project.
    fn files_project_card(&mut self, ui: &mut egui::Ui) {
        let (mut add, mut sources) = (false, false);
        card_flush(
            ui,
            &t!("files.files.title"),
            None,
            |ui| {
                add = button::secondary_button(
                    ui,
                    Some(icons::PLUS),
                    &t!("files.files.add"),
                    &t!("files.files.add.tip"),
                    Size::Small,
                    true,
                )
                .clicked();
            },
            |ui| {
                let tokens = theme::tokens(ui.ctx());
                let heads = [
                    t!("files.files.col.name"),
                    t!("files.files.col.kind"),
                    t!("files.files.col.size"),
                    t!("files.files.col.modified"),
                ];
                let head_refs: Vec<&str> = heads.iter().map(String::as_str).collect();
                table_head(ui, &FILE_COLUMNS, &head_refs);
                let file = file_name(&self.path);
                let probe = self.files_home.probes.get(&self.path);
                let there = probe.is_some_and(|p| p.exists);
                let size = match probe {
                    Some(p) if p.exists && p.bytes > 0 => size_text(p.bytes),
                    _ => t!("files.files.none"),
                };
                let modified = match probe.and_then(|p| p.modified).filter(|_| there) {
                    Some(m) => self.files_ago(m),
                    None => t!("files.files.unsaved"),
                };
                let kind = t!("files.files.kind.design");
                let row = table_row(
                    ui,
                    true,
                    &file,
                    &t!("files.files.design.tip"),
                    &FILE_COLUMNS,
                    |ui, column| match column {
                        0 => {
                            let (rect, _) = ui.allocate_exact_size(
                                egui::Vec2::splat(icons::ICON_SIZE),
                                egui::Sense::hover(),
                            );
                            (icons::FILE.0)(ui.painter(), rect, tokens.muted);
                            ui.add_space(theme::SPACE[1]);
                            ui.add(
                                egui::Label::new(fonts::mono(&file).color(tokens.text)).truncate(),
                            );
                        }
                        1 => {
                            ui.add(
                                egui::Label::new(
                                    fonts::text(fonts::Role::Body, &kind).color(tokens.text),
                                )
                                .truncate(),
                            );
                        }
                        2 => {
                            ui.add(
                                egui::Label::new(fonts::mono(&size).color(tokens.muted)).truncate(),
                            );
                        }
                        _ => {
                            ui.add(
                                egui::Label::new(
                                    fonts::text(fonts::Role::Body, &modified).color(tokens.muted),
                                )
                                .truncate(),
                            );
                        }
                    },
                );
                if row.clicked() {
                    self.change_tab(Tab::Schematic);
                }
                egui::Frame::NONE
                    .inner_margin(egui::Margin::symmetric(
                        theme::SPACE[1] as i8,
                        theme::SPACE[0] as i8,
                    ))
                    .show(ui, |ui| {
                        sources = button::ghost_button(
                            ui,
                            Some(icons::LAYERS),
                            &t!("files.files.sources"),
                            &t!("files.files.sources.tip"),
                            Size::Small,
                            true,
                        )
                        .clicked();
                    });
            },
        );
        if add {
            self.show_files(Page::Import);
        }
        if sources {
            self.open_library_sources();
        }
    }

    /// The Made files card: nothing is made yet, and it points to Manufacture.
    fn files_made_card(&mut self, ui: &mut egui::Ui) {
        let mut go = false;
        card(
            ui,
            &t!("files.made.title"),
            None,
            |_| {},
            |ui| {
                ui.add_space(theme::SPACE[4]);
                go = empty_state(
                    ui,
                    icons::UPLOAD,
                    &t!("files.made.empty.title"),
                    &t!("files.made.empty.message"),
                    &t!("files.made.empty.button"),
                    &t!("files.made.empty.tip"),
                )
                .clicked();
                ui.add_space(theme::SPACE[4]);
            },
        );
        if go {
            self.change_tab(Tab::PlotFiles);
        }
    }

    /// The Recent projects card: one row per project with its Open button.
    fn files_recent_card(&mut self, ui: &mut egui::Ui) {
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
    fn files_edit(&mut self, command: &str, params: serde_json::Value) {
        self.with_deferred_checks(|app| app.run(command, params));
    }

    /// Record or clear one metadata entry of the project.
    fn files_meta(&mut self, key: &str, text: &str) {
        if text.is_empty() {
            if self.project.meta.contains_key(key) {
                self.files_edit("project.clear_meta", json!({"key": key}));
            }
        } else {
            self.files_edit("project.set_meta", json!({"key": key, "value": text}));
        }
    }

    /// The right aside of the Files tab: the project's Properties.
    ///
    /// # Rules
    /// - Name and Author write when the field loses focus; Units and the board size write at
    ///   once.
    /// - Layers is read only; the stackup is edited in Project settings.
    ///
    /// # Dependencies
    /// - crates/ui-egui/src/ui_kit/section.rs, form_row.rs, number_field.rs, segmented.rs
    ///
    /// # Do not
    /// - Offer a second place to edit the stackup.
    pub(crate) fn files_properties(&mut self, ui: &mut egui::Ui) {
        const ROW: FormWidth = FormWidth::Aside;
        let tokens = theme::tokens(ui.ctx());
        self.properties_header(ui, &t!("files.props.title"), &file_name(&self.path));
        let project_name_now = self.project.name.clone();
        let author = self.project.meta.get("author").cloned().unwrap_or_default();
        let unit = self.project.unit.clone();
        let (w, h) = (self.project.board.width, self.project.board.height);
        let layers = self.project.copper_layer_count();
        section(
            ui,
            "files-general",
            &t!("files.props.general"),
            &t!("files.props.general.tip"),
            |ui| {
                let label = t!("files.props.name");
                if let Some(text) = form_row(ui, ROW, &label, None, None, |ui| {
                    let room = ui.available_width();
                    crate::project_settings::text_field(
                        ui,
                        "files-name",
                        &project_name_now,
                        room,
                        &t!("files.props.name.field"),
                        &t!("files.props.name.tip"),
                    )
                }) && !text.trim().is_empty()
                {
                    self.files_edit("project.rename", json!({"name": text.trim()}));
                }
                let labels: Vec<String> = UNITS
                    .iter()
                    .map(|(stem, _)| {
                        pcbcraft_text::lookup(&format!("files.props.unit.{stem}"), &[])
                    })
                    .collect();
                let tips: Vec<String> = UNITS
                    .iter()
                    .map(|(stem, _)| {
                        pcbcraft_text::lookup(&format!("files.props.unit.{stem}.tip"), &[])
                    })
                    .collect();
                let items: Vec<Segment<'_>> = labels
                    .iter()
                    .zip(&tips)
                    .map(|(label, tip)| Segment::text(label, tip))
                    .collect();
                let mut at = UNITS.iter().position(|(_, v)| *v == unit).unwrap_or(0);
                let before = at;
                form_row(ui, ROW, &t!("files.props.units"), None, None, |ui| {
                    segmented(ui, "files-units", &mut at, &items);
                });
                if at != before {
                    self.files_edit("project.set_unit", json!({"unit": UNITS[at].1}));
                }
                let label = t!("files.props.author");
                if let Some(text) = form_row(ui, ROW, &label, None, None, |ui| {
                    let room = ui.available_width();
                    crate::project_settings::text_field(
                        ui,
                        "files-author",
                        &author,
                        room,
                        &t!("files.props.author.field"),
                        &t!("files.props.author.tip"),
                    )
                }) {
                    self.files_meta("author", text.trim());
                }
            },
        );
        section(
            ui,
            "files-board",
            &t!("files.props.board"),
            &t!("files.props.board.tip"),
            |ui| {
                let spec = NumberSpec {
                    min: 1.0,
                    max: 2000.0,
                    step: 0.5,
                    places: 2,
                    width: 110.0,
                    ..NumberSpec::default()
                };
                let (mut nw, mut nh) = (w, h);
                let name = t!("files.props.width");
                let cw = form_row(ui, ROW, &name, None, None, |ui| {
                    number_field(
                        ui,
                        "files-board-w",
                        &mut nw,
                        &spec,
                        &t!("files.props.width.field"),
                        &t!("files.props.width.tip"),
                    )
                    .changed()
                });
                let name = t!("files.props.height");
                let ch = form_row(ui, ROW, &name, None, None, |ui| {
                    number_field(
                        ui,
                        "files-board-h",
                        &mut nh,
                        &spec,
                        &t!("files.props.height.field"),
                        &t!("files.props.height.tip"),
                    )
                    .changed()
                });
                if cw || ch {
                    self.width = nw;
                    self.height = nh;
                    self.files_edit("board.resize", json!({"width": nw, "height": nh}));
                }
                form_row(ui, ROW, &t!("files.props.layers"), None, None, |ui| {
                    ui.label(fonts::mono(layers.to_string()).color(tokens.text))
                        .on_hover_text(t!("files.props.layers.tip"));
                });
            },
        );
        ui.add_space(theme::SPACE[1]);
        if button::secondary_button(
            ui,
            Some(icons::GEAR),
            &t!("files.props.settings"),
            &t!("files.props.settings.tip"),
            Size::Normal,
            true,
        )
        .clicked()
        {
            self.open_project_settings(crate::project_settings::TAB_BOARD);
        }
    }
}
