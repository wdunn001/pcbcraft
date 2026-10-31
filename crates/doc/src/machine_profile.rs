//! Shop machine profiles: the numbers a fabrication writer needs from one
//! machine (work area, output dialect, laser operations, mill tools, pick and
//! place tables, UV printer limits), stored on the design as `Project.machine_profiles`.
//!
//! # Rules
//! - Pure data and bounds checks. No file or TOML code lives here; the TOML
//!   reader and writer are in crates/io/src/machine_profile.rs.
//! - Only the table that matches `kind` may be filled; every number is finite
//!   and bounded, and every row list holds at most `MAX_PROFILE_ROWS` rows.
//! - Refusals are one plain sentence a person can act on.
//!
//! # Dependencies
//! - crates/doc/src/lib.rs
//! - machine-profiles/SCHEMA.md
use serde::{Deserialize, Serialize};

/// Limit named max profile rows.
pub const MAX_PROFILE_ROWS: usize = 64;

const MAX_ID_LEN: usize = 64;
const MAX_NAME_LEN: usize = 128;
const MAX_VENDOR_LEN: usize = 128;
const MAX_MODEL_LEN: usize = 128;
const MAX_NOTES_LEN: usize = 4096;
const MAX_SHORT: usize = 64;

fn valid_slug(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_ID_LEN
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn bounded(s: &str, max: usize) -> bool {
    !s.is_empty() && s.len() <= max
}

fn optional_bounded(s: &str, max: usize) -> bool {
    s.len() <= max
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
/// Settings for work area.
pub struct WorkArea {
    /// Horizontal value in millimetres.
    pub x: f64,
    /// Vertical value in millimetres.
    pub y: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Depth value in millimetres.
    pub z: Option<f64>,
    /// Y length once a Quick Swap kit is fitted; absent when the machine has none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub y_with_quick_swap: Option<f64>,
    /// Origin.
    pub origin: String,
    /// Y up.
    pub y_up: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
/// Settings for machine output.
pub struct MachineOutput {
    /// Format.
    pub format: String,
    /// Dialect.
    pub dialect: String,
    /// Units.
    pub units: String,
    /// Line ending.
    pub line_ending: String,
    /// Mirror bottom.
    pub mirror_bottom: bool,
    /// Mirror axis.
    pub mirror_axis: String,
    /// Invert.
    pub invert: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
/// Settings for laser operation.
pub struct LaserOperation {
    /// Display name.
    pub name: String,
    /// Mode.
    pub mode: String,
    /// Speed in millimetres per second.
    pub speed_mm_s: f64,
    /// Power as a percentage.
    pub power_percent: f64,
    /// Passes.
    pub passes: u32,
    /// Frequency in kilohertz.
    pub frequency_khz: f64,
    /// Pulse width in nanoseconds.
    pub pulse_width_ns: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Line interval in millimetres.
    pub line_interval_mm: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Hatch angle in degrees.
    pub hatch_angle_deg: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Angle increment in degrees.
    pub angle_increment_deg: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Kerf offset in millimetres.
    pub kerf_offset_mm: Option<f64>,
    #[serde(default)]
    /// Verified.
    pub verified: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
/// Settings for laser section.
pub struct LaserSection {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    /// Operation.
    pub operation: Vec<LaserOperation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Field x in millimetres.
    pub field_x_mm: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Field y in millimetres.
    pub field_y_mm: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Source watts.
    pub source_watts: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Wavelength nm.
    pub wavelength_nm: Option<f64>,
    /// Square field sizes of the lenses the machine can take, in millimetres.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lens_options_mm: Vec<f64>,
    /// Lowest pulse repetition rate, in kilohertz.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frequency_khz_min: Option<f64>,
    /// Highest pulse repetition rate, in kilohertz.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frequency_khz_max: Option<f64>,
    /// Shortest pulse width, in nanoseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pulse_width_ns_min: Option<f64>,
    /// Longest pulse width, in nanoseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pulse_width_ns_max: Option<f64>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
/// Settings for spindle section.
pub struct SpindleSection {
    /// Speed style.
    pub speed_style: String,
    /// Max in revolutions per minute.
    pub max_rpm: f64,
    /// Default as a percentage.
    pub default_percent: f64,
    /// Min as a percentage.
    pub min_percent: f64,
    /// Max as a percentage.
    pub max_percent: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
/// Settings for gcode section.
pub struct GcodeSection {
    /// Header type.
    pub header_type: String,
    /// Tool head.
    pub tool_head: String,
    /// Machine label.
    pub machine_label: String,
    /// Flavor.
    pub flavor: String,
    /// Arcs allowed.
    pub arcs_allowed: bool,
    /// Tool change.
    pub tool_change: String,
    /// Safe z in millimetres.
    pub safe_z_mm: f64,
    /// Work speed in millimetres per minute.
    pub work_speed_mm_min: f64,
    /// Jog speed in millimetres per minute.
    pub jog_speed_mm_min: f64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    /// Preamble.
    pub preamble: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    /// Postamble.
    pub postamble: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
/// Settings for mill tool.
pub struct MillTool {
    /// Identifier.
    pub id: String,
    /// Diameter in millimetres.
    pub diameter_mm: f64,
    /// Feed in millimetres per minute.
    pub feed_mm_min: f64,
    /// Plunge in millimetres per minute.
    pub plunge_mm_min: f64,
    /// Spindle as a percentage.
    pub spindle_percent: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
/// Settings for mill material.
pub struct MillMaterial {
    /// Display name.
    pub name: String,
    /// Thickness in millimetres.
    pub thickness_mm: f64,
    /// Safe z in millimetres.
    pub safe_z_mm: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
/// Settings for mill section.
pub struct MillSection {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// Spindle.
    pub spindle: Option<SpindleSection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// Gcode.
    pub gcode: Option<GcodeSection>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    /// Tool.
    pub tool: Vec<MillTool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// Material.
    pub material: Option<MillMaterial>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
/// Settings for pick place board.
pub struct PickPlaceBoard {
    /// Max x in millimetres.
    pub max_x_mm: f64,
    /// Max y in millimetres.
    pub max_y_mm: f64,
    /// Origin top.
    pub origin_top: String,
    /// Origin bottom.
    pub origin_bottom: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
/// Settings for pick place rotation.
pub struct PickPlaceRotation {
    /// Min in degrees.
    pub min_deg: f64,
    /// Max in degrees.
    pub max_deg: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
/// Settings for pick place csv.
pub struct PickPlaceCsv {
    /// Native header.
    pub native_header: String,
    /// Import columns.
    pub import_columns: u32,
    /// Line ending.
    pub line_ending: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
/// Settings for feeder bank.
pub struct FeederBank {
    /// Display name.
    pub name: String,
    /// First.
    pub first: u32,
    /// Last.
    pub last: u32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
/// Settings for nozzle row.
pub struct NozzleRow {
    /// Identifier.
    pub id: String,
    /// Inner (suction) diameter in millimetres.
    pub diameter_mm: f64,
    /// Outer tip diameter in millimetres; 0 when the maker does not state it.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub outer_diameter_mm: f64,
    /// Package names this nozzle picks, matched as prefixes of the short package name.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub packages: Vec<String>,
}

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

/// One automatic-choice rule for a part: when the package name, the largest pad
/// dimension and the body height fall inside the row, the row names the nozzle
/// and the default speed and heights. The first matching row wins.
///
/// # Rules
/// - `package` is a prefix of the short package name after the profile's aliases; empty matches any.
/// - A bound of 0 means no bound. A height bound only applies when the part states its height.
///
/// # Dependencies
/// - crates/doc/src/placement_machine.rs applies the rows.
///
/// # Do not
/// - Name a nozzle that the profile's nozzle list does not hold; validation refuses it.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct PlacementRuleRow {
    /// Package name prefix; empty matches every package.
    pub package: String,
    /// Smallest largest-pad-dimension in millimetres (0 = no bound).
    pub min_size_mm: f64,
    /// Largest largest-pad-dimension in millimetres (0 = no bound).
    pub max_size_mm: f64,
    /// Smallest body height in millimetres (0 = no bound).
    pub min_height_mm: f64,
    /// Largest body height in millimetres (0 = no bound).
    pub max_height_mm: f64,
    /// Nozzle id this row chooses.
    pub nozzle: String,
    /// Default placement speed in percent; 0 uses the profile default.
    pub speed_pct: u32,
    /// Default pick height offset in millimetres.
    pub pick_height_mm: f64,
    /// Default place height offset in millimetres.
    pub place_height_mm: f64,
    /// True when parts matching the row are left out of the run (test points, marks).
    pub skip: bool,
    /// Why the row exists, in a plain sentence.
    pub note: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
/// Settings for rotation offset row.
pub struct RotationOffsetRow {
    /// Comment.
    pub comment: String,
    /// Footprint.
    pub footprint: String,
    /// Offset in degrees.
    pub offset_deg: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
/// Settings for footprint alias row.
pub struct FootprintAliasRow {
    /// Pattern.
    pub pattern: String,
    /// Replacement.
    pub replacement: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
/// Settings for feeder map row.
pub struct FeederMapRow {
    /// Comment.
    pub comment: String,
    /// Footprint.
    pub footprint: String,
    /// Feeder no.
    pub feeder_no: u32,
    /// Nozzle.
    pub nozzle: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
/// Settings for pick place section.
pub struct PickPlaceSection {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// Board.
    pub board: Option<PickPlaceBoard>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// Rotation.
    pub rotation: Option<PickPlaceRotation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// Csv.
    pub csv: Option<PickPlaceCsv>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    /// Feeders.
    pub feeders: Vec<FeederBank>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    /// Nozzle.
    pub nozzle: Vec<NozzleRow>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    /// Rotation offset.
    pub rotation_offset: Vec<RotationOffsetRow>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    /// Footprint alias.
    pub footprint_alias: Vec<FootprintAliasRow>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    /// Feeder map.
    pub feeder_map: Vec<FeederMapRow>,
    /// Nozzle types the machine can use in one run without an operator
    /// (nozzles on the heads plus nozzles in the change station); 0 = unlimited.
    pub nozzle_slots: u32,
    /// Placement speed in percent when neither a rule nor a part says one; 0 = 75.
    pub default_speed_pct: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    /// Rules that choose a nozzle, speed and heights from package size and body height.
    pub placement_rule: Vec<PlacementRuleRow>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
/// Settings for uv print section.
pub struct UvPrintSection {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    /// Dpi options.
    pub dpi_options: Vec<u32>,
    /// Default dpi.
    pub default_dpi: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    /// Channels.
    pub channels: Vec<String>,
    /// Registration.
    pub registration: String,
    /// White choke in millimetres.
    pub white_choke_mm: f64,
    /// Max substrate height in millimetres.
    pub max_substrate_height_mm: f64,
    /// Relief in millimetres.
    pub relief_mm: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
/// Settings for uv artwork section.
pub struct UvArtworkSection {
    /// Default bleed in millimetres.
    pub default_bleed_mm: f64,
    /// Pad clearance in millimetres.
    pub pad_clearance_mm: f64,
    /// Min text height in millimetres.
    pub min_text_height_mm: f64,
    /// Min stroke in millimetres.
    pub min_stroke_mm: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
/// Settings for uv limits section.
pub struct UvLimitsSection {
    /// Max width in millimetres.
    pub max_width_mm: f64,
    /// Max height in millimetres.
    pub max_height_mm: f64,
    /// Max pixels.
    pub max_pixels: u32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
/// Settings for uv printer section.
pub struct UvPrinterSection {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// Print.
    pub print: Option<UvPrintSection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// Artwork.
    pub artwork: Option<UvArtworkSection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// Limits.
    pub limits: Option<UvLimitsSection>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
/// One shop machine as the design stores it: identity, work area, output
/// dialect and the single kind-specific table the writers read.
///
/// # Rules
/// - `id` is a lowercase slug and is what an artifact refers to; it is unique per design.
/// - A built-in preset is copied into the design, so editing a design copy never changes the preset.
///
/// # Dependencies
/// - machine-profiles/SCHEMA.md
///
/// # Do not
/// - Add a field without a bound in `validate`.
pub struct MachineProfile {
    /// Identifier.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Kind of the item.
    pub kind: String,
    /// Vendor.
    pub vendor: String,
    /// Model.
    pub model: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    /// Notes.
    pub notes: String,
    /// Work area.
    pub work_area: WorkArea,
    /// Output.
    pub output: MachineOutput,
    #[serde(default, skip_serializing_if = "LaserSection::is_empty")]
    /// Laser.
    pub laser: LaserSection,
    #[serde(default, skip_serializing_if = "MillSection::is_empty")]
    /// Mill.
    pub mill: MillSection,
    #[serde(default, skip_serializing_if = "PickPlaceSection::is_empty")]
    /// Pick place.
    pub pick_place: PickPlaceSection,
    #[serde(default, skip_serializing_if = "UvPrinterSection::is_empty")]
    /// Uv printer.
    pub uv_printer: UvPrinterSection,
}

impl LaserSection {
    fn is_empty(&self) -> bool {
        self.operation.is_empty()
            && self.field_x_mm.is_none()
            && self.field_y_mm.is_none()
            && self.source_watts.is_none()
            && self.wavelength_nm.is_none()
            && self.lens_options_mm.is_empty()
            && self.frequency_khz_min.is_none()
            && self.frequency_khz_max.is_none()
            && self.pulse_width_ns_min.is_none()
            && self.pulse_width_ns_max.is_none()
    }
}

impl MillSection {
    fn is_empty(&self) -> bool {
        self.spindle.is_none()
            && self.gcode.is_none()
            && self.tool.is_empty()
            && self.material.is_none()
    }
}

impl PickPlaceSection {
    fn is_empty(&self) -> bool {
        self.board.is_none()
            && self.rotation.is_none()
            && self.csv.is_none()
            && self.feeders.is_empty()
            && self.nozzle.is_empty()
            && self.rotation_offset.is_empty()
            && self.footprint_alias.is_empty()
            && self.feeder_map.is_empty()
            && self.placement_rule.is_empty()
            && self.nozzle_slots == 0
            && self.default_speed_pct == 0
    }
}

impl UvPrinterSection {
    fn is_empty(&self) -> bool {
        self.print.is_none() && self.artwork.is_none() && self.limits.is_none()
    }
}

impl MachineProfile {
    /// Refuse a profile whose numbers, labels or tables are out of bounds.
    ///
    /// # Rules
    /// - Returns the first problem as a plain sentence; never panics.
    /// - Rejects a table that does not belong to the profile's `kind`.
    ///
    /// # Do not
    /// - Accept a non-finite number or a row list longer than `MAX_PROFILE_ROWS`.
    pub fn validate(&self) -> Result<(), String> {
        if !valid_slug(&self.id) {
            return Err(
                "machine profile id must be 1 to 64 lowercase letters, digits or hyphens".into(),
            );
        }
        if !bounded(&self.name, MAX_NAME_LEN) {
            return Err("machine profile name must be 1 to 128 bytes".into());
        }
        if !optional_bounded(&self.vendor, MAX_VENDOR_LEN)
            || !optional_bounded(&self.model, MAX_MODEL_LEN)
            || !optional_bounded(&self.notes, MAX_NOTES_LEN)
        {
            return Err("machine profile vendor, model or notes exceed the length limit".into());
        }
        let kind = self.kind.as_str();
        if !matches!(
            kind,
            "laser_galvo" | "laser_gantry" | "cnc_mill" | "pick_place" | "uv_printer"
        ) {
            return Err("machine profile kind must be laser_galvo, laser_gantry, cnc_mill, pick_place or uv_printer".into());
        }
        self.validate_work_area()?;
        self.validate_output()?;
        match kind {
            "laser_galvo" | "laser_gantry" => {
                if !self.mill.is_empty()
                    || !self.pick_place.is_empty()
                    || !self.uv_printer.is_empty()
                {
                    return Err("laser profiles may only carry laser settings".into());
                }
                self.validate_laser()?;
            }
            "cnc_mill" => {
                if !self.laser.is_empty()
                    || !self.pick_place.is_empty()
                    || !self.uv_printer.is_empty()
                {
                    return Err("mill profiles may only carry mill settings".into());
                }
                self.validate_mill()?;
            }
            "pick_place" => {
                if !self.laser.is_empty() || !self.mill.is_empty() || !self.uv_printer.is_empty() {
                    return Err("pick and place profiles may only carry placement settings".into());
                }
                self.validate_pick_place()?;
            }
            "uv_printer" => {
                if !self.laser.is_empty() || !self.mill.is_empty() || !self.pick_place.is_empty() {
                    return Err("uv printer profiles may only carry print settings".into());
                }
                self.validate_uv_printer()?;
            }
            _ => {}
        }
        Ok(())
    }

    fn validate_work_area(&self) -> Result<(), String> {
        let w = &self.work_area;
        if !w.x.is_finite()
            || !w.y.is_finite()
            || w.x <= 0.0
            || w.y <= 0.0
            || w.x > 5000.0
            || w.y > 5000.0
        {
            return Err("work area width and height must be positive finite millimetres".into());
        }
        if w.z
            .is_some_and(|z| !z.is_finite() || z <= 0.0 || z > 5000.0)
        {
            return Err("work area depth must be a positive finite millimetre value".into());
        }
        if w.y_with_quick_swap
            .is_some_and(|y| !y.is_finite() || y <= 0.0 || y > w.y)
        {
            return Err(
                "Quick Swap work depth must be positive and not exceed the full depth".into(),
            );
        }
        if !optional_bounded(&w.origin, MAX_SHORT) {
            return Err("work area origin label is too long".into());
        }
        Ok(())
    }

    fn validate_output(&self) -> Result<(), String> {
        let o = &self.output;
        if !optional_bounded(&o.format, MAX_SHORT)
            || !optional_bounded(&o.dialect, MAX_SHORT)
            || !optional_bounded(&o.units, MAX_SHORT)
            || !optional_bounded(&o.line_ending, MAX_SHORT)
            || !optional_bounded(&o.mirror_axis, MAX_SHORT)
        {
            return Err("output header fields are too long".into());
        }
        Ok(())
    }

    fn validate_laser(&self) -> Result<(), String> {
        if self.laser.operation.len() > MAX_PROFILE_ROWS {
            return Err("laser profiles carry at most 64 operations".into());
        }
        if self.laser.lens_options_mm.len() > 8
            || self
                .laser
                .lens_options_mm
                .iter()
                .any(|v| !v.is_finite() || *v <= 0.0 || *v > 2000.0)
        {
            return Err("laser lens sizes must be up to 8 positive millimetre values".into());
        }
        for (low, high, limit) in [
            (
                self.laser.frequency_khz_min,
                self.laser.frequency_khz_max,
                100_000.0,
            ),
            (
                self.laser.pulse_width_ns_min,
                self.laser.pulse_width_ns_max,
                1_000_000.0,
            ),
        ] {
            let ok = |v: Option<f64>| v.is_none_or(|x| x.is_finite() && x > 0.0 && x <= limit);
            if !ok(low) || !ok(high) || matches!((low, high), (Some(a), Some(b)) if a > b) {
                return Err("laser frequency or pulse width range is out of bounds".into());
            }
        }
        for op in &self.laser.operation {
            if !bounded(&op.name, MAX_SHORT) || !optional_bounded(&op.mode, MAX_SHORT) {
                return Err("laser operation name or mode is invalid".into());
            }
            if !op.speed_mm_s.is_finite()
                || op.speed_mm_s <= 0.0
                || op.speed_mm_s > 10_000.0
                || !op.power_percent.is_finite()
                || op.power_percent < 0.0
                || op.power_percent > 100.0
                || op.passes == 0
                || op.passes > 1000
                || !op.frequency_khz.is_finite()
                || op.frequency_khz <= 0.0
                || !op.pulse_width_ns.is_finite()
                || op.pulse_width_ns <= 0.0
            {
                return Err("laser operation speed, power or timing is out of range".into());
            }
        }
        Ok(())
    }

    fn validate_mill(&self) -> Result<(), String> {
        if self.mill.tool.len() > MAX_PROFILE_ROWS {
            return Err("mill profiles carry at most 64 tools".into());
        }
        if let Some(spindle) = &self.mill.spindle
            && (!spindle.default_percent.is_finite()
                || spindle.default_percent < 50.0
                || spindle.default_percent > 100.0
                || !spindle.min_percent.is_finite()
                || spindle.min_percent < 50.0
                || spindle.min_percent > 100.0
                || !spindle.max_percent.is_finite()
                || spindle.max_percent < 50.0
                || spindle.max_percent > 100.0
                || spindle.min_percent > spindle.max_percent)
        {
            return Err("mill spindle percent must stay between 50 and 100".into());
        }
        for tool in &self.mill.tool {
            if !bounded(&tool.id, MAX_SHORT) {
                return Err("mill tool id must be 1 to 64 bytes".into());
            }
            if !tool.diameter_mm.is_finite()
                || tool.diameter_mm <= 0.0
                || tool.diameter_mm > 100.0
                || !tool.feed_mm_min.is_finite()
                || tool.feed_mm_min <= 0.0
                || tool.feed_mm_min > 50_000.0
                || !tool.plunge_mm_min.is_finite()
                || tool.plunge_mm_min <= 0.0
                || tool.plunge_mm_min > 50_000.0
                || !tool.spindle_percent.is_finite()
                || tool.spindle_percent < 50.0
                || tool.spindle_percent > 100.0
            {
                return Err("mill tool diameter, feed or spindle percent is out of range".into());
            }
        }
        Ok(())
    }

    fn validate_pick_place(&self) -> Result<(), String> {
        let pp = &self.pick_place;
        if let Some(board) = &pp.board
            && (!board.max_x_mm.is_finite()
                || !board.max_y_mm.is_finite()
                || board.max_x_mm <= 0.0
                || board.max_y_mm <= 0.0
                || board.max_x_mm > 5000.0
                || board.max_y_mm > 5000.0)
        {
            return Err("pick and place board limits must be positive millimetres".into());
        }
        for bank in &pp.feeders {
            if bank.first == 0 || bank.first > bank.last || bank.last > 999 {
                return Err("feeder banks must run from 1 upward with first not after last".into());
            }
        }
        for row in &pp.feeder_map {
            if row.feeder_no == 0 || row.feeder_no > 999 {
                return Err("a feeder number must be between 1 and 999".into());
            }
        }
        for row in &pp.nozzle {
            if !bounded(&row.id, MAX_SHORT)
                || !row.diameter_mm.is_finite()
                || row.diameter_mm < 0.0
                || row.packages.len() > 16
                || row.packages.iter().any(|p| !bounded(p, MAX_SHORT))
            {
                return Err("a nozzle needs an id, a diameter and up to 16 package names".into());
            }
        }
        if pp.placement_rule.len() > MAX_PROFILE_ROWS * 4
            || pp.nozzle_slots > 16
            || pp.default_speed_pct > 100
        {
            return Err("placement rules are capped at 256 rows, nozzle slots at 16 and speed at 100 percent".into());
        }
        for rule in &pp.placement_rule {
            let nums = [
                rule.min_size_mm,
                rule.max_size_mm,
                rule.min_height_mm,
                rule.max_height_mm,
                rule.pick_height_mm,
                rule.place_height_mm,
            ];
            if nums.iter().any(|n| !n.is_finite() || n.abs() > 1000.0)
                || rule.speed_pct > 100
                || !optional_bounded(&rule.package, MAX_SHORT)
                || !optional_bounded(&rule.note, MAX_NOTES_LEN)
            {
                return Err(
                    "a placement rule has a number out of range or text that is too long".into(),
                );
            }
            if !(rule.skip && rule.nozzle.is_empty())
                && !pp.nozzle.iter().any(|n| n.id == rule.nozzle)
            {
                return Err(format!(
                    "a placement rule names nozzle {} which the machine does not list",
                    rule.nozzle
                ));
            }
        }
        if pp
            .rotation_offset
            .iter()
            .any(|r| !r.offset_deg.is_finite() || r.offset_deg.abs() > 360.0)
        {
            return Err("a rotation offset must be within 360 degrees either way".into());
        }
        if self.pick_place.rotation_offset.len() > MAX_PROFILE_ROWS
            || self.pick_place.footprint_alias.len() > MAX_PROFILE_ROWS
            || self.pick_place.feeder_map.len() > MAX_PROFILE_ROWS
            || self.pick_place.nozzle.len() > MAX_PROFILE_ROWS
            || self.pick_place.feeders.len() > MAX_PROFILE_ROWS
        {
            return Err("pick and place tables are capped at 64 rows each".into());
        }
        Ok(())
    }

    fn validate_uv_printer(&self) -> Result<(), String> {
        if let Some(print) = &self.uv_printer.print {
            if print.dpi_options.len() > 8 || print.dpi_options.is_empty() {
                return Err("uv printer needs one to eight dpi choices".into());
            }
            if print.dpi_options.iter().any(|d| *d < 300 || *d > 4800) {
                return Err("uv printer dpi choices must be between 300 and 4800".into());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_laser() -> MachineProfile {
        MachineProfile {
            id: "test-laser".into(),
            name: "Test laser".into(),
            kind: "laser_galvo".into(),
            vendor: "Lab".into(),
            model: "X".into(),
            notes: String::new(),
            work_area: WorkArea {
                y_with_quick_swap: None,
                x: 150.0,
                y: 150.0,
                z: None,
                origin: "center".into(),
                y_up: true,
            },
            output: MachineOutput {
                format: "svg".into(),
                dialect: "laser_job".into(),
                units: "mm".into(),
                line_ending: "lf".into(),
                mirror_bottom: true,
                mirror_axis: "x".into(),
                invert: false,
            },
            laser: LaserSection {
                operation: vec![LaserOperation {
                    name: "fiducials".into(),
                    mode: "line".into(),
                    speed_mm_s: 100.0,
                    power_percent: 30.0,
                    passes: 1,
                    frequency_khz: 50.0,
                    pulse_width_ns: 200.0,
                    line_interval_mm: None,
                    hatch_angle_deg: None,
                    angle_increment_deg: None,
                    kerf_offset_mm: None,
                    verified: false,
                }],
                field_x_mm: Some(150.0),
                field_y_mm: Some(150.0),
                source_watts: Some(30.0),
                wavelength_nm: Some(1064.0),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn minimal_mill() -> MachineProfile {
        MachineProfile {
            id: "test-mill".into(),
            name: "Test mill".into(),
            kind: "cnc_mill".into(),
            vendor: "Lab".into(),
            model: "Y".into(),
            notes: String::new(),
            work_area: WorkArea {
                y_with_quick_swap: None,
                x: 320.0,
                y: 325.0,
                z: Some(275.0),
                origin: "lower-left".into(),
                y_up: false,
            },
            output: MachineOutput {
                format: "gcode".into(),
                dialect: "snapmaker".into(),
                units: "mm".into(),
                line_ending: "lf".into(),
                mirror_bottom: false,
                mirror_axis: "x".into(),
                invert: false,
            },
            mill: MillSection {
                spindle: Some(SpindleSection {
                    speed_style: "percent".into(),
                    max_rpm: 18_000.0,
                    default_percent: 100.0,
                    min_percent: 50.0,
                    max_percent: 100.0,
                }),
                gcode: Some(GcodeSection {
                    header_type: "cnc".into(),
                    tool_head: "levelTwoCNCToolheadForSM2".into(),
                    machine_label: "Snapmaker 2.0 A350".into(),
                    flavor: "marlin".into(),
                    arcs_allowed: false,
                    tool_change: "new_file".into(),
                    safe_z_mm: 5.0,
                    work_speed_mm_min: 300.0,
                    jog_speed_mm_min: 1500.0,
                    preamble: vec!["G90".into(), "G21".into()],
                    postamble: vec!["M400".into(), "M5".into()],
                }),
                tool: vec![MillTool {
                    id: "iso".into(),
                    diameter_mm: 0.2,
                    feed_mm_min: 300.0,
                    plunge_mm_min: 100.0,
                    spindle_percent: 100.0,
                }],
                material: Some(MillMaterial {
                    name: "fr4".into(),
                    thickness_mm: 1.6,
                    safe_z_mm: 2.0,
                }),
            },
            ..Default::default()
        }
    }

    #[test]
    fn machine_profile_rejects_negative_work_area() {
        let mut p = minimal_laser();
        p.work_area.x = -1.0;
        assert!(p.validate().is_err());
        p.work_area.x = 150.0;
        p.work_area.y = 0.0;
        assert!(p.validate().is_err());
    }

    #[test]
    fn machine_profile_rejects_feed_or_spindle_out_of_range() {
        let mut p = minimal_mill();
        p.mill.tool[0].feed_mm_min = 0.0;
        assert!(p.validate().is_err());
        p.mill.tool[0].feed_mm_min = 300.0;
        p.mill.tool[0].spindle_percent = 40.0;
        assert!(p.validate().is_err());
        if let Some(spindle) = p.mill.spindle.as_mut() {
            spindle.default_percent = 40.0;
        }
        p.mill.tool[0].spindle_percent = 100.0;
        assert!(p.validate().is_err());
    }

    #[test]
    fn machine_profiles_round_trip_through_project_json() -> Result<(), String> {
        let mut project = crate::Project::default();
        project.machine_profiles.push(minimal_laser());
        project.machine_profiles.push(minimal_mill());
        project.validate()?;
        let json = serde_json::to_string(&project).map_err(|e| e.to_string())?;
        let back: crate::Project = serde_json::from_str(&json).map_err(|e| e.to_string())?;
        back.validate()?;
        if back.machine_profiles.len() != 2 || back.machine_profiles[0].id != "test-laser" {
            return Err("machine profiles did not round trip".into());
        }
        Ok(())
    }
}
