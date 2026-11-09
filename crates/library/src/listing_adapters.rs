//! Adapters that turn symbols read from a project or an installed library
//! into `PartListing` records, so one filter covers every non-network source.
//!
//! Local rules: a symbol carries only what its properties say, so every other
//! field stays empty. Nothing is guessed from a part number. At most
//! [`MAX_LISTINGS`] records come back per call; the count of what was clipped
//! is reported, not hidden. No file access here: callers pass parsed symbols.
//! Dependencies: `crates/library/src/listing.rs` for the record and the
//! normalizers.
use crate::SymbolDefinition;
use crate::listing::{
    Category, Lifecycle, ListingSource, PartListing, Unit, category_from, package_key, parse_value,
};

/// Most listings one adapter call returns.
pub const MAX_LISTINGS: usize = 5000;

/// Listings built from a run of symbols, with the true count before clipping.
#[derive(Clone, Debug, PartialEq)]
pub struct SymbolListings {
    /// At most [`MAX_LISTINGS`] records, in the order the symbols came.
    pub items: Vec<PartListing>,
    /// How many symbols were offered, so a clipped answer says so.
    pub total_known: usize,
}

/// The `Datasheet` property out of a symbol's own source text.
fn datasheet_of(raw: &str) -> Option<String> {
    let rest = &raw[raw.find("(property \"Datasheet\" \"")? + 23..];
    let mut out = String::new();
    let mut chars = rest.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => break,
            '\\' => out.push(chars.next()?),
            _ => out.push(c),
        }
    }
    let out = out.trim().to_string();
    (!out.is_empty() && out != "~").then_some(out)
}

/// Footprint property text to a package key: `Lib:R_0805_2012Metric` reads
/// through its metric code as `2012 Metric`, which keys as imperial 0805.
fn footprint_package(link: &str) -> crate::listing::PackageKey {
    let name = link.rsplit(':').next().unwrap_or(link);
    let metric = name.split('_').find_map(|part| {
        let digits = part.strip_suffix("Metric")?;
        (digits.len() == 4 && digits.chars().all(|c| c.is_ascii_digit()))
            .then(|| format!("{digits} Metric"))
    });
    let mut key = package_key(&metric.unwrap_or_else(|| name.replace('_', " ")));
    key.text = name.to_string();
    key
}

/// One symbol as a listing.
///
/// # Rules
/// `source_id` is the library path, `source_ref` the symbol name. The value
/// comes from the `Value` property through `parse_value`, with a unit hint
/// from the category. The footprint link, description and datasheet come from
/// the matching properties; the package key comes from the footprint name.
/// A symbol that states none of them leaves those fields empty.
/// # Dependencies
/// `crates/library/src/listing.rs` for `parse_value`, `package_key`, `category_from`.
/// # Do not
/// Take a value from the part name or invent a manufacturer.
pub fn listing_from_symbol(
    symbol: &SymbolDefinition,
    source: ListingSource,
    source_id: &str,
) -> PartListing {
    let library = source_id.rsplit(['/', '\\']).next().unwrap_or(source_id);
    let library = library.strip_suffix(".kicad_sym").unwrap_or(library);
    let category = category_from(library, &format!("{} {}", symbol.keywords, symbol.name));
    let hint = match category {
        Category::Resistor => Some(Unit::Ohm),
        Category::Capacitor => Some(Unit::Farad),
        Category::Inductor => Some(Unit::Henry),
        Category::Crystal => Some(Unit::Hertz),
        _ => None,
    };
    let (value, value_unit) =
        parse_value(symbol.value.trim(), hint).map_or((None, None), |(v, u)| (Some(v), Some(u)));
    let footprint = symbol.footprint.trim();
    let package = if footprint.is_empty() {
        Default::default()
    } else {
        footprint_package(footprint)
    };
    PartListing {
        source,
        source_id: source_id.to_string(),
        source_ref: symbol.name.clone(),
        manufacturer: String::new(),
        mpn: symbol.name.clone(),
        description: symbol.description.clone(),
        category,
        source_category: library.to_string(),
        package_key: package.key,
        package_text: package.text,
        value,
        value_unit,
        tolerance_pct: None,
        rated_voltage_v: None,
        temp_min_c: None,
        temp_max_c: None,
        power_w: None,
        stock: None,
        stock_at: None,
        price_breaks: vec![],
        min_order_qty: None,
        order_multiple: None,
        packaging: None,
        lifecycle: Lifecycle::Unknown,
        lifecycle_raw: String::new(),
        rohs: None,
        reach: None,
        halogen_free: None,
        moisture_level: None,
        datasheet_url: datasheet_of(&symbol.raw_source),
        symbol: Some(symbol.name.clone()),
        footprint: (!footprint.is_empty()).then(|| footprint.to_string()),
        model3d: None,
        attribution: None,
    }
}

/// Listings for a run of symbols, clipped to `room` records.
///
/// # Rules
/// `room` is at most [`MAX_LISTINGS`]; a larger one is lowered to it.
/// `total_known` counts every symbol offered, clipped or not.
/// # Dependencies
/// [`listing_from_symbol`] in this file.
/// # Do not
/// Read files or skip symbols silently.
/// Convert multiple symbols to listings with clipping.
///
/// # Rules
/// - Takes at most `room` symbols, clipped to [`MAX_LISTINGS`]
/// - Reports `total_known` as the original symbol count
/// - Each symbol is converted via [`listing_from_symbol`]
///
/// # Dependencies
/// - crates/library/src/listing_adapters.rs [`listing_from_symbol`] for per-symbol conversion
/// - crates/library/src/listing_adapters.rs [`SymbolListings`] for return structure
///
/// # Do not
/// - Assume room is honored if it exceeds MAX_LISTINGS
pub fn listings_from_symbols(
    symbols: &[SymbolDefinition],
    source: ListingSource,
    source_id: &str,
    room: usize,
) -> SymbolListings {
    SymbolListings {
        items: symbols
            .iter()
            .take(room.min(MAX_LISTINGS))
            .map(|s| listing_from_symbol(s, source, source_id))
            .collect(),
        total_known: symbols.len(),
    }
}
