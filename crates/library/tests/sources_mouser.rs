#![cfg(not(target_arch = "wasm32"))]

use pcbcraft_library::credentials::{self, MemoryKeyStore};
use pcbcraft_library::http_client::{HttpRequest, HttpResponse, RecordedHttp};
use pcbcraft_library::mouser::MouserSource;
use pcbcraft_library::sources::PartSource;
use std::sync::Arc;

const SEARCH: &str = include_str!("fixtures/sources/mouser_search.json");

fn source_err(e: pcbcraft_library::credentials::CredentialError) -> pcbcraft_library::sources::SourceError {
    pcbcraft_library::sources::SourceError::Unavailable(e.to_string())
}

#[test]
fn mouser_search_parses_a_recorded_response() -> Result<(), pcbcraft_library::sources::SourceError> {
    let store = Arc::new(MemoryKeyStore::default());
    store
        .set(credentials::keys::MOUSER_API_KEY, "sample-key")
        .map_err(source_err)?;
    let url = "https://api.mouser.com/api/v1/search/partnumber?partNumber=CRCW060310K0FKEA&apiKey=sample-key";
    let http = Arc::new(RecordedHttp::new(vec![(
        HttpRequest {
            method: "GET".into(),
            url: url.into(),
            headers: Default::default(),
            body: Vec::new(),
        },
        HttpResponse {
            status: 200,
            headers: Default::default(),
            body: SEARCH.as_bytes().to_vec(),
        },
    )]));
    let source = MouserSource::new(http, store);
    let found = source.search("CRCW060310K0FKEA")?;
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].mpn, "CRCW060310K0FKEA");
    assert!(found[0].stock > 0);
    assert!(!found[0].price_breaks.is_empty());
    Ok(())
}
