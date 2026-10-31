//! CNC mill G-code for drilling and board outline profiling.
use pcbcraft_doc::Project;
use pcbcraft_geom::{Point, polygon_area, polygon_edges};
use std::f64::consts::PI;

const MAX_TOOLS: usize = 64;
const MAX_HITS: usize = 50_000;
const LINEARIZE_STEP_MM: f64 = 0.01;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// G-code flavours the drill and outline writers speak. Snapmaker output is produced by running the
/// result through `snapmaker_finalize`.
pub enum Dialect {
    /// GRBL controllers.
    Grbl,
    /// LinuxCNC controllers.
    LinuxCnc,
    /// Marlin-style controllers.
    Marlin,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// How the job handles a change of bit between drill sizes.
pub enum ToolChange {
    /// Write one file per bit, for machines with no tool change command such as the Snapmaker.
    NewFile,
    /// One file; announce each bit change with an `M6` line.
    M6Message,
    /// One file; stop with `M0` so the operator swaps the bit.
    PauseM0,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Order holes are visited within one bit.
pub enum DrillOrder {
    /// Always go to the closest unvisited hole next; shortest travel in most layouts.
    Nearest,
    /// Sweep row by row; predictable motion, longer travel.
    Scan,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Which side of the board edge the bit centre follows.
pub enum OutlineCompensation {
    /// Bit centre one radius outside the edge, so the board keeps its drawn size.
    Outside,
    /// Bit centre one radius inside the edge, for cutting a hole or pocket.
    Inside,
    /// Bit centre on the drawn edge.
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Whether the bit turns with or against the feed direction.
pub enum CutDirection {
    /// Bit rotation matches the feed, giving a cleaner edge on most boards.
    Climb,
    /// Bit rotation opposes the feed.
    Conventional,
}

#[derive(Clone, Debug, PartialEq)]
/// One drill or router bit with the speeds the job may use it at.
pub struct Tool {
    /// Position in the bit list, from 1; names the output file and the `T` word.
    pub number: u8,
    /// Bit diameter, 0.05 to 25 mm.
    pub diameter_mm: f64,
    /// Spindle speed in revolutions per minute, 100 to 60000.
    pub spindle_rpm: u32,
    /// Feed in the XY plane while cutting, 1 to 10000.
    pub cut_feed_mm_min: f64,
    /// Feed for Z moves into the board, 1 to 5000.
    pub plunge_feed_mm_min: f64,
}

impl Tool {
    /// True when every field is finite and inside the bounds on its own doc line.
    pub fn valid(&self) -> bool {
        self.number >= 1
            && self.diameter_mm.is_finite()
            && (0.05..=25.0).contains(&self.diameter_mm)
            && self.spindle_rpm >= 100
            && self.spindle_rpm <= 60_000
            && self.cut_feed_mm_min.is_finite()
            && (1.0..=10_000.0).contains(&self.cut_feed_mm_min)
            && self.plunge_feed_mm_min.is_finite()
            && (1.0..=5_000.0).contains(&self.plunge_feed_mm_min)
    }
}

#[derive(Clone, Debug, PartialEq)]
/// Which bits are on hand and how to run the drilling.
pub struct DrillOptions {
    /// Bits available, 1 to 64. Each hole uses the smallest bit that is large enough.
    pub tools: Vec<Tool>,
    /// Depth per peck when drilling in steps, 0.2 to 5 mm; `None` drills in one plunge.
    pub peck_mm: Option<f64>,
    /// Visiting order within one bit.
    pub order: DrillOrder,
    /// Extra registration hole positions to drill with the smallest bit.
    pub alignment_holes: Vec<Point>,
    /// How a change of bit is expressed.
    pub tool_change: ToolChange,
}

impl DrillOptions {
    /// True when there are 1 to 64 valid bits, the peck is in range and every alignment point is
    /// finite.
    pub fn valid(&self) -> bool {
        !self.tools.is_empty()
            && self.tools.len() <= MAX_TOOLS
            && self.tools.iter().all(Tool::valid)
            && self
                .peck_mm
                .is_none_or(|p| p.is_finite() && (0.2..=5.0).contains(&p))
            && self.alignment_holes.iter().all(|p| p.valid())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
/// Small bridges left uncut in the outline so the board stays attached to the stock until the end.
pub struct Tabs {
    /// Number of tabs, 0 to 16, spaced evenly around the outline.
    pub count: u8,
    /// Length of each tab along the edge, 1 to 20 mm.
    pub width_mm: f64,
    /// Material left standing under each tab, 0.2 mm up to the board thickness.
    pub height_mm: f64,
}

impl Tabs {
    /// True when count, width and height are in range and the height does not exceed `thickness`.
    pub fn valid_for_thickness(&self, thickness: f64) -> bool {
        (0..=16).contains(&self.count)
            && self.width_mm.is_finite()
            && (1.0..=20.0).contains(&self.width_mm)
            && self.height_mm.is_finite()
            && (0.2..=thickness).contains(&self.height_mm)
    }
}

#[derive(Clone, Debug, PartialEq)]
/// Everything the outline cut needs besides the board and the machine.
pub struct OutlineOptions {
    /// Router bit for the cut.
    pub tool: Tool,
    /// Depth removed per pass, 0.1 to 5 mm; the board is cut in `ceil(thickness / pass_depth)`
    /// passes.
    pub pass_depth_mm: f64,
    /// Tab layout for the passes above the final depth.
    pub tabs: Tabs,
    /// Which side of the edge the bit centre follows.
    pub compensation: OutlineCompensation,
    /// Also cut the interior board cutouts, not just the outer edge.
    pub cutouts: bool,
}

impl OutlineOptions {
    /// True when the bit, pass depth and tabs are valid for a board of `thickness`.
    pub fn valid(&self, thickness: f64) -> bool {
        self.tool.valid()
            && self.pass_depth_mm.is_finite()
            && (0.1..=5.0).contains(&self.pass_depth_mm)
            && self.tabs.valid_for_thickness(thickness)
    }
}

#[derive(Clone, Debug, PartialEq)]
/// Machine facts for drill and outline output: reach, heights, cut direction and plating allowance.
pub struct CamProfile {
    /// Flavour of G-code to write.
    pub dialect: Dialect,
    /// Reach in X, 10 to 2000 mm. Any point beyond it is refused.
    pub work_x_mm: f64,
    /// Reach in Y, 10 to 2000 mm. Any point beyond it is refused.
    pub work_y_mm: f64,
    /// Height for travel between holes; must be above `retract_z_mm`.
    pub safe_z_mm: f64,
    /// Height the bit lifts to between pecks.
    pub retract_z_mm: f64,
    /// Climb or conventional cutting.
    pub direction: CutDirection,
    /// Amount added to a plated hole diameter so the finished hole is not undersized, 0 to 0.5 mm.
    pub plating_allowance_mm: f64,
    /// Whether plated holes are drilled at all; false treats every hole as unplated.
    pub fab_plated: bool,
}

impl CamProfile {
    /// True when reach, heights and plating allowance are in range and `safe_z_mm` is above
    /// `retract_z_mm`.
    pub fn valid(&self) -> bool {
        self.work_x_mm.is_finite()
            && self.work_y_mm.is_finite()
            && (10.0..=2_000.0).contains(&self.work_x_mm)
            && (10.0..=2_000.0).contains(&self.work_y_mm)
            && self.safe_z_mm.is_finite()
            && self.retract_z_mm.is_finite()
            && self.safe_z_mm > self.retract_z_mm
            && self.plating_allowance_mm.is_finite()
            && (0.0..=0.5).contains(&self.plating_allowance_mm)
    }
}

#[derive(Clone, Debug, PartialEq)]
/// One hole to drill: centre, size, optional slot and whether it is plated.
pub struct DrillHit {
    /// Hole centre x on the board.
    pub x: f64,
    /// Hole centre y on the board.
    pub y: f64,
    /// Finished hole diameter.
    pub diameter_mm: f64,
    /// Slot length; 0 for a round hole.
    pub slot_mm: f64,
    /// x part of the unit vector along the slot.
    pub slot_ux: f64,
    /// y part of the unit vector along the slot.
    pub slot_uy: f64,
    /// True when the hole is plated through.
    pub plated: bool,
}

#[derive(Clone, Debug, PartialEq)]
/// All holes sharing one diameter, slot length and plating, which one bit can drill together.
pub struct DrillTool {
    /// Hole diameter shared by the group.
    pub diameter_mm: f64,
    /// Slot length shared by the group; 0 for round holes.
    pub slot_mm: f64,
    /// Plating shared by the group.
    pub plated: bool,
    /// The holes in the group.
    pub hits: Vec<DrillHit>,
}

/// Groups every hole on the board (vias, pad drills, mounting holes) by diameter, slot and plating,
/// smallest first. Refuses a board with more than 50000 holes.
pub fn drill_tools(p: &Project) -> Result<Vec<DrillTool>, String> {
    p.validate()?;
    let hits = collect_drill_hits(p)?;
    if hits.len() > MAX_HITS {
        return Err("drill exceeds the drill hit limit".into());
    }
    let mut groups: Vec<DrillTool> = Vec::new();
    'outer: for hit in hits {
        for group in &mut groups {
            if (group.diameter_mm - hit.diameter_mm).abs() < 1e-9
                && (group.slot_mm - hit.slot_mm).abs() < 1e-9
                && group.plated == hit.plated
            {
                group.hits.push(hit);
                continue 'outer;
            }
        }
        groups.push(DrillTool {
            diameter_mm: hit.diameter_mm,
            slot_mm: hit.slot_mm,
            plated: hit.plated,
            hits: vec![hit],
        });
    }
    groups.sort_by(|a, b| {
        a.diameter_mm
            .partial_cmp(&b.diameter_mm)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                a.slot_mm
                    .partial_cmp(&b.slot_mm)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| a.plated.cmp(&b.plated))
    });
    if groups.len() > MAX_TOOLS {
        return Err("drill needs 64 or fewer distinct drill tools".into());
    }
    Ok(groups)
}

fn collect_drill_hits(p: &Project) -> Result<Vec<DrillHit>, String> {
    let mut hits = Vec::new();
    for v in &p.board.vias {
        if v.drill > 0.0 {
            hits.push(DrillHit {
                x: v.position.x,
                y: v.position.y,
                diameter_mm: v.drill,
                slot_mm: 0.0,
                slot_ux: 0.0,
                slot_uy: 0.0,
                plated: true,
            });
        }
    }
    for h in &p.board.holes {
        hits.push(DrillHit {
            x: h.position.x,
            y: h.position.y,
            diameter_mm: h.diameter,
            slot_mm: 0.0,
            slot_ux: 0.0,
            slot_uy: 0.0,
            plated: false,
        });
    }
    for f in &p.board.footprints {
        for pad in &f.pads {
            let Some(g) = pad.geometry.as_ref() else {
                continue;
            };
            let Some(drill) = g.drill.as_ref() else {
                continue;
            };
            let (wide, narrow) = (
                drill.size.x.max(drill.size.y),
                drill.size.x.min(drill.size.y),
            );
            if narrow <= 0.0 {
                continue;
            }
            let span = wide - narrow;
            let local = pad.offset.offset(drill.offset.rotate(-g.rotation));
            let world = f.position.offset(local.rotate(-f.rotation));
            let (ux, uy) = if span < 1e-9 {
                (0.0, 0.0)
            } else {
                let axis = if drill.size.x >= drill.size.y {
                    Point::new(span / 2.0, 0.0)
                } else {
                    Point::new(0.0, span / 2.0)
                };
                let dir = axis.rotate(-(g.rotation + f.rotation));
                (dir.x, dir.y)
            };
            hits.push(DrillHit {
                x: world.x,
                y: world.y,
                diameter_mm: narrow,
                slot_mm: span,
                slot_ux: ux,
                slot_uy: uy,
                plated: g.pad_type == "thru_hole",
            });
        }
    }
    Ok(hits)
}

fn effective_drill_diameter(hit: &DrillHit, profile: &CamProfile) -> f64 {
    if profile.fab_plated && hit.plated {
        (hit.diameter_mm - profile.plating_allowance_mm).max(0.05)
    } else {
        hit.diameter_mm
    }
}

fn pick_tool(diameter_mm: f64, tools: &[Tool]) -> Option<&Tool> {
    tools
        .iter()
        .filter(|t| t.diameter_mm + 1e-9 >= diameter_mm)
        .min_by(|a, b| {
            a.diameter_mm
                .partial_cmp(&b.diameter_mm)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
}

struct GcodeWriter {
    body: String,
    dialect: Dialect,
}

impl GcodeWriter {
    fn new(dialect: Dialect) -> Self {
        Self {
            body: String::new(),
            dialect,
        }
    }

    fn comment(&mut self, text: &str) {
        self.body.push('(');
        self.body.push_str(text);
        self.body.push_str(")\n");
    }

    fn line(&mut self, text: &str) {
        self.body.push_str(text);
        self.body.push('\n');
    }

    fn fmt(v: f64) -> String {
        format!("{v:.3}")
    }

    fn write_header(&mut self, kind: &str) {
        self.comment(&format!("PCBCraft {kind}"));
        self.line("G21");
        self.line("G90");
    }

    fn write_footer(&mut self) {
        self.line("M5");
        self.line("G0 Z5.000");
        self.line("M2");
    }

    fn spindle_on(&mut self, rpm: u32) {
        match self.dialect {
            Dialect::Marlin => self.line(&format!("M3 P{}", rpm)),
            _ => self.line(&format!("M3 S{rpm}")),
        }
    }

    fn rapid_xy(&mut self, x: f64, y: f64) {
        self.line(&format!("G0 X{} Y{}", Self::fmt(x), Self::fmt(y)));
    }

    fn rapid_z(&mut self, z: f64) {
        self.line(&format!("G0 Z{}", Self::fmt(z)));
    }

    fn linear_xyz(&mut self, x: f64, y: f64, z: f64, feed: f64) {
        self.line(&format!(
            "G1 X{} Y{} Z{} F{}",
            Self::fmt(x),
            Self::fmt(y),
            Self::fmt(z),
            Self::fmt(feed)
        ));
    }

    fn linear_xy(&mut self, x: f64, y: f64, feed: f64) {
        self.line(&format!(
            "G1 X{} Y{} F{}",
            Self::fmt(x),
            Self::fmt(y),
            Self::fmt(feed)
        ));
    }

    fn linear_z(&mut self, z: f64, feed: f64) {
        self.line(&format!("G1 Z{} F{}", Self::fmt(z), Self::fmt(feed)));
    }

    fn tool_pause(&mut self, next: &Tool) {
        match self.dialect {
            Dialect::Grbl | Dialect::LinuxCnc => {
                self.line("M5");
                self.rapid_z(5.0);
                self.line(&format!("M6 T{}", next.number));
                self.comment(&format!("change to {:.3} mm drill", next.diameter_mm));
            }
            Dialect::Marlin => {
                self.line("M5");
                self.rapid_z(5.0);
                self.line("M0");
                self.comment(&format!("load {:.3} mm drill", next.diameter_mm));
            }
        }
    }

    fn into_string(self) -> String {
        self.body
    }
}

fn order_hits(order: DrillOrder, hits: &mut [DrillHit]) {
    match order {
        DrillOrder::Scan => {
            hits.sort_by(|a, b| {
                a.x.partial_cmp(&b.x)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal))
            });
        }
        DrillOrder::Nearest => {
            let mut remaining: Vec<DrillHit> = hits.to_vec();
            let mut current = Point::default();
            let mut ordered = Vec::with_capacity(remaining.len());
            while !remaining.is_empty() {
                let best = remaining
                    .iter()
                    .enumerate()
                    .min_by(|(_, a), (_, b)| {
                        let da = current.distance(Point::new(a.x, a.y));
                        let db = current.distance(Point::new(b.x, b.y));
                        da.partial_cmp(&db)
                            .unwrap_or(std::cmp::Ordering::Equal)
                            .then_with(|| {
                                a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal)
                            })
                            .then_with(|| {
                                a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal)
                            })
                    })
                    .map(|(i, _)| i)
                    .unwrap_or(0);
                let hit = remaining.remove(best);
                current = Point::new(hit.x, hit.y);
                ordered.push(hit);
            }
            let len = hits.len();
            if len == ordered.len() {
                hits.clone_from_slice(&ordered);
            }
        }
    }
}

fn transform_hit(p: &Project, hit: &DrillHit) -> (f64, f64) {
    (
        hit.x - p.board.drill_origin.x,
        hit.y - p.board.drill_origin.y,
    )
}

#[derive(Clone, Copy)]
struct DrillCut {
    depth: f64,
    peck: Option<f64>,
    plunge: f64,
    cut: f64,
}

fn drill_one_hit(
    w: &mut GcodeWriter,
    p: &Project,
    hit: &DrillHit,
    profile: &CamProfile,
    cut: &DrillCut,
) {
    let DrillCut {
        depth,
        peck,
        plunge,
        cut,
    } = *cut;
    let (x, y) = transform_hit(p, hit);
    w.rapid_z(profile.safe_z_mm);
    w.rapid_xy(x, y);
    if hit.slot_mm < 1e-9 {
        let target = -depth;
        match peck {
            None => {
                w.linear_z(target, plunge);
                w.rapid_z(profile.retract_z_mm);
            }
            Some(step) if step > 0.0 => {
                let mut z = 0.0;
                while z > target + 1e-9 {
                    z = (z - step).max(target);
                    w.linear_z(z, plunge);
                    if z > target + 1e-9 {
                        w.rapid_z(profile.retract_z_mm);
                    }
                }
            }
            Some(_) => {
                w.linear_z(target, plunge);
                w.rapid_z(profile.retract_z_mm);
            }
        }
    } else {
        let half = hit.slot_mm / 2.0;
        let len = hit.slot_ux.hypot(hit.slot_uy);
        let (ux, uy) = if len < 1e-9 {
            (1.0, 0.0)
        } else {
            (hit.slot_ux / len, hit.slot_uy / len)
        };
        let sx = x - ux * half;
        let sy = y - uy * half;
        let ex = x + ux * half;
        let ey = y + uy * half;
        let target = -depth;
        match peck {
            None => {
                w.linear_z(target, plunge);
                w.rapid_xy(sx, sy);
                w.linear_xy(ex, ey, cut);
                w.rapid_z(profile.retract_z_mm);
            }
            Some(step) if step > 0.0 => {
                let mut z = 0.0;
                while z > target + 1e-9 {
                    z = (z - step).max(target);
                    w.linear_z(z, plunge);
                    w.rapid_xy(sx, sy);
                    w.linear_xy(ex, ey, cut);
                    if z > target + 1e-9 {
                        w.rapid_z(profile.retract_z_mm);
                    }
                }
            }
            Some(_) => {
                w.linear_z(target, plunge);
                w.rapid_xy(sx, sy);
                w.linear_xy(ex, ey, cut);
                w.rapid_z(profile.retract_z_mm);
            }
        }
    }
}

struct ToolJob {
    tool: Tool,
    hits: Vec<DrillHit>,
}

/// Writes the drill files, one per bit used. Each hole goes to the smallest listed bit that fits
/// its plated-adjusted size; refuses a hole no bit fits, a point past the reach, or invalid
/// options.
pub fn drill_gcode(
    p: &Project,
    options: &DrillOptions,
    profile: &CamProfile,
) -> Result<Vec<(String, String)>, String> {
    p.validate()?;
    if !options.valid() {
        return Err("invalid drill options".into());
    }
    if !profile.valid() {
        return Err("invalid machine profile".into());
    }
    let groups = drill_tools(p)?;
    let depth = p.board.thickness;
    if !depth.is_finite() || depth <= 0.0 {
        return Err("board thickness must be positive".into());
    }
    let mut jobs: Vec<ToolJob> = Vec::new();
    for group in groups {
        for hit in group.hits {
            let dia = effective_drill_diameter(&hit, profile);
            let Some(tool) = pick_tool(dia, &options.tools) else {
                return Err(format!("no drill bit large enough for {:.3} mm hole", dia));
            };
            if let Some(job) = jobs.iter_mut().find(|j| j.tool.number == tool.number) {
                job.hits.push(hit);
            } else {
                jobs.push(ToolJob {
                    tool: tool.clone(),
                    hits: vec![hit],
                });
            }
        }
    }
    jobs.sort_by(|a, b| {
        a.tool
            .diameter_mm
            .partial_cmp(&b.tool.diameter_mm)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.tool.number.cmp(&b.tool.number))
    });
    if jobs.is_empty() {
        return Ok(vec![("board-drill.nc".into(), empty_drill(profile)?)]);
    }
    let peck = options.peck_mm;
    let mut files = Vec::new();
    let multi_file = matches!(options.tool_change, ToolChange::NewFile);
    if multi_file {
        for job in &jobs {
            let mut w = GcodeWriter::new(profile.dialect);
            w.write_header("drill");
            w.rapid_z(profile.safe_z_mm);
            w.comment(&format!(
                "tool T{} {:.3} mm",
                job.tool.number, job.tool.diameter_mm
            ));
            w.spindle_on(job.tool.spindle_rpm);
            let mut hits = job.hits.clone();
            order_hits(options.order, &mut hits);
            let cut = DrillCut {
                depth,
                peck,
                plunge: job.tool.plunge_feed_mm_min,
                cut: job.tool.cut_feed_mm_min,
            };
            for hit in &hits {
                drill_one_hit(&mut w, p, hit, profile, &cut);
            }
            w.write_footer();
            files.push((
                format!("board-drill-T{}.nc", job.tool.number),
                w.into_string(),
            ));
        }
    } else {
        let mut w = GcodeWriter::new(profile.dialect);
        w.write_header("drill");
        w.rapid_z(profile.safe_z_mm);
        for (idx, job) in jobs.iter().enumerate() {
            if idx > 0 {
                match options.tool_change {
                    ToolChange::PauseM0 | ToolChange::M6Message => {
                        w.tool_pause(&job.tool);
                    }
                    ToolChange::NewFile => {}
                }
            }
            w.comment(&format!(
                "tool T{} {:.3} mm",
                job.tool.number, job.tool.diameter_mm
            ));
            w.spindle_on(job.tool.spindle_rpm);
            let mut hits = job.hits.clone();
            order_hits(options.order, &mut hits);
            let cut = DrillCut {
                depth,
                peck,
                plunge: job.tool.plunge_feed_mm_min,
                cut: job.tool.cut_feed_mm_min,
            };
            for hit in &hits {
                drill_one_hit(&mut w, p, hit, profile, &cut);
            }
            if idx + 1 < jobs.len() {
                match options.tool_change {
                    ToolChange::PauseM0 => {
                        w.line("M5");
                        w.rapid_z(profile.safe_z_mm);
                        w.line("M0");
                        w.comment("tool change");
                    }
                    ToolChange::M6Message => {
                        if let Some(next) = jobs.get(idx + 1) {
                            w.tool_pause(&next.tool);
                        }
                    }
                    ToolChange::NewFile => {}
                }
            }
        }
        w.write_footer();
        files.push(("board-drill.nc".into(), w.into_string()));
    }
    Ok(files)
}

fn empty_drill(profile: &CamProfile) -> Result<String, String> {
    let mut w = GcodeWriter::new(profile.dialect);
    w.write_header("drill");
    w.comment("no holes");
    w.write_footer();
    Ok(w.into_string())
}

fn board_outline_ring(p: &Project) -> Vec<Point> {
    let mut ring = crate::board_edge::flat_ring(p, pcbcraft_doc::VALIDATION_CHORD_ERROR);
    if ring.first() != ring.last()
        && let Some(first) = ring.first().copied()
    {
        ring.push(first);
    }
    ring
}

fn offset_ring(points: &[Point], distance: f64) -> Vec<Point> {
    let n = points.len();
    if n < 2 {
        return points.to_vec();
    }
    let open = if points.first() == points.last() {
        &points[..n - 1]
    } else {
        points
    };
    if open.len() < 3 {
        return points.to_vec();
    }
    let mut out = Vec::with_capacity(open.len() + 1);
    let count = open.len();
    for i in 0..count {
        let prev = open[(i + count - 1) % count];
        let curr = open[i];
        let next = open[(i + 1) % count];
        let e1x = curr.x - prev.x;
        let e1y = curr.y - prev.y;
        let e2x = next.x - curr.x;
        let e2y = next.y - curr.y;
        let l1 = e1x.hypot(e1y);
        let l2 = e2x.hypot(e2y);
        if l1 < 1e-12 || l2 < 1e-12 {
            out.push(curr);
            continue;
        }
        let n1x = -e1y / l1;
        let n1y = e1x / l1;
        let n2x = -e2y / l2;
        let n2y = e2x / l2;
        let mx = n1x + n2x;
        let my = n1y + n2y;
        let ml = mx.hypot(my);
        let (nx, ny) = if ml < 1e-12 {
            (n1x, n1y)
        } else {
            (mx / ml, my / ml)
        };
        out.push(Point::new(curr.x + nx * distance, curr.y + ny * distance));
    }
    if let Some(first) = out.first().copied() {
        out.push(first);
    }
    out
}

fn linearize_ring(points: &[Point]) -> Vec<Point> {
    let mut dense = Vec::new();
    for (a, b) in polygon_edges(points) {
        let len = a.distance(b);
        let steps = ((len / LINEARIZE_STEP_MM).ceil() as usize).max(1);
        for s in 0..steps {
            let t = s as f64 / steps as f64;
            dense.push(Point::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t));
        }
    }
    dense
}

fn ring_length(points: &[Point]) -> f64 {
    polygon_edges(points).map(|(a, b)| a.distance(b)).sum()
}

fn tab_intervals(perimeter: f64, tabs: Tabs) -> Vec<(f64, f64)> {
    if tabs.count == 0 || perimeter < 1e-6 {
        return Vec::new();
    }
    let width = tabs.width_mm.min(perimeter / tabs.count as f64);
    let gap = perimeter / tabs.count as f64;
    (0..tabs.count)
        .map(|i| {
            let start = gap * i as f64 + (gap - width) / 2.0;
            (start, start + width)
        })
        .collect()
}

fn cut_z_at(dist: f64, pass_z: f64, tab_z: f64, tabs: &[(f64, f64)]) -> f64 {
    for (start, end) in tabs {
        if dist + 1e-9 >= *start && dist <= *end + 1e-9 {
            return tab_z;
        }
    }
    pass_z
}

fn bounds_of_points(points: &[Point]) -> (Point, Point) {
    let mut min = Point::new(f64::INFINITY, f64::INFINITY);
    let mut max = Point::new(f64::NEG_INFINITY, f64::NEG_INFINITY);
    for p in points {
        min.x = min.x.min(p.x);
        min.y = min.y.min(p.y);
        max.x = max.x.max(p.x);
        max.y = max.y.max(p.y);
    }
    (min, max)
}

fn within_work_area(points: &[Point], profile: &CamProfile) -> bool {
    let (min, max) = bounds_of_points(points);
    min.x >= 0.0
        && min.y >= 0.0
        && max.x <= profile.work_x_mm + 1e-6
        && max.y <= profile.work_y_mm + 1e-6
}

/// Writes the board outline cut in depth passes, with tabs on every pass above the final one and
/// optional interior cutouts. Refuses invalid options or a path past the reach.
pub fn outline_gcode(
    p: &Project,
    options: &OutlineOptions,
    profile: &CamProfile,
) -> Result<String, String> {
    p.validate()?;
    if !options.valid(p.board.thickness) {
        return Err("invalid outline options".into());
    }
    if !profile.valid() {
        return Err("invalid machine profile".into());
    }
    let thickness = p.board.thickness;
    let passes = ((thickness / options.pass_depth_mm).ceil() as u32).max(1);
    let offset_dist = match options.compensation {
        OutlineCompensation::Outside => options.tool.diameter_mm / 2.0,
        OutlineCompensation::Inside => -options.tool.diameter_mm / 2.0,
        OutlineCompensation::None => 0.0,
    };
    let mut paths: Vec<Vec<Point>> = Vec::new();
    let outline = board_outline_ring(p);
    let mut outer = offset_ring(&outline, offset_dist);
    if polygon_area(&outer) < 0.0 {
        outer.reverse();
    }
    paths.push(linearize_ring(&outer));
    if options.cutouts {
        for c in &p.board.cutouts {
            let r = (c.radius - options.tool.diameter_mm / 2.0).max(0.05);
            let circle = linearize_circle(c.center, r);
            paths.push(circle);
        }
    }
    for path in &paths {
        if !within_work_area(path, profile) {
            return Err("outline exceeds the machine work area".into());
        }
    }
    let tab_z = -(thickness - options.tabs.height_mm);
    let perimeter = ring_length(&paths[0]);
    let tab_spans = tab_intervals(perimeter, options.tabs);
    let ccw = matches!(profile.direction, CutDirection::Conventional);
    let mut w = GcodeWriter::new(profile.dialect);
    w.write_header("outline");
    w.rapid_z(profile.safe_z_mm);
    w.comment(&format!(
        "tool T{} {:.3} mm",
        options.tool.number, options.tool.diameter_mm
    ));
    w.spindle_on(options.tool.spindle_rpm);
    for pass in 1..=passes {
        let pass_z = -(options.pass_depth_mm * pass as f64).min(thickness);
        w.comment(&format!("pass {pass} of {passes}"));
        for (path_idx, path) in paths.iter().enumerate() {
            if path.is_empty() {
                continue;
            }
            let mut traverse: Vec<Point> = path.clone();
            if path_idx == 0 && !ccw {
                traverse.reverse();
            }
            let start = traverse[0];
            w.rapid_z(profile.safe_z_mm);
            w.rapid_xy(start.x, start.y);
            let first_z = if pass_z < tab_z - 1e-9 {
                cut_z_at(0.0, pass_z, tab_z, &tab_spans)
            } else {
                pass_z
            };
            w.linear_z(first_z, options.tool.plunge_feed_mm_min);
            let mut dist = 0.0;
            for window in traverse.windows(2) {
                let a = window[0];
                let b = window[1];
                let seg = a.distance(b);
                if pass_z < tab_z - 1e-9 && !tab_spans.is_empty() {
                    let steps = ((seg / LINEARIZE_STEP_MM).ceil() as usize).max(1);
                    for s in 1..=steps {
                        let t = s as f64 / steps as f64;
                        let px = a.x + (b.x - a.x) * t;
                        let py = a.y + (b.y - a.y) * t;
                        let d = dist + seg * t;
                        let z = cut_z_at(d, pass_z, tab_z, &tab_spans);
                        w.linear_xyz(px, py, z, options.tool.cut_feed_mm_min);
                    }
                } else {
                    w.linear_xy(b.x, b.y, options.tool.cut_feed_mm_min);
                }
                dist += seg;
            }
            w.rapid_z(profile.retract_z_mm);
        }
    }
    w.write_footer();
    Ok(w.into_string())
}

fn linearize_circle(center: Point, radius: f64) -> Vec<Point> {
    let circumference = 2.0 * PI * radius;
    let n = ((circumference / LINEARIZE_STEP_MM).ceil() as usize).max(8);
    (0..=n)
        .map(|i| {
            let a = 2.0 * PI * i as f64 / n as f64;
            Point::new(center.x + radius * a.cos(), center.y + radius * a.sin())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn count_drill_moves(gcode: &str) -> usize {
        gcode
            .lines()
            .filter(|line| {
                let upper = line.to_ascii_uppercase();
                upper.starts_with("G1") && upper.contains('Z')
            })
            .count()
            + gcode
                .lines()
                .filter(|line| {
                    let upper = line.to_ascii_uppercase();
                    upper.starts_with("G1")
                        && !upper.contains('Z')
                        && upper.contains('X')
                        && upper.contains('Y')
                })
                .count()
    }
    use pcbcraft_doc::{Hole, Net, Via};

    fn test_profile() -> CamProfile {
        CamProfile {
            dialect: Dialect::Grbl,
            work_x_mm: 320.0,
            work_y_mm: 350.0,
            safe_z_mm: 5.0,
            retract_z_mm: 2.0,
            direction: CutDirection::Conventional,
            plating_allowance_mm: 0.0,
            fab_plated: false,
        }
    }

    fn tool(number: u8, diameter_mm: f64) -> Tool {
        Tool {
            number,
            diameter_mm,
            spindle_rpm: 10_000,
            cut_feed_mm_min: 300.0,
            plunge_feed_mm_min: 100.0,
        }
    }

    fn drill_fixture_project() -> Project {
        let mut p = Project::default();
        p.nets.push(Net {
            id: 100,
            name: "GND".into(),
            ..Default::default()
        });
        p.board.vias.push(Via {
            preserved: pcbcraft_doc::PreservedSource::default(),
            id: 1,
            net: 100,
            position: Point::new(10.0, 10.0),
            diameter: 0.6,
            drill: 0.3,
            from_layer: None,
            to_layer: None,
            covering: None,
            covering_bottom: None,
            backdrill: None,
        });
        p.board.holes.push(Hole {
            id: 2,
            position: Point::new(20.0, 15.0),
            diameter: 1.0,
            end: None,
        });
        p.board.holes.push(Hole {
            id: 3,
            position: Point::new(30.0, 20.0),
            diameter: 2.0,
            end: None,
        });
        p
    }

    fn drill_options() -> DrillOptions {
        DrillOptions {
            tools: vec![tool(1, 0.3), tool(2, 1.0), tool(3, 2.0)],
            peck_mm: None,
            order: DrillOrder::Nearest,
            alignment_holes: vec![],
            tool_change: ToolChange::PauseM0,
        }
    }

    #[test]
    fn drill_gcode_golden_three_tools_ascending_with_a_stop_between_groups() -> Result<(), String> {
        let p = drill_fixture_project();
        let options = drill_options();
        let profile = test_profile();
        let files = drill_gcode(&p, &options, &profile)?;
        assert_eq!(files.len(), 1);
        let text = &files[0].1;
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/drill_three_tools.nc");
        if std::env::var_os("PCBCRAFT_BLESS").is_some() {
            std::fs::write(&fixture, text).map_err(|e| e.to_string())?;
        }
        let expected = std::fs::read_to_string(&fixture).map_err(|e| e.to_string())?;
        assert_eq!(*text, expected, "drill G-code drifted from the fixture");
        let t1 = text.find("tool T1").ok_or("missing first tool group")?;
        let t2 = text.find("tool T2").ok_or("missing second tool group")?;
        let t3 = text.find("tool T3").ok_or("missing third tool group")?;
        assert!(t1 < t2, "smaller tool group should appear first");
        assert!(t2 < t3, "tool groups should ascend by bit size");
        assert_eq!(
            text.matches("M0").count(),
            2,
            "a stop between each tool group"
        );
        Ok(())
    }

    #[test]
    fn every_hole_is_drilled_once() -> Result<(), String> {
        let p = drill_fixture_project();
        let options = drill_options();
        let profile = test_profile();
        let files = drill_gcode(&p, &options, &profile)?;
        let text = &files[0].1;
        let expected_hits: usize = drill_tools(&p)?.iter().map(|g| g.hits.len()).sum();
        let moves = count_drill_moves(text);
        assert_eq!(
            moves, expected_hits,
            "each round hole should get one plunge move"
        );
        Ok(())
    }

    #[test]
    fn outline_gcode_golden_four_tabs_three_passes() -> Result<(), String> {
        let p = Project::default();
        let options = OutlineOptions {
            tool: tool(1, 3.0),
            pass_depth_mm: 0.6,
            tabs: Tabs {
                count: 4,
                width_mm: 5.0,
                height_mm: 0.4,
            },
            compensation: OutlineCompensation::Outside,
            cutouts: false,
        };
        let profile = test_profile();
        let text = outline_gcode(&p, &options, &profile)?;
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/outline_four_tabs_three_passes.nc");
        if std::env::var_os("PCBCRAFT_BLESS").is_some() {
            std::fs::write(&fixture, &text).map_err(|e| e.to_string())?;
        }
        let expected = std::fs::read_to_string(&fixture).map_err(|e| e.to_string())?;
        assert_eq!(text, expected, "outline G-code drifted from the fixture");
        assert_eq!(text.matches("pass 1 of 3").count(), 1);
        assert_eq!(text.matches("pass 2 of 3").count(), 1);
        assert_eq!(text.matches("pass 3 of 3").count(), 1);
        Ok(())
    }

    #[test]
    fn drill_and_outline_output_twice_is_byte_identical() -> Result<(), String> {
        let p = drill_fixture_project();
        let options = drill_options();
        let profile = test_profile();
        let a = drill_gcode(&p, &options, &profile)?;
        let b = drill_gcode(&p, &options, &profile)?;
        assert_eq!(a, b);
        let outline = OutlineOptions {
            tool: tool(1, 3.0),
            pass_depth_mm: 0.6,
            tabs: Tabs {
                count: 4,
                width_mm: 5.0,
                height_mm: 0.4,
            },
            compensation: OutlineCompensation::Outside,
            cutouts: false,
        };
        let o1 = outline_gcode(&p, &outline, &profile)?;
        let o2 = outline_gcode(&p, &outline, &profile)?;
        assert_eq!(o1, o2);
        Ok(())
    }
}
