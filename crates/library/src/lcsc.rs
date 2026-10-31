//! Live part source for the LCSC partner Open API (keyword search).

use crate::credentials::{self, KeyStore};
use crate::http_client::{HttpClient, HttpError, HttpRequest, HttpResponse};
use crate::rate_limit::{Clock, SystemClock, TokenBucket, rate_limited};
use crate::source_errors::map_auth_or_permission;
use crate::source_hosts;
use crate::source_sign::lcsc_signature;
use crate::sources::{
    Attribution, PartResult, PartSource, PriceBreak, SetupCard, SourceError, check_query,
    DEFAULT_CACHE_SECS,
};
use serde::Deserialize;
use std::sync::{Arc, Mutex};

/// Cache key segment for this source.
pub const SOURCE_ID: &str = "lcsc";
const SOURCE_NAME: &str = "LCSC";
/// Documented keyword search endpoint (LCSC Open API instruction).
pub const KEYWORD_SEARCH_URL: &str = "https://www.lcsc.com/openapi/product/search";
const REQUESTS_PER_MINUTE: u64 = 60;

/// Signed keyword search through the LCSC partner API.
pub struct LcscSource {
    http: Arc<dyn HttpClient>,
    keys: Arc<dyn KeyStore>,
    bucket: Mutex<TokenBucket>,
    clock: Arc<dyn Clock>,
}

impl LcscSource {
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
            clock,
        }
    }

    fn credentials(&self) -> Result<(String, String), SourceError> {
        let key = self
            .keys
            .get(credentials::keys::LCSC_API_KEY)
            .map_err(map_cred_err)?
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| SourceError::Unavailable("add your LCSC key in setup".into()))?;
        let secret = self
            .keys
            .get(credentials::keys::LCSC_API_SECRET)
            .map_err(map_cred_err)?
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| SourceError::Unavailable("add your LCSC secret in setup".into()))?;
        Ok((key, secret))
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

    fn signed_body(&self, keyword: &str) -> Result<String, SourceError> {
        let (key, secret) = self.credentials()?;
        let nonce = random_nonce();
        let timestamp = self.clock.now_secs().to_string();
        let signature = lcsc_signature(&key, &secret, &nonce, &timestamp);
        Ok(format!(
            r#"{{"key":"{}","nonce":"{}","timestamp":"{}","signature":"{}","keyword":"{}","current_page":1,"page_size":30}}"#,
            escape_json(&key),
            escape_json(&nonce),
            escape_json(&timestamp),
            escape_json(&signature),
            escape_json(keyword)
        ))
    }

    fn search_live(&self, query: &str) -> Result<Vec<PartResult>, SourceError> {
        if !source_hosts::url_uses_documented_host(KEYWORD_SEARCH_URL) {
            return Err(SourceError::Unavailable(
                "the search address is not allowed".into(),
            ));
        }
        self.take_token()?;
        let body = self.signed_body(query)?;
        let response = self
            .http
            .call(&HttpRequest {
                method: "POST".into(),
                url: KEYWORD_SEARCH_URL.into(),
                headers: std::collections::BTreeMap::from([(
                    "Content-Type".into(),
                    "application/json".into(),
                )]),
                body: body.into_bytes(),
            })
            .map_err(map_http_err)?;
        if response.status == 429 {
            apply_retry_after(&response, &self.bucket)?;
        }
        if let Some(err) = map_auth_or_permission(response.status, None) {
            return Err(err);
        }
        let envelope: LcscEnvelope = serde_json::from_slice(&response.body).map_err(|e| {
            SourceError::Unavailable(format!("the search answer was not valid: {e}"))
        })?;
        if let Some(err) = map_auth_or_permission(200, Some(envelope.code)) {
            return Err(err);
        }
        if !envelope.success || envelope.code != 200 {
            return Err(SourceError::Unavailable(
                envelope.message.unwrap_or_else(|| {
                    "the LCSC search failed".into()
                }),
            ));
        }
        let now = self.clock.now_secs();
        Ok(envelope
            .result
            .and_then(|r| r.product_list)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|p| part_from_product(p, now))
            .collect())
    }
}

impl PartSource for LcscSource {
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
        let mut results = self.search_live(q)?;
        results
            .pop()
            .ok_or_else(|| SourceError::NotFound(q.to_owned()))
    }
    fn setup(&self) -> SetupCard {
        SetupCard {
            steps: vec![
                "Apply for LCSC API access through the agent program on lcsc.com.".into(),
                "After review, copy the API key and secret from your account.".into(),
                "Paste both values below and press Test.".into(),
            ],
            portal_url: "https://www.lcsc.com/agent".into(),
            fields: vec![
                credentials::keys::LCSC_API_KEY.into(),
                credentials::keys::LCSC_API_SECRET.into(),
            ],
        }
    }
    fn attribution(&self) -> Attribution {
        Attribution {
            text: "Pricing and availability from LCSC".into(),
            url: "https://www.lcsc.com/".into(),
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
struct LcscEnvelope {
    #[serde(default)]
    success: bool,
    #[serde(default)]
    code: i64,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    result: Option<LcscResult>,
}

#[derive(Deserialize)]
struct LcscResult {
    #[serde(default)]
    product_list: Option<Vec<ProductJson>>,
}

#[derive(Deserialize)]
struct ProductJson {
    #[serde(default)]
    product_code: String,
    #[serde(default)]
    product_model: String,
    #[serde(default)]
    brand_name_en: String,
    #[serde(default)]
    product_desc_en: String,
    #[serde(default)]
    stock_number: u64,
    #[serde(default)]
    product_price_list: Vec<PriceJson>,
    #[serde(default)]
    product_url: String,
    #[serde(default)]
    datasheet_url: String,
}

#[derive(Deserialize)]
struct PriceJson {
    #[serde(default)]
    ladder: u32,
    #[serde(default)]
    currency_price: f64,
    #[serde(default)]
    currency: String,
}

fn part_from_product(p: ProductJson, retrieved_at: u64) -> Option<PartResult> {
    if p.product_model.is_empty() && p.product_code.is_empty() {
        return None;
    }
    let mpn = if p.product_model.is_empty() {
        p.product_code.clone()
    } else {
        p.product_model
    };
    let price_breaks = p
        .product_price_list
        .into_iter()
        .map(|b| PriceBreak {
            quantity: b.ladder.max(1),
            unit_price: b.currency_price,
            currency: if b.currency.is_empty() {
                "USD".into()
            } else {
                b.currency
            },
        })
        .collect();
    let product_url = if p.product_url.is_empty() {
        if p.product_code.is_empty() {
            String::new()
        } else {
            format!("https://www.lcsc.com/product-detail/{}.html", p.product_code)
        }
    } else {
        p.product_url
    };
    Some(PartResult {
        source: SOURCE_NAME.into(),
        mpn,
        manufacturer: p.brand_name_en,
        description: p.product_desc_en,
        kind: String::new(),
        distributor: SOURCE_NAME.into(),
        sku: p.product_code,
        stock: p.stock_number,
        price_breaks,
        lifecycle: String::new(),
        datasheet_url: p.datasheet_url,
        product_url,
        retrieved_at,
    })
}

fn random_nonce() -> String {
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{:016x}", (seed as u64) ^ (seed >> 64) as u64)
}

fn escape_json(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn map_cred_err(e: crate::credentials::CredentialError) -> SourceError {
    SourceError::Unavailable(e.to_string())
}

fn map_http_err(e: HttpError) -> SourceError {
    SourceError::Unavailable(e.to_string())
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
