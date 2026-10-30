//! Gerber naming presets and drill options: the fabrication bundle written
//! under the file names and drill style a chosen recipient expects.
//!
//! Context: issue #225, child of the export packages epic (#214). A preset is
//! data, a table of file-name patterns; it never changes what a file holds.
//! The bundle is built once by `gerber_files_at` under the project's own
//! `pcb-*` names, then this module adds the inner copper layers, rewrites the
//! drill files when the options ask for another drill style, and renames every
//! file. Because renaming is the last step, the X2 `FileFunction` attribute of
//! every file is exactly the one the plotter wrote.
//!
//! # Rules
//! - A preset names every file role once. A role maps to one file name, and
//!   `gerber_files_with` refuses a bundle in which two roles share a name.
//! - Every copper layer of the board gets exactly one file, top to bottom, with
//!   the layer number in its name.
//! - `drill_files` is `split` (plated and non-plated apart) or `merged` (one
//!   file for both). Blind and buried spans keep their own files in both.
//! - A drill file written with the default options is the one `gerber_files_at`
//!   wrote. Any other option rewrites it from the same hole list, so the holes
//!   are the same and only the encoding differs.
//! - Coordinates with a zero-suppression mode are fixed-point with the stated
//!   digits; `read_drill_file` reads every mode back.
//!
//! # Dependencies
//! - crates/io/src/lib.rs `gerber_files_at`, `project_drill_hits`
//! - crates/io/src/gerber.rs `gerber_copper`
//! - crates/io/src/gerber_job.rs `gerber_job_at`
//! - crates/io/src/fab_export.rs `fab_readme_text`
//! - crates/doc/src/lib.rs `Project::copper_layer_count`
//!
//! # Do not
//! - Edit the text of a Gerber file here; only its name changes.
//! - Name another vendor or tool in a label or tip; a preset is named for its
//!   file style.
use crate::{ProjectDrillHit, fab_export, gerber, gerber_job, project_drill_hits};
use pcbcraft_doc::{Layer, Project};
use pcbcraft_geom::Point;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// Most digits a drill coordinate may carry in all.
const MAX_DIGITS: u8 = 9;

/// How the files of a bundle are named.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum NamingPreset {
    /// The project's own names: `pcb-F.Cu.gbr`, `pcb-Edge.gbr`, `pcb-Plated.drl`.
    #[default]
    Project,
    /// Plain `.gbr` files with the X2 file function in the name, such as
    /// `board-Copper_L1_Top.gbr`; drills end in `.drl`.
    Plain,
    /// The classic three-letter extensions online prototype fabs read: `.GTL`,
    /// `.GBL`, `.GTS`, `.GKO`, inner copper `.G2L`, and `.XLN` drill files.
    Protel,
}

/// Every preset, in the order a picker lists them.
pub const NAMING_PRESETS: [NamingPreset; 3] = [
    NamingPreset::Project,
    NamingPreset::Plain,
    NamingPreset::Protel,
];

impl NamingPreset {
    /// Stable id used in commands and saved packages.
    pub fn id(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Plain => "plain",
            Self::Protel => "protel",
        }
    }
    /// Short English label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Project => "Project names",
            Self::Plain => "Plain .gbr",
            Self::Protel => "Three-letter extensions",
        }
    }
    /// Tooltip: what the style looks like and who reads it.
    pub fn tip(self) -> &'static str {
        match self {
            Self::Project => {
                "Names that start with pcb-, such as pcb-F.Cu.gbr. Any fab that reads the layer from the file header accepts them."
            }
            Self::Plain => {
                "Every Gerber file ends in .gbr and the layer is in the name, such as board-Copper_L1_Top.gbr. Fabs that read the layer from the file header accept these."
            }
            Self::Protel => {
                "The classic extensions: .GTL top copper, .GBL bottom copper, .GTS and .GBS mask, .GKO outline, .G2L inner copper. Online prototype fabs recognise these by extension."
            }
        }
    }
    /// The preset a stored id names.
    pub fn from_id(id: &str) -> Option<Self> {
        NAMING_PRESETS.iter().copied().find(|p| p.id() == id)
    }
}

/// What a file in the bundle is for, independent of how it is named.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum FileRole {
    /// Copper layer `index` counted from the top, 0 first.
    Copper(u8),
    /// Solder mask, `true` for the bottom face.
    Mask(bool),
    /// Solder paste, `true` for the bottom face.
    Paste(bool),
    /// Silkscreen, `true` for the bottom face.
    Silk(bool),
    /// Board outline.
    Edge,
    /// Drill hole map.
    DrillMap,
    /// Plated through holes.
    DrillPlated,
    /// Non-plated holes.
    DrillNonPlated,
    /// Plated and non-plated holes in one file.
    DrillMerged,
    /// Blind or buried via holes between two copper layers (0-based, top first).
    DrillSpan(u8, u8),
    /// Routed slots.
    DrillRout,
    /// The Gerber job file.
    Job,
    /// The fabrication readme.
    Readme,
}

impl FileRole {
    /// The role of a file the project writes under its own name, or `None` for
    /// a drawn-layer plot and anything else outside the preset table.
    ///
    /// # Rules
    /// - Reads the name only.
    /// - `pcb-In<n>.Cu.gbr` is copper layer `n`, counted from the top; `pcb-B.Cu.gbr` is the last of `copper` layers.
    pub fn from_project_name(name: &str, copper: u8) -> Option<Self> {
        let fixed = match name {
            "pcb-F.Cu.gbr" => Some(Self::Copper(0)),
            "pcb-B.Cu.gbr" => Some(Self::Copper(copper.saturating_sub(1).max(1))),
            "pcb-F.Mask.gbr" => Some(Self::Mask(false)),
            "pcb-B.Mask.gbr" => Some(Self::Mask(true)),
            "pcb-F.Paste.gbr" => Some(Self::Paste(false)),
            "pcb-B.Paste.gbr" => Some(Self::Paste(true)),
            "pcb-F.SilkS.gbr" => Some(Self::Silk(false)),
            "pcb-B.SilkS.gbr" => Some(Self::Silk(true)),
            "pcb-Edge.gbr" => Some(Self::Edge),
            "pcb-DrillMap.gbr" => Some(Self::DrillMap),
            "pcb-Plated.drl" => Some(Self::DrillPlated),
            "pcb-NPTH.drl" => Some(Self::DrillNonPlated),
            "pcb-Drill.drl" => Some(Self::DrillMerged),
            "pcb-Rout.drl" => Some(Self::DrillRout),
            "pcb-job.gbrjob" => Some(Self::Job),
            "README-fabrication.txt" => Some(Self::Readme),
            _ => None,
        };
        if fixed.is_some() {
            return fixed;
        }
        if let Some(n) = name
            .strip_prefix("pcb-In")
            .and_then(|s| s.strip_suffix(".Cu.gbr"))
            .and_then(|s| s.parse::<u8>().ok())
        {
            return Some(Self::Copper(n));
        }
        let span = name.strip_prefix("pcb-Drill-L")?.strip_suffix(".drl")?;
        let (a, b) = span.split_once("-L")?;
        Some(Self::DrillSpan(
            a.parse::<u8>().ok()?.checked_sub(1)?,
            b.parse::<u8>().ok()?.checked_sub(1)?,
        ))
    }
}

/// Units of the drill files.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DrillUnits {
    /// Whatever the project uses.
    #[default]
    Project,
    /// Millimetres.
    Mm,
    /// Inches.
    Inch,
}

/// How coordinates are written in a drill file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ZeroSuppression {
    /// Explicit decimal points, so no zeros are suppressed.
    #[default]
    Decimal,
    /// Fixed-point digits, leading zeros omitted.
    Leading,
    /// Fixed-point digits, trailing zeros omitted.
    Trailing,
}

impl ZeroSuppression {
    /// Stable id used in commands.
    pub fn id(self) -> &'static str {
        match self {
            Self::Decimal => "decimal",
            Self::Leading => "leading",
            Self::Trailing => "trailing",
        }
    }
    /// The mode a stored id names.
    pub fn from_id(id: &str) -> Option<Self> {
        [Self::Decimal, Self::Leading, Self::Trailing]
            .into_iter()
            .find(|z| z.id() == id)
    }
}

/// Everything that decides how the bundle is named and how its drill files
/// are written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GerberOptions {
    /// File-name style.
    pub naming: NamingPreset,
    /// One drill file for plated and non-plated holes, instead of two.
    pub merge_drills: bool,
    /// Drill units.
    pub units: DrillUnits,
    /// Coordinate encoding of the drill files.
    pub zeros: ZeroSuppression,
    /// Digits before and after the point for fixed-point coordinates; `None`
    /// picks 3 and 3 for millimetres and 2 and 4 for inches.
    pub format: Option<(u8, u8)>,
    /// Write the drill hole map layer.
    pub drill_map: bool,
    /// Write the Gerber job file.
    pub job_file: bool,
    /// Write the fabrication readme.
    pub readme: bool,
    /// Date the job file records; a fixed value keeps the bytes repeatable.
    pub creation: String,
}

impl Default for GerberOptions {
    fn default() -> Self {
        Self {
            naming: NamingPreset::Project,
            merge_drills: false,
            units: DrillUnits::Project,
            zeros: ZeroSuppression::Decimal,
            format: None,
            drill_map: true,
            job_file: true,
            readme: true,
            creation: crate::PACKAGE_CREATION_DATE.to_owned(),
        }
    }
}

impl GerberOptions {
    /// The preset's own recommended drill style: what the recipient the preset
    /// is named for reads without a question. Naming stays as given.
    ///
    /// # Rules
    /// - The three-letter style wants one plated and one non-plated file in
    ///   millimetres with decimal points.
    pub fn for_preset(naming: NamingPreset) -> Self {
        Self {
            naming,
            ..Self::default()
        }
    }
    /// Refuse an option set the writers cannot honour.
    ///
    /// # Rules
    /// - Digits are 1 to 6 before the point and 1 to 6 after, nine in all.
    pub fn validate(&self) -> Result<(), String> {
        if let Some((int, frac)) = self.format
            && (!(1..=6).contains(&int) || !(1..=6).contains(&frac) || int + frac > MAX_DIGITS)
        {
            return Err(
                "drill format needs 1 to 6 digits on each side of the point, nine at most".into(),
            );
        }
        Ok(())
    }
    /// True when the drill files can stay as the plotter wrote them.
    fn drill_is_default(&self, p: &Project) -> bool {
        self.zeros == ZeroSuppression::Decimal
            && self.format.is_none()
            && self.drill_inch(p) == (p.unit == "inch")
    }
    fn drill_inch(&self, p: &Project) -> bool {
        match self.units {
            DrillUnits::Project => p.unit == "inch",
            DrillUnits::Mm => false,
            DrillUnits::Inch => true,
        }
    }
}

/// The stem of every file name: the project name with every character other
/// than letters, digits, `-` and `_` turned into `_`; `board` when nothing is left.
pub fn file_stem(p: &Project) -> String {
    let stem: String = p
        .name
        .trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .take(48)
        .collect();
    let stem = stem.trim_matches('_').to_owned();
    if stem.is_empty() {
        "board".into()
    } else {
        stem
    }
}

/// The name a role gets under a preset.
///
/// # Rules
/// - `copper` is the number of copper layers; the bottom layer is the last.
/// - Under the three-letter style the bottom layer is `.GBL` and the layers
///   between are `.G2L`, `.G3L` and on, numbered from the top layer as 1.
///
/// # Dependencies
/// - `NamingPreset`, `FileRole` in this file.
///
/// # Do not
/// - Return a path; the name has no directory.
pub fn file_name(preset: NamingPreset, stem: &str, role: FileRole, copper: u8) -> String {
    let last = copper.saturating_sub(1);
    match preset {
        NamingPreset::Project => match role {
            FileRole::Copper(0) => "pcb-F.Cu.gbr".into(),
            FileRole::Copper(n) if n >= last => "pcb-B.Cu.gbr".into(),
            FileRole::Copper(n) => format!("pcb-In{n}.Cu.gbr"),
            FileRole::Mask(b) => format!("pcb-{}.Mask.gbr", if b { "B" } else { "F" }),
            FileRole::Paste(b) => format!("pcb-{}.Paste.gbr", if b { "B" } else { "F" }),
            FileRole::Silk(b) => format!("pcb-{}.SilkS.gbr", if b { "B" } else { "F" }),
            FileRole::Edge => "pcb-Edge.gbr".into(),
            FileRole::DrillMap => "pcb-DrillMap.gbr".into(),
            FileRole::DrillPlated => "pcb-Plated.drl".into(),
            FileRole::DrillNonPlated => "pcb-NPTH.drl".into(),
            FileRole::DrillMerged => "pcb-Drill.drl".into(),
            FileRole::DrillSpan(a, b) => format!("pcb-Drill-L{}-L{}.drl", a + 1, b + 1),
            FileRole::DrillRout => "pcb-Rout.drl".into(),
            FileRole::Job => "pcb-job.gbrjob".into(),
            FileRole::Readme => "README-fabrication.txt".into(),
        },
        NamingPreset::Plain => {
            let face = |b: bool| if b { "Bot" } else { "Top" };
            match role {
                FileRole::Copper(n) => {
                    let side = if n == 0 {
                        "Top"
                    } else if n >= last {
                        "Bot"
                    } else {
                        "Inr"
                    };
                    format!("{stem}-Copper_L{}_{side}.gbr", n + 1)
                }
                FileRole::Mask(b) => format!("{stem}-Soldermask_{}.gbr", face(b)),
                FileRole::Paste(b) => format!("{stem}-Paste_{}.gbr", face(b)),
                FileRole::Silk(b) => format!("{stem}-Legend_{}.gbr", face(b)),
                FileRole::Edge => format!("{stem}-Profile.gbr"),
                FileRole::DrillMap => format!("{stem}-Drillmap.gbr"),
                FileRole::DrillPlated => format!("{stem}-PTH.drl"),
                FileRole::DrillNonPlated => format!("{stem}-NPTH.drl"),
                FileRole::DrillMerged => format!("{stem}-Drill.drl"),
                FileRole::DrillSpan(a, b) => format!("{stem}-Via_L{}_L{}.drl", a + 1, b + 1),
                FileRole::DrillRout => format!("{stem}-Rout.drl"),
                FileRole::Job => format!("{stem}.gbrjob"),
                FileRole::Readme => "README-fabrication.txt".into(),
            }
        }
        NamingPreset::Protel => {
            let side =
                |b: bool, top: &'static str, bottom: &'static str| if b { bottom } else { top };
            match role {
                FileRole::Copper(0) => format!("{stem}.GTL"),
                FileRole::Copper(n) if n >= last => format!("{stem}.GBL"),
                FileRole::Copper(n) => format!("{stem}.G{}L", n + 1),
                FileRole::Mask(b) => format!("{stem}.{}", side(b, "GTS", "GBS")),
                FileRole::Paste(b) => format!("{stem}.{}", side(b, "GTP", "GBP")),
                FileRole::Silk(b) => format!("{stem}.{}", side(b, "GTO", "GBO")),
                FileRole::Edge => format!("{stem}.GKO"),
                FileRole::DrillMap => format!("{stem}.GD1"),
                FileRole::DrillPlated | FileRole::DrillMerged => format!("{stem}.XLN"),
                FileRole::DrillNonPlated => format!("{stem}-NPTH.XLN"),
                FileRole::DrillSpan(a, b) => format!("{stem}-L{}-L{}.XLN", a + 1, b + 1),
                FileRole::DrillRout => format!("{stem}-Rout.XLN"),
                FileRole::Job => format!("{stem}.gbrjob"),
                FileRole::Readme => "README-fabrication.txt".into(),
            }
        }
    }
}

/// The names a bundle of `copper` layers uses under a preset, one per role
/// that always exists. Used by the export preview to show the file list
/// before anything is written.
///
/// # Rules
/// - Copper layers come first, top to bottom, then the other layers.
/// - `merged` lists one drill file instead of a plated and a non-plated one.
///
/// # Dependencies
/// - `file_name` in this file.
///
/// # Do not
/// - Open a project; the list depends only on the preset and the layer count.
pub fn preview_names(preset: NamingPreset, stem: &str, copper: u8, merged: bool) -> Vec<String> {
    let mut roles: Vec<FileRole> = (0..copper.max(2)).map(FileRole::Copper).collect();
    for bottom in [false, true] {
        roles.push(FileRole::Mask(bottom));
        roles.push(FileRole::Paste(bottom));
        roles.push(FileRole::Silk(bottom));
    }
    roles.extend([
        FileRole::Edge,
        FileRole::DrillMap,
        FileRole::Job,
        FileRole::Readme,
    ]);
    if merged {
        roles.push(FileRole::DrillMerged);
    } else {
        roles.extend([FileRole::DrillPlated, FileRole::DrillNonPlated]);
    }
    roles
        .into_iter()
        .map(|r| file_name(preset, stem, r, copper.max(2)))
        .collect()
}

/// The fabrication bundle under a naming preset and drill options.
///
/// Why: the same recipe goes to fabs that read different file names and drill
/// styles. The files are those of `gerber_files_at`, plus one Gerber file for
/// each inner copper layer, so every copper layer of a 4 or 6 layer board is
/// in the bundle.
///
/// # Rules
/// - Runs `validate` on the options first.
/// - Drops the job file, the readme or the drill map when the options say so.
/// - The job file and the readme list the new names.
/// - Refuses a bundle in which two files would share a name.
///
/// # Dependencies
/// - `crate::gerber_files_at`, `gerber::gerber_copper`, `gerber_job::gerber_job_at`.
///
/// # Do not
/// - Change the content of a Gerber file; only names and drill encoding change.
pub fn gerber_files_with(
    p: &Project,
    options: &GerberOptions,
) -> Result<BTreeMap<String, String>, String> {
    options.validate()?;
    p.validate()?;
    let copper = p.copper_layer_count();
    let mut files = crate::gerber_files_at(p, &options.creation)?;
    files.remove("pcb-job.gbrjob");
    files.remove("README-fabrication.txt");
    if !options.drill_map {
        files.remove("pcb-DrillMap.gbr");
    }
    for index in 1..copper.saturating_sub(1) {
        let layer = Layer::from_index(index)?;
        // The plotter calls layer 2 the bottom face; between the outer layers
        // every copper layer is inner.
        let text = gerber::gerber_copper(p, layer)?.replace(
            &format!("Copper,L{},Bot", index + 1),
            &format!("Copper,L{},Inr", index + 1),
        );
        files.insert(format!("pcb-In{index}.Cu.gbr"), text);
    }
    if copper > 2
        && let Some(text) = files.get_mut("pcb-B.Cu.gbr")
    {
        // The bottom layer is the last of the stack; older plots stamped L2.
        *text = text.replace("Copper,L2,Bot", &format!("Copper,L{copper},Bot"));
    }
    apply_drill_options(p, options, &mut files)?;
    if options.job_file {
        let job = gerber_job::gerber_job_at(p, &files, &options.creation)?;
        files.insert("pcb-job.gbrjob".into(), job);
    }
    rename_all(p, options, files)
}

/// Write the plated, non-plated and merged drill files the options ask for,
/// under the project's own names.
fn apply_drill_options(
    p: &Project,
    options: &GerberOptions,
    files: &mut BTreeMap<String, String>,
) -> Result<(), String> {
    let hits = project_drill_hits(p)?;
    let plated: Vec<ProjectDrillHit> = hits
        .iter()
        .copied()
        .filter(|h| h.via_span.is_none() && h.plated)
        .collect();
    let unplated: Vec<ProjectDrillHit> = hits
        .iter()
        .copied()
        .filter(|h| h.via_span.is_none() && !h.plated)
        .collect();
    let default = options.drill_is_default(p);
    let origin = p.board.drill_origin;
    let layers = p.copper_layer_count();
    let write = |hits: &[ProjectDrillHit], function: &str| -> Result<String, String> {
        write_drill_file(hits, origin, options.drill_inch(p), options, function)
    };
    let legacy_plated = files.remove("pcb-Plated.drl");
    let legacy_npth = files.remove("pcb-NPTH.drl");
    let legacy_merged = files.remove("pcb-Drill.drl");
    let all: Vec<ProjectDrillHit> = plated.iter().chain(&unplated).copied().collect();
    let mut put = |name: &str,
                   hits: &[ProjectDrillHit],
                   legacy: Option<String>,
                   function: String|
     -> Result<(), String> {
        if hits.is_empty() {
            return Ok(());
        }
        let text = match legacy {
            Some(text) if default => text,
            _ => write(hits, &function)?,
        };
        files.insert(name.into(), text);
        Ok(())
    };
    if options.merge_drills {
        put(
            "pcb-Drill.drl",
            &all,
            legacy_merged,
            format!("MixedPlating,1,{layers}"),
        )?;
    } else {
        put(
            "pcb-Plated.drl",
            &plated,
            legacy_plated,
            format!("Plated,1,{layers},PTH"),
        )?;
        put(
            "pcb-NPTH.drl",
            &unplated,
            legacy_npth,
            format!("NonPlated,1,{layers},NPTH"),
        )?;
    }
    Ok(())
}

/// Rename every file to the preset's name, rewrite the job file's paths and
/// write the readme with the final list.
fn rename_all(
    p: &Project,
    options: &GerberOptions,
    files: BTreeMap<String, String>,
) -> Result<BTreeMap<String, String>, String> {
    let stem = file_stem(p);
    let copper = p.copper_layer_count();
    let mut out: BTreeMap<String, String> = BTreeMap::new();
    let mut renames: BTreeMap<String, String> = BTreeMap::new();
    for (old, text) in files {
        let new = match FileRole::from_project_name(&old, copper) {
            Some(role) => file_name(options.naming, &stem, role, copper),
            None => old.clone(),
        };
        if out.insert(new.clone(), text).is_some() {
            return Err(format!(
                "the {} naming gives two files the name {new}",
                options.naming.id()
            ));
        }
        renames.insert(old, new);
    }
    if let Some(job) = out.get_mut(&file_name(options.naming, &stem, FileRole::Job, copper)) {
        *job = rename_job_paths(job, &renames)?;
    }
    if options.readme {
        let mut names: Vec<String> = out.keys().cloned().collect();
        names.push("README-fabrication.txt".into());
        names.sort();
        out.insert(
            "README-fabrication.txt".into(),
            fab_export::fab_readme_text(p, &names)?,
        );
    }
    Ok(out)
}

/// The job file with each `Path` set to the file's new name.
fn rename_job_paths(job: &str, renames: &BTreeMap<String, String>) -> Result<String, String> {
    let mut value: Value = serde_json::from_str(job).map_err(|e| e.to_string())?;
    if let Some(list) = value
        .get_mut("FilesAttributes")
        .and_then(Value::as_array_mut)
    {
        for entry in list {
            let new = entry
                .get("Path")
                .and_then(Value::as_str)
                .and_then(|old| renames.get(old))
                .cloned();
            if let (Some(new), Some(map)) = (new, entry.as_object_mut()) {
                map.insert("Path".into(), Value::String(new));
            }
        }
    }
    serde_json::to_string_pretty(&value).map_err(|e| e.to_string())
}

/// One Excellon file for the given holes.
///
/// # Rules
/// - One tool per distinct (diameter, slot length), numbered in size order.
/// - `function` is written as an X2 file function comment.
/// - A coordinate that does not fit the digits is an error, not a clipped value.
///
/// # Dependencies
/// - `ProjectDrillHit` in crates/io/src/lib.rs.
///
/// # Do not
/// - Add attributes the plotter did not write for the default style.
fn write_drill_file(
    hits: &[ProjectDrillHit],
    origin: Point,
    inch: bool,
    options: &GerberOptions,
    function: &str,
) -> Result<String, String> {
    const MAX_TOOLS: usize = 64;
    let scale = if inch { 1.0 / 25.4 } else { 1.0 };
    let (int, frac) = options.format.unwrap_or(if inch { (2, 4) } else { (3, 3) });
    let mut tools: BTreeMap<(u64, u64), Vec<&ProjectDrillHit>> = BTreeMap::new();
    for hit in hits {
        tools.entry((hit.d, hit.span)).or_default().push(hit);
    }
    if tools.len() > MAX_TOOLS {
        return Err("a drill file needs 64 or fewer distinct tools".into());
    }
    let tool_decimals = if inch { 4 } else { 3 };
    let mut out = String::from("M48\n");
    out.push_str(&format!("; #@! TF.FileFunction,{function}\n"));
    let (zero_word, notes) = match options.zeros {
        ZeroSuppression::Decimal => ("TZ", "decimal".to_owned()),
        ZeroSuppression::Leading => ("TZ", format!("{int}:{frac} leading zeros omitted")),
        ZeroSuppression::Trailing => ("LZ", format!("{int}:{frac} trailing zeros omitted")),
    };
    out.push_str(&format!("; FORMAT={{{notes}}}\nFMAT,2\n"));
    out.push_str(&format!(
        "{},{zero_word}\n",
        if inch { "INCH" } else { "METRIC" }
    ));
    for (n, (d, _)) in tools.keys().enumerate() {
        out.push_str(&format!(
            "T{}C{:.tool_decimals$}\n",
            n + 1,
            *d as f64 / 1000.0 * scale
        ));
    }
    out.push_str("%\nG90\nG05\n");
    let coord =
        |v: f64| -> Result<String, String> { encode_coordinate(v, options.zeros, int, frac, inch) };
    for (n, group) in tools.values().enumerate() {
        out.push_str(&format!("T{}\n", n + 1));
        for hit in group {
            let (x, y) = ((hit.x - origin.x) * scale, (hit.y - origin.y) * scale);
            if hit.span == 0 {
                out.push_str(&format!("X{}Y{}\n", coord(x)?, coord(y)?));
            } else {
                let (ux, uy) = (hit.ux * scale, hit.uy * scale);
                out.push_str(&format!(
                    "X{}Y{}G85X{}Y{}\n",
                    coord(x - ux)?,
                    coord(y - uy)?,
                    coord(x + ux)?,
                    coord(y + uy)?
                ));
            }
        }
    }
    out.push_str("M30\n");
    Ok(out)
}

/// One coordinate in the chosen encoding.
fn encode_coordinate(
    value: f64,
    zeros: ZeroSuppression,
    int: u8,
    frac: u8,
    inch: bool,
) -> Result<String, String> {
    if zeros == ZeroSuppression::Decimal {
        let decimals = if inch { 4 } else { 3 };
        return Ok(format!("{value:.decimals$}"));
    }
    let scaled = (value.abs() * 10f64.powi(i32::from(frac))).round();
    if !scaled.is_finite() || scaled >= 10f64.powi(i32::from(int) + i32::from(frac)) {
        return Err(format!(
            "a drill coordinate of {value:.3} does not fit {int} digits before the point"
        ));
    }
    let width = usize::from(int) + usize::from(frac);
    let digits = format!("{:0width$}", scaled as u64);
    let body = match zeros {
        ZeroSuppression::Leading => digits.trim_start_matches('0'),
        _ => digits.trim_end_matches('0'),
    };
    let body = if body.is_empty() { "0" } else { body };
    let sign = if value < 0.0 && scaled > 0.0 { "-" } else { "" };
    Ok(format!("{sign}{body}"))
}

/// One hole read back from a drill file, in millimetres.
#[derive(Clone, Debug, PartialEq)]
pub struct DrillHole {
    /// Drill diameter in millimetres.
    pub diameter_mm: f64,
    /// Hole centre, or the first end of a slot.
    pub x_mm: f64,
    /// Hole centre, or the first end of a slot.
    pub y_mm: f64,
    /// Other end of a slot.
    pub slot_end_mm: Option<(f64, f64)>,
}

/// Read an Excellon drill file back: its tools and every hole.
///
/// Why: a written drill file must read back to the holes it was made from,
/// in every unit and zero-suppression mode this module writes.
///
/// # Rules
/// - Reads the unit, the zero mode and the digit counts from the header; a
///   coordinate with a decimal point is taken as written.
/// - Bounded: at most 50 000 holes.
///
/// # Dependencies
/// - The header lines `write_drill_file` writes (`; FORMAT={..}`, `METRIC,TZ`).
///
/// # Do not
/// - Use it for files from other sources without checking the digit counts.
pub fn read_drill_file(text: &str) -> Result<Vec<DrillHole>, String> {
    const MAX_HOLES: usize = 50_000;
    let mut inch = false;
    let mut leading_omitted = true;
    let mut digits = (3u8, 3u8);
    let mut tools: BTreeMap<u32, f64> = BTreeMap::new();
    let mut holes: Vec<DrillHole> = Vec::new();
    let mut current: Option<u32> = None;
    let mut in_header = false;
    for raw in text.lines() {
        let line = raw.trim();
        if line == "M48" {
            in_header = true;
        } else if line == "%" {
            in_header = false;
        } else if let Some(rest) = line.strip_prefix("; FORMAT={") {
            let first = rest.split_whitespace().next().unwrap_or("");
            if let Some((i, f)) = first.split_once(':')
                && let (Ok(i), Ok(f)) = (i.parse::<u8>(), f.parse::<u8>())
            {
                digits = (i, f);
            }
        } else if in_header {
            if line.starts_with("INCH") || line.starts_with("METRIC") {
                inch = line.starts_with("INCH");
                leading_omitted = !line.ends_with(",LZ");
            } else if let Some(rest) = line.strip_prefix('T')
                && let Some((n, d)) = rest.split_once('C')
            {
                let n: u32 = n.parse().map_err(|_| "tool number is not a number")?;
                let d: f64 = d.parse().map_err(|_| "tool diameter is not a number")?;
                tools.insert(n, d * if inch { 25.4 } else { 1.0 });
            }
        } else if let Some(n) = line.strip_prefix('T').and_then(|s| s.parse::<u32>().ok()) {
            current = Some(n);
        } else if line.starts_with('X') {
            if holes.len() >= MAX_HOLES {
                return Err("drill file exceeds the hole limit".into());
            }
            let tool = current.ok_or("a hole comes before any tool")?;
            let diameter_mm = *tools.get(&tool).ok_or("a hole uses an undefined tool")?;
            let unit = if inch { 25.4 } else { 1.0 };
            let read = |s: &str| decode_coordinate(s, leading_omitted, digits).map(|v| v * unit);
            let (first, slot) = match line.split_once("G85") {
                Some((a, b)) => (a, Some(b)),
                None => (line, None),
            };
            let (x, y) = xy_pair(first, &read)?;
            let slot_end_mm = match slot {
                Some(s) => Some(xy_pair(s, &read)?),
                None => None,
            };
            holes.push(DrillHole {
                diameter_mm,
                x_mm: x,
                y_mm: y,
                slot_end_mm,
            });
        }
    }
    Ok(holes)
}

/// `X..Y..` split into its two numbers.
fn xy_pair(s: &str, read: &dyn Fn(&str) -> Result<f64, String>) -> Result<(f64, f64), String> {
    let s = s
        .trim()
        .strip_prefix('X')
        .ok_or("a coordinate line lacks X")?;
    let (x, y) = s.split_once('Y').ok_or("a coordinate line lacks Y")?;
    Ok((read(x)?, read(y)?))
}

/// One coordinate back to a number in the file's unit.
fn decode_coordinate(s: &str, leading_omitted: bool, digits: (u8, u8)) -> Result<f64, String> {
    let (sign, body) = match s.strip_prefix('-') {
        Some(rest) => (-1.0, rest),
        None => (1.0, s.strip_prefix('+').unwrap_or(s)),
    };
    if body.contains('.') {
        return body
            .parse::<f64>()
            .map(|v| v * sign)
            .map_err(|_| "a coordinate is not a number".to_string());
    }
    let width = usize::from(digits.0) + usize::from(digits.1);
    let padded = if leading_omitted {
        format!("{body:0>width$}")
    } else {
        format!("{body:0<width$}")
    };
    let n: u64 = padded
        .parse()
        .map_err(|_| "a coordinate is not a number".to_string())?;
    Ok(sign * n as f64 / 10f64.powi(i32::from(digits.1)))
}

/// Copper roles of a file set, in layer order, for tests and previews.
///
/// # Rules
/// - Counts a copper file once, by its role.
///
/// # Dependencies
/// - `FileRole::from_project_name` in this file.
///
/// # Do not
/// - Parse the contents; the role comes from the project name.
pub fn copper_roles(names: &BTreeSet<String>, copper: u8) -> Vec<u8> {
    let mut found: Vec<u8> = names
        .iter()
        .filter_map(|n| match FileRole::from_project_name(n, copper) {
            Some(FileRole::Copper(i)) => Some(i),
            _ => None,
        })
        .collect();
    found.sort_unstable();
    found
}
