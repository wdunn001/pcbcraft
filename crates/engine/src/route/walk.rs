//! Walkaround: bend a 45-degree path around octagonal hulls, with counted limits only.
use pcbcraft_doc::Id;
use pcbcraft_geom::hull::{boundary_position, boundary_vertices, clip_segment, strictly_inside};
use pcbcraft_geom::{Point, segment_distance};

/// Most hull detours one walkaround may add before it gives up.
pub const WALK_ITERATIONS: usize = 40;
const STRICT: f64 = 1e-6;
const MERGE: f64 = 1e-7;

#[derive(Clone, Debug)]
pub struct Hull {
    pub pts: Vec<Point>,
    pub net: Id,
}

/// Hulls to stay out of and the rectangle the path centre line must stay inside.
pub struct Space {
    pub hulls: Vec<Hull>,
    pub lo: Point,
    pub hi: Point,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Turn {
    Shortest,
    Ccw,
    Cw,
}

fn lerp(a: Point, b: Point, t: f64) -> Point {
    Point::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t)
}

pub fn length(path: &[Point]) -> f64 {
    path.windows(2).map(|w| w[0].distance(w[1])).sum()
}

/// Number of corners sharper than a right angle.
pub fn acute(path: &[Point]) -> usize {
    path.windows(3)
        .filter(|w| {
            let (ax, ay) = (w[1].x - w[0].x, w[1].y - w[0].y);
            let (bx, by) = (w[2].x - w[1].x, w[2].y - w[1].y);
            let n = ax.hypot(ay) * bx.hypot(by);
            n > 1e-12 && (ax * bx + ay * by) / n < -0.1
        })
        .count()
}

/// Drop repeated points and points that sit on a straight run.
pub fn tidy(path: &[Point]) -> Vec<Point> {
    let mut out: Vec<Point> = Vec::with_capacity(path.len());
    for p in path {
        if out.last().is_none_or(|q| q.distance(*p) > MERGE) {
            out.push(*p);
        }
    }
    let mut result: Vec<Point> = Vec::with_capacity(out.len());
    for p in out {
        while result.len() >= 2 {
            let (a, b) = (result[result.len() - 2], result[result.len() - 1]);
            let cross = (b.x - a.x) * (p.y - b.y) - (b.y - a.y) * (p.x - b.x);
            let dot = (b.x - a.x) * (p.x - b.x) + (b.y - a.y) * (p.y - b.y);
            if cross.abs() < 1e-9 && dot > 0.0 {
                result.pop();
            } else {
                break;
            }
        }
        result.push(p);
    }
    result
}

fn first_hit(path: &[Point], hulls: &[Hull]) -> Option<(usize, usize)> {
    for (i, w) in path.windows(2).enumerate() {
        let mut best: Option<(f64, usize)> = None;
        for (h, hull) in hulls.iter().enumerate() {
            if let Some((t0, _)) = clip_segment(&hull.pts, w[0], w[1], STRICT)
                && best.is_none_or(|(t, _)| t0 < t)
            {
                best = Some((t0, h));
            }
        }
        if let Some((_, h)) = best {
            return Some((i, h));
        }
    }
    None
}

/// Re-route `path` around every hull it crosses. The first and last points never move.
pub fn walkaround(path: &[Point], space: &Space, turn: Turn) -> Option<Vec<Point>> {
    let mut path = tidy(path);
    for _ in 0..WALK_ITERATIONS {
        let Some((i, h)) = first_hit(&path, &space.hulls) else {
            return Some(path);
        };
        let hull = &space.hulls.get(h)?.pts;
        if strictly_inside(hull, *path.first()?, STRICT)
            || strictly_inside(hull, *path.last()?, STRICT)
        {
            return None;
        }
        let (a, b) = (*path.get(i)?, *path.get(i + 1)?);
        let (t0, _) = clip_segment(hull, a, b, 0.0)?;
        let entry = lerp(a, b, t0);
        let mut last = i;
        for (j, w) in path.windows(2).enumerate().skip(i) {
            if clip_segment(hull, w[0], w[1], STRICT).is_some() {
                last = j;
            }
        }
        let (c, d) = (*path.get(last)?, *path.get(last + 1)?);
        let (_, t1) = clip_segment(hull, c, d, 0.0)?;
        let exit = lerp(c, d, t1);
        let (se, _) = boundary_position(hull, entry);
        let (sx, _) = boundary_position(hull, exit);
        let detour = |ccw: bool| {
            let mut v = vec![entry];
            v.extend(boundary_vertices(hull, se, sx, ccw));
            v.push(exit);
            v
        };
        let (ccw, cw) = (detour(true), detour(false));
        let pick = match turn {
            Turn::Ccw => ccw,
            Turn::Cw => cw,
            Turn::Shortest => {
                if length(&cw) < length(&ccw) {
                    cw
                } else {
                    ccw
                }
            }
        };
        let mut next: Vec<Point> = path.get(..=i)?.to_vec();
        next.extend(pick);
        next.extend(path.get(last + 1..)?.iter().copied());
        path = tidy(&next);
    }
    None
}

/// Legal when clear of every hull, inside the keep-in rectangle and not crossing itself.
pub fn path_ok(path: &[Point], space: &Space) -> bool {
    if first_hit(path, &space.hulls).is_some() {
        return false;
    }
    let inside = |p: &Point| {
        p.x >= space.lo.x - 1e-9
            && p.y >= space.lo.y - 1e-9
            && p.x <= space.hi.x + 1e-9
            && p.y <= space.hi.y + 1e-9
    };
    if !path.iter().all(inside) {
        return false;
    }
    for i in 0..path.len().saturating_sub(1) {
        for j in i + 2..path.len().saturating_sub(1) {
            if segment_distance(path[i], path[i + 1], path[j], path[j + 1]) < 1e-9 {
                return false;
            }
        }
    }
    true
}

fn connections(a: Point, b: Point) -> Vec<Vec<Point>> {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    if dx.abs() < 1e-9 || dy.abs() < 1e-9 || (dx.abs() - dy.abs()).abs() < 1e-9 {
        return vec![vec![]];
    }
    let m = dx.abs().min(dy.abs());
    let (sx, sy) = (dx.signum() * m, dy.signum() * m);
    vec![
        vec![Point::new(a.x + sx, a.y + sy)],
        vec![Point::new(b.x - sx, b.y - sy)],
        vec![Point::new(b.x, a.y)],
        vec![Point::new(a.x, b.y)],
    ]
}

fn cost(path: &[Point]) -> (usize, f64) {
    (acute(path), length(path))
}

fn better(a: (usize, f64), b: (usize, f64)) -> bool {
    a.0 < b.0 || (a.0 == b.0 && a.1 < b.1 - 1e-9)
}

/// Shorten the path and remove sharp corners by replacing stretches with a
/// straight or one-bend 45-degree link, keeping both ends fixed.
pub fn optimize(path: Vec<Point>, space: &Space) -> Vec<Point> {
    let mut path = tidy(&path);
    for _ in 0..200 {
        let current = cost(&path);
        let mut found: Option<Vec<Point>> = None;
        'search: for i in 0..path.len() {
            for j in (i + 2..path.len()).rev() {
                let mut best: Option<((usize, f64), Vec<Point>)> = None;
                for mids in connections(path[i], path[j]) {
                    let mut trial: Vec<Point> = path[..=i].to_vec();
                    trial.extend(mids);
                    trial.extend_from_slice(&path[j..]);
                    let trial = tidy(&trial);
                    let c = cost(&trial);
                    if better(c, current)
                        && best.as_ref().is_none_or(|(b, _)| better(c, *b))
                        && path_ok(&trial, space)
                    {
                        best = Some((c, trial));
                    }
                }
                if let Some((_, t)) = best {
                    found = Some(t);
                    break 'search;
                }
            }
        }
        match found {
            Some(next) => path = next,
            None => break,
        }
    }
    path
}

/// Skeleton routes between two points: a straight run when aligned, otherwise
/// the two 45-degree one-bend routes and the two right-angle routes.
pub fn skeletons(a: Point, b: Point) -> Vec<Vec<Point>> {
    let mut out = vec![];
    let mut seen: Vec<Vec<Point>> = vec![];
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let m = dx.abs().min(dy.abs());
    let (sx, sy) = (dx.signum() * m, dy.signum() * m);
    let corners = [
        Point::new(a.x + sx, a.y + sy),
        Point::new(b.x - sx, b.y - sy),
        Point::new(b.x, a.y),
        Point::new(a.x, b.y),
    ];
    for c in corners {
        let s = tidy(&[a, c, b]);
        if !seen.contains(&s) {
            seen.push(s.clone());
            out.push(s);
        }
    }
    out
}

/// Why a path could not be found.
pub enum Failure {
    OutsideBoard,
    TooClose(Id),
    NoPath,
}

/// Best legal path from `start` to `end`: fewest sharp corners, then shortest.
pub fn find_path(start: Point, end: Point, space: &Space) -> Result<Vec<Point>, Failure> {
    for p in [start, end] {
        if p.x < space.lo.x - 1e-9
            || p.y < space.lo.y - 1e-9
            || p.x > space.hi.x + 1e-9
            || p.y > space.hi.y + 1e-9
        {
            return Err(Failure::OutsideBoard);
        }
        if let Some(h) = space
            .hulls
            .iter()
            .find(|h| strictly_inside(&h.pts, p, STRICT))
        {
            return Err(Failure::TooClose(h.net));
        }
    }
    let mut best: Option<Vec<Point>> = None;
    for skeleton in skeletons(start, end) {
        for turn in [Turn::Shortest, Turn::Ccw, Turn::Cw] {
            let Some(walked) = walkaround(&skeleton, space, turn) else {
                continue;
            };
            let path = optimize(walked, space);
            if acute(&path) == 0
                && path_ok(&path, space)
                && best.as_ref().is_none_or(|b| better(cost(&path), cost(b)))
            {
                best = Some(path);
            }
        }
    }
    best.ok_or(Failure::NoPath)
}
