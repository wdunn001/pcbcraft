//! Bill of materials rows and generic CSV text shared by the engine, MCP and packages.
use pcbcraft_doc::{Id, Project};
use std::collections::BTreeMap;

const MAX_PROPERTIES: usize = 16;
const MAX_PARTS: usize = 4096;

/// Parameters for listing or printing a bill of materials.
#[derive(Clone, Debug, Default)]
pub struct BomSheetParams {
    pub fields: Vec<String>,
    pub variant: Option<String>,
    /// Only these components; empty lists every part.
    pub ids: Vec<Id>,
    /// False prints one row per part instead of one row per kind of part.
    pub group: bool,
}

/// One grouped or ungrouped bill row.
#[derive(Clone, Debug)]
pub struct BomSheetRow {
    pub references: Vec<String>,
    pub value: String,
    pub kind: String,
    pub footprint: String,
    /// Values for every property key the caller asked for, in key order.
    pub property_values: Vec<String>,
}

/// Collect bill rows from the project.
pub fn bom_sheet_rows(p: &Project, params: &BomSheetParams) -> Result<Vec<BomSheetRow>, String> {
    p.validate()?;
    if params.fields.len() > MAX_PROPERTIES {
        return Err("a bill of materials lists at most 16 properties".into());
    }
    for key in &params.fields {
        if key.is_empty() || key.len() > 64 {
            return Err("property names must be 1 to 64 bytes".into());
        }
    }
    if params.ids.len() > MAX_PARTS {
        return Err("a bill of materials names at most 4096 parts".into());
    }
    let variant = match params.variant.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(name) => Some(
            p.variants
                .iter()
                .find(|v| v.name == name)
                .ok_or_else(|| format!("no variant named {name}"))?,
        ),
    };
    let mut groups: BTreeMap<(String, String, String, Vec<String>, String), Vec<String>> =
        BTreeMap::new();
    for c in &p.components {
        if !c.in_bom()
            || (!params.ids.is_empty() && !params.ids.contains(&c.id))
            || variant.is_some_and(|v| v.dnp.contains(&c.id))
        {
            continue;
        }
        let footprint =
            match variant.and_then(|v| v.footprints.iter().find(|o| o.component == c.id)) {
                Some(swap) => swap.footprint.clone(),
                None => match &c.library_footprint {
                    Some(footprint) => {
                        format!("{}:{}", footprint.library, footprint.definition.name)
                    }
                    None => "starter pads".into(),
                },
            };
        let property_values: Vec<String> = params
            .fields
            .iter()
            .map(|key| c.field(key).unwrap_or("").to_string())
            .collect();
        let apart = if params.group {
            String::new()
        } else {
            c.reference.clone()
        };
        groups
            .entry((
                c.value.clone(),
                c.kind.clone(),
                footprint,
                property_values,
                apart,
            ))
            .or_default()
            .push(c.reference.clone());
    }
    Ok(groups
        .into_iter()
        .map(
            |((value, kind, footprint, property_values, _), mut references)| {
                references.sort();
                BomSheetRow {
                    references,
                    value,
                    kind,
                    footprint,
                    property_values,
                }
            },
        )
        .collect())
}

fn field_at(keys: &[String], values: &[String], key: &str) -> String {
    keys.iter()
        .zip(values)
        .find(|(k, _)| k.as_str() == key)
        .map(|(_, v)| v.clone())
        .unwrap_or_default()
}

fn extra_columns(p: &Project, keys: &[String]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for key in keys {
        if key == "MPN" || key == "Manufacturer" {
            continue;
        }
        let label = p
            .bom_fields
            .iter()
            .find(|f| f.key == *key)
            .map(|f| {
                if f.label.is_empty() {
                    f.key.clone()
                } else {
                    f.label.clone()
                }
            })
            .unwrap_or_else(|| key.clone());
        out.push((key.clone(), label));
    }
    out
}

fn csv_cell(text: &str) -> String {
    if text.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text.to_owned()
    }
}

/// Generic CSV matching the default bill flavour: Reference through Quantity, then extra columns.
pub fn bom_sheet_generic_csv(
    p: &Project,
    rows: &[BomSheetRow],
    keys: &[String],
) -> String {
    let extras = extra_columns(p, keys);
    let mut out = String::from("Reference,Value,Footprint,MPN,Manufacturer,Quantity");
    for (_, label) in &extras {
        out.push(',');
        out.push_str(&csv_cell(label));
    }
    out.push('\n');
    for row in rows {
        let refs = row.references.join(", ");
        let mpn = field_at(keys, &row.property_values, "MPN");
        let manufacturer = field_at(keys, &row.property_values, "Manufacturer");
        let mut cells = vec![
            csv_cell(&refs),
            csv_cell(&row.value),
            csv_cell(&row.footprint),
            csv_cell(&mpn),
            csv_cell(&manufacturer),
            row.references.len().to_string(),
        ];
        for (key, _) in &extras {
            cells.push(csv_cell(&field_at(keys, &row.property_values, key)));
        }
        out.push_str(&cells.join(","));
        out.push('\n');
    }
    out
}

/// Rows and generic CSV for the usual disk and package export path.
pub fn bom_sheet_generic_csv_for_project(
    p: &Project,
    params: &BomSheetParams,
) -> Result<String, String> {
    let rows = bom_sheet_rows(p, params)?;
    Ok(bom_sheet_generic_csv(p, &rows, &params.fields))
}
