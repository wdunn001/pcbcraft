//! Three-layer library part records: generic component, orderable part, and
//! provenance. Listings and component fields migrate into these structs so
//! projects keep copied records offline.
//!
//! Dependencies: `listing.rs` and `sources.rs` for distributor answers;
//! `cad_cache::content_hash` for stable ids.
use crate::cad_cache::content_hash;
use crate::listing::{
    Category, Lifecycle, ListingSource, PartListing, Unit, listing_from_result,
};
use crate::sources::{PartResult, PriceBreak};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Most component or orderable records one project library holds.
pub const MAX_PART_RECORDS: usize = 4096;
/// Longest stable id string.
pub const MAX_RECORD_ID_BYTES: usize = 64;

/// Generic part: symbol, footprints, model, pin map, parameters, stable id.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentRecord {
    pub id: String,
    pub content_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub footprints: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub pin_to_pad: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub parameters: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<ProvenanceRecord>,
}

/// One manufacturer part number bound to exactly one [`ComponentRecord`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrderablePartRecord {
    pub id: String,
    pub component_id: String,
    pub manufacturer: String,
    pub mpn: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub supplier_skus: Vec<SupplierSku>,
    pub lifecycle: Lifecycle,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub lifecycle_raw: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rohs: Option<bool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub snapshots: Vec<PriceStockSnapshot>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub datasheet_url: String,
    pub provenance: ProvenanceRecord,
}

/// Distributor or seller stock-keeping identifier for an orderable part.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SupplierSku {
    pub distributor: String,
    pub sku: String,
}

/// Dated stock and price answer copied from a source.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PriceStockSnapshot {
    pub at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stock: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub price_breaks: Vec<PriceBreak>,
}

/// Where a record came from: source, licence, fetch time, revision or hash.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProvenanceRecord {
    pub source_id: String,
    pub source_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub license_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub license_url: String,
    pub fetched_at: u64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub revision: String,
}

/// Body of a component record used for content hashing (id and hash excluded).
#[derive(Serialize)]
struct ComponentHashBody<'a> {
    symbol: &'a Option<String>,
    footprints: &'a [String],
    model: &'a Option<String>,
    pin_to_pad: &'a BTreeMap<String, String>,
    parameters: &'a BTreeMap<String, String>,
    kind: &'a str,
    description: &'a str,
}

/// Hash the generic component content for a stable id.
pub fn component_content_hash(record: &ComponentRecord) -> String {
    let body = ComponentHashBody {
        symbol: &record.symbol,
        footprints: &record.footprints,
        model: &record.model,
        pin_to_pad: &record.pin_to_pad,
        parameters: &record.parameters,
        kind: &record.kind,
        description: &record.description,
    };
    let text = serde_json::to_string(&body).unwrap_or_default();
    content_hash(text.as_bytes())
}

/// Stable id for an orderable part from manufacturer and MPN.
pub fn orderable_id(manufacturer: &str, mpn: &str) -> String {
    content_hash(format!("{}\0{}", manufacturer.trim(), mpn.trim()).as_bytes())
}

fn clip_id(id: String) -> String {
    if id.len() <= MAX_RECORD_ID_BYTES {
        return id;
    }
    id.chars().take(MAX_RECORD_ID_BYTES).collect()
}

/// Match schematic pin numbers to footprint pad numbers when both use the same name.
pub fn pin_to_pad_from_pin_numbers(
    pin_numbers: impl IntoIterator<Item = String>,
    pad_numbers: impl IntoIterator<Item = String>,
) -> BTreeMap<String, String> {
    let pads: BTreeSet<String> = pad_numbers.into_iter().collect();
    pin_numbers
        .into_iter()
        .filter(|n| pads.contains(n))
        .map(|n| (n.clone(), n))
        .collect()
}

/// Build a generic component record from library links on a placed part.
pub fn component_from_links(
    symbol: Option<(String, String)>,
    footprint: Option<(String, String)>,
    model: Option<String>,
    pin_to_pad: BTreeMap<String, String>,
    kind: &str,
    value: &str,
    description: &str,
    parameters: BTreeMap<String, String>,
    provenance: Option<ProvenanceRecord>,
) -> ComponentRecord {
    let symbol = symbol.map(|(library, name)| format!("{library}:{name}"));
    let footprints = footprint
        .map(|(library, name)| vec![format!("{library}:{name}")])
        .unwrap_or_default();
    let mut record = ComponentRecord {
        id: String::new(),
        content_hash: String::new(),
        symbol,
        footprints,
        model,
        pin_to_pad,
        parameters,
        kind: kind.to_string(),
        description: description.to_string(),
        provenance,
    };
    if value.trim().is_empty() {
        record.parameters.remove("value");
    } else {
        record.parameters.insert("value".into(), value.trim().to_string());
    }
    let hash = component_content_hash(&record);
    record.content_hash = hash.clone();
    record.id = clip_id(hash);
    record
}

/// Fill an orderable record from a normalized listing.
pub fn orderable_from_listing(
    listing: &PartListing,
    component_id: &str,
    provenance: ProvenanceRecord,
) -> OrderablePartRecord {
    let mut supplier_skus = Vec::new();
    if !listing.source_ref.is_empty() && listing.source_ref != listing.mpn {
        supplier_skus.push(SupplierSku {
            distributor: listing.source_id.clone(),
            sku: listing.source_ref.clone(),
        });
    }
    let snapshot = PriceStockSnapshot {
        at: listing.stock_at.unwrap_or(provenance.fetched_at),
        stock: listing.stock,
        price_breaks: listing.price_breaks.clone(),
    };
    OrderablePartRecord {
        id: clip_id(orderable_id(&listing.manufacturer, &listing.mpn)),
        component_id: component_id.to_string(),
        manufacturer: listing.manufacturer.clone(),
        mpn: listing.mpn.clone(),
        supplier_skus,
        lifecycle: listing.lifecycle,
        lifecycle_raw: listing.lifecycle_raw.clone(),
        rohs: listing.rohs,
        snapshots: if snapshot.stock.is_some() || !snapshot.price_breaks.is_empty() {
            vec![snapshot]
        } else {
            vec![]
        },
        datasheet_url: listing.datasheet_url.clone().unwrap_or_default(),
        provenance,
    }
}

/// Provenance for a distributor fixture or live answer.
pub fn provenance_from_listing(listing: &PartListing, fetched_at: u64) -> ProvenanceRecord {
    ProvenanceRecord {
        source_id: listing.source_id.clone(),
        source_name: listing.source_id.clone(),
        license_name: String::new(),
        license_url: String::new(),
        fetched_at,
        revision: listing.mpn.clone(),
    }
}

/// Rebuild a listing view from stored records (read-only sources).
pub fn listing_from_orderable(
    orderable: &OrderablePartRecord,
    component: &ComponentRecord,
) -> PartListing {
    let (value, value_unit) = component
        .parameters
        .get("value")
        .and_then(|v| crate::listing::parse_value(v, category_hint(&component.kind)));
    let package_key = component
        .footprints
        .first()
        .map(|f| crate::listing::package_key(f))
        .unwrap_or_default();
    let latest = orderable.snapshots.last();
    PartListing {
        source: ListingSource::Project,
        source_id: orderable.provenance.source_id.clone(),
        source_ref: orderable.mpn.clone(),
        manufacturer: orderable.manufacturer.clone(),
        mpn: orderable.mpn.clone(),
        description: component.description.clone(),
        category: category_from_kind(&component.kind),
        source_category: component.kind.clone(),
        package_key: package_key.key,
        package_text: package_key.text,
        value,
        value_unit,
        tolerance_pct: None,
        rated_voltage_v: None,
        temp_min_c: None,
        temp_max_c: None,
        power_w: None,
        stock: latest.and_then(|s| s.stock),
        stock_at: latest.map(|s| s.at),
        price_breaks: latest
            .map(|s| s.price_breaks.clone())
            .unwrap_or_default(),
        min_order_qty: None,
        order_multiple: None,
        packaging: None,
        lifecycle: orderable.lifecycle,
        lifecycle_raw: orderable.lifecycle_raw.clone(),
        rohs: orderable.rohs,
        datasheet_url: (!orderable.datasheet_url.is_empty()).then(|| orderable.datasheet_url.clone()),
        symbol: component.symbol.clone(),
        footprint: component.footprints.first().cloned(),
        model3d: component.model.clone(),
        attribution: None,
    }
}

fn category_hint(kind: &str) -> Option<Unit> {
    match category_from_kind(kind) {
        Category::Resistor => Some(Unit::Ohm),
        Category::Capacitor => Some(Unit::Farad),
        Category::Inductor => Some(Unit::Henry),
        Category::Crystal => Some(Unit::Hertz),
        _ => None,
    }
}

fn category_from_kind(kind: &str) -> Category {
    crate::listing::category_from(kind, kind)
}

/// Upsert helper result after attaching distributor data to a placed part.
#[derive(Clone, Debug, PartialEq)]
pub struct AttachRecords {
    pub component: ComponentRecord,
    pub orderable: OrderablePartRecord,
}

/// Build component and orderable records from attach-source fields and links.
pub fn records_from_attach(
    symbol: Option<(String, String)>,
    footprint: Option<(String, String)>,
    model: Option<String>,
    pin_to_pad: BTreeMap<String, String>,
    kind: &str,
    value: &str,
    manufacturer: &str,
    mpn: &str,
    datasheet: &str,
    distributor: &str,
    sku: &str,
    fetched_at: u64,
) -> AttachRecords {
    let source_name = if distributor.trim().is_empty() {
        "Attached source".into()
    } else {
        distributor.trim().to_string()
    };
    let provenance = ProvenanceRecord {
        source_id: source_name.clone(),
        source_name,
        license_name: String::new(),
        license_url: String::new(),
        fetched_at,
        revision: mpn.trim().to_string(),
    };
    let component = component_from_links(
        symbol,
        footprint,
        model,
        pin_to_pad,
        kind,
        value,
        "",
        BTreeMap::new(),
        Some(provenance.clone()),
    );
    let mut listing = listing_from_result(
        &PartResult {
            source: provenance.source_id.clone(),
            mpn: mpn.trim().to_string(),
            manufacturer: manufacturer.trim().to_string(),
            description: String::new(),
            kind: kind.to_string(),
            distributor: distributor.trim().to_string(),
            sku: sku.trim().to_string(),
            stock: 0,
            price_breaks: vec![],
            lifecycle: String::new(),
            datasheet_url: datasheet.trim().to_string(),
            product_url: String::new(),
            retrieved_at: fetched_at,
        },
        &BTreeMap::new(),
    );
    listing.source = ListingSource::Project;
    listing.lifecycle = Lifecycle::Unknown;
    let orderable = orderable_from_listing(&listing, &component.id, provenance);
    AttachRecords {
        component,
        orderable,
    }
}

/// Insert or replace records in a project library store.
pub fn upsert_records(
    part_components: &mut Vec<ComponentRecord>,
    orderable_parts: &mut Vec<OrderablePartRecord>,
    attach: AttachRecords,
) -> Result<(), String> {
    upsert_component(part_components, attach.component)?;
    upsert_orderable(orderable_parts, attach.orderable)
}

pub fn upsert_component(
    part_components: &mut Vec<ComponentRecord>,
    record: ComponentRecord,
) -> Result<(), String> {
    validate_component(&record)?;
    if part_components.len() >= MAX_PART_RECORDS && !part_components.iter().any(|c| c.id == record.id)
    {
        return Err("project library part component limit reached".into());
    }
    if let Some(existing) = part_components.iter_mut().find(|c| c.id == record.id) {
        *existing = record;
    } else {
        part_components.push(record);
    }
    Ok(())
}

pub fn upsert_orderable(
    orderable_parts: &mut Vec<OrderablePartRecord>,
    record: OrderablePartRecord,
) -> Result<(), String> {
    validate_orderable(&record)?;
    if orderable_parts.len() >= MAX_PART_RECORDS
        && !orderable_parts.iter().any(|o| o.id == record.id)
    {
        return Err("project library orderable part limit reached".into());
    }
    if let Some(existing) = orderable_parts.iter_mut().find(|o| o.id == record.id) {
        *existing = record;
    } else {
        orderable_parts.push(record);
    }
    Ok(())
}

pub fn validate_component(record: &ComponentRecord) -> Result<(), String> {
    if record.id.is_empty() || record.id.len() > MAX_RECORD_ID_BYTES {
        return Err("component record id must be 1 to 64 bytes".into());
    }
    if record.content_hash.is_empty() {
        return Err("component record needs a content hash".into());
    }
    if record.footprints.len() > 32 {
        return Err("component record holds at most 32 footprints".into());
    }
    if record.pin_to_pad.len() > 256 || record.parameters.len() > 64 {
        return Err("component record field limit exceeded".into());
    }
    Ok(())
}

pub fn validate_orderable(record: &OrderablePartRecord) -> Result<(), String> {
    if record.id.is_empty() || record.id.len() > MAX_RECORD_ID_BYTES {
        return Err("orderable part id must be 1 to 64 bytes".into());
    }
    if record.component_id.is_empty() || record.component_id.len() > MAX_RECORD_ID_BYTES {
        return Err("orderable part needs a component id".into());
    }
    if record.manufacturer.trim().is_empty() || record.mpn.trim().is_empty() {
        return Err("orderable part needs a manufacturer and MPN".into());
    }
    if record.manufacturer.len() > 256 || record.mpn.len() > 256 {
        return Err("manufacturer or MPN is too long".into());
    }
    if record.supplier_skus.len() > 16 || record.snapshots.len() > 32 {
        return Err("orderable part list limit exceeded".into());
    }
    validate_provenance(&record.provenance)
}

pub fn validate_provenance(record: &ProvenanceRecord) -> Result<(), String> {
    if record.source_id.is_empty() || record.source_id.len() > 256 {
        return Err("provenance needs a source id".into());
    }
    if record.source_name.len() > 256 {
        return Err("provenance source name is too long".into());
    }
    Ok(())
}

/// Validate every record and cross-reference in a project library bundle.
pub fn validate_store(
    part_components: &[ComponentRecord],
    orderable_parts: &[OrderablePartRecord],
) -> Result<(), String> {
    if part_components.len() > MAX_PART_RECORDS || orderable_parts.len() > MAX_PART_RECORDS {
        return Err("project library part record limit reached".into());
    }
    let mut component_ids = BTreeSet::new();
    for c in part_components {
        validate_component(c)?;
        if !component_ids.insert(&c.id) {
            return Err("duplicate component record id".into());
        }
    }
    let mut orderable_ids = BTreeSet::new();
    for o in orderable_parts {
        validate_orderable(o)?;
        if !component_ids.contains(&o.component_id) {
            return Err("orderable part references a missing component record".into());
        }
        if !orderable_ids.insert(&o.id) {
            return Err("duplicate orderable part id".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orderable_round_trips_through_listing() {
        let listing = PartListing {
            source: ListingSource::Fixture,
            source_id: "Sample".into(),
            source_ref: "SKU-1".into(),
            manufacturer: "Example Components".into(),
            mpn: "SAMPLE-R-10K".into(),
            description: "10k resistor".into(),
            category: Category::Resistor,
            source_category: "resistor".into(),
            package_key: "chip-0603".into(),
            package_text: "0603".into(),
            value: Some(10_000.0),
            value_unit: Some(Unit::Ohm),
            tolerance_pct: None,
            rated_voltage_v: None,
            temp_min_c: None,
            temp_max_c: None,
            power_w: None,
            stock: Some(100),
            stock_at: Some(1_790_000_000),
            price_breaks: vec![PriceBreak {
                quantity: 1,
                unit_price: 0.01,
                currency: "USD".into(),
            }],
            min_order_qty: None,
            order_multiple: None,
            packaging: None,
            lifecycle: Lifecycle::Active,
            lifecycle_raw: "Active".into(),
            rohs: Some(true),
            datasheet_url: Some("https://example.invalid/d.pdf".into()),
            symbol: Some("Device:R".into()),
            footprint: Some("Resistor_SMD:R_0603".into()),
            model3d: None,
            attribution: None,
        };
        let component = component_from_links(
            Some(("Device".into(), "R".into())),
            Some(("Resistor_SMD".into(), "R_0603".into())),
            None,
            BTreeMap::new(),
            "resistor",
            "10k",
            "10k resistor",
            BTreeMap::new(),
            None,
        );
        let orderable = orderable_from_listing(
            &listing,
            &component.id,
            provenance_from_listing(&listing, 1_790_000_000),
        );
        let back = listing_from_orderable(&orderable, &component);
        assert_eq!(back.mpn, listing.mpn);
        assert_eq!(back.manufacturer, listing.manufacturer);
        assert_eq!(back.value, listing.value);
        assert_eq!(back.stock, listing.stock);
    }

    #[test]
    fn content_hash_is_stable_for_same_body() {
        let a = component_from_links(
            Some(("Lib".into(), "R".into())),
            None,
            None,
            BTreeMap::new(),
            "resistor",
            "4k7",
            "",
            BTreeMap::new(),
            None,
        );
        let b = component_from_links(
            Some(("Lib".into(), "R".into())),
            None,
            None,
            BTreeMap::new(),
            "resistor",
            "4k7",
            "",
            BTreeMap::new(),
            None,
        );
        assert_eq!(a.id, b.id);
        assert_eq!(a.content_hash, b.content_hash);
    }
}
