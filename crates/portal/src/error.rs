//! Errors returned by portal storage traits and request validation.

use thiserror::Error;

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum PortalError {
    #[error("{0}")]
    Invalid(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("forbidden: {0}")]
    Forbidden(String),
    #[error("rate limited for {retry_after_secs} seconds")]
    RateLimited { retry_after_secs: u64 },
    #[error("{0}")]
    Storage(String),
}
