//! Bill of materials grouping, house-specific CSV, and minimal XLSX output.
use pcbcraft_doc::{Component, Project, Variant};
use std::collections::BTreeMap;
use std::fmt::Write as _;

const MAX_BOM_PROPERTIES: usize = 16;
const MAX_BOM_ROWS: usize = 20_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum BomGrouping {
    ValueFootprint,
    #[default]
    ValueFootprintMpn,
    Mpn,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DnpMode {
    #[default]
    Exclude,
    Section,
    Column,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum BomHouse {
    #[default]
    Generic,
    JlcPcb,
    PcbWay,
    Seeed,
}

#[derive(Clone, Debug)]
pub struct BomOptions {
    pub grouping: BomGrouping,
    pub dnp: DnpMode,
    pub fields: Vec<String>,
    pub variant: Option<String>,
    pub house: BomHouse,
    pub ranges: bool,
    /// When true, generic CSV may append a trailing `# N lines, M parts` comment.
    pub summary: bool,
    /// Lexicographic reference order (legacy `bom_csv`); otherwise natural order.
    pub legacy_reference_order: bool,
    /// Omit the extra Manufacturer and MPN columns (legacy `bom_csv` layout).
    pub legacy_generic_columns: bool,
}

impl Default for BomOptions {
    fn default() -> Self {
        Self {
            grouping: BomGrouping::ValueFootprintMpn,
            dnp: DnpMode::Exclude,
            fields: Vec::new(),
            variant: None,
            house: BomHouse::Generic,
            ranges: true,
            summary: false,
            legacy_reference_order: false,
            legacy_generic_columns: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BomRow {
    pub refs: Vec<String>,
    pub quantity: usize,
    pub value: String,
    pub kind: String,
    pub footprint: String,
    pub manufacturer: String,
    pub mpn: String,
    pub distributor: Vec<(String, String)>,
    pub jlcpcb: String,
    pub link: String,
    pub fields: Vec<String>,
    pub dnp: bool,
}

#[derive(Clone, Debug)]
pub struct BomTable {
    pub columns: Vec<String>,
    pub rows: Vec<BomRow>,
    pub lines: usize,
    pub parts: usize,
}

#[derive(Clone, Debug, Default)]
pub struct PartFields {
    pub manufacturer: String,
    pub mpn: String,
    pub jlcpcb: String,
    pub mouser: String,
    pub digikey: String,
    pub link: String,
    pub distributor: Vec<(String, String)>,
}

pub fn part_fields(c: &Component) -> PartFields {
    let mut out = PartFields::default();
    for f in &c.fields {
        if f.key.eq_ignore_ascii_case("Manufacturer") {
            out.manufacturer = f.value.clone();
        } else if f.key.eq_ignore_ascii_case("MPN") || f.key.eq_ignore_ascii_case("Part Number") {
            if out.mpn.is_empty() {
                out.mpn = f.value.clone();
            }
        } else if f.key.eq_ignore_ascii_case("JLCPCB Part #") || f.key.eq_ignore_ascii_case("LCSC")
        {
            out.jlcpcb = f.value.clone();
            push_dist(&mut out.distributor, "JLCPCB", &f.value);
        } else if f.key.eq_ignore_ascii_case("Mouser") {
            out.mouser = f.value.clone();
            push_dist(&mut out.distributor, "Mouser", &f.value);
        } else if f.key.eq_ignore_ascii_case("Digi-Key") || f.key.eq_ignore_ascii_case("DigiKey") {
            out.digikey = f.value.clone();
            push_dist(&mut out.distributor, "Digi-Key", &f.value);
        } else if (f.key.eq_ignore_ascii_case("Link") || f.key.eq_ignore_ascii_case("URL"))
            && out.link.is_empty()
        {
            out.link = f.value.clone();
        }
    }
    out
}

fn push_dist(list: &mut Vec<(String, String)>, name: &str, value: &str) {
    if !value.is_empty() {
        list.push((name.into(), value.into()));
    }
}

pub fn bom_rows(p: &Project, opts: &BomOptions) -> Result<BomTable, String> {
    p.validate()?;
    validate_fields(&opts.fields)?;
    if matches!(opts.dnp, DnpMode::Section | DnpMode::Column) && opts.house != BomHouse::Generic {
        return Err("only the generic bill accepts do-not-populate sections or columns".into());
    }
    let variant = resolve_variant(p, opts.variant.as_deref())?;
    let columns = bom_columns(p, &opts.fields)?;
    let mut groups: BTreeMap<GroupKey, Vec<ComponentLine>> = BTreeMap::new();
    let mut parts = 0usize;
    for c in &p.components {
        let variant_dnp = variant.is_some_and(|v| v.dnp.contains(&c.id));
        let row_dnp = variant_dnp;
        if row_dnp && opts.dnp == DnpMode::Exclude {
            continue;
        }
        if !row_dnp {
            parts += 1;
        }
        let footprint = component_footprint(c, variant);
        let pf = part_fields(c);
        let field_values: Vec<String> = columns
            .iter()
            .map(|(key, _)| c.field(key).unwrap_or("").to_string())
            .collect();
        let mut key = group_key(
            opts.grouping,
            &c.value,
            &c.kind,
            &footprint,
            &pf.mpn,
            &field_values,
        );
        if row_dnp {
            key.dnp_line = true;
        }
        groups.entry(key).or_default().push(ComponentLine {
            reference: c.reference.clone(),
            value: c.value.clone(),
            kind: c.kind.clone(),
            footprint,
            pf,
            field_values,
            dnp: row_dnp,
        });
    }
    let mut rows = Vec::new();
    let mut dnp_rows = Vec::new();
    for (_key, mut lines) in groups {
        if lines.is_empty() {
            continue;
        }
        lines.sort_by(|a, b| {
            reference_cmp(
                a.reference.as_str(),
                b.reference.as_str(),
                opts.legacy_reference_order,
            )
        });
        let sample = &lines[0];
        let refs: Vec<String> = lines.iter().map(|l| l.reference.clone()).collect();
        let quantity = lines.len();
        let row = BomRow {
            refs,
            quantity,
            value: sample.value.clone(),
            kind: sample.kind.clone(),
            footprint: sample.footprint.clone(),
            manufacturer: sample.pf.manufacturer.clone(),
            mpn: sample.pf.mpn.clone(),
            distributor: sample.pf.distributor.clone(),
            jlcpcb: sample.pf.jlcpcb.clone(),
            link: sample.pf.link.clone(),
            fields: sample.field_values.clone(),
            dnp: sample.dnp,
        };
        if row.dnp {
            dnp_rows.push(row);
        } else {
            rows.push(row);
        }
    }
    rows.sort_by(|a, b| {
        a.value
            .cmp(&b.value)
            .then_with(|| a.kind.cmp(&b.kind))
            .then_with(|| a.footprint.cmp(&b.footprint))
            .then_with(|| a.mpn.cmp(&b.mpn))
    });
    dnp_rows.sort_by(|a, b| {
        a.value
            .cmp(&b.value)
            .then_with(|| a.kind.cmp(&b.kind))
            .then_with(|| a.footprint.cmp(&b.footprint))
    });
    if opts.dnp == DnpMode::Section && !dnp_rows.is_empty() {
        rows.push(BomRow {
            refs: vec![],
            quantity: 0,
            value: String::new(),
            kind: String::new(),
            footprint: String::new(),
            manufacturer: String::new(),
            mpn: String::new(),
            distributor: vec![],
            jlcpcb: String::new(),
            link: String::new(),
            fields: vec![String::new(); columns.len()],
            dnp: false,
        });
        rows.push(BomRow {
            refs: vec!["DNP".into()],
            quantity: 0,
            value: String::new(),
            kind: String::new(),
            footprint: String::new(),
            manufacturer: String::new(),
            mpn: String::new(),
            distributor: vec![],
            jlcpcb: String::new(),
            link: String::new(),
            fields: vec![String::new(); columns.len()],
            dnp: false,
        });
        rows.extend(dnp_rows);
    } else if opts.dnp == DnpMode::Exclude {
        // dnp_rows already dropped at component level
    } else if opts.dnp == DnpMode::Column {
        rows.extend(dnp_rows);
    }
    if rows.len() > MAX_BOM_ROWS {
        return Err("bill of materials exceeds the row limit".into());
    }
    let lines = rows
        .iter()
        .filter(|r| !r.refs.is_empty() || r.quantity > 0)
        .count();
    Ok(BomTable {
        columns: columns.into_iter().map(|(_, label)| label).collect(),
        rows,
        lines,
        parts,
    })
}

pub fn bom_csv_with_options(p: &Project, opts: &BomOptions) -> Result<String, String> {
    let table = bom_rows(p, opts)?;
    let mut ranges = opts.ranges;
    if opts.house == BomHouse::Seeed {
        ranges = false;
    }
    let mut out = match opts.house {
        BomHouse::Generic => render_generic_csv(&table, opts, ranges)?,
        BomHouse::JlcPcb => render_jlc_csv(&table, ranges)?,
        BomHouse::PcbWay => render_pcbway_csv(&table, ranges)?,
        BomHouse::Seeed => render_seeed_csv(&table)?,
    };
    if opts.summary && opts.house == BomHouse::Generic {
        out.push_str(&format!("# {} lines, {} parts\n", table.lines, table.parts));
    }
    Ok(out)
}

pub fn bom_xlsx(p: &Project, opts: &BomOptions) -> Result<Vec<u8>, String> {
    let table = bom_rows(p, opts)?;
    let mut ranges = opts.ranges;
    if opts.house == BomHouse::Seeed {
        ranges = false;
    }
    let headers = generic_headers(&table);
    let mut sheet = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
<worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\">\n\
<sheetData>\n",
    );
    write_xlsx_row(&mut sheet, 1, &headers, true)?;
    for (index, row) in table.rows.iter().enumerate() {
        if row.refs.len() == 1 && row.refs[0] == "DNP" {
            write_xlsx_row(&mut sheet, index + 2, &["DNP".to_string()], true)?;
            continue;
        }
        if row.refs.is_empty() && row.quantity == 0 {
            write_xlsx_row(&mut sheet, index + 2, &[], false)?;
            continue;
        }
        let refs = format_references(&row.refs, ranges, opts.legacy_reference_order);
        let mut cells = vec![
            row.quantity.to_string(),
            refs,
            row.value.clone(),
            row.kind.clone(),
            row.footprint.clone(),
        ];
        cells.extend(row.fields.clone());
        cells.push(row.manufacturer.clone());
        cells.push(row.mpn.clone());
        if opts.dnp == DnpMode::Column {
            cells.push(if row.dnp { "yes".into() } else { String::new() });
        }
        write_xlsx_row(&mut sheet, index + 2, &cells, false)?;
    }
    sheet.push_str("</sheetData>\n</worksheet>\n");
    let entries = xlsx_zip_entries(sheet.as_bytes())?;
    super::zip_entries(&entries)
}

fn xlsx_zip_entries(sheet: &[u8]) -> Result<Vec<(String, Vec<u8>)>, String> {
    Ok(vec![
        (
            "[Content_Types].xml".into(),
            br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>
<Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>
<Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/>
</Types>
"#
            .to_vec(),
        ),
        (
            "_rels/.rels".into(),
            br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/>
</Relationships>
"#
            .to_vec(),
        ),
        (
            "xl/workbook.xml".into(),
            br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
<sheets><sheet name="BOM" sheetId="1" r:id="rId1"/></sheets>
</workbook>
"#
            .to_vec(),
        ),
        (
            "xl/_rels/workbook.xml.rels".into(),
            br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/>
<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>
</Relationships>
"#
            .to_vec(),
        ),
        (
            "xl/styles.xml".into(),
            br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
<fonts count="2"><font><sz val="11"/><name val="Calibri"/></font><font><b/><sz val="11"/><name val="Calibri"/></font></fonts>
<fills count="1"><fill><patternFill patternType="none"/></fill></fills>
<borders count="1"><border/></borders>
<cellXfs count="2"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/><xf numFmtId="0" fontId="1" fillId="0" borderId="0"/></cellXfs>
</styleSheet>
"#
            .to_vec(),
        ),
        ("xl/worksheets/sheet1.xml".into(), sheet.to_vec()),
    ])
}

fn write_xlsx_row(
    sheet: &mut String,
    row_num: usize,
    cells: &[String],
    bold: bool,
) -> Result<(), String> {
    let style = if bold { 1 } else { 0 };
    write!(sheet, "<row r=\"{row_num}\">").map_err(|e| e.to_string())?;
    for (col, text) in cells.iter().enumerate() {
        let col_letter = xlsx_col(col);
        let cell_ref = format!("{col_letter}{row_num}");
        if text.chars().all(|c| c.is_ascii_digit()) && !text.is_empty() {
            write!(
                sheet,
                "<c r=\"{cell_ref}\" s=\"{style}\"><v>{}</v></c>",
                xml_escape(text)
            )
            .map_err(|e| e.to_string())?;
        } else {
            write!(
                sheet,
                "<c r=\"{cell_ref}\" s=\"{style}\" t=\"inlineStr\"><is><t>{}</t></is></c>",
                xml_escape(text)
            )
            .map_err(|e| e.to_string())?;
        }
    }
    sheet.push_str("</row>\n");
    Ok(())
}

fn xlsx_col(index: usize) -> String {
    let mut n = index + 1;
    let mut s = String::new();
    while n > 0 {
        let rem = (n - 1) % 26;
        s.insert(0, (b'A' + rem as u8) as char);
        n = (n - 1) / 26;
    }
    s
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
struct GroupKey {
    value: String,
    kind: String,
    footprint: String,
    mpn: String,
    fields: Vec<String>,
    mpn_only: String,
    dnp_line: bool,
}

fn group_key(
    grouping: BomGrouping,
    value: &str,
    kind: &str,
    footprint: &str,
    mpn: &str,
    fields: &[String],
) -> GroupKey {
    match grouping {
        BomGrouping::Mpn => GroupKey {
            value: String::new(),
            kind: String::new(),
            footprint: String::new(),
            mpn: String::new(),
            fields: vec![],
            mpn_only: mpn.to_string(),
            dnp_line: false,
        },
        BomGrouping::ValueFootprintMpn => GroupKey {
            value: value.into(),
            kind: kind.into(),
            footprint: footprint.into(),
            mpn: mpn.into(),
            fields: fields.to_vec(),
            mpn_only: String::new(),
            dnp_line: false,
        },
        BomGrouping::ValueFootprint => GroupKey {
            value: value.into(),
            kind: kind.into(),
            footprint: footprint.into(),
            mpn: String::new(),
            fields: fields.to_vec(),
            mpn_only: String::new(),
            dnp_line: false,
        },
    }
}

struct ComponentLine {
    reference: String,
    value: String,
    kind: String,
    footprint: String,
    pf: PartFields,
    field_values: Vec<String>,
    dnp: bool,
}

fn validate_fields(fields: &[String]) -> Result<(), String> {
    if fields.len() > MAX_BOM_PROPERTIES {
        return Err("a bill of materials lists at most 16 properties".into());
    }
    for key in fields {
        if key.is_empty() || key.len() > 64 {
            return Err("property names must be 1 to 64 bytes".into());
        }
    }
    Ok(())
}

fn resolve_variant<'a>(p: &'a Project, name: Option<&str>) -> Result<Option<&'a Variant>, String> {
    match name.map(str::trim) {
        None | Some("") => Ok(None),
        Some(name) => Ok(Some(
            p.variants
                .iter()
                .find(|v| v.name == name)
                .ok_or_else(|| format!("no variant named {name}"))?,
        )),
    }
}

fn bom_columns(p: &Project, fields: &[String]) -> Result<Vec<(String, String)>, String> {
    if fields.is_empty() {
        Ok(p.bom_fields
            .iter()
            .map(|f| {
                (
                    f.key.clone(),
                    if f.label.is_empty() {
                        f.key.clone()
                    } else {
                        f.label.clone()
                    },
                )
            })
            .collect())
    } else {
        Ok(fields
            .iter()
            .map(|key| (key.clone(), key.clone()))
            .collect())
    }
}

fn component_footprint(c: &Component, variant: Option<&Variant>) -> String {
    match variant.and_then(|v| v.footprints.iter().find(|o| o.component == c.id)) {
        Some(swap) => swap.footprint.clone(),
        None => match &c.library_footprint {
            Some(footprint) => format!("{}:{}", footprint.library, footprint.definition.name),
            None => "starter pads".into(),
        },
    }
}

fn render_generic_csv(table: &BomTable, opts: &BomOptions, ranges: bool) -> Result<String, String> {
    let mut out = String::from("quantity,references,value,kind,footprint");
    for header in &table.columns {
        out.push(',');
        out.push_str(&csv_cell(header));
    }
    if !opts.legacy_generic_columns {
        out.push_str(",Manufacturer,MPN");
    }
    if opts.dnp == DnpMode::Column {
        out.push_str(",DNP");
    }
    out.push('\n');
    for row in &table.rows {
        if row.refs.len() == 1 && row.refs[0] == "DNP" {
            out.push_str("DNP\n");
            continue;
        }
        if row.refs.is_empty() && row.quantity == 0 {
            out.push('\n');
            continue;
        }
        let refs = format_references(&row.refs, ranges, opts.legacy_reference_order);
        out.push_str(&row.quantity.to_string());
        for cell in [
            refs.as_str(),
            row.value.as_str(),
            row.kind.as_str(),
            row.footprint.as_str(),
        ] {
            out.push(',');
            out.push_str(&csv_cell(cell));
        }
        for value in &row.fields {
            out.push(',');
            out.push_str(&csv_cell(value));
        }
        if !opts.legacy_generic_columns {
            for cell in [row.manufacturer.as_str(), row.mpn.as_str()] {
                out.push(',');
                out.push_str(&csv_cell(cell));
            }
        }
        if opts.dnp == DnpMode::Column {
            out.push(',');
            out.push_str(if row.dnp { "yes" } else { "" });
        }
        out.push('\n');
    }
    Ok(out)
}

fn generic_headers(table: &BomTable) -> Vec<String> {
    let mut headers = vec![
        "quantity".into(),
        "references".into(),
        "value".into(),
        "kind".into(),
        "footprint".into(),
    ];
    headers.extend(table.columns.clone());
    headers.push("Manufacturer".into());
    headers.push("MPN".into());
    headers
}

fn render_jlc_csv(table: &BomTable, ranges: bool) -> Result<String, String> {
    let mut out = String::from("Comment,Designator,Footprint,JLCPCB Part #\n");
    for row in &table.rows {
        if row.dnp || row.refs.is_empty() {
            continue;
        }
        let refs = format_references(&row.refs, ranges, false);
        out.push_str(&csv_cell(&row.value));
        out.push(',');
        out.push_str(&csv_cell(&refs));
        out.push(',');
        out.push_str(&csv_cell(&row.footprint));
        out.push(',');
        out.push_str(&csv_cell(&row.jlcpcb));
        out.push('\n');
    }
    Ok(out)
}

fn render_pcbway_csv(table: &BomTable, ranges: bool) -> Result<String, String> {
    let header = "Line#,Quantity Per Part Number,Reference Designator,Part Number,Part Description,Package,Type,Manufacturer,Manufacturer Part Number,Supplier,Supplier Part Number";
    let mut out = format!("{header}\n");
    let mut line = 0usize;
    for row in &table.rows {
        if row.dnp || row.refs.is_empty() {
            continue;
        }
        line += 1;
        let refs = format_references(&row.refs, ranges, false);
        let supplier = row
            .distributor
            .first()
            .map(|(n, _)| n.as_str())
            .unwrap_or("");
        let supplier_pn = row
            .distributor
            .first()
            .map(|(_, v)| v.as_str())
            .unwrap_or("");
        let cells = [
            line.to_string(),
            row.quantity.to_string(),
            refs,
            row.mpn.clone(),
            row.value.clone(),
            row.footprint.clone(),
            row.kind.clone(),
            row.manufacturer.clone(),
            row.mpn.clone(),
            supplier.to_string(),
            supplier_pn.to_string(),
        ];
        for (i, cell) in cells.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str(&csv_cell(cell));
        }
        out.push('\n');
    }
    Ok(out)
}

fn render_seeed_csv(table: &BomTable) -> Result<String, String> {
    let mut out = String::new();
    for row in &table.rows {
        if row.dnp || row.refs.is_empty() {
            continue;
        }
        let refs = format_references(&row.refs, false, false);
        let sku = if row.mpn.is_empty() {
            row.value.clone()
        } else {
            row.mpn.clone()
        };
        let link = if row.link.is_empty() {
            row.distributor
                .first()
                .map(|(_, v)| v.clone())
                .unwrap_or_default()
        } else {
            row.link.clone()
        };
        out.push_str(&csv_cell(&refs));
        out.push(',');
        out.push_str(&csv_cell(&sku));
        out.push(',');
        out.push_str(&row.quantity.to_string());
        out.push(',');
        out.push_str(&csv_cell(&link));
        out.push('\n');
    }
    Ok(out)
}

pub fn csv_cell(text: &str) -> String {
    if text.contains([',', '"', '\n']) {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text.to_string()
    }
}

pub fn xml_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(ch),
        }
    }
    out
}

fn parse_designator(s: &str) -> Option<(String, u32)> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let digit_len = s.chars().rev().take_while(|c| c.is_ascii_digit()).count();
    if digit_len == 0 || digit_len == s.len() {
        return None;
    }
    let split = s.len() - digit_len;
    let (prefix, num) = s.split_at(split);
    let n: u32 = num.parse().ok()?;
    Some((prefix.to_string(), n))
}

fn reference_cmp(a: &str, b: &str, legacy: bool) -> std::cmp::Ordering {
    if legacy {
        a.cmp(b)
    } else {
        match (parse_designator(a), parse_designator(b)) {
            (Some((pa, na)), Some((pb, nb))) if pa == pb => na.cmp(&nb),
            _ => a.cmp(b),
        }
    }
}

pub fn format_references(refs: &[String], ranges: bool, legacy_order: bool) -> String {
    let mut sorted = refs.to_vec();
    sorted.sort_by(|a, b| reference_cmp(a, b, legacy_order));
    if !ranges || sorted.len() < 2 {
        return sorted.join(",");
    }
    let mut out = Vec::new();
    let mut i = 0;
    while i < sorted.len() {
        let start = i;
        let mut end = i;
        if let Some((prefix, num)) = parse_designator(&sorted[i]) {
            let mut j = i + 1;
            let mut expect = num + 1;
            while j < sorted.len() {
                if let Some((p, n)) = parse_designator(&sorted[j])
                    && p == prefix
                    && n == expect
                {
                    end = j;
                    expect += 1;
                    j += 1;
                    continue;
                }
                break;
            }
            if end > start + 1 {
                out.push(format!("{}-{}", sorted[start], sorted[end]));
                i = end + 1;
                continue;
            }
        }
        out.push(sorted[start].clone());
        i = start + 1;
    }
    out.join(",")
}

pub fn parse_bom_grouping(name: &str) -> Result<BomGrouping, String> {
    match name {
        "value_footprint" | "valueFootprint" => Ok(BomGrouping::ValueFootprint),
        "value_footprint_mpn" | "valueFootprintMpn" | "" => Ok(BomGrouping::ValueFootprintMpn),
        "mpn" => Ok(BomGrouping::Mpn),
        _ => Err(format!("unknown bill grouping {name}")),
    }
}

pub fn parse_dnp_mode(name: &str) -> Result<DnpMode, String> {
    match name {
        "exclude" | "" => Ok(DnpMode::Exclude),
        "section" => Ok(DnpMode::Section),
        "column" => Ok(DnpMode::Column),
        _ => Err(format!("unknown do-not-populate mode {name}")),
    }
}

pub fn parse_bom_house(name: &str) -> Result<BomHouse, String> {
    match name {
        "generic" | "" => Ok(BomHouse::Generic),
        "jlc" | "jlcpcb" => Ok(BomHouse::JlcPcb),
        "pcbway" => Ok(BomHouse::PcbWay),
        "seeed" => Ok(BomHouse::Seeed),
        _ => Err(format!("unknown bill house {name}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pcbcraft_doc::{Component, ComponentField, Pin, Project};
    use pcbcraft_geom::Point;

    fn resistor(id: u64, reference: &str, value: &str, mpn: &str) -> Component {
        Component {
            sheet: 0,
            id,
            reference: reference.into(),
            value: value.into(),
            kind: "resistor".into(),
            position: Point::new(10.0, 10.0),
            rotation: 0.0,
            pins: [(-1.0, "1"), (1.0, "2")]
                .map(|(x, number)| Pin {
                    number: number.into(),
                    name: String::new(),
                    offset: Point::new(x, 0.0),
                    net: None,
                    electrical_type: "passive".into(),
                    direction: None,
                    unit: 1,
                })
                .to_vec(),
            units: vec![],
            library_symbol: None,
            library_footprint: None,
            fields: if mpn.is_empty() {
                vec![]
            } else {
                vec![ComponentField {
                    key: "MPN".into(),
                    value: mpn.into(),
                }]
            },
        }
    }

    #[test]
    fn bom_rows_groups_same_mpn_into_one_line_with_quantity_and_ranges() -> Result<(), String> {
        let mut project = Project::default();
        for (id, reference) in [(1, "R1"), (2, "R2"), (3, "R3"), (4, "R4")] {
            project
                .components
                .push(resistor(id, reference, "10k", "RC0603"));
        }
        project.validate()?;
        let opts = BomOptions {
            grouping: BomGrouping::ValueFootprintMpn,
            ranges: true,
            ..Default::default()
        };
        let table = bom_rows(&project, &opts)?;
        assert_eq!(table.rows.len(), 1);
        assert_eq!(table.rows[0].quantity, 4);
        assert_eq!(format_references(&table.rows[0].refs, true, false), "R1-R4");
        Ok(())
    }

    #[test]
    fn bom_rows_splits_lines_when_mpn_differs_under_mpn_grouping() -> Result<(), String> {
        let mut project = Project::default();
        project.components.push(resistor(1, "R1", "10k", "A"));
        project.components.push(resistor(2, "R2", "10k", "B"));
        project.validate()?;
        let opts = BomOptions {
            grouping: BomGrouping::Mpn,
            ..Default::default()
        };
        let table = bom_rows(&project, &opts)?;
        assert_eq!(table.rows.len(), 2);
        Ok(())
    }

    #[test]
    fn bom_dnp_excluded_or_listed_in_a_section_by_setting() -> Result<(), String> {
        let mut project = Project::default();
        project.components.push(resistor(1, "R1", "10k", ""));
        project.components.push(resistor(2, "R2", "10k", ""));
        project.variants.push(pcbcraft_doc::Variant {
            name: "DNP".into(),
            note: None,
            dnp: vec![2],
            footprints: vec![],
        });
        project.validate()?;
        let exclude = BomOptions {
            variant: Some("DNP".into()),
            dnp: DnpMode::Exclude,
            ..Default::default()
        };
        assert_eq!(bom_rows(&project, &exclude)?.rows.len(), 1);
        let section = BomOptions {
            variant: Some("DNP".into()),
            dnp: DnpMode::Section,
            ..Default::default()
        };
        let table = bom_rows(&project, &section)?;
        assert!(
            table.rows.iter().any(|r| r.refs == vec!["DNP".to_string()]),
            "section mode labels the do-not-populate block"
        );
        Ok(())
    }

    #[test]
    fn bom_house_presets_write_the_documented_headers() -> Result<(), String> {
        let mut project = Project::default();
        project.components.push(resistor(1, "R1", "10k", "P1"));
        project.validate()?;
        let jlc = bom_csv_with_options(
            &project,
            &BomOptions {
                house: BomHouse::JlcPcb,
                ..Default::default()
            },
        )?;
        assert_eq!(
            jlc.lines().next(),
            Some("Comment,Designator,Footprint,JLCPCB Part #")
        );
        let pcbway = bom_csv_with_options(
            &project,
            &BomOptions {
                house: BomHouse::PcbWay,
                ..Default::default()
            },
        )?;
        assert!(pcbway.starts_with("Line#,Quantity Per Part Number,"));
        Ok(())
    }

    #[test]
    fn seeed_rows_never_collapse_references() -> Result<(), String> {
        let mut project = Project::default();
        for (id, reference) in [
            (1, "C1"),
            (2, "C2"),
            (3, "C3"),
            (4, "C4"),
            (5, "C5"),
            (6, "C6"),
            (7, "C7"),
        ] {
            project.components.push(resistor(id, reference, "10k", "P"));
        }
        project.validate()?;
        let text = bom_csv_with_options(
            &project,
            &BomOptions {
                house: BomHouse::Seeed,
                ranges: true,
                ..Default::default()
            },
        )?;
        assert!(
            text.contains("C1,C2,C3,C4,C5,C6,C7") || text.contains("C1, C2"),
            "seeed keeps comma-separated designators: {text}"
        );
        assert!(!text.contains("C1-C7"));
        Ok(())
    }

    #[test]
    fn natural_reference_order_puts_r2_before_r10() {
        let refs = vec!["R10".into(), "R2".into()];
        assert_eq!(format_references(&refs, false, false), "R2,R10");
    }

    #[test]
    fn bom_xlsx_is_a_zip_whose_sheet_holds_the_same_rows_as_the_csv() -> Result<(), String> {
        let mut project = Project::default();
        project.components.push(resistor(1, "R1", "10k", "P1"));
        project.validate()?;
        let opts = BomOptions::default();
        let csv = bom_csv_with_options(&project, &opts)?;
        let xlsx = bom_xlsx(&project, &opts)?;
        let sheet = String::from_utf8(
            crate::zip_entry(&xlsx, "xl/worksheets/sheet1.xml").map_err(|e| e.to_string())?,
        )
        .map_err(|_| "sheet is not utf-8")?;
        for line in csv.lines().skip(1) {
            let value = line.split(',').nth(2).unwrap_or("");
            assert!(
                sheet.contains(&xml_escape(value)),
                "sheet should carry value {value}"
            );
        }
        Ok(())
    }

    #[test]
    fn bom_xlsx_twice_is_byte_identical() -> Result<(), String> {
        let mut project = Project::default();
        project.components.push(resistor(1, "R1", "10k", "P"));
        project.validate()?;
        let opts = BomOptions::default();
        let a = bom_xlsx(&project, &opts)?;
        let b = bom_xlsx(&project, &opts)?;
        assert_eq!(a, b);
        Ok(())
    }
}
