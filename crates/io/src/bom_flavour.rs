//! Vendor-neutral bill of materials flavours as bundled data and a table writer.
//!
//! Context: issue #226. Each flavour is one record in `data/bom_flavours.json`.
//! The writer turns `bom.export` rows into headers and file text; it does not read
//! components for grouping (the engine owns that).
//!
//! # Dependencies
//! - crates/io/data/bom_flavours.json
//! - crates/engine/src/lib.rs (`bom_export`, for callers and tests)
//!
//! # Do not
//! - Name a company in the data file; describe the recipient role instead.
use pcbcraft_doc::{Id, Project};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// One column of a bill of materials flavour.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BomColumn {
    /// What fills the column: a [`DERIVED_TOKENS`] entry or a [`PART_FIELDS`] token.
    pub token: String,
    /// The header text the file carries.
    pub header: String,
}

/// What a recipient says it will not take.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BomLimits {
    /// Most rows (lines) in one upload.
    #[serde(default)]
    pub max_rows: Option<usize>,
    /// Largest file in bytes.
    #[serde(default)]
    pub max_bytes: Option<usize>,
}

fn yes() -> bool {
    true
}

/// One vendor flavour of the bill of materials, as pure data.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BomProfile {
    pub id: String,
    pub label: String,
    pub tip: String,
    #[serde(default = "yes")]
    pub verified: bool,
    pub file_type: String,
    pub file_types: Vec<String>,
    #[serde(default = "yes")]
    pub header: bool,
    pub columns: Vec<BomColumn>,
    pub group: bool,
    pub reference_separator: String,
    #[serde(default)]
    pub requires: Vec<String>,
    #[serde(default)]
    pub requires_any: Vec<String>,
    #[serde(default)]
    pub limits: BomLimits,
}

/// One standard part field: the key a part carries it under and the column token that
/// prints it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PartField {
    pub key: &'static str,
    pub token: &'static str,
    pub alias: Option<&'static str>,
    pub label: &'static str,
}

/// The part fields a flavour can ask for. A flavour's `requires` names these keys.
pub const PART_FIELDS: &[PartField] = &[
    PartField {
        key: "Manufacturer",
        token: "manufacturer",
        alias: None,
        label: "manufacturer",
    },
    PartField {
        key: "MPN",
        token: "mpn",
        alias: None,
        label: "manufacturer part number",
    },
    PartField {
        key: "Description",
        token: "description",
        alias: Some("description"),
        label: "description",
    },
    PartField {
        key: "Supplier",
        token: "supplier",
        alias: None,
        label: "supplier",
    },
    PartField {
        key: "SPN",
        token: "spn",
        alias: None,
        label: "supplier part number",
    },
];

/// Column tokens worked out from the design rather than typed in.
pub const DERIVED_TOKENS: &[&str] = &[
    "reference",
    "quantity",
    "value",
    "footprint",
    "line",
    "mount",
    "part_key",
];

const BOM_FLAVOURS_JSON: &str = include_str!("../data/bom_flavours.json");

/// Parse a flavour file: a JSON array of [`BomProfile`].
pub fn parse_bom_flavours(text: &str) -> Result<Vec<BomProfile>, String> {
    serde_json::from_str(text).map_err(|e| format!("bom_flavours.json: {e}"))
}

fn fallback_profile() -> BomProfile {
    let column = |token: &str, header: &str| BomColumn {
        token: token.into(),
        header: header.into(),
    };
    BomProfile {
        id: "generic_csv".into(),
        label: "Generic CSV".into(),
        tip: "A plain spreadsheet file with the usual columns.".into(),
        verified: true,
        file_type: "csv".into(),
        file_types: vec!["csv".into(), "tsv".into(), "json".into()],
        header: true,
        columns: vec![
            column("reference", "Reference"),
            column("value", "Value"),
            column("footprint", "Footprint"),
            column("mpn", "MPN"),
            column("manufacturer", "Manufacturer"),
            column("quantity", "Quantity"),
        ],
        group: true,
        reference_separator: ", ".into(),
        requires: vec![],
        requires_any: vec![],
        limits: BomLimits::default(),
    }
}

/// Every bill of materials flavour, in file order. The first is the default.
pub fn bom_profiles() -> &'static [BomProfile] {
    static ALL: std::sync::OnceLock<Vec<BomProfile>> = std::sync::OnceLock::new();
    ALL.get_or_init(|| match parse_bom_flavours(BOM_FLAVOURS_JSON) {
        Ok(list) if !list.is_empty() => list,
        _ => vec![fallback_profile()],
    })
}

/// The default flavour: the first one.
pub fn default_bom_profile() -> &'static BomProfile {
    &bom_profiles()[0]
}

/// A bill of materials flavour by id.
pub fn bom_profile(id: &str) -> Option<&'static BomProfile> {
    bom_profiles().iter().find(|f| f.id == id)
}

/// What one export of the bill asks for.
pub struct ExportOptions {
    pub profile: &'static BomProfile,
    pub file_type: String,
    pub variant: String,
    pub columns: Vec<String>,
    pub group: bool,
    pub only: Vec<Id>,
}

/// Rows ready to show or write.
pub struct BomTable {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub header: bool,
    pub missing: Vec<MissingField>,
}

/// Parts that lack one field the flavour needs.
#[derive(Debug, PartialEq, Eq)]
pub struct MissingField {
    pub what: String,
    pub references: Vec<String>,
}

/// The profile named in `values`, or the first one.
pub fn profile_of(values: &BTreeMap<String, String>) -> &'static BomProfile {
    values
        .get("Flavour")
        .and_then(|id| bom_profile(id))
        .unwrap_or_else(default_bom_profile)
}

/// Option values for a fresh bill: file type, every column, grouping as the profile says.
pub fn default_export_values(
    project: &Project,
    profile: &BomProfile,
) -> BTreeMap<String, String> {
    let mut columns: Vec<String> = profile.columns.iter().map(|c| c.token.clone()).collect();
    columns.extend(
        project
            .bom_fields
            .iter()
            .map(|f| format!("field:{}", f.key)),
    );
    BTreeMap::from([
        ("Flavour".to_owned(), profile.id.clone()),
        ("File type".to_owned(), profile.file_type.clone()),
        ("Build".to_owned(), String::new()),
        ("Columns".to_owned(), columns.join(",")),
        (
            "Group identical parts".to_owned(),
            profile.group.to_string(),
        ),
    ])
}

impl ExportOptions {
    /// Read dialog values; anything missing falls back to the profile.
    pub fn from_values(
        project: &Project,
        values: &BTreeMap<String, String>,
        only: &[Id],
    ) -> Self {
        let profile = profile_of(values);
        let fallback = default_export_values(project, profile);
        let get = |key: &str| {
            values
                .get(key)
                .or_else(|| fallback.get(key))
                .cloned()
                .unwrap_or_default()
        };
        let wanted = get("File type");
        let file_type = if profile.file_types.contains(&wanted) {
            wanted
        } else {
            profile.file_type.clone()
        };
        Self {
            profile,
            file_type,
            variant: get("Build"),
            columns: get("Columns")
                .split(',')
                .filter(|c| !c.is_empty())
                .map(str::to_owned)
                .collect(),
            group: get("Group identical parts") != "false",
            only: only.to_vec(),
        }
    }

    /// JSON parameters for `bom.export` from these options.
    pub fn engine_params(&self, project: &Project) -> Value {
        let wants = |token: &str| self.columns.iter().any(|c| c == token);
        let mut keys: Vec<String> = vec![];
        let ask = |keys: &mut Vec<String>, field: &PartField| {
            for key in std::iter::once(field.key).chain(field.alias) {
                if !keys.iter().any(|k| k == key) && keys.len() < 16 {
                    keys.push(key.to_owned());
                }
            }
        };
        let chosen: Vec<&BomColumn> = self
            .profile
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
        for key in self
            .profile
            .requires
            .iter()
            .chain(&self.profile.requires_any)
        {
            if let Some(field) = part_field(key) {
                ask(&mut keys, field);
            }
        }
        for f in &project.bom_fields {
            if wants(&format!("field:{}", f.key)) && !keys.contains(&f.key) && keys.len() < 16 {
                keys.push(f.key.clone());
            }
        }
        let mut params = json!({"fields": keys, "group": self.group});
        if !self.variant.is_empty() {
            params["variant"] = json!(self.variant);
        }
        if !self.only.is_empty() {
            params["ids"] = json!(self.only);
        }
        params
    }
}

fn part_field(key: &str) -> Option<&'static PartField> {
    PART_FIELDS.iter().find(|f| f.key == key)
}

fn part_value(item: &Value, field: &PartField) -> String {
    let get = |k: &str| item["fields"][k].as_str().unwrap_or("").to_owned();
    let v = get(field.key);
    match field.alias {
        Some(alias) if v.is_empty() => get(alias),
        _ => v,
    }
}

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

/// Build a table from `bom.export` items.
pub fn table_from_items(
    project: &Project,
    items: &[Value],
    o: &ExportOptions,
) -> Result<BomTable, String> {
    let wants = |token: &str| o.columns.iter().any(|c| c == token);
    let chosen: Vec<&BomColumn> = o
        .profile
        .columns
        .iter()
        .filter(|c| wants(&c.token))
        .collect();
    let mut extra: Vec<(String, String)> = vec![];
    for f in &project.bom_fields {
        if wants(&format!("field:{}", f.key)) && extra.len() < 16 {
            let label = if f.label.is_empty() { &f.key } else { &f.label };
            extra.push((f.key.clone(), label.clone()));
        }
    }
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
    for item in items {
        let refs: Vec<String> = item["references"]
            .as_array()
            .map(|a| a.iter().map(&text).collect())
            .unwrap_or_default();
        for key in &o.profile.requires {
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
            .profile
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
            let joined = r.refs.join(&o.profile.reference_separator);
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
            MissingField { what, references }
        })
        .collect();
    Ok(BomTable {
        headers,
        rows,
        header: o.profile.header,
        missing,
    })
}

/// Warnings when the receiving site will not take the file size or row count.
pub fn limit_notes(table: &BomTable, o: &ExportOptions) -> Vec<String> {
    let limits = o.profile.limits;
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
        let size = render_bom(table, &o.file_type).len();
        if size > max {
            notes.push(format!("This file is {size} bytes; the site takes {max}."));
        }
    }
    notes
}

fn csv_cell(text: &str) -> String {
    if text.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text.to_owned()
    }
}

/// File text for `table` as `csv`, `tsv` or `json`.
pub fn render_bom(table: &BomTable, file_type: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;
    use pcbcraft_engine::Session;

    fn tiny_project() -> Result<Project, String> {
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

    fn export_table(p: &Project, o: &ExportOptions) -> Result<BomTable, String> {
        let data =
            pcbcraft_engine::bom_export(p, &o.engine_params(p)).map_err(|e| e.to_string())?;
        let items = data["items"]
            .as_array()
            .cloned()
            .ok_or("bom.export returned no items")?;
        table_from_items(p, &items, o)
    }

    #[test]
    fn the_bundled_flavour_file_parses_and_leads_with_the_plain_list() -> Result<(), String> {
        let all = parse_bom_flavours(BOM_FLAVOURS_JSON)?;
        assert_eq!(all[0].id, "generic_csv");
        assert!(all.len() >= 14);
        assert_eq!(bom_profiles().len(), all.len());
        let mut ids: Vec<&str> = all.iter().map(|f| f.id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), all.len());
        Ok(())
    }

    #[test]
    fn every_required_field_of_every_flavour_exists_in_the_part_field_model() {
        let known = |key: &String| PART_FIELDS.iter().any(|f| f.key == key);
        for f in bom_profiles() {
            for key in f.requires.iter().chain(&f.requires_any) {
                assert!(known(key), "{} requires {key}, not a part field", f.id);
            }
            for c in &f.columns {
                assert!(
                    DERIVED_TOKENS.contains(&c.token.as_str())
                        || PART_FIELDS.iter().any(|p| p.token == c.token),
                    "{} has an unknown column token {}",
                    f.id,
                    c.token
                );
            }
        }
    }

    #[test]
    fn every_flavour_is_well_formed_and_unverified_ones_say_so() {
        for f in bom_profiles() {
            assert!(f.file_types.contains(&f.file_type), "{}: start type", f.id);
            assert!(!f.columns.is_empty(), "{}: columns", f.id);
            assert!(!f.tip.is_empty() && !f.label.is_empty(), "{}: words", f.id);
            assert_eq!(
                !f.verified,
                f.tip.starts_with("Unverified:") || f.tip.contains("Unverified:"),
                "{}: an unverified flavour says so in its tip",
                f.id
            );
        }
    }

    #[test]
    fn a_flavour_defined_purely_as_data_exports_correctly() -> Result<(), String> {
        let list = parse_bom_flavours(
            r#"[{"id":"test_assembler","label":"Test assembler","tip":"Only for this test.",
            "file_type":"tsv","file_types":["tsv"],"group":true,"reference_separator":";",
            "columns":[{"token":"quantity","header":"Qty"},{"token":"reference","header":"Designator"},
            {"token":"value","header":"Comment"}]}]"#,
        )?;
        let assembler: &'static BomProfile = Box::leak(Box::new(list[0].clone()));
        let p = tiny_project()?;
        let values = default_export_values(&p, assembler);
        let mut o = ExportOptions::from_values(&p, &values, &[]);
        o.profile = assembler;
        let t = export_table(&p, &o)?;
        assert_eq!(
            render_bom(&t, &o.file_type),
            "Qty\tDesignator\tComment\n1\tC1\t100n\n2\tR1;R2\t10k\n"
        );
        Ok(())
    }

    fn stamped_showcase(supplier_numbers: bool) -> Result<Project, String> {
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let path = dir.path().join("showcase.pcbcraft");
        std::fs::write(&path, include_str!("../../../examples/showcase.pcbcraft"))
            .map_err(|e| e.to_string())?;
        let mut p = crate::load(&path)?;
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

    #[test]
    fn every_bundled_flavour_matches_its_golden_file_on_the_showcase() -> Result<(), String> {
        let p = stamped_showcase(true)?;
        let bless = std::env::var("PCBCRAFT_BLESS").is_ok();
        for profile in bom_profiles() {
            let values = default_export_values(&p, profile);
            let o = ExportOptions::from_values(&p, &values, &[]);
            let t = export_table(&p, &o)?;
            assert!(
                t.missing.is_empty(),
                "{}: stamped showcase has every field: {:?}",
                profile.id,
                t.missing
            );
            let text = render_bom(&t, &o.file_type);
            let file = golden_dir().join(format!("{}.{}", profile.id, o.file_type));
            if bless {
                std::fs::create_dir_all(golden_dir()).map_err(|e| e.to_string())?;
                std::fs::write(&file, &text).map_err(|e| e.to_string())?;
            }
            let want = std::fs::read_to_string(&file)
                .map_err(|e| format!("{}: {e}; run with PCBCRAFT_BLESS=1", file.display()))?;
            assert_eq!(text, want, "{} differs from its golden file", profile.id);
            assert!(
                limit_notes(&t, &o).is_empty(),
                "{}: the showcase fits the site's limits",
                profile.id
            );
        }
        Ok(())
    }

    #[test]
    fn a_flavour_prints_headers_quantities_and_joiners_as_it_specifies() -> Result<(), String> {
        let p = stamped_showcase(true)?;
        let on = |id: &str| -> Result<(BomTable, String), String> {
            let f = bom_profile(id).ok_or(format!("no flavour {id}"))?;
            let o = ExportOptions::from_values(&p, &default_export_values(&p, f), &[]);
            let t = export_table(&p, &o)?;
            let text = render_bom(&t, &o.file_type);
            Ok((t, text))
        };
        for f in bom_profiles() {
            let o = ExportOptions::from_values(&p, &default_export_values(&p, f), &[]);
            let t = export_table(&p, &o)?;
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
        let f = bom_profile("asm_proto_a").ok_or("no flavour")?;
        let o = ExportOptions::from_values(&p, &default_export_values(&p, f), &[]);
        let t = export_table(&p, &o)?;
        assert_eq!(t.missing.len(), 1);
        assert_eq!(t.missing[0].what, "supplier part number");
        assert_eq!(t.missing[0].references.len(), p.components.len());
        assert!(!t.rows.is_empty(), "blank cells, not a refusal");
        let g = bom_profile("dist_us_b").ok_or("no flavour")?;
        let o = ExportOptions::from_values(&p, &default_export_values(&p, g), &[]);
        assert!(export_table(&p, &o)?.missing.is_empty());
        Ok(())
    }

    #[test]
    fn a_site_limit_is_reported_when_the_list_exceeds_it() -> Result<(), String> {
        let p = stamped_showcase(true)?;
        let f = bom_profile("dist_us_b").ok_or("no flavour")?;
        let mut o = ExportOptions::from_values(&p, &default_export_values(&p, f), &[]);
        o.group = false;
        let t = export_table(&p, &o)?;
        assert!(t.rows.len() <= 99 || !limit_notes(&t, &o).is_empty());
        let mut tight = f.clone();
        tight.limits.max_rows = Some(2);
        let tight: &'static BomProfile = Box::leak(Box::new(tight));
        o.profile = tight;
        let notes = limit_notes(&t, &o);
        assert_eq!(notes.len(), 1, "{notes:?}");
        assert!(notes[0].contains("takes 2"));
        Ok(())
    }
}
