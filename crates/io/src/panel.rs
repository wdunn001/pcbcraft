//! Manufacturing panel fabrication: duplicated board geometry, rails, breakaway
//! and tooling drills, panel fiducials, and per-copy pick-and-place rows.
use pcbcraft_doc::{
    Board, CopperZone, Cutout, Footprint, Hole, PanelCopyLayout, PanelLayout, PanelSeparation,
    PanelSettings, Project, Track, Via, panel_layout,
};
use pcbcraft_geom::Point;
use std::collections::BTreeMap;

const MOUSE_BITE_MM: f64 = 0.6;
const TOOLING_DRILL_MM: f64 = 2.0;
const PANEL_FIDUCIAL_MM: f64 = 1.0;
const EDGE_INSET_MM: f64 = 3.85;
const MOUSE_BITES_PER_TAB: usize = 6;

fn panel_settings(p: &Project) -> Result<&PanelSettings, String> {
    let settings = p
        .panel
        .as_ref()
        .ok_or("panel export needs panel settings on the project")?;
    if !settings.enabled {
        return Err("panel export is disabled; enable panel settings first".into());
    }
    settings.validate()?;
    Ok(settings)
}

fn offset_point(p: Point, dx: f64, dy: f64) -> Point {
    Point::new(p.x + dx, p.y + dy)
}

fn remap_id(base: u64, copy_index: usize) -> u64 {
    base.saturating_add((copy_index as u64 + 1).saturating_mul(10_000_000))
}

fn translate_track(t: &Track, dx: f64, dy: f64, copy_index: usize) -> Track {
    Track {
        id: remap_id(t.id, copy_index),
        start: offset_point(t.start, dx, dy),
        end: offset_point(t.end, dx, dy),
        ..t.clone()
    }
}

fn translate_via(v: &Via, dx: f64, dy: f64, copy_index: usize) -> Via {
    Via {
        id: remap_id(v.id, copy_index),
        position: offset_point(v.position, dx, dy),
        ..v.clone()
    }
}

fn translate_zone(z: &CopperZone, dx: f64, dy: f64, copy_index: usize) -> CopperZone {
    CopperZone {
        id: remap_id(z.id, copy_index),
        outline: z.outline.iter().map(|p| offset_point(*p, dx, dy)).collect(),
        cutouts: z
            .cutouts
            .iter()
            .map(|ring| ring.iter().map(|p| offset_point(*p, dx, dy)).collect())
            .collect(),
        ..z.clone()
    }
}

fn translate_footprint(f: &Footprint, dx: f64, dy: f64, copy_index: usize) -> Footprint {
    Footprint {
        id: remap_id(f.id, copy_index),
        position: offset_point(f.position, dx, dy),
        ..f.clone()
    }
}

fn translate_cutout(c: &Cutout, dx: f64, dy: f64, copy_index: usize) -> Cutout {
    Cutout {
        id: remap_id(c.id, copy_index),
        center: offset_point(c.center, dx, dy),
        ..c.clone()
    }
}

fn translate_hole(h: &Hole, dx: f64, dy: f64, copy_index: usize) -> Hole {
    Hole {
        id: remap_id(h.id, copy_index),
        position: offset_point(h.position, dx, dy),
        ..h.clone()
    }
}

/// Build a transient panel-space board by placing every copy with unique ids.
pub fn panel_merged_project(p: &Project, layout: &PanelLayout) -> Result<Project, String> {
    let mut out = p.clone();
    let board = &p.board;
    out.board = Board {
        width: layout.panel_width,
        height: layout.panel_height,
        outline: Some(vec![
            Point::new(0.0, 0.0),
            Point::new(layout.panel_width, 0.0),
            Point::new(layout.panel_width, layout.panel_height),
            Point::new(0.0, layout.panel_height),
            Point::new(0.0, 0.0),
        ]),
        tracks: Vec::new(),
        vias: Vec::new(),
        zones: Vec::new(),
        footprints: Vec::new(),
        cutouts: Vec::new(),
        holes: Vec::new(),
        fiducials: Vec::new(),
        ..board.clone()
    };
    for (copy_index, copy) in layout.copies.iter().enumerate() {
        for t in &board.tracks {
            out.board
                .tracks
                .push(translate_track(t, copy.x, copy.y, copy_index));
        }
        for v in &board.vias {
            out.board
                .vias
                .push(translate_via(v, copy.x, copy.y, copy_index));
        }
        for z in &board.zones {
            out.board
                .zones
                .push(translate_zone(z, copy.x, copy.y, copy_index));
        }
        for f in &board.footprints {
            out.board
                .footprints
                .push(translate_footprint(f, copy.x, copy.y, copy_index));
        }
        for c in &board.cutouts {
            out.board
                .cutouts
                .push(translate_cutout(c, copy.x, copy.y, copy_index));
        }
        for h in &board.holes {
            out.board
                .holes
                .push(translate_hole(h, copy.x, copy.y, copy_index));
        }
    }
    out.validate()?;
    Ok(out)
}

struct MechanicalHit {
    diameter_mm: f64,
    x: f64,
    y: f64,
    kind: &'static str,
}

fn tab_breakaway_holes(layout: &PanelLayout) -> Vec<MechanicalHit> {
    let s = &layout.settings;
    if s.separation != PanelSeparation::TabRouting {
        return Vec::new();
    }
    let board_w = layout.board_width;
    let board_h = layout.board_height;
    let mut hits = Vec::new();
    let bite_spacing = s.pitch_x.max(s.pitch_y) / (MOUSE_BITES_PER_TAB as f64 + 1.0);
    for col in 0..s.columns.saturating_sub(1) {
        let x = s.rail + (col + 1) as f64 * board_w + col as f64 * s.pitch_x + s.pitch_x / 2.0;
        for row in 0..s.rows {
            let y0 = s.rail + row as f64 * (board_h + s.pitch_y);
            let y1 = y0 + board_h;
            for i in 0..MOUSE_BITES_PER_TAB {
                let t = (i + 1) as f64 * bite_spacing;
                hits.push(MechanicalHit {
                    diameter_mm: MOUSE_BITE_MM,
                    x,
                    y: y0 + t.min(board_h),
                    kind: "Breakout",
                });
                hits.push(MechanicalHit {
                    diameter_mm: MOUSE_BITE_MM,
                    x,
                    y: y1 - t.min(board_h),
                    kind: "Breakout",
                });
            }
        }
    }
    for row in 0..s.rows.saturating_sub(1) {
        let y = s.rail + (row + 1) as f64 * board_h + row as f64 * s.pitch_y + s.pitch_y / 2.0;
        for col in 0..s.columns {
            let x0 = s.rail + col as f64 * (board_w + s.pitch_x);
            let x1 = x0 + board_w;
            for i in 0..MOUSE_BITES_PER_TAB {
                let t = (i + 1) as f64 * bite_spacing;
                hits.push(MechanicalHit {
                    diameter_mm: MOUSE_BITE_MM,
                    x: x0 + t.min(board_w),
                    y,
                    kind: "Breakout",
                });
                hits.push(MechanicalHit {
                    diameter_mm: MOUSE_BITE_MM,
                    x: x1 - t.min(board_w),
                    y,
                    kind: "Breakout",
                });
            }
        }
    }
    hits
}

fn tooling_holes(layout: &PanelLayout) -> Vec<MechanicalHit> {
    let inset = EDGE_INSET_MM;
    let w = layout.panel_width;
    let h = layout.panel_height;
    [
        (inset, inset),
        (w - inset, inset),
        (inset, h - inset),
        (w - inset, h - inset),
    ]
    .into_iter()
    .map(|(x, y)| MechanicalHit {
        diameter_mm: TOOLING_DRILL_MM,
        x,
        y,
        kind: "Tooling",
    })
    .collect()
}

fn panel_fiducial_centers(layout: &PanelLayout) -> [(f64, f64); 3] {
    let w = layout.panel_width;
    let h = layout.panel_height;
    let inset = EDGE_INSET_MM;
    [(inset, inset), (w - inset, inset), (inset, h - inset)]
}

fn inject_gerber_x2_part_array(text: &str) -> String {
    if text.contains("%TF.Part,Array*%") {
        return text.to_string();
    }
    let mut lines: Vec<&str> = text.lines().collect();
    if lines.is_empty() {
        return text.to_string();
    }
    lines.insert(1, "%TF.Part,Array*%");
    format!("{}\n", lines.join("\n"))
}

fn inject_fiducial_flashes(
    front: &str,
    layout: &PanelLayout,
    scale: f64,
) -> Result<String, String> {
    let d = PANEL_FIDUCIAL_MM;
    let ap_line = format!("%ADD99C,{:.6}*%\n", d * scale);
    let attr = "%TA.AperFunction,FiducialPad,Panel*%\n";
    let mut body = String::new();
    for (x, y) in panel_fiducial_centers(layout) {
        body.push_str(&format!("D99X{:.6}Y{:.6}D03*\n", x * scale, y * scale));
    }
    if front.contains("%ADD99C,") {
        return Ok(format!("{front}{body}"));
    }
    let marker = "%LPD*%\nG01*\n";
    let Some(idx) = front.find(marker) else {
        return Err("panel copper file is missing the plot header".into());
    };
    let mut out = String::new();
    out.push_str(&front[..idx]);
    out.push_str(attr);
    out.push_str(&ap_line);
    out.push_str(marker);
    out.push_str(&front[idx + marker.len()..]);
    out.push_str(&body);
    Ok(out)
}

fn append_mechanical_drills(
    drill: &str,
    hits: &[MechanicalHit],
    scale: f64,
    prec: usize,
) -> Result<String, String> {
    if hits.is_empty() {
        return Ok(drill.to_string());
    }
    let end = drill
        .rfind("M30")
        .ok_or("panel drill file is missing the program end")?;
    let (head, tail) = drill.split_at(end);
    let mut tool_index = head
        .lines()
        .filter(|line| {
            let t = line.trim();
            t.starts_with('T') && t.contains('C') && !t.contains('X')
        })
        .count();
    let mut by_diameter: BTreeMap<String, Vec<&MechanicalHit>> = BTreeMap::new();
    for hit in hits {
        by_diameter
            .entry(format!("{:.prec$}", hit.diameter_mm * scale, prec = prec))
            .or_default()
            .push(hit);
    }
    let pct = head
        .find("%\n")
        .ok_or("panel drill file is missing the tool table end")?;
    let (header, rest) = head.split_at(pct);
    let mut tool_defs = String::new();
    let mut tool_map: BTreeMap<String, usize> = BTreeMap::new();
    for (diam, group) in &by_diameter {
        tool_index += 1;
        tool_map.insert(diam.clone(), tool_index);
        let kind = group.first().map(|h| h.kind).unwrap_or("Mechanical");
        tool_defs.push_str(&format!(
            "T{tool_index}C{diam}\nG04 MechanicalDrill,{kind}*\n"
        ));
    }
    let mut body = String::new();
    for (diam, group) in &by_diameter {
        let Some(n) = tool_map.get(diam) else {
            continue;
        };
        body.push_str(&format!("T{n}\n"));
        for hit in group {
            body.push_str(&format!(
                "X{:.prec$}Y{:.prec$}\n",
                hit.x * scale,
                hit.y * scale,
                prec = prec
            ));
        }
    }
    Ok(format!("{header}{tool_defs}{rest}{body}{tail}"))
}

fn copy_edge_points(
    p: &Project,
    copy: &PanelCopyLayout,
    board_w: f64,
    board_h: f64,
) -> Vec<(f64, f64)> {
    match &p.board.outline {
        Some(o) => o.iter().map(|pt| (pt.x + copy.x, pt.y + copy.y)).collect(),
        None => vec![
            (copy.x, copy.y),
            (copy.x + board_w, copy.y),
            (copy.x + board_w, copy.y + board_h),
            (copy.x, copy.y + board_h),
        ],
    }
}

fn append_copy_outlines(edge: &str, p: &Project, layout: &PanelLayout, scale: f64) -> String {
    let marker = "M02*\n";
    let Some(idx) = edge.find(marker) else {
        return edge.to_string();
    };
    let mut extra = String::new();
    for copy in &layout.copies {
        let pts = copy_edge_points(p, copy, layout.board_width, layout.board_height);
        if pts.len() < 2 {
            continue;
        }
        extra.push_str("D10D02*\n");
        for (i, (x, y)) in pts.iter().enumerate() {
            extra.push_str(&format!(
                "X{:.6}Y{:.6}D0{}*\n",
                x * scale,
                y * scale,
                if i == 0 { 2 } else { 1 }
            ));
        }
        if let Some((x, y)) = pts.first() {
            extra.push_str(&format!("X{:.6}Y{:.6}D01*\n", x * scale, y * scale));
        }
    }
    format!("{}{}{}", &edge[..idx], extra, &edge[idx..])
}

fn vcut_gerber(layout: &PanelLayout, scale: f64) -> String {
    let s = &layout.settings;
    let board_w = layout.board_width;
    let board_h = layout.board_height;
    let mut body = String::new();
    for col in 0..s.columns.saturating_sub(1) {
        let x = s.rail + (col + 1) as f64 * board_w + col as f64 * s.pitch_x + s.pitch_x / 2.0;
        body.push_str(&format!(
            "D10D02*\nX{:.6}Y{:.6}D02*\nX{:.6}Y{:.6}D01*\n",
            x * scale,
            0.0,
            x * scale,
            layout.panel_height * scale
        ));
    }
    for row in 0..s.rows.saturating_sub(1) {
        let y = s.rail + (row + 1) as f64 * board_h + row as f64 * s.pitch_y + s.pitch_y / 2.0;
        body.push_str(&format!(
            "D10D02*\nX{:.6}Y{:.6}D02*\nX{:.6}Y{:.6}D01*\n",
            0.0,
            y * scale,
            layout.panel_width * scale,
            y * scale
        ));
    }
    format!(
        "G04 V-score layer generated by PCBCraft *\n%TF.FileFunction,Vcut*%\n%FSLANX*XAYAMOMM*%\n%ADD10C,0.050000*%\n%LPD*%\nG01*\n{body}M02*\n"
    )
}

fn rename_panel_files(files: BTreeMap<String, String>) -> BTreeMap<String, String> {
    files
        .into_iter()
        .map(|(k, v)| (k.replacen("pcb-", "panel-", 1), v))
        .collect()
}

/// Full panel fabrication set: duplicated gerbers, drill with breakaway and
/// tooling hits, optional V-cut layer, and panel pick-and-place text.
pub fn panel_fabrication_files(p: &Project) -> Result<BTreeMap<String, String>, String> {
    let settings = panel_settings(p)?;
    let layout = panel_layout(p, settings)?;
    let merged = panel_merged_project(p, &layout)?;
    let inch = p.unit == "inch";
    let scale = if inch { 1.0 / 25.4 } else { 1.0 };
    let drill_prec = if inch { 4 } else { 3 };
    let mut files = rename_panel_files(super::gerber_files(&merged)?);
    let mechanical: Vec<MechanicalHit> = tab_breakaway_holes(&layout)
        .into_iter()
        .chain(tooling_holes(&layout))
        .collect();
    if let Some(drill) = files.get_mut("panel-Drill.drl") {
        *drill = append_mechanical_drills(drill, &mechanical, scale, drill_prec)?;
    }
    if let Some(front) = files.get("panel-F.Cu.gbr") {
        let patched = inject_fiducial_flashes(front, &layout, scale)?;
        files.insert(
            "panel-F.Cu.gbr".into(),
            inject_gerber_x2_part_array(&patched),
        );
    }
    if let Some(back) = files.get_mut("panel-B.Cu.gbr") {
        *back = inject_gerber_x2_part_array(back);
    }
    if let Some(edge) = files.get("panel-Edge.gbr") {
        files.insert(
            "panel-Edge.gbr".into(),
            append_copy_outlines(edge, p, &layout, scale),
        );
    }
    if settings.separation == PanelSeparation::VScore {
        files.insert("panel-Vcut.gbr".into(), vcut_gerber(&layout, scale));
    }
    Ok(files)
}

/// Pick-and-place rows for every panel copy with `{reference}_{index}` names.
pub fn panel_placement_text(p: &Project) -> Result<String, String> {
    panel_settings(p)?;
    let settings = p.panel.as_ref().ok_or("panel settings missing")?;
    let layout = panel_layout(p, settings)?;
    p.validate()?;
    const MAX_PLACED: usize = 5_000;
    let refs: BTreeMap<u64, &pcbcraft_doc::Component> =
        p.components.iter().map(|c| (c.id, c)).collect();
    if p.board.footprints.len().saturating_mul(layout.copies.len()) > MAX_PLACED {
        return Err("panel placement file exceeds the placed-object limit".into());
    }
    let mut rows: Vec<String> = Vec::new();
    for (copy_index, copy) in layout.copies.iter().enumerate() {
        let suffix = copy_index + 1;
        for f in &p.board.footprints {
            let c = refs
                .get(&f.component)
                .ok_or("footprint references an unknown component")?;
            let back = f.pads.iter().any(|pad| {
                pad.geometry.as_ref().is_some_and(|g| {
                    g.pad_type == "smd"
                        && !g.layers.is_empty()
                        && !g.layers.iter().any(|l| l.starts_with("F."))
                })
            });
            let tht = f.pads.iter().any(|pad| {
                pad.geometry
                    .as_ref()
                    .is_some_and(|g| g.pad_type == "thru_hole")
            });
            let package = match &c.library_footprint {
                Some(footprint) => footprint.definition.name.clone(),
                None => c.kind.clone(),
            };
            let reference = format!("{}_{suffix}", c.reference);
            rows.push(format!(
                "{},\"{}\",{:.3},{:.3},{:.1},{},\"{}\"",
                if tht { "THT" } else { "SMT" },
                reference,
                f.position.x + copy.x,
                f.position.y + copy.y,
                f.rotation.rem_euclid(360.0),
                if back { "B" } else { "T" },
                package
            ));
        }
    }
    fn reference_key(row: &str) -> &str {
        row.split('"').nth(1).unwrap_or("")
    }
    rows.sort_by(|a, b| reference_key(a).cmp(reference_key(b)));
    Ok(format!(
        "File Format,ASCII,\n#\n#\tPick and Place File\t{}\n#\n#\tPanel copies: {} x {}\n#\nSMT,THT,X,Y,Rotation,Side,Package\n{}\n",
        p.name,
        layout.settings.columns,
        layout.settings.rows,
        rows.join("\n")
    ))
}

pub fn count_excellon_hits(drill: &str) -> usize {
    drill
        .lines()
        .filter(|line| {
            let t = line.trim();
            (t.starts_with('X') || t.contains('X') && t.contains('Y'))
                && !t.starts_with("M48")
                && !t.contains('C')
                && t != "%"
        })
        .filter(|line| line.contains('Y') && !line.starts_with("G"))
        .count()
}
