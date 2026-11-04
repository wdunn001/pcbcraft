//! Helpers that connect placed components to three-layer library records.
use pcbcraft_doc::{Component, PlacedPartLink, Project};
use pcbcraft_library::records::{
    self, AttachRecords, OrderablePartRecord, pin_to_pad_from_pin_numbers,
};
use std::collections::BTreeMap;

pub fn now_secs() -> u64 {
    pcbcraft_clock::unix_secs()
}

type LibraryRef = Option<(String, String)>;

pub fn link_inputs(
    project: &Project,
    component: &Component,
) -> (LibraryRef, LibraryRef, BTreeMap<String, String>) {
    let symbol = component
        .library_symbol
        .as_ref()
        .map(|s| (s.library.clone(), s.definition.name.clone()));
    let footprint = component
        .library_footprint
        .as_ref()
        .map(|f| (f.library.clone(), f.definition.name.clone()));
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
    component
        .fields
        .retain(|f| !f.key.ends_with(" SKU") && f.key != "SKU");
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

#[cfg(test)]
mod tests {
    use crate::Session;
    use pcbcraft_doc::Project;
    use serde_json::json;

    #[test]
    fn a_distributor_part_keeps_its_fields_after_save_and_reopen()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut s = Session::sample()?;
        let r99 = s.execute(
            "component.add",
            json!({"kind": "resistor", "position": {"x": 10.0, "y": 10.0}, "reference": "R99", "value": "10k"}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or("id missing")?;
        s.execute(
            "part.attach_source",
            json!({
                "id": r99,
                "manufacturer": "Example Components",
                "mpn": "EX-R-10K",
                "distributor": "Sample Shop",
                "sku": "SS-123",
                "lifecycle": "active",
                "datasheet": "https://example.invalid/d.pdf"
            }),
            None,
        )?;

        // Record the ids before serialization
        let component_id = s.project.library.part_components[0].id.clone();
        let orderable_id = s.project.library.orderable_parts[0].id.clone();

        // Serialize and deserialize
        let text = serde_json::to_string(&s.project)?;
        let back: Project = serde_json::from_str(&text)?;

        // Validate the reopened project
        back.validate()?;
        pcbcraft_library::records::validate_store(
            &back.library.part_components,
            &back.library.orderable_parts,
        )?;

        // Assert component fields survived
        let c = back
            .component(r99)
            .ok_or("component missing after reopen")?;
        assert_eq!(c.field("Manufacturer"), Some("Example Components"));
        assert_eq!(c.field("MPN"), Some("EX-R-10K"));
        assert_eq!(c.field("Sample Shop SKU"), Some("SS-123"));
        assert_eq!(c.sourcing.manufacturer, "Example Components");
        assert_eq!(c.sourcing.mpn, "EX-R-10K");
        assert_eq!(c.sourcing.suppliers[0].supplier, "Sample Shop");
        assert_eq!(c.sourcing.suppliers[0].part_number, "SS-123");
        assert!(c.sourcing.lifecycle.is_some());

        // Assert library records survived with same ids
        assert_eq!(back.library.orderable_parts.len(), 1);
        assert_eq!(back.library.orderable_parts[0].id, orderable_id);
        assert_eq!(back.library.part_components[0].id, component_id);

        Ok(())
    }

    #[test]
    fn a_reopened_project_needs_no_library_folder() -> Result<(), Box<dyn std::error::Error>> {
        let mut s = Session::sample()?;
        let r99 = s.execute(
            "component.add",
            json!({"kind": "resistor", "position": {"x": 10.0, "y": 10.0}, "reference": "R99", "value": "10k"}),
            None,
        )?["result"]["id"]
            .as_u64()
            .ok_or("id missing")?;
        s.execute(
            "part.attach_source",
            json!({
                "id": r99,
                "manufacturer": "Example Components",
                "mpn": "EX-R-10K",
                "distributor": "Sample Shop",
                "sku": "SS-123"
            }),
            None,
        )?;

        // What the part carries before saving, so reopening can be compared with it.
        let c_before = s
            .project
            .component(r99)
            .ok_or("component missing before reopen")?;
        let symbol_before = c_before.library_symbol.clone();
        let footprint_before = c_before.library_footprint.clone();

        // Serialize and deserialize
        let text = serde_json::to_string(&s.project)?;
        let back: Project = serde_json::from_str(&text)?;

        // The reopened project has no linked folders
        assert!(back.library_table.is_empty());

        // The part reads back exactly as it was saved, with no folder to look in.
        let c = back
            .component(r99)
            .ok_or("component missing after reopen")?;
        assert_eq!(c.library_symbol, symbol_before, "the symbol is unchanged");
        assert_eq!(
            c.library_footprint, footprint_before,
            "the footprint is unchanged"
        );
        let link = back.part_links.get(&r99).ok_or("part link missing")?;
        assert!(!link.component_id.is_empty());

        Ok(())
    }
}
