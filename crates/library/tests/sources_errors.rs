#![cfg(not(target_arch = "wasm32"))]

use pcbcraft_library::credentials::{self, MemoryKeyStore};
use pcbcraft_library::element14::Element14Source;

const API_KEY: &str = "test-partner-key-123456789012";
const STORE_ID: &str = "www.newark.com";

fn catalog_url(query: &str) -> String {
    let term = format!("manuPartNum:{query}");
    format!(
        "https://api.element14.com/catalog/products?callInfo.responseDataFormat=JSON&term={}&storeInfo.id={}&resultsSettings.offset=0&resultsSettings.numberOfResults=25&resultsSettings.responseGroup=medium&callInfo.apiKey={}",
        url_encode(&term),
        url_encode(STORE_ID),
        url_encode(API_KEY)
    )
}

fn url_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}
use pcbcraft_library::http_client::{HttpRequest, HttpResponse, RecordedHttp};
use pcbcraft_library::source_errors::UNAPPROVED_KEY_MESSAGE;
use pcbcraft_library::sources::{PartSource, SourceError};
use std::sync::Arc;

fn source_err(e: pcbcraft_library::credentials::CredentialError) -> SourceError {
    SourceError::Unavailable(e.to_string())
}

#[test]
fn an_unapproved_key_gives_a_plain_message() -> Result<(), SourceError> {
    let store = Arc::new(MemoryKeyStore::default());
    store
        .set(credentials::keys::ELEMENT14_API_KEY, API_KEY)
        .map_err(source_err)?;
    store
        .set(credentials::keys::ELEMENT14_STORE_ID, STORE_ID)
        .map_err(source_err)?;
    let http = Arc::new(RecordedHttp::new(vec![(
        HttpRequest {
            method: "GET".into(),
            url: catalog_url("RC0603FR-0710KL"),
            headers: Default::default(),
            body: Vec::new(),
        },
        HttpResponse {
            status: 403,
            headers: Default::default(),
            body: br#"{"errorCode":100003}"#.to_vec(),
        },
    )]));
    let source = Element14Source::new(http, store);
    let err = source.search("RC0603FR-0710KL").expect_err("forbidden key");
    assert!(
        matches!(err, SourceError::Unavailable(ref m) if m == UNAPPROVED_KEY_MESSAGE),
        "unexpected {err:?}"
    );
    Ok(())
}
