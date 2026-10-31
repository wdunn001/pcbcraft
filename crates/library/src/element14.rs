//! Live part source for the element14 partner Product Search API (REST).

use crate::credentials::{self, KeyStore};
use crate::http_client::{HttpClient, HttpError, HttpRequest, HttpResponse};
use crate::rate_limit::{Clock, SystemClock, TokenBucket, rate_limited};
use crate::source_errors::{map_auth_or_permission, map_element14_errors};
use crate::source_hosts;
use crate::sources::{
    Attribution, PartResult, PartSource, PriceBreak, SetupCard, SourceError, check_query,
    DEFAULT_CACHE_SECS, MAX_QUERY_BYTES,
};
use serde::Deserialize;
use std::sync::{Arc, Mutex};

/// Cache key segment for this source.
pub const SOURCE_ID: &str = "element14";
const SOURCE_NAME: &str = "element14";
/// Documented catalog endpoint (partner Product Search API).
pub const CATALOG_URL: &str = "https://api.element14.com/catalog/products";
const REQUESTS_PER_MINUTE: u64 = 60;
/// Longest regional store id string accepted from preferences.
pub const MAX_STORE_ID_LEN: usize = 64;

/// Keyword and manufacturer part number search through the partner REST catalog API.
pub struct Element14Source {
    http: Arc<dyn HttpClient>,
    keys: Arc<dyn KeyStore>,
    bucket: Mutex<TokenBucket>,
    clock: Arc<dyn Clock>,
}

impl Element14Source {
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

    fn api_key(&self) -> Result<String, SourceError> {
        self.keys
            .get(credentials::keys::ELEMENT14_API_KEY)
            .map_err(map_cred_err)?
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| SourceError::Unavailable("add your element14 key in setup".into()))
    }

    fn store_id(&self) -> Result<String, SourceError> {
        let id = self
            .keys
            .get(credentials::keys::ELEMENT14_STORE_ID)
            .map_err(map_cred_err)?
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| {
                SourceError::Unavailable("choose your regional store id in setup".into())
            })?;
        let trimmed = id.trim();
        if trimmed.len() > MAX_STORE_ID_LEN {
            return Err(SourceError::Invalid(format!(
                "the store id must be at most {MAX_STORE_ID_LEN} characters"
            )));
        }
        Ok(trimmed.to_owned())
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

    fn search_url(&self, term: &str, api_key: &str, store_id: &str) -> String {
        format!(
            "{CATALOG_URL}?callInfo.responseDataFormat=JSON&term={}&storeInfo.id={}&\
             resultsSettings.offset=0&resultsSettings.numberOfResults=25&\
             resultsSettings.responseGroup=medium&callInfo.apiKey={}",
            url_encode(term),
            url_encode(store_id),
            url_encode(api_key)
        )
    }

    fn call_search(&self, query: &str) -> Result<HttpResponse, SourceError> {
        let api_key = self.api_key()?;
        let store_id = self.store_id()?;
        self.take_token()?;
        let term = if query.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            format!("manuPartNum:{query}")
        } else {
            format!("any:{query}")
        };
        let url = self.search_url(&term, &api_key, &store_id);
        if !source_hosts::url_uses_documented_host(&url) {
            return Err(SourceError::Unavailable(
                "the catalog address is not allowed".into(),
            ));
        }
        let response = self
            .http
            .call(&HttpRequest {
                method: "GET".into(),
                url,
                headers: Default::default(),
                body: Vec::new(),
            })
            .map_err(map_http_err)?;
        if response.status == 429 {
            apply_retry_after(&response, &self.bucket)?;
        }
        if let Some(err) = map_auth_or_permission(response.status, None) {
            return Err(err);
        }
        if let Some(err) = map_element14_errors(&response.body) {
            return Err(err);
        }
        Ok(response)
    }

    fn search_live(&self, query: &str) -> Result<Vec<PartResult>, SourceError> {
        let response = self.call_search(query)?;
        if response.status == 404 {
            return Err(SourceError::NotFound(query.to_owned()));
        }
        if response.status != 200 {
            return Err(SourceError::Unavailable(
                "the catalog search failed".into(),
            ));
        }
        let parsed: KeywordSearchReturn = serde_json::from_slice(&response.body).map_err(|e| {
            SourceError::Unavailable(format!("the search answer was not valid: {e}"))
        })?;
        let now = self.clock.now_secs();
        Ok(parsed
            .keywordSearchReturn
            .products
            .into_iter()
            .filter_map(|p| part_from_product(p, now))
            .collect())
    }
}

impl PartSource for Element14Source {
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
                "Apply for a partner API key on the element14 developer portal.".into(),
                "Pick the regional store id that matches your account (for example www.newark.com)."
                    .into(),
                "Paste the key and store id below, then press Test.".into(),
            ],
            portal_url: "https://partner.element14.com/".into(),
            fields: vec![
                credentials::keys::ELEMENT14_API_KEY.into(),
                credentials::keys::ELEMENT14_STORE_ID.into(),
            ],
        }
    }
    fn attribution(&self) -> Attribution {
        Attribution {
            text: "Pricing and availability from element14".into(),
            url: "https://www.element14.com/".into(),
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
struct KeywordSearchReturn {
    #[serde(default)]
    keywordSearchReturn: ProductsWrapper,
}

#[derive(Default, Deserialize)]
struct ProductsWrapper {
    #[serde(default)]
    products: Vec<ProductJson>,
}

#[derive(Deserialize)]
struct ProductJson {
    #[serde(default)]
    sku: String,
    #[serde(default)]
    displayName: String,
    #[serde(default)]
    vendorName: String,
    #[serde(default)]
    brandName: String,
    #[serde(default)]
    translatedManufacturerPartNumber: String,
    #[serde(default)]
    productStatus: String,
    #[serde(default)]
    datasheets: Vec<DatasheetJson>,
    #[serde(default)]
    prices: Vec<PriceJson>,
    #[serde(default)]
    stock: StockJson,
    #[serde(default)]
    productUrl: String,
}

#[derive(Default, Deserialize)]
struct StockJson {
    #[serde(default)]
    level: u64,
}

#[derive(Deserialize)]
struct DatasheetJson {
    #[serde(default)]
    url: String,
}

#[derive(Deserialize)]
struct PriceJson {
    #[serde(default)]
    from: u32,
    #[serde(default)]
    cost: f64,
}

fn part_from_product(p: ProductJson, retrieved_at: u64) -> Option<PartResult> {
    let mpn = if p.translatedManufacturerPartNumber.is_empty() {
        p.displayName.clone()
    } else {
        p.translatedManufacturerPartNumber
    };
    if mpn.is_empty() {
        return None;
    }
    let datasheet_url = p
        .datasheets
        .into_iter()
        .find_map(|d| {
            if d.url.is_empty() {
                None
            } else {
                Some(d.url)
            }
        })
        .unwrap_or_default();
    let price_breaks = p
        .prices
        .into_iter()
        .map(|b| PriceBreak {
            quantity: b.from.max(1),
            unit_price: b.cost,
            currency: "USD".into(),
        })
        .collect();
    let manufacturer = if p.vendorName.is_empty() {
        p.brandName
    } else {
        p.vendorName
    };
    let product_url = if p.productUrl.is_empty() {
        if p.sku.is_empty() {
            String::new()
        } else {
            format!("https://www.element14.com/{}", p.sku)
        }
    } else {
        p.productUrl
    };
    Some(PartResult {
        source: SOURCE_NAME.into(),
        mpn,
        manufacturer,
        description: p.displayName,
        kind: String::new(),
        distributor: SOURCE_NAME.into(),
        sku: p.sku,
        stock: p.stock.level,
        price_breaks,
        lifecycle: p.productStatus,
        datasheet_url,
        product_url,
        retrieved_at,
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_id_bound_matches_query_limit_order() {
        assert!(MAX_STORE_ID_LEN <= MAX_QUERY_BYTES);
    }
}
