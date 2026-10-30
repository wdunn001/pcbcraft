//! Offline-first part search: controlled tags, keyword index and optional vectors.
//!
//! Listings are indexed once; search ranks by meaning-like keyword overlap, exact
//! tag filters and numeric limits from [`PartFilter`]. Embeddings are optional:
//! when a part and the query both carry a vector, cosine similarity adds to the
//! score. Nothing here talks to the network or loads a model.
//!
//! Dependencies: [`PartListing`] and [`PartFilter`] in `listing.rs`.
//! Do not: guess fields the listing left empty or rank inside one source only.

use crate::listing::{Category, ListingSource, PartFilter, PartListing, Unit};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// On-disk and in-memory index format generation.
pub const INDEX_VERSION: u32 = 1;
/// Most parts one index may hold (matches listing adapters).
pub const MAX_INDEX_PARTS: usize = 5000;
/// Longest need text in bytes.
pub const MAX_NEED_BYTES: usize = 512;
/// Most tags a query may require at once.
pub const MAX_REQUIRED_TAGS: usize = 32;
/// Longest single tag string in bytes.
pub const MAX_TAG_BYTES: usize = 64;
/// Largest embedding dimension accepted.
pub const MAX_VECTOR_DIM: usize = 384;
/// Default page size when the caller gives no limit.
pub const DEFAULT_SEARCH_LIMIT: usize = 30;

/// One reason a part ranked for a need.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MatchReason {
    /// A query word appeared in the indexed text.
    Text { token: String },
    /// A query word matched a controlled or free tag.
    Tag { tag: String },
    /// A numeric or enum filter passed.
    Filter { field: String },
    /// Vector similarity contributed (only when both sides had a vector).
    Vector { similarity: f32 },
}

/// One ranked answer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartSearchHit {
    /// The part record.
    pub listing: PartListing,
    /// Combined score, higher is better.
    pub score: f32,
    /// Why this part matched, for chips and assistant text.
    pub reasons: Vec<MatchReason>,
}

/// Need in plain words plus optional filters.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PartSearchQuery {
    /// Stated need, for example "low-noise 3.3 V regulator in a small package".
    pub need: String,
    /// Every tag here must be present on the part.
    pub tags: Vec<String>,
    /// Numeric and enum limits; see [`PartFilter`].
    pub filter: PartFilter,
    /// Optional query embedding; must match stored dimension when used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query_vector: Option<Vec<f32>>,
}

/// Serialized index written beside a persistent cache.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartSearchIndexFile {
    /// [`INDEX_VERSION`] when the file was written.
    pub version: u32,
    /// Indexed parts with tags and optional vectors.
    pub entries: Vec<IndexedPartRecord>,
}

/// One stored index row.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IndexedPartRecord {
    pub listing: PartListing,
    pub tags: Vec<String>,
    pub tokens: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vector: Option<Vec<f32>>,
}

struct Entry {
    listing: PartListing,
    tags: BTreeSet<String>,
    tokens: BTreeSet<String>,
    vector: Option<Vec<f32>>,
}

/// In-memory inverted index over part listings.
#[derive(Clone, Debug, Default)]
pub struct PartSearchIndex {
    entries: Vec<Entry>,
    by_token: BTreeMap<String, Vec<usize>>,
    by_tag: BTreeMap<String, Vec<usize>>,
}

fn slug(text: &str) -> String {
    let lower = text.trim().to_lowercase();
    let mut out = String::new();
    let mut prev_dash = false;
    for c in lower.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
            prev_dash = false;
        } else if !prev_dash && !out.is_empty() {
            out.push('-');
            prev_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

fn tokenize(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    for c in text.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            current.push(c);
        } else if !current.is_empty() {
            if current.len() >= 2 {
                out.push(current.clone());
            }
            current.clear();
        }
    }
    if current.len() >= 2 {
        out.push(current);
    }
    out
}

fn category_tag(category: Category) -> &'static str {
    match category {
        Category::Resistor => "function:resistor",
        Category::Capacitor => "function:capacitor",
        Category::Inductor => "function:inductor",
        Category::Diode => "function:diode",
        Category::Led => "function:led",
        Category::Transistor => "function:transistor",
        Category::Mosfet => "function:mosfet",
        Category::IcRegulator => "function:regulator",
        Category::IcMcu => "function:mcu",
        Category::IcLogic => "function:logic",
        Category::IcAnalog => "function:analog",
        Category::Connector => "function:connector",
        Category::Crystal => "function:crystal",
        Category::Module => "function:module",
        Category::Mechanical => "function:mechanical",
        Category::Other => "function:other",
    }
}

fn voltage_class(v: f64) -> &'static str {
    if v <= 5.0 {
        "voltage:low"
    } else if v <= 24.0 {
        "voltage:medium"
    } else {
        "voltage:high"
    }
}

const INTERFACE_WORDS: &[(&str, &str)] = &[
    ("i2c", "interface:i2c"),
    ("i²c", "interface:i2c"),
    ("spi", "interface:spi"),
    ("uart", "interface:uart"),
    ("usb", "interface:usb"),
    ("can", "interface:can"),
    ("lin", "interface:lin"),
    ("ethernet", "interface:ethernet"),
    ("hdmi", "interface:hdmi"),
    ("lvds", "interface:lvds"),
];

/// Assign controlled tags and free tags from one listing.
///
/// # Rules
/// Category, package key, source, manufacturer, voltage class, RoHS and common
/// interface words become tags. Description and MPN words become free tags
/// prefixed with `free:`.
pub fn tags_for_listing(listing: &PartListing) -> Vec<String> {
    let mut tags = BTreeSet::new();
    tags.insert(category_tag(listing.category).to_string());
    if !listing.package_key.is_empty() {
        tags.insert(format!("package:{}", listing.package_key));
    }
    let source = match listing.source {
        ListingSource::Fixture => "source:fixture",
        ListingSource::Project => "source:project",
        ListingSource::Installed => "source:installed",
        ListingSource::Distributor => "source:distributor",
    };
    tags.insert(source.to_string());
    if !listing.manufacturer.is_empty() {
        tags.insert(format!("manufacturer:{}", slug(&listing.manufacturer)));
    }
    if let Some(v) = listing.rated_voltage_v {
        tags.insert(voltage_class(v).to_string());
    }
    if listing.rohs == Some(true) {
        tags.insert("cert:rohs".into());
    }
    let hay = format!(
        "{} {} {}",
        listing.description, listing.mpn, listing.source_category
    )
    .to_lowercase();
    for (word, tag) in INTERFACE_WORDS {
        if hay.contains(word) {
            tags.insert(tag.to_string());
        }
    }
    for token in tokenize(&hay) {
        if token.len() <= 32 {
            tags.insert(format!("free:{token}"));
        }
    }
    tags.into_iter().collect()
}

fn listing_document(listing: &PartListing) -> String {
    let mut parts = vec![
        listing.mpn.clone(),
        listing.manufacturer.clone(),
        listing.description.clone(),
        listing.source_ref.clone(),
        listing.package_text.clone(),
        listing.source_category.clone(),
    ];
    if let Some(v) = listing.value {
        parts.push(format!("{v}"));
    }
    if let Some(u) = listing.value_unit {
        let unit = match u {
            Unit::Ohm => "ohm",
            Unit::Farad => "farad",
            Unit::Henry => "henry",
            Unit::Hertz => "hertz",
            Unit::Volt => "volt",
            Unit::Ampere => "ampere",
            Unit::Watt => "watt",
        };
        parts.push(unit.to_string());
    }
    parts.join(" ")
}

fn check_vector(vector: &[f32]) -> Result<(), String> {
    if vector.is_empty() || vector.len() > MAX_VECTOR_DIM {
        return Err(format!(
            "a vector must have 1 to {MAX_VECTOR_DIM} components"
        ));
    }
    if vector.iter().any(|v| !v.is_finite()) {
        return Err("vector components must be finite".into());
    }
    Ok(())
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0f64;
    let mut na = 0.0f64;
    let mut nb = 0.0f64;
    for (x, y) in a.iter().zip(b.iter()) {
        let x = f64::from(*x);
        let y = f64::from(*y);
        dot += x * y;
        na += x * x;
        nb += y * y;
    }
    if na <= 0.0 || nb <= 0.0 {
        return 0.0;
    }
    (dot / (na.sqrt() * nb.sqrt())) as f32
}

impl PartSearchIndex {
    /// Empty index.
    pub fn new() -> Self {
        Self::default()
    }

    /// Build from listings and optional per-MPN vectors keyed by manufacturer part number.
    pub fn build(
        listings: &[PartListing],
        vectors: &BTreeMap<String, Vec<f32>>,
    ) -> Result<Self, String> {
        if listings.len() > MAX_INDEX_PARTS {
            return Err(format!(
                "at most {MAX_INDEX_PARTS} parts may be indexed at once"
            ));
        }
        let mut index = Self::new();
        for listing in listings {
            let vector = vectors.get(&listing.mpn).cloned();
            if let Some(ref v) = vector {
                check_vector(v)?;
            }
            index.push(listing.clone(), vector)?;
        }
        Ok(index)
    }

    fn push(&mut self, listing: PartListing, vector: Option<Vec<f32>>) -> Result<(), String> {
        if let Some(ref v) = vector {
            check_vector(v)?;
        }
        let tags: BTreeSet<String> = tags_for_listing(&listing).into_iter().collect();
        let tokens: BTreeSet<String> = tokenize(&listing_document(&listing)).into_iter().collect();
        let id = self.entries.len();
        for token in &tokens {
            self.by_token.entry(token.clone()).or_default().push(id);
        }
        for tag in &tags {
            self.by_tag.entry(tag.clone()).or_default().push(id);
        }
        self.entries.push(Entry {
            listing,
            tags,
            tokens,
            vector,
        });
        Ok(())
    }

    /// Load from a JSON file body.
    pub fn from_json(text: &str) -> Result<Self, String> {
        let file: PartSearchIndexFile =
            serde_json::from_str(text).map_err(|e| format!("index file: {e}"))?;
        if file.version != INDEX_VERSION {
            return Err(format!(
                "index version {} is not supported (want {INDEX_VERSION})",
                file.version
            ));
        }
        if file.entries.len() > MAX_INDEX_PARTS {
            return Err(format!(
                "at most {MAX_INDEX_PARTS} parts may be indexed at once"
            ));
        }
        let mut index = Self::new();
        for row in file.entries {
            if let Some(ref v) = row.vector {
                check_vector(v)?;
            }
            let tags: BTreeSet<String> = row.tags.into_iter().collect();
            let tokens: BTreeSet<String> = row.tokens.into_iter().collect();
            let id = index.entries.len();
            for token in &tokens {
                index.by_token.entry(token.clone()).or_default().push(id);
            }
            for tag in &tags {
                index.by_tag.entry(tag.clone()).or_default().push(id);
            }
            index.entries.push(Entry {
                listing: row.listing,
                tags,
                tokens,
                vector: row.vector,
            });
        }
        Ok(index)
    }

    /// Serialize to JSON for storage beside a cache.
    pub fn to_json(&self) -> Result<String, String> {
        let entries: Vec<IndexedPartRecord> = self
            .entries
            .iter()
            .map(|e| IndexedPartRecord {
                listing: e.listing.clone(),
                tags: e.tags.iter().cloned().collect(),
                tokens: e.tokens.iter().cloned().collect(),
                vector: e.vector.clone(),
            })
            .collect();
        let file = PartSearchIndexFile {
            version: INDEX_VERSION,
            entries,
        };
        serde_json::to_string_pretty(&file).map_err(|e| e.to_string())
    }

    /// How many parts the index holds.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the index is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Refuse an out-of-bounds or contradictory query.
    pub fn validate_query(query: &PartSearchQuery) -> Result<(), String> {
        if query.need.len() > MAX_NEED_BYTES {
            return Err(format!(
                "need text may be at most {MAX_NEED_BYTES} bytes"
            ));
        }
        if query.tags.len() > MAX_REQUIRED_TAGS {
            return Err(format!(
                "at most {MAX_REQUIRED_TAGS} required tags may be set"
            ));
        }
        for tag in &query.tags {
            if tag.is_empty() || tag.len() > MAX_TAG_BYTES {
                return Err(format!(
                    "each required tag must be 1 to {MAX_TAG_BYTES} bytes"
                ));
            }
        }
        if let Some(ref v) = query.query_vector {
            check_vector(v)?;
        }
        query.filter.validate()
    }

    /// Rank parts for a need. Returns at most `limit` hits (1 to 100).
    pub fn search(&self, query: &PartSearchQuery, limit: usize) -> Result<Vec<PartSearchHit>, String> {
        Self::validate_query(query)?;
        if limit == 0 || limit > 100 {
            return Err("limit must be an integer from 1 to 100".into());
        }
        let need_tokens: Vec<String> = tokenize(&query.need);
        let required: BTreeSet<String> = query.tags.iter().cloned().collect();

        let mut candidates: BTreeSet<usize> = BTreeSet::new();
        if required.is_empty() && need_tokens.is_empty() {
            candidates.extend(0..self.entries.len());
        } else if !required.is_empty() {
            let mut sets: Option<BTreeSet<usize>> = None;
            for tag in &required {
                let Some(postings) = self.by_tag.get(tag) else {
                    return Ok(vec![]);
                };
                let set: BTreeSet<usize> = postings.iter().copied().collect();
                sets = Some(match sets {
                    None => set,
                    Some(prev) => prev.intersection(&set).copied().collect(),
                });
            }
            if let Some(set) = sets {
                candidates = set;
            }
        } else {
            for token in &need_tokens {
                if let Some(postings) = self.by_token.get(token) {
                    candidates.extend(postings.iter().copied());
                }
                if let Some(postings) = self.by_tag.get(token) {
                    candidates.extend(postings.iter().copied());
                }
                let free = format!("free:{token}");
                if let Some(postings) = self.by_tag.get(&free) {
                    candidates.extend(postings.iter().copied());
                }
            }
            if candidates.is_empty() {
                candidates.extend(0..self.entries.len());
            }
        }

        let mut scored: Vec<PartSearchHit> = Vec::new();
        let query_len = need_tokens.len().max(1) as f32;

        for id in candidates {
            let Some(entry) = self.entries.get(id) else {
                continue;
            };
            if !required.is_empty() && !required.is_subset(&entry.tags) {
                continue;
            }
            if !query.filter.matches(&entry.listing) {
                continue;
            }

            let mut reasons = Vec::new();
            let mut text_hits = 0usize;
            for token in &need_tokens {
                if entry.tokens.contains(token) {
                    text_hits += 1;
                    reasons.push(MatchReason::Text {
                        token: token.clone(),
                    });
                } else if entry.tags.contains(token) || entry.tags.contains(&format!("free:{token}"))
                {
                    reasons.push(MatchReason::Tag {
                        tag: token.clone(),
                    });
                } else if entry.tags.iter().any(|t| t.ends_with(token) || t.contains(token)) {
                    reasons.push(MatchReason::Tag {
                        tag: token.clone(),
                    });
                }
            }
            let text_score = text_hits as f32 / query_len;

            let tag_hits = need_tokens
                .iter()
                .filter(|t| {
                    entry.tags.contains(*t)
                        || entry.tags.contains(&format!("free:{t}"))
                        || entry
                            .tags
                            .iter()
                            .any(|tag| tag.contains(t.as_str()))
                })
                .count();
            let tag_score = tag_hits as f32 / query_len;

            let mut score = text_score + 0.35 * tag_score;
            if query.filter != PartFilter::default() {
                reasons.push(MatchReason::Filter {
                    field: "filter".into(),
                });
                score += 0.05;
            }

            if let (Some(qv), Some(pv)) = (&query.query_vector, &entry.vector) {
                if qv.len() == pv.len() {
                    let sim = cosine(qv, pv);
                    if sim > 0.0 {
                        reasons.push(MatchReason::Vector { similarity: sim });
                        score += 0.4 * sim;
                    }
                }
            }

            if score > 0.0 || (need_tokens.is_empty() && required.is_empty()) {
                scored.push(PartSearchHit {
                    listing: entry.listing.clone(),
                    score,
                    reasons,
                });
            }
        }

        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.listing.mpn.cmp(&b.listing.mpn))
        });
        scored.truncate(limit);
        Ok(scored)
    }
}

/// Recall at k: fraction of needs whose expected MPN appears in the top k hits.
pub fn recall_at_k(index: &PartSearchIndex, cases: &[(&str, &str)], k: usize) -> f32 {
    if cases.is_empty() {
        return 0.0;
    }
    let mut hits = 0usize;
    for (need, mpn) in cases {
        let query = PartSearchQuery {
            need: (*need).to_string(),
            ..PartSearchQuery::default()
        };
        let Ok(found) = index.search(&query, k) else {
            continue;
        };
        if found.iter().any(|h| h.listing.mpn == *mpn) {
            hits += 1;
        }
    }
    hits as f32 / cases.len() as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::listing::{Lifecycle, ListingSource};

    fn sample_listing(mpn: &str, description: &str, category: Category) -> PartListing {
        PartListing {
            source: ListingSource::Fixture,
            source_id: "sample".into(),
            source_ref: mpn.into(),
            manufacturer: "Example".into(),
            mpn: mpn.into(),
            description: description.into(),
            category,
            source_category: String::new(),
            package_key: "chip-0603".into(),
            package_text: "0603".into(),
            value: None,
            value_unit: None,
            tolerance_pct: None,
            rated_voltage_v: Some(3.3),
            temp_min_c: None,
            temp_max_c: None,
            power_w: None,
            stock: Some(100),
            stock_at: None,
            price_breaks: vec![],
            min_order_qty: None,
            order_multiple: None,
            packaging: None,
            lifecycle: Lifecycle::Active,
            lifecycle_raw: String::new(),
            rohs: Some(true),
            datasheet_url: None,
            symbol: None,
            footprint: None,
            model3d: None,
            attribution: None,
        }
    }

    #[test]
    fn tags_cover_category_package_and_interfaces() {
        let listing = sample_listing(
            "REG-3V3",
            "Low noise 3.3 V LDO regulator, I2C enable",
            Category::IcRegulator,
        );
        let tags = tags_for_listing(&listing);
        assert!(tags.iter().any(|t| t == "function:regulator"));
        assert!(tags.iter().any(|t| t == "package:chip-0603"));
        assert!(tags.iter().any(|t| t == "interface:i2c"));
        assert!(tags.iter().any(|t| t == "cert:rohs"));
    }

    #[test]
    fn search_ranks_regulator_need_above_resistor() {
        let listings = vec![
            sample_listing("R-10K", "10k resistor 0603", Category::Resistor),
            sample_listing(
                "LDO-3V3",
                "Low noise 3.3 V linear regulator small package",
                Category::IcRegulator,
            ),
        ];
        let index = PartSearchIndex::build(&listings, &BTreeMap::new()).expect("index builds");
        let query = PartSearchQuery {
            need: "low noise 3.3 V regulator small package".into(),
            ..Default::default()
        };
        let hits = index.search(&query, 5).expect("search");
        assert_eq!(hits.first().map(|h| h.listing.mpn.as_str()), Some("LDO-3V3"));
        assert!(hits[0].score >= hits.get(1).map(|h| h.score).unwrap_or(0.0));
    }

    #[test]
    fn required_tags_narrow_to_one_family() {
        let listings = vec![
            sample_listing("R-1", "resistor", Category::Resistor),
            sample_listing("C-1", "capacitor", Category::Capacitor),
        ];
        let index = PartSearchIndex::build(&listings, &BTreeMap::new()).expect("index");
        let query = PartSearchQuery {
            need: String::new(),
            tags: vec!["function:capacitor".into()],
            ..Default::default()
        };
        let hits = index.search(&query, 10).expect("search");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].listing.mpn, "C-1");
    }

    #[test]
    fn index_round_trips_through_json() {
        let listings = vec![sample_listing("X", "part", Category::Other)];
        let index = PartSearchIndex::build(&listings, &BTreeMap::new()).expect("index");
        let json = index.to_json().expect("json");
        let back = PartSearchIndex::from_json(&json).expect("parse");
        assert_eq!(back.len(), 1);
    }

    #[test]
    fn recall_at_k_on_thirty_needs_meets_floor() {
        let mut listings = Vec::new();
        let mut needs = Vec::new();
        for i in 0..30 {
            let mpn = format!("MPN-{i:03}");
            let need = match i % 5 {
                0 => format!("{mpn} low noise 3.3 V regulator"),
                1 => format!("{mpn} 10k resistor 0603"),
                2 => format!("{mpn} 100nF capacitor 16V"),
                3 => format!("{mpn} USB connector"),
                _ => format!("{mpn} microcontroller I2C"),
            };
            let category = match i % 5 {
                0 => Category::IcRegulator,
                1 => Category::Resistor,
                2 => Category::Capacitor,
                3 => Category::Connector,
                _ => Category::IcMcu,
            };
            needs.push(need.clone());
            listings.push(sample_listing(&mpn, &need, category));
        }
        let case_refs: Vec<(&str, &str)> = needs
            .iter()
            .zip(listings.iter())
            .map(|(need, listing)| (need.as_str(), listing.mpn.as_str()))
            .collect();
        let index = PartSearchIndex::build(&listings, &BTreeMap::new()).expect("index");
        let recall = recall_at_k(&index, &case_refs, 3);
        assert!(
            recall >= 0.85,
            "recall@3 should stay high on synthetic needs, got {recall}"
        );
    }
}
