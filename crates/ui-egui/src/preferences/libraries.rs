//! The Libraries tab: which part sources the library list draws from, and fetching.
//!
//! Context: mockup `prefs-libraries`. The Library sources card lists the sources with an On
//! switch each; the Fetching card holds the 3D model switch. The four sources are the ones
//! the library list already filters by: Built in, This project, Installed and the Open library.
//! Local rules: a switch writes `Preferences::library_sources_off` and the library list's own
//! source filter at once; the list's source chips and this tab always show the same state.
//! The names of sources that are off are kept, so they stay off after a restart.
//!
//! # Dependencies
//! - crates/ui-egui/src/library_dock.rs (`Dock::hide_*`)
//! - crates/ui-egui/src/ui_kit/table.rs, checkbox.rs, card.rs
//!
//! # Do not
//! - Add a source row that no part list reads; a switch must change what the list shows.
use super::note;
use crate::ui_kit::card::card;
use crate::ui_kit::checkbox::checkbox;
use crate::ui_kit::form_row::{FormWidth, form_row};
use crate::ui_kit::table::{self, Column};
use crate::{App, fonts, theme};
use pcbcraft_doc::TEMPLATES;
use pcbcraft_text::t;

/// Stored names of the four sources, in the order of the list.
const SOURCES: [&str; 4] = ["built_in", "project", "installed", "online"];

impl App {
    /// Copy the stored source switches to the library list's filter. Called at start and
    /// after Reset to defaults.
    ///
    /// # Rules
    /// - A name in `library_sources_off` hides that source in the list.
    ///
    /// # Dependencies
    /// - crates/ui-egui/src/library_dock.rs
    ///
    /// # Do not
    /// - Call it every frame; the list's own chips would be undone.
    pub(crate) fn apply_library_source_prefs(&mut self) {
        let off = |name: &str| {
            self.preferences
                .library_sources_off
                .iter()
                .any(|n| n == name)
        };
        let flags = [
            off(SOURCES[0]),
            off(SOURCES[1]),
            off(SOURCES[2]),
            off(SOURCES[3]),
        ];
        self.dock.hide_built_in = flags[0];
        self.dock.hide_project = flags[1];
        self.dock.hide_installed = flags[2];
        self.dock.hide_online = flags[3];
    }

    /// Copy the library list's source filter to the stored switches, once per frame.
    ///
    /// # Rules
    /// - Only writes when something differs, so the preferences are not touched every frame.
    ///
    /// # Dependencies
    /// - crates/ui-egui/src/library_dock.rs
    ///
    /// # Do not
    /// - Read the stored names back into the list from here.
    pub(crate) fn store_library_source_prefs(&mut self) {
        let hidden = [
            self.dock.hide_built_in,
            self.dock.hide_project,
            self.dock.hide_installed,
            self.dock.hide_online,
        ];
        let now: Vec<String> = SOURCES
            .iter()
            .zip(hidden)
            .filter(|(_, hide)| *hide)
            .map(|(name, _)| (*name).to_owned())
            .collect();
        if now != self.preferences.library_sources_off {
            self.preferences.library_sources_off = now;
        }
    }

    /// The Libraries tab.
    ///
    /// # Rules
    /// - Rows read what each source holds now; the Open library says it is fetched on search.
    ///
    /// # Dependencies
    /// - crates/ui-egui/src/ui_kit/table.rs
    ///
    /// # Do not
    /// - Offer Remove for a source the app ships.
    pub(super) fn prefs_libraries(&mut self, ui: &mut egui::Ui) {
        let tk = theme::tokens(ui.ctx());
        let heads = (
            t!("prefs.lib.col.source"),
            t!("prefs.lib.col.kind"),
            t!("prefs.lib.col.contents"),
            t!("prefs.lib.col.on"),
        );
        let cols = [
            Column {
                title: &heads.0,
                width: 240.0,
            },
            Column {
                title: &heads.1,
                width: 220.0,
            },
            Column {
                title: &heads.2,
                width: 300.0,
            },
            Column {
                title: &heads.3,
                width: 80.0,
            },
            Column {
                title: "",
                width: 0.0,
            },
        ];
        let rows = [
            (
                t!("prefs.lib.built_in"),
                t!("prefs.lib.built_in.kind"),
                t!("prefs.lib.built_in.contents", n = TEMPLATES.len()),
                t!("prefs.lib.built_in.tip"),
            ),
            (
                t!("prefs.lib.project"),
                t!("prefs.lib.project.kind"),
                t!(
                    "prefs.lib.project.contents",
                    n = self.project.library.symbols.len()
                ),
                t!("prefs.lib.project.tip"),
            ),
            (
                t!("prefs.lib.installed"),
                t!("prefs.lib.installed.kind"),
                t!("prefs.lib.installed.contents", n = self.dock.total),
                t!("prefs.lib.installed.tip"),
            ),
            (
                t!("prefs.lib.online"),
                t!("prefs.lib.online.kind"),
                t!("prefs.lib.online.contents"),
                t!("prefs.lib.online.tip"),
            ),
        ];
        card(
            ui,
            &t!("prefs.lib.sources"),
            None,
            |_| {},
            |ui| {
                table::head(ui, &cols);
                for (i, (name, kind, contents, tip)) in rows.iter().enumerate() {
                    let mut hidden = [
                        self.dock.hide_built_in,
                        self.dock.hide_project,
                        self.dock.hide_installed,
                        self.dock.hide_online,
                    ][i];
                    let mut on = !hidden;
                    table::row(ui, &cols, |cells| {
                        cells.cell(|ui| {
                            ui.label(fonts::strong(name).color(tk.text));
                        });
                        cells.cell(|ui| {
                            ui.label(fonts::text(fonts::Role::Body, kind).color(tk.muted));
                        });
                        cells.cell(|ui| {
                            ui.label(fonts::mono(contents).color(tk.text));
                        });
                        cells.cell(|ui| {
                            checkbox(ui, &mut on, &t!("prefs.lib.on_for", source = name), tip);
                        });
                        cells.cell(|_| {});
                    });
                    if on == hidden {
                        hidden = !on;
                        match i {
                            0 => self.dock.hide_built_in = hidden,
                            1 => self.dock.hide_project = hidden,
                            2 => self.dock.hide_installed = hidden,
                            _ => self.dock.hide_online = hidden,
                        }
                    }
                }
            },
        );
        self.store_library_source_prefs();
        ui.add_space(theme::SPACE[3]);
        card(
            ui,
            &t!("prefs.lib.fetching"),
            None,
            |_| {},
            |ui| {
                let name = t!("prefs.lib.fetch3d");
                form_row(
                    ui,
                    FormWidth::Page,
                    &name,
                    Some(&t!("prefs.lib.fetch3d.hint")),
                    None,
                    |ui| {
                        checkbox(
                            ui,
                            &mut self.preferences.fetch_open_library_3d,
                            &name,
                            &t!("prefs.lib.fetch3d.tip"),
                        );
                    },
                );
            },
        );
        ui.add_space(theme::SPACE[1]);
        note(ui, t!("prefs.lib.note"));
    }
}
