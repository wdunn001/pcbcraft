//! The bill of materials as a table: rows for the preview, text for the file.
//!
//! Context: issue #194. The Export dialog (`file_dialogs.rs`), the part right-click
//! menu and the Manufacture outputs list all read their rows here, so what is
//! previewed is what is written. Rows come from `pcbcraft_engine::bom_export`; this
//! file only picks columns, applies a vendor flavour and writes text.
//! Local rules: a flavour is data (`pcbcraft_io::formats::BOM_FLAVOURS`); nothing here
//! may name a vendor. Option values arrive as the dialog's string map, keyed by the
//! option labels in the registry.
//!
//! # Dependencies
//! - crates/io/src/formats.rs (`BomFlavour`, `BOM_FLAVOURS`)
//! - crates/engine/src/lib.rs (`bom_export`)
//! - crates/doc/src/lib.rs (`Project`, `BOM_EXCLUDE_FIELD`)
//!
//! # Do not
//! - Read `Project.components` to build rows; the engine owns grouping and skipping.
use pcbcraft_doc::{Id, Project};
use pcbcraft_io::formats::{BOM_FLAVOURS, BomFlavour, bom_flavour};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// The registry labels the dialog stores BOM option values under.
pub(crate) const FLAVOUR: &str = "Flavour";
pub(crate) const FILE_TYPE: &str = "File type";
pub(crate) const BUILD: &str = "Build";
pub(crate) const COLUMNS: &str = "Columns";
pub(crate) const GROUP: &str = "Group identical parts";

/// What one export of the bill asks for.
pub(crate) struct Options {
    pub flavour: &'static BomFlavour,
    /// `csv`, `tsv` or `json`.
    pub file_type: String,
    /// Assembly variant name, empty for the plain build.
    pub variant: String,
    /// Column tokens (`reference`...) and `field:KEY` for project columns.
    pub columns: Vec<String>,
    pub group: bool,
    /// Component ids to list; empty lists every part.
    pub only: Vec<Id>,
}

/// Rows ready to show or write.
pub(crate) struct Table {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

/// The flavour named in `values`, or the first one.
pub(crate) fn flavour_of(values: &BTreeMap<String, String>) -> &'static BomFlavour {
    values
        .get(FLAVOUR)
        .and_then(|id| bom_flavour(id))
        .unwrap_or(&BOM_FLAVOURS[0])
}

/// The option values a fresh bill starts with: the flavour's file type, every column of
/// the flavour and of the project, grouping as the flavour says.
pub(crate) fn default_values(project: &Project, flavour: &BomFlavour) -> BTreeMap<String, String> {
    let mut columns: Vec<String> = flavour
        .columns
        .iter()
        .map(|(t, _)| (*t).to_owned())
        .collect();
    columns.extend(
        project
            .bom_fields
            .iter()
            .map(|f| format!("field:{}", f.key)),
    );
    BTreeMap::from([
        (FLAVOUR.to_owned(), flavour.id.to_owned()),
        (FILE_TYPE.to_owned(), flavour.file_type.to_owned()),
        (BUILD.to_owned(), String::new()),
        (COLUMNS.to_owned(), columns.join(",")),
        (GROUP.to_owned(), flavour.group.to_string()),
    ])
}

impl Options {
    /// Read the dialog's values; anything missing falls back to the flavour.
    pub(crate) fn from_values(
        project: &Project,
        values: &BTreeMap<String, String>,
        only: &[Id],
    ) -> Self {
        let flavour = flavour_of(values);
        let fallback = default_values(project, flavour);
        let get = |key: &str| {
            values
                .get(key)
                .or_else(|| fallback.get(key))
                .cloned()
                .unwrap_or_default()
        };
        Self {
            flavour,
            file_type: get(FILE_TYPE),
            variant: get(BUILD),
            columns: get(COLUMNS)
                .split(',')
                .filter(|c| !c.is_empty())
                .map(str::to_owned)
                .collect(),
            group: get(GROUP) != "false",
            only: only.to_vec(),
        }
    }
}

/// Build the rows for `options`.
///
/// Why: one place decides which parts, columns and headers a bill has.
///
/// # Rules
/// - Columns follow the flavour's order, then the project's own columns.
/// - Rows the chosen columns cannot tell apart are merged when grouping.
///
/// # Dependencies
/// - crates/engine/src/lib.rs `bom_export`.
///
/// # Do not
/// - Fail on an empty project; an empty table is a fine answer.
pub(crate) fn table(project: &Project, o: &Options) -> Result<Table, String> {
    let wants = |token: &str| o.columns.iter().any(|c| c == token);
    let mut keys: Vec<String> = vec![];
    for (token, key) in [("mpn", "MPN"), ("manufacturer", "Manufacturer")] {
        if wants(token) && o.flavour.columns.iter().any(|(t, _)| *t == token) {
            keys.push(key.to_owned());
        }
    }
    let mut extra: Vec<(String, String)> = vec![];
    for f in &project.bom_fields {
        if wants(&format!("field:{}", f.key)) && !keys.contains(&f.key) && keys.len() < 16 {
            keys.push(f.key.clone());
            let label = if f.label.is_empty() { &f.key } else { &f.label };
            extra.push((f.key.clone(), label.clone()));
        }
    }
    let mut params = json!({"fields": keys, "group": o.group});
    if !o.variant.is_empty() {
        params["variant"] = json!(o.variant);
    }
    if !o.only.is_empty() {
        params["ids"] = json!(o.only);
    }
    let data = pcbcraft_engine::bom_export(project, &params).map_err(|e| e.to_string())?;
    let items = data["items"].as_array().cloned().unwrap_or_default();
    let chosen: Vec<&(&str, &str)> = o.flavour.columns.iter().filter(|(t, _)| wants(t)).collect();
    let mut headers: Vec<String> = chosen.iter().map(|(_, h)| (*h).to_owned()).collect();
    headers.extend(extra.iter().map(|(_, label)| label.clone()));
    let text = |v: &Value| v.as_str().unwrap_or("").to_owned();
    struct Row {
        refs: Vec<String>,
        quantity: u64,
        cells: Vec<String>,
    }
    let mut rows: Vec<Row> = vec![];
    let mut seen: BTreeMap<Vec<String>, usize> = BTreeMap::new();
    for item in &items {
        let refs: Vec<String> = item["references"]
            .as_array()
            .map(|a| a.iter().map(&text).collect())
            .unwrap_or_default();
        let field = |k: &str| text(&item["fields"][k]);
        let mut cells: Vec<String> = chosen
            .iter()
            .map(|(token, _)| match *token {
                "value" => text(&item["value"]),
                "footprint" => text(&item["footprint"]),
                "mpn" => field("MPN"),
                "manufacturer" => field("Manufacturer"),
                _ => String::new(),
            })
            .collect();
        cells.extend(extra.iter().map(|(key, _)| field(key)));
        let quantity = item["quantity"].as_u64().unwrap_or(0);
        // The cells that tell parts apart: everything except references and quantity.
        let key: Vec<String> = chosen
            .iter()
            .zip(&cells)
            .filter(|((t, _), _)| *t != "reference" && *t != "quantity")
            .map(|(_, c)| c.clone())
            .chain(cells.iter().skip(chosen.len()).cloned())
            .collect();
        match seen.get(&key) {
            Some(&at) if o.group => {
                rows[at].refs.extend(refs);
                rows[at].quantity += quantity;
            }
            _ => {
                seen.insert(key, rows.len());
                rows.push(Row {
                    refs,
                    quantity,
                    cells,
                });
            }
        }
    }
    let rows = rows
        .into_iter()
        .map(|mut r| {
            r.refs.sort();
            let joined = r.refs.join(o.flavour.reference_separator);
            for (i, (token, _)) in chosen.iter().enumerate() {
                match *token {
                    "reference" => r.cells[i] = joined.clone(),
                    "quantity" => r.cells[i] = r.quantity.to_string(),
                    _ => {}
                }
            }
            r.cells
        })
        .collect();
    Ok(Table { headers, rows })
}

/// Quote a CSV cell only when it holds a comma, quote or line break.
fn csv_cell(text: &str) -> String {
    if text.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text.to_owned()
    }
}

/// The file text for `table` as `csv`, `tsv` or `json`.
pub(crate) fn render(table: &Table, file_type: &str) -> String {
    match file_type {
        "json" => {
            let rows: Vec<Value> = table
                .rows
                .iter()
                .map(|row| {
                    let map: serde_json::Map<String, Value> = table
                        .headers
                        .iter()
                        .cloned()
                        .zip(row.iter().map(|c| json!(c)))
                        .collect();
                    Value::Object(map)
                })
                .collect();
            serde_json::to_string_pretty(&rows).unwrap_or_else(|_| "[]".into())
        }
        "tsv" => {
            let clean = |c: &String| c.replace(['\t', '\n', '\r'], " ");
            let mut out = table
                .headers
                .iter()
                .map(clean)
                .collect::<Vec<_>>()
                .join("\t");
            out.push('\n');
            for row in &table.rows {
                out.push_str(&row.iter().map(clean).collect::<Vec<_>>().join("\t"));
                out.push('\n');
            }
            out
        }
        _ => {
            let mut out = table
                .headers
                .iter()
                .map(|h| csv_cell(h))
                .collect::<Vec<_>>()
                .join(",");
            out.push('\n');
            for row in &table.rows {
                out.push_str(
                    &row.iter()
                        .map(|c| csv_cell(c))
                        .collect::<Vec<_>>()
                        .join(","),
                );
                out.push('\n');
            }
            out
        }
    }
}

/// The file name a bill is written under: `bom.csv`, or `bom-Slim.csv` for a variant.
pub(crate) fn file_name(o: &Options) -> String {
    let safe: String = o
        .variant
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if safe.is_empty() {
        format!("bom.{}", o.file_type)
    } else {
        format!("bom-{safe}.{}", o.file_type)
    }
}

/// A short phrase for a Recent entry: "Bill of materials (CSV, grouped)".
pub(crate) fn summary(project: &Project, values: &BTreeMap<String, String>) -> String {
    let o = Options::from_values(project, values, &[]);
    let mut parts = vec![o.file_type.to_uppercase()];
    if o.flavour.id != BOM_FLAVOURS[0].id {
        parts.insert(0, o.flavour.label.to_owned());
    }
    parts.push(
        if o.group {
            "grouped"
        } else {
            "one row per part"
        }
        .to_owned(),
    );
    if !o.variant.is_empty() {
        parts.push(o.variant.clone());
    }
    format!("Bill of materials ({})", parts.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pcbcraft_engine::Session;

    fn project() -> Result<Project, String> {
        let mut s = Session::default();
        for (reference, value) in [("R1", "10k"), ("R2", "10k"), ("C1", "100n")] {
            s.execute(
                "component.add",
                json!({"kind":"resistor","reference":reference,"value":value,"position":{"x":1.0,"y":1.0}}),
                None,
            )
            .map_err(|e| e.to_string())?;
        }
        serde_json::from_value(s.inspect()["project"].clone()).map_err(|e| e.to_string())
    }

    #[test]
    fn a_flavour_defined_purely_as_data_exports_correctly() -> Result<(), String> {
        static ASSEMBLER: BomFlavour = BomFlavour {
            id: "test_assembler",
            label: "Test assembler",
            tip: "Only for this test.",
            file_type: "tsv",
            columns: &[
                ("quantity", "Qty"),
                ("reference", "Designator"),
                ("value", "Comment"),
            ],
            group: true,
            reference_separator: ";",
        };
        let p = project()?;
        let values = default_values(&p, &ASSEMBLER);
        let mut o = Options::from_values(&p, &values, &[]);
        // `from_values` looks flavours up by id; this one is not in the table.
        o.flavour = &ASSEMBLER;
        let t = table(&p, &o)?;
        assert_eq!(
            render(&t, &o.file_type),
            "Qty\tDesignator\tComment\n1\tC1\t100n\n2\tR1;R2\t10k\n"
        );
        Ok(())
    }

    #[test]
    fn the_generic_flavour_lists_parts_apart_and_in_json() -> Result<(), String> {
        let p = project()?;
        let mut values = default_values(&p, &BOM_FLAVOURS[0]);
        values.insert(GROUP.into(), "false".into());
        let o = Options::from_values(&p, &values, &[]);
        let t = table(&p, &o)?;
        assert_eq!(t.rows.len(), 3, "one row per part");
        assert!(
            render(&t, "csv").starts_with("Reference,Value,Footprint,MPN,Manufacturer,Quantity\n")
        );
        assert!(render(&t, "json").contains("\"Reference\""));
        assert_eq!(file_name(&o), "bom.csv");
        Ok(())
    }
}
