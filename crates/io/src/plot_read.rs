//! Read fabrication plot and drill files for inspection (bounded, read-only).

use pcbcraft_geom::Point;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::{MAX_FILE_BYTES, read_text};

/// Total extracted bytes allowed when reading a plot zip (same budget as one file).
pub const MAX_PLOT_ZIP_BYTES: u64 = MAX_FILE_BYTES;

const MAX_OPS: usize = 100_000;
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
    pub flash_count: usize,
    pub draw_count: usize,
    pub hints: Vec<PlotHint>,
}

#[derive(Clone, Debug)]
pub struct PlotDrillTool {
    pub number: u32,
    pub diameter_mm: f64,
    pub plated: Option<bool>,
    pub hit_count: usize,
}

#[derive(Clone, Debug)]
pub struct PlotDrillHit {
    pub tool: u32,
    pub x_mm: f64,
    pub y_mm: f64,
    pub slot_end: Option<(f64, f64)>,
}

#[derive(Clone, Debug)]
pub struct PlotPackage {
    pub layers: Vec<PlotLayer>,
    pub drill_tools: Vec<PlotDrillTool>,
    pub drill_hits: Vec<PlotDrillHit>,
    pub hints: Vec<PlotHint>,
}

fn gerber_statements(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for ch in text.chars() {
        if ch == '*' {
            if !cur.is_empty() {
                out.push(cur.clone());
                cur.clear();
            }
        } else if ch != '\r' {
            cur.push(ch);
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur);
    }
    out
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
    let mut flashes = 0usize;
    let mut draws = 0usize;
    for stmt in gerber_statements(text) {
        let s = stmt.trim();
        if s.is_empty() || s.starts_with('G') && !s.contains('D') {
            continue;
        }
        if s.contains("D03") {
            flashes += 1;
        } else if s.contains("D01") {
            draws += 1;
        }
    }
    Ok((flashes, draws))
}

/// Parse one gerber layer file into counts and drawable geometry.
pub fn parse_gerber_layer(text: &str, source_name: &str) -> Result<PlotLayer, String> {
    let mut inch = text.contains("MOIN") || text.contains("%MOIN");
    if text.contains("MOMM") || text.contains("%MOMM") {
        inch = false;
    }
    let mut kind = PlotLayerKind::Unknown;
    let mut polarity_negative = false;
    let mut saw_x2 = false;
    let mut hints = Vec::new();
    let mut apertures: BTreeMap<u32, PlotAperture> = BTreeMap::new();
    let mut flashes = Vec::new();
    let mut strokes = Vec::new();
    let mut regions = Vec::new();
    let mut current_ap: u32 = 10;
    let mut ops = 0usize;
    let mut region_pts: Vec<(f64, f64)> = Vec::new();
    let mut in_region = false;
    let mut pending: Option<(f64, f64)> = None;
    let mut stroke_pts: Vec<(f64, f64)> = Vec::new();

    for stmt in gerber_statements(text) {
        let s = stmt.trim();
        if s.is_empty() {
            continue;
        }
        if let Some(rest) = s.strip_prefix("%TF.FileFunction,") {
            saw_x2 = true;
            kind = classify_from_x2(rest);
            continue;
        }
        if s.starts_with("%TF.FilePolarity,Negative") {
            polarity_negative = true;
            saw_x2 = true;
            continue;
        }
        if s.starts_with("%TF.") {
            saw_x2 = true;
            continue;
        }
        if s.starts_with("%TA.AperFunction,") {
            saw_x2 = true;
            if let Some(ap) = apertures.get_mut(&current_ap) {
                ap.function = Some(s.strip_prefix("%TA.AperFunction,").unwrap_or("").into());
            }
            continue;
        }
        if s.starts_with("%ADD") {
            if apertures.len() >= MAX_APERTURES {
                return Err("plot layer exceeds the aperture limit".into());
            }
            if let Some((code, ap)) = parse_add_aperture(s, inch) {
                apertures.insert(code, ap);
            }
            continue;
        }
        if s.starts_with("G36") {
            in_region = true;
            region_pts.clear();
            stroke_pts.clear();
            continue;
        }
        if s.starts_with("G37") {
            in_region = false;
            if region_pts.len() >= 3 {
                regions.push(region_pts.clone());
            }
            ops += 1;
            if ops > MAX_OPS {
                return Err("plot layer exceeds the object limit".into());
            }
            continue;
        }
        if let Some(d_only) = s.strip_prefix('D')
            && d_only.len() <= 3
            && d_only.chars().all(|c| c.is_ascii_digit())
            && let Ok(code) = d_only.parse::<u32>()
        {
            current_ap = code;
            continue;
        }
        if let Some((x, y)) = parse_coord_pair(s) {
            let (x_mm, y_mm) = inch_to_mm(inch, x, y);
            if s.contains("D03") {
                ops += 1;
                if ops > MAX_OPS {
                    return Err("plot layer exceeds the object limit".into());
                }
                flashes.push(PlotFlash {
                    x_mm,
                    y_mm,
                    aperture: current_ap,
                });
                pending = Some((x_mm, y_mm));
            } else if s.contains("D02") {
                stroke_pts.clear();
                stroke_pts.push((x_mm, y_mm));
                pending = Some((x_mm, y_mm));
            } else if s.contains("D01") {
                ops += 1;
                if ops > MAX_OPS {
                    return Err("plot layer exceeds the object limit".into());
                }
                if stroke_pts.is_empty() && let Some(p) = pending {
                    stroke_pts.push(p);
                }
                stroke_pts.push((x_mm, y_mm));
                if in_region {
                    region_pts.push((x_mm, y_mm));
                } else if stroke_pts.len() >= 2 {
                    strokes.push(PlotStroke {
                        points: stroke_pts.clone(),
                        aperture: current_ap,
                    });
                    stroke_pts.clear();
                    stroke_pts.push((x_mm, y_mm));
                }
                pending = Some((x_mm, y_mm));
            }
        } else if s.contains('D')
            && s.len() < 8
            && let Some(num) = s.strip_prefix('D').and_then(|t| t.parse::<u32>().ok())
        {
            current_ap = num;
        }
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
        if let Some(rest) = line.strip_prefix('T') && let Ok(n) = rest.parse::<u32>() {
            current_tool = n;
            continue;
        }
        if line.starts_with('X') {
            if hits.len() >= MAX_HITS {
                return Err("drill file exceeds the hit limit".into());
            }
            if let Some(g85) = line.find("G85") {
                let start = parse_coord_pair(line)
                    .ok_or_else(|| "drill slot start coordinates are missing".to_string())?;
                let end_part = &line[g85 + 3..];
                let end = parse_coord_pair(end_part)
                    .ok_or_else(|| "drill slot end coordinates are missing".to_string())?;
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
    for entry in entries {
        let Some(name) = entry.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !is_plot_name_ext(name) {
            continue;
        }
        let lower = name.to_ascii_lowercase();
        if lower.ends_with(".drl") {
            let text = read_text(&entry)?;
            let (tools, hits) = parse_excellon_drill(&text)?;
            drill_tools = tools;
            drill_hits = hits;
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
    for (name, data) in collected {
        if name.to_ascii_lowercase().ends_with(".drl") {
            let text =
                std::str::from_utf8(&data).map_err(|_| "drill file is not UTF-8".to_string())?;
            let (tools, hits) = parse_excellon_drill(text)?;
            drill_tools = tools;
            drill_hits = hits;
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

pub fn distance_mm(a: Point, b: Point) -> f64 {
    ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

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
