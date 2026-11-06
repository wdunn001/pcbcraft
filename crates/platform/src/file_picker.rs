//! File and folder picker dialogs for the user interface.

use thiserror::Error;

#[derive(Error, Debug, PartialEq, Eq)]
pub enum PickerError {
    #[error("user cancelled")]
    Cancelled,
    #[error("io error: {0}")]
    Io(String),
}

#[derive(Clone, Debug, Default)]
pub struct OpenOptions {
    pub title: Option<String>,
    pub file_types: Option<Vec<String>>,
    pub multi_select: bool,
    pub initial_path: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct SaveOptions {
    pub title: Option<String>,
    pub file_types: Option<Vec<String>>,
    pub initial_path: Option<String>,
}

/// A handle to a user-selected location that the FileStore can read from.
pub trait FileHandle: Send + Sync {
    fn path(&self) -> String;
}

pub trait FilePicker: Send + Sync {
    fn open(&self, options: &OpenOptions) -> Result<Vec<Box<dyn FileHandle>>, PickerError>;
    fn save(&self, options: &SaveOptions) -> Result<Box<dyn FileHandle>, PickerError>;
    fn pick_folder(&self, title: &str) -> Result<Box<dyn FileHandle>, PickerError>;
}
