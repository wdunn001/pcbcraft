//! Live distributor source for the Product Information API (keyword search and product details).

use crate::credentials::{self, KeyStore};
use crate::http_client::{HttpClient, HttpError, HttpRequest, HttpResponse};
use crate::rate_limit::{Clock, SystemClock, TokenBucket, rate_limited};
use crate::sources::{
    Attribution, DEFAULT_CACHE_SECS, PartResult, PartSource, PriceBreak, SetupCard, SourceError,
    check_query,
};
use serde::Deserialize;
use std::sync::{Arc, Mutex};

/// Cache key segment for this source.
pub const SOURCE_ID: &str = "digikey";
const SOURCE_NAME: &str = "Digi-Key";
const TOKEN_URL: &str = "https://api.digikey.com/v1/oauth2/token";
const SEARCH_URL: &str = "https://api.digikey.com/products/v4/search/keyword";
const DETAILS_URL: &str = "https://api.digikey.com/products/v4/product/";

/// One search per minute budget from the public rate table.
const REQUESTS_PER_MINUTE: u64 = 120;

struct OAuthToken {
    access_token: String,
    expires_at: u64,
}

/// Live keyword search and product details through the distributor Product Information API.
pub struct DigiKeySource {
    http: Arc<dyn HttpClient>,
    keys: Arc<dyn KeyStore>,
    bucket: Mutex<TokenBucket>,
    token: Mutex<Option<OAuthToken>>,
    clock: Arc<dyn Clock>,
}

impl DigiKeySource {
    pub fn new(http: Arc<dyn HttpClient>, keys: Arc<dyn KeyStore>) -> Self {
        Self::with_clock(http, keys, Arc::new(SystemClock))
    }

    pub fn with_clock(
        http: Arc<dyn HttpClient>,
        keys: Arc<dyn KeyStore>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        let bucket = TokenBucket::new(REQUESTS_PER_MINUTE, REQUESTS_PER_MINUTE, {
            let c = Arc::clone(&clock);
            Box::new(ClockProxy(c))
        });
        Self {
            http,
            keys,
            bucket: Mutex::new(bucket),
            token: Mutex::new(None),
            clock,
        }
    }

    fn now(&self) -> u64 {
        self.clock.now_secs()
    }

    fn credentials_ready(&self) -> Result<(String, String), SourceError> {
        let id = self
            .keys
            .get(credentials::keys::DIGIKEY_CLIENT_ID)
            .map_err(map_cred_err)?
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| SourceError::Unavailable("add your Digi-Key keys in setup".into()))?;
        let secret = self
            .keys
            .get(credentials::keys::DIGIKEY_CLIENT_SECRET)
            .map_err(map_cred_err)?
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| SourceError::Unavailable("add your Digi-Key keys in setup".into()))?;
        Ok((id, secret))
    }

    fn take_token(&self) -> Result<(), SourceError> {
        let mut bucket = self
            .bucket
            .lock()
            .map_err(|_| SourceError::Unavailable("the rate limiter lock failed".into()))?;
        match bucket.try_take() {
            Ok(()) => Ok(()),
            Err(wait) => Err(rate_limited(wait)),
        }
    }

    fn bearer(&self, client_id: &str, client_secret: &str) -> Result<String, SourceError> {
        let now = self.now();
        if let Ok(guard) = self.token.lock()
            && let Some(tok) = guard.as_ref()
            && tok.expires_at > now.saturating_add(30)
        {
            return Ok(tok.access_token.clone());
        }
        self.take_token()?;
        let body = format!(
            "client_id={}&client_secret={}&grant_type=client_credentials",
            url_encode(client_id),
            url_encode(client_secret)
        );
        let response = self
            .http
            .call(&HttpRequest {
                method: "POST".into(),
                url: TOKEN_URL.into(),
                headers: Default::default(),
                body: body.into_bytes(),
            })
            .map_err(map_http_err)?;
        handle_rate_limit(&response)?;
        if response.status != 200 {
            return Err(SourceError::Unavailable(
                "could not sign in to the distributor".into(),
            ));
        }
        let parsed: TokenResponse = serde_json::from_slice(&response.body).map_err(|e| {
            SourceError::Unavailable(format!("the sign-in answer was not valid: {e}"))
        })?;
        let expires_at = now.saturating_add(parsed.expires_in.min(3600));
        let access = parsed.access_token;
        if let Ok(mut guard) = self.token.lock() {
            *guard = Some(OAuthToken {
                access_token: access.clone(),
                expires_at,
            });
        }
        Ok(access)
    }

    fn authorized_post(
        &self,
        url: &str,
        client_id: &str,
        body: &str,
    ) -> Result<HttpResponse, SourceError> {
        let (id, secret) = self.credentials_ready()?;
        let bearer = self.bearer(&id, &secret)?;
        self.take_token()?;
        let mut headers = std::collections::BTreeMap::new();
        headers.insert("Authorization".into(), format!("Bearer {bearer}"));
        headers.insert("X-DIGIKEY-Client-Id".into(), client_id.to_owned());
        headers.insert("Content-Type".into(), "application/json".into());
        let response = self
            .http
            .call(&HttpRequest {
                method: "POST".into(),
                url: url.into(),
                headers,
                body: body.as_bytes().to_vec(),
            })
            .map_err(map_http_err)?;
        if response.status == 429 {
            apply_retry_after(&response, &self.bucket)?;
        }
        handle_rate_limit(&response)?;
        Ok(response)
    }

    fn authorized_get(&self, url: &str, client_id: &str) -> Result<HttpResponse, SourceError> {
        let (id, secret) = self.credentials_ready()?;
        let bearer = self.bearer(&id, &secret)?;
        self.take_token()?;
        let mut headers = std::collections::BTreeMap::new();
        headers.insert("Authorization".into(), format!("Bearer {bearer}"));
        headers.insert("X-DIGIKEY-Client-Id".into(), client_id.to_owned());
        let response = self
            .http
            .call(&HttpRequest {
                method: "GET".into(),
                url: url.into(),
                headers,
                body: Vec::new(),
            })
            .map_err(map_http_err)?;
        if response.status == 429 {
            apply_retry_after(&response, &self.bucket)?;
        }
        handle_rate_limit(&response)?;
        Ok(response)
    }

    fn search_live(&self, query: &str) -> Result<Vec<PartResult>, SourceError> {
        let (client_id, _) = self.credentials_ready()?;
        let body = format!(
            r#"{{"Keywords":"{}","RecordCount":25,"RecordStartPosition":0}}"#,
            escape_json_string(query)
        );
        let response = self.authorized_post(SEARCH_URL, &client_id, &body)?;
        if response.status == 404 {
            return Err(SourceError::NotFound(query.to_owned()));
        }
        if response.status != 200 {
            return Err(SourceError::Unavailable(
                "the distributor search failed".into(),
            ));
        }
        let parsed: SearchResponse = serde_json::from_slice(&response.body).map_err(|e| {
            SourceError::Unavailable(format!("the search answer was not valid: {e}"))
        })?;
        let now = self.now();
        Ok(parsed
            .Products
            .into_iter()
            .filter_map(|p| part_from_product(p, now))
            .collect())
    }

    fn fetch_live(&self, mpn: &str) -> Result<PartResult, SourceError> {
        let (client_id, _) = self.credentials_ready()?;
        let url = format!("{DETAILS_URL}{}", url_encode_path(mpn));
        let response = self.authorized_get(&url, &client_id)?;
        if response.status == 404 {
            return Err(SourceError::NotFound(mpn.to_owned()));
        }
        if response.status == 400 {
            let text = String::from_utf8_lossy(&response.body);
            if text.contains("Multiple") || text.contains("multiple") {
                return Err(SourceError::Invalid("several parts match, pick one".into()));
            }
        }
        if response.status != 200 {
            return Err(SourceError::Unavailable(
                "the distributor details call failed".into(),
            ));
        }
        let parsed: ProductResponse = serde_json::from_slice(&response.body).map_err(|e| {
            SourceError::Unavailable(format!("the details answer was not valid: {e}"))
        })?;
        part_from_product(parsed.Product, self.now())
            .ok_or_else(|| SourceError::NotFound(mpn.to_owned()))
    }
}

impl PartSource for DigiKeySource {
    fn id(&self) -> &str {
        SOURCE_ID
    }
    fn name(&self) -> &str {
        SOURCE_NAME
    }
    fn search(&self, query: &str) -> Result<Vec<PartResult>, SourceError> {
        let q = check_query(query)?;
        let results = self.search_live(q)?;
        crate::source_log::query_summary(SOURCE_ID, q.len(), results.len());
        Ok(results)
    }
    fn fetch(&self, mpn: &str) -> Result<PartResult, SourceError> {
        let q = check_query(mpn)?;
        let part = self.fetch_live(q)?;
        crate::source_log::query_summary(SOURCE_ID, q.len(), 1);
        Ok(part)
    }
    fn setup(&self) -> SetupCard {
        SetupCard {
            steps: vec![
                "Create a free developer account on the distributor site.".into(),
                "Register an app and copy the client id and client secret.".into(),
                "Paste both values into the fields below and press Test.".into(),
            ],
            portal_url: "https://developer.digikey.com/".into(),
            fields: vec![
                credentials::keys::DIGIKEY_CLIENT_ID.into(),
                credentials::keys::DIGIKEY_CLIENT_SECRET.into(),
            ],
        }
    }
    fn attribution(&self) -> Attribution {
        Attribution {
            text: "Pricing and availability from Digi-Key Electronics".into(),
            url: "https://www.digikey.com/".into(),
        }
    }
    fn cache_secs(&self) -> u64 {
        DEFAULT_CACHE_SECS
    }
}

struct ClockProxy(Arc<dyn Clock>);

impl Clock for ClockProxy {
    fn now_secs(&self) -> u64 {
        self.0.now_secs()
    }
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: u64,
}

#[derive(Deserialize)]
struct SearchResponse {
    #[serde(default)]
    Products: Vec<ProductJson>,
}

#[derive(Deserialize)]
struct ProductResponse {
    Product: ProductJson,
}

#[derive(Deserialize)]
struct ProductJson {
    #[serde(default)]
    ManufacturerProductNumber: String,
    #[serde(default)]
    Manufacturer: ManufacturerJson,
    #[serde(default)]
    Description: DescriptionJson,
    #[serde(default)]
    QuantityAvailable: u64,
    #[serde(default)]
    ProductUrl: String,
    #[serde(default)]
    DatasheetUrl: String,
    #[serde(default)]
    ProductStatus: StatusJson,
    #[serde(default)]
    StandardPricing: Vec<PriceJson>,
}

#[derive(Default, Deserialize)]
struct ManufacturerJson {
    #[serde(default)]
    Name: String,
}

#[derive(Default, Deserialize)]
struct DescriptionJson {
    #[serde(default)]
    ProductDescription: String,
}

#[derive(Default, Deserialize)]
struct StatusJson {
    #[serde(default)]
    Status: String,
}

#[derive(Deserialize)]
struct PriceJson {
    #[serde(default)]
    BreakQuantity: u32,
    #[serde(default)]
    UnitPrice: f64,
    #[serde(default)]
    Currency: String,
}

fn part_from_product(p: ProductJson, retrieved_at: u64) -> Option<PartResult> {
    if p.ManufacturerProductNumber.is_empty() {
        return None;
    }
    let price_breaks = p
        .StandardPricing
        .into_iter()
        .map(|b| PriceBreak {
            quantity: b.BreakQuantity.max(1),
            unit_price: b.UnitPrice,
            currency: if b.Currency.is_empty() {
                "USD".into()
            } else {
                b.Currency
            },
        })
        .collect();
    Some(PartResult {
        source: SOURCE_NAME.into(),
        mpn: p.ManufacturerProductNumber,
        manufacturer: p.Manufacturer.Name,
        description: p.Description.ProductDescription,
        kind: String::new(),
        distributor: SOURCE_NAME.into(),
        sku: String::new(),
        stock: p.QuantityAvailable,
        price_breaks,
        lifecycle: p.ProductStatus.Status,
        datasheet_url: p.DatasheetUrl,
        product_url: p.ProductUrl,
        retrieved_at,
    })
}

fn map_cred_err(e: crate::credentials::CredentialError) -> SourceError {
    SourceError::Unavailable(e.to_string())
}

fn map_http_err(e: HttpError) -> SourceError {
    SourceError::Unavailable(e.to_string())
}

fn handle_rate_limit(response: &HttpResponse) -> Result<(), SourceError> {
    if response.status == 429 {
        let wait = response
            .headers
            .get("Retry-After")
            .and_then(|h| h.parse::<u64>().ok())
            .unwrap_or(60);
        return Err(rate_limited(wait));
    }
    Ok(())
}

fn apply_retry_after(
    response: &HttpResponse,
    bucket: &Mutex<TokenBucket>,
) -> Result<(), SourceError> {
    let wait = response
        .headers
        .get("Retry-After")
        .and_then(|h| h.parse::<u64>().ok())
        .unwrap_or(60);
    if let Ok(mut b) = bucket.lock() {
        b.pause_for(wait);
    }
    Err(rate_limited(wait))
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

fn url_encode_path(s: &str) -> String {
    url_encode(s)
}

fn escape_json_string(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}
