//! Preset board shapes: turn a shape name and its sizes into outline vertices
//! and arc midpoints for `board.set_shape`.
//!
//! Every generator is pure: sizes in, a closed counter-clockwise ring out, with
//! the lower-left corner of the vertices at exactly zero. Round shapes use
//! quarter-turn arcs, so a circle is four vertices and four arcs, not a polygon.
//! Rules: sizes are finite and between 0.1 and 2000 mm; a corner radius or cut
//! must leave a real straight part; an ellipse is a 40 sided polygon because the
//! outline arcs are circular.
//!
//! # Dependencies
//! - crates/geom/src/lib.rs (Point)

use pcbcraft_geom::Point;

/// Vertices on the ellipse preset.
const ELLIPSE_SIDES: usize = 40;
/// Most sides the polygon preset accepts.
pub(super) const MAX_SIDES: usize = 60;
/// Smallest and largest size any preset accepts, in millimetres.
const MIN_SIZE: f64 = 0.1;
const MAX_SIZE: f64 = 2000.0;

/// Sizes a preset may use; each shape reads only the ones it names.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Sizes {
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub diameter: Option<f64>,
    pub radius: Option<f64>,
    pub sides: Option<usize>,
    pub notch_width: Option<f64>,
    pub notch_height: Option<f64>,
    pub chamfer: Option<f64>,
}

/// Vertices plus one optional arc midpoint per segment.
pub(super) type Ring = (Vec<Point>, Vec<Option<Point>>);

fn size(name: &str, v: Option<f64>) -> Result<f64, String> {
    let v = v.ok_or_else(|| format!("this shape needs {name}"))?;
    if v.is_finite() && (MIN_SIZE..=MAX_SIZE).contains(&v) {
        Ok(v)
    } else {
        Err(format!(
            "{name} must be between {MIN_SIZE} and {MAX_SIZE} millimetres"
        ))
    }
}

fn pt(x: f64, y: f64) -> Point {
    Point::new(x, y)
}

fn straight(points: Vec<Point>) -> Ring {
    (points, Vec::new())
}

/// Build the ring for `kind`.
///
/// # Rules
/// - `kind` is one of rectangle, rounded_rectangle, circle, ellipse, polygon,
///   l_shape or chamfered_rectangle.
/// - The caller checks the ring with the outline rules before storing it.
///
/// # Do not
/// - Do not store anything here; the caller writes the outline.
pub(super) fn build(kind: &str, s: &Sizes) -> Result<Ring, String> {
    match kind {
        "rectangle" => {
            let (w, h) = (size("width", s.width)?, size("height", s.height)?);
            Ok(straight(vec![
                pt(0.0, 0.0),
                pt(w, 0.0),
                pt(w, h),
                pt(0.0, h),
            ]))
        }
        "rounded_rectangle" => {
            let (w, h) = (size("width", s.width)?, size("height", s.height)?);
            let r = size("radius", s.radius)?;
            if r * 2.0 >= w.min(h) {
                return Err("the corner radius must be less than half of the shorter side".into());
            }
            let k = r * (1.0 - std::f64::consts::FRAC_1_SQRT_2);
            let points = vec![
                pt(r, 0.0),
                pt(w - r, 0.0),
                pt(w, r),
                pt(w, h - r),
                pt(w - r, h),
                pt(r, h),
                pt(0.0, h - r),
                pt(0.0, r),
            ];
            let arcs = vec![
                None,
                Some(pt(w - k, k)),
                None,
                Some(pt(w - k, h - k)),
                None,
                Some(pt(k, h - k)),
                None,
                Some(pt(k, k)),
            ];
            Ok((points, arcs))
        }
        "circle" => {
            let d = size("diameter", s.diameter)?;
            let r = d / 2.0;
            let m = r * std::f64::consts::FRAC_1_SQRT_2;
            let points = vec![pt(d, r), pt(r, d), pt(0.0, r), pt(r, 0.0)];
            let arcs = vec![
                Some(pt(r + m, r + m)),
                Some(pt(r - m, r + m)),
                Some(pt(r - m, r - m)),
                Some(pt(r + m, r - m)),
            ];
            Ok((points, arcs))
        }
        "ellipse" => {
            let (w, h) = (size("width", s.width)?, size("height", s.height)?);
            let ring = (0..ELLIPSE_SIDES)
                .map(|i| {
                    let a = std::f64::consts::TAU * i as f64 / ELLIPSE_SIDES as f64;
                    pt(w / 2.0 + w / 2.0 * a.cos(), h / 2.0 + h / 2.0 * a.sin())
                })
                .collect();
            Ok(straight(zero_corner(ring)))
        }
        "polygon" => {
            let d = size("diameter", s.diameter)?;
            let n = s.sides.ok_or("a polygon needs its number of sides")?;
            if !(3..=MAX_SIDES).contains(&n) {
                return Err(format!("a polygon needs 3 to {MAX_SIDES} sides"));
            }
            let ring = (0..n)
                .map(|i| {
                    let a =
                        std::f64::consts::TAU * i as f64 / n as f64 - std::f64::consts::FRAC_PI_2;
                    pt(d / 2.0 * a.cos(), d / 2.0 * a.sin())
                })
                .collect();
            Ok(straight(zero_corner(ring)))
        }
        "l_shape" => {
            let (w, h) = (size("width", s.width)?, size("height", s.height)?);
            let (nw, nh) = (
                size("notch_width", s.notch_width)?,
                size("notch_height", s.notch_height)?,
            );
            if nw >= w || nh >= h {
                return Err("the notch must be smaller than the board".into());
            }
            Ok(straight(vec![
                pt(0.0, 0.0),
                pt(w, 0.0),
                pt(w, h - nh),
                pt(w - nw, h - nh),
                pt(w - nw, h),
                pt(0.0, h),
            ]))
        }
        "chamfered_rectangle" => {
            let (w, h) = (size("width", s.width)?, size("height", s.height)?);
            let c = size("chamfer", s.chamfer)?;
            if c * 2.0 >= w.min(h) {
                return Err("the chamfer must be less than half of the shorter side".into());
            }
            Ok(straight(vec![
                pt(c, 0.0),
                pt(w - c, 0.0),
                pt(w, c),
                pt(w, h - c),
                pt(w - c, h),
                pt(c, h),
                pt(0.0, h - c),
                pt(0.0, c),
            ]))
        }
        other => Err(format!(
            "unknown shape {other}; use rectangle, rounded_rectangle, circle, ellipse, polygon, l_shape or chamfered_rectangle"
        )),
    }
}

/// Slide a ring so its lowest x and y are exactly zero.
fn zero_corner(ring: Vec<Point>) -> Vec<Point> {
    let min_x = ring.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
    let min_y = ring.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
    ring.into_iter()
        .map(|p| pt(p.x - min_x, p.y - min_y))
        .collect()
}
