//! Read fabrication plot and drill files for inspection (bounded, read-only).
//!
//! Parsed Gerber layers and Excellon drill data are returned as plain structs
//! so callers can preview fabrication output without editing the project.
//!
//! # Rules
//! - Plot operations, apertures and drill hits are capped (`MAX_OPS`, `MAX_APERTURES`, `MAX_HITS`).
//! - Zip archives use the same byte budget as one on-disk file (`MAX_PLOT_ZIP_BYTES`).
//!
//! # Dependencies
//! - crates/geom/src/lib.rs
//! - crates/io/src/lib.rs
//! - crates/io/tests/fixtures/shaped_pads_F_Cu.gbr

use pcbcraft_geom::Point;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::{MAX_FILE_BYTES, read_text};

/// Total extracted bytes allowed when reading a plot zip (same budget as one file).
pub const MAX_PLOT_ZIP_BYTES: u64 = MAX_FILE_BYTES;

/// Operations one plot layer may carry. Sized to the writer's per-layer
/// budget (`MAX_COPPER_LAYER_OPS`) so a file this crate writes also loads.
const MAX_OPS: usize = 1_000_000;
const MAX_APERTURES: usize = 64;
const MAX_HITS: usize = 50_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlotHint {
    pub message: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlotLayerKind {
    FrontCopper,
    BackCopper,
    Edge,
    DrillMap,
    Mask,
    Paste,
    Silk,
    DrillOverlay,
    Unknown,
}

#[derive(Clone, Debug)]
pub struct PlotAperture {
    pub code: u32,
    pub template: String,
    pub width_mm: f64,
    pub height_mm: f64,
    pub function: Option<String>,
}

#[derive(Clone, Debug)]
pub struct PlotFlash {
    pub x_mm: f64,
    pub y_mm: f64,
    pub aperture: u32,
}

#[derive(Clone, Debug)]
pub struct PlotStroke {
    pub points: Vec<(f64, f64)>,
    pub aperture: u32,
}

#[derive(Clone, Debug)]
pub struct PlotLayer {
    pub id: String,
    pub kind: PlotLayerKind,
    pub color: [u8; 3],
    pub polarity_negative: bool,
    pub apertures: Vec<PlotAperture>,
    pub flashes: Vec<PlotFlash>,
    pub strokes: Vec<PlotStroke>,
    pub regions: Vec<Vec<(f64, f64)>>,
    /// Per region, true when it was written with clear polarity (`%LPC*%`),
    /// that is a hole cut out of copper drawn before it.
    pub region_clear: Vec<bool>,
    pub flash_count: usize,
    pub draw_count: usize,
    pub hints: Vec<PlotHint>,
}

impl PlotLayer {
    /// Copper area of the regions in square millimetres: dark contours less
    /// clear-polarity contours (holes), each by its own absolute area.
    ///
    /// # Rules
    /// - Assumes each clear contour lies inside dark copper written before it,
    ///   which is how this crate writes pours.
    pub fn region_area_mm2(&self) -> f64 {
        self.regions
            .iter()
            .zip(&self.region_clear)
            .map(|(ring, clear)| {
                let points: Vec<Point> = ring.iter().map(|(x, y)| Point::new(*x, *y)).collect();
                let area = pcbcraft_geom::polygon_area(&points).abs();
                if *clear { -area } else { area }
            })
            .sum()
    }
}
#[derive(Clone, Debug)]
pub struct PlotDrillTool {
    /// Tool index from `T` lines in the drill body.
    pub number: u32,
    /// Cutter diameter in millimetres from the header `T<n>C<diameter>` definition.
    pub diameter_mm: f64,
    /// Plated versus unplated when the drill file states it; otherwise unset.
    pub plated: Option<bool>,
    /// How many hits in the file used this tool number.
    pub hit_count: usize,
}

/// One drilled hole or slot start from an Excellon body line.
///
/// # Rules
/// - Round holes set `slot_end` to none; `G85` slot lines store the far end in millimetres.
///
/// # Dependencies
/// - crates/io/src/plot_read.rs
#[derive(Clone, Debug)]
pub struct PlotDrillHit {
    /// Tool number active when this hit was written.
    pub tool: u32,
    /// Hole or slot start x in millimetres.
    pub x_mm: f64,
    /// Hole or slot start y in millimetres.
    pub y_mm: f64,
    /// Far end of a routed slot in millimetres, or none for a round hit.
    pub slot_end: Option<(f64, f64)>,
}

/// Every Gerber layer and drill record loaded from one folder or zip archive.
///
/// # Rules
/// - `layers` includes a synthetic drill overlay when any hits were read.
/// - `hints` collects non-fatal parse notes from individual Gerber layers.
///
/// # Dependencies
/// - crates/io/src/plot_read.rs
#[derive(Clone, Debug)]
pub struct PlotPackage {
    /// Gerber layers in filename order, plus an optional drill overlay layer.
    pub layers: Vec<PlotLayer>,
    /// Drill tools merged from every `.drl` file, with renumbered indices.
    pub drill_tools: Vec<PlotDrillTool>,
    /// Drill and slot hits merged from every `.drl` file.
    pub drill_hits: Vec<PlotDrillHit>,
    /// Parse hints surfaced while loading Gerber files in the package.
    pub hints: Vec<PlotHint>,
}

/// Split a plot file into statements. The flag is true for extended
/// commands, the ones written between `%` marks.
fn gerber_statements(text: &str) -> Vec<(bool, String)> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut extended = false;
    // A comment closed by `*%` is a known quirk of some writers; the stray
    // percent sign must not open an extended command.
    let mut after_comment = false;
    for ch in text.chars() {
        if ch == '%' && after_comment {
            after_comment = false;
            continue;
        }
        after_comment = false;
        match ch {
            '%' => {
                extended = !extended;
                cur.clear();
            }
            '*' => {
                let t = cur.trim();
                if !t.is_empty() {
                    after_comment = !extended && t.starts_with("G04");
                    out.push((extended, t.to_owned()));
                }
                cur.clear();
            }
            '\r' | '\n' if extended => {}
            _ => cur.push(ch),
        }
    }
    out
}

/// One plot statement split into its letter-number words.
#[derive(Default)]
struct Words {
    d: Vec<u32>,
    g: Option<u32>,
    x: Option<f64>,
    y: Option<f64>,
    i: Option<f64>,
    j: Option<f64>,
}

/// Parse a coordinate word the way the Gerber Layer Format Specification
/// reads it: an integer scaled by the `%FS` decimal count, so `X10000000` in a
/// 4.6 file is ten millimetres.
///
/// # Rules
/// - Text with a decimal point is taken as written. Files written before
///   issue #684 carry that form under a 4.6 declaration, and they still load.
///
/// # Do not
/// - Write decimal coordinates; the writers emit integers only.
fn coordinate(text: &str, decimals: i32) -> Option<f64> {
    if text.contains('.') {
        text.parse().ok()
    } else {
        let v: f64 = text.parse().ok()?;
        Some(v / 10f64.powi(decimals))
    }
}

fn split_words(stmt: &str, decimals: i32) -> Words {
    let mut w = Words::default();
    let chars: Vec<char> = stmt.chars().collect();
    let mut at = 0;
    while at < chars.len() {
        let letter = chars[at];
        at += 1;
        let start = at;
        while at < chars.len()
            && (chars[at].is_ascii_digit() || matches!(chars[at], '.' | '-' | '+'))
        {
            at += 1;
        }
        let num: String = chars[start..at].iter().collect();
        match letter {
            'D' => {
                if let Ok(n) = num.parse::<u32>() {
                    w.d.push(n);
                }
            }
            'G' => w.g = num.parse().ok(),
            'X' => w.x = coordinate(&num, decimals),
            'Y' => w.y = coordinate(&num, decimals),
            'I' => w.i = coordinate(&num, decimals),
            'J' => w.j = coordinate(&num, decimals),
            _ => {}
        }
    }
    w
}

/// The number of decimals in `FSLAX<i><d>Y<i><d>`, six when absent.
fn format_decimals(text: &str) -> i32 {
    text.find("FSLAX")
        .or_else(|| text.find("FSLNX"))
        .or_else(|| text.find("FSLA"))
        .and_then(|at| {
            let rest = &text[at..];
            let x = rest.find('X')?;
            let digits: Vec<char> = rest[x + 1..].chars().take(2).collect();
            digits.get(1).and_then(|c| c.to_digit(10)).map(|d| d as i32)
        })
        .unwrap_or(6)
}

/// True when the statement draws, flashes or moves.
fn statement_op(w: &Words) -> Option<u32> {
    if let Some(op) = w.d.iter().copied().find(|d| (1..=3).contains(d)) {
        return Some(op);
    }
    if matches!(w.g, Some(2 | 3)) && (w.x.is_some() || w.y.is_some() || w.i.is_some()) {
        return Some(1);
    }
    None
}

fn parse_coord_pair(stmt: &str) -> Option<(f64, f64)> {
    let x_at = stmt.find('X')?;
    let y_at = stmt.find('Y')?;
    if y_at <= x_at {
        return None;
    }
    let x_str = &stmt[x_at + 1..y_at];
    let y_end = stmt[y_at + 1..]
        .find(|c: char| !c.is_ascii_digit() && c != '.' && c != '-' && c != '+')
        .unwrap_or(stmt.len() - y_at - 1);
    let y_str = &stmt[y_at + 1..y_at + 1 + y_end];
    let x: f64 = x_str.parse().ok()?;
    let y: f64 = y_str.parse().ok()?;
    Some((x, y))
}

fn inch_to_mm(scale_inch: bool, x: f64, y: f64) -> (f64, f64) {
    if scale_inch {
        (x * 25.4, y * 25.4)
    } else {
        (x, y)
    }
}

fn layer_color(kind: PlotLayerKind) -> [u8; 3] {
    match kind {
        PlotLayerKind::FrontCopper => [184, 115, 51],
        PlotLayerKind::BackCopper => [139, 90, 43],
        PlotLayerKind::Edge => [250, 220, 60],
        PlotLayerKind::DrillMap => [160, 160, 200],
        PlotLayerKind::Mask => [32, 120, 80],
        PlotLayerKind::Paste => [180, 180, 200],
        PlotLayerKind::Silk => [240, 240, 200],
        PlotLayerKind::DrillOverlay => [255, 255, 255],
        PlotLayerKind::Unknown => [140, 140, 160],
    }
}

fn classify_from_filename(name: &str) -> PlotLayerKind {
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".drl") {
        return PlotLayerKind::DrillOverlay;
    }
    if lower.contains("f.cu") || lower.contains("-f_") || lower.contains("_f_cu") {
        return PlotLayerKind::FrontCopper;
    }
    if lower.contains("b.cu") || lower.contains("-b_") || lower.contains("_b_cu") {
        return PlotLayerKind::BackCopper;
    }
    if lower.contains("edge") || lower.contains("profile") || lower.contains("outline") {
        return PlotLayerKind::Edge;
    }
    if lower.contains("drillmap") || lower.contains("drill_map") {
        return PlotLayerKind::DrillMap;
    }
    if lower.contains("mask") {
        return PlotLayerKind::Mask;
    }
    if lower.contains("paste") {
        return PlotLayerKind::Paste;
    }
    if lower.contains("silks") || lower.contains("legend") || lower.contains("silk") {
        return PlotLayerKind::Silk;
    }
    PlotLayerKind::Unknown
}

fn classify_from_x2(file_function: &str) -> PlotLayerKind {
    let ff = file_function.to_ascii_lowercase();
    if ff.contains("copper") && (ff.contains("top") || ff.contains("l1")) {
        PlotLayerKind::FrontCopper
    } else if ff.contains("copper") && (ff.contains("bot") || ff.contains("bottom")) {
        PlotLayerKind::BackCopper
    } else if ff.contains("profile") || ff.contains("outline") {
        PlotLayerKind::Edge
    } else if ff.contains("soldermask") {
        PlotLayerKind::Mask
    } else if ff.contains("paste") {
        PlotLayerKind::Paste
    } else if ff.contains("legend") || ff.contains("silk") {
        PlotLayerKind::Silk
    } else {
        PlotLayerKind::Unknown
    }
}

fn parse_add_aperture(stmt: &str, inch: bool) -> Option<(u32, PlotAperture)> {
    let inner = stmt.strip_prefix("%ADD")?;
    let star = inner.find('*')?;
    let head = &inner[..star];
    let code_end = head.find(|c: char| !c.is_ascii_digit())?;
    let code: u32 = head[..code_end].parse().ok()?;
    let rest = &head[code_end..];
    let scale = if inch { 25.4 } else { 1.0 };
    if let Some(r) = rest.strip_prefix('C') {
        let d: f64 = r.trim_start_matches(',').parse().ok()?;
        return Some((
            code,
            PlotAperture {
                code,
                template: "C".into(),
                width_mm: d * scale,
                height_mm: d * scale,
                function: None,
            },
        ));
    }
    if let Some(r) = rest.strip_prefix('R') {
        if rest.starts_with("R2") {
            return None;
        }
        let parts: Vec<&str> = r.trim_start_matches(',').split('X').collect();
        if parts.len() >= 2 {
            let w: f64 = parts[0].parse().ok()?;
            let h: f64 = parts[1].parse().ok()?;
            return Some((
                code,
                PlotAperture {
                    code,
                    template: "R".into(),
                    width_mm: w * scale,
                    height_mm: h * scale,
                    function: None,
                },
            ));
        }
    }
    if let Some(r) = rest.strip_prefix('O') {
        let parts: Vec<&str> = r.trim_start_matches(',').split('X').collect();
        if parts.len() >= 2 {
            let w: f64 = parts[0].parse().ok()?;
            let h: f64 = parts[1].parse().ok()?;
            return Some((
                code,
                PlotAperture {
                    code,
                    template: "O".into(),
                    width_mm: w * scale,
                    height_mm: h * scale,
                    function: None,
                },
            ));
        }
    }
    None
}

/// Count semantic flashes (`D03`) and draw segments (`D01`) in gerber text.
pub fn count_flashes_and_draws(text: &str) -> Result<(usize, usize), String> {
    let decimals = format_decimals(text);
    let mut flashes = 0usize;
    let mut draws = 0usize;
    for (extended, stmt) in gerber_statements(text) {
        if extended || stmt.starts_with("G04") {
            continue;
        }
        let words = split_words(&stmt, decimals);
        match statement_op(&words) {
            Some(3) => flashes += 1,
            Some(1) => draws += 1,
            _ => {}
        }
    }
    Ok((flashes, draws))
}

/// Points along an arc from `from` to `to` about `center`, `to` included.
/// Equal ends make a full circle.
fn arc_points(
    from: (f64, f64),
    to: (f64, f64),
    center: (f64, f64),
    clockwise: bool,
) -> Vec<(f64, f64)> {
    let radius = (from.0 - center.0).hypot(from.1 - center.1);
    if !radius.is_finite() || radius <= 0.0 {
        return vec![to];
    }
    let start = (from.1 - center.1).atan2(from.0 - center.0);
    let end = (to.1 - center.1).atan2(to.0 - center.0);
    let tau = std::f64::consts::TAU;
    let mut sweep = if clockwise { start - end } else { end - start };
    while sweep < 0.0 {
        sweep += tau;
    }
    while sweep > tau {
        sweep -= tau;
    }
    if sweep < 1e-9 {
        sweep = tau;
    }
    let steps = ((sweep / 0.1).ceil() as usize).clamp(2, 128);
    let mut out = Vec::with_capacity(steps);
    for n in 1..steps {
        let t = sweep * n as f64 / steps as f64;
        let a = if clockwise { start - t } else { start + t };
        out.push((center.0 + radius * a.cos(), center.1 + radius * a.sin()));
    }
    out.push(to);
    out
}

/// Parse one gerber layer file into counts and drawable geometry.
pub fn parse_gerber_layer(text: &str, source_name: &str) -> Result<PlotLayer, String> {
    let inch = text.contains("MOIN") && !text.contains("MOMM");
    let decimals = format_decimals(text);
    let unit = if inch { 25.4 } else { 1.0 };
    let mut kind = PlotLayerKind::Unknown;
    let mut polarity_negative = false;
    let mut saw_x2 = false;
    let mut hints = Vec::new();
    let mut apertures: BTreeMap<u32, PlotAperture> = BTreeMap::new();
    let mut flashes = Vec::new();
    let mut strokes = Vec::new();
    let mut regions: Vec<Vec<(f64, f64)>> = Vec::new();
    let mut region_clear: Vec<bool> = Vec::new();
    let mut clear_polarity = false;
    let mut current_ap: u32 = 10;
    let mut ops = 0usize;
    let mut in_region = false;
    let mut contour: Vec<(f64, f64)> = Vec::new();
    let mut at = (0.0f64, 0.0f64);
    let mut clockwise = true;

    for (extended, stmt) in gerber_statements(text) {
        let s = stmt.as_str();
        if extended {
            if let Some(rest) = s.strip_prefix("TF.FileFunction,") {
                saw_x2 = true;
                kind = classify_from_x2(rest);
            } else if s.starts_with("TF.FilePolarity,Negative") {
                polarity_negative = true;
                saw_x2 = true;
            } else if s == "LPC" {
                clear_polarity = true;
            } else if s == "LPD" {
                clear_polarity = false;
            } else if s.starts_with("TF.") {
                saw_x2 = true;
            } else if let Some(rest) = s.strip_prefix("TA.AperFunction,") {
                saw_x2 = true;
                if let Some(ap) = apertures.get_mut(&current_ap) {
                    ap.function = Some(rest.into());
                }
            } else if s.starts_with("ADD") {
                if apertures.len() >= MAX_APERTURES {
                    return Err("plot layer exceeds the aperture limit".into());
                }
                if let Some((code, ap)) = parse_add_aperture(&format!("%{s}*"), inch) {
                    apertures.insert(code, ap);
                }
            }
            continue;
        }
        if s.starts_with("G04") {
            continue;
        }
        let words = split_words(s, decimals);
        if let Some(code) = words.d.iter().copied().find(|d| *d >= 10) {
            current_ap = code;
        }
        match words.g {
            Some(2) => clockwise = true,
            Some(3) => clockwise = false,
            Some(36) => {
                in_region = true;
                contour.clear();
                continue;
            }
            Some(37) => {
                if contour.len() >= 3 {
                    regions.push(std::mem::take(&mut contour));
                    region_clear.push(clear_polarity);
                }
                contour.clear();
                in_region = false;
                continue;
            }
            _ => {}
        }
        let Some(op) = statement_op(&words) else {
            continue;
        };
        let to = (
            words.x.map_or(at.0, |v| v * unit),
            words.y.map_or(at.1, |v| v * unit),
        );
        ops += 1;
        if ops > MAX_OPS {
            return Err(format!(
                "plot layer {source_name} exceeds the object limit of {MAX_OPS} operations"
            ));
        }
        match op {
            3 => flashes.push(PlotFlash {
                x_mm: to.0,
                y_mm: to.1,
                aperture: current_ap,
            }),
            2 => {
                if in_region && contour.len() >= 3 {
                    regions.push(std::mem::take(&mut contour));
                    region_clear.push(clear_polarity);
                }
                contour.clear();
                if in_region {
                    contour.push(to);
                }
            }
            _ => {
                let arc = (words.i.is_some() || words.j.is_some())
                    .then(|| (words.i.unwrap_or(0.0) * unit, words.j.unwrap_or(0.0) * unit));
                let mut path = vec![at];
                match arc {
                    Some((i, j)) => {
                        path.extend(arc_points(at, to, (at.0 + i, at.1 + j), clockwise))
                    }
                    None => path.push(to),
                }
                if in_region {
                    if contour.is_empty() {
                        contour.push(at);
                    }
                    contour.extend(path.into_iter().skip(1));
                } else {
                    strokes.push(PlotStroke {
                        points: path,
                        aperture: current_ap,
                    });
                }
            }
        }
        at = to;
    }

    if !saw_x2 {
        kind = classify_from_filename(source_name);
        hints.push(PlotHint {
            message: "This file set does not describe itself with layer metadata; names were guessed from filenames.".into(),
        });
    }
    let (flash_count, draw_count) = count_flashes_and_draws(text)?;
    Ok(PlotLayer {
        id: source_name.to_owned(),
        kind,
        color: layer_color(kind),
        polarity_negative,
        apertures: apertures.into_values().collect(),
        flashes,
        strokes,
        regions,
        region_clear,
        flash_count,
        draw_count,
        hints,
    })
}

/// Parse an Excellon drill file written by the native export path.
pub fn parse_excellon_drill(text: &str) -> Result<(Vec<PlotDrillTool>, Vec<PlotDrillHit>), String> {
    let inch = text.contains("INCH");
    let scale = if inch { 25.4 } else { 1.0 };
    let mut tools: BTreeMap<u32, f64> = BTreeMap::new();
    let mut hits: Vec<PlotDrillHit> = Vec::new();
    let mut current_tool = 1u32;
    let mut in_header = text.contains("M48");
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line == "M48" {
            in_header = true;
            continue;
        }
        if line == "%" {
            in_header = false;
            continue;
        }
        if in_header {
            if let Some(rest) = line.strip_prefix('T')
                && let Some((num, diam)) = rest.split_once('C')
                && let Ok(n) = num.parse::<u32>()
                && let Ok(d) = diam.parse::<f64>()
            {
                tools.insert(n, d * scale);
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix('T')
            && let Ok(n) = rest.parse::<u32>()
        {
            current_tool = n;
            continue;
        }
        if line.starts_with('X') {
            if hits.len() >= MAX_HITS {
                return Err("drill file exceeds the hit limit".into());
            }
            if let Some(g85) = line.find("G85") {
                let start = parse_coord_pair(line)
                    .ok_or_else(|| "slot start has no coordinates".to_string())?;
                let end_part = &line[g85 + 3..];
                let end = parse_coord_pair(end_part)
                    .ok_or_else(|| "slot end has no coordinates".to_string())?;
                let (sx, sy) = inch_to_mm(inch, start.0, start.1);
                let (ex, ey) = inch_to_mm(inch, end.0, end.1);
                hits.push(PlotDrillHit {
                    tool: current_tool,
                    x_mm: sx,
                    y_mm: sy,
                    slot_end: Some((ex, ey)),
                });
            } else if let Some((x, y)) = parse_coord_pair(line) {
                let (x_mm, y_mm) = inch_to_mm(inch, x, y);
                hits.push(PlotDrillHit {
                    tool: current_tool,
                    x_mm,
                    y_mm,
                    slot_end: None,
                });
            }
        }
    }
    let mut tool_list: Vec<PlotDrillTool> = tools
        .iter()
        .map(|(n, d)| PlotDrillTool {
            number: *n,
            diameter_mm: *d,
            plated: None,
            hit_count: hits.iter().filter(|h| h.tool == *n).count(),
        })
        .collect();
    tool_list.sort_by_key(|t| t.number);
    Ok((tool_list, hits))
}

/// Parse an XNC drill file from the native export path (two-digit tools, rout slots).
pub fn parse_xnc_drill(text: &str) -> Result<(Vec<PlotDrillTool>, Vec<PlotDrillHit>), String> {
    let inch = text.lines().any(|l| l.trim() == "INCH");
    let scale = if inch { 25.4 } else { 1.0 };
    let mut tools: BTreeMap<u32, f64> = BTreeMap::new();
    let mut hits: Vec<PlotDrillHit> = Vec::new();
    let mut current_tool = 1u32;
    let mut in_header = false;
    let mut pending_slot_start: Option<(f64, f64)> = None;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line == "M48" {
            in_header = true;
            continue;
        }
        if line == "%" {
            in_header = false;
            continue;
        }
        if in_header {
            if let Some(rest) = line.strip_prefix('T')
                && let Some((num, diam)) = rest.split_once('C')
                && let Ok(n) = num.parse::<u32>()
                && let Ok(d) = diam.parse::<f64>()
            {
                tools.insert(n, d * scale);
            }
            continue;
        }
        if line == "M15" || line == "M16" {
            continue;
        }
        if let Some(rest) = line.strip_prefix('T')
            && let Ok(n) = rest.parse::<u32>()
        {
            current_tool = n;
            continue;
        }
        if let Some(coords) = line.strip_prefix("G00") {
            pending_slot_start = parse_coord_pair(coords).map(|(x, y)| inch_to_mm(inch, x, y));
            continue;
        }
        if let Some(coords) = line.strip_prefix("G01") {
            let Some((sx, sy)) = pending_slot_start.take() else {
                return Err("rout slot missing a G00 start".into());
            };
            let Some((ex, ey)) = parse_coord_pair(coords) else {
                return Err("rout slot end has no coordinates".into());
            };
            let (ex, ey) = inch_to_mm(inch, ex, ey);
            if hits.len() >= MAX_HITS {
                return Err("drill file exceeds the hit limit".into());
            }
            hits.push(PlotDrillHit {
                tool: current_tool,
                x_mm: sx,
                y_mm: sy,
                slot_end: Some((ex, ey)),
            });
            continue;
        }
        if line.starts_with('X') {
            if hits.len() >= MAX_HITS {
                return Err("drill file exceeds the hit limit".into());
            }
            if let Some((x, y)) = parse_coord_pair(line) {
                let (x_mm, y_mm) = inch_to_mm(inch, x, y);
                hits.push(PlotDrillHit {
                    tool: current_tool,
                    x_mm,
                    y_mm,
                    slot_end: None,
                });
            }
        }
    }
    let mut tool_list: Vec<PlotDrillTool> = tools
        .iter()
        .map(|(n, d)| PlotDrillTool {
            number: *n,
            diameter_mm: *d,
            plated: None,
            hit_count: hits.iter().filter(|h| h.tool == *n).count(),
        })
        .collect();
    tool_list.sort_by_key(|t| t.number);
    Ok((tool_list, hits))
}

fn is_plot_name_ext(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".gbr") || lower.ends_with(".drl")
}

fn safe_zip_name(name: &[u8]) -> Result<String, String> {
    if name.is_empty() || name.len() > 255 {
        return Err("archive entry name is invalid".into());
    }
    if name.starts_with(b"/") || name.windows(2).any(|w| w == b"..") {
        return Err("archive entry path is not allowed".into());
    }
    let text =
        std::str::from_utf8(name).map_err(|_| "archive entry name is not UTF-8".to_string())?;
    if text.contains('\\') || text.contains(':') {
        return Err("archive entry path is not allowed".into());
    }
    Ok(text.split('/').next_back().unwrap_or(text).to_owned())
}

/// True for a combined drill file when separate plated and unplated files
/// sit beside it, so its holes are not counted twice.
fn skip_combined_drill(name: &str, drill_names: &[String]) -> bool {
    let lower = name.to_ascii_lowercase();
    let split = drill_names.iter().any(|n| {
        let n = n.to_ascii_lowercase();
        n.contains("plated") || n.contains("npth")
    });
    split && !lower.contains("plated") && !lower.contains("npth")
}

/// Add one drill file to the running set, renumbering its tools so two files
/// never share a tool number.
fn merge_drill(
    tools: &mut Vec<PlotDrillTool>,
    hits: &mut Vec<PlotDrillHit>,
    new_tools: Vec<PlotDrillTool>,
    new_hits: Vec<PlotDrillHit>,
) {
    let offset = tools.iter().map(|t| t.number).max().unwrap_or(0);
    tools.extend(new_tools.into_iter().map(|mut t| {
        t.number += offset;
        t
    }));
    hits.extend(new_hits.into_iter().map(|mut h| {
        h.tool += offset;
        h
    }));
}

/// Load every plot/drill file in a folder.
pub fn load_plot_folder(path: &Path) -> Result<PlotPackage, String> {
    let mut layers = Vec::new();
    let mut drill_tools = Vec::new();
    let mut drill_hits = Vec::new();
    let mut hints = Vec::new();
    let mut entries: Vec<PathBuf> = std::fs::read_dir(path)
        .map_err(|e| format!("could not read folder: {e}"))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .collect();
    entries.sort();
    let drill_names: Vec<String> = entries
        .iter()
        .filter_map(|e| e.file_name().and_then(|n| n.to_str()).map(str::to_owned))
        .filter(|n| n.to_ascii_lowercase().ends_with(".drl"))
        .collect();
    for entry in entries {
        let Some(name) = entry.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !is_plot_name_ext(name) {
            continue;
        }
        let lower = name.to_ascii_lowercase();
        if lower.ends_with(".drl") {
            if skip_combined_drill(name, &drill_names) {
                continue;
            }
            let text = read_text(&entry)?;
            let (tools, hits) = parse_excellon_drill(&text)?;
            merge_drill(&mut drill_tools, &mut drill_hits, tools, hits);
            continue;
        }
        let text = read_text(&entry)?;
        let layer = parse_gerber_layer(&text, name)?;
        hints.extend(layer.hints.clone());
        layers.push(layer);
    }
    if !drill_hits.is_empty() {
        layers.push(drill_overlay_layer(&drill_hits));
    }
    if layers.is_empty() {
        return Err("folder holds no plot or drill files".into());
    }
    Ok(PlotPackage {
        layers,
        drill_tools,
        drill_hits,
        hints,
    })
}

fn drill_overlay_layer(hits: &[PlotDrillHit]) -> PlotLayer {
    PlotLayer {
        id: "Drill hits".into(),
        kind: PlotLayerKind::DrillOverlay,
        color: layer_color(PlotLayerKind::DrillOverlay),
        polarity_negative: false,
        apertures: Vec::new(),
        flashes: hits
            .iter()
            .filter(|h| h.slot_end.is_none())
            .map(|h| PlotFlash {
                x_mm: h.x_mm,
                y_mm: h.y_mm,
                aperture: h.tool,
            })
            .collect(),
        strokes: hits
            .iter()
            .filter_map(|h| {
                h.slot_end.map(|(ex, ey)| PlotStroke {
                    points: vec![(h.x_mm, h.y_mm), (ex, ey)],
                    aperture: h.tool,
                })
            })
            .collect(),
        regions: Vec::new(),
        region_clear: Vec::new(),
        flash_count: hits.iter().filter(|h| h.slot_end.is_none()).count(),
        draw_count: hits.iter().filter(|h| h.slot_end.is_some()).count(),
        hints: Vec::new(),
    }
}

/// Load plot files from a zip archive (fabrication package, not design archive).
pub fn load_plot_zip(bytes: &[u8]) -> Result<PlotPackage, String> {
    let eocd = bytes
        .windows(22)
        .rposition(|w| w.starts_with(b"PK\x05\x06"))
        .ok_or_else(|| "not a zip archive: no directory".to_string())?;
    let read_u16 = |at: usize| -> u16 { u16::from_le_bytes([bytes[at], bytes[at + 1]]) };
    let read_u32 = |at: usize| -> u32 {
        u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
    };
    let entries = read_u16(eocd + 10) as usize;
    let mut at = read_u32(eocd + 16) as usize;
    let mut collected: Vec<(String, Vec<u8>)> = Vec::new();
    let mut total: u64 = 0;
    for _ in 0..entries {
        if !bytes[at..].starts_with(b"PK\x01\x02") || at + 46 > bytes.len() {
            return Err("corrupt zip directory".into());
        }
        let size = read_u32(at + 20) as usize;
        let name_len = read_u16(at + 28) as usize;
        let extra_len = read_u16(at + 30) as usize;
        let comment_len = read_u16(at + 32) as usize;
        let local_at = read_u32(at + 42) as usize;
        if name_len == 0 || at + 46 + name_len > bytes.len() {
            return Err("corrupt zip directory entry".into());
        }
        let name_bytes = &bytes[at + 46..at + 46 + name_len];
        let name = safe_zip_name(name_bytes)?;
        if is_plot_name_ext(&name) {
            if local_at + 30 > bytes.len() || !bytes[local_at..].starts_with(b"PK\x03\x04") {
                return Err("corrupt zip entry header".into());
            }
            let local_name = read_u16(local_at + 26) as usize;
            let local_extra = read_u16(local_at + 28) as usize;
            if local_at + 30 + local_name > bytes.len() {
                return Err("corrupt zip entry header".into());
            }
            let data_at = local_at + 30 + local_name + local_extra;
            if data_at + size > bytes.len() {
                return Err("zip entry runs past end of file".into());
            }
            if size as u64 > MAX_FILE_BYTES {
                return Err("zip entry is too large".into());
            }
            total += size as u64;
            if total > MAX_PLOT_ZIP_BYTES {
                return Err("zip contents exceed the size budget".into());
            }
            collected.push((name, bytes[data_at..data_at + size].to_vec()));
        }
        at += 46 + name_len + extra_len + comment_len;
    }
    if collected.is_empty() {
        return Err("zip holds no plot or drill files".into());
    }
    let mut layers = Vec::new();
    let mut drill_tools = Vec::new();
    let mut drill_hits = Vec::new();
    let mut hints = Vec::new();
    collected.sort_by(|a, b| a.0.cmp(&b.0));
    let drill_names: Vec<String> = collected
        .iter()
        .map(|(n, _)| n.clone())
        .filter(|n| n.to_ascii_lowercase().ends_with(".drl"))
        .collect();
    for (name, data) in collected {
        if name.to_ascii_lowercase().ends_with(".drl") {
            if skip_combined_drill(&name, &drill_names) {
                continue;
            }
            let text =
                std::str::from_utf8(&data).map_err(|_| "drill file is not UTF-8".to_string())?;
            let (tools, hits) = parse_excellon_drill(text)?;
            merge_drill(&mut drill_tools, &mut drill_hits, tools, hits);
        } else {
            let text =
                std::str::from_utf8(&data).map_err(|_| "plot file is not UTF-8".to_string())?;
            let layer = parse_gerber_layer(text, &name)?;
            hints.extend(layer.hints.clone());
            layers.push(layer);
        }
    }
    if !drill_hits.is_empty() {
        layers.push(drill_overlay_layer(&drill_hits));
    }
    Ok(PlotPackage {
        layers,
        drill_tools,
        drill_hits,
        hints,
    })
}

/// Bounding box of every flash and stroke in a plot package, as (min_x, min_y, max_x, max_y) in millimetres.
///
/// # Rules
/// - Returns `None` when the package has no flashes and no strokes.
///
/// # Dependencies
/// - crates/io/src/plot_read.rs
pub fn layer_bounds(package: &PlotPackage) -> Option<(f64, f64, f64, f64)> {
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    let mut any = false;
    for layer in &package.layers {
        for f in &layer.flashes {
            any = true;
            min_x = min_x.min(f.x_mm);
            min_y = min_y.min(f.y_mm);
            max_x = max_x.max(f.x_mm);
            max_y = max_y.max(f.y_mm);
        }
        for s in &layer.strokes {
            for (x, y) in &s.points {
                any = true;
                min_x = min_x.min(*x);
                min_y = min_y.min(*y);
                max_x = max_x.max(*x);
                max_y = max_y.max(*y);
            }
        }
    }
    if any {
        Some((min_x, min_y, max_x, max_y))
    } else {
        None
    }
}

/// Straight-line distance between two board points in millimetres.
///
/// # Dependencies
/// - crates/geom/src/lib.rs
pub fn distance_mm(a: Point, b: Point) -> f64 {
    ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arcs_and_whole_number_coordinates_scale_to_millimetres() -> Result<(), String> {
        let text = "%FSLAX24Y24*%\n%MOMM*%\n%ADD10C,0.2*%\nG01*\nD10*\nX100000Y100000D02*\nX200000Y100000D01*\nG03*\nX300000Y200000I0J100000D01*\nD10*\nX50000Y50000D03*\nM02*\n";
        let layer = parse_gerber_layer(text, "arc.gbr")?;
        assert_eq!(layer.flash_count, 1);
        assert_eq!(layer.draw_count, 2);
        assert_eq!(layer.flashes[0].x_mm, 5.0);
        assert!(layer.strokes[1].points.len() > 3, "the arc is sampled");
        let last = layer.strokes[1].points.last().copied().unwrap_or_default();
        assert!((last.0 - 30.0).abs() < 1e-6 && (last.1 - 20.0).abs() < 1e-6);
        Ok(())
    }

    #[test]
    fn slot_lines_in_a_drill_file_read_as_spans() -> Result<(), String> {
        let text = "M48\nFMAT,2\nMETRIC,TZ\nT1C0.800\n%\nG90\nG05\nT1\nX1.000Y2.000\nX9.850Y10.000G85X10.150Y10.000\n";
        let (tools, hits) = parse_excellon_drill(text)?;
        assert_eq!(tools.len(), 1);
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[1].slot_end, Some((10.15, 10.0)));
        Ok(())
    }

    #[test]
    fn shaped_pads_fixture_has_x2_copper_top() -> Result<(), String> {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/shaped_pads_F_Cu.gbr");
        let text = read_text(&path)?;
        let layer = parse_gerber_layer(&text, "shaped_pads_F_Cu.gbr")?;
        assert_eq!(layer.kind, PlotLayerKind::FrontCopper);
        assert!(layer.flash_count >= 2);
        Ok(())
    }
}
