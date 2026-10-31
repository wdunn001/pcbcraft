//! Puts the cards of the Library tab in the order the library crate ranks one
//! search: exact part number, then package, then name, with a part listed by
//! two sources shown once.
//!
//! # Rules
//! - An empty search keeps the list as it is; ranking starts when the person types.
//! - Cards are never changed, only reordered or dropped as repeats.
//! - No file access, no network, no drawing here; the cards were already read.
//!
//! # Dependencies
//! - crates/library/src/listing_search.rs
//! - crates/library/src/listing.rs
//! - crates/ui-egui/src/library_dock.rs
use pcbcraft_library::listing::{ListingSource, listing_from_result, package_key};
use pcbcraft_library::listing_search::{Found, merge};
use pcbcraft_library::sources::PartResult;
use std::collections::BTreeMap;

/// What a card knows about its part, for ranking.
pub(crate) struct CardFacts<'a> {
    pub(crate) source: ListingSource,
    pub(crate) source_id: &'a str,
    pub(crate) source_ref: &'a str,
    pub(crate) mpn: &'a str,
    pub(crate) manufacturer: &'a str,
    pub(crate) description: &'a str,
    pub(crate) package: &'a str,
}

/// The ranking record for one card.
pub(crate) fn found_for(facts: &CardFacts<'_>, result: Option<&PartResult>) -> Found {
    let blank;
    let part = match result {
        Some(p) => p,
        None => {
            blank = PartResult {
                source: facts.source_id.to_owned(),
                mpn: facts.mpn.to_owned(),
                manufacturer: facts.manufacturer.to_owned(),
                description: facts.description.to_owned(),
                kind: String::new(),
                distributor: String::new(),
                sku: facts.source_ref.to_owned(),
                stock: 0,
                price_breaks: Vec::new(),
                lifecycle: String::new(),
                datasheet_url: String::new(),
                product_url: String::new(),
                retrieved_at: 0,
            };
            &blank
        }
    };
    let mut listing = listing_from_result(part, &BTreeMap::new());
    listing.source = facts.source;
    listing.source_id = facts.source_id.to_owned();
    listing.source_ref = facts.source_ref.to_owned();
    if !facts.package.is_empty() {
        let package = package_key(facts.package);
        listing.package_key = package.key;
        listing.package_text = package.text;
    }
    // A card says nothing about stock unless it came from a shop.
    if facts.source != ListingSource::Distributor {
        listing.stock = None;
        listing.stock_at = None;
    }
    Found {
        listing,
        licence: String::new(),
        fetched_at: result.map(|r| r.retrieved_at),
    }
}

/// Indexes of `found` in display order, repeats removed. An empty `query`
/// returns every index in the order given.
pub(crate) fn order(query: &str, found: Vec<Found>) -> Vec<usize> {
    if query.trim().is_empty() {
        return (0..found.len()).collect();
    }
    let ids: Vec<String> = found
        .iter()
        .map(|f| pcbcraft_library::listing_search::stable_id(&f.listing))
        .collect();
    merge(query, 0, found)
        .iter()
        .filter_map(|hit| ids.iter().position(|id| *id == hit.id))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(name: &str, source: ListingSource) -> Found {
        found_for(
            &CardFacts {
                source,
                source_id: "lib",
                source_ref: name,
                mpn: name,
                manufacturer: "",
                description: "",
                package: "",
            },
            None,
        )
    }

    #[test]
    fn an_empty_search_keeps_the_order_and_a_typed_one_ranks() {
        let cards = vec![
            card("Resistor pack", ListingSource::Project),
            card("R", ListingSource::Installed),
            card("resistor", ListingSource::OpenLibrary),
        ];
        assert_eq!(order("  ", cards.clone()), vec![0, 1, 2]);
        assert_eq!(
            order("resistor", cards),
            vec![2, 0, 1],
            "exact name, then names that start with it, then the rest"
        );
    }
}
