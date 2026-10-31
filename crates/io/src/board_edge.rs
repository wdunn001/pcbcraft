//! Board outline for fabrication: true arcs in Gerber, DXF and SVG; flattened
//! rings everywhere else.
use pcbcraft_doc::{Project, segment_arc};
use pcbcraft_geom::{Arc, Point};
use std::f64::consts::PI;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum EdgeSegment {
    Line { from: Point, to: Point },
    Arc { from: Point, to: Point, arc: Arc },
}

pub(crate) fn rectangle_points(p: &Project) -> Vec<Point> {
    vec![
        Point::new(0.0, 0.0),
        Point::new(p.board.width, 0.0),
        Point::new(p.board.width, p.board.height),
        Point::new(0.0, p.board.height),
    ]
}

pub(crate) fn outline_vertices(p: &Project) -> Option<Vec<Point>> {
    p.board.outline.as_ref().filter(|o| o.len() >= 3).cloned()
}

pub(crate) fn segments(p: &Project) -> Vec<EdgeSegment> {
    let Some(points) = outline_vertices(p) else {
        let rect = rectangle_points(p);
        let mut out: Vec<EdgeSegment> = rect
            .windows(2)
            .map(|w| EdgeSegment::Line {
                from: w[0],
                to: w[1],
            })
            .collect();
        out.push(EdgeSegment::Line {
            from: rect[3],
            to: rect[0],
        });
        return out;
    };
    let n = points.len();
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let from = points[i];
        let to = points[(i + 1) % n];
        if let Some(arc) = segment_arc(&points, &p.board.outline_arcs, i) {
            out.push(EdgeSegment::Arc { from, to, arc });
        } else {
            out.push(EdgeSegment::Line { from, to });
        }
    }
    out
}

pub(crate) fn flat_ring(p: &Project, max_chord_error: f64) -> Vec<Point> {
    let ring = p.board.edge_ring(max_chord_error);
    if ring.is_empty() {
        rectangle_points(p)
    } else {
        ring
    }
}

pub(crate) fn flat_tuples(p: &Project, max_chord_error: f64) -> Vec<(f64, f64)> {
    flat_ring(p, max_chord_error)
        .iter()
        .map(|pt| (pt.x, pt.y))
        .collect()
}

pub(crate) fn gerber_body(scale: f64, p: &Project) -> Result<(String, usize), String> {
    let segs = segments(p);
    if segs.is_empty() {
        return Ok((String::new(), 0));
    }
    let mut out = String::new();
    let mut ops = 0usize;
    for (i, seg) in segs.iter().enumerate() {
        match seg {
            EdgeSegment::Line { from, to } => {
                if i == 0 {
                    out.push_str(&format!(
                        "X{:.6}Y{:.6}D02*\n",
                        from.x * scale,
                        from.y * scale
                    ));
                }
                out.push_str(&format!("X{:.6}Y{:.6}D01*\n", to.x * scale, to.y * scale));
                ops += 1;
            }
            EdgeSegment::Arc { from, to, arc } => {
                if i == 0 {
                    out.push_str(&format!(
                        "X{:.6}Y{:.6}D02*\n",
                        from.x * scale,
                        from.y * scale
                    ));
                }
                out.push_str("G75*\n");
                let clockwise = arc.sweep < 0.0;
                out.push_str(&format!(
                    "X{:.6}Y{:.6}D02*\nX{:.6}Y{:.6}I{:.6}J{:.6}G0{}*\nG01*\n",
                    from.x * scale,
                    from.y * scale,
                    to.x * scale,
                    to.y * scale,
                    (arc.center.x - from.x) * scale,
                    (arc.center.y - from.y) * scale,
                    if clockwise { 2 } else { 3 }
                ));
                ops += 1;
            }
        }
    }
    Ok((out, ops))
}

pub(crate) fn svg_path_d(p: &Project) -> String {
    let segs = segments(p);
    let mut d = String::new();
    for (i, seg) in segs.iter().enumerate() {
        match seg {
            EdgeSegment::Line { from, to } => {
                if i == 0 {
                    d.push_str(&format!("M {} {}", from.x, from.y));
                }
                d.push_str(&format!(" L {} {}", to.x, to.y));
            }
            EdgeSegment::Arc { from, to, arc } => {
                if i == 0 {
                    d.push_str(&format!("M {} {}", from.x, from.y));
                }
                d.push_str(&format!(
                    " A {:.6} {:.6} 0 {} {} {} {}",
                    arc.radius,
                    arc.radius,
                    usize::from(arc.sweep.abs() > PI),
                    usize::from(arc.sweep < 0.0),
                    to.x,
                    to.y
                ));
            }
        }
    }
    d.push_str(" Z");
    d
}

pub(crate) fn write_dxf_edge(w: &mut super::DxfWriter, p: &Project) -> Result<(), String> {
    let segs = segments(p);
    if segs.is_empty() {
        return Ok(());
    }
    let all_lines = segs.iter().all(|s| matches!(s, EdgeSegment::Line { .. }));
    if all_lines {
        let mut points: Vec<(f64, f64)> = Vec::with_capacity(segs.len());
        for s in &segs {
            let EdgeSegment::Line { from, .. } = s else {
                return Err("edge segment kind changed".into());
            };
            points.push((from.x, from.y));
        }
        return w.polyline("Edge", &points, true);
    }
    let deg = |a: f64| {
        let mut d = a.to_degrees() % 360.0;
        if d < 0.0 {
            d += 360.0;
        }
        d
    };
    for seg in segs {
        match seg {
            EdgeSegment::Line { from, to } => {
                w.line("Edge", (from.x, from.y), (to.x, to.y))?;
            }
            EdgeSegment::Arc { arc, .. } => {
                let end_angle = arc.start_angle + arc.sweep;
                let (from, to) = if arc.sweep >= 0.0 {
                    (arc.start_angle, end_angle)
                } else {
                    (end_angle, arc.start_angle)
                };
                w.arc(
                    "Edge",
                    (arc.center.x, arc.center.y),
                    arc.radius,
                    deg(from),
                    deg(to),
                )?;
            }
        }
    }
    Ok(())
}

const DXF_EPS: f64 = 1e-4;

fn points_close(a: Point, b: Point) -> bool {
    (a.x - b.x).abs() <= DXF_EPS && (a.y - b.y).abs() <= DXF_EPS
}

fn dxf_arc_endpoints(center: Point, radius: f64, start_deg: f64, end_deg: f64) -> (Point, Point) {
    let at = |deg: f64| {
        let rad = deg.to_radians();
        Point::new(center.x + radius * rad.cos(), center.y + radius * rad.sin())
    };
    (at(start_deg), at(end_deg))
}

fn dxf_arc_mid(center: Point, radius: f64, start_deg: f64, end_deg: f64) -> Option<Point> {
    let (start, end) = dxf_arc_endpoints(center, radius, start_deg, end_deg);
    let mut sweep = (end_deg - start_deg).to_radians();
    if sweep <= -PI {
        sweep += 2.0 * PI;
    } else if sweep > PI {
        sweep -= 2.0 * PI;
    }
    let mid_angle = start_deg.to_radians() + sweep / 2.0;
    let mid = Point::new(
        center.x + radius * mid_angle.cos(),
        center.y + radius * mid_angle.sin(),
    );
    pcbcraft_geom::arc_through(start, mid, end).map(|_| mid)
}

/// How an imported outline was encoded in the drawing file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DxfOutlineSource {
    /// One closed lightweight polyline entity.
    Polyline(usize),
    /// Several Edge-layer lines and arcs chained into one loop.
    Chained,
}

type DxfOutlineImport = (DxfOutlineSource, Vec<Point>, Vec<Option<Point>>);

/// Closed board outline from DXF entities on the Edge layer: a closed polyline
/// or a single loop of lines and arcs.
pub fn dxf_import_outline(
    entities: &[super::DxfEntity],
) -> Result<Option<DxfOutlineImport>, String> {
    let candidate = |e: &super::DxfEntity| {
        matches!(
            &e.shape,
            super::DxfShape::Polyline { points, closed: true }
                if points.len() >= 3 && points.len() <= 64
        )
    };
    if let Some(index) = entities
        .iter()
        .position(|e| candidate(e) && e.layer.eq_ignore_ascii_case("edge"))
        .or_else(|| entities.iter().position(candidate))
    {
        let super::DxfShape::Polyline { points, .. } = &entities[index].shape else {
            return Err("outline candidate changed".into());
        };
        let distinct: std::collections::BTreeSet<(u64, u64)> = points
            .iter()
            .map(|q| (q.x.to_bits(), q.y.to_bits()))
            .collect();
        if distinct.len() != points.len() {
            return Err("outline vertices must be distinct".into());
        }
        return Ok(Some((
            DxfOutlineSource::Polyline(index),
            points.clone(),
            Vec::new(),
        )));
    }

    #[derive(Clone)]
    struct RawSeg {
        a: Point,
        b: Point,
        mid: Option<Point>,
    }
    let mut raw: Vec<RawSeg> = Vec::new();
    for e in entities {
        if !e.layer.eq_ignore_ascii_case("edge") {
            continue;
        }
        match &e.shape {
            super::DxfShape::Line { start, end } => {
                raw.push(RawSeg {
                    a: *start,
                    b: *end,
                    mid: None,
                });
            }
            super::DxfShape::Arc {
                center,
                radius,
                start_deg,
                end_deg,
            } => {
                let (a, b) = dxf_arc_endpoints(*center, *radius, *start_deg, *end_deg);
                let mid = dxf_arc_mid(*center, *radius, *start_deg, *end_deg);
                raw.push(RawSeg { a, b, mid });
            }
            _ => {}
        }
    }
    if raw.len() < 3 || raw.len() > 64 {
        return Ok(None);
    }
    let mut unused: Vec<usize> = (0..raw.len()).collect();
    let mut chain: Vec<RawSeg> = Vec::with_capacity(raw.len());
    chain.push(raw[unused.remove(0)].clone());
    while !unused.is_empty() {
        let tail = chain.last().map(|s| s.b).ok_or("empty chain")?;
        let pos = unused
            .iter()
            .position(|&i| points_close(raw[i].a, tail) || points_close(raw[i].b, tail))
            .ok_or("edge segments do not form one closed loop")?;
        let idx = unused.remove(pos);
        let mut seg = raw[idx].clone();
        if points_close(seg.b, tail) && !points_close(seg.a, tail) {
            std::mem::swap(&mut seg.a, &mut seg.b);
        }
        if !points_close(seg.a, tail) {
            return Err("edge segments do not form one closed loop".into());
        }
        chain.push(seg);
    }
    let first = chain.first().map(|s| s.a).ok_or("empty chain")?;
    let last = chain.last().map(|s| s.b).ok_or("empty chain")?;
    if !points_close(first, last) {
        return Err("edge segments do not close".into());
    }
    let vertices: Vec<Point> = chain.iter().map(|s| s.a).collect();
    if vertices.len() < 3 || vertices.len() > 64 {
        return Ok(None);
    }
    let distinct: std::collections::BTreeSet<(u64, u64)> = vertices
        .iter()
        .map(|q| (q.x.to_bits(), q.y.to_bits()))
        .collect();
    if distinct.len() != vertices.len() {
        return Err("outline vertices must be distinct".into());
    }
    let arcs: Vec<Option<Point>> = chain.iter().map(|s| s.mid).collect();
    Ok(Some((DxfOutlineSource::Chained, vertices, arcs)))
}
