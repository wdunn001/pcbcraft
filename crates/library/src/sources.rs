//! Online part sources behind one trait. A source answers a part search with
//! stock, prices, lifecycle and a datasheet link. This module holds the trait,
//! the result record, a short-lived response cache and a source that reads
//! recorded answers from a JSON file. It makes no network calls and holds no
//! keys; live distributor sources implement the same trait elsewhere.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Longest query a source is asked to answer.
pub const MAX_QUERY_BYTES: usize = 128;
/// Most results one search returns.
pub const MAX_RESULTS: usize = 50;
/// Most answers the cache keeps at once.
pub const MAX_CACHE_ENTRIES: usize = 256;
/// Default life of a cached answer: 24 hours.
pub const DEFAULT_CACHE_SECS: u64 = 24 * 60 * 60;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PriceBreak {
    pub quantity: u32,
    pub unit_price: f64,
    pub currency: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartResult {
    /// Name of the source that answered, shown as the badge on the card.
    pub source: String,
    pub mpn: String,
    pub manufacturer: String,
    #[serde(default)]
    pub description: String,
    /// Generic part kind the schematic can place, such as "resistor"; empty
    /// when the source does not say.
    #[serde(default)]
    pub kind: String,
    /// Seller that holds the stock, for example the distributor's name.
    #[serde(default)]
    pub distributor: String,
    #[serde(default)]
    pub sku: String,
    #[serde(default)]
    pub stock: u64,
    #[serde(default)]
    pub price_breaks: Vec<PriceBreak>,
    #[serde(default)]
    pub lifecycle: String,
    #[serde(default)]
    pub datasheet_url: String,
    #[serde(default)]
    pub product_url: String,
    /// Seconds since the Unix epoch when the source gave this answer.
    pub retrieved_at: u64,
}

/// What a person has to do before a live source works for them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetupCard {
    pub steps: Vec<String>,
    pub portal_url: String,
    /// Names of the values the person pastes into preferences.
    pub fields: Vec<String>,
}

/// Credit the source asks to see next to its data.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attribution {
    pub text: String,
    pub url: String,
}

#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum SourceError {
    #[error("{0}")]
    Invalid(String),
    #[error("no part found for {0}")]
    NotFound(String),
    #[error("the source asks for a pause of {retry_after_secs} seconds")]
    RateLimited { retry_after_secs: u64 },
    #[error("{0}")]
    Unavailable(String),
}

/// Sources answer part searches with stock, prices, lifecycle and datasheet
/// links. Implementations may be live (network) or recorded (file). Every
/// implementation must be thread-safe and return bounded results.
///
/// # Rules
/// - `id()` returns a short stable string used as the cache key.
/// - `search()` returns at most `MAX_RESULTS` entries.
/// - `fetch()` returns `NotFound` when the part number is not recognized.
/// - Error variants: `Invalid` (bad input), `NotFound` (no match),
///   `RateLimited` (back off), `Unavailable` (service down).
///
/// # Dependencies
/// - crates/library/src/sources.rs `PartResult` (this module): the record returned by searches.
/// - crates/library/src/sources.rs `SourceError` (this module): error types.
/// - crates/library/src/sources.rs `SetupCard` (this module): setup instructions shown to the user.
/// - crates/library/src/sources.rs `Attribution` (this module): credit text and link.
///
/// # Implementations
/// - crates/library/src/sources.rs `FixtureSource` (this module): recorded answers from JSON files.
/// - crates/library/src/sources.rs Live distributor sources: separate crates that implement this trait and
///   are discovered through the source registry.
///
/// # Do not
/// - Return unbounded result lists; truncate to `MAX_RESULTS`.
/// - Store mutable state without synchronization; the trait requires `Send + Sync`.
/// - Return `Invalid` for a part number that simply has no match; use `NotFound`.
pub trait PartSource: Send + Sync {
    /// Short stable identifier used as the cache key.
    fn id(&self) -> &str;
    /// Name shown on cards.
    fn name(&self) -> &str;
    /// Search by keyword or part number.
    fn search(&self, query: &str) -> Result<Vec<PartResult>, SourceError>;
    /// Fetch the single part with exactly this part number.
    fn fetch(&self, mpn: &str) -> Result<PartResult, SourceError>;
    fn setup(&self) -> SetupCard;
    fn attribution(&self) -> Attribution;
    /// The source's own attribute names and values for one part number, such
    /// as `Resistance` and `Package / Case`, for `listing::listing_from_result`.
    /// Empty when the source has none.
    fn attributes(&self, _mpn: &str) -> BTreeMap<String, String> {
        BTreeMap::new()
    }
    /// Seconds a cached answer from this source stays usable.
    fn cache_secs(&self) -> u64 {
        DEFAULT_CACHE_SECS
    }
    /// Most searches per second this source allows. Pacing stays below this.
    fn requests_per_sec(&self) -> f64 {
        4.0
    }
}

/// Reject queries a source should never see.
pub fn check_query(query: &str) -> Result<&str, SourceError> {
    let q = query.trim();
    if q.is_empty() {
        return Err(SourceError::Invalid("type a part number or keyword".into()));
    }
    if q.len() > MAX_QUERY_BYTES || q.chars().any(char::is_control) {
        return Err(SourceError::Invalid(
            "the search text is too long or has control characters".into(),
        ));
    }
    Ok(q)
}

struct CacheEntry {
    stored_at: u64,
    results: Vec<PartResult>,
}

/// Short-lived answers keyed by source and request. It is a response cache
/// only: entries expire and nothing is merged into a catalogue.
#[derive(Default)]
pub struct ResponseCache {
    entries: BTreeMap<String, CacheEntry>,
    /// How many times a source was actually asked, for status lines and tests.
    pub misses: u64,
}

impl ResponseCache {
    fn key(source: &str, query: &str) -> String {
        format!("{source}\u{1f}search\u{1f}{}", query.trim().to_lowercase())
    }
    /// Return a still-fresh memory entry without asking the source.
    pub fn get_if_fresh(
        &self,
        source: &dyn PartSource,
        query: &str,
        now: u64,
    ) -> Option<Vec<PartResult>> {
        let query = check_query(query).ok()?;
        let key = Self::key(source.id(), query);
        let entry = self.entries.get(&key)?;
        (now.saturating_sub(entry.stored_at) < source.cache_secs())
            .then(|| entry.results.clone())
    }

    /// Store an answer the caller already fetched.
    pub fn remember(
        &mut self,
        source: &dyn PartSource,
        query: &str,
        now: u64,
        results: Vec<PartResult>,
    ) -> Result<(), SourceError> {
        let query = check_query(query)?;
        let key = Self::key(source.id(), query);
        if self.entries.len() >= MAX_CACHE_ENTRIES
            && let Some(oldest) = self
                .entries
                .iter()
                .min_by_key(|(_, e)| e.stored_at)
                .map(|(k, _)| k.clone())
        {
            self.entries.remove(&oldest);
        }
        self.entries.insert(
            key,
            CacheEntry {
                stored_at: now,
                results,
            },
        );
        Ok(())
    }

    /// Answer from the cache while the entry is younger than the source's
    /// window, otherwise ask the source and store the answer. `now` is
    /// seconds since the Unix epoch.
    pub fn search(
        &mut self,
        source: &dyn PartSource,
        query: &str,
        now: u64,
    ) -> Result<Vec<PartResult>, SourceError> {
        let query = check_query(query)?;
        if let Some(hit) = self.get_if_fresh(source, query, now) {
            return Ok(hit);
        }
        self.misses += 1;
        let key = Self::key(source.id(), query);
        let mut results = source.search(query)?;
        results.truncate(MAX_RESULTS);
        if self.entries.len() >= MAX_CACHE_ENTRIES
            && let Some(oldest) = self
                .entries
                .iter()
                .min_by_key(|(_, e)| e.stored_at)
                .map(|(k, _)| k.clone())
        {
            self.entries.remove(&oldest);
        }
        self.entries.insert(
            key,
            CacheEntry {
                stored_at: now,
                results: results.clone(),
            },
        );
        Ok(results)
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    /// Forget every stored answer.
    pub fn purge(&mut self) {
        self.entries.clear();
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FixtureFile {
    name: String,
    #[serde(default)]
    #[allow(dead_code)]
    note: String,
    parts: Vec<PartResult>,
}

/// A source that answers from recorded JSON. It needs no network and no key.
pub struct FixtureSource {
    name: String,
    parts: Vec<PartResult>,
    /// Lowercase part number to the recorded attribute map.
    attributes: BTreeMap<String, BTreeMap<String, String>>,
}

impl FixtureSource {
    /// Read a recorded answer set. Bounded: at most 1000 parts.
    pub fn from_json(text: &str) -> Result<Self, SourceError> {
        if text.len() > 4_000_000 {
            return Err(SourceError::Invalid(
                "the recorded file is too large".into(),
            ));
        }
        let file: FixtureFile = serde_json::from_str(text)
            .map_err(|e| SourceError::Invalid(format!("the recorded file is not valid: {e}")))?;
        if file.parts.len() > 1000 {
            return Err(SourceError::Invalid("too many recorded parts".into()));
        }
        Ok(Self {
            name: file.name,
            parts: file.parts,
            attributes: BTreeMap::new(),
        })
    }

    /// Add recorded attributes: a JSON object mapping a part number to a flat
    /// object of text values. Bounded like [`FixtureSource::from_json`].
    pub fn with_attributes(mut self, json: &str) -> Result<Self, SourceError> {
        if json.len() > 4_000_000 {
            return Err(SourceError::Invalid(
                "the recorded attributes file is too large".into(),
            ));
        }
        let map: BTreeMap<String, BTreeMap<String, String>> = serde_json::from_str(json)
            .map_err(|e| SourceError::Invalid(format!("the attributes file is not valid: {e}")))?;
        if map.len() > 1000 {
            return Err(SourceError::Invalid("too many recorded attributes".into()));
        }
        self.attributes = map
            .into_iter()
            .map(|(mpn, attrs)| (mpn.to_lowercase(), attrs))
            .collect();
        Ok(self)
    }
}

impl PartSource for FixtureSource {
    // Contract: id() returns "fixture", search() returns at most MAX_RESULTS
    // entries matching the query, fetch() returns NotFound when the part
    // number is not in the recorded set.
    fn id(&self) -> &str {
        "fixture"
    }
    fn name(&self) -> &str {
        &self.name
    }
    fn search(&self, query: &str) -> Result<Vec<PartResult>, SourceError> {
        let q = check_query(query)?.to_lowercase();
        Ok(self
            .parts
            .iter()
            .filter(|p| {
                p.mpn.to_lowercase().contains(&q)
                    || p.manufacturer.to_lowercase().contains(&q)
                    || p.description.to_lowercase().contains(&q)
            })
            .take(MAX_RESULTS)
            .cloned()
            .collect())
    }
    fn fetch(&self, mpn: &str) -> Result<PartResult, SourceError> {
        let q = check_query(mpn)?;
        self.parts
            .iter()
            .find(|p| p.mpn.eq_ignore_ascii_case(q))
            .cloned()
            .ok_or_else(|| SourceError::NotFound(q.to_owned()))
    }
    fn attributes(&self, mpn: &str) -> BTreeMap<String, String> {
        self.attributes
            .get(&mpn.trim().to_lowercase())
            .cloned()
            .unwrap_or_default()
    }
    fn setup(&self) -> SetupCard {
        SetupCard {
            steps: vec!["Nothing to set up. These parts are samples.".into()],
            portal_url: String::new(),
            fields: vec![],
        }
    }
    fn attribution(&self) -> Attribution {
        Attribution {
            text: "Sample data, not a real catalogue".into(),
            url: String::new(),
        }
    }
}
