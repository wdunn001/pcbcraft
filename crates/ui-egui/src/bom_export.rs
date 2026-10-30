//! The bill of materials as a table: rows for the preview, text for the file.
//!
//! Context: issue #194. The Export dialog (`file_dialogs.rs`), the part right-click
//! menu and the Manufacture outputs list all read their rows here, so what is
//! previewed is what is written. Rows come from `pcbcraft_engine::bom_export`; this
//! file only picks columns, applies a vendor flavour and writes text.
//! Local rules: a flavour is data (`crates/io/data/bom_flavours.json`); nothing here
//! may name a vendor. Option values arrive as the dialog's string map, keyed by the
//! option labels in the registry.
//!
//! # Dependencies
//! - crates/io/src/formats.rs (`BomFlavour`, `PART_FIELDS`, `bom_flavour`)
//! - crates/io/data/bom_flavours.json
//! - crates/engine/src/lib.rs (`bom_export`)
//! - crates/doc/src/lib.rs (`Project`, `BOM_EXCLUDE_FIELD`)
//!
//! # Do not
//! - Read `Project.components` to build rows; the engine owns grouping and skipping.
use pcbcraft_doc::{Id, Project};
use pcbcraft_io::formats::{BomFlavour, PART_FIELDS, PartField, bom_flavour, default_bom_flavour};
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
    /// False when the flavour's file has no header row (the preview still names columns).
    pub header: bool,
    /// Parts that lack a field the flavour needs, one entry per field.
    pub missing: Vec<Missing>,
}

/// Parts that lack one field the flavour needs.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Missing {
    /// Plain words for the field, for example "supplier part number".
    pub what: String,
    /// The references of the parts without it, sorted.
    pub references: Vec<String>,
}

/// The flavour named in `values`, or the first one.
pub(crate) fn flavour_of(values: &BTreeMap<String, String>) -> &'static BomFlavour {
    values
        .get(FLAVOUR)
        .and_then(|id| bom_flavour(id))
        .unwrap_or_else(default_bom_flavour)
}

/// The option values a fresh bill starts with: the flavour's file type, every column of
/// the flavour and of the project, grouping as the flavour says.
pub(crate) fn default_values(project: &Project, flavour: &BomFlavour) -> BTreeMap<String, String> {
    let mut columns: Vec<String> = flavour.columns.iter().map(|c| c.token.clone()).collect();
    columns.extend(
        project
            .bom_fields
            .iter()
            .map(|f| format!("field:{}", f.key)),
    );
    BTreeMap::from([
        (FLAVOUR.to_owned(), flavour.id.clone()),
        (FILE_TYPE.to_owned(), flavour.file_type.clone()),
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
        // A file type the flavour cannot be written as falls back to its own.
        let wanted = get(FILE_TYPE);
        let file_type = if flavour.file_types.contains(&wanted) {
            wanted
        } else {
            flavour.file_type.clone()
        };
        Self {
            flavour,
            file_type,
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

/// The value of a standard part field on a bill item: the key, else its older spelling.
fn part_value(item: &Value, field: &PartField) -> String {
    let get = |k: &str| item["fields"][k].as_str().unwrap_or("").to_owned();
    let v = get(field.key);
    match field.alias {
        Some(alias) if v.is_empty() => get(alias),
        _ => v,
    }
}

fn part_field(key: &str) -> Option<&'static PartField> {
    PART_FIELDS.iter().find(|f| f.key == key)
}

/// Surface mount, Thru-hole or Hybrid, from the pads of the part's library package;
/// empty when the part has no library package to read.
fn mount_of(project: &Project, reference: &str) -> String {
    let Some(footprint) = project
        .components
        .iter()
        .find(|c| c.reference == reference)
        .and_then(|c| c.library_footprint.as_ref())
    else {
        return String::new();
    };
    let pads = &footprint.definition.pads;
    let through = pads.iter().any(|p| p.pad_type == "thru_hole");
    let surface = pads.iter().any(|p| p.pad_type == "smd");
    match (surface, through) {
        (true, true) => "Hybrid",
        (false, true) => "Thru-hole",
        (true, false) => "Surface mount",
        (false, false) => "",
    }
    .to_owned()
}

/// Build the rows for `options`.
///
/// Why: one place decides which parts, columns and headers a bill has, and which parts
/// lack what the flavour needs.
///
/// # Rules
/// - Columns follow the flavour's order, then the project's own columns.
/// - Rows the chosen columns cannot tell apart are merged when grouping.
/// - Parts missing a required field are listed in `missing`; the rows are still built.
///
/// # Dependencies
/// - crates/engine/src/lib.rs `bom_export`.
/// - crates/io/src/formats.rs `PART_FIELDS`.
///
/// # Do not
/// - Fail on an empty project; an empty table is a fine answer.
pub(crate) fn table(project: &Project, o: &Options) -> Result<Table, String> {
    let wants = |token: &str| o.columns.iter().any(|c| c == token);
    let mut keys: Vec<String> = vec![];
    let ask = |keys: &mut Vec<String>, field: &PartField| {
        for key in std::iter::once(field.key).chain(field.alias) {
            if !keys.iter().any(|k| k == key) && keys.len() < 16 {
                keys.push(key.to_owned());
            }
        }
    };
    let chosen: Vec<&pcbcraft_io::formats::BomColumn> = o
        .flavour
        .columns
        .iter()
        .filter(|c| wants(&c.token))
        .collect();
    for c in &chosen {
        if let Some(field) = PART_FIELDS.iter().find(|f| f.token == c.token) {
            ask(&mut keys, field);
        }
        if c.token == "part_key" {
            for key in ["MPN", "SPN"] {
                if let Some(field) = part_field(key) {
                    ask(&mut keys, field);
                }
            }
        }
    }
    for key in o.flavour.requires.iter().chain(&o.flavour.requires_any) {
        if let Some(field) = part_field(key) {
            ask(&mut keys, field);
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
    let mut headers: Vec<String> = chosen.iter().map(|c| c.header.clone()).collect();
    headers.extend(extra.iter().map(|(_, label)| label.clone()));
    let text = |v: &Value| v.as_str().unwrap_or("").to_owned();
    struct Row {
        refs: Vec<String>,
        quantity: u64,
        cells: Vec<String>,
    }
    let mut rows: Vec<Row> = vec![];
    let mut seen: BTreeMap<Vec<String>, usize> = BTreeMap::new();
    let mut missing: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for item in &items {
        let refs: Vec<String> = item["references"]
            .as_array()
            .map(|a| a.iter().map(&text).collect())
            .unwrap_or_default();
        for key in &o.flavour.requires {
            if let Some(field) = part_field(key)
                && part_value(item, field).is_empty()
            {
                missing
                    .entry(field.label.to_owned())
                    .or_default()
                    .extend(refs.iter().cloned());
            }
        }
        let any: Vec<&PartField> = o
            .flavour
            .requires_any
            .iter()
            .filter_map(|k| part_field(k))
            .collect();
        if !any.is_empty() && any.iter().all(|f| part_value(item, f).is_empty()) {
            let what = any.iter().map(|f| f.label).collect::<Vec<_>>().join(" or ");
            missing
                .entry(what)
                .or_default()
                .extend(refs.iter().cloned());
        }
        let field = |k: &str| text(&item["fields"][k]);
        let first = refs.first().cloned().unwrap_or_default();
        let mut cells: Vec<String> = chosen
            .iter()
            .map(|c| match c.token.as_str() {
                "value" => text(&item["value"]),
                "footprint" => text(&item["footprint"]),
                "mount" => mount_of(project, &first),
                "part_key" => {
                    let by = |k: &str| {
                        part_field(k)
                            .map(|f| part_value(item, f))
                            .unwrap_or_default()
                    };
                    let mpn = by("MPN");
                    if mpn.is_empty() { by("SPN") } else { mpn }
                }
                token => PART_FIELDS
                    .iter()
                    .find(|f| f.token == token)
                    .map(|f| part_value(item, f))
                    .unwrap_or_default(),
            })
            .collect();
        cells.extend(extra.iter().map(|(key, _)| field(key)));
        let quantity = item["quantity"].as_u64().unwrap_or(0);
        // The cells that tell parts apart: everything except references, quantity and line.
        let key: Vec<String> = chosen
            .iter()
            .zip(&cells)
            .filter(|(c, _)| !matches!(c.token.as_str(), "reference" | "quantity" | "line"))
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
        .enumerate()
        .map(|(n, mut r)| {
            r.refs.sort();
            let joined = r.refs.join(&o.flavour.reference_separator);
            for (i, c) in chosen.iter().enumerate() {
                match c.token.as_str() {
                    "reference" => r.cells[i] = joined.clone(),
                    "quantity" => r.cells[i] = r.quantity.to_string(),
                    "line" => r.cells[i] = (n + 1).to_string(),
                    _ => {}
                }
            }
            r.cells
        })
        .collect();
    let missing = missing
        .into_iter()
        .map(|(what, mut references)| {
            references.sort();
            references.dedup();
            Missing { what, references }
        })
        .collect();
    Ok(Table {
        headers,
        rows,
        header: o.flavour.header,
        missing,
    })
}

/// Warnings about what the receiving site will not take: too many lines or too big a file.
pub(crate) fn limit_notes(table: &Table, o: &Options) -> Vec<String> {
    let limits = o.flavour.limits;
    let mut notes = vec![];
    if let Some(max) = limits.max_rows
        && table.rows.len() > max
    {
        notes.push(format!(
            "This list has {} lines; the site takes {max} unless it raises your limit.",
            table.rows.len()
        ));
    }
    if let Some(max) = limits.max_bytes {
        let size = render(table, &o.file_type).len();
        if size > max {
            notes.push(format!("This file is {size} bytes; the site takes {max}."));
        }
    }
    notes
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
            let mut out = String::new();
            if table.header {
                out = table
                    .headers
                    .iter()
                    .map(clean)
                    .collect::<Vec<_>>()
                    .join("\t");
                out.push('\n');
            }
            for row in &table.rows {
                out.push_str(&row.iter().map(clean).collect::<Vec<_>>().join("\t"));
                out.push('\n');
            }
            out
        }
        _ => {
            let mut out = String::new();
            if table.header {
                out = table
                    .headers
                    .iter()
                    .map(|h| csv_cell(h))
                    .collect::<Vec<_>>()
                    .join(",");
                out.push('\n');
            }
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
    if o.flavour.id != default_bom_flavour().id {
        parts.insert(0, o.flavour.label.clone());
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
        let list = pcbcraft_io::formats::parse_bom_flavours(
            r#"[{"id":"test_assembler","label":"Test assembler","tip":"Only for this test.",
            "file_type":"tsv","file_types":["tsv"],"group":true,"reference_separator":";",
            "columns":[{"token":"quantity","header":"Qty"},{"token":"reference","header":"Designator"},
            {"token":"value","header":"Comment"}]}]"#,
        )?;
        // Leaked once per test run so the flavour can be borrowed for 'static.
        let assembler: &'static BomFlavour = Box::leak(Box::new(list[0].clone()));
        let p = project()?;
        let values = default_values(&p, assembler);
        let mut o = Options::from_values(&p, &values, &[]);
        // `from_values` looks flavours up by id; this one is not in the table.
        o.flavour = assembler;
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
        let mut values = default_values(&p, default_bom_flavour());
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

    /// The showcase board with sourcing fields stamped on every part, so each flavour
    /// has something to print. Parts of one value and package share a part number.
    fn stamped_showcase(supplier_numbers: bool) -> Result<Project, String> {
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let path = dir.path().join("showcase.pcbcraft");
        std::fs::write(&path, include_str!("../../../examples/showcase.pcbcraft"))
            .map_err(|e| e.to_string())?;
        let mut p = pcbcraft_io::load(&path)?;
        for c in &mut p.components {
            let slug: String = c
                .value
                .chars()
                .filter(|ch| ch.is_ascii_alphanumeric())
                .collect();
            let mut stamp = vec![
                ("Manufacturer", "Maker".to_owned()),
                ("MPN", format!("MP-{slug}")),
                ("Supplier", "Shop".to_owned()),
            ];
            if supplier_numbers {
                stamp.push(("SPN", format!("S-{slug}")));
            }
            for (key, value) in stamp {
                c.fields.push(pcbcraft_doc::ComponentField {
                    key: key.into(),
                    value,
                });
            }
        }
        Ok(p)
    }

    fn golden_dir() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/bom")
    }

    /// One golden file per bundled flavour on the showcase board. Set
    /// `PCBCRAFT_BLESS=1` to rewrite them after a deliberate change.
    #[test]
    fn every_bundled_flavour_matches_its_golden_file_on_the_showcase() -> Result<(), String> {
        let p = stamped_showcase(true)?;
        let bless = std::env::var("PCBCRAFT_BLESS").is_ok();
        for flavour in pcbcraft_io::formats::bom_flavours() {
            let values = default_values(&p, flavour);
            let o = Options::from_values(&p, &values, &[]);
            let t = table(&p, &o)?;
            assert!(
                t.missing.is_empty(),
                "{}: the stamped showcase has every field: {:?}",
                flavour.id,
                t.missing
            );
            let text = render(&t, &o.file_type);
            let file = golden_dir().join(format!("{}.{}", flavour.id, o.file_type));
            if bless {
                std::fs::create_dir_all(golden_dir()).map_err(|e| e.to_string())?;
                std::fs::write(&file, &text).map_err(|e| e.to_string())?;
            }
            let want = std::fs::read_to_string(&file)
                .map_err(|e| format!("{}: {e}; run with PCBCRAFT_BLESS=1", file.display()))?;
            assert_eq!(text, want, "{} differs from its golden file", flavour.id);
            assert!(
                limit_notes(&t, &o).is_empty(),
                "{}: the showcase fits the site's limits",
                flavour.id
            );
        }
        Ok(())
    }

    #[test]
    fn a_flavour_prints_headers_quantities_and_joiners_as_it_specifies() -> Result<(), String> {
        let p = stamped_showcase(true)?;
        let on = |id: &str| -> Result<(Table, String), String> {
            let f = bom_flavour(id).ok_or(format!("no flavour {id}"))?;
            let o = Options::from_values(&p, &default_values(&p, f), &[]);
            let t = table(&p, &o)?;
            let text = render(&t, &o.file_type);
            Ok((t, text))
        };
        // The quantity of a row is the number of references in it.
        for f in pcbcraft_io::formats::bom_flavours() {
            let o = Options::from_values(&p, &default_values(&p, f), &[]);
            let t = table(&p, &o)?;
            let (Some(refs), Some(qty)) = (
                f.columns.iter().position(|c| c.token == "reference"),
                f.columns.iter().position(|c| c.token == "quantity"),
            ) else {
                continue;
            };
            for row in &t.rows {
                assert_eq!(
                    row[refs]
                        .split(f.reference_separator.trim())
                        .count()
                        .to_string(),
                    row[qty],
                    "{}: quantity equals the reference count in {:?}",
                    f.id,
                    row
                );
            }
        }
        let (_, no_header) = on("asm_proto_c")?;
        assert!(!no_header.starts_with("Designator"), "no header row");
        let (a, _) = on("asm_proto_a")?;
        assert_eq!(
            a.headers,
            ["Comment", "Designator", "Footprint", "Supplier Part #"]
        );
        let (b, _) = on("asm_proto_b")?;
        assert_eq!(
            b.rows.iter().map(|r| r[0].clone()).collect::<Vec<_>>(),
            (1..=b.rows.len())
                .map(|n| n.to_string())
                .collect::<Vec<_>>(),
            "line numbers count up from 1"
        );
        let types: Vec<&str> = b.rows.iter().map(|r| r[6].as_str()).collect();
        assert!(
            types
                .iter()
                .all(|t| ["Surface mount", "Thru-hole", "Hybrid"].contains(t)),
            "every Type is one of the three words: {types:?}"
        );
        assert!(types.contains(&"Surface mount") && types.contains(&"Thru-hole"));
        Ok(())
    }

    #[test]
    fn parts_without_the_needed_field_are_named_and_the_rows_still_build() -> Result<(), String> {
        let p = stamped_showcase(false)?;
        let f = bom_flavour("asm_proto_a").ok_or("no flavour")?;
        let o = Options::from_values(&p, &default_values(&p, f), &[]);
        let t = table(&p, &o)?;
        assert_eq!(t.missing.len(), 1);
        assert_eq!(t.missing[0].what, "supplier part number");
        assert_eq!(t.missing[0].references.len(), p.components.len());
        assert!(!t.rows.is_empty(), "blank cells, not a refusal");
        // A flavour that takes either number is content with the manufacturer's.
        let g = bom_flavour("dist_us_b").ok_or("no flavour")?;
        let o = Options::from_values(&p, &default_values(&p, g), &[]);
        assert!(table(&p, &o)?.missing.is_empty());
        Ok(())
    }

    #[test]
    fn a_site_limit_is_reported_when_the_list_exceeds_it() -> Result<(), String> {
        let p = stamped_showcase(true)?;
        let f = bom_flavour("dist_us_b").ok_or("no flavour")?;
        let mut o = Options::from_values(&p, &default_values(&p, f), &[]);
        o.group = false;
        let t = table(&p, &o)?;
        assert!(t.rows.len() <= 99 || !limit_notes(&t, &o).is_empty());
        let mut tight = f.clone();
        tight.limits.max_rows = Some(2);
        let tight: &'static BomFlavour = Box::leak(Box::new(tight));
        o.flavour = tight;
        let notes = limit_notes(&t, &o);
        assert_eq!(notes.len(), 1, "{notes:?}");
        assert!(notes[0].contains("takes 2"));
        Ok(())
    }
}
