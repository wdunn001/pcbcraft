//! Isolation milling G-code generation.
use crate::cam_shapes::{LayerShape, layer_shapes};
use pcbcraft_doc::{Layer, Project};
use pcbcraft_geom::{
    OffsetJoin, Point, Ring, hatch, polygon_boundary_distance, polygon_difference,
    polygon_offset, polygon_union, segment_distance,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const LINEARISE_TOL_MM: f64 = 0.01;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// G-code flavours the isolation writer can speak. They differ in the spindle word: Snapmaker takes
/// `M3 P<percent>`, the others `M3 S<rpm>`.
pub enum Dialect {
    /// Plain `M3 S<rpm>` output for GRBL controllers.
    Grbl,
    /// Plain `M3 S<rpm>` output for LinuxCNC; no extra header.
    LinuxCnc,
    /// Plain `M3 S<rpm>` output for Marlin-style controllers without a machine header.
    Marlin,
    /// Spindle on as `M3 P<percent>` and a `; Snapmaker CNC` first comment; the full header is
    /// added later by `snapmaker_finalize`.
    Snapmaker,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
/// How a tool states its spindle speed: raw revolutions per minute, or a percent of the machine
/// maximum.
pub enum Spindle {
    /// Revolutions per minute; written as `S` on every dialect except Snapmaker, which needs a
    /// percent.
    Rpm(u32),
    /// Percent of the machine maximum speed; written as `P` on Snapmaker and scaled to `S`
    /// elsewhere.
    Percent(u8),
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
/// One isolation bit: its size, feeds, how deep it cuts, how many passes, and spindle speed.
pub struct Tool {
    /// Cutting width of the bit at the tip. Isolation rings are offset by half of it.
    pub diameter_mm: f64,
    /// Feed rate while cutting in the XY plane.
    pub feed_mm_min: f64,
    /// Feed rate for the Z moves down into the copper and back up.
    pub plunge_mm_min: f64,
    /// Cut depth below the board top. Must not exceed the board thickness.
    pub depth_mm: f64,
    /// Isolation rings routed around each region; each extra pass steps outward by the overlap
    /// setting.
    pub passes: u8,
    /// Spindle speed for this bit.
    pub spindle: Spindle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Which copper face to isolate. The bottom face is mirrored so the file cuts it from the back.
pub enum Side {
    /// Front copper, cut as drawn.
    Top,
    /// Back copper, mirrored about `MirrorAxis` so the board can be flipped on the bed.
    Bottom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Axis the bottom side is flipped about when the board is turned over on the machine bed.
pub enum MirrorAxis {
    /// Flip left to right: x becomes `2 * centre.x - x`.
    X,
    /// Flip front to back: y becomes `2 * centre.y - y`.
    Y,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Area to clear with a larger bit after isolation: optional rectangle, hatch tool and fill.
pub struct RubRegion {
    /// When `None`, the rubout fills the whole board side up to the board width and height.
    pub rect: Option<(Point, Point)>,
    /// Bit used for the hatch and island contours; written to its own G-code file.
    pub tool: Tool,
    /// Stepover as a percent of the rubout bit width, 30 to 80 inclusive.
    pub stepover_pct: u8,
    /// Hatch line angle in degrees.
    pub angle_deg: f64,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
/// The machine facts the isolation writer needs: dialect, work area corners, safe height and jog
/// speed. Built from a mill profile by `snapmaker_cnc_profile` or the default for tests.
pub struct CncProfile {
    /// Flavour of G-code to write.
    pub dialect: Dialect,
    /// Lower corner of the reachable XY area; any point below it is refused.
    pub work_min: Point,
    /// Upper corner of the reachable XY area; any point above it is refused.
    pub work_max: Point,
    /// Height the bit travels at between cuts.
    pub safe_z_mm: f64,
    /// Feed rate for rapid moves between cuts.
    pub jog_mm_min: f64,
}

impl Default for CncProfile {
    fn default() -> Self {
        Self {
            dialect: Dialect::Grbl,
            work_min: Point::new(0.0, 0.0),
            work_max: Point::new(320.0, 350.0),
            safe_z_mm: 5.0,
            jog_mm_min: 3000.0,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Everything one isolation job needs besides the board and the machine.
pub struct IsolationOptions {
    /// Which copper face to isolate.
    pub side: Side,
    /// Bit, depth, feeds and spindle speed.
    pub tool: Tool,
    /// How much consecutive passes overlap, 0 to 90 percent of the bit width.
    pub overlap_pct: u8,
    /// Optional copper rubout after isolation; uses `RubRegion::tool` in a second file.
    pub rubout: Option<RubRegion>,
    /// Axis used to flip the bottom side; ignored for the top.
    pub mirror_axis: MirrorAxis,
    /// Also write two registration holes outside the board so the two sides line up after the flip.
    pub alignment_holes: bool,
}

#[derive(Clone, Debug)]
/// Collects G-code lines for one file and tracks bounds, travel lengths and a time estimate as
/// moves are added. Rules: only absolute XY moves go through `rapid` and `cut`; Z moves go through
/// `plunge` and `retract`.
pub struct GcodeWriter {
    dialect: Dialect,
    lines: Vec<String>,
    bounds: (Point, Point),
    cut_mm: f64,
    plunge_mm: f64,
    rapid_mm: f64,
    estimate_s: f64,
    at: Option<Point>,
    z: f64,
    feed: f64,
}

impl GcodeWriter {
    /// Starts an empty file for `dialect`, with the tool assumed to sit at `safe_z`.
    pub fn new(dialect: Dialect, safe_z: f64) -> Self {
        Self {
            dialect,
            lines: vec![],
            bounds: (
                Point::new(f64::INFINITY, f64::INFINITY),
                Point::new(f64::NEG_INFINITY, f64::NEG_INFINITY),
            ),
            cut_mm: 0.0,
            plunge_mm: 0.0,
            rapid_mm: 0.0,
            estimate_s: 0.0,
            at: None,
            z: safe_z,
            feed: 0.0,
        }
    }

    fn note(&mut self, p: Point) {
        self.bounds.0.x = self.bounds.0.x.min(p.x);
        self.bounds.0.y = self.bounds.0.y.min(p.y);
        self.bounds.1.x = self.bounds.1.x.max(p.x);
        self.bounds.1.y = self.bounds.1.y.max(p.y);
    }

    fn move_time(&mut self, to: Point, cut: bool) {
        if let Some(from) = self.at {
            let dist = from.distance(to);
            if cut {
                self.cut_mm += dist;
                if self.feed > 0.0 {
                    self.estimate_s += dist / self.feed * 60.0;
                }
            } else if self.feed > 0.0 {
                self.rapid_mm += dist;
                self.estimate_s += dist / self.feed * 60.0;
            }
        }
        self.at = Some(to);
        self.note(to);
    }

    /// Adds a `; text` line. Comments do not affect bounds or the estimate.
    pub fn comment(&mut self, text: &str) {
        self.lines.push(format!("; {text}"));
    }

    /// Adds a `G0` move to `p` at the jog feed. Counts toward the time estimate and the bounds.
    pub fn rapid(&mut self, p: Point, jog: f64) {
        self.feed = jog;
        self.move_time(p, false);
        self.lines.push(format!("G0 X{:.3} Y{:.3}", p.x, p.y));
    }

    /// Adds a `G1` cutting move to `p` at `feed`. Counts toward the time estimate and the bounds.
    pub fn cut(&mut self, p: Point, feed: f64) {
        self.feed = feed;
        self.move_time(p, true);
        self.lines
            .push(format!("G1 X{:.3} Y{:.3} F{:.0}", p.x, p.y, feed));
    }

    /// Adds a `G1 Z` move down to `z` at `rate`. Does nothing before the first XY move.
    pub fn plunge(&mut self, z: f64, rate: f64) {
        if let Some(at) = self.at {
            let dist = (self.z - z).abs();
            self.plunge_mm += dist;
            if rate > 0.0 {
                self.estimate_s += dist / rate * 60.0;
            }
            self.z = z;
            self.lines.push(format!("G1 Z{:.3} F{:.0}", z, rate));
            self.note(at);
        }
    }

    /// Adds a `G1 Z` move up to `safe_z` at `rate`. Does nothing before the first XY move.
    pub fn retract(&mut self, safe_z: f64, rate: f64) {
        if let Some(at) = self.at {
            let dist = (safe_z - self.z).abs();
            self.plunge_mm += dist;
            if rate > 0.0 {
                self.estimate_s += dist / rate * 60.0;
            }
            self.z = safe_z;
            self.lines.push(format!("G1 Z{:.3} F{:.0}", safe_z, rate));
            self.note(at);
        }
    }

    /// Adds the spindle start line for this dialect (`M3 P` on Snapmaker, `M3 S` otherwise).
    pub fn spindle_on(&mut self, spindle: Spindle) {
        let line = match (self.dialect, spindle) {
            (Dialect::Snapmaker, Spindle::Percent(p)) => format!("M3 P{p}"),
            (_, Spindle::Rpm(r)) => format!("M3 S{r}"),
            (_, Spindle::Percent(p)) => format!("M3 S{}", p * 100),
        };
        self.lines.push(line);
    }

    /// Adds `M5`.
    pub fn spindle_off(&mut self) {
        self.lines.push("M5".into());
    }

    /// Joins the lines into the file text, ending with a newline. Contains no timestamps, so equal
    /// input gives equal bytes.
    pub fn finish(self) -> String {
        self.lines.join("\n") + "\n"
    }

    /// Machine time in seconds: cut, rapid and Z travel each divided by their feed.
    pub fn estimate_s(&self) -> f64 {
        self.estimate_s
    }
}

fn decimal(value: f64) -> String {
    let text = format!("{value:.3}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn board_centre(p: &Project) -> Point {
    Point::new(p.board.width / 2.0, p.board.height / 2.0)
}

fn mirror_point(p: Point, axis: MirrorAxis, centre: Point) -> Point {
    match axis {
        MirrorAxis::X => Point::new(2.0 * centre.x - p.x, p.y),
        MirrorAxis::Y => Point::new(p.x, 2.0 * centre.y - p.y),
    }
}

fn transform_point(p: Point, side: Side, axis: MirrorAxis, centre: Point) -> Point {
    if side == Side::Top {
        p
    } else {
        mirror_point(p, axis, centre)
    }
}

fn min_shape_distance(a: &LayerShape, b: &LayerShape) -> f64 {
    if a.net == b.net {
        return f64::INFINITY;
    }
    let edges_a: Vec<(Point, Point)> = pcbcraft_geom::polygon_edges(&a.outline).collect();
    let edges_b: Vec<(Point, Point)> = pcbcraft_geom::polygon_edges(&b.outline).collect();
    let mut best = f64::INFINITY;
    for v in a.outline.iter().copied().chain(b.outline.iter().copied()) {
        best = best.min(polygon_boundary_distance(v, &b.outline));
        best = best.min(polygon_boundary_distance(v, &a.outline));
    }
    for (a0, a1) in &edges_a {
        for (b0, b1) in &edges_b {
            best = best.min(segment_distance(*a0, *a1, *b0, *b1));
        }
    }
    best
}

fn check_net_clearance(shapes: &[LayerShape], tool_d: f64) -> Result<(), String> {
    for i in 0..shapes.len() {
        for j in (i + 1)..shapes.len() {
            let d = min_shape_distance(&shapes[i], &shapes[j]);
            if d < tool_d {
                return Err(format!(
                    "nets {} and {} are {:.3} mm apart, less than the {:.3} mm tool; widen clearance between objects {} and {}",
                    shapes[i].net, shapes[j].net, d, tool_d, shapes[i].id, shapes[j].id
                ));
            }
        }
    }
    Ok(())
}

fn linearise_ring(ring: &Ring) -> Vec<Point> {
    let open = {
        let mut pts: Vec<Point> = ring.to_vec();
        if pts.len() >= 2
            && pts
                .first()
                .zip(pts.last())
                .is_some_and(|(a, b)| a.distance(*b) < 1e-9)
        {
            pts.pop();
        }
        pts
    };
    if open.len() < 2 {
        return vec![];
    }
    let mut path = vec![open[0]];
    for window in open.windows(2) {
        let a = window[0];
        let b = window[1];
        if a.distance(b) <= LINEARISE_TOL_MM {
            path.push(b);
            continue;
        }
        let steps = ((a.distance(b) / LINEARISE_TOL_MM).ceil() as usize).max(1);
        for i in 1..=steps {
            let t = i as f64 / steps as f64;
            path.push(Point::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t));
        }
    }
    path
}

/// Closed rings the bit centre follows: the union of all copper on the chosen side, grown outward
/// by half the bit width, once per pass. Refuses an overlap over 90 percent, and nets closer than
/// the bit can pass.
pub fn isolation_paths(p: &Project, opts: &IsolationOptions) -> Result<Vec<Ring>, String> {
    if opts.overlap_pct > 90 {
        return Err("overlap must be between 0 and 90 percent".into());
    }
    let layer = match opts.side {
        Side::Top => Layer::Front,
        Side::Bottom => Layer::Back,
    };
    let shapes = layer_shapes(p, layer)?;
    check_net_clearance(&shapes, opts.tool.diameter_mm)?;
    let rings: Vec<Ring> = shapes.iter().map(|s| s.outline.clone()).collect();
    let union = polygon_union(&rings);
    if union.is_empty() {
        return Ok(vec![]);
    }
    let radius = opts.tool.diameter_mm / 2.0;
    let step = opts.tool.diameter_mm * (1.0 - opts.overlap_pct as f64 / 100.0);
    let mut paths = Vec::new();
    for pass in 0..opts.tool.passes.max(1) {
        let delta = radius + pass as f64 * step;
        paths.extend(polygon_offset(&union, delta, OffsetJoin::Round));
    }
    Ok(paths)
}

fn board_rect_ring(p: &Project) -> Ring {
    let w = p.board.width;
    let h = p.board.height;
    vec![
        Point::new(0.0, 0.0),
        Point::new(w, 0.0),
        Point::new(w, h),
        Point::new(0.0, h),
        Point::new(0.0, 0.0),
    ]
}

fn normalize_rect(a: Point, b: Point) -> Result<(Point, Point), String> {
    if !a.valid() || !b.valid() {
        return Err("rubout rect coordinates are invalid".into());
    }
    let (lo_x, hi_x) = if a.x <= b.x { (a.x, b.x) } else { (b.x, a.x) };
    let (lo_y, hi_y) = if a.y <= b.y { (a.y, b.y) } else { (b.y, a.y) };
    if hi_x - lo_x < 1e-6 || hi_y - lo_y < 1e-6 {
        return Err("rubout rect must have positive width and height".into());
    }
    Ok((Point::new(lo_x, lo_y), Point::new(hi_x, hi_y)))
}

fn rect_to_ring(min: Point, max: Point) -> Ring {
    vec![
        Point::new(min.x, min.y),
        Point::new(max.x, min.y),
        Point::new(max.x, max.y),
        Point::new(min.x, max.y),
        Point::new(min.x, min.y),
    ]
}

fn join_hatch_zigzag(segments: &[(Point, Point)], stepover: f64) -> Vec<Vec<Point>> {
    if segments.is_empty() {
        return vec![];
    }
    let join_dist = stepover * 1.01 + 1e-6;
    let mut used = vec![false; segments.len()];
    let mut paths = Vec::new();
    for start in 0..segments.len() {
        if used[start] {
            continue;
        }
        used[start] = true;
        let (a, b) = segments[start];
        let mut path = vec![a, b];
        loop {
            let Some(tail) = path.last().copied() else {
                break;
            };
            let mut extended = false;
            for i in 0..segments.len() {
                if used[i] {
                    continue;
                }
                let (c, d) = segments[i];
                if tail.distance(c) <= join_dist {
                    used[i] = true;
                    path.push(c);
                    path.push(d);
                    extended = true;
                    break;
                }
                if tail.distance(d) <= join_dist {
                    used[i] = true;
                    path.push(d);
                    path.push(c);
                    extended = true;
                    break;
                }
            }
            if !extended {
                break;
            }
        }
        paths.push(path);
    }
    paths
}

/// Hatch and contour paths that clear unused copper inside `RubRegion` without crossing `keep`
/// isolation rings.
pub fn rubout_paths(
    p: &Project,
    side: Side,
    region: &RubRegion,
    keep: &[Ring],
    isolation_tool_d_mm: f64,
) -> Result<Vec<Vec<Point>>, String> {
    if region.stepover_pct < 30 || region.stepover_pct > 80 {
        return Err("rubout stepover_pct must be between 30 and 80".into());
    }
    if region.tool.diameter_mm < isolation_tool_d_mm {
        return Err(format!(
            "rubout tool diameter {:.3} mm is smaller than the isolation tool {:.3} mm",
            region.tool.diameter_mm, isolation_tool_d_mm
        ));
    }
    if region.tool.diameter_mm <= 0.0
        || region.tool.feed_mm_min <= 0.0
        || region.tool.plunge_mm_min <= 0.0
    {
        return Err("rubout tool diameter, feed and plunge must be positive".into());
    }
    if !region.angle_deg.is_finite() {
        return Err("rubout angle_deg must be finite".into());
    }
    let layer = match side {
        Side::Top => Layer::Front,
        Side::Bottom => Layer::Back,
    };
    let shapes = layer_shapes(p, layer)?;
    let copper: Vec<Ring> = shapes.iter().map(|s| s.outline.clone()).collect();
    let rect_ring = match region.rect {
        Some((a, b)) => {
            let (lo, hi) = normalize_rect(a, b)?;
            rect_to_ring(lo, hi)
        }
        None => board_rect_ring(p),
    };
    let clearance = region.tool.diameter_mm / 2.0;
    let grown = polygon_offset(&polygon_union(&copper), clearance, OffsetJoin::Round);
    let free = polygon_difference(&[rect_ring], &grown);
    if free.is_empty() {
        return Ok(vec![]);
    }
    let stepover = region.tool.diameter_mm * f64::from(region.stepover_pct) / 100.0;
    let segments = hatch(&free, stepover, region.angle_deg);
    let mut paths = join_hatch_zigzag(&segments, stepover);
    for ring in &free {
        let contour = linearise_ring(ring);
        if contour.len() >= 3 {
            paths.push(contour);
        }
    }
    for path in &paths {
        for window in path.windows(2) {
            let a = window[0];
            let b = window[1];
            for ring in keep {
                for (c, d) in pcbcraft_geom::polygon_edges(ring) {
                    if segment_distance(a, b, c, d) < 1e-6 {
                        return Err("rubout path crosses an isolation ring".into());
                    }
                }
            }
        }
    }
    Ok(paths)
}

fn rubout_side_suffix(side: Side) -> &'static str {
    match side {
        Side::Top => "top",
        Side::Bottom => "bottom",
    }
}

fn rubout_file_name(side: Side, diameter_mm: f64) -> String {
    format!(
        "rubout-{}-{}.nc",
        decimal(diameter_mm),
        rubout_side_suffix(side)
    )
}

fn write_polylines(
    writer: &mut GcodeWriter,
    paths: &[Vec<Point>],
    tool: &Tool,
    profile: &CncProfile,
    side: Side,
    axis: MirrorAxis,
    centre: Point,
) -> Result<(), String> {
    let safe = profile.safe_z_mm;
    for path in paths {
        if path.len() < 2 {
            continue;
        }
        let mut pts: Vec<Point> = path.to_vec();
        for pt in &mut pts {
            *pt = transform_point(*pt, side, axis, centre);
        }
        check_bounds(&pts, profile)?;
        writer.rapid(pts[0], profile.jog_mm_min);
        writer.plunge(-tool.depth_mm, tool.plunge_mm_min);
        for pt in pts.iter().skip(1) {
            writer.cut(*pt, tool.feed_mm_min);
        }
        writer.retract(safe, tool.plunge_mm_min);
    }
    Ok(())
}

fn in_work_area(p: Point, profile: &CncProfile) -> bool {
    p.x >= profile.work_min.x
        && p.y >= profile.work_min.y
        && p.x <= profile.work_max.x
        && p.y <= profile.work_max.y
}

fn check_bounds(points: &[Point], profile: &CncProfile) -> Result<(), String> {
    for p in points {
        if !p.valid() || !in_work_area(*p, profile) {
            return Err(format!(
                "coordinate ({:.3}, {:.3}) is outside the machine work area",
                p.x, p.y
            ));
        }
    }
    Ok(())
}

fn header(profile: &CncProfile, estimate_s: f64, tool: &Tool) -> Vec<String> {
    let mut lines = vec![
        "G21".into(),
        "G90".into(),
        "G17".into(),
        format!("; estimate_s={:.1}", estimate_s),
        format!(
            "; tool_d={} feed={} plunge={} depth={} passes={}",
            decimal(tool.diameter_mm),
            decimal(tool.feed_mm_min),
            decimal(tool.plunge_mm_min),
            decimal(tool.depth_mm),
            tool.passes
        ),
    ];
    if profile.dialect == Dialect::Snapmaker {
        lines.insert(0, "; Snapmaker CNC".into());
    }
    lines
}

fn write_paths(
    writer: &mut GcodeWriter,
    paths: &[Ring],
    tool: &Tool,
    profile: &CncProfile,
    side: Side,
    axis: MirrorAxis,
    centre: Point,
) -> Result<(), String> {
    let safe = profile.safe_z_mm;
    for ring in paths {
        let mut path = linearise_ring(ring);
        if path.is_empty() {
            continue;
        }
        for pt in &mut path {
            *pt = transform_point(*pt, side, axis, centre);
        }
        check_bounds(&path, profile)?;
        writer.rapid(path[0], profile.jog_mm_min);
        writer.plunge(-tool.depth_mm, tool.plunge_mm_min);
        for pt in path.iter().skip(1) {
            writer.cut(*pt, tool.feed_mm_min);
        }
        writer.retract(safe, tool.plunge_mm_min);
    }
    Ok(())
}

/// What an isolation job returns: the named files, the time estimate and, if asked for, the two
/// registration holes.
pub struct IsolationGcodeResult {
    /// File name and text pairs; one entry for the generic writer.
    pub files: Vec<(String, String)>,
    /// Estimated machine time in seconds for all files.
    pub estimate_s: f64,
    /// Positions of the two registration holes when `alignment_holes` was set.
    pub alignment: Option<[Point; 2]>,
}

/// Writes the isolation G-code for one copper side. Refuses a cut deeper than the board, a
/// non-positive diameter, feed or plunge, and any point outside the profile work area; the reason
/// names the limit.
pub fn isolation_gcode(
    p: &Project,
    opts: &IsolationOptions,
    profile: &CncProfile,
) -> Result<IsolationGcodeResult, String> {
    p.validate()?;
    if opts.tool.depth_mm > p.board.thickness {
        return Err(format!(
            "cut depth {:.3} mm exceeds board thickness {:.3} mm",
            opts.tool.depth_mm, p.board.thickness
        ));
    }
    if opts.tool.diameter_mm <= 0.0
        || opts.tool.feed_mm_min <= 0.0
        || opts.tool.plunge_mm_min <= 0.0
    {
        return Err("tool diameter, feed and plunge must be positive".into());
    }
    if let Some(rub) = &opts.rubout {
        if rub.tool.depth_mm > p.board.thickness {
            return Err(format!(
                "rubout cut depth {:.3} mm exceeds board thickness {:.3} mm",
                rub.tool.depth_mm, p.board.thickness
            ));
        }
        if rub.tool.diameter_mm < opts.tool.diameter_mm {
            return Err(format!(
                "rubout tool diameter {:.3} mm is smaller than the isolation tool {:.3} mm",
                rub.tool.diameter_mm, opts.tool.diameter_mm
            ));
        }
    }
    let paths = isolation_paths(p, opts)?;
    let centre = board_centre(p);
    let mut writer = GcodeWriter::new(profile.dialect, profile.safe_z_mm);
    for line in header(profile, 0.0, &opts.tool) {
        writer.lines.push(line);
    }
    writer.spindle_on(opts.tool.spindle);
    write_paths(
        &mut writer,
        &paths,
        &opts.tool,
        profile,
        opts.side,
        opts.mirror_axis,
        centre,
    )?;
    writer.spindle_off();
    writer.lines.push("M2".into());
    let mut estimate = writer.estimate_s();
    if let Some(first) = writer
        .lines
        .iter_mut()
        .find(|l| l.starts_with("; estimate_s="))
    {
        *first = format!("; estimate_s={:.1}", estimate);
    }
    let body = writer.finish();
    let name = match opts.side {
        Side::Top => "isolation-top.nc".into(),
        Side::Bottom => "isolation-bottom.nc".into(),
    };
    let mut files = vec![(name, body)];
    if let Some(rub) = &opts.rubout {
        let rub_polylines = rubout_paths(p, opts.side, rub, &paths, opts.tool.diameter_mm)?;
        let mut rub_writer = GcodeWriter::new(profile.dialect, profile.safe_z_mm);
        for line in header(profile, 0.0, &rub.tool) {
            rub_writer.lines.push(line);
        }
        rub_writer.spindle_on(rub.tool.spindle);
        write_polylines(
            &mut rub_writer,
            &rub_polylines,
            &rub.tool,
            profile,
            opts.side,
            opts.mirror_axis,
            centre,
        )?;
        rub_writer.spindle_off();
        rub_writer.lines.push("M2".into());
        let rub_estimate_s = rub_writer.estimate_s();
        estimate += rub_estimate_s;
        if let Some(first) = rub_writer
            .lines
            .iter_mut()
            .find(|l| l.starts_with("; estimate_s="))
        {
            *first = format!("; estimate_s={:.1}", rub_estimate_s);
        }
        files.push((
            rubout_file_name(opts.side, rub.tool.diameter_mm),
            rub_writer.finish(),
        ));
    }
    let mut alignment = None;
    if opts.side == Side::Bottom && opts.alignment_holes {
        let outline = p
            .board
            .outline
            .as_ref()
            .ok_or("alignment holes need a board outline".to_string())?;
        let mut min_x = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        for pt in outline {
            min_x = min_x.min(pt.x);
            max_x = max_x.max(pt.x);
        }
        let y = match opts.mirror_axis {
            MirrorAxis::X => centre.y,
            MirrorAxis::Y => outline.iter().map(|p| p.y).sum::<f64>() / outline.len() as f64,
        };
        let holes = [
            transform_point(
                Point::new(min_x - 5.0, y),
                opts.side,
                opts.mirror_axis,
                centre,
            ),
            transform_point(
                Point::new(max_x + 5.0, y),
                opts.side,
                opts.mirror_axis,
                centre,
            ),
        ];
        check_bounds(&holes, profile)?;
        alignment = Some([holes[0], holes[1]]);
    }
    Ok(IsolationGcodeResult {
        files,
        estimate_s: estimate,
        alignment,
    })
}

/// Reads isolation job options from the same JSON shape as the `cam.isolation` command.
pub fn isolation_options_from_value(params: &Value) -> Result<IsolationOptions, String> {
    let side = match params.get("side").and_then(Value::as_str) {
        Some("top") => Side::Top,
        Some("bottom") => Side::Bottom,
        _ => return Err("side must be top or bottom".into()),
    };
    let tool_val = params
        .get("tool")
        .ok_or_else(|| "missing tool".to_string())?;
    let diameter_mm = tool_val
        .get("diameter_mm")
        .and_then(Value::as_f64)
        .filter(|v| v.is_finite() && *v > 0.0)
        .ok_or_else(|| "tool.diameter_mm must be positive".to_string())?;
    let feed_mm_min = tool_val
        .get("feed_mm_min")
        .and_then(Value::as_f64)
        .filter(|v| v.is_finite() && *v > 0.0)
        .ok_or_else(|| "tool.feed_mm_min must be positive".to_string())?;
    let plunge_mm_min = tool_val
        .get("plunge_mm_min")
        .and_then(Value::as_f64)
        .filter(|v| v.is_finite() && *v > 0.0)
        .ok_or_else(|| "tool.plunge_mm_min must be positive".to_string())?;
    let depth_mm = tool_val
        .get("depth_mm")
        .and_then(Value::as_f64)
        .filter(|v| v.is_finite() && *v > 0.0)
        .ok_or_else(|| "tool.depth_mm must be positive".to_string())?;
    let passes = tool_val
        .get("passes")
        .and_then(Value::as_u64)
        .filter(|v| (1..=8).contains(v))
        .ok_or_else(|| "tool.passes must be 1 to 8".to_string())? as u8;
    let spindle_rpm = tool_val
        .get("spindle_rpm")
        .and_then(Value::as_u64)
        .filter(|v| *v > 0)
        .ok_or_else(|| "tool.spindle_rpm must be positive".to_string())?
        as u32;
    let overlap_pct = params
        .get("overlap_pct")
        .and_then(Value::as_u64)
        .unwrap_or(50) as u8;
    if overlap_pct > 90 {
        return Err("overlap must be between 0 and 90 percent".into());
    }
    let mirror_axis = match params
        .get("mirror_axis")
        .and_then(Value::as_str)
        .unwrap_or("y")
    {
        "x" => MirrorAxis::X,
        "y" => MirrorAxis::Y,
        _ => return Err("mirror_axis must be x or y".into()),
    };
    let alignment_holes = params
        .get("alignment_holes")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    Ok(IsolationOptions {
        side,
        tool: Tool {
            diameter_mm,
            feed_mm_min,
            plunge_mm_min,
            depth_mm,
            passes,
            spindle: Spindle::Rpm(spindle_rpm),
        },
        overlap_pct,
        rubout: None,
        mirror_axis,
        alignment_holes,
    })
}

/// Linearised tool centre paths in board space, mirrored when isolating the bottom side.
pub fn isolation_toolpaths(
    p: &Project,
    opts: &IsolationOptions,
) -> Result<Vec<Vec<Point>>, String> {
    let paths = isolation_paths(p, opts)?;
    let centre = board_centre(p);
    let mut out = Vec::new();
    for ring in paths {
        let mut path = linearise_ring(&ring);
        if path.is_empty() {
            continue;
        }
        for pt in &mut path {
            *pt = transform_point(*pt, opts.side, opts.mirror_axis, centre);
        }
        out.push(path);
    }
    Ok(out)
}
