#![cfg(not(target_arch = "wasm32"))]

use pcbcraft_library::credentials::{self, MemoryKeyStore};
use pcbcraft_library::element14::{Element14Source, MAX_STORE_ID_LEN};
use pcbcraft_library::http_client::{HttpRequest, HttpResponse, RecordedHttp};
use pcbcraft_library::sources::{PartSource, SourceError};
use std::sync::Arc;

const SEARCH: &str = include_str!("fixtures/sources/element14_search.json");
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

fn keys() -> Result<Arc<MemoryKeyStore>, pcbcraft_library::credentials::CredentialError> {
    let store = Arc::new(MemoryKeyStore::default());
    store.set(credentials::keys::ELEMENT14_API_KEY, API_KEY)?;
    store.set(credentials::keys::ELEMENT14_STORE_ID, STORE_ID)?;
    Ok(store)
}

fn source_err(e: pcbcraft_library::credentials::CredentialError) -> SourceError {
    SourceError::Unavailable(e.to_string())
}

#[test]
fn element14_search_parses_a_recorded_response() -> Result<(), SourceError> {
    let http = Arc::new(RecordedHttp::new(vec![(
        HttpRequest {
            method: "GET".into(),
            url: catalog_url("RC0603FR-0710KL"),
            headers: Default::default(),
            body: Vec::new(),
        },
        HttpResponse {
            status: 200,
            headers: Default::default(),
            body: SEARCH.as_bytes().to_vec(),
        },
    )]));
    let source = Element14Source::new(http, keys().map_err(source_err)?);
    let found = source.search("RC0603FR-0710KL")?;
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].mpn, "RC0603FR-0710KL");
    assert!(found[0].stock > 0);
    assert!(!found[0].price_breaks.is_empty());
    assert!(!found[0].datasheet_url.is_empty());
    assert!(!found[0].product_url.is_empty());
    Ok(())
}

#[test]
fn element14_store_id_is_bounded() -> Result<(), SourceError> {
    let store = Arc::new(MemoryKeyStore::default());
    store
        .set(credentials::keys::ELEMENT14_API_KEY, API_KEY)
        .map_err(source_err)?;
    let long = "x".repeat(MAX_STORE_ID_LEN + 1);
    store
        .set(credentials::keys::ELEMENT14_STORE_ID, &long)
        .map_err(source_err)?;
    let http = Arc::new(RecordedHttp::new(vec![]));
    let source = Element14Source::new(http, store);
    let err = source
        .search("RC0603FR-0710KL")
        .expect_err("store id too long");
    assert!(
        matches!(err, SourceError::Invalid(ref m) if m.contains("store id")),
        "unexpected {err:?}"
    );
    Ok(())
}
