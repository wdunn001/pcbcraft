//! Poured copper prepared for a Gerber copper file: ring thinning,
//! container-first ordering and the per-layer operation budget.
//!
//! # Rules
//! - Thinning keeps original vertices only, so no point moves off the fill.
//! - A ring that collapses below three vertices or a hair of area is dropped.
//!
//! # Dependencies
//! - crates/geom/src/poly.rs Piece, Ring
//! - crates/io/src/lib.rs gerber_files_with_budget (the only caller)
//!
//! # Do not
//! - Round vertices here; the writer owns the number format.
use pcbcraft_geom::poly::{Piece, Ring};
use pcbcraft_geom::{Point, polygon_area};

/// Operations one copper layer file may carry (flashes, stroke points and
/// region vertices). A dense six-layer board with full pours stays near a tenth
/// of this; the cap exists to stop runaway output, not to size real boards.
pub const MAX_COPPER_LAYER_OPS: usize = 1_000_000;
/// Simplification tolerances in millimetres, finest first. The first is below
/// what any fabricator resolves; the coarser ones are the fallback when a pour
/// would not fit the layer budget.
pub const TOLERANCES_MM: [f64; 3] = [0.001, 0.005, 0.02];
/// Smallest ring area in square millimetres worth writing.
const MIN_RING_AREA: f64 = 1e-6;

/// Perpendicular distance from `p` to the segment `a`..`b`.
fn distance_to_segment(p: Point, a: Point, b: Point) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let length_sq = dx * dx + dy * dy;
    if length_sq <= f64::EPSILON {
        return ((p.x - a.x).powi(2) + (p.y - a.y).powi(2)).sqrt();
    }
    let t = (((p.x - a.x) * dx + (p.y - a.y) * dy) / length_sq).clamp(0.0, 1.0);
    let (cx, cy) = (a.x + t * dx, a.y + t * dy);
    ((p.x - cx).powi(2) + (p.y - cy).powi(2)).sqrt()
}

/// Mark the vertices of `points[from..=to]` that Douglas-Peucker keeps.
fn keep_between(points: &[Point], from: usize, to: usize, tolerance: f64, keep: &mut [bool]) {
    let mut stack = vec![(from, to)];
    while let Some((lo, hi)) = stack.pop() {
        if hi <= lo + 1 {
            continue;
        }
        let mut far = lo;
        let mut far_distance = 0.0;
        for index in lo + 1..hi {
            let d = distance_to_segment(points[index], points[lo], points[hi]);
            if d > far_distance {
                far_distance = d;
                far = index;
            }
        }
        if far_distance > tolerance {
            keep[far] = true;
            stack.push((lo, far));
            stack.push((far, hi));
        }
    }
}

/// A ring thinned to `tolerance`, open (no repeated first point). Returns
/// `None` when it collapses below a triangle or below `MIN_RING_AREA`.
fn simplify_ring(ring: &Ring, tolerance: f64) -> Option<Ring> {
    let mut points: Vec<Point> = Vec::with_capacity(ring.len());
    for p in ring {
        if points.last().is_none_or(|q| q.x != p.x || q.y != p.y) {
            points.push(*p);
        }
    }
    if points.len() > 1
        && points
            .first()
            .zip(points.last())
            .is_some_and(|(a, b)| a.x == b.x && a.y == b.y)
    {
        points.pop();
    }
    if points.len() < 3 {
        return None;
    }
    // Anchor on the first vertex and the vertex farthest from it, then thin
    // each half; a closed ring has no natural end to start from.
    let far = (1..points.len()).max_by(|a, b| {
        let da = (points[*a].x - points[0].x).hypot(points[*a].y - points[0].y);
        let db = (points[*b].x - points[0].x).hypot(points[*b].y - points[0].y);
        da.total_cmp(&db)
    })?;
    let mut keep = vec![false; points.len()];
    keep[0] = true;
    keep[far] = true;
    keep_between(&points, 0, far, tolerance, &mut keep);
    let mut tail: Vec<Point> = points[far..].to_vec();
    tail.push(points[0]);
    let mut tail_keep = vec![false; tail.len()];
    tail_keep[0] = true;
    tail_keep[tail.len() - 1] = true;
    keep_between(&tail, 0, tail.len() - 1, tolerance, &mut tail_keep);
    let mut out: Vec<Point> = Vec::new();
    for (index, point) in points.iter().enumerate().take(far + 1) {
        if keep[index] {
            out.push(*point);
        }
    }
    for (index, point) in tail.iter().enumerate().take(tail.len() - 1).skip(1) {
        if tail_keep[index] {
            out.push(*point);
        }
    }
    (out.len() >= 3 && polygon_area(&out).abs() >= MIN_RING_AREA).then_some(out)
}

/// A pour piece thinned to `tolerance`: outer ring first, then its holes. A
/// piece whose outer ring collapses is dropped; a collapsed hole is dropped alone.
pub fn simplify_piece(piece: &Piece, tolerance: f64) -> Option<Piece> {
    let (outer, holes) = piece.split_first()?;
    let outer = simplify_ring(outer, tolerance)?;
    let mut rings = vec![outer];
    rings.extend(holes.iter().filter_map(|h| simplify_ring(h, tolerance)));
    Some(rings)
}

/// Written vertices of a piece, counting each ring's closing point.
pub fn piece_vertices(piece: &Piece) -> usize {
    piece.iter().map(|ring| ring.len() + 1).sum()
}

/// Sort pieces so containers precede anything inside their holes: outer ring
/// area, largest first. A clear-polarity hole written after a piece that sits
/// in it would erase that piece.
pub fn order_containers_first(pieces: &mut [Piece]) {
    pieces.sort_by(|a, b| {
        let area = |p: &Piece| p.first().map_or(0.0, |r| polygon_area(r).abs());
        area(b).total_cmp(&area(a))
    });
}
