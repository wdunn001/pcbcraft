//! Helpers that connect placed components to three-layer library records.
use pcbcraft_doc::{Component, PlacedPartLink, Project};
use pcbcraft_library::records::{
    self, AttachRecords, OrderablePartRecord, pin_to_pad_from_pin_numbers,
};
use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn link_inputs(project: &Project, component: &Component) -> (Option<(String, String)>, Option<(String, String)>, BTreeMap<String, String>) {
    let symbol = component.library_symbol.as_ref().map(|s| {
        (
            s.library.clone(),
            s.definition.name.clone(),
        )
    });
    let footprint = component.library_footprint.as_ref().map(|f| {
        (
            f.library.clone(),
            f.definition.name.clone(),
        )
    });
    let pin_to_pad = if let Some(fp) = &component.library_footprint {
        pin_to_pad_from_pin_numbers(
            component.pins.iter().map(|p| p.number.clone()),
            fp.definition.pads.iter().map(|p| p.number.clone()),
        )
    } else if let Some(bf) = project
        .board
        .footprints
        .iter()
        .find(|f| f.component == component.id)
    {
        pin_to_pad_from_pin_numbers(
            component.pins.iter().map(|p| p.number.clone()),
            bf.pads.iter().map(|p| p.number.clone()),
        )
    } else {
        BTreeMap::new()
    };
    (symbol, footprint, pin_to_pad)
}

pub fn attach_source_records(
    project: &mut Project,
    component_id: pcbcraft_doc::Id,
    manufacturer: &str,
    mpn: &str,
    datasheet: &str,
    distributor: &str,
    sku: &str,
) -> Result<AttachRecords, String> {
    let (kind, value, symbol, footprint, pin_to_pad) = {
        let component = project
            .components
            .iter()
            .find(|c| c.id == component_id)
            .ok_or_else(|| "component not found".to_string())?;
        let (symbol, footprint, pin_to_pad) = link_inputs(project, component);
        (
            component.kind.clone(),
            component.value.clone(),
            symbol,
            footprint,
            pin_to_pad,
        )
    };
    let attach = records::records_from_attach(
        symbol,
        footprint,
        None,
        pin_to_pad,
        &kind,
        &value,
        manufacturer,
        mpn,
        datasheet,
        distributor,
        sku,
        now_secs(),
    );
    records::upsert_records(
        &mut project.library.part_components,
        &mut project.library.orderable_parts,
        attach.clone(),
    )?;
    project.part_links.insert(
        component_id,
        PlacedPartLink {
            component_id: attach.component.id.clone(),
            orderable_part_id: Some(attach.orderable.id.clone()),
        },
    );
    Ok(attach)
}

pub fn apply_orderable_fields(
    component: &mut Component,
    orderable: &OrderablePartRecord,
) -> Result<(), String> {
    clear_orderable_fields(component)?;
    set_field(
        component,
        "Manufacturer",
        Some(orderable.manufacturer.as_str()),
    )?;
    set_field(component, "MPN", Some(orderable.mpn.as_str()))?;
    if !orderable.datasheet_url.is_empty() {
        set_field(component, "Datasheet", Some(&orderable.datasheet_url))?;
    }
    for sku in &orderable.supplier_skus {
        let key = if sku.distributor.is_empty() {
            "SKU".to_string()
        } else {
            format!("{} SKU", sku.distributor)
        };
        set_field(component, &key, Some(sku.sku.as_str()))?;
    }
    Ok(())
}

pub fn clear_orderable_fields(component: &mut Component) -> Result<(), String> {
    for key in ["Manufacturer", "MPN", "Datasheet"] {
        clear_field(component, key)?;
    }
    component.fields.retain(|f| !f.key.ends_with(" SKU") && f.key != "SKU");
    Ok(())
}

fn set_field(component: &mut Component, key: &str, value: Option<&str>) -> Result<(), String> {
    match value {
        Some(value) if !value.is_empty() => {
            if let Some(existing) = component.fields.iter_mut().find(|f| f.key == key) {
                existing.value = value.to_string();
            } else if component.fields.len() >= 32 {
                return Err("a component carries at most 32 fields".into());
            } else {
                component.fields.push(pcbcraft_doc::ComponentField {
                    key: key.to_string(),
                    value: value.to_string(),
                });
            }
        }
        _ => {
            clear_field(component, key)?;
        }
    }
    Ok(())
}

fn clear_field(component: &mut Component, key: &str) -> Result<(), String> {
    if let Some(index) = component.fields.iter().position(|f| f.key == key) {
        component.fields.remove(index);
    }
    Ok(())
}
