//! HTTP client abstraction for network access.
//!
//! The trait is defined here so that both the library crate (which makes
//! distributor API calls) and tests (which replay fixtures) share the same
//! interface. The production implementation lives in pcbcraft-io.

use std::collections::BTreeMap;
use thiserror::Error;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpRequest {
    pub method: String,
    pub url: String,
    pub headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum HttpError {
    #[error("{0}")]
    Failed(String),
}

pub trait HttpClient: Send + Sync {
    fn call(&self, request: &HttpRequest) -> Result<HttpResponse, HttpError>;
}
