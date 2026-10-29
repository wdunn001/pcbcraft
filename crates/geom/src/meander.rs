//! Serpentine (meander) geometry for length tuning.
//!
//! A meander replaces a straight run with `bumps` one-sided bumps. Each bump
//! rises `amplitude` perpendicular to the run, travels `pitch` along it and
//! comes back, then the run continues `pitch` before the next bump, so every
//! pair of neighbouring legs is `pitch - width` apart edge to edge. Corners
//! are either mitred at 45 degrees or rounded. Every shape is built
//! centred in the run and measured exactly, so a caller solves amplitude
//! against real length rather than a formula.
use crate::Point;

/// How corners are cut.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Corner {
    /// 45 degree mitres.
    Mitre,
    /// Circular arcs.
    Round,
}

/// One piece of a meander path.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Seg {
    Line(Point, Point),
    /// Start, a point on the arc between the ends, and the end.
    Arc(Point, Point, Point),
}

impl Seg {
    pub fn start(&self) -> Point {
        match *self {
            Seg::Line(a, _) | Seg::Arc(a, _, _) => a,
        }
    }
    pub fn end(&self) -> Point {
        match *self {
            Seg::Line(_, b) | Seg::Arc(_, _, b) => b,
        }
    }
    /// Length in millimetres.
    pub fn length(&self) -> f64 {
        match *self {
            Seg::Line(a, b) => a.distance(b),
            Seg::Arc(a, m, b) => crate::arc_through(a, m, b).map_or(a.distance(b), |c| c.length()),
        }
    }
}

/// Total length of a path.
pub fn path_length(path: &[Seg]) -> f64 {
    path.iter().map(Seg::length).sum()
}

/// The run being tuned and the shape wanted.
#[derive(Clone, Copy, Debug)]
pub struct Run {
    pub start: Point,
    pub end: Point,
    /// Copper width of the tuned track.
    pub width: f64,
    /// Edge to edge gap between neighbouring legs.
    pub spacing: f64,
    pub corner: Corner,
    /// Which side of the run the bumps rise on: 1 is to the left of the
    /// direction start to end, -1 to the right.
    pub side: i8,
}

impl Run {
    fn length(&self) -> f64 {
        self.start.distance(self.end)
    }
    /// Distance between leg centrelines.
    pub fn pitch(&self) -> f64 {
        self.spacing + self.width
    }
    fn cut(&self, amplitude: f64) -> f64 {
        (0.25 * self.pitch()).min(0.4 * amplitude)
    }
    fn margin(&self, amplitude: f64) -> f64 {
        self.spacing.max(self.cut(amplitude))
    }
    /// Most bumps that fit in the run for an amplitude, capped at 64.
    pub fn max_bumps(&self, amplitude: f64) -> usize {
        let room = self.length() - 2.0 * self.margin(amplitude) + self.pitch();
        let n = (room / (2.0 * self.pitch())).floor();
        if n.is_finite() && n >= 1.0 {
            (n as usize).min(64)
        } else {
            0
        }
    }
}

fn corner(path: &mut Vec<Seg>, from: Point, at: Point, to: Point, cut: f64, style: Corner) {
    let (d1, d2) = (unit(from, at), unit(at, to));
    let p1 = Point::new(at.x - d1.x * cut, at.y - d1.y * cut);
    let p2 = Point::new(at.x + d2.x * cut, at.y + d2.y * cut);
    match style {
        Corner::Mitre => path.push(Seg::Line(p1, p2)),
        Corner::Round => {
            let centre = Point::new(p1.x + d2.x * cut, p1.y + d2.y * cut);
            let (vx, vy) = (at.x - centre.x, at.y - centre.y);
            let norm = vx.hypot(vy);
            if norm < 1e-12 {
                path.push(Seg::Line(p1, p2));
            } else {
                let mid = Point::new(centre.x + vx / norm * cut, centre.y + vy / norm * cut);
                path.push(Seg::Arc(p1, mid, p2));
            }
        }
    }
}

fn unit(a: Point, b: Point) -> Point {
    let d = a.distance(b);
    if d < 1e-12 {
        Point::new(0.0, 0.0)
    } else {
        Point::new((b.x - a.x) / d, (b.y - a.y) / d)
    }
}

/// The meander path for `bumps` bumps of `amplitude`, or none when the shape
/// does not fit the run or an input is unusable. The first segment starts at
/// `run.start` and the last ends at `run.end`.
pub fn shape(run: &Run, bumps: usize, amplitude: f64) -> Option<Vec<Seg>> {
    let length = run.length();
    let pitch = run.pitch();
    if bumps == 0
        || bumps > 64
        || !(length.is_finite() && length > 0.0)
        || !(amplitude.is_finite() && amplitude > 0.0)
        || !(pitch.is_finite() && pitch > 0.0)
    {
        return None;
    }
    let cut = run.cut(amplitude);
    let span = (2 * bumps - 1) as f64 * pitch;
    let x0 = (length - span) / 2.0;
    if cut <= 0.0 || x0 + 1e-9 < run.margin(amplitude) {
        return None;
    }
    let dir = unit(run.start, run.end);
    let normal = Point::new(-dir.y * f64::from(run.side), dir.x * f64::from(run.side));
    let at = |x: f64, y: f64| {
        Point::new(
            run.start.x + dir.x * x + normal.x * y,
            run.start.y + dir.y * x + normal.y * y,
        )
    };
    // Vertices of the square wave, then the corners are cut.
    let mut pts = vec![at(0.0, 0.0)];
    for k in 0..bumps {
        let x = x0 + (2 * k) as f64 * pitch;
        pts.push(at(x, 0.0));
        pts.push(at(x, amplitude));
        pts.push(at(x + pitch, amplitude));
        pts.push(at(x + pitch, 0.0));
    }
    pts.push(at(length, 0.0));
    let mut path = Vec::new();
    let mut cursor = pts[0];
    for i in 1..pts.len() - 1 {
        let (before, here, after) = (pts[i - 1], pts[i], pts[i + 1]);
        // The first and last points are lead in and out ends, not corners.
        let mut piece = Vec::new();
        corner(&mut piece, before, here, after, cut, run.corner);
        if let Some(first) = piece.first()
            && cursor.distance(first.start()) > 1e-9
        {
            path.push(Seg::Line(cursor, first.start()));
        }
        if let Some(last) = piece.last() {
            cursor = last.end();
        }
        path.extend(piece);
    }
    let last = *pts.last()?;
    if cursor.distance(last) > 1e-9 {
        path.push(Seg::Line(cursor, last));
    }
    path.iter()
        .all(|s| s.length().is_finite() && s.start().valid() && s.end().valid())
        .then_some(path)
}

/// Length a shape adds over the straight run it replaces.
pub fn added_length(run: &Run, bumps: usize, amplitude: f64) -> Option<f64> {
    shape(run, bumps, amplitude).map(|p| path_length(&p) - run.length())
}

/// Smallest amplitude a bump may have, in millimetres.
pub const MIN_AMPLITUDE: f64 = 0.1;

/// The amplitude, within `MIN_AMPLITUDE..=max`, at which `bumps` bumps add
/// exactly `extra` millimetres, or none when that is out of reach.
pub fn amplitude_for(run: &Run, bumps: usize, extra: f64, max: f64) -> Option<f64> {
    if !(extra.is_finite() && extra > 0.0) || !(max.is_finite() && max >= MIN_AMPLITUDE) {
        return None;
    }
    let low = added_length(run, bumps, MIN_AMPLITUDE)?;
    let high = added_length(run, bumps, max)?;
    if extra < low - 1e-9 || extra > high + 1e-9 {
        return None;
    }
    let (mut lo, mut hi) = (MIN_AMPLITUDE, max);
    for _ in 0..80 {
        let mid = (lo + hi) / 2.0;
        match added_length(run, bumps, mid) {
            Some(a) if a < extra => lo = mid,
            Some(_) => hi = mid,
            None => return None,
        }
    }
    Some((lo + hi) / 2.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seg_length_of_line_and_arc() {
        let line = Seg::Line(Point::new(0.0, 0.0), Point::new(3.0, 4.0));
        assert!((line.length() - 5.0).abs() < 1e-12);
        let half = Seg::Arc(
            Point::new(1.0, 0.0),
            Point::new(0.0, 1.0),
            Point::new(-1.0, 0.0),
        );
        assert!((half.length() - std::f64::consts::PI).abs() < 1e-9);
        let flat = Seg::Arc(
            Point::new(0.0, 0.0),
            Point::new(1.0, 0.0),
            Point::new(2.0, 0.0),
        );
        assert!((flat.length() - 2.0).abs() < 1e-12);
    }

    #[test]
    fn path_length_sums_segments() {
        assert_eq!(path_length(&[]), 0.0);
        let path = [
            Seg::Line(Point::new(0.0, 0.0), Point::new(3.0, 4.0)),
            Seg::Line(Point::new(3.0, 4.0), Point::new(3.0, 6.0)),
            Seg::Arc(
                Point::new(1.0, 0.0),
                Point::new(0.0, 1.0),
                Point::new(-1.0, 0.0),
            ),
        ];
        assert!((path_length(&path) - (7.0 + std::f64::consts::PI)).abs() < 1e-9);
    }
    type T = Result<(), String>;
    fn run(corner: Corner) -> Run {
        Run {
            start: Point::new(0.0, 0.0),
            end: Point::new(40.0, 0.0),
            width: 0.3,
            spacing: 1.0,
            corner,
            side: 1,
        }
    }
    fn some<V>(v: Option<V>) -> Result<V, String> {
        v.ok_or_else(|| "no meander".to_string())
    }

    #[test]
    fn path_is_continuous_and_keeps_its_ends() -> T {
        for c in [Corner::Mitre, Corner::Round] {
            let r = run(c);
            let path = some(shape(&r, 3, 2.0))?;
            assert_eq!(path[0].start(), r.start);
            assert!(path[path.len() - 1].end().distance(r.end) < 1e-9);
            for pair in path.windows(2) {
                assert!(pair[0].end().distance(pair[1].start()) < 1e-9);
            }
        }
        Ok(())
    }

    #[test]
    fn solved_amplitude_adds_the_requested_length() -> T {
        for c in [Corner::Mitre, Corner::Round] {
            let r = run(c);
            let a = some(amplitude_for(&r, 4, 10.0, 8.0))?;
            let added = some(added_length(&r, 4, a))?;
            assert!((added - 10.0).abs() < 1e-6, "{added}");
        }
        Ok(())
    }

    #[test]
    fn legs_keep_the_requested_spacing() -> T {
        let r = run(Corner::Mitre);
        let path = some(shape(&r, 3, 2.0))?;
        let legs: Vec<f64> = path
            .iter()
            .filter_map(|s| match s {
                Seg::Line(a, b) if (a.x - b.x).abs() < 1e-9 && a.distance(*b) > 0.5 => Some(a.x),
                _ => None,
            })
            .collect();
        assert_eq!(legs.len(), 6);
        for pair in legs.windows(2) {
            assert!((pair[1] - pair[0] - r.pitch()).abs() < 1e-6, "{pair:?}");
        }
        Ok(())
    }

    #[test]
    fn bumps_rise_on_the_chosen_side_only() -> T {
        let mut r = run(Corner::Mitre);
        let up = some(shape(&r, 2, 1.5))?;
        assert!(
            up.iter()
                .all(|s| s.start().y >= -1e-9 && s.end().y >= -1e-9)
        );
        r.side = -1;
        let down = some(shape(&r, 2, 1.5))?;
        assert!(
            down.iter()
                .all(|s| s.start().y <= 1e-9 && s.end().y <= 1e-9)
        );
        Ok(())
    }

    #[test]
    fn unfit_shapes_are_refused() {
        let r = run(Corner::Mitre);
        assert!(shape(&r, 0, 1.0).is_none());
        assert!(shape(&r, 65, 1.0).is_none());
        assert!(shape(&r, 40, 1.0).is_none(), "too many bumps for the run");
        assert!(shape(&r, 2, f64::NAN).is_none());
        assert!(amplitude_for(&r, 2, 1.0e6, 5.0).is_none());
        assert!(amplitude_for(&r, 2, -1.0, 5.0).is_none());
        let n = r.max_bumps(1.0);
        assert!(n >= 1 && shape(&r, n, 1.0).is_some() && shape(&r, n + 1, 1.0).is_none());
    }
}
