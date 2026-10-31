//! Data-only export templates over the generic pin map.
use crate::pin_map::{PinMapRow, pin_map_rows};
use pcbcraft_doc::Project;
use serde::Deserialize;
use std::path::Path;

const MAX_TEMPLATE_BYTES: usize = 65_536;
const MAX_OUTPUT_BYTES: usize = 8_388_608;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PinMapTemplate {
    schema: u32,
    kind: String,
    #[serde(default)]
    header: String,
    row: String,
    #[serde(default = "default_sort")]
    sort: String,
}

fn default_sort() -> String {
    "net_then_reference".into()
}

/// Parsed export template ready to run over a project.
#[derive(Clone, Debug)]
pub struct ExportTemplate {
    pub kind: String,
    header: String,
    row: String,
    sort: String,
}

/// Load one export template TOML from disk.
///
/// # Rules
/// - Refuses unknown fields and unsupported schema or kind values.
/// - Template files must be at most `MAX_TEMPLATE_BYTES`.
///
/// # Dependencies
/// - crates/plugin/SCHEMA.md export template section.
///
/// # Do not
/// - Execute code or read the network; templates are data only.
pub fn load_export_template(path: &Path) -> Result<ExportTemplate, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("could not read export template {}: {e}", path.display()))?;
    if text.len() > MAX_TEMPLATE_BYTES {
        return Err("export template is too large".into());
    }
    let doc: PinMapTemplate =
        toml::from_str(&text).map_err(|e| format!("invalid template: {e}"))?;
    if doc.schema != 1 {
        return Err(format!("unsupported template schema {}", doc.schema));
    }
    if doc.kind != "pin_map" {
        return Err(format!("unsupported template kind {}", doc.kind));
    }
    if doc.row.is_empty() || doc.row.len() > 4096 {
        return Err("template row must be 1 to 4096 bytes".into());
    }
    if doc.header.len() > 4096 {
        return Err("template header must be at most 4096 bytes".into());
    }
    let sort = doc.sort;
    if sort != "net_then_reference" && sort != "reference_then_pin" {
        return Err(format!("unsupported template sort {sort}"));
    }
    Ok(ExportTemplate {
        kind: doc.kind,
        header: doc.header,
        row: doc.row,
        sort,
    })
}

fn apply_row(template: &str, row: &PinMapRow) -> String {
    template
        .replace("{net}", &row.net)
        .replace("{reference}", &row.reference)
        .replace("{pin}", &row.pin)
}

fn sort_rows(rows: &mut [PinMapRow], sort: &str) {
    match sort {
        "reference_then_pin" => {
            rows.sort_by(|a, b| (&a.reference, &a.pin, &a.net).cmp(&(&b.reference, &b.pin, &b.net)))
        }
        _ => {
            rows.sort_by(|a, b| (&a.net, &a.reference, &a.pin).cmp(&(&b.net, &b.reference, &b.pin)))
        }
    }
}

/// Render a pin-map export template against the current project.
///
/// # Rules
/// - Output size is capped at `MAX_OUTPUT_BYTES`.
///
/// # Dependencies
/// - crates/plugin/src/pin_map.rs `pin_map_rows`.
///
/// # Do not
/// - Mutate the project.
pub fn render_pin_map(p: &Project, template: &ExportTemplate) -> Result<String, String> {
    if template.kind != "pin_map" {
        return Err(format!("template kind {} is not pin_map", template.kind));
    }
    let mut rows = pin_map_rows(p)?;
    sort_rows(&mut rows, &template.sort);
    let mut out = template.header.clone();
    for row in rows {
        out.push_str(&apply_row(&template.row, &row));
        if out.len() > MAX_OUTPUT_BYTES {
            return Err("export output is too large".into());
        }
    }
    Ok(out)
}
