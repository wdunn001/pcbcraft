//! Panel fabrication export: duplicated board artwork, mechanical holes and placement.

use pcbcraft_doc::{
    Board, CopperZone, Cutout, Fiducial, Footprint, Hole, Layer, PanelLayout, PanelSeparation,
    PanelSettings, Project, Track, Via, panel_layout,
};
use pcbcraft_geom::Point;
use std::collections::BTreeMap;

const MOUSE_BITE_DIAMETER: f64 = 0.6;
const TOOLING_DIAMETER: f64 = 3.0;
const PANEL_FIDUCIAL_DIAMETER: f64 = 1.0;

fn translate_point(p: Point, dx: f64, dy: f64) -> Point {
    Point::new(p.x + dx, p.y + dy)
}

fn remap_id(id: u64, copy_index: usize) -> u64 {
    id.wrapping_add((copy_index as u64).wrapping_mul(10_000_000))
}

fn translate_footprint(f: &Footprint, dx: f64, dy: f64, copy_index: usize) -> Footprint {
    let mut out = f.clone();
    out.id = remap_id(f.id, copy_index);
    out.position = translate_point(f.position, dx, dy);
    out
}

fn translate_track(t: &Track, dx: f64, dy: f64, copy_index: usize) -> Track {
    let mut out = t.clone();
    out.id = remap_id(t.id, copy_index);
    out.start = translate_point(t.start, dx, dy);
    out.end = translate_point(t.end, dx, dy);
    out
}

fn translate_via(v: &Via, dx: f64, dy: f64, copy_index: usize) -> Via {
    let mut out = v.clone();
    out.id = remap_id(v.id, copy_index);
    out.position = translate_point(v.position, dx, dy);
    out
}

fn translate_hole(h: &Hole, dx: f64, dy: f64, copy_index: usize) -> Hole {
    let mut out = h.clone();
    out.id = remap_id(h.id, copy_index);
    out.position = translate_point(h.position, dx, dy);
    if let Some(end) = h.end {
        out.end = Some(translate_point(end, dx, dy));
    }
    out
}

fn translate_cutout(c: &Cutout, dx: f64, dy: f64, copy_index: usize) -> Cutout {
    let mut out = c.clone();
    out.id = remap_id(c.id, copy_index);
    out.center = translate_point(c.center, dx, dy);
    out
}

fn translate_zone(z: &CopperZone, dx: f64, dy: f64, copy_index: usize) -> CopperZone {
    let mut out = z.clone();
    out.id = remap_id(z.id, copy_index);
    out.outline = z
        .outline
        .iter()
        .map(|p| translate_point(*p, dx, dy))
        .collect();
    out.cutouts = z
        .cutouts
        .iter()
        .map(|ring| ring.iter().map(|p| translate_point(*p, dx, dy)).collect())
        .collect();
    out
}

fn add_panel_fiducials(board: &mut Board, layout: &PanelLayout, next_id: &mut u64) {
    let edge = 3.85;
    let positions = [
        (edge, edge),
        (layout.panel_width - edge, edge),
        (edge, layout.panel_height - edge),
    ];
    for (x, y) in positions {
        *next_id += 1;
        board.fiducials.push(Fiducial {
            id: *next_id,
            position: Point::new(x, y),
            diameter: PANEL_FIDUCIAL_DIAMETER,
            layer: Layer::Front,
        });
    }
}

fn add_tooling_holes(board: &mut Board, layout: &PanelLayout, next_id: &mut u64) {
    let inset = layout.rail / 2.0;
    let positions = [
        (inset, inset),
        (layout.panel_width - inset, inset),
        (inset, layout.panel_height - inset),
        (layout.panel_width - inset, layout.panel_height - inset),
    ];
    for (x, y) in positions {
        *next_id += 1;
        board.holes.push(Hole {
            id: *next_id,
            position: Point::new(x, y),
            diameter: TOOLING_DIAMETER,
            end: None,
        });
    }
}

fn add_mouse_bite_holes(
    board: &mut Board,
    layout: &PanelLayout,
    settings: &PanelSettings,
    next_id: &mut u64,
) {
    if settings.separation != PanelSeparation::Tab {
        return;
    }
    let step = 2.5_f64;
    for col in 0..layout.columns.saturating_sub(1) {
        let x = layout.rail
            + (col + 1) as f64 * layout.board_width
            + col as f64 * layout.pitch_x
            + layout.pitch_x / 2.0;
        let mut y = layout.rail;
        while y
            <= layout.rail
                + layout.rows as f64 * layout.board_height
                + (layout.rows - 1) as f64 * layout.pitch_y
        {
            *next_id += 1;
            board.holes.push(Hole {
                id: *next_id,
                position: Point::new(x, y),
                diameter: MOUSE_BITE_DIAMETER,
                end: None,
            });
            y += step;
        }
    }
    for row in 0..layout.rows.saturating_sub(1) {
        let y = layout.rail
            + (row + 1) as f64 * layout.board_height
            + row as f64 * layout.pitch_y
            + layout.pitch_y / 2.0;
        let mut x = layout.rail;
        while x
            <= layout.rail
                + layout.columns as f64 * layout.board_width
                + (layout.columns - 1) as f64 * layout.pitch_x
        {
            *next_id += 1;
            board.holes.push(Hole {
                id: *next_id,
                position: Point::new(x, y),
                diameter: MOUSE_BITE_DIAMETER,
                end: None,
            });
            x += step;
        }
    }
}

pub(crate) fn build_panel_project(
    p: &Project,
    layout: &PanelLayout,
    settings: &PanelSettings,
) -> Result<Project, String> {
    let mut out = p.clone();
    out.board.footprints.clear();
    out.board.tracks.clear();
    out.board.vias.clear();
    out.board.holes.clear();
    out.board.cutouts.clear();
    out.board.zones.clear();
    out.board.fiducials.clear();
    out.board.outline = None;
    out.board.outline_arcs.clear();
    out.board.width = layout.panel_width;
    out.board.height = layout.panel_height;
    out.board.drill_origin = Point::new(0.0, 0.0);

    for copy in &layout.copies {
        if copy.index > 1 {
            for c in &p.components {
                let mut clone = c.clone();
                clone.id = remap_id(c.id, copy.index);
                clone.reference = format!("{}_{}", c.reference, copy.index);
                out.components.push(clone);
            }
        }
        let dx = copy.x;
        let dy = copy.y;
        for f in &p.board.footprints {
            let mut fp = translate_footprint(f, dx, dy, copy.index);
            if copy.index > 1 {
                fp.component = remap_id(f.component, copy.index);
            }
            out.board.footprints.push(fp);
        }
        for t in &p.board.tracks {
            out.board
                .tracks
                .push(translate_track(t, dx, dy, copy.index));
        }
        for v in &p.board.vias {
            out.board.vias.push(translate_via(v, dx, dy, copy.index));
        }
        for h in &p.board.holes {
            out.board.holes.push(translate_hole(h, dx, dy, copy.index));
        }
        for c in &p.board.cutouts {
            out.board
                .cutouts
                .push(translate_cutout(c, dx, dy, copy.index));
        }
        for z in &p.board.zones {
            out.board.zones.push(translate_zone(z, dx, dy, copy.index));
        }
    }

    let mut next_id = 900_000_000_u64;
    add_panel_fiducials(&mut out.board, layout, &mut next_id);
    add_tooling_holes(&mut out.board, layout, &mut next_id);
    add_mouse_bite_holes(&mut out.board, layout, settings, &mut next_id);
    let _ = settings.tab_channel;
    out.validate()?;
    Ok(out)
}

fn inject_part_array_header(content: &mut String) {
    if content.contains("TF.Part,Array") {
        return;
    }
    if let Some(pos) = content.find("%LPD*%") {
        content.insert_str(pos, "%TF.Part,Array*%\n");
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum DrillRole {
    Plated,
    Breakout,
    Tooling,
}

struct PanelDrillHit {
    d_um: u64,
    x: f64,
    y: f64,
    role: DrillRole,
}

fn panel_drill_file(p: &Project, layout: &PanelLayout) -> Result<String, String> {
    let panel = build_panel_project(p, layout, &p.panel)?;
    let inch = p.unit == "inch";
    let scale = if inch { 1.0 / 25.4 } else { 1.0 };
    let (drill_header, drill_prec) = if inch {
        ("INCH,TZ", 4)
    } else {
        ("METRIC,TZ", 3)
    };

    let mut hits: Vec<PanelDrillHit> = Vec::new();
    for v in &panel.board.vias {
        if v.drill > 0.0 {
            hits.push(PanelDrillHit {
                d_um: (v.drill * 1_000.0).round() as u64,
                x: v.position.x,
                y: v.position.y,
                role: DrillRole::Plated,
            });
        }
    }
    for f in &panel.board.footprints {
        for pad in &f.pads {
            let Some(g) = pad.geometry.as_ref() else {
                continue;
            };
            let Some(drill) = g.drill.as_ref() else {
                continue;
            };
            let narrow = drill.size.x.min(drill.size.y);
            if narrow <= 0.0 {
                continue;
            }
            let local = pad.offset.offset(drill.offset.rotate(-g.rotation));
            let world = f.position.offset(local.rotate(-f.rotation));
            hits.push(PanelDrillHit {
                d_um: (narrow * 1_000.0).round() as u64,
                x: world.x,
                y: world.y,
                role: DrillRole::Plated,
            });
        }
    }
    for h in &panel.board.holes {
        let role = if (h.diameter - MOUSE_BITE_DIAMETER).abs() < 0.05 {
            DrillRole::Breakout
        } else if (h.diameter - TOOLING_DIAMETER).abs() < 0.05 {
            DrillRole::Tooling
        } else {
            DrillRole::Plated
        };
        hits.push(PanelDrillHit {
            d_um: (h.diameter * 1_000.0).round() as u64,
            x: h.position.x,
            y: h.position.y,
            role,
        });
    }

    hits.sort_by(|a, b| {
        a.d_um
            .cmp(&b.d_um)
            .then(a.role.cmp(&b.role))
            .then(a.x.total_cmp(&b.x))
            .then(a.y.total_cmp(&b.y))
    });

    let mut tools: Vec<(u64, DrillRole)> = Vec::new();
    for h in &hits {
        if !tools.iter().any(|(d, r)| *d == h.d_um && *r == h.role) {
            tools.push((h.d_um, h.role));
        }
    }

    let mut drill = format!("M48\nFMAT,2\n{drill_header}\n");
    for (n, (d, role)) in tools.iter().enumerate() {
        let attr = match role {
            DrillRole::Plated => "",
            DrillRole::Breakout => "%TA.AperFunction,MechanicalDrill,Breakout*%\n",
            DrillRole::Tooling => "%TA.AperFunction,MechanicalDrill,Tooling*%\n",
        };
        drill.push_str(attr);
        drill.push_str(&format!(
            "T{}C{:.drill_prec$}\n",
            n + 1,
            *d as f64 / 1000.0 * scale
        ));
    }
    drill.push_str("%\nG90\nG05\n");
    for (n, (d, role)) in tools.iter().enumerate() {
        drill.push_str(&format!("T{}\n", n + 1));
        for h in hits.iter().filter(|h| h.d_um == *d && h.role == *role) {
            let (x, y) = (h.x * scale, h.y * scale);
            drill.push_str(&format!("X{x:.drill_prec$}Y{y:.drill_prec$}\n"));
        }
    }
    drill.push_str("M30\n");
    Ok(drill)
}

fn vcut_layer(layout: &PanelLayout, inch: bool) -> String {
    let scale = if inch { 1.0 / 25.4 } else { 1.0 };
    let mo = if inch { "MOIN" } else { "MOMM" };
    let mut body = String::new();
    for col in 0..layout.columns.saturating_sub(1) {
        let x = layout.rail
            + (col + 1) as f64 * layout.board_width
            + col as f64 * layout.pitch_x
            + layout.pitch_x / 2.0;
        body.push_str(&format!(
            "G01*\nX{:.6}Y{:.6}D02*\nX{:.6}Y{:.6}D01*\n",
            x * scale,
            0.0,
            x * scale,
            layout.panel_height * scale
        ));
    }
    for row in 0..layout.rows.saturating_sub(1) {
        let y = layout.rail
            + (row + 1) as f64 * layout.board_height
            + row as f64 * layout.pitch_y
            + layout.pitch_y / 2.0;
        body.push_str(&format!(
            "G01*\nX{:.6}Y{:.6}D02*\nX{:.6}Y{:.6}D01*\n",
            0.0,
            y * scale,
            layout.panel_width * scale,
            y * scale
        ));
    }
    format!(
        "G04 panel V-cut *\n%TF.FileFunction,Vcut*%\n%FSLANX36Y36*%\n%{mo}*%\n%LPD*%\n{body}M02*\n"
    )
}

/// Write the full panel fabrication set for the stored panel settings.
pub fn panel_fabrication_files(p: &Project) -> Result<BTreeMap<String, String>, String> {
    if !p.panel.enabled {
        return Err("panel export needs enabled panel settings".into());
    }
    p.panel.validate_fields()?;
    let layout = panel_layout(p)?;
    let panel_project = build_panel_project(p, &layout, &p.panel)?;
    let mut files = crate::gerber_files(&panel_project)?;
    for name in ["pcb-F.Cu.gbr", "pcb-B.Cu.gbr", "pcb-Edge.gbr"] {
        if let Some(text) = files.get_mut(name) {
            inject_part_array_header(text);
        }
    }
    files.insert("pcb-Drill.drl".into(), panel_drill_file(p, &layout)?);
    if p.panel.separation == PanelSeparation::VScore {
        files.insert("pcb-Vcut.gbr".into(), vcut_layer(&layout, p.unit == "inch"));
    }
    Ok(files)
}

/// Pick-and-place listing every copy with `{reference}_{index}` suffixes.
pub fn panel_placement_text(p: &Project) -> Result<String, String> {
    if !p.panel.enabled {
        return Err("panel placement needs enabled panel settings".into());
    }
    let layout = panel_layout(p)?;
    const MAX_PLACED: usize = 5_000;
    let refs: BTreeMap<u64, &pcbcraft_doc::Component> =
        p.components.iter().map(|c| (c.id, c)).collect();
    if p.board.footprints.len() * layout.copies.len() > MAX_PLACED {
        return Err("placement file exceeds the placed-object limit".into());
    }
    let mut rows: Vec<String> = Vec::new();
    for copy in &layout.copies {
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
            let reference = format!("{}_{}", c.reference, copy.index);
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
    rows.sort_by(|a, b| {
        a.split('"')
            .nth(1)
            .unwrap_or("")
            .cmp(b.split('"').nth(1).unwrap_or(""))
    });
    Ok(format!(
        "File Format,ASCII,\n#\n#\tPick and Place File\t{}\n#\nSMT,THT,X,Y,Rotation,Side,Package\n{}\n",
        p.name,
        rows.join("\n")
    ))
}
