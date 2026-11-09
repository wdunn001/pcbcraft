//! The Sources page of the Library tab: where parts, symbols and footprints come from.
//!
//! Context: issue #1799 and the approved mockup `library-sources`. This page replaces the old
//! Libraries page, the Libraries tab of Preferences and the Library sources button of the Files
//! page. The table lists This project, the built in parts, the installed libraries, every
//! folder the person linked and the online library. Each row has an On switch, a kind, its
//! contents and its state. A linked folder also has Update and Remove.
//!
//! Local rules: a switch writes the same names the library list already reads
//! (`Preferences::library_sources_off`): `built_in`, `project`, `installed`, `online`, and
//! `folder:<path>` for one linked folder. A name in that list is off. The list is saved with the
//! other preferences. Adding and removing a folder go through the existing `library.add_folder`
//! and `library.remove_folder` reads, run on a worker, exactly as the Vendors page does.
//!
//! Not built, because no code supports it yet: git repositories, vendor keys, single files and
//! a per source read time. The browser build has no folder paths; there the Add folder button
//! is off and says so.
//!
//! # Dependencies
//! - crates/ui-egui/src/library.rs (`Browser::request`, `Browser::poll`)
//! - crates/ui-egui/src/preferences/libraries.rs (`store_library_source_prefs`)
//! - crates/ui-egui/src/library_dock.rs (`Dock::hide_*`)
//! - crates/ui-egui/src/ui_kit/table.rs, checkbox.rs, card.rs, button.rs
//!
//! # Do not
//! - Add a row that no part list reads; a switch must change what the list shows.
//! - Offer Remove for a source the app ships.
//! - Call a browser API; the browser provider is separate work.
use super::*;
use crate::a11y::Named;
use crate::ui_kit::button::ghost_button;
use crate::ui_kit::card::{card, card_flush};
use crate::ui_kit::checkbox::checkbox;
use crate::ui_kit::table::{self, Column};
use pcbcraft_doc::TEMPLATES;

/// The prefix of the name a switched off linked folder keeps in `library_sources_off`.
pub(crate) const FOLDER_PREFIX: &str = "folder:";

/// One row of the sources table.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SourceRow {
    /// Stable key: `project`, `built_in`, `installed`, `online` or `folder:<path>`.
    pub(crate) key: String,
    /// What the person calls it.
    pub(crate) name: String,
    /// Plain kind, such as Folder.
    pub(crate) kind: String,
    /// Where it lives, in words or as a path.
    pub(crate) location: String,
    /// What it holds, in words.
    pub(crate) contents: String,
    /// On, Off, Ready or No libraries found.
    pub(crate) state: String,
    /// True when the source feeds the part list.
    pub(crate) on: bool,
    /// The folder path of a linked folder; only such a row can be removed.
    pub(crate) folder: Option<String>,
    /// Symbols (project) or symbol libraries (folders and installed).
    pub(crate) symbols: usize,
    /// Footprints (project) or footprint libraries (folders and installed).
    pub(crate) footprints: usize,
    /// Tooltip of the On switch.
    pub(crate) tip: String,
}

impl App {
    /// Whether a name is in the saved list of switched off sources.
    pub(crate) fn library_source_is_off(&self, name: &str) -> bool {
        self.preferences
            .library_sources_off
            .iter()
            .any(|n| n == name)
    }

    /// Whether a part at `path` sits inside a linked folder that is switched off.
    ///
    /// # Rules
    /// - Compares whole path parts, so `/a/b` does not hide `/a/bc`.
    ///
    /// # Dependencies
    /// - crates/ui-egui/src/library_dock.rs (`dock_gather_cards`)
    pub(crate) fn library_folder_off(&self, path: &str) -> bool {
        self.preferences
            .library_sources_off
            .iter()
            .filter_map(|n| n.strip_prefix(FOLDER_PREFIX))
            .any(|folder| std::path::Path::new(path).starts_with(folder))
    }

    /// Switch a source on or off and save the choice with the other preferences.
    ///
    /// # Rules
    /// - The four app wide sources move the library list's own chips; a folder adds or drops its
    ///   `folder:<path>` name.
    pub(crate) fn library_source_set(&mut self, key: &str, on: bool) {
        match key {
            "built_in" => self.dock.hide_built_in = !on,
            "project" => self.dock.hide_project = !on,
            "installed" => self.dock.hide_installed = !on,
            "online" => self.dock.hide_online = !on,
            folder => {
                let list = &mut self.preferences.library_sources_off;
                list.retain(|n| n != folder);
                if !on {
                    list.push(folder.to_owned());
                }
            }
        }
        self.store_library_source_prefs();
    }

    /// Read every library folder again, so new files show up in the list.
    pub(crate) fn library_sources_update(&mut self) {
        self.library.catalog = None;
        self.library.sources = None;
        self.library.results = None;
        self.library.preview = None;
    }

    /// Open the Library tab on its Sources page.
    ///
    /// # Rules
    /// - Leaves the Machines view, so the page is the one showing.
    /// - A story-only deep link: the View menu item Library sources opens it too.
    pub(crate) fn open_library_sources(&mut self) {
        self.set_library_left_tab("library");
        self.libws.page = Page::Sources;
        if self.tab != Tab::Library {
            self.change_tab(Tab::Library);
        }
    }

    /// Open the Library tab on its Collections page.
    pub(crate) fn open_library_collections(&mut self) {
        self.set_library_left_tab("library");
        self.libws.page = Page::Collections;
        if self.tab != Tab::Library {
            self.change_tab(Tab::Library);
        }
    }

    /// The rows of the sources table, read from what the app holds now.
    ///
    /// # Rules
    /// - The row order is This project, Built in parts, Installed libraries, linked folders,
    ///   Open library.
    /// - A folder row exists only when the folder listing names it; the browser build lists none.
    pub(crate) fn library_source_rows(&self) -> Vec<SourceRow> {
        let state_for = |on: bool, ready: String| {
            if on {
                ready
            } else {
                t!("libws.sources.state.off")
            }
        };
        let listing: Vec<(String, String, String, usize, usize)> = self
            .library
            .sources
            .as_ref()
            .and_then(|v| v.get("sources"))
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .map(|s| {
                        let text = |k: &str| {
                            s.get(k)
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_owned()
                        };
                        let count = |k: &str| {
                            s.get(k)
                                .and_then(Value::as_u64)
                                .and_then(|n| usize::try_from(n).ok())
                                .unwrap_or(0)
                        };
                        (
                            text("name"),
                            text("kind"),
                            text("path"),
                            count("symbol_libraries"),
                            count("footprint_libraries"),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        let mut rows = Vec::new();
        let project_on = !self.dock.hide_project;
        rows.push(SourceRow {
            key: "project".into(),
            name: t!("libws.sources.row.project"),
            kind: t!("libws.sources.row.project.kind"),
            location: t!("libws.sources.row.project.location"),
            contents: t!(
                "libws.sources.row.project.contents",
                n = self.project.library.symbols.len()
            ),
            state: state_for(project_on, t!("libws.sources.state.on")),
            on: project_on,
            folder: None,
            symbols: self.project.library.symbols.len(),
            footprints: self.project.library.footprints.len(),
            tip: t!("libws.sources.row.project.tip"),
        });
        let built_in_on = !self.dock.hide_built_in;
        rows.push(SourceRow {
            key: "built_in".into(),
            name: t!("libws.sources.row.built_in"),
            kind: t!("libws.sources.row.built_in.kind"),
            location: t!("libws.sources.row.built_in.location"),
            contents: t!("libws.sources.row.built_in.contents", n = TEMPLATES.len()),
            state: state_for(built_in_on, t!("libws.sources.state.on")),
            on: built_in_on,
            folder: None,
            symbols: TEMPLATES.len(),
            footprints: 0,
            tip: t!("libws.sources.row.built_in.tip"),
        });
        let installed: Vec<&(String, String, String, usize, usize)> =
            listing.iter().filter(|s| s.1 != "added").collect();
        let installed_on = !self.dock.hide_installed;
        let installed_libraries: usize = installed.iter().map(|s| s.3 + s.4).sum();
        rows.push(SourceRow {
            key: "installed".into(),
            name: t!("libws.sources.row.installed"),
            kind: t!("libws.sources.row.installed.kind"),
            location: installed
                .first()
                .map_or_else(|| t!("libws.sources.row.installed.none"), |s| s.2.clone()),
            contents: t!(
                "libws.sources.row.installed.contents",
                n = installed_libraries
            ),
            state: state_for(installed_on, t!("libws.sources.state.ready")),
            on: installed_on,
            folder: None,
            symbols: installed.iter().map(|s| s.3).sum(),
            footprints: installed.iter().map(|s| s.4).sum(),
            tip: t!("libws.sources.row.installed.tip"),
        });
        for (name, _, path, symbols, footprints) in listing.iter().filter(|s| s.1 == "added") {
            let key = format!("{FOLDER_PREFIX}{path}");
            let on = !self.library_source_is_off(&key);
            let ready = if symbols + footprints > 0 {
                t!("libws.sources.state.ready")
            } else {
                t!("libws.sources.state.empty")
            };
            rows.push(SourceRow {
                key,
                name: name.clone(),
                kind: t!("libws.sources.row.folder.kind"),
                location: path.clone(),
                contents: t!(
                    "libws.sources.row.folder.contents",
                    symbols = symbols,
                    footprints = footprints
                ),
                state: state_for(on, ready),
                on,
                folder: Some(path.clone()),
                symbols: *symbols,
                footprints: *footprints,
                tip: t!("libws.sources.row.folder.tip", name = name),
            });
        }
        let online_on = !self.dock.hide_online;
        rows.push(SourceRow {
            key: "online".into(),
            name: t!("libws.sources.row.online"),
            kind: t!("libws.sources.row.online.kind"),
            location: t!("libws.sources.row.online.location"),
            contents: t!("libws.sources.row.online.contents"),
            state: state_for(online_on, t!("libws.sources.state.ready")),
            on: online_on,
            folder: None,
            symbols: 0,
            footprints: 0,
            tip: t!("libws.sources.row.online.tip"),
        });
        rows
    }

    /// The row the Properties aside shows: the picked one, else the first.
    fn library_source_selected(&self, rows: &[SourceRow]) -> Option<SourceRow> {
        self.libws
            .sources_selected
            .as_ref()
            .and_then(|key| rows.iter().find(|r| &r.key == key))
            .or_else(|| rows.first())
            .cloned()
    }

    /// Ask for the folder listing once and take in a finished read.
    fn library_sources_poll(&mut self, ctx: &egui::Context) {
        self.library.poll();
        if self.library.sources.is_none() && self.library.pending.is_none() {
            self.library.request(ctx, "library.sources", json!({}));
        }
    }

    /// Take a folder away from the list: Remove on a row or in Properties.
    fn library_source_remove(&mut self, ctx: &egui::Context, row: &SourceRow) {
        let Some(path) = row.folder.clone() else {
            return;
        };
        self.preferences
            .library_sources_off
            .retain(|n| n != &row.key);
        if self.libws.sources_selected.as_deref() == Some(row.key.as_str()) {
            self.libws.sources_selected = None;
        }
        self.library
            .request(ctx, "library.remove_folder", json!({"path": path}));
    }

    /// The tool bar of the Sources page: Add source and Update all.
    pub(crate) fn library_sources_toolbar(&mut self, ui: &mut egui::Ui) {
        if primary_button(
            ui,
            Some(crate::ui_kit::icons::PLUS),
            &t!("libws.sources.add_source"),
            &t!("libws.sources.add_source.tip"),
            Size::Normal,
            true,
        )
        .clicked()
        {
            self.libws.sources_focus_add = true;
        }
        if secondary_button(
            ui,
            None,
            &t!("libws.sources.update_all"),
            &t!("libws.sources.update_all.tip"),
            Size::Normal,
            self.library.pending.is_none(),
        )
        .clicked()
        {
            self.library_sources_update();
        }
    }

    /// The Sources page.
    ///
    /// # Rules
    /// - A switch, Update and Remove act at once; nothing waits for a Done.
    /// - The typed folder lives in `Browser::new_source`, so it survives a redraw.
    pub(crate) fn library_sources_page(&mut self, ui: &mut egui::Ui) {
        let t = theme::tokens(ui.ctx());
        self.library_sources_poll(ui.ctx());
        let rows = self.library_source_rows();
        let selected = self.library_source_selected(&rows).map(|r| r.key);
        ui.label(fonts::display(t!("libws.sources.page_title")).color(t.text));
        ui.label(fonts::text(fonts::Role::Body, t!("libws.sources.page_caption")).color(t.muted));
        ui.add_space(theme::SPACE[2]);
        let heads = [
            t!("libws.sources.col.on"),
            t!("libws.sources.col.source"),
            t!("libws.sources.col.kind"),
            t!("libws.sources.col.contents"),
            t!("libws.sources.col.state"),
        ];
        // Narrow centres drop the Kind column; Properties on the right still names the kind.
        let total = ui.available_width();
        let wide = total >= 820.0;
        let state_w = if wide { 120.0 } else { 80.0 };
        let kind_w = if wide { 140.0 } else { 0.0 };
        let rest = (total - 48.0 - 150.0 - state_w - kind_w).max(160.0);
        let mut cols = vec![
            Column {
                title: &heads[0],
                width: 48.0,
            },
            Column {
                title: &heads[1],
                width: rest * 0.45,
            },
        ];
        if wide {
            cols.push(Column {
                title: &heads[2],
                width: kind_w,
            });
        }
        cols.push(Column {
            title: &heads[3],
            width: rest * 0.55,
        });
        cols.push(Column {
            title: &heads[4],
            width: state_w,
        });
        cols.push(Column {
            title: "",
            width: 0.0,
        });
        let mut toggle: Option<(String, bool)> = None;
        let mut pick: Option<String> = None;
        let mut update = false;
        let mut remove: Option<SourceRow> = None;
        let idle = self.library.pending.is_none();
        card_flush(
            ui,
            &t!("libws.sources.table_title"),
            Some(&t!("libws.sources.table_caption")),
            |_| {},
            |ui| {
                table::head(ui, &cols);
                for row in &rows {
                    let mut on = row.on;
                    let chosen = selected.as_deref() == Some(row.key.as_str());
                    table::row(ui, &cols, |cells| {
                        cells.cell(|ui| {
                            ui.add_space(12.0);
                            checkbox(
                                ui,
                                &mut on,
                                &t!("libws.sources.on_for", source = row.name),
                                &row.tip,
                            );
                        });
                        cells.cell(|ui| {
                            let shown = ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing.y = 0.0;
                                let colour = if chosen { t.copper } else { t.text };
                                ui.add(
                                    egui::Label::new(fonts::strong(&row.name).color(colour))
                                        .truncate(),
                                );
                                ui.add(
                                    egui::Label::new(fonts::caption(&row.location).color(t.muted))
                                        .truncate(),
                                );
                            });
                            let click = ui
                                .interact(
                                    shown.response.rect,
                                    ui.id().with(("library-source", &row.key)),
                                    egui::Sense::click(),
                                )
                                .on_hover_cursor(egui::CursorIcon::PointingHand)
                                .on_hover_text(t!("libws.sources.select.tip", name = row.name));
                            click.widget_info(|| {
                                egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &row.name)
                            });
                            if click.clicked() {
                                pick = Some(row.key.clone());
                            }
                        });
                        if wide {
                            cells.cell(|ui| {
                                ui.add(
                                    egui::Label::new(
                                        fonts::text(fonts::Role::Body, &row.kind).color(t.muted),
                                    )
                                    .truncate(),
                                );
                            });
                        }
                        cells.cell(|ui| {
                            ui.add(
                                egui::Label::new(fonts::mono(&row.contents).color(t.text))
                                    .truncate(),
                            );
                        });
                        cells.cell(|ui| {
                            ui.add(
                                egui::Label::new(
                                    fonts::text(fonts::Role::Body, &row.state).color(t.muted),
                                )
                                .truncate(),
                            );
                        });
                        cells.cell(|ui| {
                            if row.folder.is_some() {
                                if ghost_button(
                                    ui,
                                    None,
                                    &t!("libws.sources.remove"),
                                    &t!("libws.sources.remove.tip", name = row.name),
                                    Size::Small,
                                    idle,
                                )
                                .clicked()
                                {
                                    remove = Some(row.clone());
                                }
                                if ghost_button(
                                    ui,
                                    None,
                                    &t!("libws.sources.update"),
                                    &t!("libws.sources.update.tip", name = row.name),
                                    Size::Small,
                                    idle,
                                )
                                .clicked()
                                {
                                    update = true;
                                }
                            }
                        });
                    });
                    if on != row.on {
                        toggle = Some((row.key.clone(), on));
                    }
                }
            },
        );
        ui.add_space(theme::SPACE[3]);
        self.library_sources_add_card(ui);
        ui.add_space(theme::SPACE[3]);
        card(
            ui,
            &t!("libws.sources.fetch.title"),
            None,
            |_| {},
            |ui| {
                let name = t!("libws.sources.fetch3d");
                form_row(
                    ui,
                    FormWidth::Page,
                    &name,
                    Some(&t!("libws.sources.fetch3d.hint")),
                    None,
                    |ui| {
                        crate::ui_kit::checkbox::checkbox(
                            ui,
                            &mut self.preferences.fetch_open_library_3d,
                            &name,
                            &t!("libws.sources.fetch3d.tip"),
                        );
                    },
                );
            },
        );
        ui.add_space(theme::SPACE[1]);
        ui.label(fonts::caption(t!("libws.sources.note")).color(t.muted));
        if let Some((key, on)) = toggle {
            self.library_source_set(&key, on);
        }
        if let Some(key) = pick {
            self.libws.sources_selected = Some(key);
        }
        if update {
            self.library_sources_update();
        }
        if let Some(row) = remove {
            self.library_source_remove(ui.ctx(), &row);
        }
    }

    /// The Add a source card: a folder on this computer.
    ///
    /// # Rules
    /// - Add folder is off in the browser build, which has no folder paths, and says why.
    fn library_sources_add_card(&mut self, ui: &mut egui::Ui) {
        let t = theme::tokens(ui.ctx());
        let web = self.host.takes_bytes();
        card(
            ui,
            &t!("libws.sources.add.title"),
            None,
            |_| {},
            |ui| {
                ui.label(fonts::strong(t!("libws.sources.add.folder")).color(t.text));
                ui.label(fonts::caption(t!("libws.sources.add.folder.caption")).color(t.muted));
                ui.add_space(theme::SPACE[1]);
                ui.horizontal(|ui| {
                    if web {
                        if secondary_button(
                            ui,
                            None,
                            &t!("libws.sources.load_file"),
                            &t!("libws.sources.load_file.tip"),
                            Size::Small,
                            true,
                        )
                        .clicked()
                        {
                            self.library_load_start(ui.ctx());
                        }
                    } else if secondary_button(
                        ui,
                        None,
                        &t!("libws.sources.choose"),
                        &t!("libws.sources.choose.tip"),
                        Size::Small,
                        self.library.pending.is_none(),
                    )
                    .clicked()
                    {
                        self.open_chooser(crate::path_chooser::Purpose::AddSource);
                    }
                    let hint = t!("library.collection_browser.folder_to_add_as_a");
                    let field = ui
                        .add_enabled(
                            !web,
                            crate::targets::text_field(&mut self.library.new_source)
                                .hint_text(&hint)
                                .desired_width(320.0),
                        )
                        .named(&hint)
                        .on_hover_text(t!("library.collection_browser.folder_to_add_as_a.tip"));
                    if self.libws.sources_focus_add {
                        field.request_focus();
                        self.libws.sources_focus_add = false;
                    }
                    let ready = !web
                        && !self.library.new_source.trim().is_empty()
                        && self.library.pending.is_none();
                    let tip = if web {
                        t!("libws.sources.add.web_tip")
                    } else {
                        t!("library.collection_browser.add_folder.tip")
                    };
                    let pressed = secondary_button(
                        ui,
                        None,
                        &t!("libws.sources.add"),
                        &tip,
                        Size::Small,
                        ready,
                    )
                    .clicked();
                    let entered = field.lost_focus()
                        && ui.input(|i| i.key_pressed(egui::Key::Enter))
                        && ready;
                    if pressed || entered {
                        let path = self.library.new_source.trim().to_owned();
                        self.library
                            .request(ui.ctx(), "library.add_folder", json!({"path": path}));
                    }
                });
                if !self.library.notice.is_empty() {
                    ui.label(fonts::caption(&self.library.notice).color(t.muted));
                }
                if !self.library.error.is_empty() {
                    ui.label(fonts::caption(&self.library.error).color(t.red));
                }
            },
        );
    }

    /// The right aside of the Sources page: the picked source.
    ///
    /// # Rules
    /// - Read again and Remove act like the row buttons; Remove shows for linked folders only.
    pub(crate) fn library_sources_properties(&mut self, ui: &mut egui::Ui) {
        let t = theme::tokens(ui.ctx());
        let rows = self.library_source_rows();
        let Some(row) = self.library_source_selected(&rows) else {
            return;
        };
        let idle = self.library.pending.is_none();
        let mut update = false;
        let mut remove = false;
        egui::Frame::NONE
            .inner_margin(egui::Margin::symmetric(12, 8))
            .show(ui, |ui| {
                ui.label(fonts::title(&row.name).color(t.text));
                ui.label(fonts::caption(&row.kind).color(t.muted));
                ui.add_space(8.0);
                let value = |ui: &mut egui::Ui, label: String, text: String| {
                    form_row(ui, FormWidth::Aside, &label, None, None, |ui| {
                        ui.add(
                            egui::Label::new(fonts::text(fonts::Role::Body, text).color(t.muted))
                                .truncate(),
                        );
                    });
                };
                value(ui, t!("libws.sources.props.name"), row.name.clone());
                value(ui, t!("libws.sources.props.location"), row.location.clone());
                value(ui, t!("libws.sources.props.state"), row.state.clone());
                if row.key != "online" {
                    value(
                        ui,
                        t!("libws.sources.props.symbols"),
                        row.symbols.to_string(),
                    );
                    if row.key != "built_in" {
                        value(
                            ui,
                            t!("libws.sources.props.footprints"),
                            row.footprints.to_string(),
                        );
                    }
                }
                ui.add_space(8.0);
                if row.folder.is_some() {
                    ui.horizontal(|ui| {
                        update = secondary_button(
                            ui,
                            None,
                            &t!("libws.sources.props.read_again"),
                            &t!("libws.sources.update.tip", name = row.name),
                            Size::Normal,
                            idle,
                        )
                        .clicked();
                        remove = secondary_button(
                            ui,
                            None,
                            &t!("libws.sources.props.remove"),
                            &t!("libws.sources.remove.tip", name = row.name),
                            Size::Normal,
                            idle,
                        )
                        .clicked();
                    });
                }
            });
        if update {
            self.library_sources_update();
        }
        if remove {
            self.library_source_remove(ui.ctx(), &row);
        }
    }
}
