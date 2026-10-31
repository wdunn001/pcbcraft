//! Board half of the showcase generator: outline, stackup, placement, copper.
//!
//! Routing is guided by the engine's own checks: every attempt runs as one
//! atomic batch, is kept only when it adds no finding, and is undone
//! otherwise, so the stored file is clean by construction.
//!
//! # Rules
//! - Runs after `board.sync`; every footprint already exists.
//! - A failed attempt must leave the session exactly as it was.
//!
//! # Dependencies
//! - xtask/src/showcase/design.rs
//!
//! # Do not
//! - Move a part after routing has started.
use super::{Gen, Res, design::PINS, layout};
use pcbcraft_engine::Command;
use pcbcraft_geom::Point;
use serde_json::{Value, json};

const WIDTH: f64 = layout::WIDTH;
const HEIGHT: f64 = layout::HEIGHT;
const CORNER: f64 = 3.0;

fn pt(x: f64, y: f64) -> Value {
    json!({"x": x, "y": y})
}

/// Rounded rectangle outline as a vertex ring, inset from the board edge.
pub fn ring(inset: f64) -> Vec<Value> {
    let mut v = Vec::new();
    let r = CORNER - inset;
    let corners = [
        (CORNER, CORNER, 180.0_f64),
        (WIDTH - CORNER, CORNER, 270.0),
        (WIDTH - CORNER, HEIGHT - CORNER, 0.0),
        (CORNER, HEIGHT - CORNER, 90.0),
    ];
    for (cx, cy, start) in corners {
        for i in 0..=4 {
            let a = (start + 22.5 * f64::from(i)).to_radians();
            v.push(pt(round3(cx + r * a.cos()), round3(cy + r * a.sin())));
        }
    }
    v
}

/// Closed copy of a ring, for zone outlines.
pub fn closed_ring(inset: f64) -> Vec<Value> {
    let mut v = ring(inset);
    if let Some(first) = v.first().cloned() {
        v.push(first);
    }
    v
}

fn round3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

static CHECKS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static CHECK_MICROS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Number of check runs and their total time in milliseconds, for the log.
pub fn check_stats() -> (usize, usize) {
    (
        CHECKS.load(std::sync::atomic::Ordering::Relaxed),
        CHECK_MICROS.load(std::sync::atomic::Ordering::Relaxed) / 1000,
    )
}

/// Count of design-rule findings that are not plain missing connections.
pub fn bad_count(g: &Gen) -> usize {
    let started = pcbcraft_clock::instant_now();
    let n = pcbcraft_rules::drc(&g.s.project)
        .iter()
        .filter(|f| f.code != "unrouted" && f.code != "via_in_pad_not_capped")
        .count();
    CHECKS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    CHECK_MICROS.fetch_add(
        started.elapsed().as_micros() as usize,
        std::sync::atomic::Ordering::Relaxed,
    );
    n
}

/// Count of connections still missing.
pub fn open_count(g: &Gen) -> usize {
    pcbcraft_rules::airwires(&g.s.project).len()
}

/// Plain name of a board object for the log: a reference, or the id.
fn describe(g: &Gen, id: u64) -> String {
    let p = &g.s.project;
    if let Some(f) = p.board.footprints.iter().find(|f| f.id == id) {
        return p
            .component(f.component)
            .map_or(format!("#{id}"), |c| c.reference.clone());
    }
    if let Some(t) = p.board.tracks.iter().find(|t| t.id == id) {
        return format!(
            "track {} ({:.1},{:.1})",
            p.net_name(t.net),
            t.start.x,
            t.start.y
        );
    }
    if let Some(v) = p.board.vias.iter().find(|v| v.id == id) {
        return format!(
            "via {} ({:.1},{:.1})",
            p.net_name(v.net),
            v.position.x,
            v.position.y
        );
    }
    format!("#{id}")
}

fn messages(g: &Gen) -> Vec<String> {
    pcbcraft_rules::drc(&g.s.project)
        .iter()
        .filter(|f| f.code != "unrouted")
        .map(|f| {
            format!(
                "{}: {} [{}]",
                f.code,
                f.message,
                f.objects
                    .iter()
                    .map(|o| describe(g, *o))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
        .collect()
}

/// Run commands as one atomic batch and keep them only when they add no
/// finding; otherwise the session is left exactly as it was.
pub fn attempt(g: &mut Gen, steps: Vec<(&str, Value)>) -> bool {
    let before = bad_count(g);
    let cmds: Vec<Command> = steps
        .into_iter()
        .map(|(c, p)| Command {
            command: c.into(),
            params: p,
        })
        .collect();
    if g.s.batch(&cmds, None).is_err() {
        return false;
    }
    if bad_count(g) > before {
        let _ = g.s.undo();
        return false;
    }
    true
}

/// Like `attempt`, but prints the first new finding when a batch is refused.
pub fn attempt_verbose(g: &mut Gen, steps: Vec<(&str, Value)>, label: &str) -> bool {
    let before = messages(g);
    let cmds: Vec<Command> = steps
        .into_iter()
        .map(|(c, p)| Command {
            command: c.into(),
            params: p,
        })
        .collect();
    if let Err(e) = g.s.batch(&cmds, None) {
        println!("refused {label}: {e}");
        return false;
    }
    let after = messages(g);
    if after.len() > before.len() {
        let new: Vec<&String> = after
            .iter()
            .filter(|m| !before.contains(m))
            .take(2)
            .collect();
        println!("refused {label}: {new:?}");
        let _ = g.s.undo();
        return false;
    }
    true
}

pub const SIGNAL: f64 = 0.2;
pub const VIA: f64 = 0.45;
pub const DRILL: f64 = 0.2;

fn net_id(g: &Gen, name: &str) -> Res<u64> {
    g.nets
        .get(name)
        .copied()
        .ok_or_else(|| format!("unknown net {name}"))
}

fn seg_dist(p: Point, a: Point, b: Point) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len2 = dx * dx + dy * dy;
    let t = if len2 < 1e-12 {
        0.0
    } else {
        (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0)
    };
    p.distance(Point::new(a.x + t * dx, a.y + t * dy))
}

fn cross(o: Point, a: Point, b: Point) -> f64 {
    (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x)
}

/// Distance between two segments, zero when they cross.
fn seg_seg(a: Point, b: Point, c: Point, d: Point) -> f64 {
    let (d1, d2, d3, d4) = (
        cross(a, b, c),
        cross(a, b, d),
        cross(c, d, a),
        cross(c, d, b),
    );
    if d1 * d2 < 0.0 && d3 * d4 < 0.0 {
        return 0.0;
    }
    seg_dist(a, c, d)
        .min(seg_dist(b, c, d))
        .min(seg_dist(c, a, b))
        .min(seg_dist(d, a, b))
}

/// Distance from a segment to a pad outline, zero when it starts inside.
fn seg_to_pad(from: Point, to: Point, f: &pcbcraft_doc::Footprint, pad: &pcbcraft_doc::Pad) -> f64 {
    let ring: Vec<Point> = pad
        .outline_points()
        .into_iter()
        .map(|q| f.position.offset(q.rotate(-f.rotation)))
        .collect();
    if ring.len() < 3 {
        return seg_dist(f.pad_world_position(pad), from, to) - pad.bounding_radius();
    }
    let mut best = f64::INFINITY;
    for i in 0..ring.len() {
        let (p, q) = (ring[i], ring[(i + 1) % ring.len()]);
        best = best.min(seg_seg(from, to, p, q));
    }
    best
}

/// Cheap clearance pre-check for a via and the stub that feeds it, so the
/// full check runs only for candidates that probably pass.
pub fn stub_why(g: &Gen, net: u64, from: Point, via: Point) -> Option<String> {
    stub_why_margin(g, net, from, via, 0.17)
}

/// Same check with a chosen clearance margin in millimetres.
pub fn stub_why_margin(g: &Gen, net: u64, from: Point, via: Point, margin: f64) -> Option<String> {
    let p = &g.s.project;
    if net == g.nets.get("3V3").copied().unwrap_or(0) && via.x > 61.6 && via.y > 3.0 && via.y < 49.0
    {
        return Some("inside the sensor rail".to_string());
    }
    if via.x < 0.9 || via.y < 0.9 || via.x > WIDTH - 0.9 || via.y > HEIGHT - 0.9 {
        return Some(format!("line {}", line!()));
    }
    for f in &p.board.footprints {
        for pad in &f.pads {
            let c = f.pad_world_position(pad);
            let r = pad.bounding_radius();
            if pad.net == Some(net) {
                if c.distance(from) > 0.01 && c.distance(via) - r - VIA / 2.0 < 0.1 {
                    return Some(format!("line {}", line!()));
                }
                continue;
            }
            if c.distance(from) - r - SIGNAL / 2.0 > 2.5 && c.distance(via) - r - VIA / 2.0 > 2.5 {
                continue;
            }
            if seg_to_pad(from, via, f, pad) - SIGNAL / 2.0 < margin
                || seg_to_pad(via, via, f, pad) - VIA / 2.0 < margin
            {
                return Some(format!("line {}", line!()));
            }
        }
    }
    for t in &p.board.tracks {
        if t.net != net && (seg_dist(via, t.start, t.end) - t.width / 2.0 - VIA / 2.0 < margin) {
            return Some(format!("line {}", line!()));
        }
    }
    for v in &p.board.vias {
        if via.distance(v.position) - v.diameter / 2.0 - VIA / 2.0 < margin + 0.1 {
            return Some(format!("line {}", line!()));
        }
    }
    for h in &p.board.holes {
        if via.distance(h.start()) - h.diameter / 2.0 - VIA / 2.0 < 0.5 {
            return Some(format!("line {}", line!()));
        }
    }
    None
}

/// Eight compass directions, nearest to `prefer` first.
fn directions(prefer: Point) -> Vec<(f64, f64)> {
    let diag = std::f64::consts::FRAC_1_SQRT_2;
    let mut d: Vec<(f64, f64)> = vec![
        (1.0, 0.0),
        (diag, diag),
        (0.0, 1.0),
        (-diag, diag),
        (-1.0, 0.0),
        (-diag, -diag),
        (0.0, -1.0),
        (diag, -diag),
    ];
    d.sort_by(|a, b| {
        let da = a.0 * prefer.x + a.1 * prefer.y;
        let db = b.0 * prefer.x + b.1 * prefer.y;
        db.partial_cmp(&da).unwrap_or(std::cmp::Ordering::Equal)
    });
    d
}

fn snap(v: f64) -> f64 {
    (v * 20.0).round() / 20.0
}

/// Drop a via next to a pad, joined to it by a short top-layer stub.
/// Returns the via centre when one fits.
/// `fanout`, optionally without the full check per candidate (the
/// geometric pre-check still applies; the caller checks the whole board).
pub fn fanout_with(
    g: &mut Gen,
    net: &str,
    reference: &str,
    pad: &str,
    prefer: Point,
    verify: bool,
) -> Res<Option<Point>> {
    let at = pad_at(g, reference, pad)?;
    let n = net_id(g, net)?;
    for dist in [0.85, 1.05, 1.3, 1.7, 2.2] {
        for (dx, dy) in directions(prefer) {
            // Axis stubs keep their length; diagonal stubs move the same
            // distance along both axes so the stub stays at 45 degrees.
            let (ox, oy) = if dx != 0.0 && dy != 0.0 {
                let step = snap(dist * std::f64::consts::FRAC_1_SQRT_2);
                (dx.signum() * step, dy.signum() * step)
            } else {
                (snap(dx * dist), snap(dy * dist))
            };
            let v = Point::new(at.x + ox, at.y + oy);
            if let Some(why) = stub_why(g, n, at, v) {
                if std::env::var_os("SHOWCASE_DEBUG_STUB").is_some() {
                    println!("stub {reference}.{pad} {v:?}: {why}");
                }
                continue;
            }
            let steps = vec![
                (
                    "track.add",
                    json!({"net": n, "start": pt(at.x, at.y), "end": pt(v.x, v.y), "width": SIGNAL, "layer": "F.Cu"}),
                ),
                (
                    "via.place",
                    json!({"net": n, "position": pt(v.x, v.y), "diameter": VIA, "drill": DRILL}),
                ),
            ];
            if verify {
                if attempt(g, steps) {
                    return Ok(Some(v));
                }
            } else {
                let cmds: Vec<Command> = steps
                    .into_iter()
                    .map(|(c, p)| Command {
                        command: c.into(),
                        params: p,
                    })
                    .collect();
                if g.s.batch(&cmds, None).is_ok() {
                    return Ok(Some(v));
                }
            }
        }
    }
    Ok(None)
}

/// Pads of one net, from the design data.
pub fn net_pads(net: &str) -> Vec<(&'static str, &'static str)> {
    PINS.iter()
        .flat_map(|(r, pins)| {
            pins.iter()
                .filter(move |(_, n)| *n == net)
                .map(move |(p, _)| (*r, *p))
        })
        .collect()
}

/// Print findings by code, for the generator log.
pub fn report(g: &Gen) {
    let mut counts = std::collections::BTreeMap::<String, usize>::new();
    for f in pcbcraft_rules::drc(&g.s.project) {
        *counts.entry(f.code.to_string()).or_default() += 1;
    }
    println!("drc by code: {counts:?}");
}

/// Give every pad of a plane net a short stub and a via, pointing away from its part.
pub fn plane_vias(
    g: &mut Gen,
    net: &str,
    only: Option<&[(&str, &str)]>,
    verify: bool,
) -> Res<Vec<String>> {
    let mut failed = Vec::new();
    for (r, p) in net_pads(net) {
        if only.is_some_and(|o| !o.contains(&(r, p))) {
            continue;
        }
        let at = pad_at(g, r, p)?;
        let centre =
            g.s.project
                .board
                .footprints
                .iter()
                .find(|f| Some(f.component) == g.comp.get(r).copied())
                .map(|f| f.position)
                .unwrap_or(at);
        let prefer = Point::new(at.x - centre.x, at.y - centre.y);
        if fanout_with(g, net, r, p, prefer, verify)?.is_none() {
            failed.push(format!("{net}: {r}.{p}"));
        }
    }
    Ok(failed)
}

/// Place every footprint and set the board basics.
pub fn build(g: &mut Gen) -> Res<()> {
    g.cmd("board.resize", json!({"width": WIDTH, "height": HEIGHT}))?;
    g.cmd("board.set_outline", json!({"points": ring(0.0)}))?;
    for (name, kind, t) in [
        ("F.Cu", "copper", 0.035),
        ("Prepreg 1", "prepreg", 0.2104),
        ("In1.Cu", "copper", 0.0152),
        ("Core", "core", 1.065),
        ("In2.Cu", "copper", 0.0152),
        ("Prepreg 2", "prepreg", 0.2104),
        ("B.Cu", "copper", 0.035),
    ] {
        g.cmd(
            "board.set_layer_record",
            json!({"name": name, "kind": kind, "thickness": t}),
        )?;
    }
    g.cmd(
        "board.set_copper_layer_role",
        json!({"layer": "In1.Cu", "role": "ground"}),
    )?;
    g.cmd(
        "board.set_copper_layer_role",
        json!({"layer": "In2.Cu", "role": "power"}),
    )?;
    g.cmd("board.set_finish", json!({"finish": "ENIG"}))?;
    g.cmd("board.rules.set", json!({"clearance": 0.15}))?;
    for (reference, x, y, rot) in layout::PLACEMENT {
        let cid = g.id_of(reference)?;
        let fid = footprint_id(g, cid)?;
        g.cmd("footprint.move", json!({"id": fid, "position": pt(*x, *y)}))?;
        g.cmd("footprint.rotate", json!({"id": fid, "degrees": rot}))?;
    }
    for line in layout::audit(&g.s.project) {
        println!("layout: {line}");
    }
    if std::env::var_os("SHOWCASE_PADS").is_some() {
        for (reference, pins) in PINS {
            let line: Vec<String> = pins
                .iter()
                .filter(|(_, n)| !["GND", "3V3"].contains(n))
                .filter_map(|(p, n)| {
                    pad_at(g, reference, p)
                        .ok()
                        .map(|a| format!("{p}:{n}({:.2},{:.2})", a.x, a.y))
                })
                .collect();
            println!("pads {reference}: {}", line.join(" "));
        }
    }
    let stage = std::env::var("SHOWCASE_STAGE").unwrap_or_default();
    if stage == "place" {
        return Ok(());
    }
    super::finish::fittings(g)?;
    let started = pcbcraft_clock::instant_now();
    let refused = super::routes::all(g)?;
    println!(
        "planned routes done {:?} refused {refused} checks {:?}",
        started.elapsed(),
        check_stats()
    );
    report(g);
    println!("open connections {}", open_count(g));
    if stage == "routes" {
        return Ok(());
    }
    super::finish::rules(g)?;
    super::finish::planes(g)?;
    println!("planes done {:?}", started.elapsed());
    super::finish::artwork(g)?;
    super::finish::cap_vias_in_pads(g)?;
    super::finish::builds(g)?;
    report(g);
    for f in pcbcraft_rules::drc(&g.s.project)
        .iter()
        .filter(|f| f.code != "unrouted")
    {
        println!(
            "finding {}: {} [{}]",
            f.code,
            f.message,
            f.objects
                .iter()
                .map(|o| describe(g, *o))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    for f in pcbcraft_rules::erc(&g.s.project) {
        println!("erc {}: {} {:?}", f.code, f.message, f.objects);
    }
    for w in pcbcraft_rules::airwires(&g.s.project) {
        println!(
            "open {} ({:.2},{:.2}) to ({:.2},{:.2})",
            g.s.project.net_name(w.net),
            w.from.x,
            w.from.y,
            w.to.x,
            w.to.y
        );
    }
    println!("open connections {}", open_count(g));
    Ok(())
}

/// Board footprint id of a component.
pub fn footprint_id(g: &Gen, component: u64) -> Res<u64> {
    g.s.project
        .board
        .footprints
        .iter()
        .find(|f| f.component == component)
        .map(|f| f.id)
        .ok_or_else(|| "footprint missing".to_string())
}

/// World position of a pad.
pub fn pad_at(g: &Gen, reference: &str, pad: &str) -> Res<Point> {
    let cid = g.id_of(reference)?;
    g.s.project
        .board
        .footprints
        .iter()
        .find(|f| f.component == cid)
        .and_then(|f| f.pad_position(pad))
        .ok_or_else(|| format!("no pad {reference}.{pad}"))
}
