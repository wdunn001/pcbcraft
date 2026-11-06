//! Browser implementation of the HttpClient trait for distributor sources.
//!
//! Uses the browser's fetch API to make HTTP requests to Mouser, LCSC, JLCPCB,
//! and other distributor APIs. This enables the Library tab to search and
//! fetch part data in the browser build.
//!
//! # Rules
//! - GET only, no credentials, no cookies
//! - CORS mode with credentials omitted
//! - Errors are reported as plain text, never with header values
//!
//! # Dependencies
//! - crates/library/src/http_client.rs (HttpClient trait)
//! - apps/pcbcraft-web/src/browser_fetch.rs (same fetch pattern)
//!
//! # Do not
//! - Add an origin allowlist here; the user chose the distributor in settings
//! - Log request headers or keys
use pcbcraft_library::http_client::{HttpClient, HttpError, HttpRequest, HttpResponse};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

/// Browser HTTP client using the fetch API.
#[derive(Clone)]
pub struct BrowserHttpClient;

impl BrowserHttpClient {
    pub fn new() -> Self {
        Self
    }
}

/// Contract: crates/library/src/http_client.rs
impl HttpClient for BrowserHttpClient {
    fn call(&self, request: &HttpRequest) -> Result<HttpResponse, HttpError> {
        // In wasm, we can't block and wait for fetch synchronously.
        // The caller must use spawn_local and a callback pattern.
        // This implementation provides the synchronous interface by
        // using a thread-local slot pattern similar to wasm_host.rs.
        todo!("Implement synchronous fetch via thread-local slot pattern")
    }
}
