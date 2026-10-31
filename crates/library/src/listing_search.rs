//! One search over every place a part can come from: the open project, the
//! installed libraries, recorded sample parts, the open online library and
//! live distributors. Each source answers in its own shape; this file turns
//! the answers into [`PartListing`] records, merges them into one list, drops
//! repeats of the same manufacturer part number and ranks what is left.
//!
//! # Rules
//! - No network, no keys, no files here. Providers wrap sources that already
//!   exist (`PartSource`, `OpenLibrarySource`) or symbols the caller parsed.
//! - Ranking is exact part number, then package match, then name; ties go to
//!   the nearer source (project, installed, sample, open library, distributor).
//! - One source failing (offline, rate limited) never hides the others; the
//!   failure is reported beside the hits.
//! - A listing keeps its source, licence, fetch time and a stable id.
//!
//! # Dependencies
//! - crates/library/src/listing.rs
//! - crates/library/src/listing_adapters.rs
//! - crates/library/src/sources.rs
//! - crates/library/src/open_library.rs
use crate::SymbolDefinition;
use crate::listing::{ListingSource, PartListing, listing_from_result, package_key};
use crate::listing_adapters::listing_from_symbol;
use crate::open_library::{CardScope, OpenLibrarySource};
use crate::sources::{MAX_RESULTS, PartSource, SourceError, check_query};
use std::collections::BTreeMap;
use std::sync::Arc;

/// Most hits one search returns.
pub const MAX_HITS: usize = 200;

/// One listing as a provider found it, with where it came from.
#[derive(Clone, Debug, PartialEq)]
pub struct Found {
    /// The normalized record.
    pub listing: PartListing,
    /// Licence line to show with the part; empty when the source gave none.
    pub licence: String,
    /// Seconds since the Unix epoch when the source gave this answer; empty
    /// for parts read from the open project, which are never stale.
    pub fetched_at: Option<u64>,
}

/// How well a hit matches the query, best first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Rank {
    /// The part number is the query.
    ExactMpn,
    /// A word of the query names the part's package.
    Package,
    /// The part number starts with the query.
    NamePrefix,
    /// The part number or description holds the query.
    NameContains,
    /// The source returned it for another reason.
    Other,
}

/// One line of the merged list.
#[derive(Clone, Debug, PartialEq)]
pub struct Hit {
    /// Stable across searches: kind, source name and the source's own key.
    pub id: String,
    /// The record shown; empty fields are filled from the other sources that
    /// listed the same part number.
    pub listing: PartListing,
    /// Licence line of the source shown.
    pub licence: String,
    /// When the source gave this answer, if it says.
    pub fetched_at: Option<u64>,
    /// Seconds between `fetched_at` and the time of the search.
    pub age_secs: Option<u64>,
    /// How the hit matched.
    pub rank: Rank,
    /// Ids of the same part in other sources, dropped as repeats.
    pub also_in: Vec<String>,
}

/// A source that could not answer, in words a person can read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceFailure {
    /// The provider's label.
    pub source: String,
    /// What went wrong.
    pub message: String,
    /// True when the source could not be reached at all.
    pub offline: bool,
}

/// The merged answer.
#[derive(Clone, Debug, PartialEq)]
pub struct SearchOutcome {
    /// Ranked hits, at most the limit asked for.
    pub hits: Vec<Hit>,
    /// Sources that failed; the hits come from the rest.
    pub failures: Vec<SourceFailure>,
    /// How many hits there were before the limit cut the list.
    pub total_known: usize,
}

/// A place parts can be searched.
///
/// # Rules
/// - `search` returns at most [`MAX_RESULTS`] records and never blocks on a
///   network call the caller did not ask for.
/// - `now` is seconds since the Unix epoch, passed in so tests are exact.
///
/// # Implementations
/// - crates/library/src/listing_search.rs `SymbolProvider`
/// - crates/library/src/listing_search.rs `SourceProvider`
/// - crates/library/src/listing_search.rs `OpenLibraryProvider`
pub trait ListingProvider: Send + Sync {
    /// Name shown when this source fails.
    fn label(&self) -> String;
    /// Listings for a checked, non-empty query.
    fn search(&self, query: &str, now: u64) -> Result<Vec<Found>, SourceError>;
}

fn kind_name(source: ListingSource) -> &'static str {
    match source {
        ListingSource::Fixture => "fixture",
        ListingSource::Project => "project",
        ListingSource::Installed => "installed",
        ListingSource::OpenLibrary => "open_library",
        ListingSource::Distributor => "distributor",
    }
}

/// The id of a listing: `kind/source/ref`. Two searches that find the same
/// part in the same source give the same id.
pub fn stable_id(listing: &PartListing) -> String {
    format!(
        "{}/{}/{}",
        kind_name(listing.source),
        listing.source_id,
        listing.source_ref
    )
}

/// Nearer sources win ties and supply the shown record.
fn priority(source: ListingSource) -> u8 {
    match source {
        ListingSource::Project => 0,
        ListingSource::Installed => 1,
        ListingSource::Fixture => 2,
        ListingSource::OpenLibrary => 3,
        ListingSource::Distributor => 4,
    }
}

/// The part number as a repeat key, or `None` when the listing has no real one.
///
/// Symbols from a library carry their own name as the part number, which is
/// not a manufacturer part number, so a listing counts only when it names a
/// manufacturer or comes from a sample or distributor source.
/// TODO(#611): key on the typed manufacturer part number field once
/// feat-part-fields-227 is on main, and on the manufacturer as well.
fn mpn_key(listing: &PartListing) -> Option<String> {
    let mpn = listing.mpn.split_whitespace().collect::<Vec<_>>().join(" ");
    let real = !listing.manufacturer.trim().is_empty()
        || matches!(
            listing.source,
            ListingSource::Fixture | ListingSource::Distributor
        );
    (!mpn.is_empty() && real).then(|| mpn.to_lowercase())
}

fn rank_of(query: &str, listing: &PartListing) -> Rank {
    let q = query.trim().to_lowercase();
    let mpn = listing.mpn.trim().to_lowercase();
    if !q.is_empty() && mpn == q {
        return Rank::ExactMpn;
    }
    if !listing.package_key.is_empty()
        && q.split_whitespace()
            .any(|w| package_key(w).key == listing.package_key)
    {
        return Rank::Package;
    }
    if !q.is_empty() && mpn.starts_with(&q) {
        return Rank::NamePrefix;
    }
    if !q.is_empty() && (mpn.contains(&q) || listing.description.to_lowercase().contains(&q)) {
        return Rank::NameContains;
    }
    Rank::Other
}

/// Fill the empty fields of `base` from `other`, never replacing a given one.
fn fill(base: &mut PartListing, other: &PartListing) {
    if base.manufacturer.is_empty() {
        base.manufacturer.clone_from(&other.manufacturer);
    }
    if base.description.is_empty() {
        base.description.clone_from(&other.description);
    }
    if base.package_key.is_empty() && base.package_text.is_empty() {
        base.package_key.clone_from(&other.package_key);
        base.package_text.clone_from(&other.package_text);
    }
    if base.stock.is_none() {
        base.stock = other.stock;
        base.stock_at = other.stock_at;
    }
    if base.price_breaks.is_empty() {
        base.price_breaks.clone_from(&other.price_breaks);
    }
    if base.value.is_none() {
        base.value = other.value;
        base.value_unit = other.value_unit;
    }
    if base.rated_voltage_v.is_none() {
        base.rated_voltage_v = other.rated_voltage_v;
    }
    if base.lifecycle == crate::listing::Lifecycle::Unknown {
        base.lifecycle = other.lifecycle;
        base.lifecycle_raw.clone_from(&other.lifecycle_raw);
    }
    if base.datasheet_url.is_none() {
        base.datasheet_url.clone_from(&other.datasheet_url);
    }
    if base.symbol.is_none() {
        base.symbol.clone_from(&other.symbol);
    }
    if base.footprint.is_none() {
        base.footprint.clone_from(&other.footprint);
    }
    if base.model3d.is_none() {
        base.model3d.clone_from(&other.model3d);
    }
}

/// One part while merging: the record shown, the ids it absorbed, and the
/// sources it holds so a source never merges with itself.
type Group = (Found, Vec<String>, Vec<(ListingSource, String)>);

/// Merge listings from any number of sources into one ranked list.
///
/// # Rules
/// - Listings with the same part number (see `mpn_key`) from different
///   sources become one hit; the nearest source supplies the record, empty
///   fields are filled from the others, the others' ids go to `also_in`.
/// - Two listings of one source never merge with each other: a library can
///   hold a symbol and a footprint of the same name.
/// - The result does not depend on the order of `found`.
/// - `age_secs` is zero when a source's clock is ahead of `now`.
/// # Dependencies
/// `stable_id`, `rank_of` in this file; `PartListing` in `./listing.rs`.
/// # Do not
/// Call a source or drop a listing that has no part number.
pub fn merge(query: &str, now: u64, mut found: Vec<Found>) -> Vec<Hit> {
    found.sort_by(|a, b| {
        (priority(a.listing.source), stable_id(&a.listing))
            .cmp(&(priority(b.listing.source), stable_id(&b.listing)))
    });
    found.dedup_by(|b, a| stable_id(&a.listing) == stable_id(&b.listing));
    let mut groups: Vec<Group> = Vec::new();
    let mut by_key: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for item in found {
        let origin = (item.listing.source, item.listing.source_id.clone());
        let key = mpn_key(&item.listing);
        let home = key.as_ref().and_then(|k| {
            by_key
                .get(k)?
                .iter()
                .copied()
                .find(|&g| !groups[g].2.contains(&origin))
        });
        match home {
            Some(g) => {
                fill(&mut groups[g].0.listing, &item.listing);
                groups[g].1.push(stable_id(&item.listing));
                groups[g].2.push(origin);
            }
            None => {
                if let Some(k) = key {
                    by_key.entry(k).or_default().push(groups.len());
                }
                groups.push((item, Vec::new(), vec![origin]));
            }
        }
    }
    let mut hits: Vec<Hit> = groups
        .into_iter()
        .map(|(item, also_in, _)| Hit {
            id: stable_id(&item.listing),
            rank: rank_of(query, &item.listing),
            age_secs: item.fetched_at.map(|t| now.saturating_sub(t)),
            fetched_at: item.fetched_at,
            licence: item.licence,
            listing: item.listing,
            also_in,
        })
        .collect();
    hits.sort_by(|a, b| {
        (
            a.rank,
            priority(a.listing.source),
            a.listing.mpn.to_lowercase(),
            &a.id,
        )
            .cmp(&(
                b.rank,
                priority(b.listing.source),
                b.listing.mpn.to_lowercase(),
                &b.id,
            ))
    });
    hits
}

/// Ask every provider, merge, rank and cut to `limit`.
///
/// # Rules
/// - The query is checked once by `check_query`; a bad one is an error.
/// - A provider that fails is listed in `failures`; the rest still answer.
///   Offline means every provider that needs a connection said so, and hits
///   from saved or local data still come back.
/// - `limit` is clamped to 1 through [`MAX_HITS`].
/// # Dependencies
/// [`merge`] in this file; `check_query` in `./sources.rs`.
/// # Do not
/// Retry a failed provider or call one twice.
pub fn search(
    providers: &[&dyn ListingProvider],
    query: &str,
    now: u64,
    limit: usize,
) -> Result<SearchOutcome, SourceError> {
    let query = check_query(query)?;
    let mut found = Vec::new();
    let mut failures = Vec::new();
    for p in providers {
        match p.search(query, now) {
            Ok(mut f) => {
                f.truncate(MAX_RESULTS);
                found.extend(f);
            }
            Err(SourceError::NotFound(_)) => {}
            Err(e) => failures.push(SourceFailure {
                source: p.label(),
                offline: matches!(e, SourceError::Unavailable(_)),
                message: e.to_string(),
            }),
        }
    }
    let mut hits = merge(query, now, found);
    let total_known = hits.len();
    hits.truncate(limit.clamp(1, MAX_HITS));
    Ok(SearchOutcome {
        hits,
        failures,
        total_known,
    })
}

/// Symbols the caller already parsed: the open project's library, or one
/// installed library file.
///
/// Contract: crates/library/src/listing_search.rs (`ListingProvider`)
pub struct SymbolProvider {
    kind: ListingSource,
    source_id: String,
    licence: String,
    symbols: Vec<SymbolDefinition>,
}

impl SymbolProvider {
    /// Symbols from `source_id` (a library path or the project name).
    /// `kind` is `Project` or `Installed`.
    pub fn new(
        kind: ListingSource,
        source_id: &str,
        licence: &str,
        symbols: Vec<SymbolDefinition>,
    ) -> Self {
        Self {
            kind,
            source_id: source_id.to_owned(),
            licence: licence.to_owned(),
            symbols,
        }
    }
}

/// Contract: crates/library/src/listing_search.rs
impl ListingProvider for SymbolProvider {
    fn label(&self) -> String {
        self.source_id.clone()
    }
    fn search(&self, query: &str, _now: u64) -> Result<Vec<Found>, SourceError> {
        let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
        Ok(self
            .symbols
            .iter()
            .filter(|s| {
                let text = format!("{} {} {}", s.name, s.description, s.keywords).to_lowercase();
                words.iter().all(|w| text.contains(w.as_str()))
            })
            .take(MAX_RESULTS)
            .map(|s| Found {
                listing: listing_from_symbol(s, self.kind, &self.source_id),
                licence: self.licence.clone(),
                fetched_at: None,
            })
            .collect())
    }
}

/// Any [`PartSource`]: recorded sample parts, or a live distributor.
///
/// Contract: crates/library/src/listing_search.rs (`ListingProvider`)
pub struct SourceProvider {
    kind: ListingSource,
    source: Arc<dyn PartSource>,
}

impl SourceProvider {
    /// A provider over `source`; `kind` is `Fixture` or `Distributor`.
    pub fn new(kind: ListingSource, source: Arc<dyn PartSource>) -> Self {
        Self { kind, source }
    }
}

/// Contract: crates/library/src/listing_search.rs
impl ListingProvider for SourceProvider {
    fn label(&self) -> String {
        self.source.name().to_owned()
    }
    fn search(&self, query: &str, _now: u64) -> Result<Vec<Found>, SourceError> {
        let attribution = self.source.attribution();
        Ok(self
            .source
            .search(query)?
            .into_iter()
            .take(MAX_RESULTS)
            .map(|r| {
                let mut listing = listing_from_result(&r, &self.source.attributes(&r.mpn));
                listing.source = self.kind;
                listing.attribution = Some(attribution.clone());
                Found {
                    licence: attribution.text.clone(),
                    fetched_at: Some(r.retrieved_at),
                    listing,
                }
            })
            .collect())
    }
}

/// The open online library: symbols and footprints, from the saved copy when
/// there is no connection.
///
/// Contract: crates/library/src/listing_search.rs (`ListingProvider`)
pub struct OpenLibraryProvider {
    source: Arc<OpenLibrarySource>,
    scope: CardScope,
}

impl OpenLibraryProvider {
    /// A provider over `source` for the kinds of card asked.
    pub fn new(source: Arc<OpenLibrarySource>, scope: CardScope) -> Self {
        Self { source, scope }
    }
}

/// Contract: crates/library/src/listing_search.rs
impl ListingProvider for OpenLibraryProvider {
    fn label(&self) -> String {
        PartSource::name(self.source.as_ref()).to_owned()
    }
    fn search(&self, query: &str, _now: u64) -> Result<Vec<Found>, SourceError> {
        let attribution = crate::cad::CadSource::attribution(self.source.as_ref());
        Ok(self
            .source
            .search_cards(self.scope, query)?
            .into_iter()
            .take(MAX_RESULTS)
            .map(|r| {
                let mut listing = listing_from_result(&r, &BTreeMap::new());
                listing.source = ListingSource::OpenLibrary;
                // A library card has no stock: it is a drawing, not an offer.
                listing.stock = None;
                listing.stock_at = None;
                listing.attribution = Some(attribution.clone());
                Found {
                    licence: attribution.text.clone(),
                    fetched_at: Some(r.retrieved_at),
                    listing,
                }
            })
            .collect())
    }
}
