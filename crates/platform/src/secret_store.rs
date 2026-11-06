//! Secure storage for credentials and tokens.

use thiserror::Error;

#[derive(Error, Debug, PartialEq, Eq)]
pub enum SecretError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("already exists: {0}")]
    AlreadyExists(String),
    #[error("io error: {0}")]
    Io(String),
}

pub trait SecretStore: Send + Sync {
    fn get(&self, name: &str) -> Result<Option<Vec<u8>>, SecretError>;
    fn set(&self, name: &str, value: &[u8]) -> Result<(), SecretError>;
    fn delete(&self, name: &str) -> Result<(), SecretError>;
    fn list(&self) -> Result<Vec<String>, SecretError>;
}
