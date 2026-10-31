//! Live distributor source for the Mouser Search API.

use crate::credentials::{self, KeyStore};
use crate::http_client::{HttpClient, HttpError, HttpRequest};
use crate::rate_limit::{Clock, SystemClock, TokenBucket, rate_limited};
use crate::sources::{
    Attribution, DEFAULT_CACHE_SECS, PartResult, PartSource, PriceBreak, SetupCard, SourceError,
    check_query,
};
use serde::Deserialize;
use std::sync::{Arc, Mutex};

/// Cache key segment for this source.
pub const SOURCE_ID: &str = "mouser";
const SOURCE_NAME: &str = "Mouser";
const SEARCH_BASE: &str = "https://api.mouser.com/api/v1/search/partnumber";

/// Public rate table: thirty calls per minute.
const REQUESTS_PER_MINUTE: u64 = 30;

/// Live part number search through the distributor Search API.
pub struct MouserSource {
    http: Arc<dyn HttpClient>,
    keys: Arc<dyn KeyStore>,
    bucket: Mutex<TokenBucket>,
    clock: Arc<dyn Clock>,
}

impl MouserSource {
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
            .get(credentials::keys::MOUSER_API_KEY)
            .map_err(|e| SourceError::Unavailable(e.to_string()))?
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| SourceError::Unavailable("add your Mouser key in setup".into()))
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

    fn search_live(&self, query: &str) -> Result<Vec<PartResult>, SourceError> {
        let key = self.api_key()?;
        self.take_token()?;
        let url = format!(
            "{SEARCH_BASE}?partNumber={}&apiKey={}",
            url_encode(query),
            url_encode(&key)
        );
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
            let wait = response
                .headers
                .get("Retry-After")
                .and_then(|h| h.parse::<u64>().ok())
                .unwrap_or(12);
            if let Ok(mut b) = self.bucket.lock() {
                b.pause_for(wait);
            }
            return Err(rate_limited(wait));
        }
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
        let now = self.clock.now_secs();
        Ok(parsed
            .search_results
            .Parts
            .into_iter()
            .filter_map(|p| part_from_part(p, now))
            .collect())
    }
}

impl PartSource for MouserSource {
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
                "Create a free developer account on the distributor site.".into(),
                "Request a Search API key and copy it.".into(),
                "Paste the key into the field below and press Test.".into(),
            ],
            portal_url: "https://www.mouser.com/api-hub/".into(),
            fields: vec![credentials::keys::MOUSER_API_KEY.into()],
        }
    }
    fn attribution(&self) -> Attribution {
        Attribution {
            text: "Pricing and availability from Mouser Electronics".into(),
            url: "https://www.mouser.com/".into(),
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
struct SearchResponse {
    #[serde(default, rename = "SearchResults")]
    search_results: PartsWrapper,
}

#[derive(Default, Deserialize)]
struct PartsWrapper {
    #[serde(default)]
    Parts: Vec<PartJson>,
}

#[derive(Deserialize)]
struct PartJson {
    #[serde(default)]
    ManufacturerPartNumber: String,
    #[serde(default)]
    Manufacturer: String,
    #[serde(default)]
    Description: String,
    #[serde(default)]
    Availability: String,
    #[serde(default)]
    DataSheetUrl: String,
    #[serde(default)]
    ProductDetailUrl: String,
    #[serde(default)]
    PriceBreaks: Vec<PriceJson>,
}

#[derive(Deserialize)]
struct PriceJson {
    #[serde(default)]
    Quantity: u32,
    #[serde(default)]
    Price: String,
    #[serde(default)]
    Currency: String,
}

fn part_from_part(p: PartJson, retrieved_at: u64) -> Option<PartResult> {
    if p.ManufacturerPartNumber.is_empty() {
        return None;
    }
    let stock = parse_availability(&p.Availability);
    let price_breaks = p
        .PriceBreaks
        .into_iter()
        .filter_map(|b| {
            let unit = parse_price(&b.Price)?;
            Some(PriceBreak {
                quantity: b.Quantity.max(1),
                unit_price: unit,
                currency: if b.Currency.is_empty() {
                    "USD".into()
                } else {
                    b.Currency
                },
            })
        })
        .collect();
    Some(PartResult {
        source: SOURCE_NAME.into(),
        mpn: p.ManufacturerPartNumber,
        manufacturer: p.Manufacturer,
        description: p.Description,
        kind: String::new(),
        distributor: SOURCE_NAME.into(),
        sku: String::new(),
        stock,
        price_breaks,
        lifecycle: String::new(),
        datasheet_url: p.DataSheetUrl,
        product_url: p.ProductDetailUrl,
        retrieved_at,
    })
}

fn parse_availability(text: &str) -> u64 {
    text.chars()
        .filter(|c| c.is_ascii_digit())
        .collect::<String>()
        .parse()
        .unwrap_or(0)
}

fn parse_price(text: &str) -> Option<f64> {
    let cleaned: String = text
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    cleaned.parse().ok()
}

fn map_http_err(e: HttpError) -> SourceError {
    SourceError::Unavailable(e.to_string())
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
