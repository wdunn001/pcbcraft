//! Pick-and-place file flavours: the layout choices of a placement list kept as data.
//!
//! Context: assemblers each ask for their own placement list (header names, units,
//! origin, side words, one file or one per side, a stated rotation sense). A
//! [`PlacementFlavour`] records those choices and [`placement_flavoured`] writes the
//! file or files; the bundled flavours live in `crates/io/data/placement-flavours.toml`.
//!
//! # Rules
//! - Pure text generation from a `Project`; the engine or `generate` writes to disk.
//! - `placement_text` is untouched; a design with no flavour keeps its old output.
//! - A part is held back by a flavour when it is unpopulated in the chosen variant or
//!   carries a truthy `DNP` or `Exclude from BOM` field; the flavour's [`DnpRule`] then
//!   drops it, keeps it, or keeps it with a populate flag.
//! - Bottom-side angle handling is one explicit [`BottomRotation`] choice, applied
//!   after any rotation offset, never mixed into the coordinates.
//! - Text fields never contain the delimiter, a quote or a line break.
//!
//! # Dependencies
//! - crates/io/data/placement-flavours.toml
//! - crates/doc/src/lib.rs (`Project`, `Footprint`, `Variant`)
//!
//! # Do not
//! - Name a vendor in a flavour record or a test; roles only.
//! - Write the machine-specific YY1 files here; see crates/io/src/neoden_yy1.rs.

use pcbcraft_doc::{Component, Footprint, Layer, Project};
use pcbcraft_geom::Point;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const MAX_PLACED: usize = 5_000;
const MAX_COLUMNS: usize = 16;
const MAX_OFFSETS: usize = 512;
const BUNDLED: &str = include_str!("../data/placement-flavours.toml");
/// Component field keys that mark a part as not to be fitted, matched ignoring case.
const EXCLUDE_KEYS: [&str; 2] = ["DNP", "Exclude from BOM"];

/// Length unit of the coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlacementUnits {
    /// Millimetres.
    Mm,
    /// Thousandths of an inch.
    Mil,
    /// Inches.
    Inch,
}

/// Where coordinates are measured from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlacementOrigin {
    /// The design's own origin, coordinates as stored.
    Design,
    /// The lower-left corner of the board outline.
    BoardLowerLeft,
    /// The origin the board records for drill files (the auxiliary origin).
    Aux,
    /// The centre of the board outline's bounding box.
    BoardCentre,
}

/// What happens to the angle of a bottom-side part.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BottomRotation {
    /// The stored angle, as seen from the top.
    AsPlaced,
    /// The angle negated: the part turned over about a horizontal axis.
    Negate,
    /// 180 degrees minus the angle: the part turned over about a vertical axis.
    Mirror,
}

/// How angles are written.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AngleRange {
    /// 0 up to but not including 360.
    #[serde(rename = "zero_to_360")]
    ZeroTo360,
    /// Above -180 up to and including 180.
    #[serde(rename = "signed_180")]
    Signed180,
}

/// What a flavour does with a part that is not to be fitted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DnpRule {
    /// Leave the part out of the file.
    Drop,
    /// Write it like any other part.
    Keep,
    /// Write it and say so in the `populate` column (`Y` or `N`).
    Flag,
}

/// What one column holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColumnField {
    /// Reference designator.
    Designator,
    /// X coordinate.
    X,
    /// Y coordinate.
    Y,
    /// Rotation in degrees.
    Rotation,
    /// Side word.
    Side,
    /// Package name.
    Package,
    /// Component value.
    Value,
    /// `SMT` or `THT`.
    Mount,
    /// `Y` or `N`, whether the part is fitted.
    Populate,
}

/// One column: the header text and what fills it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlacementColumn {
    /// Header text, written exactly.
    pub header: String,
    /// What the column holds.
    pub field: ColumnField,
}

/// One placement list layout.
///
/// # Rules
/// - Checked by [`PlacementFlavour::validate`] before any file is written.
///
/// # Dependencies
/// - crates/io/data/placement-flavours.toml holds the bundled records.
///
/// # Do not
/// - Add a field without a default when the record is read from a saved project.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlacementFlavour {
    /// Stable id, letters, digits, `_` and `-`.
    pub id: String,
    /// Plain name shown to the user.
    pub name: String,
    /// Field separator: comma, tab or semicolon.
    pub delimiter: String,
    /// Whether a header row is written.
    pub header: bool,
    /// Columns in file order.
    pub columns: Vec<PlacementColumn>,
    /// Coordinate unit.
    pub units: PlacementUnits,
    /// Decimals for coordinates and angles, 0 to 6.
    pub decimals: u8,
    /// Coordinate origin.
    pub origin: PlacementOrigin,
    /// Word for the top side.
    pub side_top: String,
    /// Word for the bottom side.
    pub side_bottom: String,
    /// One file per side instead of one file.
    pub split_by_side: bool,
    /// Leave through-hole parts out.
    pub smt_only: bool,
    /// Rule for parts that are not to be fitted.
    pub dnp: DnpRule,
    /// List fiducials as rows.
    pub fiducials: bool,
    /// Angle handling for bottom-side parts.
    pub bottom_rotation: BottomRotation,
    /// Mirror bottom-side X about the board's vertical centre line.
    pub bottom_mirror_x: bool,
    /// Angle range written.
    pub angle_range: AngleRange,
    /// File name without extension.
    pub file_stem: String,
    /// File extension without the dot.
    pub extension: String,
}

/// A rotation correction for one package: parts whose package starts with
/// `footprint` get `offset_deg` added before the bottom-side policy runs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RotationOffset {
    /// Package name prefix.
    pub footprint: String,
    /// Degrees to add.
    pub offset_deg: f64,
}

/// Choices for one export that are not part of the flavour.
#[derive(Clone, Debug, Default)]
pub struct PlacementOptions {
    /// Name of the assembly variant whose unpopulated parts count as not fitted.
    pub variant: Option<String>,
    /// Per-package rotation corrections; the first matching prefix wins.
    pub offsets: Vec<RotationOffset>,
}

/// The files of one export.
#[derive(Clone, Debug, PartialEq)]
pub struct PlacementOutput {
    /// Relative file name and text, in write order.
    pub files: Vec<(String, String)>,
}

#[derive(Deserialize)]
struct Bundle {
    flavour: Vec<PlacementFlavour>,
}

fn plain_id(s: &str, max: usize) -> bool {
    !s.is_empty()
        && s.len() <= max
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

impl PlacementFlavour {
    /// Check the record: sizes, characters and that a populate flag has a column.
    ///
    /// # Rules
    /// - Header text never holds the delimiter, a quote or a line break.
    ///
    /// # Do not
    /// - Skip this before writing a file from a record that came from outside.
    pub fn validate(&self) -> Result<(), String> {
        if !plain_id(&self.id, 48) {
            return Err("flavour id must be 1 to 48 letters, digits, _ or -".into());
        }
        if self.name.trim().is_empty() || self.name.len() > 96 {
            return Err("flavour name must be 1 to 96 characters".into());
        }
        if ![",", "\t", ";"].contains(&self.delimiter.as_str()) {
            return Err("delimiter must be a comma, a tab or a semicolon".into());
        }
        if self.columns.is_empty() || self.columns.len() > MAX_COLUMNS {
            return Err(format!("a flavour has 1 to {MAX_COLUMNS} columns"));
        }
        for c in &self.columns {
            if c.header.len() > 48 || c.header.contains(['\n', '\r', '"']) {
                return Err("column header is too long or has a quote or line break".into());
            }
            if c.header.contains(self.delimiter.as_str()) {
                return Err("column header holds the delimiter".into());
            }
        }
        if self.decimals > 6 {
            return Err("decimals must be 0 to 6".into());
        }
        for w in [&self.side_top, &self.side_bottom] {
            if w.is_empty() || w.len() > 16 || w.contains([',', ';', '\t', '\n', '\r', '"']) {
                return Err("side words are 1 to 16 plain characters".into());
            }
        }
        if !plain_id(&self.file_stem, 32) || !plain_id(&self.extension, 8) {
            return Err("file stem and extension are plain names".into());
        }
        if self.dnp == DnpRule::Flag
            && !self
                .columns
                .iter()
                .any(|c| c.field == ColumnField::Populate)
        {
            return Err("a populate flag needs a populate column".into());
        }
        Ok(())
    }
}

/// The flavours that ship with the app, in file order.
///
/// # Rules
/// - Every record passes [`PlacementFlavour::validate`]; a test pins that.
///
/// # Dependencies
/// - crates/io/data/placement-flavours.toml
///
/// # Do not
/// - Panic on a damaged data file; return an error instead.
pub fn bundled_placement_flavours() -> Result<Vec<PlacementFlavour>, String> {
    let bundle: Bundle = toml::from_str(BUNDLED).map_err(|e| format!("flavour data: {e}"))?;
    for f in &bundle.flavour {
        f.validate()?;
    }
    Ok(bundle.flavour)
}

/// Read one flavour record from TOML text, for a user-defined flavour.
///
/// # Rules
/// - The record is validated before it is returned.
pub fn placement_flavour_from_toml(text: &str) -> Result<PlacementFlavour, String> {
    if text.len() > 16 * 1024 {
        return Err("flavour record is too large".into());
    }
    let f: PlacementFlavour = toml::from_str(text).map_err(|e| format!("flavour record: {e}"))?;
    f.validate()?;
    Ok(f)
}

/// One placed part as a flavour sees it, before it is turned into text.
#[derive(Clone, Debug, PartialEq)]
pub struct PlacedRow {
    /// Reference designator.
    pub reference: String,
    /// Component id; 0 for a fiducial.
    pub component: u64,
    /// X in the flavour's unit, from the flavour's origin.
    pub x: f64,
    /// Y in the flavour's unit, from the flavour's origin.
    pub y: f64,
    /// Angle after the flavour's bottom policy and range.
    pub rotation: f64,
    /// Angle as stored on the board, before offsets and policy.
    pub stored_rotation: f64,
    /// True on the bottom face.
    pub bottom: bool,
    /// Package name as the design states it.
    pub package: String,
    /// Component value.
    pub value: String,
    /// True for a through-hole part.
    pub tht: bool,
    /// False when a variant or a property leaves the part unfitted.
    pub fitted: bool,
}
type Row = PlacedRow;

fn truthy(v: &str) -> bool {
    matches!(
        v.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "y" | "x" | "dnp"
    )
}

fn excluded_by_field(c: &Component) -> bool {
    c.fields
        .iter()
        .any(|f| EXCLUDE_KEYS.iter().any(|k| f.key.eq_ignore_ascii_case(k)) && truthy(&f.value))
}

fn is_tht(f: &Footprint) -> bool {
    f.pads.iter().any(|pad| {
        pad.geometry
            .as_ref()
            .is_some_and(|g| g.pad_type == "thru_hole")
    })
}

/// Sort key that puts C2 before C10.
fn natural(reference: &str) -> (String, u64, String) {
    let split = reference
        .find(|c: char| c.is_ascii_digit())
        .unwrap_or(reference.len());
    let (prefix, rest) = reference.split_at(split);
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    (
        prefix.to_string(),
        digits.parse().unwrap_or(0),
        rest[digits.len()..].to_string(),
    )
}

fn bounds(p: &Project) -> (Point, Point) {
    let pts: &[Point] = p.board.outline.as_deref().unwrap_or(&[]);
    if pts.len() >= 3 {
        let mut lo = pts[0];
        let mut hi = pts[0];
        for q in pts {
            lo = Point::new(lo.x.min(q.x), lo.y.min(q.y));
            hi = Point::new(hi.x.max(q.x), hi.y.max(q.y));
        }
        (lo, hi)
    } else {
        (
            Point::new(0.0, 0.0),
            Point::new(p.board.width, p.board.height),
        )
    }
}

/// Turn a stored angle into the written one under a bottom-side policy.
///
/// # Rules
/// - `offset` is added first; the policy applies to the sum.
/// - The result is normalised to `range`, so 360 never appears.
pub fn flavour_rotation(
    stored: f64,
    offset: f64,
    bottom: bool,
    policy: BottomRotation,
    range: AngleRange,
) -> f64 {
    let r = stored + offset;
    let r = match (bottom, policy) {
        (true, BottomRotation::Negate) => -r,
        (true, BottomRotation::Mirror) => 180.0 - r,
        _ => r,
    };
    let r = r.rem_euclid(360.0);
    match range {
        AngleRange::ZeroTo360 => r,
        AngleRange::Signed180 if r > 180.0 => r - 360.0,
        AngleRange::Signed180 => r,
    }
}

fn number(v: f64, decimals: u8) -> String {
    let s = format!("{:.*}", usize::from(decimals), v);
    let zero = s
        .trim_start_matches('-')
        .chars()
        .all(|c| c == '0' || c == '.');
    if zero {
        s.trim_start_matches('-').to_string()
    } else {
        s
    }
}

fn clean(s: &str) -> String {
    s.chars()
        .map(|c| {
            if matches!(c, ',' | ';' | '\t' | '\r' | '\n' | '"') {
                ' '
            } else {
                c
            }
        })
        .collect::<String>()
        .trim()
        .to_string()
}

/// The rows of a placement list as numbers, for a writer that adds its own columns.
///
/// # Rules
/// - The same rows, order, origin, unit scaling, bottom X mirror and rotation policy
///   that [`placement_flavoured`] writes; nothing is formatted.
/// - Validates the design and the flavour first.
///
/// # Dependencies
/// - crates/io/src/neoden_yy1.rs uses it for coordinates and rotation.
///
/// # Do not
/// - Format a number here; the caller rounds for its own file.
pub fn placed_rows(
    p: &Project,
    flavour: &PlacementFlavour,
    options: &PlacementOptions,
) -> Result<Vec<PlacedRow>, String> {
    p.validate()?;
    collect_rows(p, flavour, options)
}

/// Rows without the whole-design check, so tests can use back-side parts.
pub(crate) fn placed_rows_unchecked(
    p: &Project,
    flavour: &PlacementFlavour,
    options: &PlacementOptions,
) -> Result<Vec<PlacedRow>, String> {
    collect_rows(p, flavour, options)
}

fn collect_rows(
    p: &Project,
    fl: &PlacementFlavour,
    options: &PlacementOptions,
) -> Result<Vec<Row>, String> {
    fl.validate()?;
    if options.offsets.len() > MAX_OFFSETS
        || options
            .offsets
            .iter()
            .any(|o| !o.offset_deg.is_finite() || o.offset_deg.abs() > 360.0)
    {
        return Err("rotation offsets are limited to 512 rows of at most 360 degrees".into());
    }
    if p.board.footprints.len() > MAX_PLACED {
        return Err("placement file exceeds the placed-object limit".into());
    }
    let variant = match options.variant.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(name) => Some(
            p.variants
                .iter()
                .find(|v| v.name == name)
                .ok_or_else(|| format!("no variant named {name}"))?,
        ),
    };
    let comps: BTreeMap<u64, &Component> = p.components.iter().map(|c| (c.id, c)).collect();
    let (lo, hi) = bounds(p);
    let origin = match fl.origin {
        PlacementOrigin::Design => Point::new(0.0, 0.0),
        PlacementOrigin::BoardLowerLeft => lo,
        PlacementOrigin::Aux => p.board.drill_origin,
        PlacementOrigin::BoardCentre => Point::new((lo.x + hi.x) / 2.0, (lo.y + hi.y) / 2.0),
    };
    let scale = match fl.units {
        PlacementUnits::Mm => 1.0,
        PlacementUnits::Mil => 1000.0 / 25.4,
        PlacementUnits::Inch => 1.0 / 25.4,
    };
    let mut rows: Vec<Row> = Vec::new();
    for f in &p.board.footprints {
        let c = comps
            .get(&f.component)
            .ok_or("footprint references an unknown component")?;
        let tht = is_tht(f);
        if tht && fl.smt_only {
            continue;
        }
        let fitted = !(variant.is_some_and(|v| v.dnp.contains(&c.id)) || excluded_by_field(c));
        if !fitted && fl.dnp == DnpRule::Drop {
            continue;
        }
        let swapped = variant
            .and_then(|v| v.footprints.iter().find(|o| o.component == c.id))
            .map(|o| o.footprint.clone());
        let package = swapped.unwrap_or_else(|| match &c.library_footprint {
            Some(l) => l.definition.name.clone(),
            None => c.kind.clone(),
        });
        let bottom = f.board_side() == Layer::Back;
        let offset = options
            .offsets
            .iter()
            .find(|o| package.starts_with(&o.footprint))
            .map_or(0.0, |o| o.offset_deg);
        let x = if bottom && fl.bottom_mirror_x {
            lo.x + hi.x - f.position.x
        } else {
            f.position.x
        };
        rows.push(Row {
            reference: clean(&c.reference),
            component: c.id,
            stored_rotation: f.rotation,
            x: (x - origin.x) * scale,
            y: (f.position.y - origin.y) * scale,
            rotation: flavour_rotation(
                f.rotation,
                offset,
                bottom,
                fl.bottom_rotation,
                fl.angle_range,
            ),
            bottom,
            package: clean(&package),
            value: clean(&c.value),
            tht,
            fitted,
        });
    }
    rows.sort_by_key(|r| natural(&r.reference));
    if fl.fiducials {
        for (i, fid) in p.board.fiducials.iter().enumerate() {
            let bottom = fid.layer == Layer::Back;
            let x = if bottom && fl.bottom_mirror_x {
                lo.x + hi.x - fid.position.x
            } else {
                fid.position.x
            };
            rows.push(Row {
                reference: format!("FID{:02}", i + 1),
                component: 0,
                stored_rotation: 0.0,
                x: (x - origin.x) * scale,
                y: (fid.position.y - origin.y) * scale,
                rotation: 0.0,
                bottom,
                package: "FIDUCIAL".into(),
                value: String::new(),
                tht: false,
                fitted: true,
            });
        }
    }
    Ok(rows)
}

/// Write the placement list or lists for a design in one flavour.
///
/// # Rules
/// - Validates the design and the flavour first; an unknown variant is an error.
/// - Rows sort by reference in natural order, fiducials last.
/// - With `split_by_side` the files are `<stem>-top.<ext>` and `<stem>-bottom.<ext>`,
///   both written even when one is empty; otherwise the one file is `<stem>.<ext>`.
/// - Lines end in `\n`.
///
/// # Dependencies
/// - crates/doc/src/lib.rs
///
/// # Do not
/// - Change `placement_text` or reorder columns away from the flavour's order.
pub fn placement_flavoured(
    p: &Project,
    flavour: &PlacementFlavour,
    options: &PlacementOptions,
) -> Result<PlacementOutput, String> {
    p.validate()?;
    render(p, flavour, options)
}

/// The export without the whole-design check, so tests can put parts on the
/// bottom face of a design that cannot hold them yet.
fn render(
    p: &Project,
    fl: &PlacementFlavour,
    options: &PlacementOptions,
) -> Result<PlacementOutput, String> {
    let rows = collect_rows(p, fl, options)?;
    let line = |r: &Row| -> String {
        fl.columns
            .iter()
            .map(|c| match c.field {
                ColumnField::Designator => r.reference.clone(),
                ColumnField::X => number(r.x, fl.decimals),
                ColumnField::Y => number(r.y, fl.decimals),
                ColumnField::Rotation => number(r.rotation, fl.decimals),
                ColumnField::Side => if r.bottom {
                    &fl.side_bottom
                } else {
                    &fl.side_top
                }
                .clone(),
                ColumnField::Package => r.package.clone(),
                ColumnField::Value => r.value.clone(),
                ColumnField::Mount => if r.tht { "THT" } else { "SMT" }.into(),
                ColumnField::Populate => if r.fitted { "Y" } else { "N" }.into(),
            })
            .collect::<Vec<_>>()
            .join(&fl.delimiter)
    };
    let header = fl
        .columns
        .iter()
        .map(|c| c.header.as_str())
        .collect::<Vec<_>>()
        .join(&fl.delimiter);
    let file = |name: String, pick: &dyn Fn(&Row) -> bool| {
        let mut text = String::new();
        if fl.header {
            text.push_str(&header);
            text.push('\n');
        }
        for r in rows.iter().filter(|r| pick(r)) {
            text.push_str(&line(r));
            text.push('\n');
        }
        (name, text)
    };
    let (stem, ext) = (&fl.file_stem, &fl.extension);
    let files = if fl.split_by_side {
        vec![
            file(format!("{stem}-top.{ext}"), &|r| !r.bottom),
            file(format!("{stem}-bottom.{ext}"), &|r| r.bottom),
        ]
    } else {
        vec![file(format!("{stem}.{ext}"), &|_| true)]
    };
    Ok(PlacementOutput { files })
}
