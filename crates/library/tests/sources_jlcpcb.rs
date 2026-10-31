#![cfg(not(target_arch = "wasm32"))]

use pcbcraft_library::credentials::{self, MemoryKeyStore};
use pcbcraft_library::http_client::{HttpRequest, HttpResponse, RecordedHttp};
use pcbcraft_library::jlcpcb::{COMPONENT_DETAIL_URL, JlcpcbSource};
use pcbcraft_library::sources::{PartSource, SourceError};
use std::sync::Arc;

const DETAIL: &str = include_str!("fixtures/sources/jlcpcb_detail.json");

fn keys() -> Result<Arc<MemoryKeyStore>, pcbcraft_library::credentials::CredentialError> {
    let store = Arc::new(MemoryKeyStore::default());
    store.set(credentials::keys::JLCPCB_APP_ID, "app-test")?;
    store.set(credentials::keys::JLCPCB_ACCESS_KEY, "access-test")?;
    store.set(credentials::keys::JLCPCB_SECRET_KEY, "secret-test")?;
    Ok(store)
}

fn source_err(e: pcbcraft_library::credentials::CredentialError) -> SourceError {
    SourceError::Unavailable(e.to_string())
}

#[test]
fn jlcpcb_search_parses_a_recorded_response() -> Result<(), SourceError> {
    let http = Arc::new(RecordedHttp::new(vec![(
        HttpRequest {
            method: "POST".into(),
            url: COMPONENT_DETAIL_URL.into(),
            headers: Default::default(),
            body: Vec::new(),
        },
        HttpResponse {
            status: 200,
            headers: Default::default(),
            body: DETAIL.as_bytes().to_vec(),
        },
    )]));
    let source = JlcpcbSource::new(http, keys().map_err(source_err)?);
    let found = source.search("C25804")?;
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].mpn, "RC0603FR-0710KL");
    assert_eq!(found[0].sku, "C25804");
    assert!(found[0].stock > 0);
    assert!(!found[0].price_breaks.is_empty());
    assert!(!found[0].datasheet_url.is_empty());
    Ok(())
}
