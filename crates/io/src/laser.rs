//! Galvo laser job writers (SVG and LaserCraft `.lcproj`).
use pcbcraft_doc::{Footprint, Layer, Pad, Project};
use pcbcraft_geom::{Point, hull, polygon_is_simple};
use serde::Serialize;
use std::collections::BTreeSet;

pub const LASER_OPERATION_IDS: [&str; 6] = [
    "fiducials",
    "isolation-cut",
    "copper-rubout",
    "mask-open",
    "legend",
    "stencil-cut",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MirrorAxis {
    X,
    Y,
}

impl MirrorAxis {
    pub fn parse(raw: &str) -> Result<Self, String> {
        match raw {
            "x" => Ok(Self::X),
            "y" => Ok(Self::Y),
            _ => Err("mirror axis must be x or y".into()),
        }
    }
}

#[derive(Clone, Debug)]
pub struct LaserGalvoField {
    pub width_mm: f64,
    pub height_mm: f64,
    pub label: String,
}

#[derive(Clone, Debug)]
pub struct LaserGalvoOperation {
    pub name: String,
    pub mode: String,
    pub speed_mm_s: f64,
    pub power_min: f64,
    pub power_max: f64,
    pub passes: u32,
    pub frequency_khz: f64,
    pub q_pulse_width_ns: f64,
    pub line_interval_mm: Option<f64>,
    pub hatch_angle_deg: Option<f64>,
    pub angle_increment_deg: Option<f64>,
    pub kerf_offset_mm: Option<f64>,
    pub verified: bool,
}

#[derive(Clone, Debug)]
pub struct LaserGalvoProfile {
    pub id: String,
    pub kind: String,
    pub fields: Vec<LaserGalvoField>,
    pub frequency_khz_min: f64,
    pub frequency_khz_max: f64,
    pub pulse_width_ns_min: f64,
    pub pulse_width_ns_max: f64,
    pub operations: Vec<LaserGalvoOperation>,
}

#[derive(Clone, Debug)]
pub struct LaserJobExportOptions {
    pub side: Layer,
    pub operations: BTreeSet<String>,
    pub mirror_axis: MirrorAxis,
    pub margin_mm: f64,
    pub isolation_width_mm: f64,
    pub rubout: bool,
    pub stencil_reduction_pct: f64,
}

/// Resolve a galvo laser profile by id, preferring the design's own copy.
///
/// The job reads the design's own copy (`Project.machine_profiles`) so edits
/// apply; a built-in preset of the same id is the fallback.
///
/// # Rules
/// - Only `laser_galvo` profiles qualify; any other kind is refused in plain words.
///
/// # Dependencies
/// - crates/doc/src/machine_profile.rs
/// - crates/io/src/machine_profile.rs
///
/// # Do not
/// - Read a profile from anywhere but the design or the built-in presets.
pub fn laser_galvo_profile_for_export(
    project: &Project,
    profile_id: &str,
) -> Result<LaserGalvoProfile, String> {
    if let Some(found) = project.machine_profiles.iter().find(|m| m.id == profile_id) {
        return laser_galvo_from_machine(found);
    }
    laser_galvo_builtin_profile(profile_id)
}

/// Resolve a built-in galvo preset by id without consulting a design.
///
/// # Dependencies
/// - crates/io/src/machine_profile.rs
pub fn laser_galvo_builtin_profile(profile_id: &str) -> Result<LaserGalvoProfile, String> {
    if profile_id == "commarker-b6-mopa-150" {
        let mut profile = commarker_b6_mopa_profile()?;
        profile.id = "commarker-b6-mopa-150".into();
        profile.fields.retain(|field| field.width_mm <= 150.0);
        return Ok(profile);
    }
    if crate::builtin_machine_profile_ids().contains(&profile_id) {
        return laser_galvo_from_machine(&crate::builtin_machine_profile(profile_id)?);
    }
    Err(format!("unknown laser galvo profile \"{profile_id}\""))
}

/// The ComMarker B6 MOPA preset as a galvo job profile.
///
/// # Dependencies
/// - machine-profiles/commarker-b6-mopa.toml
pub fn commarker_b6_mopa_profile() -> Result<LaserGalvoProfile, String> {
    laser_galvo_from_machine(&crate::builtin_machine_profile("commarker-b6-mopa")?)
}

/// Convert a stored machine profile into what the job writers read.
///
/// # Rules
/// - Refuses a profile that is not `laser_galvo` or lacks a field size or
///   frequency and pulse ranges, naming what is missing.
/// - Lens sizes other than the profile's own field are labelled unverified.
fn laser_galvo_from_machine(m: &pcbcraft_doc::MachineProfile) -> Result<LaserGalvoProfile, String> {
    if m.kind != "laser_galvo" {
        return Err(format!("machine profile \"{}\" is not a galvo laser", m.id));
    }
    m.validate()?;
    let laser = &m.laser;
    let field = laser
        .field_x_mm
        .ok_or_else(|| format!("machine profile \"{}\" has no field size", m.id))?;
    let lenses: Vec<f64> = if laser.lens_options_mm.is_empty() {
        vec![field]
    } else {
        laser.lens_options_mm.clone()
    };
    let range = |low: Option<f64>, high: Option<f64>, what: &str| match (low, high) {
        (Some(low), Some(high)) => Ok((low, high)),
        _ => Err(format!("machine profile \"{}\" has no {what} range", m.id)),
    };
    let (frequency_khz_min, frequency_khz_max) = range(
        laser.frequency_khz_min,
        laser.frequency_khz_max,
        "frequency",
    )?;
    let (pulse_width_ns_min, pulse_width_ns_max) = range(
        laser.pulse_width_ns_min,
        laser.pulse_width_ns_max,
        "pulse width",
    )?;
    Ok(LaserGalvoProfile {
        id: m.id.clone(),
        kind: m.kind.clone(),
        fields: lenses
            .into_iter()
            .map(|size| LaserGalvoField {
                width_mm: size,
                height_mm: size,
                label: if (size - field).abs() < 1e-9 {
                    format!("F-theta {size} mm")
                } else {
                    format!("F-theta {size} mm (unverified)")
                },
            })
            .collect(),
        frequency_khz_min,
        frequency_khz_max,
        pulse_width_ns_min,
        pulse_width_ns_max,
        operations: laser
            .operation
            .iter()
            .map(|o| LaserGalvoOperation {
                name: o.name.clone(),
                mode: o.mode.clone(),
                speed_mm_s: o.speed_mm_s,
                power_min: o.power_percent,
                power_max: o.power_percent,
                passes: o.passes,
                frequency_khz: o.frequency_khz,
                q_pulse_width_ns: o.pulse_width_ns,
                line_interval_mm: o.line_interval_mm,
                hatch_angle_deg: o.hatch_angle_deg,
                angle_increment_deg: o.angle_increment_deg,
                kerf_offset_mm: o.kerf_offset_mm,
                verified: o.verified,
            })
            .collect(),
    })
}

pub fn laser_job_work_area_mm(p: &Project, margin_mm: f64) -> Result<(f64, f64), String> {
    if !margin_mm.is_finite() || !(0.0..=50.0).contains(&margin_mm) {
        return Err("margin must be 0 to 50 millimetres".into());
    }
    let (min, max) = board_content_bounds(p);
    let width = (max.x - min.x) + 2.0 * margin_mm;
    let height = (max.y - min.y) + 2.0 * margin_mm;
    if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
        return Err("board bounds are not usable for laser export".into());
    }
    Ok((width, height))
}

pub fn laser_job_validate_work_area(
    profile: &LaserGalvoProfile,
    width_mm: f64,
    height_mm: f64,
) -> Result<(), String> {
    let fits = profile
        .fields
        .iter()
        .any(|field| width_mm <= field.width_mm + 1e-9 && height_mm <= field.height_mm + 1e-9);
    if fits {
        Ok(())
    } else {
        Err(format!(
            "the board plus margin is {width_mm:.1} by {height_mm:.1} mm and exceeds every work area in profile {}",
            profile.id
        ))
    }
}

pub fn laser_job_svg(
    p: &Project,
    side: Layer,
    profile: &LaserGalvoProfile,
    options: &LaserJobExportOptions,
) -> Result<String, String> {
    validate_profile_operations(profile, &options.operations)?;
    let (width_mm, height_mm) = laser_job_work_area_mm(p, options.margin_mm)?;
    laser_job_validate_work_area(profile, width_mm, height_mm)?;
    let (min, max) = board_content_bounds(p);
    let frame = JobFrame {
        min,
        max,
        margin: options.margin_mm,
        mirror: side == Layer::Back,
        axis: options.mirror_axis,
    };
    let groups = collect_groups(p, side, profile, options, &frame)?;
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width_mm}mm\" height=\"{height_mm}mm\" viewBox=\"0 0 {width_mm} {height_mm}\">\n"
    );
    for group in groups {
        svg.push_str(&format!("  <g id=\"{}\">\n", group.id));
        for ring in &group.rings {
            svg.push_str("    ");
            svg.push_str(&path_for_ring(ring, &frame));
            svg.push('\n');
        }
        svg.push_str("  </g>\n");
    }
    svg.push_str("</svg>\n");
    Ok(svg)
}

pub fn laser_job_lcproj(
    p: &Project,
    side: Layer,
    profile: &LaserGalvoProfile,
    options: &LaserJobExportOptions,
) -> Result<String, String> {
    validate_profile_operations(profile, &options.operations)?;
    let (width_mm, height_mm) = laser_job_work_area_mm(p, options.margin_mm)?;
    laser_job_validate_work_area(profile, width_mm, height_mm)?;
    let (min, max) = board_content_bounds(p);
    let frame = JobFrame {
        min,
        max,
        margin: options.margin_mm,
        mirror: side == Layer::Back,
        axis: options.mirror_axis,
    };
    let groups = collect_groups(p, side, profile, options, &frame)?;
    let mut cut_settings: Vec<LcCutSetting> = Vec::new();
    let mut shapes: Vec<LcShape> = Vec::new();
    for (index, group) in groups.iter().enumerate() {
        let op = profile
            .operations
            .iter()
            .find(|row| row.name == group.id)
            .ok_or_else(|| format!("profile is missing operation \"{}\"", group.id))?;
        if op.frequency_khz < profile.frequency_khz_min - 1e-9
            || op.frequency_khz > profile.frequency_khz_max + 1e-9
            || op.q_pulse_width_ns < profile.pulse_width_ns_min - 1e-9
            || op.q_pulse_width_ns > profile.pulse_width_ns_max + 1e-9
        {
            return Err(format!(
                "operation {} has frequency or pulse width outside the profile range",
                group.id
            ));
        }
        cut_settings.push(LcCutSetting {
            index,
            name: group.id.clone(),
            speed_mm_s: op.speed_mm_s,
            power_min: op.power_min,
            power_max: op.power_max,
            num_passes: op.passes,
            air_assist: true,
            galvo: LcGalvo {
                mode: if op.mode == "fill" {
                    "Fill".into()
                } else {
                    "Line".into()
                },
                frequency_khz: op.frequency_khz,
                q_pulse_width_ns: op.q_pulse_width_ns,
                global_passes: 1,
                fill: LcFill::new(
                    op.line_interval_mm.unwrap_or(0.1),
                    op.hatch_angle_deg.unwrap_or(45.0),
                    op.angle_increment_deg.unwrap_or(90.0),
                ),
                line: LcLine::new(op.kerf_offset_mm.unwrap_or(0.0)),
                tabs: LcTabs {
                    enabled: false,
                    automatic: true,
                    size_mm: 0.5,
                    spacing_mm: 50.0,
                    tabs_per_shape: 1,
                    skip_inner_shapes: false,
                    cut_power_pct: 0.0,
                },
                wobble: LcWobble {
                    enabled: false,
                    step_mm: 0.05,
                    size_mm: 0.1,
                },
                timing_override: None,
            },
        });
        for ring in &group.rings {
            let points: Vec<LcPoint> = ring
                .iter()
                .map(|p| frame.to_job(*p))
                .map(|p| LcPoint { x: p.x, y: p.y })
                .collect();
            shapes.push(LcShape {
                kind: "Path".into(),
                cut_index: index,
                geometry: LcGeometry {
                    polyline: LcPolyline {
                        points,
                        closed: true,
                    },
                },
            });
        }
    }
    let doc = LcProject {
        app_version: "1".into(),
        shapes,
        cut_settings,
        width_mm,
        height_mm,
    };
    serde_json::to_string_pretty(&doc).map_err(|e| e.to_string())
}

fn validate_profile_operations(
    profile: &LaserGalvoProfile,
    operations: &BTreeSet<String>,
) -> Result<(), String> {
    if profile.kind != "laser_galvo" {
        return Err("laser jobs need a profile whose kind is laser_galvo".into());
    }
    if operations.is_empty() || operations.len() > 6 {
        return Err("laser jobs need one to six operation groups".into());
    }
    for id in operations {
        if !LASER_OPERATION_IDS.contains(&id.as_str()) {
            return Err(format!("unknown laser operation \"{id}\""));
        }
        if !profile.operations.iter().any(|row| row.name == *id) {
            return Err(format!("profile is missing operation \"{id}\""));
        }
    }
    Ok(())
}

struct JobFrame {
    min: Point,
    max: Point,
    margin: f64,
    mirror: bool,
    axis: MirrorAxis,
}

impl JobFrame {
    fn to_job(&self, point: Point) -> Point {
        let mut p = point;
        if self.mirror {
            let cx = (self.min.x + self.max.x) / 2.0;
            let cy = (self.min.y + self.max.y) / 2.0;
            p = match self.axis {
                MirrorAxis::Y => Point::new(2.0 * cx - p.x, p.y),
                MirrorAxis::X => Point::new(p.x, 2.0 * cy - p.y),
            };
        }
        Point::new(
            p.x - self.min.x + self.margin,
            p.y - self.min.y + self.margin,
        )
    }
}

struct OperationGroup {
    id: String,
    rings: Vec<Vec<Point>>,
}

fn collect_groups(
    p: &Project,
    side: Layer,
    profile: &LaserGalvoProfile,
    options: &LaserJobExportOptions,
    _frame: &JobFrame,
) -> Result<Vec<OperationGroup>, String> {
    let mut groups = Vec::new();
    for op_id in LASER_OPERATION_IDS {
        if !options.operations.contains(op_id) {
            continue;
        }
        let rings = match op_id {
            "fiducials" => fiducial_rings(p, side)?,
            "isolation-cut" => {
                pcbcraft_rules::isolation_cut_polygons(p, side, options.isolation_width_mm)?
                    .into_iter()
                    .map(|item| item.ring)
                    .collect()
            }
            "copper-rubout" => {
                if !options.rubout {
                    vec![]
                } else {
                    copper_rubout_rings(p, side, profile)?
                }
            }
            "mask-open" => mask_open_rings(p, side)?,
            "legend" => legend_rings(p, side)?,
            "stencil-cut" => stencil_rings(p, side, options.stencil_reduction_pct)?,
            _ => vec![],
        };
        if rings.is_empty() {
            continue;
        }
        for ring in &rings {
            if !polygon_is_simple(ring) {
                return Err(format!("operation {op_id} emitted a non-simple ring"));
            }
        }
        groups.push(OperationGroup {
            id: op_id.to_string(),
            rings,
        });
    }
    Ok(groups)
}

fn fiducial_rings(p: &Project, side: Layer) -> Result<Vec<Vec<Point>>, String> {
    let mut rings = Vec::new();
    for fid in p.board.fiducials.iter().filter(|f| f.layer == side) {
        rings.push(circle_ring(fid.position, fid.diameter / 2.0));
    }
    Ok(rings)
}

fn copper_rubout_rings(
    p: &Project,
    side: Layer,
    profile: &LaserGalvoProfile,
) -> Result<Vec<Vec<Point>>, String> {
    let row = profile
        .operations
        .iter()
        .find(|op| op.name == "copper-rubout")
        .ok_or_else(|| "profile is missing copper-rubout".to_string())?;
    let interval = row.line_interval_mm.unwrap_or(0.025);
    let angle = row.hatch_angle_deg.unwrap_or(45.0);
    let increment = row.angle_increment_deg.unwrap_or(90.0);
    let _ = (interval, angle, increment);
    let outlines = pcbcraft_rules::laser_copper_outlines(p, side)?;
    Ok(outlines.into_iter().map(|item| item.ring).collect())
}

fn mask_open_rings(p: &Project, side: Layer) -> Result<Vec<Vec<Point>>, String> {
    let mut rings = Vec::new();
    for fp in &p.board.footprints {
        let margin = footprint_mask_margin(p, fp);
        for pad in &fp.pads {
            if !pad_on_layer(pad, side) {
                continue;
            }
            if fp.pad_position(&pad.number).is_none() {
                continue;
            }
            let local: Vec<Point> = pad
                .outline_points()
                .into_iter()
                .map(|pt| fp.position.offset(pt.rotate(-fp.rotation)))
                .collect();
            let ring = hull::octagon(&local, margin.max(0.0));
            if ring.len() >= 3 && polygon_is_simple(&ring) {
                rings.push(ring);
            }
        }
    }
    Ok(rings)
}

fn legend_rings(p: &Project, side: Layer) -> Result<Vec<Vec<Point>>, String> {
    let silk = if side == Layer::Back {
        "B.SilkS"
    } else {
        "F.SilkS"
    };
    let mut rings = Vec::new();
    for text in p.board.texts.iter().filter(|t| t.layer == side) {
        let w = text.text.len() as f64 * text.size * 0.55;
        let h = text.size;
        let ring = rect_ring(text.position, w, h, text.rotation);
        if polygon_is_simple(&ring) {
            rings.push(ring);
        }
    }
    for g in p.graphics.iter().filter(|g| g.layer == silk) {
        if g.points.len() >= 2 {
            for pair in g.points.windows(2) {
                let ring = hull::octagon(pair, g.width.max(0.05) / 2.0);
                if polygon_is_simple(&ring) {
                    rings.push(ring);
                }
            }
        }
    }
    Ok(rings)
}

fn stencil_rings(p: &Project, side: Layer, reduction_pct: f64) -> Result<Vec<Vec<Point>>, String> {
    if !reduction_pct.is_finite() || !(0.0..=50.0).contains(&reduction_pct) {
        return Err("stencil reduction must be 0 to 50 percent".into());
    }
    let scale = 1.0 - reduction_pct / 100.0;
    let mut rings = Vec::new();
    for fp in &p.board.footprints {
        for pad in &fp.pads {
            if !pad_on_layer(pad, side) {
                continue;
            }
            let Some(position) = fp.pad_position(&pad.number) else {
                continue;
            };
            let local: Vec<Point> = pad
                .outline_points()
                .into_iter()
                .map(|pt| Point::new(pt.x * scale, pt.y * scale))
                .collect();
            let mut ring: Vec<Point> = local.iter().map(|pt| position.offset(*pt)).collect();
            if ring.len() >= 3 {
                if let Some(first) = ring.first().copied() {
                    ring.push(first);
                }
                if polygon_is_simple(&ring) {
                    rings.push(ring);
                }
            }
        }
    }
    let (min, max) = board_content_bounds(p);
    let frame = hull::octagon(
        &[
            Point::new(min.x, min.y),
            Point::new(max.x, min.y),
            Point::new(max.x, max.y),
            Point::new(min.x, max.y),
        ],
        0.5,
    );
    if polygon_is_simple(&frame) {
        rings.push(frame);
    }
    Ok(rings)
}

fn footprint_mask_margin(p: &Project, fp: &Footprint) -> f64 {
    p.component(fp.component)
        .and_then(|c| c.library_footprint.as_ref())
        .and_then(|l| l.definition.rules.as_ref())
        .and_then(|r| r.mask_margin)
        .unwrap_or(0.0)
}

fn pad_on_layer(pad: &Pad, layer: Layer) -> bool {
    let Some(g) = pad.geometry.as_ref() else {
        return layer == Layer::Front;
    };
    if g.pad_type == "thru_hole" {
        return true;
    }
    g.layers.iter().any(|name| name == layer.name())
}

fn board_content_bounds(p: &Project) -> (Point, Point) {
    let mut min = Point::new(p.board.width, p.board.height);
    let mut max = Point::default();
    let extend = |min: &mut Point, max: &mut Point, point: Point| {
        if point.x.is_finite() && point.y.is_finite() {
            min.x = min.x.min(point.x);
            min.y = min.y.min(point.y);
            max.x = max.x.max(point.x);
            max.y = max.y.max(point.y);
        }
    };
    for fp in &p.board.footprints {
        for pad in &fp.pads {
            if let Some(pos) = fp.pad_position(&pad.number) {
                extend(&mut min, &mut max, pos);
            }
        }
    }
    for t in &p.board.tracks {
        extend(&mut min, &mut max, t.start);
        extend(&mut min, &mut max, t.end);
    }
    for fid in &p.board.fiducials {
        extend(&mut min, &mut max, fid.position);
    }
    if !min.x.is_finite() {
        return (Point::default(), Point::new(p.board.width, p.board.height));
    }
    (
        Point::new(min.x.min(0.0), min.y.min(0.0)),
        Point::new(max.x.max(p.board.width), max.y.max(p.board.height)),
    )
}

fn circle_ring(center: Point, radius: f64) -> Vec<Point> {
    let segments = 32;
    let mut ring = Vec::with_capacity(segments);
    for i in 0..segments {
        let angle = i as f64 / segments as f64 * std::f64::consts::TAU;
        ring.push(Point::new(
            center.x + radius * angle.cos(),
            center.y + radius * angle.sin(),
        ));
    }
    ring
}

fn rect_ring(anchor: Point, width: f64, height: f64, rotation_deg: f64) -> Vec<Point> {
    let corners = [
        Point::default(),
        Point::new(width, 0.0),
        Point::new(width, height),
        Point::new(0.0, height),
    ];
    corners
        .iter()
        .map(|c| anchor.offset(c.rotate(-rotation_deg)))
        .collect()
}

fn path_for_ring(ring: &[Point], frame: &JobFrame) -> String {
    let mut d = String::from("<path fill=\"none\" stroke=\"#000\" d=\"");
    for (index, point) in ring.iter().map(|p| frame.to_job(*p)).enumerate() {
        if index == 0 {
            d.push_str(&format!("M {:.6} {:.6}", point.x, point.y));
        } else {
            d.push_str(&format!(" L {:.6} {:.6}", point.x, point.y));
        }
    }
    d.push_str(" Z\"/>");
    d
}

#[derive(Serialize)]
struct LcProject {
    app_version: String,
    shapes: Vec<LcShape>,
    cut_settings: Vec<LcCutSetting>,
    width_mm: f64,
    height_mm: f64,
}

#[derive(Serialize)]
struct LcShape {
    kind: String,
    cut_index: usize,
    geometry: LcGeometry,
}

#[derive(Serialize)]
struct LcGeometry {
    #[serde(rename = "Polyline")]
    polyline: LcPolyline,
}

#[derive(Serialize)]
struct LcPolyline {
    points: Vec<LcPoint>,
    closed: bool,
}

#[derive(Serialize)]
struct LcPoint {
    x: f64,
    y: f64,
}

#[derive(Serialize)]
struct LcCutSetting {
    index: usize,
    name: String,
    speed_mm_s: f64,
    power_min: f64,
    power_max: f64,
    num_passes: u32,
    air_assist: bool,
    galvo: LcGalvo,
}

/// Galvo block in the shape LaserCraft's `GalvoSettings` loads: every
/// section is present, so the shop program opens the file without defaults
/// filling in missing parts.
#[derive(Serialize)]
struct LcGalvo {
    mode: String,
    frequency_khz: f64,
    q_pulse_width_ns: f64,
    global_passes: u32,
    fill: LcFill,
    line: LcLine,
    tabs: LcTabs,
    wobble: LcWobble,
    timing_override: Option<()>,
}

#[derive(Serialize)]
struct LcFill {
    line_interval_mm: f64,
    hatch_angle_deg: f64,
    angle_increment_deg: f64,
    auto_rotate: bool,
    dot_width_adjust: bool,
    dot_width_mm: f64,
    ramp_length_mm: f64,
    ramp_outer_edge: bool,
    cross_hatch: bool,
    overscan_mm: f64,
    bidirectional: bool,
    flood_fill: bool,
    dot_mode: bool,
    dot_time_ms: f64,
}

impl LcFill {
    fn new(line_interval_mm: f64, hatch_angle_deg: f64, angle_increment_deg: f64) -> Self {
        Self {
            line_interval_mm,
            hatch_angle_deg,
            angle_increment_deg,
            auto_rotate: false,
            dot_width_adjust: false,
            dot_width_mm: 0.08,
            ramp_length_mm: 0.0,
            ramp_outer_edge: false,
            cross_hatch: false,
            overscan_mm: 0.0,
            bidirectional: true,
            flood_fill: false,
            dot_mode: false,
            dot_time_ms: 1.0,
        }
    }
}

#[derive(Serialize)]
struct LcLine {
    kerf_offset_mm: f64,
    perforation: bool,
    perforation_cut_mm: f64,
    perforation_skip_mm: f64,
    overcut_mm: f64,
    lead_in_mm: f64,
    lead_out_mm: f64,
}

impl LcLine {
    fn new(kerf_offset_mm: f64) -> Self {
        Self {
            kerf_offset_mm,
            perforation: false,
            perforation_cut_mm: 0.1,
            perforation_skip_mm: 0.1,
            overcut_mm: 0.0,
            lead_in_mm: 0.0,
            lead_out_mm: 0.0,
        }
    }
}

#[derive(Serialize)]
struct LcTabs {
    enabled: bool,
    automatic: bool,
    size_mm: f64,
    spacing_mm: f64,
    tabs_per_shape: u32,
    skip_inner_shapes: bool,
    cut_power_pct: f64,
}

#[derive(Serialize)]
struct LcWobble {
    enabled: bool,
    step_mm: f64,
    size_mm: f64,
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use pcbcraft_doc::{Component, Fiducial, Footprint, Id, Net, Pad, Pin, Track};
    use std::fs;

    fn sample_component(id: Id, reference: &str) -> Component {
        Component {
            id,
            reference: reference.into(),
            value: reference.into(),
            kind: "resistor".into(),
            position: Point::default(),
            rotation: 0.0,
            pins: vec![Pin {
                number: "1".into(),
                name: "1".into(),
                offset: Point::default(),
                net: Some(401),
                electrical_type: "passive".into(),
                direction: None,
                unit: 1,
            }],
            units: vec![],
            library_symbol: None,
            library_footprint: None,
            fields: vec![],
            sourcing: Default::default(),
            sheet: 0,
        }
    }

    pub fn laser_job_two_pad_one_track_project() -> Project {
        let mut project = Project {
            name: "laser-fixture".into(),
            ..Project::default()
        };
        project.board.width = 20.0;
        project.board.height = 15.0;
        const FP_A: Id = 101;
        const FP_B: Id = 102;
        const COMP_A: Id = 201;
        const COMP_B: Id = 202;
        const TRACK: Id = 301;
        const NET: Id = 401;
        project.board.footprints = vec![
            Footprint {
                id: FP_A,
                component: COMP_A,
                position: Point::new(5.0, 7.5),
                rotation: 0.0,
                locked: None,
                side: None,
                pads: vec![Pad {
                    number: "1".into(),
                    offset: Point::default(),
                    diameter: 1.6,
                    net: Some(NET),
                    castellated: None,
                    geometry: None,
                }],
            },
            Footprint {
                id: FP_B,
                component: COMP_B,
                position: Point::new(15.0, 7.5),
                rotation: 0.0,
                locked: None,
                side: None,
                pads: vec![Pad {
                    number: "1".into(),
                    offset: Point::default(),
                    diameter: 1.6,
                    net: Some(NET),
                    castellated: None,
                    geometry: None,
                }],
            },
        ];
        project.board.tracks = vec![Track {
            id: TRACK,
            net: NET,
            start: Point::new(5.0, 7.5),
            end: Point::new(15.0, 7.5),
            width: 0.2,
            layer: Layer::Front,
            arc: None,
        }];
        project.components = vec![
            sample_component(COMP_A, "P1"),
            sample_component(COMP_B, "P2"),
        ];
        project.nets = vec![Net {
            id: NET,
            name: "SIG".into(),
            target_length: None,
            length_tolerance: None,
        }];
        project
    }

    fn default_options() -> LaserJobExportOptions {
        LaserJobExportOptions {
            side: Layer::Front,
            operations: BTreeSet::from([
                "fiducials".into(),
                "isolation-cut".into(),
                "copper-rubout".into(),
                "mask-open".into(),
                "legend".into(),
                "stencil-cut".into(),
            ]),
            mirror_axis: MirrorAxis::Y,
            margin_mm: 1.0,
            isolation_width_mm: 0.05,
            rubout: true,
            stencil_reduction_pct: 10.0,
        }
    }

    #[test]
    fn laser_job_svg_two_pad_one_track_matches_golden() -> Result<(), String> {
        let project = laser_job_two_pad_one_track_project();
        project.validate()?;
        let profile = commarker_b6_mopa_profile()?;
        let options = default_options();
        let svg = laser_job_svg(&project, Layer::Front, &profile, &options)?;
        assert!(svg.contains("width=\"22mm\""));
        assert!(svg.contains("height=\"17mm\""));
        assert!(svg.contains("id=\"isolation-cut\""));
        for id in LASER_OPERATION_IDS {
            if options.operations.contains(id) {
                // groups may be omitted when empty
                let _ = id;
            }
        }
        for group_id in ["isolation-cut", "copper-rubout", "mask-open", "stencil-cut"] {
            if !svg.contains(&format!("id=\"{group_id}\"")) {
                return Err(format!("expected group {group_id} in SVG export"));
            }
        }
        for chunk in svg.split("<path ").skip(1) {
            if !chunk.contains(" Z\"") {
                return Err("every exported path must close with Z".into());
            }
        }
        check_golden("laser_job_two_pad_one_track.svg", &svg)?;
        Ok(())
    }

    /// Compare against a committed fixture. Set `PCBCRAFT_BLESS=1` to rewrite
    /// it after a deliberate change to the export.
    fn check_golden(name: &str, actual: &str) -> Result<(), String> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name);
        if std::env::var("PCBCRAFT_BLESS").is_ok() {
            fs::write(&path, actual).map_err(|e| e.to_string())?;
            return Ok(());
        }
        let golden = fs::read_to_string(&path)
            .map_err(|e| format!("golden fixture {name} is missing: {e}"))?;
        if golden != actual {
            return Err(format!(
                "laser export differs from the golden fixture {name}"
            ));
        }
        Ok(())
    }

    #[test]
    fn laser_job_svg_paths_are_closed_simple_rings() -> Result<(), String> {
        let project = laser_job_two_pad_one_track_project();
        let profile = commarker_b6_mopa_profile()?;
        let svg = laser_job_svg(&project, Layer::Front, &profile, &default_options())?;
        let mut rings = 0;
        for chunk in svg.split(" d=\"").skip(1) {
            let data = chunk.split('"').next().unwrap_or_default();
            let nums: Vec<f64> = data
                .split_whitespace()
                .filter_map(|t| t.parse().ok())
                .collect();
            let ring: Vec<Point> = nums
                .as_chunks::<2>()
                .0
                .iter()
                .map(|c| Point::new(c[0], c[1]))
                .collect();
            if ring.len() < 3 || !data.trim_end().ends_with('Z') {
                return Err("path is not a closed ring".into());
            }
            if !pcbcraft_geom::polygon_is_simple(&ring) {
                return Err("exported ring is not simple".into());
            }
            rings += 1;
        }
        if rings == 0 {
            return Err("no rings exported".into());
        }
        Ok(())
    }

    #[test]
    fn laser_job_lcproj_matches_golden_and_has_full_galvo_blocks() -> Result<(), String> {
        let project = laser_job_two_pad_one_track_project();
        let profile = commarker_b6_mopa_profile()?;
        let text = laser_job_lcproj(&project, Layer::Front, &profile, &default_options())?;
        check_golden("laser_job_two_pad_one_track.lcproj", &text)?;
        let value: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        for setting in value["cut_settings"].as_array().into_iter().flatten() {
            let galvo = &setting["galvo"];
            for key in ["fill", "line", "tabs", "wobble"] {
                if !galvo[key].is_object() {
                    return Err(format!("galvo block lacks {key}"));
                }
            }
            if !galvo["timing_override"].is_null() {
                return Err("timing override must be null".into());
            }
        }
        if value["shapes"][0]["geometry"]["Polyline"]["closed"] != true {
            return Err("shapes must carry closed polylines".into());
        }
        Ok(())
    }

    #[test]
    fn laser_job_lcproj_parse_has_cut_setting_per_group() -> Result<(), String> {
        let project = laser_job_two_pad_one_track_project();
        project.validate()?;
        let profile = commarker_b6_mopa_profile()?;
        let options = default_options();
        let text = laser_job_lcproj(&project, Layer::Front, &profile, &options)?;
        let value: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        let settings = value["cut_settings"]
            .as_array()
            .ok_or_else(|| "cut_settings missing".to_string())?;
        let shapes = value["shapes"]
            .as_array()
            .ok_or_else(|| "shapes missing".to_string())?;
        assert!(!settings.is_empty());
        let mut cut_indices = BTreeSet::new();
        for shape in shapes {
            if let Some(index) = shape["cut_index"].as_u64() {
                cut_indices.insert(index);
            }
        }
        assert_eq!(settings.len(), cut_indices.len());
        for shape in shapes {
            let index = shape["cut_index"]
                .as_u64()
                .ok_or_else(|| "shape cut_index missing".to_string())?
                as usize;
            if index >= settings.len() {
                return Err("shape cut_index out of range".into());
            }
        }
        Ok(())
    }

    #[test]
    fn laser_job_lcproj_frequency_and_pulse_width_stay_in_profile_range() -> Result<(), String> {
        let project = laser_job_two_pad_one_track_project();
        let profile = commarker_b6_mopa_profile()?;
        let text = laser_job_lcproj(&project, Layer::Front, &profile, &default_options())?;
        let value: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        for setting in value["cut_settings"].as_array().into_iter().flatten() {
            let freq = setting["galvo"]["frequency_khz"]
                .as_f64()
                .ok_or_else(|| "frequency missing".to_string())?;
            let pulse = setting["galvo"]["q_pulse_width_ns"]
                .as_f64()
                .ok_or_else(|| "pulse width missing".to_string())?;
            if freq < profile.frequency_khz_min || freq > profile.frequency_khz_max {
                return Err("frequency outside profile range".into());
            }
            if pulse < profile.pulse_width_ns_min || pulse > profile.pulse_width_ns_max {
                return Err("pulse width outside profile range".into());
            }
        }
        Ok(())
    }

    #[test]
    fn laser_job_bottom_side_mirrors_fiducials_onto_top() -> Result<(), String> {
        // Board is 20 mm wide: a back mark at x = 17 mirrors about x = 10 onto
        // the front mark at x = 3, so both sides share one registration spot.
        let mut project = laser_job_two_pad_one_track_project();
        project.board.fiducials.push(Fiducial {
            id: 9001,
            position: Point::new(3.0, 4.0),
            diameter: 1.0,
            layer: Layer::Front,
        });
        project.board.fiducials.push(Fiducial {
            id: 9002,
            position: Point::new(17.0, 4.0),
            diameter: 1.0,
            layer: Layer::Back,
        });
        project.validate()?;
        let profile = commarker_b6_mopa_profile()?;
        let mut front_ops = BTreeSet::new();
        front_ops.insert("fiducials".into());
        let front_opts = LaserJobExportOptions {
            operations: front_ops.clone(),
            ..default_options()
        };
        let back_opts = LaserJobExportOptions {
            side: Layer::Back,
            operations: front_ops,
            ..default_options()
        };
        let front = laser_job_svg(&project, Layer::Front, &profile, &front_opts)?;
        let back = laser_job_svg(&project, Layer::Back, &profile, &back_opts)?;
        let front_center = extract_first_fiducial_center(&front)?;
        let back_center = extract_first_fiducial_center(&back)?;
        if (front_center.x - back_center.x).abs() > 1e-3
            || (front_center.y - back_center.y).abs() > 1e-3
        {
            return Err(format!(
                "back fiducial did not mirror onto the top position: front {front_center:?} back {back_center:?}"
            ));
        }
        Ok(())
    }

    fn extract_first_fiducial_center(svg: &str) -> Result<Point, String> {
        let marker = "id=\"fiducials\"";
        let start = svg
            .find(marker)
            .ok_or_else(|| "fiducials group missing".to_string())?;
        let section = &svg[start..];
        let path = section
            .find(" d=\"")
            .ok_or_else(|| "fiducial path missing".to_string())?;
        let data = section[path + 4..].split('"').next().unwrap_or_default();
        let nums: Vec<f64> = data
            .split_whitespace()
            .filter_map(|token| token.parse().ok())
            .collect();
        let count = nums.len() / 2;
        if count < 3 {
            return Err("fiducial path coordinates missing".to_string());
        }
        // The ring starts at a different angle after a mirror, so compare the
        // centre of the ring, not its first point.
        let (mut cx, mut cy) = (0.0, 0.0);
        for pair in nums.as_chunks::<2>().0.iter() {
            cx += pair[0];
            cy += pair[1];
        }
        Ok(Point::new(cx / count as f64, cy / count as f64))
    }

    #[test]
    fn laser_job_reads_the_profile_recorded_on_the_design() -> Result<(), String> {
        let mut project = laser_job_two_pad_one_track_project();
        let mut mine = crate::builtin_machine_profile("commarker-b6-mopa")?;
        mine.id = "my-laser".into();
        for op in &mut mine.laser.operation {
            op.speed_mm_s = 111.0;
        }
        project.machine_profiles.push(mine);
        let resolved = laser_galvo_profile_for_export(&project, "my-laser")?;
        assert!(resolved.operations.iter().all(|o| o.speed_mm_s == 111.0));
        let fallback = laser_galvo_profile_for_export(&project, "commarker-b6-mopa")?;
        assert_eq!(fallback.operations[1].speed_mm_s, 750.0);
        assert!(laser_galvo_profile_for_export(&project, "neoden-yy1").is_err());
        assert!(laser_galvo_profile_for_export(&project, "nope").is_err());
        Ok(())
    }
}
