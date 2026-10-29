//! Hand-planned copper: Manhattan waypoints with 45-degree corner cuts.
//!
//! A route is a list of waypoints. The generator cuts every corner at 45
//! degrees, places a via where a waypoint says so and switches copper layer
//! there. The whole route is one atomic batch that is kept only when it adds
//! no finding, and the first new finding is printed when it is refused.
//!
//! # Rules
//! - Waypoints are millimetres from the top-left corner of the board.
//! - A route that fails leaves the session exactly as it was.
//!
//! # Dependencies
//! - xtask/src/showcase/board.rs
//!
//! # Do not
//! - Call this before every footprint is placed.
use super::{Gen, Res, board};
use pcbcraft_geom::Point;
use serde_json::{Value, json};

/// Corner cut length in millimetres.
const CUT: f64 = 0.6;

/// One waypoint of a route.
#[derive(Clone, Copy, Debug)]
pub enum W {
    /// The centre of a pad.
    P(&'static str, &'static str),
    /// An absolute point.
    A(f64, f64),
    /// Move along x to this value, keeping y.
    X(f64),
    /// Move along y to this value, keeping x.
    Y(f64),
    /// Start on the bottom layer instead of the top.
    B,
    /// A via at the current point; the route continues on the other outer layer.
    V,
}

fn unit(a: Point, b: Point) -> (Point, f64) {
    let d = a.distance(b);
    if d < 1e-9 {
        (Point::new(0.0, 0.0), 0.0)
    } else {
        (Point::new((b.x - a.x) / d, (b.y - a.y) / d), d)
    }
}

/// Lay one route; returns whether it was kept.
pub fn lay(g: &mut Gen, net: &str, width: f64, w: &[W]) -> Res<bool> {
    let n = *g
        .nets
        .get(net)
        .ok_or_else(|| format!("unknown net {net}"))?;
    let mut pts: Vec<(Point, bool)> = Vec::new();
    let mut layer = "F.Cu";
    for item in w {
        match *item {
            W::B => layer = "B.Cu",
            W::P(r, p) => pts.push((board::pad_at(g, r, p)?, false)),
            W::A(x, y) => pts.push((Point::new(x, y), false)),
            W::X(x) => {
                let y = pts.last().ok_or("X first")?.0.y;
                pts.push((Point::new(x, y), false));
            }
            W::Y(y) => {
                let x = pts.last().ok_or("Y first")?.0.x;
                pts.push((Point::new(x, y), false));
            }
            W::V => {
                if let Some(last) = pts.last_mut() {
                    last.1 = true;
                }
            }
        }
    }
    // Cut corners.
    let mut out: Vec<(Point, bool)> = Vec::new();
    for i in 0..pts.len() {
        let (p, via) = pts[i];
        if i == 0 || i + 1 == pts.len() || via {
            out.push((p, via));
            continue;
        }
        let (u1, l1) = unit(pts[i - 1].0, p);
        let (u2, l2) = unit(p, pts[i + 1].0);
        let cross = u1.x * u2.y - u1.y * u2.x;
        if cross.abs() < 0.99 || l1 < 1e-6 || l2 < 1e-6 {
            out.push((p, false));
            continue;
        }
        let c = CUT.min(l1 / 2.0).min(l2 / 2.0);
        out.push((Point::new(p.x - u1.x * c, p.y - u1.y * c), false));
        out.push((Point::new(p.x + u2.x * c, p.y + u2.y * c), false));
    }
    for pair in out.windows(2) {
        let (dx, dy) = (pair[1].0.x - pair[0].0.x, pair[1].0.y - pair[0].0.y);
        let off = if dx.abs() < 1e-6 || dy.abs() < 1e-6 {
            0.0
        } else {
            (dx.abs() - dy.abs()).abs()
        };
        if off > 1e-3 {
            println!(
                "not 45 degrees: {net} ({:.2},{:.2}) to ({:.2},{:.2})",
                pair[0].0.x, pair[0].0.y, pair[1].0.x, pair[1].0.y
            );
        }
    }
    let mut steps: Vec<(&str, Value)> = Vec::new();
    for i in 0..out.len() {
        if i > 0 {
            let (a, b) = (out[i - 1].0, out[i].0);
            if a.distance(b) > 1e-6 {
                steps.push(("track.add", json!({"net": n, "start": {"x": a.x, "y": a.y}, "end": {"x": b.x, "y": b.y}, "width": width, "layer": layer})));
            }
        }
        if out[i].1 {
            steps.push(("via.place", json!({"net": n, "position": {"x": out[i].0.x, "y": out[i].0.y}, "diameter": board::VIA, "drill": board::DRILL})));
            layer = if layer == "F.Cu" { "B.Cu" } else { "F.Cu" };
        }
    }
    let ok = board::attempt_verbose(g, steps, net);
    Ok(ok)
}
