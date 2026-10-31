#![cfg(not(target_arch = "wasm32"))]

use pcbcraft_library::credentials::{self, MemoryKeyStore};
use pcbcraft_library::digikey::DigiKeySource;
use pcbcraft_library::http_client::{HttpRequest, HttpResponse, RecordedHttp};
use pcbcraft_library::rate_limit::FixedClock;
use pcbcraft_library::sources::{PartSource, SourceError};
use std::sync::Arc;

const TOKEN: &str = include_str!("fixtures/sources/digikey_token.json");
const SEARCH: &str = include_str!("fixtures/sources/digikey_search.json");
const MULTI: &str = include_str!("fixtures/sources/digikey_details_multiple.json");

fn source_err(e: pcbcraft_library::credentials::CredentialError) -> SourceError {
    SourceError::Unavailable(e.to_string())
}

fn keys() -> Result<Arc<MemoryKeyStore>, pcbcraft_library::credentials::CredentialError> {
    let store = Arc::new(MemoryKeyStore::default());
    store.set(credentials::keys::DIGIKEY_CLIENT_ID, "test-id")?;
    store.set(credentials::keys::DIGIKEY_CLIENT_SECRET, "test-secret")?;
    Ok(store)
}

fn token_req() -> HttpRequest {
    HttpRequest {
        method: "POST".into(),
        url: "https://api.digikey.com/v1/oauth2/token".into(),
        headers: Default::default(),
        body: Vec::new(),
    }
}

#[test]
fn digikey_search_parses_a_recorded_response() -> Result<(), SourceError> {
    let http = Arc::new(RecordedHttp::new(vec![
        (
            token_req(),
            HttpResponse {
                status: 200,
                headers: Default::default(),
                body: TOKEN.as_bytes().to_vec(),
            },
        ),
        (
            HttpRequest {
                method: "POST".into(),
                url: "https://api.digikey.com/products/v4/search/keyword".into(),
                headers: Default::default(),
                body: Vec::new(),
            },
            HttpResponse {
                status: 200,
                headers: Default::default(),
                body: SEARCH.as_bytes().to_vec(),
            },
        ),
    ]));
    let source = DigiKeySource::new(http, keys().map_err(source_err)?);
    let found = source.search("RC0603FR-0710KL")?;
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].mpn, "RC0603FR-0710KL");
    assert!(found[0].stock > 0);
    assert!(!found[0].price_breaks.is_empty());
    assert!(!found[0].product_url.is_empty());
    Ok(())
}

#[test]
fn digikey_details_with_several_matches_asks_the_user_to_pick() -> Result<(), SourceError> {
    let http = Arc::new(RecordedHttp::new(vec![
        (
            token_req(),
            HttpResponse {
                status: 200,
                headers: Default::default(),
                body: TOKEN.as_bytes().to_vec(),
            },
        ),
        (
            HttpRequest {
                method: "GET".into(),
                url: "https://api.digikey.com/products/v4/product/NE555P".into(),
                headers: Default::default(),
                body: Vec::new(),
            },
            HttpResponse {
                status: 400,
                headers: Default::default(),
                body: MULTI.as_bytes().to_vec(),
            },
        ),
    ]));
    let source = DigiKeySource::new(http, keys().map_err(source_err)?);
    let err = source.fetch("NE555P").expect_err("ambiguous mpn");
    assert!(
        matches!(err, SourceError::Invalid(ref m) if m.contains("several parts match")),
        "unexpected {err:?}"
    );
    Ok(())
}

#[test]
fn digikey_token_is_refreshed_before_it_expires() -> Result<(), SourceError> {
    let clock = Arc::new(FixedClock::new(1_000_000));
    let http = Arc::new(RecordedHttp::new(vec![
        (
            token_req(),
            HttpResponse {
                status: 200,
                headers: Default::default(),
                body: TOKEN.as_bytes().to_vec(),
            },
        ),
        (
            HttpRequest {
                method: "POST".into(),
                url: "https://api.digikey.com/products/v4/search/keyword".into(),
                headers: Default::default(),
                body: Vec::new(),
            },
            HttpResponse {
                status: 200,
                headers: Default::default(),
                body: SEARCH.as_bytes().to_vec(),
            },
        ),
        (
            token_req(),
            HttpResponse {
                status: 200,
                headers: Default::default(),
                body: TOKEN.as_bytes().to_vec(),
            },
        ),
        (
            HttpRequest {
                method: "POST".into(),
                url: "https://api.digikey.com/products/v4/search/keyword".into(),
                headers: Default::default(),
                body: Vec::new(),
            },
            HttpResponse {
                status: 200,
                headers: Default::default(),
                body: SEARCH.as_bytes().to_vec(),
            },
        ),
    ]));
    let source = DigiKeySource::with_clock(http.clone(), keys().map_err(source_err)?, clock.clone());
    source.search("RC0603FR-0710KL")?;
    clock.advance(601);
    source.search("RC0603FR-0710KL")?;
    assert_eq!(
        http.calls_made(),
        4,
        "a fresh token is fetched after the old one expires"
    );
    Ok(())
}
