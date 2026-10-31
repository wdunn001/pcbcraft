//! HTTP behind a trait so live distributor sources and tests share one shape.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};

/// One outbound call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpRequest {
    pub method: String,
    pub url: String,
    pub headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
}

/// Answer from the transport.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
}

/// Why a call did not return a response body.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum HttpError {
    #[error("{0}")]
    Failed(String),
}

/// Makes distributor API calls on native builds; tests replay fixtures.
pub trait HttpClient: Send + Sync {
    fn call(&self, request: &HttpRequest) -> Result<HttpResponse, HttpError>;
}

/// Replays recorded answers in order.
pub struct RecordedHttp {
    script: Vec<(HttpRequest, HttpResponse)>,
    step: AtomicUsize,
}

impl RecordedHttp {
    pub fn new(script: Vec<(HttpRequest, HttpResponse)>) -> Self {
        Self {
            script,
            step: AtomicUsize::new(0),
        }
    }

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
        let _ = &expected.body;
        Ok(response.clone())
    }
}

/// Match a recorded POST with a JSON body prefix (order-sensitive fields ignored).
pub fn post_json(url: &str, body_prefix: &str) -> HttpRequest {
    HttpRequest {
        method: "POST".into(),
        url: url.into(),
        headers: BTreeMap::new(),
        body: body_prefix.as_bytes().to_vec(),
    }
}

pub fn get(url: &str) -> HttpRequest {
    HttpRequest {
        method: "GET".into(),
        url: url.into(),
        headers: BTreeMap::new(),
        body: Vec::new(),
    }
}

pub fn response(status: u16, body: &str) -> HttpResponse {
    HttpResponse {
        status,
        headers: BTreeMap::new(),
        body: body.as_bytes().to_vec(),
    }
}

pub fn response_with_header(status: u16, name: &str, value: &str, body: &str) -> HttpResponse {
    let mut headers = BTreeMap::new();
    headers.insert(name.into(), value.into());
    HttpResponse {
        status,
        headers,
        body: body.as_bytes().to_vec(),
    }
}
