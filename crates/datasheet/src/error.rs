use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum OpenError {
    #[error("The file is not a readable document.")]
    Invalid,
    #[error("{0}")]
    Message(String),
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum LoadError {
    #[error("The address is empty.")]
    Empty,
    #[error("Only web addresses and local file paths are supported.")]
    Unsupported,
    #[error("{0}")]
    Failed(String),
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RenderError {
    #[error("That page is out of range.")]
    PageOutOfRange,
    #[error("{0}")]
    Failed(String),
}
