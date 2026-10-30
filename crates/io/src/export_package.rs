//! Export packages: name templates, the writer table and the package builder.
//!
//! Context: issue #224 (child of #214). A package is a `PackageDef` record
//! (`crates/doc/src/package_def.rs`); this module turns one into bytes. It expands the
//! name templates, runs each content through the existing writers, and hands the files to
//! the deterministic ZIP writer in `package.rs` (manifest first, fixed stamps, 256 file cap).
//!
//! Local rules:
//! - [`CONTENT_FORMATS`] is the only list of writer ids. A format whose writer does not
//!   exist yet is listed as unavailable with its reason and the issue that tracks it; a
//!   package that uses one is unavailable and never builds a partial file set.
//! - An option a format does not know is refused, never ignored.
//! - Nothing here reads the clock. `{date}` and `{rev}` come from the project's own
//!   metadata, so two exports of one design are byte-identical.
//!
//! # Dependencies
//! - crates/doc/src/package_def.rs (`PackageDef`, `check_template`)
//! - crates/io/src/package.rs (`package_zip`, `package_manifest`, `PackageEntry`)
//! - crates/io/src/lib.rs (the writers), crates/io/src/gerber_job.rs (`gerber_job_at`)
//! - crates/io/data/packages.json (the bundled records)
//!
//! # Do not
//! - Add a writer id without a row in [`CONTENT_FORMATS`] and a branch in `render_content`.
//! - Read the wall clock or the file system while building.
use crate::package::{PackageEntry, package_manifest, package_zip};
use pcbcraft_doc::{PackageContent, PackageDef, Project, check_template};
use std::collections::BTreeMap;
use std::sync::OnceLock;

/// Whether a writer exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// The writer runs.
    Ready,
    /// The writer is a stub or missing: why, and the tracking issue on the fork.
    Unavailable(&'static str, u32),
}

/// One writer format a package content may name.
#[derive(Clone, Copy, Debug)]
pub struct ContentFormat {
    /// Stable id used in package records.
    pub id: &'static str,
    /// Plain English label for the file list.
    pub label: &'static str,
    /// Options it accepts, as (key, allowed values); the first value is the default.
    pub options: &'static [(&'static str, &'static [&'static str])],
    /// Whether it can run.
    pub status: Status,
}

const NAMING: &[(&str, &[&str])] = &[("naming", &["ucamco", "protel"])];
const NONE: &[(&str, &[&str])] = &[];
const fn ready(id: &'static str, label: &'static str) -> ContentFormat {
    ContentFormat {
        id,
        label,
        options: NONE,
        status: Status::Ready,
    }
}
const fn waiting(
    id: &'static str,
    label: &'static str,
    reason: &'static str,
    issue: u32,
) -> ContentFormat {
    ContentFormat {
        id,
        label,
        options: NONE,
        status: Status::Unavailable(reason, issue),
    }
}

/// Every writer format a package may name.
pub static CONTENT_FORMATS: &[ContentFormat] = &[
    ContentFormat {
        id: "gerber_layers",
        label: "Gerber layers",
        options: NAMING,
        status: Status::Ready,
    },
    ContentFormat {
        id: "gerber_drill",
        label: "Drill files",
        options: NAMING,
        status: Status::Ready,
    },
    ready("gerber_job", "Gerber job file"),
    ready("drill_map", "Drill map"),
    ready("fab_readme", "Fabrication readme"),
    ContentFormat {
        id: "bom_csv",
        label: "Bill of materials (CSV)",
        options: &[("profile", &["generic"])],
        status: Status::Ready,
    },
    ContentFormat {
        id: "cpl_csv",
        label: "Component placement (CSV)",
        options: &[("profile", &["prototype"])],
        status: Status::Ready,
    },
    ready("ipc2581", "IPC-2581 XML"),
    ready("odb", "ODB++ archive"),
    ready("ipc356", "IPC-D-356 test netlist"),
    ready("step", "STEP board"),
    ready("idf_board", "IDF board"),
    ready("dxf_board", "DXF drawing"),
    ready("pdf_schematic", "Schematic PDF"),
    ready("pdf_board", "Board PDF"),
    ready("stackup", "Stackup report"),
    ready("drill_chart", "Drill chart"),
    waiting(
        "fab_drawing",
        "Fab drawing",
        "the fab drawing with drill chart, stackup, impedance and notes is not written yet",
        404,
    ),
    waiting(
        "bom_contract",
        "Contract assembler BOM",
        "vendor BOM column profiles are not written yet",
        226,
    ),
    waiting(
        "cpl_contract",
        "Contract assembler placement",
        "placement file flavours (per side, inches, lower-left origin) are not written yet",
        228,
    ),
    waiting(
        "assembly_drawing_top",
        "Assembly drawing, top",
        "per-side assembly drawings are not written yet",
        406,
    ),
    waiting(
        "assembly_drawing_bottom",
        "Assembly drawing, bottom",
        "per-side assembly drawings are not written yet",
        406,
    ),
    waiting(
        "idf_library",
        "IDF library",
        "the IDF library file is not written yet",
        411,
    ),
    waiting(
        "paste_gerbers",
        "Paste layers",
        "paste stencil output is a stub",
        420,
    ),
    waiting(
        "stencil_cut",
        "Stencil cut file",
        "paste stencil output is a stub",
        420,
    ),
    waiting(
        "revision_notes",
        "Revision notes",
        "the revision notes generator does not exist yet",
        425,
    ),
];

/// The writer row for `id`.
pub fn content_format(id: &str) -> Option<&'static ContentFormat> {
    CONTENT_FORMATS.iter().find(|f| f.id == id)
}

/// Whether a package can run, and if not why.
///
/// # Rules
/// - `reasons` has one line per distinct waiting writer; `issues` the fork issue numbers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Availability {
    /// True when every content can run.
    pub available: bool,
    /// Plain-language reasons, empty when available.
    pub reasons: Vec<String>,
    /// Tracking issues the package waits on.
    pub issues: Vec<u32>,
}

/// Checks a record against the writer table: known formats, known options. A format that is
/// not written yet makes the package unavailable (not an error); an unknown format or option
/// is an error.
pub fn availability(def: &PackageDef) -> Result<Availability, String> {
    def.validate()?;
    let mut reasons = Vec::new();
    let mut issues = Vec::new();
    for c in &def.files {
        let f = content_format(&c.format)
            .ok_or_else(|| format!("{}: unknown format {}", def.id, c.format))?;
        for (k, v) in &c.options {
            let allowed = f
                .options
                .iter()
                .find(|(name, _)| name == k)
                .ok_or_else(|| format!("{}: {} has no option {k}", def.id, f.id))?;
            if !allowed.1.contains(&v.as_str()) {
                return Err(format!("{}: {k} cannot be {v} for {}", def.id, f.id));
            }
        }
        if let Status::Unavailable(why, issue) = f.status {
            let line = format!("{}: {why} (#{issue})", f.label);
            if !reasons.contains(&line) {
                reasons.push(line);
            }
            if !issues.contains(&issue) {
                issues.push(issue);
            }
        }
    }
    Ok(Availability {
        available: reasons.is_empty(),
        reasons,
        issues,
    })
}

/// The packages that ship with PCBCraft, from `data/packages.json`.
///
/// # Rules
/// - Parsed once; a record that fails validation is a build-time test failure, and at run
///   time the list is empty rather than partly loaded.
pub fn bundled_packages() -> &'static [PackageDef] {
    static LIST: OnceLock<Vec<PackageDef>> = OnceLock::new();
    LIST.get_or_init(|| {
        serde_json::from_str::<Vec<PackageDef>>(include_str!("../data/packages.json"))
            .ok()
            .filter(|l| l.iter().all(|d| availability(d).is_ok()))
            .unwrap_or_default()
    })
}

/// Resolves a package by id: the project's own first, then the bundled ones.
pub fn find_package(p: &Project, id: &str) -> Option<PackageDef> {
    p.packages
        .iter()
        .find(|d| d.id == id)
        .cloned()
        .or_else(|| bundled_packages().iter().find(|d| d.id == id).cloned())
}

/// Values the name tokens take for one export.
#[derive(Clone, Debug)]
pub struct Tokens {
    project: String,
    rev: String,
    date: String,
    variant: String,
}

fn clean(text: &str, fallback: &str) -> String {
    let s: String = text
        .trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let s = s.trim_matches(['.', '_']).to_string();
    if s.is_empty() {
        fallback.into()
    } else {
        s.chars().take(48).collect()
    }
}

impl Tokens {
    /// Token values from the project: its name, its `revision` and `date` metadata and the
    /// chosen variant. Missing metadata reads as `A`, `undated` and `base`.
    pub fn of(p: &Project, variant: Option<&str>) -> Self {
        let meta = |k: &str, d: &str| clean(p.meta.get(k).map(String::as_str).unwrap_or(""), d);
        Self {
            project: clean(&p.name, "project"),
            rev: meta("revision", "A"),
            date: meta("date", "undated"),
            variant: clean(variant.unwrap_or(""), "base"),
        }
    }
}

/// Expands a name template. `side`, `layer` and `ext` are `None` when the content has no
/// such value; a template that asks for one then fails loudly.
pub fn expand(
    template: &str,
    t: &Tokens,
    side: Option<&str>,
    layer: Option<&str>,
    ext: Option<&str>,
) -> Result<String, String> {
    check_template(template)?;
    let mut out = String::new();
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let close = after.find('}').ok_or("a brace is not closed")?;
        let value = match &after[..close] {
            "project" => Some(t.project.as_str()),
            "rev" => Some(t.rev.as_str()),
            "date" => Some(t.date.as_str()),
            "variant" => Some(t.variant.as_str()),
            "side" => side,
            "layer" => layer,
            "ext" => ext,
            _ => None,
        };
        let token = &after[..close];
        out.push_str(value.ok_or_else(|| format!("{template}: {{{token}}} has no value here"))?);
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    check_template(&out)?;
    Ok(out)
}

/// Protel-style extension for a Gerber layer name such as `F.Cu`.
fn protel_ext(layer: &str) -> &'static str {
    match layer {
        "F.Cu" => "GTL",
        "B.Cu" => "GBL",
        "F.Mask" => "GTS",
        "B.Mask" => "GBS",
        "F.SilkS" => "GTO",
        "B.SilkS" => "GBO",
        "F.Paste" => "GTP",
        "B.Paste" => "GBP",
        "Edge" => "GKO",
        _ => "gbr",
    }
}

fn side_of(layer: &str) -> &'static str {
    if layer.starts_with("F.") {
        "top"
    } else if layer.starts_with("B.") {
        "bottom"
    } else {
        "both"
    }
}

/// Surface-mount parts only, as `Designator,Mid X,Mid Y,Rotation,Layer` in millimetres from
/// the design origin; parts a variant leaves off are dropped.
fn cpl_prototype(p: &Project, variant: Option<&str>) -> Result<String, String> {
    p.validate()?;
    let dnp: Vec<u64> = match variant {
        None => vec![],
        Some(name) => p
            .variants
            .iter()
            .find(|v| v.name == name)
            .ok_or_else(|| format!("no variant named {name}"))?
            .dnp
            .clone(),
    };
    let mut rows: Vec<(String, String)> = Vec::new();
    for f in &p.board.footprints {
        let c = p
            .components
            .iter()
            .find(|c| c.id == f.component)
            .ok_or("footprint references an unknown component")?;
        let tht = f.pads.iter().any(|pad| {
            pad.geometry
                .as_ref()
                .is_some_and(|g| g.pad_type == "thru_hole")
        });
        if tht || dnp.contains(&c.id) {
            continue;
        }
        let back = f.board_side() == pcbcraft_doc::Layer::Back;
        rows.push((
            c.reference.clone(),
            format!(
                "{},{:.3},{:.3},{:.1},{}",
                c.reference,
                f.position.x,
                f.position.y,
                f.rotation.rem_euclid(360.0),
                if back { "Bottom" } else { "Top" }
            ),
        ));
    }
    rows.sort();
    let body: Vec<String> = rows.into_iter().map(|(_, r)| r).collect();
    Ok(format!(
        "Designator,Mid X,Mid Y,Rotation,Layer\n{}\n",
        body.join("\n")
    ))
}

/// One content rendered: the files it wrote, as (name, kind, bytes).
type Rendered = Vec<PackageEntry>;

fn text_entry(name: String, kind: &str, text: String) -> PackageEntry {
    PackageEntry {
        name,
        kind: kind.into(),
        bytes: text.into_bytes(),
    }
}

fn render_content(
    p: &Project,
    c: &PackageContent,
    t: &Tokens,
    variant: Option<&str>,
    gerbers: &BTreeMap<String, String>,
) -> Result<Rendered, String> {
    let name = |side: Option<&str>, layer: Option<&str>, ext: Option<&str>| {
        expand(&c.name, t, side, layer, ext)
    };
    let protel = c.options.get("naming").is_some_and(|v| v == "protel");
    let one = |ext: &str, text: String| -> Result<Rendered, String> {
        Ok(vec![text_entry(
            name(None, None, Some(ext))?,
            &c.kind,
            text,
        )])
    };
    match c.format.as_str() {
        "gerber_layers" => {
            let mut out = Vec::new();
            for (file, text) in gerbers {
                let Some(layer) = file
                    .strip_prefix("pcb-")
                    .and_then(|f| f.strip_suffix(".gbr"))
                    .filter(|l| *l != "DrillMap")
                else {
                    continue;
                };
                let ext = if protel { protel_ext(layer) } else { "gbr" };
                out.push(text_entry(
                    name(Some(side_of(layer)), Some(layer), Some(ext))?,
                    &c.kind,
                    text.clone(),
                ));
            }
            Ok(out)
        }
        "gerber_drill" => {
            let mut out = Vec::new();
            for (file, text) in gerbers {
                let Some(layer) = file
                    .strip_prefix("pcb-")
                    .and_then(|f| f.strip_suffix(".drl"))
                    .map(|l| match l {
                        "Plated" => "PTH",
                        other => other,
                    })
                    .filter(|l| *l != "Drill")
                else {
                    continue;
                };
                let ext = if protel { "TXT" } else { "drl" };
                out.push(text_entry(
                    name(None, Some(layer), Some(ext))?,
                    &c.kind,
                    text.clone(),
                ));
            }
            Ok(out)
        }
        "drill_map" => {
            let text = gerbers
                .get("pcb-DrillMap.gbr")
                .ok_or("the design has no drill map")?;
            let n = name(None, Some("DrillMap"), Some("gbr"))?;
            Ok(vec![text_entry(n, &c.kind, text.clone())])
        }
        "bom_csv" => one("csv", crate::bom_csv(p, &[], variant)?),
        "cpl_csv" => one("csv", cpl_prototype(p, variant)?),
        "ipc2581" => one("xml", crate::ipc2581_text(p)?),
        "odb" => Ok(vec![PackageEntry {
            name: name(None, None, Some("odb.zip"))?,
            kind: c.kind.clone(),
            bytes: crate::odb_package(p)?,
        }]),
        "ipc356" => one("txt", crate::ipc356_netlist(p)?),
        "step" => one("step", crate::step_board(p)?),
        "idf_board" => one("emn", crate::idf_board(p)?),
        "dxf_board" => one("dxf", crate::dxf_board(p)?),
        "pdf_schematic" => Ok(vec![PackageEntry {
            name: name(None, Some("schematic"), Some("pdf"))?,
            kind: c.kind.clone(),
            bytes: crate::pdf_document(p, "schematic")?,
        }]),
        "pdf_board" => Ok(vec![PackageEntry {
            name: name(None, Some("board"), Some("pdf"))?,
            kind: c.kind.clone(),
            bytes: crate::pdf_document(p, "pcb")?,
        }]),
        "stackup" => one("txt", crate::stackup_text(p)?),
        "drill_chart" => one("csv", crate::drill_chart_csv(p)?),
        other => Err(format!("{other} has no writer here")),
    }
}

/// What a package export produces: the files to put on disk and the files inside them.
///
/// # Rules
/// - `files` are (relative name, bytes) in write order; for a zip package it is the one
///   archive, for a folder the entries and `manifest.json`, for a file package the one file.
/// - `entries` names what the package holds, sorted, whatever the container.
#[derive(Clone, Debug)]
pub struct PackageOutput {
    /// Package id.
    pub id: String,
    /// What to write.
    pub files: Vec<(String, Vec<u8>)>,
    /// File names inside the package.
    pub entries: Vec<String>,
}

/// Builds a package from a record.
///
/// Why: this is the single place a package becomes bytes, so the export command, the tests
/// and the dialog preview agree.
///
/// # Rules
/// - Refuses an unavailable package, listing every reason, and builds nothing.
/// - Refuses duplicate or out-of-range file names and more than 256 files.
/// - Same design, same record, same variant: same bytes.
///
/// # Dependencies
/// - crates/io/src/package.rs for the archive and manifest.
///
/// # Do not
/// - Write to disk; the engine command does that.
pub fn build_package(
    p: &Project,
    def: &PackageDef,
    variant: Option<&str>,
) -> Result<PackageOutput, String> {
    let a = availability(def)?;
    if !a.available {
        return Err(format!(
            "{} is not available: {}",
            def.name,
            a.reasons.join("; ")
        ));
    }
    p.validate()?;
    if let Some(v) = variant
        && !p.variants.iter().any(|x| x.name == v)
    {
        return Err(format!("no variant named {v}"));
    }
    let t = Tokens::of(p, variant);
    let needs_gerbers = def.files.iter().any(|c| {
        matches!(
            c.format.as_str(),
            "gerber_layers" | "gerber_drill" | "drill_map" | "gerber_job" | "fab_readme"
        )
    });
    let gerbers = if needs_gerbers {
        crate::gerber_files(p)?
    } else {
        BTreeMap::new()
    };
    let mut entries: Vec<PackageEntry> = Vec::new();
    for c in def
        .files
        .iter()
        .filter(|c| !matches!(c.format.as_str(), "gerber_job" | "fab_readme"))
    {
        entries.extend(render_content(p, c, &t, variant, &gerbers)?);
    }
    // The job file and the readme describe the files, so they are written last, from the
    // names the package really uses.
    for c in def.files.iter().filter(|c| c.format == "gerber_job") {
        let set: BTreeMap<String, String> = entries
            .iter()
            .filter(|e| matches!(e.kind.as_str(), "gerbers"))
            .map(|e| {
                (
                    e.name.clone(),
                    String::from_utf8_lossy(&e.bytes).into_owned(),
                )
            })
            .collect();
        let text =
            crate::gerber_job::gerber_job_at(p, &set, crate::package::PACKAGE_CREATION_DATE)?;
        entries.push(text_entry(
            expand(&c.name, &t, None, Some("job"), Some("gbrjob"))?,
            &c.kind,
            text,
        ));
    }
    for c in def.files.iter().filter(|c| c.format == "fab_readme") {
        let n = expand(&c.name, &t, None, Some("README"), Some("txt"))?;
        let mut names: Vec<String> = entries.iter().map(|e| e.name.clone()).collect();
        names.push(n.clone());
        names.sort();
        entries.push(text_entry(n, &c.kind, crate::fab_readme_text(p, &names)?));
    }
    if entries.is_empty() {
        return Err(format!("{} wrote no files", def.name));
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    let names: Vec<String> = entries.iter().map(|e| e.name.clone()).collect();
    let files = match def.container.as_str() {
        "zip" => {
            let archive = expand(&def.archive, &t, None, None, None)?;
            vec![(archive, package_zip(p, &def.id, variant, &entries)?)]
        }
        "folder" => {
            let manifest = package_manifest(p, &def.id, variant, &entries)?;
            let mut files = vec![(
                crate::package::MANIFEST_NAME.to_string(),
                manifest.into_bytes(),
            )];
            files.extend(entries.into_iter().map(|e| (e.name, e.bytes)));
            files
        }
        _ => entries
            .into_iter()
            .take(1)
            .map(|e| (e.name, e.bytes))
            .collect(),
    };
    Ok(PackageOutput {
        id: def.id.clone(),
        files,
        entries: names,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_expand_and_a_missing_value_fails_loudly() -> Result<(), String> {
        let p = Project {
            name: "My board!".into(),
            ..Project::default()
        };
        let t = Tokens::of(&p, None);
        assert_eq!(
            expand("{project}-fab-{rev}.zip", &t, None, None, None)?,
            "My_board-fab-A.zip"
        );
        assert_eq!(
            expand(
                "{project}-{layer}.{ext}",
                &t,
                None,
                Some("F.Cu"),
                Some("gbr")
            )?,
            "My_board-F.Cu.gbr"
        );
        assert!(expand("{project}.{ext}", &t, None, None, None).is_err());
        assert!(expand("{nope}", &t, None, None, None).is_err());
        assert!(expand("../{project}", &t, None, None, None).is_err());
        Ok(())
    }

    #[test]
    fn every_bundled_package_names_only_known_formats() -> Result<(), String> {
        let list = bundled_packages();
        assert_eq!(list.len(), 10, "the starting set has ten packages");
        let mut ids = std::collections::BTreeSet::new();
        for d in list {
            assert!(ids.insert(d.id.clone()), "duplicate id {}", d.id);
            availability(d)?;
            for c in &d.files {
                assert!(
                    content_format(&c.format).is_some(),
                    "{}: {}",
                    d.id,
                    c.format
                );
            }
        }
        Ok(())
    }

    #[test]
    fn duplicate_file_names_after_expansion_are_refused() -> Result<(), String> {
        use pcbcraft_doc::{PackageContent, PackageDef};
        use std::collections::BTreeMap;
        let p = Project::default();
        let def = PackageDef {
            id: "dup".into(),
            name: "Dup".into(),
            audience: "test".into(),
            container: "folder".into(),
            archive: String::new(),
            experimental: false,
            files: vec![
                PackageContent {
                    kind: "bom".into(),
                    format: "bom_csv".into(),
                    options: BTreeMap::new(),
                    name: "parts.csv".into(),
                },
                PackageContent {
                    kind: "report".into(),
                    format: "stackup".into(),
                    options: BTreeMap::new(),
                    name: "parts.csv".into(),
                },
            ],
        };
        def.validate()?;
        let err = build_package(&p, &def, None).unwrap_err();
        assert!(
            err.contains("parts.csv"),
            "the refusal names the duplicate: {err}"
        );
        Ok(())
    }

    #[test]
    fn waiting_writers_make_a_package_unavailable_with_the_issue() -> Result<(), String> {
        let by = |id: &str| bundled_packages().iter().find(|d| d.id == id).cloned();
        let q = availability(&by("fab_quickturn").ok_or("missing")?)?;
        assert!(!q.available && q.issues == vec![404]);
        let s = availability(&by("stencil").ok_or("missing")?)?;
        assert!(!s.available && s.issues == vec![420]);
        for id in [
            "fab_prototype",
            "fab_x2",
            "assembly_prototype",
            "ipc2581",
            "odb",
        ] {
            assert!(availability(&by(id).ok_or("missing")?)?.available, "{id}");
        }
        Ok(())
    }
}
