//! Where this computer keeps the app's own settings files.
//!
//! The recent-designs list, the library source list and the assistant
//! conversations are host knowledge, not design content. Each [`Session`]
//! carries one `UserConfig`, so a test gives its session a private folder and
//! two tests never see each other's files.
//!
//! # Rules
//! - A default or detached config has no folder: reads find nothing and writes
//!   are refused or skipped, so a session built by a test can never touch the
//!   real configuration folder.
//! - Only the application entry points call [`UserConfig::from_environment`].
//!
//! # Dependencies
//! - crates/engine/src/recents.rs
//! - crates/engine/src/library.rs
//! - crates/assistant/src/store.rs
//!
//! # Do not
//! - Read `XDG_CONFIG_HOME`, `APPDATA` or `HOME` anywhere else.
//! - Keep a process-wide override; pass the config instead.
use std::path::{Path, PathBuf};

/// The folder that holds the app's settings files, or none.
///
/// # Rules
/// - `Default` is detached (no folder).
/// - The folder is the `pcbcraft` directory itself; file names are joined below it.
///
/// # Do not
/// - Create the folder before something is written.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UserConfig {
    folder: Option<PathBuf>,
}

impl UserConfig {
    /// A config with no folder: nothing is remembered and nothing is written.
    pub fn detached() -> Self {
        Self::default()
    }

    /// A config rooted at `folder`, such as a test's own temporary directory.
    pub fn in_folder(folder: impl Into<PathBuf>) -> Self {
        Self {
            folder: Some(folder.into()),
        }
    }

    /// The per-user `pcbcraft` folder from `XDG_CONFIG_HOME`, `APPDATA` or
    /// `HOME/.config`, or a detached config when none is set.
    ///
    /// # Do not
    /// - Call this from a test or a library; only a binary's `main` does.
    pub fn from_environment() -> Self {
        let directory = |key: &str| {
            std::env::var(key)
                .ok()
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
        };
        directory("XDG_CONFIG_HOME")
            .or_else(|| directory("APPDATA"))
            .or_else(|| directory("HOME").map(|home| home.join(".config")))
            .map_or_else(Self::detached, |base| {
                Self::in_folder(base.join("pcbcraft"))
            })
    }

    /// The folder, when there is one.
    pub fn folder(&self) -> Option<&Path> {
        self.folder.as_deref()
    }

    /// `name` below the folder, or the plain refusal a caller can show.
    pub fn file(&self, name: &str) -> Result<PathBuf, String> {
        self.folder
            .as_ref()
            .map(|folder| folder.join(name))
            .ok_or_else(|| "no user configuration folder was found".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_detached_config_has_no_files_and_a_folder_config_joins_names() {
        assert!(UserConfig::default().file("x.json").is_err());
        let config = UserConfig::in_folder("/somewhere/pcbcraft");
        assert_eq!(
            config.file("x.json"),
            Ok(PathBuf::from("/somewhere/pcbcraft/x.json"))
        );
    }
}
