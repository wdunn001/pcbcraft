//! HTTP client operations behind a trait for wasm and native builds.
//!
//! Portable code calls this trait; the native provider wraps ureq and the
//! browser provider uses fetch. Tests use RecordedHttp to replay fixtures.
//!
//! # Rules
/// - URLs must be absolute http:// or https://.
/// - Headers are case-insensitive strings.
/// - Timeouts are set by the caller, not the trait.
///
/// # Dependencies
/// - `crates/clock` (for timeouts)
///
/// # Do not
/// - Call ureq or fetch directly in portable code; use this trait instead.
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};

/// One outbound HTTP call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpRequest {
    /// The HTTP method (GET, POST, etc.).
    pub method: String,
    /// The absolute URL.
    pub url: String,
    /// Request headers.
    pub headers: BTreeMap<String, String>,
    /// Request body bytes.
    pub body: Vec<u8>,
}

/// Answer from the HTTP transport.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpResponse {
    /// The status code (200, 404, etc.).
    pub status: u16,
    /// Response headers.
    pub headers: BTreeMap<String, String>,
    /// Response body bytes.
    pub body: Vec<u8>,
}

/// Why an HTTP call did not return a response.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum HttpError {
    /// The call failed for a reason described in the string.
    #[error("{0}")]
    Failed(String),
}

/// Makes HTTP calls on native and wasm builds.
///
/// Portable code holds a Box<dyn HttpClient> and calls its methods. The
/// concrete implementation is chosen at startup based on the target platform.
///
/// # Rules
/// - All methods are Send + Sync so the client can be shared.
/// - URLs must be absolute http:// or https://.
///
/// # Dependencies
/// - `crates/clock` (for timeouts)
///
/// # Do not
/// - Call ureq or fetch directly in portable code; use this trait instead.
pub trait HttpClient: Send + Sync {
    /// Make one HTTP call and return the response.
    fn call(&self, request: &HttpRequest) -> Result<HttpResponse, HttpError>;
}

/// Replays recorded HTTP answers in order for tests.
pub struct RecordedHttp {
    script: Vec<(HttpRequest, HttpResponse)>,
    step: AtomicUsize,
}

impl RecordedHttp {
    /// Create a new recorded client with the given script.
    pub fn new(script: Vec<(HttpRequest, HttpResponse)>) -> Self {
        Self {
            script,
            step: AtomicUsize::new(0),
        }
    }

    /// How many calls have been made so far.
    pub fn calls_made(&self) -> usize {
        self.step.load(Ordering::SeqCst)
    }
}

impl HttpClient for RecordedHttp {
    fn call(&self, request: &HttpRequest) -> Result<HttpResponse, HttpError> {
        let i = self.step.fetch_add(1, Ordering::SeqCst);
        let (expected, response) = self
            .script
            .get(i)
            .ok_or_else(|| HttpError::Failed(format!("no recorded answer for step {i}")))?;
        if expected.method != request.method || expected.url != request.url {
            return Err(HttpError::Failed(format!(
                "step {i} expected {} {}, got {} {}",
                expected.method, expected.url, request.method, request.url
            )));
        }
        Ok(response.clone())
    }
}

/// Helper to build a GET request.
pub fn get(url: &str) -> HttpRequest {
    HttpRequest {
        method: "GET".into(),
        url: url.into(),
        headers: BTreeMap::new(),
        body: Vec::new(),
    }
}

/// Helper to build a POST request with a JSON body.
pub fn post_json(url: &str, body: &str) -> HttpRequest {
    HttpRequest {
        method: "POST".into(),
        url: url.into(),
        headers: BTreeMap::new(),
        body: body.as_bytes().to_vec(),
    }
}

/// Helper to build a response with a body.
pub fn response(status: u16, body: &str) -> HttpResponse {
    HttpResponse {
        status,
        headers: BTreeMap::new(),
        body: body.as_bytes().to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recorded_http_replays_in_order() {
        let client = RecordedHttp::new(vec![
            (get("http://example.com/a"), response(200, "A")),
            (get("http://example.com/b"), response(200, "B")),
        ]);
        let r1 = client.call(&get("http://example.com/a")).unwrap();
        let r2 = client.call(&get("http://example.com/b")).unwrap();
        assert_eq!(r1.status, 200);
        assert_eq!(r2.status, 200);
        assert_eq!(client.calls_made(), 2);
    }
}
