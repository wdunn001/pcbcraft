//! Live part source for the JLCPCB Open Platform Components API.

use crate::credentials::{self, KeyStore};
use crate::http_client::{HttpClient, HttpError, HttpRequest, HttpResponse};
use crate::rate_limit::{Clock, SystemClock, TokenBucket, rate_limited};
use crate::source_errors::map_auth_or_permission;
use crate::source_hosts;
use crate::source_sign::jlcpcb_authorization;
use crate::sources::{
    Attribution, PartResult, PartSource, PriceBreak, SetupCard, SourceError, check_query,
    DEFAULT_CACHE_SECS,
};
use serde::Deserialize;
use std::sync::{Arc, Mutex};

/// Cache key segment for this source.
pub const SOURCE_ID: &str = "jlcpcb";
const SOURCE_NAME: &str = "JLCPCB";
/// Documented component detail endpoint (Components API).
pub const COMPONENT_DETAIL_URL: &str =
    "https://open.jlcpcb.com/overseas/openapi/component/getComponentDetailByCode";
const REQUESTS_PER_MINUTE: u64 = 30;

/// Detail lookup by LCSC part code through the approved Components API.
pub struct JlcpcbSource {
    http: Arc<dyn HttpClient>,
    keys: Arc<dyn KeyStore>,
    bucket: Mutex<TokenBucket>,
    clock: Arc<dyn Clock>,
}

impl JlcpcbSource {
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

    fn credentials(&self) -> Result<(String, String, String), SourceError> {
        let app_id = self
            .keys
            .get(credentials::keys::JLCPCB_APP_ID)
            .map_err(map_cred_err)?
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| SourceError::Unavailable("add your JLCPCB app id in setup".into()))?;
        let access = self
            .keys
            .get(credentials::keys::JLCPCB_ACCESS_KEY)
            .map_err(map_cred_err)?
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| {
                SourceError::Unavailable("add your JLCPCB access key in setup".into())
            })?;
        let secret = self
            .keys
            .get(credentials::keys::JLCPCB_SECRET_KEY)
            .map_err(map_cred_err)?
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| {
                SourceError::Unavailable("add your JLCPCB secret key in setup".into())
            })?;
        Ok((app_id, access, secret))
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

    fn normalize_lcsc_code(query: &str) -> Option<String> {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return None;
        }
        let upper = trimmed.to_ascii_uppercase();
        if upper.starts_with('C') && upper[1..].chars().all(|c| c.is_ascii_digit()) {
            return Some(upper);
        }
        if trimmed.chars().all(|c| c.is_ascii_digit()) {
            return Some(format!("C{trimmed}"));
        }
        None
    }

    fn detail_live(&self, code: &str) -> Result<Vec<PartResult>, SourceError> {
        if !source_hosts::url_uses_documented_host(COMPONENT_DETAIL_URL) {
            return Err(SourceError::Unavailable(
                "the component address is not allowed".into(),
            ));
        }
        let (app_id, access, secret) = self.credentials()?;
        self.take_token()?;
        let body = format!(r#"{{"componentCodes":["{}"]}}"#, escape_json(code));
        let nonce = random_nonce();
        let timestamp = self.clock.now_secs();
        let auth = jlcpcb_authorization(
            &app_id,
            &access,
            &secret,
            "POST",
            COMPONENT_DETAIL_URL,
            &body,
            &nonce,
            timestamp,
        );
        let mut headers = std::collections::BTreeMap::new();
        headers.insert("Content-Type".into(), "application/json".into());
        headers.insert("Authorization".into(), auth);
        let response = self
            .http
            .call(&HttpRequest {
                method: "POST".into(),
                url: COMPONENT_DETAIL_URL.into(),
                headers,
                body: body.into_bytes(),
            })
            .map_err(map_http_err)?;
        if response.status == 429 {
            apply_retry_after(&response, &self.bucket)?;
        }
        if let Some(err) = map_auth_or_permission(response.status, None) {
            return Err(err);
        }
        let envelope: JlcpcbEnvelope = serde_json::from_slice(&response.body).map_err(|e| {
            SourceError::Unavailable(format!("the detail answer was not valid: {e}"))
        })?;
        if let Some(err) = map_auth_or_permission(200, Some(envelope.code)) {
            return Err(err);
        }
        if envelope.code != 200 {
            return Err(SourceError::Unavailable(
                envelope
                    .message
                    .unwrap_or_else(|| "the component detail call failed".into()),
            ));
        }
        let now = self.clock.now_secs();
        Ok(envelope
            .data
            .and_then(|d| d.component_list)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|p| part_from_component(p, now))
            .collect())
    }

    fn search_live(&self, query: &str) -> Result<Vec<PartResult>, SourceError> {
        let code = Self::normalize_lcsc_code(query)
            .ok_or_else(|| SourceError::NotFound(query.to_owned()))?;
        self.detail_live(&code)
    }
}

impl PartSource for JlcpcbSource {
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
                "Apply for API access on the JLCPCB API platform.".into(),
                "After review, create an app and copy the app id, access key, and secret key."
                    .into(),
                "Paste all three values below and press Test.".into(),
            ],
            portal_url: "https://api.jlcpcb.com/".into(),
            fields: vec![
                credentials::keys::JLCPCB_APP_ID.into(),
                credentials::keys::JLCPCB_ACCESS_KEY.into(),
                credentials::keys::JLCPCB_SECRET_KEY.into(),
            ],
        }
    }
    fn attribution(&self) -> Attribution {
        Attribution {
            text: "Pricing and availability from JLCPCB".into(),
            url: "https://jlcpcb.com/".into(),
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
struct JlcpcbEnvelope {
    #[serde(default)]
    code: i64,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    data: Option<JlcpcbData>,
}

#[derive(Deserialize)]
struct JlcpcbData {
    #[serde(default, alias = "componentList")]
    component_list: Option<Vec<ComponentJson>>,
}

#[derive(Deserialize)]
struct ComponentJson {
    #[serde(default, alias = "lcscCode")]
    lcsc_code: String,
    #[serde(default, alias = "componentCode")]
    component_code: String,
    #[serde(default)]
    mpn: String,
    #[serde(default, alias = "componentModelEn")]
    component_model_en: String,
    #[serde(default)]
    manufacturer: String,
    #[serde(default, alias = "brandNameEn")]
    brand_name_en: String,
    #[serde(default)]
    description: String,
    #[serde(default, alias = "describeEn")]
    describe_en: String,
    #[serde(default)]
    stock: u64,
    #[serde(default, alias = "stockNumber")]
    stock_number: u64,
    #[serde(default)]
    price_list: Vec<PriceJson>,
    #[serde(default, alias = "componentPriceList")]
    component_price_list: Vec<PriceJson>,
    #[serde(default)]
    datasheet: String,
    #[serde(default, alias = "dataSheetUrl")]
    data_sheet_url: String,
    #[serde(default)]
    product_url: String,
}

#[derive(Deserialize)]
struct PriceJson {
    #[serde(default)]
    ladder: u32,
    #[serde(default, alias = "startNumber")]
    start_number: u32,
    #[serde(default)]
    price: f64,
    #[serde(default, alias = "componentPrice")]
    component_price: f64,
    #[serde(default)]
    currency: String,
}

fn part_from_component(p: ComponentJson, retrieved_at: u64) -> Option<PartResult> {
    let sku = if p.lcsc_code.is_empty() {
        p.component_code
    } else {
        p.lcsc_code
    };
    if sku.is_empty() {
        return None;
    }
    let mpn = if !p.mpn.is_empty() {
        p.mpn
    } else if !p.component_model_en.is_empty() {
        p.component_model_en
    } else {
        sku.clone()
    };
    let prices = if p.price_list.is_empty() {
        p.component_price_list
    } else {
        p.price_list
    };
    let price_breaks = prices
        .into_iter()
        .map(|b| {
            let qty = if b.ladder > 0 {
                b.ladder
            } else {
                b.start_number
            };
            let unit = if b.price > 0.0 {
                b.price
            } else {
                b.component_price
            };
            PriceBreak {
                quantity: qty.max(1),
                unit_price: unit,
                currency: if b.currency.is_empty() {
                    "USD".into()
                } else {
                    b.currency
                },
            }
        })
        .collect();
    let manufacturer = if p.manufacturer.is_empty() {
        p.brand_name_en
    } else {
        p.manufacturer
    };
    let description = if p.description.is_empty() {
        p.describe_en
    } else {
        p.description
    };
    let datasheet = if p.datasheet.is_empty() {
        p.data_sheet_url
    } else {
        p.datasheet
    };
    let stock = if p.stock > 0 { p.stock } else { p.stock_number };
    Some(PartResult {
        source: SOURCE_NAME.into(),
        mpn,
        manufacturer,
        description,
        kind: String::new(),
        distributor: SOURCE_NAME.into(),
        sku: sku.clone(),
        stock,
        price_breaks,
        lifecycle: String::new(),
        datasheet_url: datasheet,
        product_url: if p.product_url.is_empty() {
            format!("https://jlcpcb.com/partdetail/{sku}")
        } else {
            p.product_url
        },
        retrieved_at,
    })
}

fn random_nonce() -> String {
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{:032x}", (seed as u128) ^ (seed >> 32) as u128)
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
