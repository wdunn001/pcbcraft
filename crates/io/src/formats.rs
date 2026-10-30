//! The format registry: one table naming every file format PCBCraft reads or writes.
//!
//! Context: owner decision 2026-10-29 (issue #194). The File menu has no per-format
//! items; Open, Import, Save as and Export as build their format lists, desktop file
//! filters and the browser `accept` list from [`FORMATS`], so a new format is added
//! here and nowhere else. A row only *describes* a format and names the command that
//! runs it; the readers and writers stay where they are.
//! Local rules:
//! - Every format has exactly one [`Scope`]. Save as lists the `Project` scope that
//!   can be written; Export as lists the `Output` scope; the two never overlap.
//! - The first row matching an extension wins in [`for_path`].
//! - A [`Run`] names a real command or tool and every option with a `param` names a
//!   documented parameter of it; the tests in `crates/mcp/src/lib.rs` pin both.
//!
//! # Dependencies
//! - crates/doc/src/lib.rs (`OUTPUT_KINDS`, checked by the mcp tests)
//! - crates/engine/src/lib.rs (`bom_export`, `commands`)
//!
//! # Do not
//! - Add a File menu item for one format; add a row here instead.
//! - Put a path, host name or user name in a row.
use serde::Deserialize;

/// What a format is of: the whole design, something made from it, or pieces brought in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    /// The whole design. Save and Save as write it; Open reads it.
    Project,
    /// Something made from the design: a bill of materials, fabrication files, a drawing.
    Output,
    /// Pieces brought into the open design (a drawing, a library part, a machine profile).
    Parts,
}

/// Which way a format can go.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    /// Read as the design itself (File > Open).
    Open,
    /// Read into the open design (File > Import).
    Import,
    /// Written (File > Save as for the project scope, File > Export as for outputs).
    Export,
}

/// The heading a format sits under in its dialog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    /// Project files and their archives.
    Project,
    /// Files a board maker needs.
    Fabrication,
    /// Files an assembler needs.
    Assembly,
    /// Drawings, reports and pictures.
    Documentation,
    /// Files other tools open.
    Exchange,
    /// Drawings, parts and profiles brought in.
    Library,
}

impl Group {
    /// English heading; the UI shows it as is.
    pub fn label(self) -> &'static str {
        match self {
            Self::Project => "Project",
            Self::Fabrication => "Fabrication",
            Self::Assembly => "Assembly",
            Self::Documentation => "Documentation",
            Self::Exchange => "Exchange",
            Self::Library => "Parts and drawings",
        }
    }
}

/// What runs a format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Run {
    /// An engine command, named as `list_commands` shows it.
    Engine(&'static str),
    /// A tool of the MCP server that is not an engine command (`open`, `save`, `export`).
    Tool(&'static str),
    /// One of `pcbcraft_doc::OUTPUT_KINDS`, run through an output job.
    Kind(&'static str),
}

/// What an option's value looks like.
#[derive(Clone, Copy, Debug)]
pub enum OptionKind {
    /// A checkbox.
    Bool {
        /// Whether it starts on.
        default: bool,
    },
    /// (value as sent, English label).
    /// One of several values.
    Choice {
        /// The values and their English labels.
        choices: &'static [(&'static str, &'static str)],
        /// Index in `choices` that starts chosen.
        default: usize,
    },
    /// The assembly variants of the open design, or none.
    Variant,
    /// The columns of a bill of materials.
    Columns,
    /// The vendor flavours of a bill of materials ([`bom_flavours`]).
    Flavour,
}

/// One option of a format.
#[derive(Clone, Copy, Debug)]
pub struct OptionSpec {
    /// The documented parameter of the format's command it fills; `None` for an option
    /// the app applies itself (the file encoding of a bill of materials).
    pub param: Option<&'static str>,
    /// English label.
    pub label: &'static str,
    /// Tooltip: what it does and why.
    pub tip: &'static str,
    /// What its value looks like.
    pub kind: OptionKind,
}

/// One file format.
#[derive(Clone, Copy, Debug)]
pub struct FormatSpec {
    /// Stable id.
    pub id: &'static str,
    /// English label.
    pub label: &'static str,
    /// Tooltip: what the format is for, in plain words.
    pub tip: &'static str,
    /// Extensions, lower case, preferred first; matching ignores case.
    pub extensions: &'static [&'static str],
    /// Whole project, made from it, or brought into it.
    pub scope: Scope,
    /// The heading it sits under.
    pub group: Group,
    /// What reads it as a project (File > Open).
    pub open: Option<Run>,
    /// What reads it into the open design (File > Import).
    pub import: Option<Run>,
    /// What writes it (Save as for a project format, Export for an output).
    pub export: Option<Run>,
    /// Options offered when it is written or imported.
    pub options: &'static [OptionSpec],
    /// Shown directly under File > Export or File > Import, not only under More.
    pub common: bool,
}

impl FormatSpec {
    /// The directions this format can go.
    pub fn directions(&self) -> Vec<Direction> {
        let mut out = vec![];
        if self.open.is_some() {
            out.push(Direction::Open);
        }
        if self.import.is_some() {
            out.push(Direction::Import);
        }
        if self.export.is_some() {
            out.push(Direction::Export);
        }
        out
    }
    /// True when `path` ends in one of this format's extensions.
    pub fn matches(&self, path: &str) -> bool {
        let lower = path.to_ascii_lowercase();
        self.extensions
            .iter()
            .any(|e| lower.ends_with(&format!(".{e}")))
    }
}

/// The columns of a bill of materials file.
pub const BOM_FILE: OptionSpec = OptionSpec {
    param: None,
    label: "File type",
    tip: "CSV opens in any spreadsheet. TSV is the same with tabs. JSON is for programs.",
    kind: OptionKind::Choice {
        choices: &[("csv", "CSV"), ("tsv", "TSV"), ("json", "JSON")],
        default: 0,
    },
};

const BOM_OPTIONS: &[OptionSpec] = &[
    OptionSpec {
        param: None,
        label: "Flavour",
        tip: "Match the file a distributor or assembler asks for: its column names, grouping and separators.",
        kind: OptionKind::Flavour,
    },
    BOM_FILE,
    OptionSpec {
        param: Some("variant"),
        label: "Build",
        tip: "Print the list for one assembly variant: parts it leaves off are skipped.",
        kind: OptionKind::Variant,
    },
    OptionSpec {
        param: Some("fields"),
        label: "Columns",
        tip: "Choose which columns the list has.",
        kind: OptionKind::Columns,
    },
    OptionSpec {
        param: Some("group"),
        label: "Group identical parts",
        tip: "One row per kind of part with its references listed. Off gives one row per part.",
        kind: OptionKind::Bool { default: true },
    },
];

const USE_OUTLINE: &[OptionSpec] = &[OptionSpec {
    param: Some("use_outline"),
    label: "Use as board outline",
    tip: "Trace a closed shape in the file as the board outline.",
    kind: OptionKind::Bool { default: false },
}];

const VIEW: &[OptionSpec] = &[OptionSpec {
    param: Some("view"),
    label: "View",
    tip: "Which drawing to write: the board or the schematic.",
    kind: OptionKind::Choice {
        choices: &[("pcb", "Board"), ("schematic", "Schematic")],
        default: 0,
    },
}];

const fn output(
    id: &'static str,
    label: &'static str,
    tip: &'static str,
    extensions: &'static [&'static str],
    group: Group,
    run: Run,
) -> FormatSpec {
    FormatSpec {
        id,
        label,
        tip,
        extensions,
        scope: Scope::Output,
        group,
        open: None,
        import: None,
        export: Some(run),
        options: &[],
        common: false,
    }
}

/// Mark a row as one of the quick entries under Export and Import.
const fn common(mut f: FormatSpec) -> FormatSpec {
    f.common = true;
    f
}

const fn part(
    id: &'static str,
    label: &'static str,
    tip: &'static str,
    extensions: &'static [&'static str],
    command: &'static str,
    options: &'static [OptionSpec],
) -> FormatSpec {
    FormatSpec {
        id,
        label,
        tip,
        extensions,
        scope: Scope::Parts,
        group: Group::Library,
        open: None,
        import: Some(Run::Engine(command)),
        export: None,
        options,
        common: false,
    }
}

/// Every format. Project scope first, then outputs by group, then parts.
pub static FORMATS: &[FormatSpec] = &[
    FormatSpec {
        id: "pcbcraft",
        label: "PCBCraft project",
        tip: "The whole design in PCBCraft's own file. Saving and opening lose nothing.",
        extensions: &["pcbcraft"],
        scope: Scope::Project,
        group: Group::Project,
        open: Some(Run::Tool("open")),
        import: None,
        export: Some(Run::Tool("save")),
        options: &[],
        common: false,
    },
    FormatSpec {
        id: "archive",
        label: "Project archive",
        tip: "The whole design and its attached files in one ZIP, to hand to a colleague.",
        extensions: &["pcbcraft.zip"],
        scope: Scope::Project,
        group: Group::Project,
        open: Some(Run::Engine("project.import_archive")),
        import: None,
        export: Some(Run::Engine("project.export_archive")),
        options: &[],
        common: false,
    },
    FormatSpec {
        id: "bom",
        label: "Bill of materials",
        tip: "The shopping list for your board: every part, how many, and which part number to buy.",
        extensions: &["csv", "tsv", "json"],
        scope: Scope::Output,
        group: Group::Assembly,
        open: None,
        import: None,
        export: Some(Run::Engine("bom.export")),
        options: BOM_OPTIONS,
        common: true,
    },
    common(output(
        "placement",
        "Pick and place",
        "Where each part goes and which way it faces, for the assembly machine.",
        &["txt"],
        Group::Assembly,
        Run::Kind("placement"),
    )),
    output(
        "netlist",
        "Net list",
        "Which pins are joined together, as plain text.",
        &["net"],
        Group::Assembly,
        Run::Kind("netlist"),
    ),
    output(
        "ipc2581",
        "Assembly exchange (IPC-2581)",
        "One file with the outline, nets, parts, holes and copper for the assembler.",
        &["xml"],
        Group::Assembly,
        Run::Kind("ipc2581"),
    ),
    common(output(
        "gerbers",
        "Copper and drill files",
        "Gerber copper layers, the board outline and drill files that a board maker needs.",
        &["gbr", "drl"],
        Group::Fabrication,
        Run::Kind("gerbers"),
    )),
    output(
        "drill_chart",
        "Drill chart",
        "Every drill size with its plating and hole count.",
        &["csv"],
        Group::Fabrication,
        Run::Kind("drill_chart"),
    ),
    output(
        "odb",
        "Board package (ODB++)",
        "A ZIP of layers, pad shapes and parts for makers who ask for it.",
        &["zip"],
        Group::Fabrication,
        Run::Kind("odb"),
    ),
    output(
        "ipc356",
        "Test netlist (IPC-D-356)",
        "Net, part, pin and position for bare-board electrical test.",
        &["ipc"],
        Group::Fabrication,
        Run::Kind("ipc356"),
    ),
    common(output(
        "pdf",
        "Drawing (PDF)",
        "A printable one-page drawing of the board.",
        &["pdf"],
        Group::Documentation,
        Run::Kind("pdf"),
    )),
    output(
        "svg",
        "Drawing (SVG)",
        "A web drawing of the board that scales without blur.",
        &["svg"],
        Group::Documentation,
        Run::Kind("svg"),
    ),
    output(
        "report",
        "Design report",
        "A plain-text summary of the board, every part and every net.",
        &["rpt"],
        Group::Documentation,
        Run::Kind("report"),
    ),
    output(
        "stackup",
        "Stackup sheet",
        "Layer notes, substrate, finish and machining tolerances.",
        &["stk"],
        Group::Documentation,
        Run::Kind("stackup"),
    ),
    output(
        "stats",
        "Board statistics",
        "Counts of parts, nets, pins, routing, vias and zones as a CSV.",
        &["csv"],
        Group::Documentation,
        Run::Kind("stats"),
    ),
    FormatSpec {
        id: "picture",
        label: "Picture (PNG)",
        tip: "A picture of the board or the schematic.",
        extensions: &["png"],
        scope: Scope::Output,
        group: Group::Documentation,
        open: None,
        import: None,
        export: Some(Run::Tool("export")),
        options: VIEW,
        common: false,
    },
    common(output(
        "step",
        "3D model (STEP)",
        "The board as a solid model for mechanical design.",
        &["step", "stp"],
        Group::Exchange,
        Run::Kind("step"),
    )),
    output(
        "vrml",
        "3D viewer file (VRML)",
        "A 3D scene of the board for viewers.",
        &["wrl"],
        Group::Exchange,
        Run::Kind("vrml"),
    ),
    output(
        "dxf",
        "Board drawing (DXF)",
        "The board as a CAD drawing: copper, outline and labels.",
        &["dxf"],
        Group::Exchange,
        Run::Kind("dxf"),
    ),
    output(
        "idf",
        "Mechanical drawing (IDF)",
        "Board size and each part's position, for enclosure design.",
        &["idf"],
        Group::Exchange,
        Run::Kind("idf"),
    ),
    output(
        "dsn",
        "Autorouter file",
        "Nets, placements and keepouts for an outside autorouter.",
        &["dsn"],
        Group::Exchange,
        Run::Kind("dsn"),
    ),
    FormatSpec {
        id: "board_interchange",
        label: "Board interchange file",
        tip: "The board in a text file other design tools can open. Covers a supported subset.",
        extensions: &["kicad_pcb"],
        scope: Scope::Output,
        group: Group::Exchange,
        open: None,
        import: None,
        export: Some(Run::Tool("export")),
        options: &[],
        common: false,
    },
    common(part(
        "dxf_drawing",
        "DXF drawing",
        "Lines, arcs, circles and text brought in as board graphics.",
        &["dxf"],
        "dxf.import",
        USE_OUTLINE,
    )),
    common(part(
        "svg_drawing",
        "SVG drawing",
        "Shapes and text brought in as board graphics.",
        &["svg"],
        "svg.import",
        USE_OUTLINE,
    )),
    part(
        "sheet",
        "Schematic sheet",
        "A sheet file that arrives as a new schematic page.",
        &["kicad_sch"],
        "schematic.import_sheet",
        &[],
    ),
    common(part(
        "symbols",
        "Symbol library file",
        "Symbols added to the project library.",
        &["kicad_sym"],
        "library.import_symbol_file",
        &[],
    )),
    common(part(
        "footprints",
        "Footprint file",
        "A package added to the project library.",
        &["kicad_mod"],
        "library.import_footprint_file",
        &[],
    )),
    part(
        "machine",
        "Machine profile",
        "A shop machine's settings, added to the design.",
        &["toml"],
        "machine.import",
        &[],
    ),
];

/// One column of a bill of materials flavour.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BomColumn {
    /// What fills the column: a [`BOM_TOKENS`] entry.
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

/// One vendor flavour of the bill of materials, as pure data: which columns, what the
/// headers say, whether identical parts share a row, how references are joined, which
/// file types it takes and which part fields it needs.
///
/// Why: a distributor or assembler upload format is a record in
/// `crates/io/data/bom_flavours.json`, not code; adding one is adding a record.
///
/// # Rules
/// - `requires` and `requires_any` hold keys of [`PART_FIELDS`]; a test pins that.
/// - A flavour with `verified` false says so in its `tip`.
///
/// # Dependencies
/// - crates/io/data/bom_flavours.json
///
/// # Do not
/// - Name a company in the data file; describe the role instead.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BomFlavour {
    /// Stable id.
    pub id: String,
    /// English label shown under the BOM format.
    pub label: String,
    /// Tooltip: who asks for this file.
    pub tip: String,
    /// False when no primary page gave the column list.
    #[serde(default = "yes")]
    pub verified: bool,
    /// The file type a fresh export starts with.
    pub file_type: String,
    /// The file types this flavour can be written as: `csv`, `tsv`, `json`.
    pub file_types: Vec<String>,
    /// False writes no header row.
    #[serde(default = "yes")]
    pub header: bool,
    /// Columns in file order.
    pub columns: Vec<BomColumn>,
    /// True gives one row per kind of part.
    pub group: bool,
    /// Joins the references of a grouped row.
    pub reference_separator: String,
    /// Part fields every listed part must carry.
    #[serde(default)]
    pub requires: Vec<String>,
    /// Part fields of which every listed part must carry at least one.
    #[serde(default)]
    pub requires_any: Vec<String>,
    /// Size limits of the receiving site.
    #[serde(default)]
    pub limits: BomLimits,
}

/// One standard part field: the key a part carries it under and the column token that
/// prints it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PartField {
    /// The key in a part's fields.
    pub key: &'static str,
    /// The column token.
    pub token: &'static str,
    /// Older spelling of the key that is read when the key itself is empty.
    pub alias: Option<&'static str>,
    /// Plain words for messages.
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

/// Column tokens worked out from the design rather than typed in:
/// `reference`, `quantity`, `value`, `footprint`, `line` (row number), `mount`
/// (Surface mount, Thru-hole or Hybrid, from the pads) and `part_key` (the
/// manufacturer part number, else the supplier part number).
pub const DERIVED_TOKENS: &[&str] = &[
    "reference",
    "quantity",
    "value",
    "footprint",
    "line",
    "mount",
    "part_key",
];

/// The bundled flavour file.
const BOM_FLAVOURS_JSON: &str = include_str!("../data/bom_flavours.json");

/// Parse a flavour file: a JSON array of [`BomFlavour`].
pub fn parse_bom_flavours(text: &str) -> Result<Vec<BomFlavour>, String> {
    serde_json::from_str(text).map_err(|e| format!("bom_flavours.json: {e}"))
}

/// The plain list used if the bundled file ever fails to parse (a test pins that it
/// does not).
fn fallback_flavour() -> BomFlavour {
    let column = |token: &str, header: &str| BomColumn {
        token: token.into(),
        header: header.into(),
    };
    BomFlavour {
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

/// Every bill of materials flavour, in file order. The first is the default; the list is
/// never empty.
pub fn bom_flavours() -> &'static [BomFlavour] {
    static ALL: std::sync::OnceLock<Vec<BomFlavour>> = std::sync::OnceLock::new();
    ALL.get_or_init(|| match parse_bom_flavours(BOM_FLAVOURS_JSON) {
        Ok(list) if !list.is_empty() => list,
        _ => vec![fallback_flavour()],
    })
}

/// The default flavour: the first one.
pub fn default_bom_flavour() -> &'static BomFlavour {
    &bom_flavours()[0]
}

/// A bill of materials flavour by id.
pub fn bom_flavour(id: &str) -> Option<&'static BomFlavour> {
    bom_flavours().iter().find(|f| f.id == id)
}

/// The first format that can go `direction` and whose extension matches `path`.
pub fn for_path(path: &str, direction: Direction) -> Option<&'static FormatSpec> {
    FORMATS
        .iter()
        .find(|f| f.directions().contains(&direction) && f.matches(path))
}

/// The formats that can go `direction` in `scope`, in table order.
pub fn listed(direction: Direction, scope: Scope) -> Vec<&'static FormatSpec> {
    FORMATS
        .iter()
        .filter(|f| f.scope == scope && f.directions().contains(&direction))
        .collect()
}

/// Formats of one dialog grouped under their headings, in the order the groups first appear.
pub fn grouped(direction: Direction, scope: Scope) -> Vec<(Group, Vec<&'static FormatSpec>)> {
    let mut out: Vec<(Group, Vec<&'static FormatSpec>)> = vec![];
    for f in listed(direction, scope) {
        match out.iter_mut().find(|(g, _)| *g == f.group) {
            Some((_, list)) => list.push(f),
            None => out.push((f.group, vec![f])),
        }
    }
    out
}

/// Every extension a dialog for `direction` accepts, without the dot: what the browser
/// `accept` list and a desktop "all supported files" filter are made of.
pub fn extensions(direction: Direction) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = vec![];
    for f in FORMATS
        .iter()
        .filter(|f| f.directions().contains(&direction))
    {
        for e in f.extensions {
            if !out.contains(e) {
                out.push(e);
            }
        }
    }
    out
}

/// The browser `accept` value for `direction`: `.pcbcraft,.dxf,...`.
pub fn accept_list(direction: Direction) -> String {
    extensions(direction)
        .iter()
        .map(|e| format!(".{e}"))
        .collect::<Vec<_>>()
        .join(",")
}

/// Desktop file-dialog filters for `direction`: (label, extensions), one per format.
pub fn filters(direction: Direction) -> Vec<(&'static str, Vec<&'static str>)> {
    FORMATS
        .iter()
        .filter(|f| f.directions().contains(&direction))
        .map(|f| (f.label, f.extensions.to_vec()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_extensions_are_lower_case() {
        for (i, f) in FORMATS.iter().enumerate() {
            assert!(
                FORMATS[..i].iter().all(|o| o.id != f.id),
                "{} appears twice",
                f.id
            );
            assert!(!f.extensions.is_empty(), "{} has no extension", f.id);
            assert!(f.extensions.iter().all(|e| *e == e.to_ascii_lowercase()));
            assert!(!f.tip.is_empty() && !f.label.is_empty());
            assert!(!f.directions().is_empty(), "{} goes nowhere", f.id);
        }
    }

    #[test]
    fn save_as_and_export_as_never_list_the_same_format() {
        let saves = listed(Direction::Export, Scope::Project);
        let exports = listed(Direction::Export, Scope::Output);
        assert!(!saves.is_empty() && !exports.is_empty());
        for s in &saves {
            assert!(exports.iter().all(|e| e.id != s.id), "{} is in both", s.id);
        }
        assert_eq!(saves[0].id, "pcbcraft", "Save as defaults to the project");
    }

    #[test]
    fn every_scope_holds_only_what_it_should() {
        for f in FORMATS {
            match f.scope {
                Scope::Project => assert!(f.import.is_none(), "{} is a whole project", f.id),
                Scope::Output => assert!(
                    f.open.is_none() && f.import.is_none(),
                    "{} is made from the project, never read as one",
                    f.id
                ),
                Scope::Parts => assert!(
                    f.open.is_none() && f.export.is_none(),
                    "{} only comes in",
                    f.id
                ),
            }
        }
    }

    #[test]
    fn paths_pick_the_row_that_reads_them() {
        let open = |p: &str| for_path(p, Direction::Open).map(|f| f.id);
        assert_eq!(open("board.PCBCRAFT"), Some("pcbcraft"));
        assert_eq!(open("board.pcbcraft.zip"), Some("archive"));
        let import = |p: &str| for_path(p, Direction::Import).map(|f| f.id);
        assert_eq!(import("outline.dxf"), Some("dxf_drawing"));
        assert_eq!(import("logo.svg"), Some("svg_drawing"));
        assert_eq!(import("notes.pdf"), None);
    }

    #[test]
    fn the_accept_list_comes_from_the_table() {
        let accept = accept_list(Direction::Open);
        assert!(accept.contains(".pcbcraft") && accept.contains(".pcbcraft.zip"));
        assert!(!accept.contains(".csv"), "outputs are not opened");
        assert!(accept_list(Direction::Import).contains(".dxf"));
    }

    #[test]
    fn the_bill_of_materials_offers_columns_grouping_and_builds() {
        let bom = FORMATS.iter().find(|f| f.id == "bom").map(|f| f.options);
        let labels: Vec<&str> = bom.unwrap_or(&[]).iter().map(|o| o.label).collect();
        assert_eq!(
            labels,
            [
                "Flavour",
                "File type",
                "Build",
                "Columns",
                "Group identical parts"
            ]
        );
    }

    #[test]
    fn the_bundled_flavour_file_parses_and_leads_with_the_plain_list() -> Result<(), String> {
        let all = parse_bom_flavours(BOM_FLAVOURS_JSON)?;
        assert_eq!(all[0].id, "generic_csv", "the first is the default");
        assert!(all.len() >= 14, "every flavour with enough detail ships");
        assert_eq!(bom_flavours().len(), all.len(), "the table is the file");
        let mut ids: Vec<&str> = all.iter().map(|f| f.id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), all.len(), "ids are unique");
        Ok(())
    }

    #[test]
    fn every_required_field_of_every_flavour_exists_in_the_part_field_model() {
        let known = |key: &String| PART_FIELDS.iter().any(|f| f.key == key);
        for f in bom_flavours() {
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
        for f in bom_flavours() {
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
}
