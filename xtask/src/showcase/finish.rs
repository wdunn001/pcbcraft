//! Everything on the board that is not a part or a hand-planned route:
//! mounting holes, plane vias, copper pours, stitching, classes, pair rules,
//! silkscreen, the user layers, builds and output jobs.
//!
//! # Rules
//! - Pours go in after every track, because each fill is computed from the
//!   copper that exists when the check runs.
//! - Vias placed after the pours are placed by geometry first and verified by
//!   one whole-board check at the end of each round.
//!
//! # Dependencies
//! - xtask/src/showcase/board.rs
//! - xtask/src/showcase/design.rs
//!
//! # Do not
//! - Place a part here.
use super::board::{self, pad_at};
use super::{Gen, Res};
use pcbcraft_engine::Command;
use pcbcraft_geom::Point;
use serde_json::{Value, json};

fn pt(x: f64, y: f64) -> Value {
    json!({"x": x, "y": y})
}

fn closed_rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<Value> {
    vec![pt(x0, y0), pt(x1, y0), pt(x1, y1), pt(x0, y1), pt(x0, y0)]
}

/// Mounting holes, a lanyard cutout and assembly marks.
pub fn fittings(g: &mut Gen) -> Res<()> {
    for (x, y) in [(2.8, 2.8), (69.2, 2.8), (2.8, 57.2), (69.2, 57.2)] {
        g.cmd(
            "board.hole_place",
            json!({"position": pt(x, y), "diameter": 2.2}),
        )?;
    }
    g.cmd(
        "board.cutout.add",
        json!({"center": pt(42.0, 57.3), "radius": 1.5}),
    )?;
    for (x, y) in [(14.0, 57.0), (57.0, 57.2), (69.4, 22.0)] {
        g.cmd(
            "board.fiducial_place",
            json!({"position": pt(x, y), "diameter": 1.0}),
        )?;
    }
    Ok(())
}

/// Net classes, the USB pair and the rule area.
pub fn rules(g: &mut Gen) -> Res<()> {
    let id = |g: &Gen, n: &str| g.nets[n];
    let power: Vec<u64> = ["VBUS_RAW", "VBUS", "VBAT"]
        .iter()
        .map(|n| id(g, n))
        .collect();
    let r = g.cmd(
        "netclass.add",
        json!({"name": "Power", "clearance": 0.15, "width": 0.4, "nets": power}),
    )?;
    let power_class = r["id"].as_u64().ok_or("class id")?;
    g.cmd(
        "netclass.width_rules",
        json!({"class": power_class, "min_width": 0.3}),
    )?;
    g.cmd(
        "netclass.set",
        json!({"class": power_class, "via_diameter": 0.6, "via_drill": 0.3}),
    )?;
    let usb = [id(g, "USB_DP"), id(g, "USB_DN")];
    let r = g.cmd(
        "netclass.add",
        json!({"name": "USB", "clearance": 0.15, "width": 0.2, "nets": usb}),
    )?;
    let usb_class = r["id"].as_u64().ok_or("class id")?;
    g.cmd(
        "netclass.set",
        json!({"class": usb_class, "pair_width": 0.2, "pair_gap": 0.3}),
    )?;
    g.cmd("board.set_diffpair", json!({"name": "USB", "net_p": usb[0], "net_n": usb[1], "gap": 0.3, "length_tolerance": 3.0}))?;
    let length = |g: &Gen, net: u64| -> f64 {
        g.s.project
            .board
            .tracks
            .iter()
            .filter(|t| t.net == net)
            .map(|t| t.start.distance(t.end))
            .sum()
    };
    let target = length(g, usb[0]).max(length(g, usb[1])).ceil();
    g.cmd(
        "netclass.length_rules",
        json!({"class": usb_class, "target_length": target, "length_tolerance": 3.0}),
    )?;
    g.cmd("rules.area_add", json!({"name": "Under the cover", "clearance": 0.15, "x": 61.9, "y": 3.4, "width": 8.7, "height": 45.2, "max_height": 3.5}))?;
    Ok(())
}

fn zone(g: &mut Gen, layer: &str, net: &str, outline: Vec<Value>) -> Res<u64> {
    let n = g.nets[net];
    let r = g.cmd(
        "zone.place",
        json!({"net": n, "layer": layer, "outline": outline}),
    )?;
    let id = r["id"].as_u64().ok_or("zone id")?;
    g.cmd("zone.pour", json!({"id": id, "pad_connect": "solid", "via_connect": "solid", "min_width": 0.2, "islands": "keep"}))?;
    Ok(id)
}

/// Pads of a net that an airwire still leaves unconnected.
fn stranded(g: &Gen, net: &str) -> Vec<(&'static str, &'static str)> {
    let id = g.nets[net];
    let wires = pcbcraft_rules::airwires(&g.s.project);
    board::net_pads(net)
        .into_iter()
        .filter(|(r, p)| {
            pad_at(g, r, p).is_ok_and(|at| {
                wires.iter().any(|w| {
                    w.net == id && (w.from.distance(at) < 1e-3 || w.to.distance(at) < 1e-3)
                })
            })
        })
        .collect()
}

/// Plane vias, the four pours, the pads the pours miss and the stitching.
pub fn planes(g: &mut Gen) -> Res<()> {
    let rest: Vec<(&str, &str)> = board::net_pads("3V3")
        .into_iter()
        .filter(|p| *p != ("U7", "1"))
        .collect();
    let mut failed = board::plane_vias(g, "3V3", Some(&rest), true)?;
    failed.extend(board::plane_vias(g, "3V3S", None, true)?);
    for f in &failed {
        println!("no plane via: {f}");
    }
    let outline = board::closed_ring(0.5);
    let hole = |r: f64| -> Vec<Value> {
        (0..=16)
            .map(|i| {
                let a = f64::from(i % 16) * std::f64::consts::TAU / 16.0;
                pt(
                    ((42.0 + r * a.cos()) * 1000.0).round() / 1000.0,
                    ((57.3 + r * a.sin()) * 1000.0).round() / 1000.0,
                )
            })
            .collect()
    };
    let z = zone(g, "In1.Cu", "GND", outline.clone())?;
    g.cmd("zone.cutout", json!({"zone": z, "outline": hole(2.2)}))?;
    // The sensor rail is an island of the inner power layer; the main 3V3 pour
    // leaves a gap round it.
    let rail = |grow: f64| -> Vec<Value> {
        closed_rect(61.9 - grow, 3.4 - grow, 70.6 + grow, 48.6 + grow)
    };
    let z = zone(g, "In2.Cu", "3V3", outline.clone())?;
    g.cmd("zone.cutout", json!({"zone": z, "outline": hole(2.2)}))?;
    g.cmd("zone.cutout", json!({"zone": z, "outline": rail(0.5)}))?;
    zone(g, "In2.Cu", "3V3S", rail(0.0))?;
    let back = zone(g, "B.Cu", "GND", outline.clone())?;
    g.cmd("zone.cutout", json!({"zone": back, "outline": hole(2.2)}))?;
    let z = zone(g, "F.Cu", "GND", outline)?;
    g.cmd("zone.cutout", json!({"zone": z, "outline": hole(2.2)}))?;
    for round in 0..4 {
        let mut any = false;
        for net in ["GND", "3V3", "3V3S"] {
            let list = stranded(g, net);
            if list.is_empty() {
                continue;
            }
            any = true;
            println!("round {round}: {net} pads still open {list:?}");
            for f in board::plane_vias(g, net, Some(&list), false)? {
                println!("no plane via: {f}");
            }
        }
        if !any {
            break;
        }
    }
    stitch(g)?;
    for round in 0..3 {
        let n = join_islands(g)?;
        println!("island round {round}: {n} vias");
        if n == 0 {
            break;
        }
    }
    Ok(())
}

fn ring_has(ring: &[Point], p: Point) -> bool {
    let mut inside = false;
    for i in 0..ring.len() {
        let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
    }
    inside
}

fn ring_edge_distance(ring: &[Point], p: Point) -> f64 {
    let mut best = f64::INFINITY;
    for i in 0..ring.len() {
        let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let len2 = dx * dx + dy * dy;
        let t = if len2 < 1e-12 {
            0.0
        } else {
            (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0)
        };
        best = best.min(p.distance(Point::new(a.x + t * dx, a.y + t * dy)));
    }
    best
}

/// Give every detached pour piece a via of its own net, so the check that
/// reports detached copper has nothing to report.
fn join_islands(g: &mut Gen) -> Res<usize> {
    let mut wanted: Vec<(u64, Point)> = Vec::new();
    let mut cut: Vec<(u64, Vec<Value>)> = Vec::new();
    let project = g.s.project.clone();
    for zone in &project.board.zones {
        let fill = pcbcraft_doc::zone_fill(&project, zone).map_err(|e| e.to_string())?;
        for piece in &fill.pieces {
            let Some(outer) = piece.first() else { continue };
            let touched = project
                .board
                .vias
                .iter()
                .any(|v| v.net == zone.net && ring_has(outer, v.position))
                || project.board.footprints.iter().any(|f| {
                    f.pads.iter().any(|p| {
                        p.net == Some(zone.net) && ring_has(outer, f.pad_world_position(p))
                    })
                });
            if touched {
                continue;
            }
            let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
            for q in outer {
                x0 = x0.min(q.x);
                y0 = y0.min(q.y);
                x1 = x1.max(q.x);
                y1 = y1.max(q.y);
            }
            let mut found = None;
            let mut y = y0;
            'scan: while y <= y1 {
                let mut x = x0;
                while x <= x1 {
                    let p = Point::new(x, y);
                    let in_hole = piece.iter().skip(1).any(|h| ring_has(h, p));
                    if ring_has(outer, p)
                        && !in_hole
                        && ring_edge_distance(outer, p) > 0.5
                        && piece.iter().skip(1).all(|h| ring_edge_distance(h, p) > 0.5)
                    {
                        found = Some(p);
                        break 'scan;
                    }
                    x += 0.2;
                }
                y += 0.2;
            }
            if let Some(p) = found {
                wanted.push((zone.net, p));
            } else if (x1 - x0) * (y1 - y0) < 30.0 {
                // Too thin for a via: take the sliver out of the pour instead.
                cut.push((zone.id, closed_rect(x0 - 0.1, y0 - 0.1, x1 + 0.1, y1 + 0.1)));
            } else {
                println!(
                    "no room for a via in a detached piece of zone {} ({x0:.1},{y0:.1})-({x1:.1},{y1:.1})",
                    zone.id
                );
            }
        }
    }
    let cmds: Vec<Command> = wanted
        .iter()
        .map(|(net, p)| Command { command: "via.place".into(), params: json!({"net": net, "position": pt(p.x, p.y), "diameter": board::VIA, "drill": board::DRILL}) })
        .collect();
    for chunk in cmds.chunks(100) {
        g.s.batch(chunk, None).map_err(|e| e.to_string())?;
    }
    let slivers = cut.len();
    for (zone, outline) in cut {
        g.cmd("zone.cutout", json!({"zone": zone, "outline": outline}))?;
    }
    Ok(cmds.len() + slivers)
}

/// A grid of ground vias in every spot with room, away from the rail pour,
/// the logo plate and the board edge.
fn stitch(g: &mut Gen) -> Res<()> {
    let gnd = g.nets["GND"];
    let mut steps: Vec<Command> = Vec::new();
    let mut placed: Vec<Point> = Vec::new();
    let mut y = 4.5;
    while y < 56.0 {
        let mut x = 4.5;
        while x < 68.0 {
            let p = Point::new(x, y);
            let in_rail = (60.0..71.5).contains(&x) && (2.5..49.5).contains(&y);
            let in_plate = (2.0..16.5).contains(&x) && (19.5..42.5).contains(&y);
            let near = placed.iter().any(|q| q.distance(p) < 3.0);
            if !in_rail
                && !in_plate
                && !near
                && board::stub_why_margin(g, gnd, p, p, 0.45).is_none()
            {
                steps.push(Command { command: "via.place".into(), params: json!({"net": gnd, "position": pt(x, y), "diameter": board::VIA, "drill": board::DRILL}) });
                placed.push(p);
            }
            x += 4.5;
        }
        y += 4.5;
    }
    for chunk in steps.chunks(100) {
        g.s.batch(chunk, None).map_err(|e| e.to_string())?;
    }
    println!("stitching vias {}", placed.len());
    Ok(())
}

/// Silkscreen, the logo layer and the cut line.
pub fn artwork(g: &mut Gen) -> Res<()> {
    for (text, x, y, size) in [
        ("SENSOR NODE", 2.5, 22.5, 1.2),
        ("PCBCraft showcase", 2.5, 24.6, 0.8),
        ("Rev A  CC BY-SA 4.0", 2.5, 40.5, 0.7),
    ] {
        g.cmd(
            "text.place",
            json!({"text": text, "position": pt(x, y), "size": size, "layer": "F.Cu"}),
        )?;
    }
    let logo = g.cmd("board.layer_add", json!({"name": "Logo", "kind": "silk"}))?["id"]
        .as_u64()
        .ok_or("layer id")?;
    // A leaf over three signal arcs, centred on the plate.
    let (cx, cy) = (9.0, 32.0);
    g.cmd(
        "board.layer_circle_add",
        json!({"layer": logo, "x": cx, "y": cy, "radius": 1.0}),
    )?;
    for (i, r) in [3.0, 5.0, 7.0].iter().enumerate() {
        let mut pts: Vec<Value> = Vec::new();
        for k in 0..=12 {
            let a = (-50.0 + 100.0 * f64::from(k) / 12.0_f64).to_radians();
            pts.push(json!([
                ((cx + r * a.cos()) * 1000.0).round() / 1000.0,
                ((cy - r * a.sin() * 0.0 - r * a.sin()) * 1000.0).round() / 1000.0
            ]));
        }
        let _ = i;
        g.cmd(
            "board.layer_poly_add",
            json!({"layer": logo, "points": pts}),
        )?;
    }
    let cut = g.cmd(
        "board.layer_add",
        json!({"name": "Cut line", "kind": "cut"}),
    )?["id"]
        .as_u64()
        .ok_or("layer id")?;
    for ((x1, y1), (x2, y2)) in [
        ((3.0, 20.5), (15.5, 20.5)),
        ((15.5, 20.5), (15.5, 41.5)),
        ((15.5, 41.5), (3.0, 41.5)),
        ((3.0, 41.5), (3.0, 20.5)),
    ] {
        g.cmd(
            "board.layer_line_add",
            json!({"layer": cut, "x1": x1, "y1": y1, "x2": x2, "y2": y2}),
        )?;
    }
    Ok(())
}

/// Fill and cap every via that sits inside a pad, so a via under a test
/// point does not wick solder away from the joint.
///
/// # Rules
/// - Runs after every via is placed and before the builds.
/// - Uses the same test the design check uses, so the check stays quiet.
///
/// # Dependencies
/// - crates/rules/src/lib.rs `via_in_pad_message`.
/// - crates/engine/src/via_protection.rs `via.set_covering`.
///
/// # Do not
/// - Move a via; only its protection changes.
pub fn cap_vias_in_pads(g: &mut Gen) -> Res<()> {
    let ids: Vec<u64> =
        g.s.project
            .board
            .vias
            .iter()
            .filter(|v| pcbcraft_rules::via_in_pad_message(&g.s.project, v).is_some())
            .map(|v| v.id)
            .collect();
    for id in ids {
        g.cmd(
            "via.set_covering",
            json!({"id": id, "covering": "filled_capped"}),
        )?;
    }
    Ok(())
}

/// The two builds and the fabrication recipes.
pub fn builds(g: &mut Gen) -> Res<()> {
    g.cmd(
        "variant.create",
        json!({"name": "Full", "note": "Every part fitted, including both sensors."}),
    )?;
    let sensors: Vec<u64> = ["U5", "C14", "C15", "U6", "C16", "R8", "R9"]
        .iter()
        .map(|r| g.comp[r])
        .collect();
    g.cmd("variant.create", json!({"name": "Lean", "note": "No sensors fitted; the input capacitor drops to a smaller package.", "dnp": sensors}))?;
    let c1 = g.comp["C1"];
    g.cmd(
        "variant.set_footprint",
        json!({"name": "Lean", "component": c1, "footprint": "Chip_0402"}),
    )?;
    g.cmd(
        "output_job.add",
        json!({"name": "Fabrication", "kinds": ["gerbers", "drill_chart", "stackup", "ipc356"]}),
    )?;
    g.cmd(
        "output_job.add",
        json!({"name": "Assembly", "kinds": ["bom", "placement", "netlist"]}),
    )?;
    g.cmd(
        "output_job.add",
        json!({"name": "Documents", "kinds": ["pdf", "svg", "stats", "report"]}),
    )?;
    Ok(())
}
