//! Fabrication drawing PDF: outline, stackup, drill chart, impedance and notes.
use pcbcraft_doc::{Layer, Project};
use pcbcraft_geom::impedance as imp;
use pcbcraft_geom::Point;
use std::collections::BTreeSet;

use crate::{drill_chart_csv, pdf_bytes_from_ops, stackup_text};

const DEFAULT_COPPER_MM: f64 = 0.035;
const PDF_MAX_OPS: usize = 20_000;
const INK: [f64; 3] = [0.05, 0.05, 0.05];

/// One-page fabrication drawing with extractable text for stackup, drills and revision.
pub fn fab_drawing_pdf(p: &Project) -> Result<Vec<u8>, String> {
    p.validate()?;
    let mut sheet = FabSheet::new();
    sheet.op("1 1 1 rg 0 0 842 595 re f")?;
    let title = project_title(p);
    sheet.text(40.0, 560.0, 14.0, &format!("Fabrication drawing: {title}"))?;
    draw_outline_panel(p, &mut sheet)?;
    let mut y = 540.0;
    y = write_section(p, &mut sheet, y, "Layer stackup", stackup_section_lines(p)?)?;
    y = write_section(p, &mut sheet, y, "Drill chart", drill_section_lines(p)?)?;
    y = write_section(p, &mut sheet, y, "Controlled impedance", impedance_section_lines(p)?)?;
    y = write_section(p, &mut sheet, y, "Fabrication notes", notes_section_lines(p)?)?;
    y = write_section(p, &mut sheet, y, "Board class", board_class_lines(p)?)?;
    let _ = write_section(
        p,
        &mut sheet,
        y.min(120.0),
        "Revision block",
        revision_block_lines(p),
    )?;
    pdf_bytes_from_ops(sheet.ops)
}

fn project_title(p: &Project) -> &str {
    if p.name.trim().is_empty() {
        "untitled board"
    } else {
        p.name.trim()
    }
}

fn write_section(
    _p: &Project,
    sheet: &mut FabSheet,
    mut y: f64,
    heading: &str,
    lines: Vec<String>,
) -> Result<f64, String> {
    if lines.is_empty() {
        return Ok(y);
    }
    if y < 80.0 {
        return Err("fabrication drawing does not fit on one page".into());
    }
    sheet.text(320.0, y, 11.0, heading)?;
    y -= 16.0;
    for line in lines {
        if y < 72.0 {
            return Err("fabrication drawing does not fit on one page".into());
        }
        sheet.text(328.0, y, 9.0, &line)?;
        y -= 11.0;
    }
    y -= 6.0;
    Ok(y)
}

fn stackup_section_lines(p: &Project) -> Result<Vec<String>, String> {
    if p.board.layer_records.is_empty() {
        return Ok(vec![]);
    }
    let raw = stackup_text(p)?;
    let mut out = Vec::new();
    let mut in_table = false;
    for line in raw.lines() {
        if line.starts_with("NAME,KIND,THICKNESS_MM") {
            in_table = true;
            out.push(line.to_string());
            continue;
        }
        if in_table {
            if line.is_empty() || line.starts_with("Total recorded") {
                if line.starts_with("Total recorded") {
                    out.push(line.to_string());
                }
                break;
            }
            out.push(line.to_string());
        }
    }
    if let Some(m) = &p.board.machining {
        if let Some(v) = m.drill_tolerance {
            out.push(format!("Drill tolerance +/-{v:.3} mm"));
        }
        if let Some(v) = m.edge_tolerance {
            out.push(format!("Edge tolerance +/-{v:.3} mm"));
        }
        if let Some(v) = m.edge_plating {
            out.push(format!("Edge plating: {}", if v { "yes" } else { "no" }));
        }
    }
    Ok(out)
}

fn drill_section_lines(p: &Project) -> Result<Vec<String>, String> {
    let csv = drill_chart_csv(p)?;
    let mut rows: Vec<String> = Vec::new();
    for line in csv.lines() {
        if line.starts_with("TOOL,") {
            rows.push(line.to_string());
        } else if line.starts_with('T') && line.contains(',') && !line.starts_with("TOTAL") {
            rows.push(line.to_string());
        }
    }
    if rows.len() <= 1 {
        return Ok(vec![]);
    }
    Ok(rows)
}

fn dielectric_for(p: &Project, layer: Layer) -> (f64, f64) {
    p.dielectric_beside(&p.layer_name(layer))
        .unwrap_or((p.board.thickness, p.board.er))
}

fn copper_thickness(p: &Project, layer: Layer) -> f64 {
    let name = p.layer_name(layer);
    p.board
        .layer_records
        .iter()
        .find(|r| r.name == name && r.kind == "copper" && r.thickness > 0.0)
        .map(|r| r.thickness)
        .unwrap_or(DEFAULT_COPPER_MM)
}

fn widest(p: &Project, net: u64) -> Option<(f64, Layer)> {
    p.board
        .tracks
        .iter()
        .filter(|t| t.net == net)
        .fold(None, |best: Option<(f64, Layer)>, t| match best {
            Some((w, _)) if w >= t.width => best,
            _ => Some((t.width, t.layer)),
        })
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

fn impedance_section_lines(p: &Project) -> Result<Vec<String>, String> {
    let mut lines = Vec::new();
    let mut seen_pairs: BTreeSet<String> = BTreeSet::new();
    for net in &p.nets {
        let Some((width, layer)) = widest(p, net.id) else {
            continue;
        };
        let copper = copper_thickness(p, layer);
        let (height, er) = dielectric_for(p, layer);
        let Some(m) = imp::microstrip(width, height, copper, er) else {
            continue;
        };
        let mut line = format!(
            "{} {} mm on {}: {:.2} ohm",
            net.name,
            round2(width),
            p.layer_name(layer),
            round2(m.ohms)
        );
        if let Some(pair) = p
            .board
            .diff_pairs
            .iter()
            .find(|d| d.net_p == net.id || d.net_n == net.id)
            && seen_pairs.insert(pair.name.clone())
            && let Some(c) = imp::coupled_microstrip(width, pair.gap, height, er)
        {
            line.push_str(&format!(", pair {} Zdiff {:.2} ohm", pair.name, round2(c.z_diff())));
        }
        lines.push(line);
    }
    Ok(lines)
}

fn notes_section_lines(p: &Project) -> Result<Vec<String>, String> {
    const REVISION_KEYS: &[&str] = &[
        "title", "revision", "date", "company", "drawn_by", "contact",
    ];
    let mut lines = Vec::new();
    if let Some(v) = p.meta.get("notes").filter(|s| !s.is_empty()) {
        lines.push(v.clone());
    }
    for (key, value) in &p.meta {
        if value.is_empty() || REVISION_KEYS.contains(&key.as_str()) {
            continue;
        }
        if key == "notes" || key == "ipc_class" || key == "impedance" {
            continue;
        }
        lines.push(format!("{key}: {value}"));
    }
    if let Some(v) = p.meta.get("impedance").filter(|s| !s.is_empty()) {
        lines.push(format!("Impedance notes: {v}"));
    }
    if p.board.finish.trim().len() > 0 {
        lines.push(format!("Surface finish: {}", p.board.finish.trim()));
    }
    Ok(lines)
}

fn board_class_lines(p: &Project) -> Result<Vec<String>, String> {
    let mut lines = Vec::new();
    if let Some(v) = p.meta.get("ipc_class").filter(|s| !s.is_empty()) {
        lines.push(format!("IPC class: {v}"));
    }
    for class in &p.net_classes {
        lines.push(format!(
            "Net class {}: clearance {:.3} mm, width {:.3} mm",
            class.name, class.clearance, class.width
        ));
    }
    Ok(lines)
}

fn page_settings(p: &Project) -> pcbcraft_doc::PageSettings {
    p.page_settings
        .iter()
        .find(|s| s.sheet == 0)
        .cloned()
        .unwrap_or_default()
}

fn meta_or_page<'a>(p: &'a Project, key: &str, page: &str) -> Option<&'a str> {
    if !page.is_empty() {
        return Some(page);
    }
    p.meta.get(key).map(|s| s.as_str()).filter(|s| !s.is_empty())
}

fn revision_block_lines(p: &Project) -> Vec<String> {
    let page = page_settings(p);
    let mut lines = vec![
        format!(
            "Title: {}",
            meta_or_page(p, "title", page.title.as_str())
                .unwrap_or(project_title(p))
        ),
        format!(
            "Revision: {}",
            meta_or_page(p, "revision", page.revision.as_str()).unwrap_or("-")
        ),
        format!(
            "Date: {}",
            meta_or_page(p, "date", page.date.as_str()).unwrap_or("-")
        ),
        format!(
            "Company: {}",
            meta_or_page(p, "company", page.company.as_str()).unwrap_or("-")
        ),
        format!(
            "Drawn by: {}",
            meta_or_page(p, "drawn_by", page.drawn_by.as_str()).unwrap_or("-")
        ),
    ];
    if let Some(v) = p.meta.get("contact").filter(|s| !s.is_empty()) {
        lines.push(format!("Contact: {v}"));
    }
    lines
}

fn board_edge(p: &Project) -> Vec<Point> {
    match &p.board.outline {
        Some(o) => o.clone(),
        None => vec![
            Point::new(0.0, 0.0),
            Point::new(p.board.width, 0.0),
            Point::new(p.board.width, p.board.height),
            Point::new(0.0, p.board.height),
        ],
    }
}

fn draw_outline_panel(p: &Project, sheet: &mut FabSheet) -> Result<(), String> {
    let edge = board_edge(p);
    let (min_x, max_x, min_y, max_y) = edge.iter().fold(
        (f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY),
        |(min_x, max_x, min_y, max_y), pt| {
            (
                min_x.min(pt.x),
                max_x.max(pt.x),
                min_y.min(pt.y),
                max_y.max(pt.y),
            )
        },
    );
    let design_w = (max_x - min_x).max(0.001);
    let design_h = (max_y - min_y).max(0.001);
    let panel_w = 260.0;
    let panel_h = 220.0;
    let x0 = 40.0;
    let y0 = 280.0;
    let scale = (panel_w / design_w).min(panel_h / design_h);
    let map = |pt: Point| -> (f64, f64) {
        (
            x0 + (pt.x - min_x) * scale,
            y0 + (pt.y - min_y) * scale,
        )
    };
    sheet.text(x0, y0 + panel_h + 14.0, 10.0, "Board outline")?;
    let mut pts: Vec<(f64, f64)> = edge.iter().map(|p| map(*p)).collect();
    if pts.len() >= 2 {
        pts.push(pts[0]);
        sheet.polyline(&pts, 0.6, INK, false)?;
    }
    for v in &p.board.vias {
        if v.drill > 0.0 {
            let (cx, cy) = map(v.position);
            sheet.circle(cx, cy, (v.drill * scale * 0.5).max(1.0), INK, false)?;
        }
    }
    for h in &p.board.holes {
        let center = if h.is_slot() {
            h.center()
        } else {
            h.position
        };
        let (cx, cy) = map(center);
        let r = (h.diameter * scale * 0.5).max(1.0);
        sheet.circle(cx, cy, r, INK, false)?;
    }
    let width_mm = design_w;
    let height_mm = design_h;
    sheet.text(x0, y0 - 12.0, 9.0, &format!("Width {width_mm:.3} mm"))?;
    sheet.text(x0, y0 - 24.0, 9.0, &format!("Height {height_mm:.3} mm"))?;
    Ok(())
}

struct FabSheet {
    ops: String,
    count: usize,
}

impl FabSheet {
    fn new() -> Self {
        Self {
            ops: String::new(),
            count: 0,
        }
    }
    fn n(v: f64) -> String {
        format!("{:.3}", v + 0.0)
    }
    fn escape(s: &str) -> String {
        let ascii: String = s
            .chars()
            .map(|c| {
                if c.is_ascii_graphic() || c == ' ' {
                    c
                } else {
                    '?'
                }
            })
            .collect();
        ascii
            .replace('\\', r"\\")
            .replace('(', r"\(")
            .replace(')', r"\)")
    }
    fn op(&mut self, text: &str) -> Result<(), String> {
        self.count += 1;
        if self.count > PDF_MAX_OPS {
            return Err("the fabrication drawing is too complex for one PDF page".into());
        }
        self.ops.push_str(text);
        self.ops.push('\n');
        Ok(())
    }
    fn text(&mut self, x: f64, y: f64, size: f64, value: &str) -> Result<(), String> {
        if value.is_empty() {
            return Ok(());
        }
        self.op(&format!(
            "BT /F1 {} Tf {} {} Td ({}) Tj ET",
            Self::n(size),
            Self::n(x),
            Self::n(y),
            Self::escape(value)
        ))
    }
    fn color(&mut self, rgb: [f64; 3], stroke: bool) -> Result<(), String> {
        let [r, g, b] = rgb;
        self.op(&format!(
            "{} {} {} {}",
            Self::n(r),
            Self::n(g),
            Self::n(b),
            if stroke { "RG" } else { "rg" }
        ))
    }
    fn polyline(&mut self, points: &[(f64, f64)], w: f64, rgb: [f64; 3], fill: bool) -> Result<(), String> {
        if points.len() < 2 {
            return Ok(());
        }
        self.color(rgb, true)?;
        self.color(rgb, false)?;
        self.op(&format!("{} w", Self::n(w)))?;
        let (x, y) = points[0];
        self.op(&format!("{} {} m", Self::n(x), Self::n(y)))?;
        for (x, y) in &points[1..] {
            self.op(&format!("{} {} l", Self::n(*x), Self::n(*y)))?;
        }
        self.op(if fill { "B" } else { "S" })
    }
    fn circle(&mut self, cx: f64, cy: f64, r: f64, rgb: [f64; 3], fill: bool) -> Result<(), String> {
        const C: f64 = 0.552_284_749_8;
        self.color(rgb, true)?;
        self.color(rgb, false)?;
        self.op("0.4 w")?;
        self.op(&format!("{} {} m", Self::n(cx - r), Self::n(cy)))?;
        let k = r * C;
        let arcs = [
            (cx - r, cy + k, cx - k, cy + r, cx, cy + r),
            (cx + k, cy + r, cx + r, cy + k, cx + r, cy),
            (cx + r, cy - k, cx + k, cy - r, cx, cy - r),
            (cx - k, cy - r, cx - r, cy - k, cx - r, cy),
        ];
        for (x1, y1, x2, y2, x3, y3) in arcs {
            self.op(&format!(
                "{} {} {} {} {} {} c",
                Self::n(x1),
                Self::n(y1),
                Self::n(x2),
                Self::n(y2),
                Self::n(x3),
                Self::n(y3)
            ))?;
        }
        self.op("h")?;
        self.op(if fill { "B" } else { "S" })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pcbcraft_doc::{LayerRecord, MachiningNotes, Net, Via};

    #[test]
    fn fab_drawing_pdf_lists_stackup_drills_and_revision() -> Result<(), String> {
        let mut project = Project::default();
        project.meta.insert("revision".into(), "B".into());
        project.meta.insert("company".into(), "Example Co".into());
        project.board.layer_records.push(LayerRecord {
            name: "L1".into(),
            kind: "copper".into(),
            thickness: 0.035,
            ..Default::default()
        });
        project.board.layer_records.push(LayerRecord {
            name: "core".into(),
            kind: "core".into(),
            thickness: 1.2,
            ..Default::default()
        });
        project.board.machining = Some(MachiningNotes {
            drill_tolerance: Some(0.05),
            ..Default::default()
        });
        project.nets.push(Net {
            id: 1,
            name: "SIG".into(),
            ..Default::default()
        });
        project.board.tracks.push(pcbcraft_doc::Track {
            id: 2,
            net: 1,
            start: Point::new(5.0, 5.0),
            end: Point::new(15.0, 5.0),
            width: 0.25,
            layer: pcbcraft_doc::Layer::Front,
            arc: None,
        });
        project.board.vias.push(Via {
            id: 3,
            net: 1,
            position: Point::new(10.0, 10.0),
            diameter: 0.8,
            drill: 0.3,
            from_layer: None,
            to_layer: None,
            covering: None,
        });
        project.validate()?;
        let pdf = fab_drawing_pdf(&project)?;
        let text = String::from_utf8(pdf).map_err(|_| "PDF uses ASCII")?;
        assert!(text.contains("L1,copper,0.035"), "stackup row is extractable");
        assert!(text.contains("T1,0.300"), "drill chart row is extractable");
        assert!(text.contains("Revision block"), "revision heading is present");
        assert!(text.contains("Revision: B"), "revision value is present");
        assert!(text.contains("SIG"), "impedance line names the net");
        Ok(())
    }

    #[test]
    fn fab_drawing_omits_empty_stackup_and_drill_sections() -> Result<(), String> {
        let project = Project::default();
        project.validate()?;
        let pdf = fab_drawing_pdf(&project)?;
        let text = String::from_utf8(pdf).map_err(|_| "PDF uses ASCII")?;
        assert!(!text.contains("Layer stackup"), "empty stackup is omitted");
        assert!(!text.contains("Drill chart"), "empty drill chart is omitted");
        assert!(text.contains("Revision block"), "revision block stays");
        Ok(())
    }
}
