#![cfg(not(target_arch = "wasm32"))]

use pcbcraft_library::credentials::{self, MemoryKeyStore};
use pcbcraft_library::http_client::{HttpRequest, HttpResponse, RecordedHttp};
use pcbcraft_library::lcsc::{LcscSource, KEYWORD_SEARCH_URL};
use pcbcraft_library::sources::{PartSource, SourceError};
use std::sync::Arc;

const SEARCH: &str = include_str!("fixtures/sources/lcsc_search.json");

fn keys() -> Result<Arc<MemoryKeyStore>, pcbcraft_library::credentials::CredentialError> {
    let store = Arc::new(MemoryKeyStore::default());
    store.set(credentials::keys::LCSC_API_KEY, "test-lcsc-key")?;
    store.set(credentials::keys::LCSC_API_SECRET, "test-lcsc-secret")?;
    Ok(store)
}

fn source_err(e: pcbcraft_library::credentials::CredentialError) -> SourceError {
    SourceError::Unavailable(e.to_string())
}

#[test]
fn lcsc_search_parses_a_recorded_response() -> Result<(), SourceError> {
    let http = Arc::new(RecordedHttp::new(vec![(
        HttpRequest {
            method: "POST".into(),
            url: KEYWORD_SEARCH_URL.into(),
            headers: Default::default(),
            body: Vec::new(),
        },
        HttpResponse {
            status: 200,
            headers: Default::default(),
            body: SEARCH.as_bytes().to_vec(),
        },
    )]));
    let source = LcscSource::new(http, keys().map_err(source_err)?);
    let found = source.search("0402WGF5101TCE")?;
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].mpn, "0402WGF5101TCE");
    assert!(found[0].stock > 0);
    assert!(!found[0].price_breaks.is_empty());
    assert!(!found[0].product_url.is_empty());
    Ok(())
}
