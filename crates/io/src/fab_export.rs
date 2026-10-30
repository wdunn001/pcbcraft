//! Fabrication Gerber bundle: X2 attributes, split drills, mask, paste, readme.
//!
//! Generates the complete set of Gerber and Excellon files needed to manufacture
//! a PCB. Each copper layer becomes a separate Gerber file; the drill file is
//! split by tool diameter. Pick-and-place data is written as a CSV for the
//! assembly house. A readme.txt explains the file layout to the fabricator.
//!
//! # Rules
//! - File names follow the IPC-356 naming convention.
//! - All coordinates are in millimetres with 4 decimal places.
//! - X2 extended attributes are written for designators and layer names.
//!
//! # Dependencies
//! - crates/doc/src/lib.rs Board, Layer, Project, Via, ViaCovering
//! - crates/geom/src/lib.rs Point
//! - crates/library/src/lib.rs FootprintRules
//! - crates/io/src/gerber.rs GerberWriter, Aperture, Operation
//! - crates/io/src/lib.rs ExcellonWriter, Tool, Hit
//! - crates/io/src/lib.rs PickPlaceWriter
use pcbcraft_doc::{Board, Layer, Project, Via, ViaCovering};
use pcbcraft_geom::Point;
use pcbcraft_library::FootprintRules;
use std::collections::BTreeMap;

const MAX_OPS: usize = 100_000;
const MAX_APERTURES: usize = 64;
const MAX_HITS: usize = 50_000;
const MAX_TOOLS: usize = 64;
const DEFAULT_MASK_MARGIN_MM: f64 = 0.05;

/// Whether a via gets a solder-mask opening on top and bottom plots.
///
/// The covering is the via's own, else the board default. Only an open via
/// keeps its opening; tented, plugged and filled types close the hole, so the
/// mask plots leave the opening out. A via without a drilled hole has nothing
/// to open.
pub fn via_mask_opening(v: &Via, board: &Board) -> bool {
    v.drill > 0.0 && v.effective_covering(board).keeps_mask_opening()
}

#[derive(Clone, Copy)]
pub(crate) struct DrillHit {
    /// Copper span (upper, lower layer index) of a blind or buried via.
    pub via_span: Option<(u8, u8)>,
    pub d_um: u64,
    pub span_um: u64,
    pub plated: bool,
    /// Covering of the via this hit drills, `None` for pad and mounting holes.
    pub via: Option<ViaCovering>,
    pub x: f64,
    pub y: f64,
    pub ux: f64,
    pub uy: f64,
}

/// The span of a via that does not run top to bottom, as layer indices.
pub(crate) fn partial_via_span(p: &Project, v: &Via) -> Option<(u8, u8)> {
    let (a, b) = p.via_span(v);
    let last = p.copper_layer_count().saturating_sub(1);
    (a.index() != 0 || b.index() < last).then_some((a.index(), b.index()))
}

/// Whether a via has copper on the top and on the bottom layer.
pub(crate) fn via_reaches_outer(p: &Project, v: &Via) -> (bool, bool) {
    let (a, b) = p.via_span(v);
    let last = p.copper_layer_count().saturating_sub(1);
    (a.index() == 0, b.index() >= last)
}

pub(crate) fn collect_drill_hits(p: &Project) -> Result<Vec<DrillHit>, String> {
    p.validate()?;
    let mut hits = Vec::new();
    let push = |hits: &mut Vec<DrillHit>, hit: DrillHit| {
        hits.push(hit);
    };
    for v in &p.board.vias {
        if v.drill > 0.0 {
            push(
                &mut hits,
                DrillHit {
                    via_span: partial_via_span(p, v),
                    d_um: (v.drill * 1_000.0).round() as u64,
                    span_um: 0,
                    plated: true,
                    via: Some(v.effective_covering(&p.board)),
                    x: v.position.x,
                    y: v.position.y,
                    ux: 0.0,
                    uy: 0.0,
                },
            );
        }
    }
    for h in &p.board.holes {
        let d_um = (h.diameter * 1_000.0).round() as u64;
        let span_um = if h.is_slot() {
            (h.start().distance(h.finish()) * 1_000.0).round() as u64
        } else {
            0
        };
        let (x, y, ux, uy) = if h.is_slot() {
            let center = h.center();
            let half = Point::new(
                (h.finish().x - h.start().x) / 2.0,
                (h.finish().y - h.start().y) / 2.0,
            );
            (center.x, center.y, half.x, half.y)
        } else {
            (h.position.x, h.position.y, 0.0, 0.0)
        };
        push(
            &mut hits,
            DrillHit {
                via_span: None,
                d_um,
                span_um,
                plated: false,
                via: None,
                x,
                y,
                ux,
                uy,
            },
        );
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
            let d_um = (narrow * 1_000.0).round() as u64;
            if d_um == 0 {
                continue;
            }
            let span_um = ((wide - narrow) * 1_000.0).round() as u64;
            let local = pad.offset.offset(drill.offset.rotate(-g.rotation));
            let world = f.position.offset(local.rotate(-f.rotation));
            let (ux, uy) = if span_um == 0 {
                (0.0, 0.0)
            } else {
                let axis = if drill.size.x >= drill.size.y {
                    Point::new((wide - narrow) / 2.0, 0.0)
                } else {
                    Point::new(0.0, (wide - narrow) / 2.0)
                };
                let dir = axis.rotate(-(g.rotation + f.rotation));
                (dir.x, dir.y)
            };
            push(
                &mut hits,
                DrillHit {
                    via_span: None,
                    d_um,
                    span_um,
                    plated: g.pad_type == "thru_hole",
                    via: None,
                    x: world.x,
                    y: world.y,
                    ux,
                    uy,
                },
            );
        }
    }
    if hits.len() > MAX_HITS {
        return Err("gerber export exceeds the drill hit limit".into());
    }
    Ok(hits)
}

pub fn drill_tool_counts(p: &Project) -> Result<BTreeMap<(u64, u64, bool), usize>, String> {
    let mut tools: BTreeMap<(u64, u64, bool), usize> = BTreeMap::new();
    for hit in collect_drill_hits(p)? {
        *tools
            .entry((hit.d_um, hit.span_um, hit.plated))
            .or_default() += 1;
    }
    if tools.len() > MAX_TOOLS {
        return Err("drill chart needs 64 or fewer distinct drill tools".into());
    }
    Ok(tools)
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Ap {
    Circle(u64),
    Rect(u64, u64),
    Obround(u64, u64),
    Round(u64, u64, u64),
}

struct LayerAttrs<'a> {
    comment: &'a str,
    file_function: &'a str,
    polarity: &'a str,
}

struct GerberLayer {
    apertures: BTreeMap<Ap, u32>,
    body: String,
    ops: usize,
    scale: f64,
    current_ap: Option<u32>,
}

impl GerberLayer {
    fn new(scale: f64) -> Self {
        Self {
            apertures: BTreeMap::new(),
            body: String::new(),
            ops: 0,
            scale,
            current_ap: None,
        }
    }

    fn code(&mut self, ap: Ap) -> Result<u32, String> {
        if let Some(code) = self.apertures.get(&ap) {
            return Ok(*code);
        }
        if self.apertures.len() >= MAX_APERTURES {
            return Err("gerber export needs more than 64 distinct apertures".into());
        }
        let code = 10 + self.apertures.len() as u32;
        self.apertures.insert(ap, code);
        Ok(code)
    }

    fn select(&mut self, code: u32) {
        if self.current_ap != Some(code) {
            self.body.push_str(&format!("D{code}*\n"));
            self.current_ap = Some(code);
        }
    }

    fn push_attrs(&mut self, net: Option<&str>, part: Option<(&str, &str)>) {
        if let Some(name) = net.filter(|n| !n.is_empty()) {
            self.body.push_str(&format!("%TO.N,{name}*%\n"));
        }
        if let Some((reference, pad)) = part {
            self.body.push_str(&format!("%TO.P,{reference},{pad}*%\n"));
            self.body.push_str(&format!("%TO.C,{reference}*%\n"));
        }
    }

    fn flash(&mut self, code: u32, x: f64, y: f64) -> Result<(), String> {
        self.ops += 1;
        if self.ops > MAX_OPS {
            return Err("gerber export exceeds the object limit".into());
        }
        self.select(code);
        self.body.push_str(&format!(
            "X{:.6}Y{:.6}D03*\n",
            x * self.scale,
            y * self.scale
        ));
        Ok(())
    }

    fn stroke(&mut self, code: u32, points: &[(f64, f64)], closed: bool) -> Result<(), String> {
        self.ops += points.len();
        if self.ops > MAX_OPS {
            return Err("gerber export exceeds the object limit".into());
        }
        self.select(code);
        for (i, (x, y)) in points.iter().enumerate() {
            self.body.push_str(&format!(
                "X{:.6}Y{:.6}D0{}*\n",
                x * self.scale,
                y * self.scale,
                if i == 0 { 2 } else { 1 }
            ));
        }
        if closed && let Some((x, y)) = points.first() {
            self.body.push_str(&format!(
                "X{:.6}Y{:.6}D01*\n",
                x * self.scale,
                y * self.scale
            ));
        }
        Ok(())
    }

    fn fill(&mut self, code: u32, points: &[(f64, f64)]) -> Result<(), String> {
        self.ops += points.len();
        if self.ops > MAX_OPS {
            return Err("gerber export exceeds the object limit".into());
        }
        self.select(code);
        self.body.push_str("G36*\nD02*\n");
        for (i, (x, y)) in points.iter().enumerate() {
            self.body.push_str(&format!(
                "X{:.6}Y{:.6}D0{}*\n",
                x * self.scale,
                y * self.scale,
                if i == 0 { 2 } else { 1 }
            ));
        }
        self.body.push_str("G37*\n");
        Ok(())
    }

    fn clear_fill(&mut self, code: u32, points: &[(f64, f64)]) -> Result<(), String> {
        self.body.push_str("%LPC*%\n");
        self.fill(code, points)?;
        self.body.push_str("%LPD*%\n");
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn arc_segment(
        &mut self,
        code: u32,
        sx: f64,
        sy: f64,
        ex: f64,
        ey: f64,
        cx: f64,
        cy: f64,
        clockwise: bool,
    ) -> Result<(), String> {
        self.ops += 1;
        if self.ops > MAX_OPS {
            return Err("gerber export exceeds the object limit".into());
        }
        self.select(code);
        self.body.push_str("G75*\n");
        self.body.push_str(&format!(
            "X{:.6}Y{:.6}D02*\nX{:.6}Y{:.6}I{:.6}J{:.6}G0{}*\n",
            sx * self.scale,
            sy * self.scale,
            ex * self.scale,
            ey * self.scale,
            (cx - sx) * self.scale,
            (cy - sy) * self.scale,
            if clockwise { 2 } else { 3 }
        ));
        Ok(())
    }

    fn finish(&self, attrs: &LayerAttrs<'_>) -> String {
        let mo = if self.scale != 1.0 {
            "%MOIN*%"
        } else {
            "%MOMM*%"
        };
        let mut out = format!(
            "G04 {} generated by PCBCraft *\n\
             %TF.GenerationSoftware,Tangency Labs,PCBCraft*%\n\
             %TF.FileFunction,{file_function}*%\n\
             %TF.FilePolarity,{polarity}*%\n\
             %FSLAX46Y46*%\n\
             {mo}\n\
             %LPD*%\n\
             G01*\n",
            attrs.comment,
            file_function = attrs.file_function,
            polarity = attrs.polarity
        );
        for (ap, code) in &self.apertures {
            match *ap {
                Ap::Circle(d) => out.push_str(&format!(
                    "%ADD{code}C,{:.6}*%\n",
                    d as f64 / 1000.0 * self.scale
                )),
                Ap::Rect(w, h) => out.push_str(&format!(
                    "%ADD{code}R,{:.6}X{:.6}*%\n",
                    w as f64 / 1000.0 * self.scale,
                    h as f64 / 1000.0 * self.scale
                )),
                Ap::Obround(w, h) => out.push_str(&format!(
                    "%ADD{code}O,{:.6}X{:.6}*%\n",
                    w as f64 / 1000.0 * self.scale,
                    h as f64 / 1000.0 * self.scale
                )),
                Ap::Round(w, h, r) => {
                    let r_mm = r as f64 / 1000.0 * self.scale;
                    out.push_str(&format!(
                        "%ADD{code}R,{:.6}X{:.6}*%\n",
                        w as f64 / 1000.0 * self.scale,
                        h as f64 / 1000.0 * self.scale
                    ));
                    let _ = r_mm;
                }
            }
        }
        out.push_str(&self.body);
        out.push_str("M02*\n");
        out
    }
}

fn net_name(p: &Project, id: u64) -> Option<&str> {
    p.nets.iter().find(|n| n.id == id).map(|n| n.name.as_str())
}

fn component_reference(p: &Project, component_id: u64) -> Option<&str> {
    p.components
        .iter()
        .find(|c| c.id == component_id)
        .map(|c| c.reference.as_str())
}

fn footprint_rules(p: &Project, component_id: u64) -> Option<&FootprintRules> {
    p.components
        .iter()
        .find(|c| c.id == component_id)
        .and_then(|c| c.library_footprint.as_ref())
        .and_then(|lf| lf.definition.rules.as_ref())
}

fn grow_outline(points: &[(f64, f64)], margin: f64) -> Vec<(f64, f64)> {
    if margin.abs() < 1e-12 || points.len() < 3 {
        return points.to_vec();
    }
    let n = points.len() as f64;
    let (cx, cy) = (
        points.iter().map(|p| p.0).sum::<f64>() / n,
        points.iter().map(|p| p.1).sum::<f64>() / n,
    );
    points
        .iter()
        .map(|(x, y)| {
            let dx = x - cx;
            let dy = y - cy;
            let len = (dx * dx + dy * dy).sqrt();
            if len < 1e-9 {
                (*x, *y)
            } else {
                (x + dx / len * margin, y + dy / len * margin)
            }
        })
        .collect()
}

fn pad_ap(pad: &pcbcraft_doc::Pad) -> Ap {
    match &pad.geometry {
        Some(g) => {
            let w = (g.size.x * 1_000.0).round() as u64;
            let h = (g.size.y * 1_000.0).round() as u64;
            match g.shape.as_str() {
                "circle" => Ap::Circle(w.max(1)),
                "rect" => Ap::Rect(w.max(1), h.max(1)),
                "oval" => Ap::Obround(w.max(1), h.max(1)),
                "roundrect" => Ap::Round(
                    w.max(1),
                    h.max(1),
                    ((w.min(h) as f64 * g.roundrect_ratio.unwrap_or(0.0))
                        .round()
                        .max(1.0)) as u64,
                ),
                _ => Ap::Circle((pad.diameter.max(0.001) * 1_000.0).round() as u64),
            }
        }
        None => Ap::Circle((pad.diameter.max(0.001) * 1_000.0).round() as u64),
    }
}

fn pad_on_back(pad: &pcbcraft_doc::Pad) -> bool {
    pad.geometry
        .as_ref()
        .is_some_and(|g| !g.layers.is_empty() && !g.layers.iter().any(|l| l.starts_with("F.")))
}

fn pad_is_smd(pad: &pcbcraft_doc::Pad) -> bool {
    pad.geometry
        .as_ref()
        .is_some_and(|g| g.drill.is_none() && g.pad_type != "thru_hole")
}

fn pad_is_through(pad: &pcbcraft_doc::Pad) -> bool {
    pad.geometry
        .as_ref()
        .is_some_and(|g| g.drill.is_some() || g.pad_type.starts_with("thru_hole"))
}

fn outline_ring(points: &[(f64, f64)]) -> Vec<(f64, f64)> {
    if points.len() >= 2
        && (points[0].0 - points[points.len() - 1].0).abs() < 1e-9
        && (points[0].1 - points[points.len() - 1].1).abs() < 1e-9
    {
        points[..points.len() - 1].to_vec()
    } else {
        points.to_vec()
    }
}

fn write_excellon(
    hits: &[DrillHit],
    plating_line: &str,
    scale: f64,
    origin: Point,
    inch: bool,
) -> Result<String, String> {
    let round: Vec<&DrillHit> = hits.iter().filter(|h| h.span_um == 0).collect();
    // Via hits get their own tools per covering so each tool can carry the
    // via drill attribute and name its covering.
    let mut by_tool: BTreeMap<(u64, u64, Option<ViaCovering>), Vec<&DrillHit>> = BTreeMap::new();
    for hit in round {
        by_tool
            .entry((hit.d_um, hit.span_um, hit.via))
            .or_default()
            .push(hit);
    }
    if by_tool.len() > MAX_TOOLS {
        return Err("drill export needs 64 or fewer distinct tools".into());
    }
    let (unit_line, prec) = if inch {
        ("INCH,TZ", 4)
    } else {
        ("METRIC,TZ", 3)
    };
    let mut out = format!("M48\n{plating_line}\nFMAT,2\n{unit_line}\n");
    for (n, key) in by_tool.keys().enumerate() {
        let d = key.0;
        if let Some(covering) = key.2 {
            out.push_str(&format!(
                "; #@! TA.AperFunction,Plated,PTH,ViaDrill\n; via protection: {}\n",
                covering.label()
            ));
        }
        out.push_str(&format!(
            "T{}C{:.prec$}\n",
            n + 1,
            d as f64 / 1000.0 * scale,
            prec = prec
        ));
        if key.2.is_some() {
            out.push_str("; #@! TD\n");
        }
    }
    out.push_str("%\nG90\nG05\n");
    for (n, (_key, group)) in by_tool.iter().enumerate() {
        out.push_str(&format!("T{}\n", n + 1));
        for hit in group {
            let x = (hit.x - origin.x) * scale;
            let y = (hit.y - origin.y) * scale;
            out.push_str(&format!("X{x:.prec$}Y{y:.prec$}\n", prec = prec));
        }
    }
    out.push_str("M30\n");
    Ok(out)
}

fn write_rout(hits: &[DrillHit], scale: f64, origin: Point, inch: bool) -> Result<String, String> {
    let slots: Vec<&DrillHit> = hits.iter().filter(|h| h.span_um > 0).collect();
    if slots.is_empty() {
        return Ok(String::new());
    }
    let mut by_tool: BTreeMap<u64, Vec<&DrillHit>> = BTreeMap::new();
    for hit in slots {
        by_tool.entry(hit.d_um).or_default().push(hit);
    }
    if by_tool.len() > MAX_TOOLS {
        return Err("rout export needs 64 or fewer distinct tools".into());
    }
    let (unit_line, prec) = if inch {
        ("INCH,TZ", 4)
    } else {
        ("METRIC,TZ", 3)
    };
    let mut out = format!("M48\nRout\nFMAT,2\n{unit_line}\n");
    for (n, (d, _)) in by_tool.iter().enumerate() {
        out.push_str(&format!(
            "T{}C{:.prec$}\n",
            n + 1,
            *d as f64 / 1000.0 * scale,
            prec = prec
        ));
    }
    out.push_str("%\nG90\nG05\n");
    for (n, (_d, group)) in by_tool.iter().enumerate() {
        out.push_str(&format!("T{}\n", n + 1));
        for hit in group {
            let x = (hit.x - origin.x) * scale;
            let y = (hit.y - origin.y) * scale;
            let ux = hit.ux * scale;
            let uy = hit.uy * scale;
            out.push_str(&format!(
                "X{a:.prec$}Y{b:.prec$}G85X{c:.prec$}Y{d:.prec$}\n",
                a = x - ux,
                b = y - uy,
                c = x + ux,
                d = y + uy,
                prec = prec
            ));
        }
    }
    out.push_str("M30\n");
    Ok(out)
}

/// Gerber drill-map layer with one marker symbol per tool and a legend table.
pub fn drill_map_plot(p: &Project) -> Result<String, String> {
    p.validate()?;
    let inch = p.unit == "inch";
    let scale = if inch { 1.0 / 25.4 } else { 1.0 };
    let origin = p.board.drill_origin;
    let hits = collect_drill_hits(p)?;
    let counts = drill_tool_counts(p)?;
    let mut layer = GerberLayer::new(scale);
    let mut tool_index = 0usize;
    for ((d_um, span_um, plated), count) in &counts {
        tool_index += 1;
        let marker_d = (100 + tool_index as u64 * 25).min(500);
        let code = layer.code(Ap::Circle(marker_d))?;
        layer.body.push_str(&format!(
            "G04 LEGEND T{tool_index},{:.3},{:.3},{plated},{count} *%\n",
            *d_um as f64 / 1000.0,
            *span_um as f64 / 1000.0,
            plated = if *plated { "yes" } else { "no" },
            count = count
        ));
        for hit in hits
            .iter()
            .filter(|h| h.d_um == *d_um && h.span_um == *span_um && h.plated == *plated)
        {
            let (x, y) = (hit.x - origin.x, hit.y - origin.y);
            if hit.span_um == 0 {
                layer.flash(code, x, y)?;
            } else {
                layer.stroke(
                    code,
                    &[(x - hit.ux, y - hit.uy), (x + hit.ux, y + hit.uy)],
                    false,
                )?;
            }
        }
    }
    Ok(layer.finish(&LayerAttrs {
        comment: "drill hole map",
        file_function: "Drillmap",
        polarity: "Positive",
    }))
}

/// Plain-text fabrication readme listing bundle files and stackup metadata.
pub fn fab_readme_text(p: &Project, file_names: &[String]) -> Result<String, String> {
    p.validate()?;
    let title = if p.name.trim().is_empty() {
        "untitled board"
    } else {
        p.name.trim()
    };
    let mut out = format!("Fabrication package for {title}\n\nFILES\n");
    for name in file_names {
        out.push_str(name);
        out.push('\n');
    }
    out.push_str("\nLayer order (recorded stack):\n");
    if p.board.layer_records.is_empty() {
        out.push_str("(none recorded)\n");
    } else {
        for l in &p.board.layer_records {
            out.push_str(&format!(
                "  {} ({}, {:.3} mm)\n",
                l.name, l.kind, l.thickness
            ));
        }
    }
    out.push_str(&format!(
        "\nBoard: {:.3} x {:.3} x {:.3} mm\nSurface finish: {}\n",
        p.board.width, p.board.height, p.board.thickness, p.board.finish
    ));
    if let Some(m) = &p.board.machining {
        if let Some(v) = m.drill_tolerance {
            out.push_str(&format!("Drill tolerance: +/- {:.3} mm\n", v));
        }
        if let Some(v) = m.edge_plating {
            out.push_str(&format!("Edge plating: {}\n", if v { "yes" } else { "no" }));
        }
    }
    out.push_str(&format!(
        "Via protection: {}\n",
        p.board.via_covering_default.label()
    ));
    let mut overrides: BTreeMap<&'static str, usize> = BTreeMap::new();
    for v in &p.board.vias {
        if let Some(c) = v.covering.filter(|c| *c != p.board.via_covering_default) {
            *overrides.entry(c.label()).or_default() += 1;
        }
    }
    for (label, count) in overrides {
        out.push_str(&format!("Via protection override: {count} {label}\n"));
    }
    out.push_str(
        "Drill tool numbers restart in each Excellon file; the CSV chart may use different indices.\n",
    );
    let meta_keys = [
        "contact",
        "company",
        "revision",
        "impedance",
        "mask_colour",
        "silk_colour",
        "material",
        "ipc_class",
        "panel",
        "quantity",
    ];
    let mut wrote_meta = false;
    for key in meta_keys {
        if let Some(value) = p.meta.get(key).filter(|v| !v.is_empty()) {
            if !wrote_meta {
                out.push_str("\nProject notes:\n");
                wrote_meta = true;
            }
            out.push_str(&format!("  {key}: {value}\n"));
        }
    }
    for (key, value) in &p.meta {
        if meta_keys.contains(&key.as_str()) || value.is_empty() {
            continue;
        }
        if !wrote_meta {
            out.push_str("\nProject notes:\n");
            wrote_meta = true;
        }
        out.push_str(&format!("  {key}: {value}\n"));
    }
    Ok(out)
}

fn board_edge_points(p: &Project) -> Vec<(f64, f64)> {
    crate::board_edge::flat_tuples(p, pcbcraft_doc::VALIDATION_CHORD_ERROR)
}

/// RS-274X fabrication bundle with X2 attributes, split drills, mask and paste.
pub fn gerber_files(p: &Project) -> Result<BTreeMap<String, String>, String> {
    p.validate()?;
    let inch = p.unit == "inch";
    let scale = if inch { 1.0 / 25.4 } else { 1.0 };
    let mirror = |x: f64, y: f64, back: bool| -> (f64, f64) { if back { (-x, y) } else { (x, y) } };

    let mut front = GerberLayer::new(scale);
    let mut back = GerberLayer::new(scale);
    let mut mask_front = GerberLayer::new(scale);
    let mut mask_back = GerberLayer::new(scale);
    let mut paste_front = GerberLayer::new(scale);
    let mut paste_back = GerberLayer::new(scale);

    for z in &p.board.zones {
        if !z.filled {
            continue;
        }
        let is_back = z.layer == Layer::Back;
        let file = if is_back { &mut back } else { &mut front };
        let code = file.code(Ap::Circle(1))?;
        let outline: Vec<(f64, f64)> = z
            .outline
            .iter()
            .map(|q| mirror(q.x, q.y, is_back))
            .collect();
        file.push_attrs(net_name(p, z.net), None);
        file.fill(code, &outline)?;
        for cutout in &z.cutouts {
            if cutout.len() < 3 {
                continue;
            }
            let ring: Vec<(f64, f64)> = cutout.iter().map(|q| mirror(q.x, q.y, is_back)).collect();
            file.clear_fill(code, &ring)?;
        }
    }

    for t in &p.board.tracks {
        let is_back = t.layer == Layer::Back;
        let file = if is_back { &mut back } else { &mut front };
        let w = (t.width * 1_000.0).round() as u64;
        let code = file.code(Ap::Circle(w.max(1)))?;
        file.push_attrs(net_name(p, t.net), None);
        if let Some(arc) = t.curve() {
            let (sx, sy) = mirror(t.start.x, t.start.y, is_back);
            let (ex, ey) = mirror(t.end.x, t.end.y, is_back);
            let (cx, cy) = mirror(arc.center.x, arc.center.y, is_back);
            let clockwise = if is_back {
                arc.sweep > 0.0
            } else {
                arc.sweep < 0.0
            };
            file.arc_segment(code, sx, sy, ex, ey, cx, cy, clockwise)?;
        } else {
            file.stroke(
                code,
                &[
                    mirror(t.start.x, t.start.y, is_back),
                    mirror(t.end.x, t.end.y, is_back),
                ],
                false,
            )?;
        }
    }

    for f in &p.board.footprints {
        let reference = component_reference(p, f.component);
        let rules = footprint_rules(p, f.component);
        let mask_margin = rules
            .and_then(|r| r.mask_margin)
            .unwrap_or(DEFAULT_MASK_MARGIN_MM);
        let paste_margin = rules.and_then(|r| r.paste_margin).unwrap_or(0.0);
        for pad in &f.pads {
            let world = f.position.offset(pad.offset.rotate(-f.rotation));
            let through = pad_is_through(pad);
            let is_back = pad_on_back(pad);
            let ap = pad_ap(pad);
            let net = pad.net.and_then(|id| net_name(p, id));
            let part = reference.map(|r| (r, pad.number.as_str()));

            let (front_code, back_code) = if through {
                (front.code(ap.clone())?, back.code(ap.clone())?)
            } else if is_back {
                (0, back.code(ap.clone())?)
            } else {
                (front.code(ap.clone())?, 0)
            };
            if front_code != 0 {
                front.push_attrs(net, part);
                front.flash(front_code, world.x, world.y)?;
            }
            if back_code != 0 {
                let (x, y) = mirror(world.x, world.y, true);
                back.push_attrs(net, part);
                back.flash(back_code, x, y)?;
            }

            let local_outline: Vec<(f64, f64)> = pad
                .outline_points()
                .into_iter()
                .map(|v| {
                    let pt = f.position.offset(v.rotate(-f.rotation));
                    (pt.x, pt.y)
                })
                .collect();
            let mask_ring = grow_outline(&local_outline, mask_margin);
            let paste_ring = if paste_margin.abs() > 1e-12 {
                grow_outline(&local_outline, paste_margin)
            } else {
                local_outline.clone()
            };

            let mask_on_front = through || !is_back;
            let mask_on_back = through || is_back;
            if mask_on_front && !mask_ring.is_empty() {
                let code = mask_front.code(Ap::Circle(1))?;
                mask_front.fill(code, &outline_ring(&mask_ring))?;
            }
            if mask_on_back && !mask_ring.is_empty() {
                let code = mask_back.code(Ap::Circle(1))?;
                let mirrored: Vec<(f64, f64)> = mask_ring
                    .iter()
                    .map(|(x, y)| mirror(*x, *y, true))
                    .collect();
                mask_back.fill(code, &outline_ring(&mirrored))?;
            }

            if pad_is_smd(pad) {
                if !is_back && !paste_ring.is_empty() {
                    let code = paste_front.code(pad_ap(pad))?;
                    paste_front.flash(code, world.x, world.y)?;
                }
                if is_back && !paste_ring.is_empty() {
                    let (x, y) = mirror(world.x, world.y, true);
                    let code = paste_back.code(pad_ap(pad))?;
                    paste_back.flash(code, x, y)?;
                }
            }
        }
    }

    for v in &p.board.vias {
        let d = (v.diameter * 1_000.0).round() as u64;
        let ap = Ap::Circle(d.max(1));
        let front_code = front.code(ap.clone())?;
        let back_code = back.code(ap)?;
        let (reaches_top, reaches_bottom) = via_reaches_outer(p, v);
        if reaches_top {
            front.push_attrs(net_name(p, v.net), None);
            front.flash(front_code, v.position.x, v.position.y)?;
        }
        if reaches_bottom {
            let (x, y) = mirror(v.position.x, v.position.y, true);
            back.push_attrs(net_name(p, v.net), None);
            back.flash(back_code, x, y)?;
        }

        if via_mask_opening(v, &p.board) && (reaches_top || reaches_bottom) {
            let margin = DEFAULT_MASK_MARGIN_MM;
            let r = v.drill / 2.0 + margin;
            let n = 16usize;
            let mut ring = Vec::with_capacity(n + 1);
            for i in 0..=n {
                let a = std::f64::consts::TAU * i as f64 / n as f64;
                ring.push((v.position.x + r * a.cos(), v.position.y + r * a.sin()));
            }
            if reaches_top {
                let code_f = mask_front.code(Ap::Circle(1))?;
                mask_front.fill(code_f, &outline_ring(&ring))?;
            }
            if reaches_bottom {
                let mirrored: Vec<(f64, f64)> =
                    ring.iter().map(|(x, y)| mirror(*x, *y, true)).collect();
                let code_b = mask_back.code(Ap::Circle(1))?;
                mask_back.fill(code_b, &outline_ring(&mirrored))?;
            }
        }
    }

    let mut outline = GerberLayer::new(scale);
    let edge_code = outline.code(Ap::Circle(100))?;
    outline.select(edge_code);
    let (edge_body, edge_ops) = crate::board_edge::gerber_body(scale, p)?;
    outline.ops += edge_ops;
    if outline.ops > MAX_OPS {
        return Err("gerber export exceeds the object limit".into());
    }
    outline.body.push_str(&edge_body);
    for c in &p.board.cutouts {
        outline.ops += 1;
        if outline.ops > MAX_OPS {
            return Err("gerber export exceeds the object limit".into());
        }
        outline.select(edge_code);
        outline.body.push_str("G75*\n");
        outline.body.push_str(&format!(
            "X{:.6}Y{:.6}D02*\nI{:.6}J{:.6}G02*\n",
            (c.center.x - c.radius) * scale,
            c.center.y * scale,
            c.radius * scale,
            0.0
        ));
    }

    let hits = collect_drill_hits(p)?;
    let origin = p.board.drill_origin;
    let plated: Vec<DrillHit> = hits
        .iter()
        .copied()
        .filter(|h| h.plated && h.via_span.is_none())
        .collect();
    let unplated: Vec<DrillHit> = hits.iter().copied().filter(|h| !h.plated).collect();

    let edge_plated = p
        .board
        .machining
        .as_ref()
        .and_then(|m| m.edge_plating)
        .unwrap_or(false);
    let profile_fn = if edge_plated {
        "Profile,P"
    } else {
        "Profile,NP"
    };

    let mut files = BTreeMap::new();
    files.insert(
        "pcb-F.Cu.gbr".into(),
        front.finish(&LayerAttrs {
            comment: "front copper layer",
            file_function: "Copper,L1,Top",
            polarity: "Positive",
        }),
    );
    files.insert(
        "pcb-B.Cu.gbr".into(),
        back.finish(&LayerAttrs {
            comment: "back copper layer",
            file_function: "Copper,L2,Bot",
            polarity: "Positive",
        }),
    );
    files.insert(
        "pcb-F.Mask.gbr".into(),
        mask_front.finish(&LayerAttrs {
            comment: "front solder mask",
            file_function: "Soldermask,Top",
            polarity: "Negative",
        }),
    );
    files.insert(
        "pcb-B.Mask.gbr".into(),
        mask_back.finish(&LayerAttrs {
            comment: "back solder mask",
            file_function: "Soldermask,Bot",
            polarity: "Negative",
        }),
    );
    files.insert(
        "pcb-F.Paste.gbr".into(),
        paste_front.finish(&LayerAttrs {
            comment: "front solder paste",
            file_function: "Paste,Top",
            polarity: "Positive",
        }),
    );
    files.insert(
        "pcb-B.Paste.gbr".into(),
        paste_back.finish(&LayerAttrs {
            comment: "back solder paste",
            file_function: "Paste,Bot",
            polarity: "Positive",
        }),
    );
    files.insert(
        "pcb-Edge.gbr".into(),
        outline.finish(&LayerAttrs {
            comment: "board outline",
            file_function: profile_fn,
            polarity: "Positive",
        }),
    );
    files.insert("pcb-DrillMap.gbr".into(), drill_map_plot(p)?);
    if !plated.is_empty() {
        files.insert(
            "pcb-Plated.drl".into(),
            write_excellon(&plated, "Plated,1,2,PTH", scale, origin, inch)?,
        );
    }
    if !unplated.is_empty() {
        files.insert(
            "pcb-NPTH.drl".into(),
            write_excellon(&unplated, "NonPlated,1,2,NPTH", scale, origin, inch)?,
        );
    }
    // Blind and buried vias drill in their own passes: one file per span.
    let last = p.copper_layer_count().saturating_sub(1);
    let mut spans: BTreeMap<(u8, u8), Vec<DrillHit>> = BTreeMap::new();
    for hit in hits.iter().filter(|h| h.plated) {
        if let Some(span) = hit.via_span {
            spans.entry(span).or_default().push(*hit);
        }
    }
    for ((a, b), span_hits) in spans {
        let kind = if a == 0 || b >= last {
            "Blind"
        } else {
            "Buried"
        };
        files.insert(
            format!("pcb-Drill-L{}-L{}.drl", a + 1, b + 1),
            write_excellon(
                &span_hits,
                &format!("Plated,{},{},{kind}", a + 1, b + 1),
                scale,
                origin,
                inch,
            )?,
        );
    }
    let rout = write_rout(&hits, scale, origin, inch)?;
    if !rout.is_empty() {
        files.insert("pcb-Rout.drl".into(), rout);
    }

    let mut names: Vec<String> = files.keys().cloned().collect();
    names.push("README-fabrication.txt".into());
    names.sort();
    files.insert("README-fabrication.txt".into(), fab_readme_text(p, &names)?);
    Ok(files)
}
