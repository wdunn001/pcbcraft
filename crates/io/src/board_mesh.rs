//! Pure 3D mesh builder for the board. No rendering and no UI types: the
//! engine, the MCP server and any later viewer read the same triangle lists.
//!
//! Frame: x and y are the board's own millimetre coordinates (the same ones
//! the STEP and VRML writers use), z is up. The bottom face of the board
//! slab sits at z = 0 and the top face at z = board thickness. Every triangle
//! winds counter-clockwise when seen from outside its solid.
//!
//! What it builds: the board slab with its outline, circular cutouts,
//! unplated holes, via and pad drills; top and bottom copper (pads, tracks,
//! vias, poured zones, fiducials, net ties) as thin prisms; a solder mask
//! sheet with pad openings; silk strokes; and one body per placed footprint.
//! Bodies are parametric defaults chosen from the pad pattern. A footprint
//! that references a model carries the exact placement matrix ([`model_matrix`]),
//! and [`with_models`] swaps the box for the loaded model's triangles.
//!
//! Limits are explicit: [`BoardMesh::warnings`] says what was skipped and why.
use crate::model_load::LoadedModel;
use pcbcraft_doc::{ArtKind, ArtShape, Component, Footprint, Id, Layer, Pad, Project};
use pcbcraft_geom::Point;
use pcbcraft_library::Model3D;
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// Copper foil thickness in millimetres (one ounce).
pub const COPPER_THICKNESS: f64 = 0.035;
/// Solder mask thickness above copper in millimetres.
pub const MASK_THICKNESS: f64 = 0.02;
/// Silk screen ink thickness in millimetres.
pub const SILK_THICKNESS: f64 = 0.02;
/// Mask opening grown around every pad, in millimetres.
const MASK_MARGIN: f64 = 0.05;
/// Largest triangle count one mesh may hold.
pub const MAX_TRIANGLES: usize = 600_000;
const MAX_RING: usize = 256;
const MAX_MERGED_VERTICES: usize = 12_000;
const MAX_HOLES_PER_POLYGON: usize = 2_000;
const POST_HEIGHT: f64 = 6.0;
const EPS: f64 = 1e-9;

/// What a part of the mesh is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PartKind {
    Board,
    CopperTop,
    CopperBottom,
    MaskTop,
    MaskBottom,
    SilkTop,
    SilkBottom,
    Body,
}
impl PartKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Board => "board",
            Self::CopperTop => "copper_top",
            Self::CopperBottom => "copper_bottom",
            Self::MaskTop => "mask_top",
            Self::MaskBottom => "mask_bottom",
            Self::SilkTop => "silk_top",
            Self::SilkBottom => "silk_bottom",
            Self::Body => "body",
        }
    }
}

/// Which parts to build. All on by default.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MeshLayers {
    pub board: bool,
    pub copper: bool,
    pub mask: bool,
    pub silk: bool,
    pub bodies: bool,
}
impl Default for MeshLayers {
    fn default() -> Self {
        Self {
            board: true,
            copper: true,
            mask: true,
            silk: true,
            bodies: true,
        }
    }
}

/// Where a model file belongs, as a row-major 4x4 matrix taking the model's
/// own millimetre coordinates to board coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct ModelPlacement {
    pub path: String,
    pub matrix: [f64; 16],
    /// True once [`with_models`] replaced the default box by the file's triangles.
    pub loaded: bool,
}

/// One named triangle list.
#[derive(Clone, Debug, PartialEq)]
pub struct MeshPart {
    pub kind: PartKind,
    pub name: String,
    pub component: Option<Id>,
    /// Package family of a parametric body, such as `chip` or `soic`.
    pub family: Option<String>,
    /// Where a body height came from: `default` or `field`.
    pub height_source: Option<&'static str>,
    /// The placement a loaded model would take, when the footprint names one.
    pub model: Option<ModelPlacement>,
    /// Diffuse colour 0..1 from a loaded model file, when it has one.
    pub color: Option<[f32; 3]>,
    pub positions: Vec<[f64; 3]>,
    pub indices: Vec<u32>,
}
impl MeshPart {
    fn new(kind: PartKind, name: String) -> Self {
        Self {
            kind,
            name,
            component: None,
            family: None,
            height_source: None,
            model: None,
            color: None,
            positions: vec![],
            indices: vec![],
        }
    }
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }
    pub fn bounds(&self) -> Option<([f64; 3], [f64; 3])> {
        let first = *self.positions.first()?;
        let mut min = first;
        let mut max = first;
        for p in &self.positions {
            min = [min[0].min(p[0]), min[1].min(p[1]), min[2].min(p[2])];
            max = [max[0].max(p[0]), max[1].max(p[1]), max[2].max(p[2])];
        }
        Some((min, max))
    }
}

/// The finished mesh.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BoardMesh {
    pub parts: Vec<MeshPart>,
    pub warnings: Vec<String>,
}
impl BoardMesh {
    pub fn vertex_count(&self) -> usize {
        self.parts.iter().map(|p| p.positions.len()).sum()
    }
    pub fn triangle_count(&self) -> usize {
        self.parts.iter().map(MeshPart::triangle_count).sum()
    }
    pub fn bounds(&self) -> Option<([f64; 3], [f64; 3])> {
        let mut found: Option<([f64; 3], [f64; 3])> = None;
        for (lo, hi) in self.parts.iter().filter_map(MeshPart::bounds) {
            found = Some(match found {
                None => (lo, hi),
                Some((min, max)) => (
                    [min[0].min(lo[0]), min[1].min(lo[1]), min[2].min(lo[2])],
                    [max[0].max(hi[0]), max[1].max(hi[1]), max[2].max(hi[2])],
                ),
            });
        }
        found
    }
    /// JSON for the engine and MCP. `geometry` false leaves out the vertex
    /// and index arrays and keeps counts and bounds.
    pub fn to_json(&self, geometry: bool) -> Value {
        let parts: Vec<Value> = self
            .parts
            .iter()
            .map(|part| {
                let mut v = json!({
                    "kind": part.kind.name(),
                    "name": part.name,
                    "vertices": part.positions.len(),
                    "triangles": part.triangle_count(),
                    "bounds": bounds_json(part.bounds()),
                });
                if let Some(map) = v.as_object_mut() {
                    if let Some(id) = part.component {
                        map.insert("component".into(), json!(id));
                    }
                    if let Some(family) = &part.family {
                        map.insert("family".into(), json!(family));
                        map.insert("source".into(), json!("parametric"));
                    }
                    if let Some(source) = part.height_source {
                        map.insert("height_source".into(), json!(source));
                    }
                    if let Some(model) = &part.model {
                        map.insert(
                            "model".into(),
                            json!({"path": model.path, "matrix": model.matrix.to_vec(), "loaded": model.loaded}),
                        );
                    }
                    if geometry {
                        let flat: Vec<f64> =
                            part.positions.iter().flat_map(|p| rounded(*p)).collect();
                        map.insert("positions".into(), json!(flat));
                        map.insert("indices".into(), json!(part.indices));
                    }
                }
                v
            })
            .collect();
        json!({
            "units": "millimetres",
            "axes": "x and y are board coordinates, z is up; the board bottom face is z=0",
            "winding": "counter-clockwise seen from outside",
            "vertices": self.vertex_count(),
            "triangles": self.triangle_count(),
            "bounds": bounds_json(self.bounds()),
            "parts": parts,
            "warnings": self.warnings,
        })
    }
}
fn bounds_json(b: Option<([f64; 3], [f64; 3])>) -> Value {
    match b {
        Some((lo, hi)) => json!({"min": rounded(lo).to_vec(), "max": rounded(hi).to_vec()}),
        None => Value::Null,
    }
}
fn r6(v: f64) -> f64 {
    let r = (v * 1e6).round() / 1e6;
    if r == 0.0 { 0.0 } else { r }
}
fn rounded(p: [f64; 3]) -> [f64; 3] {
    [r6(p[0]), r6(p[1]), r6(p[2])]
}

/// Counts of things skipped, reported once per kind so output stays short.
#[derive(Default)]
struct Warnings {
    counts: BTreeMap<&'static str, usize>,
}
impl Warnings {
    fn note(&mut self, message: &'static str) {
        let count = self.counts.entry(message).or_insert(0);
        *count = count.saturating_add(1);
    }
    fn finish(self) -> Vec<String> {
        self.counts
            .into_iter()
            .map(|(m, n)| format!("{n} {m}"))
            .collect()
    }
}

struct Budget {
    triangles: usize,
}
impl Budget {
    fn spend(&mut self, n: usize) -> Result<(), String> {
        self.triangles = self.triangles.saturating_add(n);
        if self.triangles > MAX_TRIANGLES {
            return Err(format!(
                "3D mesh exceeds the limit of {MAX_TRIANGLES} triangles"
            ));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------- polygons

fn cross(a: Point, b: Point, c: Point) -> f64 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}
fn signed_area(ring: &[Point]) -> f64 {
    let mut sum = 0.0;
    for (i, a) in ring.iter().enumerate() {
        let b = ring[(i + 1) % ring.len()];
        sum += a.x * b.y - b.x * a.y;
    }
    sum / 2.0
}
fn same(a: Point, b: Point) -> bool {
    (a.x - b.x).abs() < EPS && (a.y - b.y).abs() < EPS
}
/// Drop a repeated closing vertex and repeated neighbours, thin very long
/// rings, and wind the result counter-clockwise. None when under 3 vertices
/// or without area.
fn clean_ring(points: &[Point]) -> Option<Vec<Point>> {
    let mut ring: Vec<Point> = Vec::with_capacity(points.len());
    for p in points {
        if !p.x.is_finite() || !p.y.is_finite() {
            return None;
        }
        if ring.last().is_none_or(|last| !same(*last, *p)) {
            ring.push(*p);
        }
    }
    while ring.len() > 1
        && ring
            .first()
            .zip(ring.last())
            .is_some_and(|(a, b)| same(*a, *b))
    {
        ring.pop();
    }
    while ring.len() > MAX_RING {
        ring = ring.into_iter().step_by(2).collect();
    }
    if ring.len() < 3 {
        return None;
    }
    let area = signed_area(&ring);
    if area.abs() < 1e-12 {
        return None;
    }
    if area < 0.0 {
        ring.reverse();
    }
    Some(ring)
}
fn point_in_polygon(ring: &[Point], p: Point) -> bool {
    let mut inside = false;
    let mut j = ring.len().saturating_sub(1);
    for i in 0..ring.len() {
        let (a, b) = (ring[i], ring[j]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
        j = i;
    }
    inside
}
fn on_segment(a: Point, b: Point, p: Point) -> bool {
    cross(a, b, p).abs() <= 1e-9
        && p.x >= a.x.min(b.x) - 1e-9
        && p.x <= a.x.max(b.x) + 1e-9
        && p.y >= a.y.min(b.y) - 1e-9
        && p.y <= a.y.max(b.y) + 1e-9
}
/// True when the closed segments share any point.
fn segments_touch(a: Point, b: Point, c: Point, d: Point) -> bool {
    let d1 = cross(a, b, c);
    let d2 = cross(a, b, d);
    let d3 = cross(c, d, a);
    let d4 = cross(c, d, b);
    if ((d1 > 1e-9 && d2 < -1e-9) || (d1 < -1e-9 && d2 > 1e-9))
        && ((d3 > 1e-9 && d4 < -1e-9) || (d3 < -1e-9 && d4 > 1e-9))
    {
        return true;
    }
    on_segment(a, b, c) || on_segment(a, b, d) || on_segment(c, d, a) || on_segment(c, d, b)
}
fn ring_bounds(ring: &[Point]) -> (Point, Point) {
    let mut lo = Point::new(f64::MAX, f64::MAX);
    let mut hi = Point::new(f64::MIN, f64::MIN);
    for p in ring {
        lo.x = lo.x.min(p.x);
        lo.y = lo.y.min(p.y);
        hi.x = hi.x.max(p.x);
        hi.y = hi.y.max(p.y);
    }
    (lo, hi)
}
fn bounds_overlap(a: &(Point, Point), b: &(Point, Point)) -> bool {
    a.0.x <= b.1.x && b.0.x <= a.1.x && a.0.y <= b.1.y && b.0.y <= a.1.y
}
/// Whether any edge of `a` touches any edge of `b`.
fn edges_conflict(a: &[Point], b: &[Point]) -> bool {
    for i in 0..a.len() {
        let (p, q) = (a[i], a[(i + 1) % a.len()]);
        for j in 0..b.len() {
            if segments_touch(p, q, b[j], b[(j + 1) % b.len()]) {
                return true;
            }
        }
    }
    false
}
fn rings_conflict(a: &[Point], b: &[Point]) -> bool {
    edges_conflict(a, b)
        || a.first().is_some_and(|p| point_in_polygon(b, *p))
        || b.first().is_some_and(|p| point_in_polygon(a, *p))
}
/// Keep the holes that lie wholly inside the outline and touch no other
/// kept hole; the rest cannot be cut without breaking the polygon.
fn usable_holes(outer: &[Point], holes: Vec<Vec<Point>>, skipped: &mut usize) -> Vec<Vec<Point>> {
    let outer_bounds = ring_bounds(outer);
    let mut kept: Vec<(Vec<Point>, (Point, Point))> = vec![];
    for hole in holes {
        let b = ring_bounds(&hole);
        let inside = b.0.x > outer_bounds.0.x
            && b.1.x < outer_bounds.1.x
            && b.0.y > outer_bounds.0.y
            && b.1.y < outer_bounds.1.y
            && hole.iter().all(|p| point_in_polygon(outer, *p))
            && !edges_conflict(outer, &hole);
        let clear = kept
            .iter()
            .all(|(other, ob)| !bounds_overlap(&b, ob) || !rings_conflict(&hole, other));
        if inside && clear && kept.len() < MAX_HOLES_PER_POLYGON {
            kept.push((hole, b));
        } else {
            *skipped += 1;
        }
    }
    kept.into_iter().map(|(h, _)| h).collect()
}
fn convex(ring: &[Point]) -> bool {
    (0..ring.len()).all(|i| {
        cross(
            ring[i],
            ring[(i + 1) % ring.len()],
            ring[(i + 2) % ring.len()],
        ) > 1e-12
    })
}
fn in_triangle(a: Point, b: Point, c: Point, p: Point) -> bool {
    cross(a, b, p) >= -1e-12 && cross(b, c, p) >= -1e-12 && cross(c, a, p) >= -1e-12
}
/// Ear clipping over one counter-clockwise ring that may carry bridge
/// duplicates. Triangles index into `pts`.
fn ear_clip(pts: &[Point]) -> Option<Vec<[usize; 3]>> {
    let n = pts.len();
    if n < 3 {
        return None;
    }
    let mut prev: Vec<usize> = (0..n).map(|i| (i + n - 1) % n).collect();
    let mut next: Vec<usize> = (0..n).map(|i| (i + 1) % n).collect();
    let mut remaining = n;
    let mut tris: Vec<[usize; 3]> = Vec::with_capacity(n.saturating_sub(2));
    let mut cur = 0usize;
    let mut idle = 0usize;
    while remaining > 3 {
        let (p, nx) = (prev[cur], next[cur]);
        let c = cross(pts[p], pts[cur], pts[nx]);
        let mut done = false;
        if c.abs() <= 1e-12 {
            done = true;
        } else if c > 0.0 {
            let mut ear = true;
            let mut v = next[nx];
            while v != p {
                let q = pts[v];
                if !same(q, pts[p])
                    && !same(q, pts[cur])
                    && !same(q, pts[nx])
                    && in_triangle(pts[p], pts[cur], pts[nx], q)
                {
                    ear = false;
                    break;
                }
                v = next[v];
            }
            if ear {
                tris.push([p, cur, nx]);
                done = true;
            }
        }
        cur = nx;
        if done {
            next[p] = nx;
            prev[nx] = p;
            remaining -= 1;
            idle = 0;
        } else {
            idle += 1;
            if idle > remaining {
                return None;
            }
        }
    }
    let (a, b) = (next[cur], prev[cur]);
    if cross(pts[b], pts[cur], pts[a]) > 1e-12 {
        tris.push([b, cur, a]);
    }
    Some(tris)
}
fn edges_clear(h: Point, v: Point, rings: &[&[Point]]) -> bool {
    for ring in rings {
        for i in 0..ring.len() {
            let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
            if same(a, h) || same(b, h) || same(a, v) || same(b, v) {
                continue;
            }
            if segments_touch(h, v, a, b) {
                return false;
            }
        }
    }
    true
}
type Triangulation = (Vec<Point>, Vec<[usize; 3]>);
/// Triangulate a counter-clockwise outline with clockwise holes. Returns the
/// vertex list the triangles index into, or None when the shape defeats the
/// clipper.
fn triangulate(outer: &[Point], holes: &[Vec<Point>]) -> Option<Triangulation> {
    if holes.is_empty() {
        if outer.len() == 3 || convex(outer) {
            let tris = (1..outer.len().saturating_sub(1))
                .map(|i| [0, i, i + 1])
                .collect();
            return Some((outer.to_vec(), tris));
        }
        return ear_clip(outer).map(|t| (outer.to_vec(), t));
    }
    let mut order: Vec<usize> = (0..holes.len()).collect();
    let max_x = |h: &[Point]| h.iter().map(|p| p.x).fold(f64::MIN, f64::max);
    order.sort_by(|a, b| {
        max_x(&holes[*b])
            .total_cmp(&max_x(&holes[*a]))
            .then(a.cmp(b))
    });
    let mut ring: Vec<Point> = outer.to_vec();
    for (done, &hi) in order.iter().enumerate() {
        let hole = &holes[hi];
        let start = hole.iter().enumerate().fold(
            0usize,
            |best, (i, p)| if p.x > hole[best].x { i } else { best },
        );
        let h = hole[start];
        let mut candidates: Vec<usize> = (0..ring.len()).collect();
        candidates.sort_by(|a, b| {
            ring[*a]
                .distance(h)
                .total_cmp(&ring[*b].distance(h))
                .then(a.cmp(b))
        });
        let mut blockers: Vec<&[Point]> = vec![ring.as_slice(), hole.as_slice()];
        for &later in order.iter().skip(done + 1) {
            blockers.push(holes[later].as_slice());
        }
        let mut bridge = None;
        for &c in candidates.iter().take(256) {
            let v = ring[c];
            let mid = Point::new((h.x + v.x) / 2.0, (h.y + v.y) / 2.0);
            if edges_clear(h, v, &blockers)
                && point_in_polygon(outer, mid)
                && !holes.iter().any(|other| point_in_polygon(other, mid))
            {
                bridge = Some(c);
                break;
            }
        }
        let c = bridge?;
        let mut merged: Vec<Point> = Vec::with_capacity(ring.len() + hole.len() + 2);
        merged.extend_from_slice(&ring[..=c]);
        for k in 0..hole.len() {
            merged.push(hole[(start + k) % hole.len()]);
        }
        merged.push(h);
        merged.extend_from_slice(&ring[c..]);
        ring = merged;
        if ring.len() > MAX_MERGED_VERTICES {
            return None;
        }
    }
    ear_clip(&ring).map(|t| (ring, t))
}

// ----------------------------------------------------------------- builder

struct Builder<'a> {
    part: MeshPart,
    budget: &'a mut Budget,
}
impl Builder<'_> {
    fn vertex(&mut self, x: f64, y: f64, z: f64) -> u32 {
        self.part.positions.push([x, y, z]);
        u32::try_from(self.part.positions.len().saturating_sub(1)).unwrap_or(u32::MAX)
    }
    fn tri(&mut self, a: u32, b: u32, c: u32) {
        self.part.indices.extend([a, b, c]);
    }
    /// Extrude a counter-clockwise ring with optional holes from z0 to z1.
    /// Returns false when the shape could not be triangulated.
    fn prism(
        &mut self,
        outer: &[Point],
        holes: Vec<Vec<Point>>,
        z0: f64,
        z1: f64,
        warnings: &mut Warnings,
    ) -> Result<bool, String> {
        let mut skipped = 0usize;
        let holes: Vec<Vec<Point>> = usable_holes(outer, holes, &mut skipped)
            .into_iter()
            .map(|mut h| {
                h.reverse();
                h
            })
            .collect();
        for _ in 0..skipped {
            warnings.note("holes overlapped or left their polygon and were not cut");
        }
        let (cut, verts, tris) = match if holes.is_empty() {
            None
        } else {
            triangulate(outer, &holes)
        } {
            Some((v, t)) => (true, v, t),
            None => match triangulate(outer, &[]) {
                Some((v, t)) => {
                    if !holes.is_empty() {
                        warnings
                            .note("polygons with holes could not be triangulated; holes ignored");
                    }
                    (false, v, t)
                }
                None => {
                    warnings.note("polygons could not be triangulated and were skipped");
                    return Ok(false);
                }
            },
        };
        let wall_edges = if cut {
            outer.len() + holes.iter().map(Vec::len).sum::<usize>()
        } else {
            outer.len()
        };
        self.budget
            .spend(tris.len().saturating_mul(2).saturating_add(wall_edges * 2))?;
        let top: Vec<u32> = verts.iter().map(|p| self.vertex(p.x, p.y, z1)).collect();
        let bottom: Vec<u32> = verts.iter().map(|p| self.vertex(p.x, p.y, z0)).collect();
        for t in &tris {
            self.tri(top[t[0]], top[t[1]], top[t[2]]);
            self.tri(bottom[t[0]], bottom[t[2]], bottom[t[1]]);
        }
        self.walls(outer, z0, z1);
        if cut {
            for hole in &holes {
                self.walls(hole, z0, z1);
            }
        }
        Ok(true)
    }
    /// Side faces; the face normal is on the right of the walking direction,
    /// so counter-clockwise rings face out and clockwise rings face inward.
    fn walls(&mut self, ring: &[Point], z0: f64, z1: f64) {
        for (i, a) in ring.iter().enumerate() {
            let b = ring[(i + 1) % ring.len()];
            let a0 = self.vertex(a.x, a.y, z0);
            let b0 = self.vertex(b.x, b.y, z0);
            let b1 = self.vertex(b.x, b.y, z1);
            let a1 = self.vertex(a.x, a.y, z1);
            self.tri(a0, b0, b1);
            self.tri(a0, b1, a1);
        }
    }
}

// --------------------------------------------------------------- transforms

pub(crate) fn to_board(fp: &Footprint, local: Point) -> Point {
    fp.position.offset(local.rotate(-fp.rotation))
}
pub(crate) fn map_ring(fp: &Footprint, local: &[Point]) -> Vec<Point> {
    local.iter().map(|p| to_board(fp, *p)).collect()
}
fn circle_segments(radius: f64) -> usize {
    if radius < 0.5 {
        12
    } else if radius < 2.0 {
        24
    } else {
        36
    }
}
pub(crate) fn circle_ring(center: Point, radius: f64) -> Vec<Point> {
    let n = circle_segments(radius);
    (0..n)
        .map(|i| {
            let a = std::f64::consts::TAU * (i as f64) / (n as f64);
            Point::new(center.x + radius * a.cos(), center.y + radius * a.sin())
        })
        .collect()
}
/// A stadium outline around the segment `a` to `b`: the shape a routed slot
/// leaves, with half-circle caps at each centre.
fn capsule_ring(a: Point, b: Point, radius: f64) -> Vec<Point> {
    let mut points = Vec::new();
    let length = a.distance(b);
    if length < 1e-6 {
        return circle_ring(a, radius);
    }
    let n = circle_segments(radius).max(8);
    let axis = (b.y - a.y).atan2(b.x - a.x);
    // Half circle around the far centre, then half circle around the near
    // one, so the ring runs along one side and back along the other.
    for i in 0..=n {
        let angle = axis + std::f64::consts::PI * (i as f64) / (n as f64);
        points.push(Point::new(
            b.x + radius * angle.cos(),
            b.y + radius * angle.sin(),
        ));
    }
    for i in 0..=n {
        let angle = axis - std::f64::consts::PI * (i as f64) / (n as f64);
        points.push(Point::new(
            a.x + radius * angle.cos(),
            a.y + radius * angle.sin(),
        ));
    }
    clean_ring(&points).unwrap_or_default()
}
pub(crate) fn grow(ring: &[Point], margin: f64) -> Vec<Point> {
    let (lo, hi) = ring_bounds(ring);
    let (hx, hy) = ((hi.x - lo.x) / 2.0, (hi.y - lo.y) / 2.0);
    let (cx, cy) = ((hi.x + lo.x) / 2.0, (hi.y + lo.y) / 2.0);
    let sx = if hx > EPS { (hx + margin) / hx } else { 1.0 };
    let sy = if hy > EPS { (hy + margin) / hy } else { 1.0 };
    ring.iter()
        .map(|p| Point::new(cx + (p.x - cx) * sx, cy + (p.y - cy) * sy))
        .collect()
}
fn stroke_ring(a: Point, b: Point, width: f64) -> Option<Vec<Point>> {
    let len = a.distance(b);
    if len < 1e-6 {
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

/// Row-major 4x4 matrix that places a footprint's 3D model on the board.
///
/// The model's own transform is: scale first, then turn about X by minus rx,
/// then about Y by minus ry, then about Z by minus rz, then offset in
/// millimetres. The footprint's position and rotation (about Z, sign
/// matching the 2D canvas) come after that. A bottom-side part is flipped
/// about the board plane: the model matrix applies inside the footprint
/// frame before the flip, so a model offset of +1 mm in z ends up 1 mm below
/// the bottom face. Front parts stand on the top face at `thickness`.
pub fn model_matrix(
    model: &Model3D,
    position: Point,
    rotation: f64,
    bottom: bool,
    thickness: f64,
) -> [f64; 16] {
    let scale = diag(model.scale[0], model.scale[1], model.scale[2]);
    let turn = mul(
        &rot_z(-model.rotate[2]),
        &mul(&rot_y(-model.rotate[1]), &rot_x(-model.rotate[0])),
    );
    let local = mul(
        &translate(model.offset[0], model.offset[1], model.offset[2]),
        &mul(&turn, &scale),
    );
    let flip = if bottom {
        rot_x(180.0)
    } else {
        diag(1.0, 1.0, 1.0)
    };
    let footprint = mul(
        &translate(position.x, position.y, if bottom { 0.0 } else { thickness }),
        &mul(&rot_z(-rotation), &flip),
    );
    mul(&footprint, &local)
}
/// What a model lookup answered for one part.
pub enum ModelLookup {
    /// The file is still being read; keep the box for now.
    Loading,
    /// The file loaded.
    Ready(std::sync::Arc<LoadedModel>),
    /// The file cannot be used; the sentence says why.
    Failed(String),
}
/// The load state of one part's model, for the parts list.
#[derive(Clone, Debug, PartialEq)]
pub enum ModelStatus {
    /// The file is still being read.
    Loading,
    /// The model replaced the box.
    Loaded,
    /// The box stayed; the sentence says why.
    Failed(String),
}

/// Replace default boxes by model files. For each body that names a model,
/// `lookup` is asked once for the path; a ready model's triangles go through
/// the part's placement matrix and take the box's place. Returns the state
/// per component so the UI can say why a box stayed.
///
/// # Rules
/// The part keeps its component id and `PartKind::Body`, so picking works.
/// A failed or pending model leaves the box untouched.
/// # Dependencies
/// `./model_load.rs` for [`LoadedModel`]; `model_matrix` above for placement.
/// # Do not
/// Do not read files here; the caller owns loading and caching.
pub fn with_models(
    mesh: &mut BoardMesh,
    mut lookup: impl FnMut(&str) -> ModelLookup,
) -> Vec<(Id, ModelStatus)> {
    let mut notes = vec![];
    for part in &mut mesh.parts {
        let (Some(id), Some(model)) = (part.component, part.model.as_mut()) else {
            continue;
        };
        if part.kind != PartKind::Body {
            continue;
        }
        match lookup(&model.path) {
            ModelLookup::Loading => notes.push((id, ModelStatus::Loading)),
            ModelLookup::Failed(reason) => notes.push((id, ModelStatus::Failed(reason))),
            ModelLookup::Ready(m) => {
                part.positions = m
                    .positions
                    .iter()
                    .map(|p| apply_matrix(&model.matrix, *p))
                    .collect();
                part.indices = m.indices.clone();
                part.family = Some("model".into());
                part.color = m.color;
                model.loaded = true;
                notes.push((id, ModelStatus::Loaded));
            }
        }
    }
    notes
}

/// Apply a [`model_matrix`] to one model vertex.
pub fn apply_matrix(m: &[f64; 16], p: [f64; 3]) -> [f64; 3] {
    [
        m[0] * p[0] + m[1] * p[1] + m[2] * p[2] + m[3],
        m[4] * p[0] + m[5] * p[1] + m[6] * p[2] + m[7],
        m[8] * p[0] + m[9] * p[1] + m[10] * p[2] + m[11],
    ]
}
fn diag(x: f64, y: f64, z: f64) -> [f64; 16] {
    [
        x, 0.0, 0.0, 0.0, 0.0, y, 0.0, 0.0, 0.0, 0.0, z, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}
fn translate(x: f64, y: f64, z: f64) -> [f64; 16] {
    [
        1.0, 0.0, 0.0, x, 0.0, 1.0, 0.0, y, 0.0, 0.0, 1.0, z, 0.0, 0.0, 0.0, 1.0,
    ]
}
fn rot_x(degrees: f64) -> [f64; 16] {
    let (s, c) = degrees.to_radians().sin_cos();
    [
        1.0, 0.0, 0.0, 0.0, 0.0, c, -s, 0.0, 0.0, s, c, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}
fn rot_y(degrees: f64) -> [f64; 16] {
    let (s, c) = degrees.to_radians().sin_cos();
    [
        c, 0.0, s, 0.0, 0.0, 1.0, 0.0, 0.0, -s, 0.0, c, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}
fn rot_z(degrees: f64) -> [f64; 16] {
    let (s, c) = degrees.to_radians().sin_cos();
    [
        c, -s, 0.0, 0.0, s, c, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}
fn mul(a: &[f64; 16], b: &[f64; 16]) -> [f64; 16] {
    let mut out = [0.0; 16];
    for row in 0..4 {
        for col in 0..4 {
            let mut sum = 0.0;
            for k in 0..4 {
                sum += a[row * 4 + k] * b[k * 4 + col];
            }
            out[row * 4 + col] = sum;
        }
    }
    out
}

// ------------------------------------------------------------------- bodies

#[derive(Clone, Debug)]
struct Body {
    family: &'static str,
    /// Counter-clockwise footprint outline in footprint-local millimetres.
    ring: Vec<Point>,
    height: f64,
    /// Square pin posts rising above the body, as local rings.
    pins: Vec<Vec<Point>>,
}
impl Body {
    fn plain(family: &'static str, ring: Vec<Point>, height: f64) -> Self {
        Self {
            family,
            ring,
            height,
            pins: vec![],
        }
    }
}
struct PadBox {
    centre: Point,
    lo: Point,
    hi: Point,
    through: bool,
    round: bool,
}
impl PadBox {
    fn width(&self) -> f64 {
        self.hi.x - self.lo.x
    }
    fn height(&self) -> f64 {
        self.hi.y - self.lo.y
    }
}
fn pad_box(pad: &Pad) -> Option<PadBox> {
    let points = pad.outline_points();
    if points.is_empty() {
        return None;
    }
    let (lo, hi) = ring_bounds(&points);
    let geometry = pad.geometry.as_ref();
    Some(PadBox {
        centre: pad.offset,
        lo,
        hi,
        through: geometry.is_some_and(|g| g.drill.is_some()),
        round: geometry.is_none_or(|g| g.shape == "circle"),
    })
}
pub(crate) fn rect_ring(cx: f64, cy: f64, w: f64, h: f64) -> Vec<Point> {
    vec![
        Point::new(cx - w / 2.0, cy - h / 2.0),
        Point::new(cx + w / 2.0, cy - h / 2.0),
        Point::new(cx + w / 2.0, cy + h / 2.0),
        Point::new(cx - w / 2.0, cy + h / 2.0),
    ]
}
fn clusters(values: &mut [f64], tolerance: f64) -> Vec<f64> {
    values.sort_by(f64::total_cmp);
    let mut out: Vec<f64> = vec![];
    for v in values.iter() {
        if out.last().is_none_or(|last| (v - last).abs() > tolerance) {
            out.push(*v);
        }
    }
    out
}
/// Smallest positive gap between neighbouring distinct values.
fn smallest_gap(values: &[f64]) -> Option<f64> {
    values
        .windows(2)
        .map(|w| w[1] - w[0])
        .filter(|g| *g > 0.05)
        .fold(None, |best: Option<f64>, g| {
            Some(best.map_or(g, |b| b.min(g)))
        })
}
fn chip_height(length: f64) -> f64 {
    if length <= 1.2 {
        0.35
    } else if length <= 1.8 {
        0.45
    } else if length <= 2.4 {
        0.55
    } else if length <= 3.5 {
        0.6
    } else {
        1.0
    }
}
fn header_hint(component: Option<&Component>) -> bool {
    let Some(c) = component else { return false };
    let name = c
        .library_footprint
        .as_ref()
        .map(|l| l.definition.name.to_ascii_lowercase())
        .unwrap_or_default();
    c.kind.eq_ignore_ascii_case("connector")
        || name.contains("header")
        || name.contains("pinsocket")
        || name.contains("connector")
}

/// Choose a parametric body from the pad pattern.
fn default_body(fp: &Footprint, component: Option<&Component>) -> Option<Body> {
    let pads: Vec<PadBox> = fp.pads.iter().filter_map(pad_box).collect();
    if pads.is_empty() {
        return None;
    }
    let n = pads.len();
    let all_points: Vec<Point> = pads.iter().flat_map(|p| [p.lo, p.hi]).collect();
    let (lo, hi) = ring_bounds(&all_points);
    let through = pads.iter().filter(|p| p.through).count() * 2 >= n;
    let generic = || {
        Body::plain(
            "generic",
            rect_ring(
                (lo.x + hi.x) / 2.0,
                (lo.y + hi.y) / 2.0,
                (hi.x - lo.x).max(0.5),
                (hi.y - lo.y).max(0.5),
            ),
            1.0,
        )
    };
    let centroid = Point::new(
        pads.iter().map(|p| p.centre.x).sum::<f64>() / n as f64,
        pads.iter().map(|p| p.centre.y).sum::<f64>() / n as f64,
    );
    let span = (hi.x - lo.x).max(hi.y - lo.y);
    let mut areas: Vec<f64> = pads.iter().map(|p| p.width() * p.height()).collect();
    areas.sort_by(f64::total_cmp);
    let median = areas.get(n / 2).copied().unwrap_or(0.0);
    let signal: Vec<&PadBox> = pads
        .iter()
        .filter(|p| {
            !(n >= 5
                && p.centre.distance(centroid) < 0.15 * span
                && p.width() * p.height() >= 2.0 * median)
        })
        .collect();
    let mut xs: Vec<f64> = signal.iter().map(|p| p.centre.x).collect();
    let mut ys: Vec<f64> = signal.iter().map(|p| p.centre.y).collect();
    let xc = clusters(&mut xs, 0.1);
    let yc = clusters(&mut ys, 0.1);

    if through {
        return through_body(component, &pads, &xc, &yc).or_else(|| Some(generic()));
    }
    if n == 2 {
        let (a, b) = (&pads[0], &pads[1]);
        let c = a.centre.distance(b.centre);
        if c < 0.2 {
            return Some(generic());
        }
        let along_x = (b.centre.x - a.centre.x).abs() >= (b.centre.y - a.centre.y).abs();
        let across = if along_x {
            (a.height() + b.height()) / 2.0
        } else {
            (a.width() + b.width()) / 2.0
        };
        let length = c + 0.15;
        let width = (across * 0.9).max(0.2);
        let (w, h) = if along_x {
            (length, width)
        } else {
            (width, length)
        };
        return Some(Body::plain(
            "chip",
            rect_ring(
                (a.centre.x + b.centre.x) / 2.0,
                (a.centre.y + b.centre.y) / 2.0,
                w,
                h,
            ),
            chip_height(length),
        ));
    }
    // Area array.
    if n >= 12 {
        let (lo_x, hi_x) = (
            xc.first().copied().unwrap_or(0.0),
            xc.last().copied().unwrap_or(0.0),
        );
        let (lo_y, hi_y) = (
            yc.first().copied().unwrap_or(0.0),
            yc.last().copied().unwrap_or(0.0),
        );
        let interior = pads
            .iter()
            .filter(|p| {
                p.centre.x > lo_x + 0.1
                    && p.centre.x < hi_x - 0.1
                    && p.centre.y > lo_y + 0.1
                    && p.centre.y < hi_y - 0.1
            })
            .count();
        let round = pads.iter().filter(|p| p.round).count() * 2 >= n;
        if round && interior * 4 >= n {
            let pitch = smallest_gap(&xc)
                .into_iter()
                .chain(smallest_gap(&yc))
                .fold(f64::MAX, f64::min);
            let pitch = if pitch.is_finite() && pitch < 10.0 {
                pitch
            } else {
                1.0
            };
            return Some(Body::plain(
                "bga",
                rect_ring(
                    (lo_x + hi_x) / 2.0,
                    (lo_y + hi_y) / 2.0,
                    hi_x - lo_x + pitch,
                    hi_y - lo_y + pitch,
                ),
                1.0,
            ));
        }
    }
    // Pads on all four sides: quad flat packages.
    if n >= 8 && xc.len() >= 2 && yc.len() >= 2 {
        let (x_lo, x_hi) = (xc[0], xc[xc.len() - 1]);
        let (y_lo, y_hi) = (yc[0], yc[yc.len() - 1]);
        let count = |f: &dyn Fn(&PadBox) -> bool| signal.iter().filter(|p| f(p)).count();
        // A side counts only with pads between the corners; two rows of
        // pads would otherwise look like four short sides.
        let between_y = |p: &PadBox| p.centre.y > y_lo + 0.1 && p.centre.y < y_hi - 0.1;
        let between_x = |p: &PadBox| p.centre.x > x_lo + 0.1 && p.centre.x < x_hi - 0.1;
        let left = count(&|p| (p.centre.x - x_lo).abs() < 0.1 && between_y(p));
        let right = count(&|p| (p.centre.x - x_hi).abs() < 0.1 && between_y(p));
        let bottom = count(&|p| (p.centre.y - y_lo).abs() < 0.1 && between_x(p));
        let top = count(&|p| (p.centre.y - y_hi).abs() < 0.1 && between_x(p));
        if left >= 1 && right >= 1 && top >= 1 && bottom >= 1 {
            let lead_x = signal
                .iter()
                .filter(|p| (p.centre.x - x_lo).abs() < 0.1)
                .map(|p| p.width())
                .fold(0.0, f64::max);
            let lead_y = signal
                .iter()
                .filter(|p| (p.centre.y - y_lo).abs() < 0.1)
                .map(|p| p.height())
                .fold(0.0, f64::max);
            let (cx, cy) = ((x_lo + x_hi) / 2.0, (y_lo + y_hi) / 2.0);
            if lead_x >= 1.0 || lead_y >= 1.0 {
                let w = (x_hi - x_lo - lead_x).max(0.5);
                let h = (y_hi - y_lo - lead_y).max(0.5);
                return Some(Body::plain("qfp", rect_ring(cx, cy, w, h), 1.4));
            }
            return Some(Body::plain(
                "qfn",
                rect_ring(
                    cx,
                    cy,
                    (hi.x - lo.x - 0.1).max(0.5),
                    (hi.y - lo.y - 0.1).max(0.5),
                ),
                0.8,
            ));
        }
    }
    // Two rows: SOT, SOIC, TSSOP and relatives.
    if (yc.len() == 2 && xc.len() >= 2) || (xc.len() == 2 && yc.len() >= 2) {
        let rows_along_x = yc.len() == 2;
        let (gap_centres, lead, ext_lo, ext_hi, pitch) = if rows_along_x {
            let lead = signal.iter().map(|p| p.height()).fold(0.0, f64::max);
            let ext = signal.iter().fold((f64::MAX, f64::MIN), |(a, b), p| {
                (a.min(p.lo.x), b.max(p.hi.x))
            });
            (yc[1] - yc[0], lead, ext.0, ext.1, smallest_gap(&xc))
        } else {
            let lead = signal.iter().map(|p| p.width()).fold(0.0, f64::max);
            let ext = signal.iter().fold((f64::MAX, f64::MIN), |(a, b), p| {
                (a.min(p.lo.y), b.max(p.hi.y))
            });
            (xc[1] - xc[0], lead, ext.0, ext.1, smallest_gap(&yc))
        };
        let gap = gap_centres - lead;
        if gap >= 0.5 {
            let length = ext_hi - ext_lo + 0.3;
            let along_mid = (ext_lo + ext_hi) / 2.0;
            let ring = if rows_along_x {
                rect_ring(along_mid, (yc[0] + yc[1]) / 2.0, length, gap)
            } else {
                rect_ring((xc[0] + xc[1]) / 2.0, along_mid, gap, length)
            };
            let soic = pitch.is_some_and(|p| p >= 1.0) && gap > 3.0;
            return Some(Body::plain(
                if soic { "soic" } else { "sot" },
                ring,
                if soic { 1.6 } else { 1.1 },
            ));
        }
    }
    Some(generic())
}
fn through_body(
    component: Option<&Component>,
    pads: &[PadBox],
    xc: &[f64],
    yc: &[f64],
) -> Option<Body> {
    let n = pads.len();
    let hinted = header_hint(component);
    if n == 2 && !hinted {
        let (a, b) = (&pads[0], &pads[1]);
        let c = a.centre.distance(b.centre);
        if c < 0.2 {
            return None;
        }
        let mid = Point::new(
            (a.centre.x + b.centre.x) / 2.0,
            (a.centre.y + b.centre.y) / 2.0,
        );
        if c >= 6.5 {
            let length = (c * 0.62).max(2.0);
            let diameter = (length * 0.4).max(1.5);
            let (ux, uy) = ((b.centre.x - a.centre.x) / c, (b.centre.y - a.centre.y) / c);
            let (nx, ny) = (-uy * diameter / 2.0, ux * diameter / 2.0);
            let (hx, hy) = (ux * length / 2.0, uy * length / 2.0);
            let ring = vec![
                Point::new(mid.x - hx - nx, mid.y - hy - ny),
                Point::new(mid.x + hx - nx, mid.y + hy - ny),
                Point::new(mid.x + hx + nx, mid.y + hy + ny),
                Point::new(mid.x - hx + nx, mid.y - hy + ny),
            ];
            return Some(Body::plain("axial", ring, diameter));
        }
        let diameter = (2.0 * c).max(4.0);
        return Some(Body::plain(
            "radial",
            circle_ring(mid, diameter / 2.0),
            6.0 + 0.7 * diameter,
        ));
    }
    let single_row = xc.len() == 1 || yc.len() == 1;
    let two_rows_x = yc.len() == 2 && xc.len() >= 2;
    let two_rows_y = xc.len() == 2 && yc.len() >= 2;
    let row_gap = if two_rows_x {
        yc[1] - yc[0]
    } else if two_rows_y {
        xc[1] - xc[0]
    } else {
        0.0
    };
    if (two_rows_x || two_rows_y) && !hinted && row_gap >= 5.0 {
        let corners: Vec<Point> = pads.iter().flat_map(|p| [p.lo, p.hi]).collect();
        let (lo, hi) = ring_bounds(&corners);
        let pitch = if two_rows_x {
            smallest_gap(xc)
        } else {
            smallest_gap(yc)
        }
        .unwrap_or(2.54);
        let per_row = (n as f64 / 2.0).ceil();
        let length = per_row * pitch - 0.3;
        let width = row_gap - 1.3;
        if width < 0.5 || length < 0.5 {
            return None;
        }
        let (cx, cy) = ((lo.x + hi.x) / 2.0, (lo.y + hi.y) / 2.0);
        let ring = if two_rows_x {
            rect_ring(cx, cy, length, width)
        } else {
            rect_ring(cx, cy, width, length)
        };
        return Some(Body::plain("dip", ring, 3.3));
    }
    if single_row || ((two_rows_x || two_rows_y) && row_gap <= 3.0) || hinted {
        let centres: Vec<Point> = pads.iter().map(|p| p.centre).collect();
        let (lo, hi) = ring_bounds(&centres);
        let mut pitch = f64::MAX;
        for (i, a) in centres.iter().enumerate() {
            for b in centres.iter().skip(i + 1) {
                let d = a.distance(*b);
                if d > 0.3 {
                    pitch = pitch.min(d);
                }
            }
        }
        if !pitch.is_finite() || pitch > 10.0 {
            return None;
        }
        let ring = rect_ring(
            (lo.x + hi.x) / 2.0,
            (lo.y + hi.y) / 2.0,
            hi.x - lo.x + pitch,
            hi.y - lo.y + pitch,
        );
        let pins = centres
            .iter()
            .map(|c| rect_ring(c.x, c.y, 0.64, 0.64))
            .collect();
        return Some(Body {
            family: "header",
            ring,
            height: 2.5,
            pins,
        });
    }
    None
}
fn parse_height(text: &str) -> Option<f64> {
    let trimmed = text.trim();
    let number = trimmed.strip_suffix("mm").unwrap_or(trimmed).trim();
    let v: f64 = number.parse().ok()?;
    (v.is_finite() && (0.05..=100.0).contains(&v)).then_some(v)
}

// -------------------------------------------------------------------- build

/// Build the whole mesh for a validated project.
pub fn board_mesh(p: &Project, layers: MeshLayers) -> Result<BoardMesh, String> {
    p.validate()?;
    let thickness = p.board.thickness;
    let mut budget = Budget { triangles: 0 };
    let mut warnings = Warnings::default();
    let mut parts: Vec<MeshPart> = vec![];

    let outline_points: Vec<Point> = match &p.board.outline {
        Some(o) if o.len() >= 3 => o.clone(),
        _ => vec![
            Point::new(0.0, 0.0),
            Point::new(p.board.width, 0.0),
            Point::new(p.board.width, p.board.height),
            Point::new(0.0, p.board.height),
        ],
    };
    let outline = clean_ring(&outline_points)
        .ok_or("board outline has no area, so there is nothing to build")?;

    // Rings that go clean through the board.
    let mut drills: Vec<Vec<Point>> = vec![];
    for fp in &p.board.footprints {
        for pad in &fp.pads {
            drills.extend(clean_ring(&map_ring(fp, &pad.drill_outline_points())));
        }
    }
    for via in &p.board.vias {
        if via.drill > 0.0 {
            drills.extend(clean_ring(&circle_ring(via.position, via.drill / 2.0)));
        }
    }
    for hole in &p.board.holes {
        if hole.is_slot() {
            drills.extend(clean_ring(&capsule_ring(
                hole.start(),
                hole.finish(),
                hole.diameter / 2.0,
            )));
        } else {
            drills.extend(clean_ring(&circle_ring(hole.position, hole.diameter / 2.0)));
        }
    }
    for cut in &p.board.cutouts {
        drills.extend(clean_ring(&circle_ring(cut.center, cut.radius)));
    }
    let slab_holes = drills;

    if layers.board {
        let mut b = Builder {
            part: MeshPart::new(PartKind::Board, "board".into()),
            budget: &mut budget,
        };
        if !b.prism(&outline, slab_holes.clone(), 0.0, thickness, &mut warnings)? {
            return Err("the board outline could not be triangulated".into());
        }
        parts.push(b.part);
    }

    if layers.copper {
        for (kind, top) in [(PartKind::CopperTop, true), (PartKind::CopperBottom, false)] {
            let (z0, z1) = if top {
                (thickness, thickness + COPPER_THICKNESS)
            } else {
                (-COPPER_THICKNESS, 0.0)
            };
            let side = if top { Layer::Front } else { Layer::Back };
            let mut b = Builder {
                part: MeshPart::new(kind, kind.name().into()),
                budget: &mut budget,
            };
            for fp in &p.board.footprints {
                for pad in &fp.pads {
                    let (front, back, copper) = pad_sides(fp, pad);
                    if !copper || (top && !front) || (!top && !back) {
                        continue;
                    }
                    let Some(ring) = clean_ring(&map_ring(fp, &pad.outline_points())) else {
                        continue;
                    };
                    let hole = clean_ring(&map_ring(fp, &pad.drill_outline_points()));
                    b.prism(&ring, hole.into_iter().collect(), z0, z1, &mut warnings)?;
                }
            }
            for via in &p.board.vias {
                let ring = circle_ring(via.position, via.diameter / 2.0);
                let hole = (via.drill > 0.0).then(|| circle_ring(via.position, via.drill / 2.0));
                b.prism(&ring, hole.into_iter().collect(), z0, z1, &mut warnings)?;
            }
            for f in p.board.fiducials.iter().filter(|f| f.layer == side) {
                let ring = circle_ring(f.position, f.diameter / 2.0);
                b.prism(&ring, vec![], z0, z1, &mut warnings)?;
            }
            if top {
                for tie in &p.board.netties {
                    let (s, c) = tie.rotation.to_radians().sin_cos();
                    let r = tie.diameter / 2.0;
                    for sign in [-1.0, 1.0] {
                        let centre = Point::new(
                            tie.position.x + sign * r * c,
                            tie.position.y + sign * r * s,
                        );
                        b.prism(&circle_ring(centre, r), vec![], z0, z1, &mut warnings)?;
                    }
                }
            }
            for t in p.board.tracks.iter().filter(|t| t.layer == side) {
                if let Some(ring) = track_ring(t) {
                    b.prism(&ring, vec![], z0, z1, &mut warnings)?;
                }
            }
            for z in p.board.zones.iter().filter(|z| z.layer == side && z.filled) {
                let Some(ring) = clean_ring(&z.outline) else {
                    warnings.note("zones had no area and were skipped");
                    continue;
                };
                let holes = z.cutouts.iter().filter_map(|c| clean_ring(c)).collect();
                b.prism(&ring, holes, z0, z1, &mut warnings)?;
            }
            if !b.part.indices.is_empty() {
                parts.push(b.part);
            }
        }
    }

    if layers.mask {
        for (kind, top) in [(PartKind::MaskTop, true), (PartKind::MaskBottom, false)] {
            let (z0, z1) = if top {
                (
                    thickness + COPPER_THICKNESS,
                    thickness + COPPER_THICKNESS + MASK_THICKNESS,
                )
            } else {
                (-COPPER_THICKNESS - MASK_THICKNESS, -COPPER_THICKNESS)
            };
            let mut holes = slab_holes.clone();
            let mut openings: Vec<Vec<Point>> = vec![];
            for fp in &p.board.footprints {
                let margin = p
                    .component(fp.component)
                    .and_then(|c| c.library_footprint.as_ref())
                    .and_then(|l| l.definition.rules.as_ref())
                    .and_then(|r| r.mask_margin)
                    .filter(|m| m.is_finite() && m.abs() <= 1.0)
                    .unwrap_or(MASK_MARGIN);
                for pad in &fp.pads {
                    let (front, back, _) = pad_sides(fp, pad);
                    if (top && !front) || (!top && !back) {
                        continue;
                    }
                    if let Some(ring) = clean_ring(&map_ring(fp, &pad.outline_points())) {
                        openings.extend(clean_ring(&grow(&ring, margin)));
                    }
                }
            }
            if holes.len() + openings.len() > MAX_HOLES_PER_POLYGON {
                warnings.note("mask sheets had too many openings; pad openings were left out");
            } else {
                holes.extend(openings);
            }
            let mut b = Builder {
                part: MeshPart::new(kind, kind.name().into()),
                budget: &mut budget,
            };
            b.prism(&outline, holes, z0, z1, &mut warnings)?;
            parts.push(b.part);
        }
    }

    if layers.silk {
        let top_z0 = thickness + COPPER_THICKNESS + MASK_THICKNESS;
        let top_z1 = top_z0 + SILK_THICKNESS;
        let bottom_z0 = -COPPER_THICKNESS - MASK_THICKNESS - SILK_THICKNESS;
        let bottom_z1 = -COPPER_THICKNESS - MASK_THICKNESS;
        // Collect strokes first: (start, end, width, on_top).
        let mut strokes: Vec<(Point, Point, f64, bool)> = vec![];
        for fp in &p.board.footprints {
            let Some(lib) = p
                .component(fp.component)
                .and_then(|c| c.library_footprint.as_ref())
            else {
                continue;
            };
            for g in &lib.definition.graphics {
                let Some((layer, width)) = graphic_layer(g) else {
                    continue;
                };
                if !layer.contains("Silk") {
                    continue;
                }
                let on_top = layer.starts_with('F');
                if !on_top && !layer.starts_with('B') {
                    continue;
                }
                let pts = thin_polyline(&map_ring(fp, &g.path_points()), 48);
                for pair in pts.windows(2) {
                    strokes.push((pair[0], pair[1], width.clamp(0.1, 1.0), on_top));
                }
            }
        }
        for layer in p
            .board
            .user_layers
            .iter()
            .filter(|l| l.visible && l.kind == ArtKind::Silk)
        {
            for shape in &layer.shapes {
                let polyline: Vec<Point> = match shape {
                    ArtShape::Line { start, end } => vec![*start, *end],
                    ArtShape::Rect {
                        x,
                        y,
                        width,
                        height,
                        rotate,
                    } => {
                        let c = Point::new(x + width / 2.0, y + height / 2.0);
                        let mut pts: Vec<Point> = rect_ring(0.0, 0.0, *width, *height)
                            .iter()
                            .map(|q| c.offset(q.rotate(*rotate)))
                            .collect();
                        pts.extend(pts.first().copied());
                        pts
                    }
                    ArtShape::Circle { center, radius } => {
                        let mut pts = circle_ring(*center, *radius);
                        pts.extend(pts.first().copied());
                        pts
                    }
                    ArtShape::Poly { points } => {
                        let mut pts = points.clone();
                        pts.extend(pts.first().copied());
                        pts
                    }
                    ArtShape::Text { .. } => {
                        warnings.note("drawn text labels are not shown in 3D");
                        vec![]
                    }
                };
                for pair in thin_polyline(&polyline, 48).windows(2) {
                    strokes.push((pair[0], pair[1], 0.15, true));
                }
            }
        }
        for _ in &p.board.texts {
            warnings.note("board text labels are not shown in 3D");
        }
        for (kind, top) in [(PartKind::SilkTop, true), (PartKind::SilkBottom, false)] {
            let (z0, z1) = if top {
                (top_z0, top_z1)
            } else {
                (bottom_z0, bottom_z1)
            };
            let mut b = Builder {
                part: MeshPart::new(kind, kind.name().into()),
                budget: &mut budget,
            };
            for (a, c, w, on_top) in &strokes {
                if *on_top == top
                    && let Some(ring) = stroke_ring(*a, *c, *w)
                {
                    b.prism(&ring, vec![], z0, z1, &mut warnings)?;
                }
            }
            parts.push(b.part);
        }
    }

    if layers.bodies {
        for fp in &p.board.footprints {
            let component = p.component(fp.component);
            let Some(mut body) = default_body(fp, component) else {
                continue;
            };
            let mut height_source = "default";
            if let Some(text) = component.and_then(|c| c.field("height")) {
                match parse_height(text) {
                    Some(h) => {
                        body.height = h;
                        height_source = "field";
                    }
                    None => warnings
                        .note("component height fields were not a size between 0.05 and 100 mm"),
                }
            }
            let name = component
                .map(|c| c.reference.clone())
                .unwrap_or_else(|| format!("footprint {}", fp.id));
            let mut b = Builder {
                part: MeshPart::new(PartKind::Body, name),
                budget: &mut budget,
            };
            b.part.component = Some(fp.component);
            b.part.family = Some(body.family.to_string());
            b.part.height_source = Some(height_source);
            if let Some(model) = component
                .and_then(|c| c.library_footprint.as_ref())
                .and_then(|l| l.definition.model.as_ref())
            {
                b.part.model = Some(ModelPlacement {
                    path: model.path.clone(),
                    matrix: model_matrix(
                        model,
                        fp.position,
                        fp.rotation,
                        fp.board_side() == Layer::Back,
                        thickness,
                    ),
                    loaded: false,
                });
            }
            if let Some(ring) = clean_ring(&map_ring(fp, &body.ring)) {
                b.prism(
                    &ring,
                    vec![],
                    thickness,
                    thickness + body.height,
                    &mut warnings,
                )?;
            }
            for pin in &body.pins {
                if let Some(ring) = clean_ring(&map_ring(fp, pin)) {
                    b.prism(
                        &ring,
                        vec![],
                        thickness + body.height,
                        thickness + body.height + POST_HEIGHT,
                        &mut warnings,
                    )?;
                }
            }
            if !b.part.indices.is_empty() {
                parts.push(b.part);
            }
        }
    }

    Ok(BoardMesh {
        parts,
        warnings: warnings.finish(),
    })
}

/// Which copper sides a pad touches, and whether it has copper at all.
pub(crate) fn pad_sides(fp: &Footprint, pad: &Pad) -> (bool, bool, bool) {
    match &pad.geometry {
        None => (true, false, true),
        Some(g) => {
            if g.pad_type == "np_thru_hole" || g.pad_type == "unplated" {
                return (false, false, false);
            }
            if g.pad_type == "thru_hole" {
                return (true, true, true);
            }
            let layers = fp.pad_geometry_layers(pad);
            let front = layers
                .iter()
                .any(|l| l == "F.Cu" || l == "*.Cu" || l == "F&B.Cu");
            let back = layers
                .iter()
                .any(|l| l == "B.Cu" || l == "*.Cu" || l == "F&B.Cu");
            if front || back {
                (front, back, true)
            } else if fp.board_side() == Layer::Back {
                (false, true, true)
            } else {
                (true, false, true)
            }
        }
    }
}
pub(crate) fn graphic_layer(g: &pcbcraft_library::Graphic) -> Option<(&str, f64)> {
    use pcbcraft_library::Graphic as G;
    match g {
        G::Line { width, layer, .. }
        | G::Rectangle { width, layer, .. }
        | G::Circle { width, layer, .. }
        | G::Arc { width, layer, .. }
        | G::Polyline { width, layer, .. } => Some((layer.as_str(), *width)),
        G::Text { .. } => None,
    }
}
pub(crate) fn thin_polyline(points: &[Point], max_segments: usize) -> Vec<Point> {
    if points.len() <= max_segments + 1 {
        return points.to_vec();
    }
    let step = points.len().div_ceil(max_segments);
    let mut out: Vec<Point> = points.iter().copied().step_by(step).collect();
    if let (Some(last), Some(kept)) = (points.last().copied(), out.last().copied())
        && !same(last, kept)
    {
        out.push(last);
    }
    out
}
/// Copper outline of one track: a rectangle, or a strip along its arc.
fn track_ring(t: &pcbcraft_doc::Track) -> Option<Vec<Point>> {
    let half = t.width / 2.0;
    if let Some(arc) = t.curve() {
        let steps =
            ((arc.sweep.abs() / (std::f64::consts::PI / 18.0)).ceil() as usize).clamp(1, 64);
        let inner = (arc.radius - half).max(0.001);
        let outer = arc.radius + half;
        let at = |i: usize, r: f64| {
            let a = arc.start_angle + arc.sweep * (i as f64) / (steps as f64);
            Point::new(arc.center.x + r * a.cos(), arc.center.y + r * a.sin())
        };
        let mut ring: Vec<Point> = (0..=steps).map(|i| at(i, outer)).collect();
        ring.extend((0..=steps).rev().map(|i| at(i, inner)));
        return clean_ring(&ring);
    }
    stroke_ring(t.start, t.end, t.width)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pcbcraft_doc::{CopperZone, Cutout, Hole, Net, Track, Via};

    type Check = Result<(), String>;

    fn base() -> Project {
        let mut p = Project::default();
        p.board.width = 40.0;
        p.board.height = 30.0;
        p.board.thickness = 1.6;
        p.nets.push(Net {
            id: 1,
            name: "N1".into(),
            ..Net::default()
        });
        p
    }
    fn slab_only() -> MeshLayers {
        MeshLayers {
            copper: false,
            mask: false,
            silk: false,
            bodies: false,
            ..MeshLayers::default()
        }
    }
    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<Point> {
        vec![
            Point::new(x0, y0),
            Point::new(x1, y0),
            Point::new(x1, y1),
            Point::new(x0, y1),
        ]
    }
    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }
    /// Area of a regular polygon inscribed in a circle.
    fn polygon_area(radius: f64, n: usize) -> f64 {
        let n = n as f64;
        0.5 * n * radius * radius * (std::f64::consts::TAU / n).sin()
    }
    /// Sum of signed volumes: positive for a closed solid wound outward.
    fn volume(part: &MeshPart) -> f64 {
        let mut v = 0.0;
        for t in part.indices.chunks(3) {
            let at = |i: usize| {
                part.positions
                    .get(t[i] as usize)
                    .copied()
                    .unwrap_or([0.0; 3])
            };
            let (a, b, c) = (at(0), at(1), at(2));
            v += a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
                + a[2] * (b[0] * c[1] - b[1] * c[0]);
        }
        v / 6.0
    }
    fn part(mesh: &BoardMesh, kind: PartKind) -> Result<&MeshPart, String> {
        mesh.parts
            .iter()
            .find(|p| p.kind == kind)
            .ok_or_else(|| format!("no {} part", kind.name()))
    }

    #[test]
    fn plain_rectangle_slab_has_golden_counts_and_bounds() -> Check {
        let mesh = board_mesh(&base(), slab_only())?;
        let slab = part(&mesh, PartKind::Board)?;
        assert_eq!(
            slab.positions.len(),
            24,
            "two caps of 4 plus four wall quads"
        );
        assert_eq!(slab.triangle_count(), 12);
        let (lo, hi) = slab.bounds().ok_or("no bounds")?;
        assert_eq!(lo, [0.0, 0.0, 0.0]);
        assert_eq!(hi, [40.0, 30.0, 1.6]);
        assert!(close(volume(slab), 40.0 * 30.0 * 1.6), "wound outward");
        Ok(())
    }

    #[test]
    fn concave_outline_triangulates_with_the_exact_volume() -> Check {
        let mut p = base();
        // An L shape: 40x30 with a 20x15 notch removed.
        p.board.outline = Some(vec![
            Point::new(0.0, 0.0),
            Point::new(40.0, 0.0),
            Point::new(40.0, 15.0),
            Point::new(20.0, 15.0),
            Point::new(20.0, 30.0),
            Point::new(0.0, 30.0),
        ]);
        let mesh = board_mesh(&p, slab_only())?;
        let slab = part(&mesh, PartKind::Board)?;
        assert!(close(volume(slab), 900.0 * 1.6), "{}", volume(slab));
        assert_eq!(slab.triangle_count(), 4 * 2 + 6 * 2);
        Ok(())
    }

    #[test]
    fn cutouts_holes_and_via_drills_are_cut_from_the_slab() -> Check {
        let mut p = base();
        p.board.cutouts.push(Cutout {
            id: 2,
            center: Point::new(10.0, 10.0),
            radius: 2.0,
        });
        p.board.holes.push(Hole {
            id: 3,
            position: Point::new(30.0, 20.0),
            diameter: 3.2,
            end: None,
        });
        p.board.vias.push(Via {
            id: 4,
            net: 1,
            position: Point::new(20.0, 15.0),
            diameter: 0.8,
            drill: 0.4,
            from_layer: None,
            to_layer: None,
            covering: None,
            covering_bottom: None,
        });
        let mesh = board_mesh(&p, slab_only())?;
        let slab = part(&mesh, PartKind::Board)?;
        // Cutout r2.0 -> 36 segments, hole r1.6 -> 24, via r0.2 -> 12.
        let cut = polygon_area(2.0, 36) + polygon_area(1.6, 24) + polygon_area(0.2, 12);
        assert!(
            close(volume(slab), (1200.0 - cut) * 1.6),
            "{}",
            volume(slab)
        );
        // Merged cap ring: 4 + (36 + 24 + 12) + 2 per bridge; walls 4 per edge.
        let caps = 2 * (4 + 36 + 24 + 12 + 2 * 3);
        let walls = 4 * (4 + 36 + 24 + 12);
        assert_eq!(slab.positions.len(), caps + walls);
        assert!(mesh.warnings.is_empty(), "{:?}", mesh.warnings);
        Ok(())
    }

    #[test]
    fn a_hole_that_leaves_the_outline_is_skipped_and_reported() -> Check {
        let mut p = base();
        p.board.holes.push(Hole {
            id: 2,
            position: Point::new(0.5, 15.0),
            diameter: 3.0,
            end: None,
        });
        let mesh = board_mesh(&p, slab_only())?;
        assert_eq!(part(&mesh, PartKind::Board)?.positions.len(), 24);
        assert_eq!(
            mesh.warnings,
            vec!["1 holes overlapped or left their polygon and were not cut".to_string()]
        );
        Ok(())
    }

    fn pad_at(number: &str, x: f64, y: f64, w: f64, h: f64, shape: &str) -> Pad {
        Pad {
            number: number.into(),
            offset: Point::new(x, y),
            diameter: w.min(h),
            net: None,
            castellated: None,
            geometry: Some(pcbcraft_library::FootprintPad {
                number: number.into(),
                pad_type: "smd".into(),
                shape: shape.into(),
                position: Point::new(x, y),
                rotation: 0.0,
                size: Point::new(w, h),
                layers: vec!["F.Cu".into()],
                drill: None,
                roundrect_ratio: None,
                raw_source: String::new(),
                chamfer: None,
                polygon: Vec::new(),
            }),
        }
    }
    fn through_pad(number: &str, x: f64, y: f64, d: f64, drill: f64) -> Pad {
        let mut pad = pad_at(number, x, y, d, d, "circle");
        if let Some(g) = pad.geometry.as_mut() {
            g.pad_type = "thru_hole".into();
            g.layers = vec!["*.Cu".into()];
            g.drill = Some(pcbcraft_library::Drill {
                size: Point::new(drill, drill),
                offset: Point::new(0.0, 0.0),
            });
        }
        pad
    }
    fn footprint(pads: Vec<Pad>) -> Footprint {
        Footprint {
            id: 10,
            component: 11,
            position: Point::new(20.0, 15.0),
            rotation: 0.0,
            locked: None,
            side: None,
            pads,
        }
    }
    fn body_of(pads: Vec<Pad>) -> Result<Body, String> {
        default_body(&footprint(pads), None).ok_or_else(|| "no body".to_string())
    }
    fn size_of(body: &Body) -> (f64, f64) {
        let (lo, hi) = ring_bounds(&body.ring);
        (hi.x - lo.x, hi.y - lo.y)
    }
    /// A valid project with one component whose embedded footprint holds
    /// these pads, placed at (20, 15).
    fn placed(pads: Vec<Pad>) -> Project {
        let mut p = base();
        let definition = pcbcraft_library::FootprintDefinition {
            name: "T".into(),
            description: String::new(),
            keywords: String::new(),
            pads: pads.iter().filter_map(|pad| pad.geometry.clone()).collect(),
            graphics: vec![],
            model: None,
            rules: None,
            raw_source: String::new(),
            unsupported: vec![],
        };
        p.components.push(Component {
            sourcing: Default::default(),
            id: 11,
            reference: "R1".into(),
            value: "10k".into(),
            kind: "resistor".into(),
            position: Point::new(0.0, 0.0),
            rotation: 0.0,
            pins: pads
                .iter()
                .map(|pad| pcbcraft_doc::Pin {
                    number: pad.number.clone(),
                    name: pad.number.clone(),
                    offset: Point::new(0.0, 0.0),
                    net: None,
                    electrical_type: "passive".into(),
                    direction: None,
                    unit: 1,
                })
                .collect(),
            units: vec![],
            library_symbol: None,
            library_footprint: Some(pcbcraft_doc::LibraryFootprint {
                library: "test".into(),
                definition,
            }),
            fields: vec![],
            sheet: 0,
        sourcing: Default::default(),
        compliance: Default::default(),
        });
        p.board.footprints.push(footprint(pads));
        p
    }
    fn resistor_project() -> Project {
        placed(vec![
            pad_at("1", -0.9, 0.0, 1.025, 1.4, "roundrect"),
            pad_at("2", 0.9, 0.0, 1.025, 1.4, "roundrect"),
        ])
    }

    #[test]
    fn package_families_come_from_pad_patterns() -> Check {
        // 0805 chip.
        let chip = body_of(vec![
            pad_at("1", -0.9, 0.0, 1.025, 1.4, "roundrect"),
            pad_at("2", 0.9, 0.0, 1.025, 1.4, "roundrect"),
        ])?;
        assert_eq!(chip.family, "chip");
        let (l, w) = size_of(&chip);
        assert!(close(l, 1.95) && close(w, 1.26), "{l} {w}");
        // SOIC-8.
        let mut soic = vec![];
        for i in 0..4 {
            let x = -1.905 + 1.27 * i as f64;
            soic.push(pad_at("1", x, 2.7, 0.6, 1.55, "roundrect"));
            soic.push(pad_at("2", x, -2.7, 0.6, 1.55, "roundrect"));
        }
        let soic = body_of(soic)?;
        assert_eq!(soic.family, "soic");
        let (l, w) = size_of(&soic);
        assert!(close(l, 4.71) && close(w, 3.85), "{l} {w}");
        // SOT-23.
        let sot = body_of(vec![
            pad_at("1", -1.1375, -0.95, 1.0, 0.7, "roundrect"),
            pad_at("2", -1.1375, 0.95, 1.0, 0.7, "roundrect"),
            pad_at("3", 1.1375, 0.0, 1.0, 0.7, "roundrect"),
        ])?;
        assert_eq!(sot.family, "sot");
        // QFP: 8 pads per side with long leads.
        let mut qfp = vec![];
        for i in 0..8 {
            let t = -3.5 + i as f64;
            qfp.push(pad_at("1", -4.25, t, 1.6, 0.3, "roundrect"));
            qfp.push(pad_at("2", 4.25, t, 1.6, 0.3, "roundrect"));
            qfp.push(pad_at("3", t, -4.25, 0.3, 1.6, "roundrect"));
            qfp.push(pad_at("4", t, 4.25, 0.3, 1.6, "roundrect"));
        }
        let qfp = body_of(qfp)?;
        assert_eq!(qfp.family, "qfp");
        let (w, h) = size_of(&qfp);
        assert!(close(w, 6.9) && close(h, 6.9), "{w} {h}");
        // QFN with a thermal pad.
        let mut qfn = vec![];
        for i in 0..4 {
            let t = -0.75 + 0.5 * i as f64;
            qfn.push(pad_at("1", -1.45, t, 0.8, 0.25, "roundrect"));
            qfn.push(pad_at("2", 1.45, t, 0.8, 0.25, "roundrect"));
            qfn.push(pad_at("3", t, -1.45, 0.25, 0.8, "roundrect"));
            qfn.push(pad_at("4", t, 1.45, 0.25, 0.8, "roundrect"));
        }
        qfn.push(pad_at("17", 0.0, 0.0, 1.7, 1.7, "rect"));
        assert_eq!(body_of(qfn)?.family, "qfn");
        // BGA 5x5.
        let mut bga = vec![];
        for i in 0..5 {
            for j in 0..5 {
                bga.push(pad_at(
                    "1",
                    -2.0 + i as f64,
                    -2.0 + j as f64,
                    0.5,
                    0.5,
                    "circle",
                ));
            }
        }
        let bga = body_of(bga)?;
        assert_eq!(bga.family, "bga");
        let (w, h) = size_of(&bga);
        assert!(close(w, 5.0) && close(h, 5.0), "{w} {h}");
        // DIP-8.
        let mut dip = vec![];
        for i in 0..4 {
            let x = -3.81 + 2.54 * i as f64;
            dip.push(through_pad("1", x, -3.81, 1.6, 0.8));
            dip.push(through_pad("2", x, 3.81, 1.6, 0.8));
        }
        let dip = body_of(dip)?;
        assert_eq!(dip.family, "dip");
        let (w, h) = size_of(&dip);
        assert!(close(w, 9.86) && close(h, 6.32), "{w} {h}");
        // Axial resistor, radial capacitor, 1x4 header.
        let axial = body_of(vec![
            through_pad("1", -5.08, 0.0, 1.6, 0.8),
            through_pad("2", 5.08, 0.0, 1.6, 0.8),
        ])?;
        assert_eq!(axial.family, "axial");
        let radial = body_of(vec![
            through_pad("1", -1.25, 0.0, 1.6, 0.8),
            through_pad("2", 1.25, 0.0, 1.6, 0.8),
        ])?;
        assert_eq!(radial.family, "radial");
        assert_eq!(radial.ring.len(), 36);
        let header = body_of(
            (0..4)
                .map(|i| through_pad("1", 2.54 * i as f64, 0.0, 1.7, 1.0))
                .collect(),
        )?;
        assert_eq!(header.family, "header");
        assert_eq!(header.pins.len(), 4);
        let (w, h) = size_of(&header);
        assert!(close(w, 10.16) && close(h, 2.54), "{w} {h}");
        Ok(())
    }

    fn unit_model(offset: [f64; 3], rotate: [f64; 3], scale: f64) -> Model3D {
        Model3D {
            path: "m.step".into(),
            offset,
            scale: [scale; 3],
            rotate,
            raw_source: String::new(),
        }
    }

    #[test]
    fn model_transform_applies_scale_then_x_y_z_turns_then_offset() -> Check {
        let model = unit_model([2.54, 0.0, 1.0], [90.0, 0.0, 90.0], 2.0);
        let m = model_matrix(&model, Point::new(10.0, 20.0), 0.0, false, 1.6);
        // (0,1,0): scale -> (0,2,0); Rx(-90) -> (0,0,-2); Rz(-90) leaves z;
        // offset -> (2.54, 0, -1); footprint at (10,20) on the 1.6 top face.
        let v = apply_matrix(&m, [0.0, 1.0, 0.0]);
        assert!(
            close(v[0], 12.54) && close(v[1], 20.0) && close(v[2], 0.6),
            "{v:?}"
        );
        // (1,0,0): scale -> (2,0,0); Rx leaves x; Rz(-90) -> (0,-2,0).
        let v = apply_matrix(&m, [1.0, 0.0, 0.0]);
        assert!(
            close(v[0], 12.54) && close(v[1], 18.0) && close(v[2], 2.6),
            "{v:?}"
        );
        Ok(())
    }

    #[test]
    fn footprint_rotation_turns_the_model_with_the_canvas_sign() -> Check {
        let model = unit_model([0.0; 3], [0.0; 3], 1.0);
        let m = model_matrix(&model, Point::new(0.0, 0.0), 90.0, false, 0.0);
        let v = apply_matrix(&m, [1.0, 0.0, 0.0]);
        let canvas = Point::new(1.0, 0.0).rotate(-90.0);
        assert!(close(v[0], canvas.x) && close(v[1], canvas.y), "{v:?}");
        Ok(())
    }

    #[test]
    fn a_bottom_part_with_z_offset_sits_below_the_bottom_face() -> Check {
        let model = unit_model([0.0, 0.0, 1.0], [0.0; 3], 1.0);
        let m = model_matrix(&model, Point::new(5.0, 5.0), 0.0, true, 1.6);
        let v = apply_matrix(&m, [0.0, 0.0, 0.0]);
        assert!(close(v[2], -1.0), "{v:?}");
        Ok(())
    }

    #[test]
    fn a_placed_resistor_gets_pads_mask_openings_and_a_chip_body() -> Check {
        let mesh = board_mesh(&resistor_project(), MeshLayers::default())?;
        let copper = part(&mesh, PartKind::CopperTop)?;
        // Two rectangular pads: 24 vertices each.
        assert_eq!(copper.positions.len(), 48);
        let (lo, hi) = copper.bounds().ok_or("no copper bounds")?;
        assert!(close(lo[2], 1.6) && close(hi[2], 1.6 + COPPER_THICKNESS));
        assert!(lo[0] > 18.0 && hi[0] < 22.0);
        let mask = part(&mesh, PartKind::MaskTop)?;
        // Outer 4 plus two 4-corner openings with 2 bridge vertices each.
        assert_eq!(
            mask.positions.len(),
            2 * (4 + 2 * (4 + 2)) + 4 * (4 + 2 * 4)
        );
        let body = part(&mesh, PartKind::Body)?;
        assert_eq!(body.family.as_deref(), Some("chip"));
        assert_eq!(body.component, Some(11));
        let (blo, bhi) = body.bounds().ok_or("no body bounds")?;
        assert!(
            close(blo[2], 1.6) && close(bhi[2], 1.6 + 0.55),
            "{blo:?} {bhi:?}"
        );
        assert!(close(bhi[0] - blo[0], 1.95));
        assert!(mesh.parts.iter().all(|p| p.kind != PartKind::CopperBottom));
        Ok(())
    }

    #[test]
    fn output_is_deterministic() -> Check {
        let p = resistor_project();
        let a = board_mesh(&p, MeshLayers::default())?;
        let b = board_mesh(&p, MeshLayers::default())?;
        assert_eq!(a, b);
        assert_eq!(a.to_json(true).to_string(), b.to_json(true).to_string());
        Ok(())
    }

    #[test]
    fn a_height_field_overrides_the_default() -> Check {
        let mut p = resistor_project();
        if let Some(c) = p.components.first_mut() {
            c.fields.push(pcbcraft_doc::ComponentField {
                key: "height".into(),
                value: "2.5 mm".into(),
            });
        }
        let mesh = board_mesh(&p, MeshLayers::default())?;
        let body = part(&mesh, PartKind::Body)?;
        assert_eq!(body.height_source, Some("field"));
        let (blo, bhi) = body.bounds().ok_or("no bounds")?;
        assert!(close(bhi[2] - blo[2], 2.5));
        if let Some(f) = p.components.first_mut().and_then(|c| c.fields.first_mut()) {
            f.value = "huge".into();
        }
        let mesh = board_mesh(&p, MeshLayers::default())?;
        assert_eq!(part(&mesh, PartKind::Body)?.height_source, Some("default"));
        assert_eq!(mesh.warnings.len(), 1);
        Ok(())
    }

    #[test]
    fn through_hole_pads_cut_the_slab_and_plate_both_sides() -> Check {
        let p = placed(vec![
            through_pad("1", -1.27, 0.0, 1.7, 1.0),
            through_pad("2", 1.27, 0.0, 1.7, 1.0),
        ]);
        let mesh = board_mesh(&p, MeshLayers::default())?;
        assert!(part(&mesh, PartKind::CopperBottom).is_ok());
        let slab = part(&mesh, PartKind::Board)?;
        // Drill 1.0 mm oval outline: 4 corner arcs of 17 points, thinned and
        // cleaned; only check the slab lost the drilled volume.
        assert!(volume(slab) < 1200.0 * 1.6 - 1.0);
        assert!(volume(slab) > 1200.0 * 1.6 - 3.0);
        Ok(())
    }

    #[test]
    fn tracks_and_arc_tracks_become_copper_strips() -> Check {
        let mut p = base();
        p.board.tracks.push(Track {
            id: 2,
            net: 1,
            start: Point::new(5.0, 5.0),
            end: Point::new(15.0, 5.0),
            width: 0.5,
            layer: Layer::Front,
            arc: None,
        });
        p.board.tracks.push(Track {
            id: 3,
            net: 1,
            start: Point::new(5.0, 10.0),
            end: Point::new(15.0, 10.0),
            width: 0.5,
            layer: Layer::Back,
            arc: Some(Point::new(10.0, 12.0)),
        });
        let mesh = board_mesh(&p, MeshLayers::default())?;
        let top = part(&mesh, PartKind::CopperTop)?;
        assert_eq!(top.positions.len(), 24, "one straight strip");
        assert!(close(volume(top), 10.0 * 0.5 * COPPER_THICKNESS));
        let bottom = part(&mesh, PartKind::CopperBottom)?;
        let (lo, hi) = bottom.bounds().ok_or("no bounds")?;
        assert!(hi[1] > 11.9 && lo[2] < 0.0 && close(hi[2], 0.0));
        Ok(())
    }

    #[test]
    fn zones_pour_with_cutouts_and_unfilled_zones_are_left_out() -> Check {
        let mut p = base();
        let mut outline = rect(5.0, 5.0, 25.0, 25.0);
        outline.push(outline[0]);
        let mut cut = rect(10.0, 10.0, 12.0, 12.0);
        cut.push(cut[0]);
        p.board.zones.push(CopperZone {
            id: 2,
            net: 1,
            layer: Layer::Front,
            outline: outline.clone(),
            cutouts: vec![cut],
            filled: true,
            kind: None,
            pour: None,
        });
        p.board.zones.push(CopperZone {
            id: 3,
            net: 1,
            layer: Layer::Front,
            outline,
            cutouts: vec![],
            filled: false,
            kind: None,
            pour: None,
        });
        let mesh = board_mesh(&p, MeshLayers::default())?;
        let top = part(&mesh, PartKind::CopperTop)?;
        assert!(
            close(volume(top), (400.0 - 4.0) * COPPER_THICKNESS),
            "{}",
            volume(top)
        );
        Ok(())
    }

    #[test]
    fn silk_user_layers_become_thin_strokes_above_the_mask() -> Check {
        let mut p = base();
        p.board.user_layers.push(pcbcraft_doc::ArtLayer {
            id: 2,
            name: "Marks".into(),
            kind: ArtKind::Silk,
            dotted: false,
            dot_pitch: 1.0,
            dot_size: 0.5,
            visible: true,
            shapes: vec![
                ArtShape::Line {
                    start: Point::new(5.0, 5.0),
                    end: Point::new(10.0, 5.0),
                },
                ArtShape::Text {
                    at: Point::new(1.0, 1.0),
                    text: "x".into(),
                    size: 1.0,
                    rotate: 0.0,
                },
            ],
        });
        let mesh = board_mesh(&p, MeshLayers::default())?;
        let silk = part(&mesh, PartKind::SilkTop)?;
        assert_eq!(silk.positions.len(), 24);
        let (lo, hi) = silk.bounds().ok_or("no bounds")?;
        assert!(close(lo[2], 1.6 + COPPER_THICKNESS + MASK_THICKNESS));
        assert!(close(hi[2] - lo[2], SILK_THICKNESS));
        assert_eq!(
            mesh.warnings,
            vec!["1 drawn text labels are not shown in 3D".to_string()]
        );
        Ok(())
    }

    #[test]
    fn the_triangle_budget_refuses_a_runaway_board() -> Check {
        let mut budget = Budget {
            triangles: MAX_TRIANGLES - 1,
        };
        assert!(budget.spend(1).is_ok());
        assert!(budget.spend(1).is_err());
        Ok(())
    }

    #[test]
    fn json_carries_counts_bounds_and_optional_geometry() -> Check {
        let mesh = board_mesh(&resistor_project(), MeshLayers::default())?;
        let full = mesh.to_json(true);
        let summary = mesh.to_json(false);
        assert_eq!(full["vertices"], summary["vertices"]);
        assert!(full["parts"][0].get("positions").is_some());
        assert!(summary["parts"][0].get("positions").is_none());
        assert_eq!(full["bounds"]["min"][0], 0.0);
        Ok(())
    }
}
