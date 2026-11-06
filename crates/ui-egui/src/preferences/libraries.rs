//! Keeping the library list's source switches and the saved names of sources that are off in step.
//!
//! Context: issue #1799. The switches are drawn on the Sources page of the Library tab
//! (`library_workspace/sources.rs`). The four app wide sources are the ones the library list
//! already filters by: Built in, This project, Installed and the Open library. A linked folder
//! that is off keeps a `folder:<path>` name, which this module leaves alone.
//! Local rules: the names of sources that are off are kept, so they stay off after a restart.
//!
//! # Dependencies
//! - crates/ui-egui/src/library_dock.rs (`Dock::hide_*`)
//! - crates/ui-egui/src/library_workspace/sources.rs (`FOLDER_PREFIX`)
//!
//! # Do not
//! - Draw anything here; the Sources page owns the controls.
use crate::App;

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
        let mut now: Vec<String> = SOURCES
            .iter()
            .zip(hidden)
            .filter(|(_, hide)| *hide)
            .map(|(name, _)| (*name).to_owned())
            .collect();
        now.extend(
            self.preferences
                .library_sources_off
                .iter()
                .filter(|n| n.starts_with(crate::library_workspace::sources::FOLDER_PREFIX))
                .cloned(),
        );
        if now != self.preferences.library_sources_off {
            self.preferences.library_sources_off = now;
        }
    }
}
