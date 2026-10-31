//! Copper geometry on one layer for fabrication output.
use pcbcraft_doc::{Footprint, Id, Layer, Project};
use pcbcraft_geom::{Point, Ring};

#[derive(Clone, Debug)]
/// One copper region on a layer as a closed ring, with the object that owns it and its net.
pub struct LayerShape {
    /// Id of the pad, track, via or zone this region came from.
    pub id: Id,
    /// Net the region belongs to, used to refuse isolation gaps between nets that are too tight.
    pub net: Id,
    /// Closed ring in board millimetres.
    pub outline: Ring,
}

const EPS: f64 = 1e-6;

fn clean_ring(points: &[Point]) -> Option<Ring> {
    let mut ring: Vec<Point> = points.iter().copied().filter(|p| p.valid()).collect();
    if ring.len() < 3 {
        return None;
    }
    if ring
        .first()
        .zip(ring.last())
        .is_some_and(|(a, b)| a.distance(*b) < EPS)
    {
        ring.pop();
    }
    ring.dedup_by(|a, b| a.distance(*b) < EPS);
    if ring.len() < 3 {
        return None;
    }
    let first = ring[0];
    ring.push(first);
    Some(ring)
}

fn circle_ring(centre: Point, radius: f64) -> Ring {
    let steps = 32;
    let mut ring: Vec<Point> = (0..steps)
        .map(|i| {
            let a = 2.0 * std::f64::consts::PI * (i as f64) / (steps as f64);
            Point::new(centre.x + radius * a.cos(), centre.y + radius * a.sin())
        })
        .collect();
    if let Some(first) = ring.first().copied() {
        ring.push(first);
    }
    ring
}

fn stroke_ring(a: Point, b: Point, width: f64) -> Option<Ring> {
    let len = a.distance(b);
    if len < EPS {
        return None;
    }
    let (dx, dy) = ((b.x - a.x) / len, (b.y - a.y) / len);
    let (nx, ny) = (-dy * width / 2.0, dx * width / 2.0);
    clean_ring(&[
        Point::new(a.x - nx, a.y - ny),
        Point::new(b.x - nx, b.y - ny),
        Point::new(b.x + nx, b.y + ny),
        Point::new(a.x + nx, a.y + ny),
    ])
}

fn track_ring(t: &pcbcraft_doc::Track) -> Option<Ring> {
    let half = t.width / 2.0;
    if let Some(arc) = t.curve() {
        let centerline = pcbcraft_doc::arc_polyline(&arc, crate::DEFAULT_MAX_DEVIATION_MM);
        let steps = centerline.len().saturating_sub(1).max(1);
        let inner = (arc.radius - half).max(0.001);
        let outer = arc.radius + half;
        let at = |i: usize, r: f64| {
            let f = i as f64 / steps as f64;
            let a = arc.start_angle + arc.sweep * f;
            Point::new(arc.center.x + r * a.cos(), arc.center.y + r * a.sin())
        };
        let mut ring: Vec<Point> = (0..=steps).map(|i| at(i, outer)).collect();
        ring.extend((0..=steps).rev().map(|i| at(i, inner)));
        return clean_ring(&ring);
    }
    stroke_ring(t.start, t.end, t.width)
}

fn map_ring(fp: &Footprint, local: &Ring) -> Ring {
    local
        .iter()
        .map(|p| fp.position.offset(p.rotate(-fp.rotation)))
        .collect()
}

fn pad_sides(pad: &pcbcraft_doc::Pad) -> (bool, bool, bool) {
    let Some(g) = &pad.geometry else {
        return (true, false, true);
    };
    let layers = &g.layers;
    let front = layers.iter().any(|l| l == "F.Cu" || l == "*.Cu");
    let back = layers.iter().any(|l| l == "B.Cu" || l == "*.Cu");
    let copper = front || back;
    (front, back, copper)
}

/// Every copper region on one layer, with the owning object id and net.
pub fn layer_shapes(project: &Project, layer: Layer) -> Result<Vec<LayerShape>, String> {
    project.validate()?;
    let top = layer == Layer::Front;
    let mut shapes = Vec::new();
    for fp in &project.board.footprints {
        for pad in &fp.pads {
            let (front, back, copper) = pad_sides(pad);
            if !copper || (top && !front) || (!top && !back) {
                continue;
            }
            let Some(ring) = clean_ring(&map_ring(fp, &pad.outline_points())) else {
                continue;
            };
            shapes.push(LayerShape {
                id: fp.id,
                net: pad.net.unwrap_or(0),
                outline: ring,
            });
        }
    }
    for via in &project.board.vias {
        shapes.push(LayerShape {
            id: via.id,
            net: via.net,
            outline: circle_ring(via.position, via.diameter / 2.0),
        });
    }
    for f in project.board.fiducials.iter().filter(|f| f.layer == layer) {
        shapes.push(LayerShape {
            id: f.id,
            net: 0,
            outline: circle_ring(f.position, f.diameter / 2.0),
        });
    }
    if top {
        for tie in &project.board.netties {
            let (s, c) = tie.rotation.to_radians().sin_cos();
            let r = tie.diameter / 2.0;
            for (net, sign) in [(tie.net_a, -1.0), (tie.net_b, 1.0)] {
                let centre =
                    Point::new(tie.position.x + sign * r * c, tie.position.y + sign * r * s);
                shapes.push(LayerShape {
                    id: tie.id,
                    net,
                    outline: circle_ring(centre, r),
                });
            }
        }
    }
    for t in project.board.tracks.iter().filter(|t| t.layer == layer) {
        if let Some(ring) = track_ring(t) {
            shapes.push(LayerShape {
                id: t.id,
                net: t.net,
                outline: ring,
            });
        }
    }
    for z in project
        .board
        .zones
        .iter()
        .filter(|z| z.layer == layer && z.filled)
    {
        let Some(ring) = clean_ring(&z.outline) else {
            continue;
        };
        shapes.push(LayerShape {
            id: z.id,
            net: z.net,
            outline: ring,
        });
    }
    Ok(shapes)
}
