//! HTTP client trait and test implementation for library sources.
//!
//! The trait is re-exported from pcbcraft-platform; this module provides
//! the RecordedHttp test fixture and convenience constructors used by
//! library source tests.

pub use pcbcraft_platform::{HttpClient, HttpError, HttpRequest, HttpResponse};

use std::sync::atomic::{AtomicUsize, Ordering};

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

pub fn post_json(url: &str, body_prefix: &str) -> HttpRequest {
    let headers = std::collections::BTreeMap::new();
    HttpRequest {
        method: "POST".into(),
        url: url.into(),
        headers,
        body: body_prefix.as_bytes().to_vec(),
    }
}

pub fn get(url: &str) -> HttpRequest {
    let headers = std::collections::BTreeMap::new();
    HttpRequest {
        method: "GET".into(),
        url: url.into(),
        headers,
        body: Vec::new(),
    }
}

pub fn response(status: u16, body: &str) -> HttpResponse {
    let headers = std::collections::BTreeMap::new();
    HttpResponse {
        status,
        headers,
        body: body.as_bytes().to_vec(),
    }
}

pub fn response_with_header(status: u16, name: &str, value: &str, body: &str) -> HttpResponse {
    let mut headers = std::collections::BTreeMap::new();
    headers.insert(name.into(), value.into());
    HttpResponse {
        status,
        headers,
        body: body.as_bytes().to_vec(),
    }
}
