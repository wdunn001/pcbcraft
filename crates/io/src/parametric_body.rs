//! Parametric package bodies from footprint names, stored descriptions, and pad
//! patterns. Footprint-local millimetres, z up, seating plane at z = 0.

use pcbcraft_doc::{Component, Footprint, Pad};
use pcbcraft_geom::Point;
use pcbcraft_library::listing::package_key;
use serde_json::Value;

const EPS: f64 = 1e-9;
const POST_HEIGHT: f64 = 6.0;

/// One extruded solid in footprint-local coordinates.
#[derive(Clone, Debug)]
pub(crate) struct Solid {
    pub ring: Vec<Point>,
    pub z0: f64,
    pub z1: f64,
}

/// Parametric body for one footprint before board placement.
#[derive(Clone, Debug)]
pub(crate) struct ParametricBody {
    pub family: &'static str,
    pub ring: Vec<Point>,
    pub height: f64,
    pub standoff: f64,
    pub leads: Vec<Solid>,
    pub pins: Vec<Solid>,
    pub pin1_chamfer: Option<Solid>,
    pub polarity: Option<Solid>,
}

impl ParametricBody {
    fn plain(family: &'static str, ring: Vec<Point>, height: f64) -> Self {
        Self {
            family,
            ring,
            height,
            standoff: 0.0,
            leads: vec![],
            pins: vec![],
            pin1_chamfer: None,
            polarity: None,
        }
    }
}

/// Resolve a parametric body. Priority: stored `body` field on the component,
/// footprint name grammar, then pad-pattern inference.
pub(crate) fn resolve(fp: &Footprint, component: Option<&Component>) -> Option<ParametricBody> {
    let name = component
        .and_then(|c| c.library_footprint.as_ref())
        .map(|l| l.definition.name.as_str());
    let mut body = if let Some(over) = component.and_then(stored_body) {
        infer_from_pads(fp, component).map(|mut b| {
            if let Some(f) = over.family {
                b.family = f;
            }
            if let Some(h) = over.height {
                b.height = h;
            }
            b
        })?
    } else {
        infer_from_pads(fp, component)?
    };
    if let Some(hint) = name.and_then(family_from_footprint_name) {
        body = apply_name_family(body, hint);
    }
    finish_body(body, fp, component)
}

#[derive(Clone, Copy)]
struct StoredBody {
    family: Option<&'static str>,
    height: Option<f64>,
}

fn stored_body(c: &Component) -> Option<StoredBody> {
    let text = c.field("body")?;
    let v: Value = serde_json::from_str(text.trim()).ok()?;
    let family = v
        .get("family")
        .and_then(Value::as_str)
        .and_then(parse_family_token);
    let height = v
        .get("height")
        .or_else(|| v.get("height_mm"))
        .and_then(|h| h.as_f64())
        .filter(|h| h.is_finite() && (0.05..=100.0).contains(h));
    Some(StoredBody { family, height })
}

fn parse_family_token(s: &str) -> Option<&'static str> {
    match s.trim().to_ascii_lowercase().as_str() {
        "chip" | "chip_2terminal" | "r" | "c" | "l" => Some("chip"),
        "sot" => Some("sot"),
        "soic" | "ssop" | "tssop" | "msop" | "sop" => Some("soic"),
        "qfp" | "lqfp" | "tqfp" => Some("qfp"),
        "qfn" | "dfn" | "son" => Some("qfn"),
        "dip" => Some("dip"),
        "header" | "header_single" | "header_double" => Some("header"),
        "bga" => Some("bga"),
        "axial" => Some("axial"),
        "radial" => Some("radial"),
        _ => None,
    }
}

fn family_from_footprint_name(name: &str) -> Option<&'static str> {
    let lower = name.to_ascii_lowercase();
    if lower.contains("header") || lower.contains("pinheader") || lower.contains("pinsocket") {
        return Some("header");
    }
    if lower.starts_with('r') && lower.contains('_') && lower.contains("metric") {
        return Some("chip");
    }
    if lower.starts_with('c') && lower.contains('_') && lower.contains("metric") {
        return Some("chip");
    }
    if lower.starts_with("led_") {
        return Some("chip");
    }
    let key = package_key(name).key;
    if key.is_empty() {
        return None;
    }
    if key.starts_with("chip-") {
        return Some("chip");
    }
    let head = key.split('-').next()?;
    parse_family_token(head)
}

fn apply_name_family(mut body: ParametricBody, hint: &'static str) -> ParametricBody {
    let override_generic = body.family == "generic";
    if override_generic || body.family != hint {
        body.family = hint;
        body.height = default_height(hint, &body.ring);
    }
    body
}

fn default_height(family: &str, ring: &[Point]) -> f64 {
    match family {
        "chip" => {
            let (lo, hi) = ring_bounds(ring);
            chip_height((hi.x - lo.x).max(hi.y - lo.y))
        }
        "sot" => 1.1,
        "soic" => 1.6,
        "qfp" => 1.4,
        "qfn" => 0.8,
        "dip" => 3.3,
        "header" => 2.5,
        "bga" => 1.0,
        "axial" => 3.0,
        "radial" => 6.0,
        _ => 1.0,
    }
}

fn finish_body(
    mut body: ParametricBody,
    fp: &Footprint,
    component: Option<&Component>,
) -> Option<ParametricBody> {
    let pads: Vec<PadBox> = fp.pads.iter().filter_map(pad_box).collect();
    body.leads = leads_for_family(body.family, &pads, body.height);
    if body.family == "header" && body.pins.is_empty() {
        body.pins = pads
            .iter()
            .map(|p| Solid {
                ring: rect_ring(p.centre.x, p.centre.y, 0.64, 0.64),
                z0: body.height,
                z1: body.height + POST_HEIGHT,
            })
            .collect();
    }
    if let Some(pin1) = pin1_location(fp, &pads) {
        body.pin1_chamfer = pin1_chamfer_solid(&body.ring, pin1, body.height);
        body.ring = chamfer_ring_corner(&body.ring, pin1);
    }
    if body.family == "chip" && pads.len() == 2 {
        body.polarity = polarity_mark(&pads, component, body.height);
    }
    Some(body)
}

fn pin1_location(fp: &Footprint, pads: &[PadBox]) -> Option<Point> {
    if let Some(pad) = fp.pads.iter().find(|p| p.number == "1") {
        return pad_box(pad).map(|b| b.centre);
    }
    pads.iter()
        .min_by(|a, b| {
            a.centre
                .x
                .total_cmp(&b.centre.x)
                .then(a.centre.y.total_cmp(&b.centre.y))
        })
        .map(|p| p.centre)
}

fn chamfer_ring_corner(ring: &[Point], pin1: Point) -> Vec<Point> {
    if ring.len() < 4 {
        return ring.to_vec();
    }
    let mut best = 0usize;
    let mut best_d = f64::MAX;
    for (i, p) in ring.iter().enumerate() {
        let d = p.distance(pin1);
        if d < best_d {
            best_d = d;
            best = i;
        }
    }
    let n = ring.len();
    let prev = ring[(best + n - 1) % n];
    let corner = ring[best];
    let next = ring[(best + 1) % n];
    let cut = 0.35;
    let p1 = Point::new(
        corner.x + (prev.x - corner.x).signum() * cut.min(prev.distance(corner)),
        corner.y + (prev.y - corner.y).signum() * cut.min(prev.distance(corner)),
    );
    let p2 = Point::new(
        corner.x + (next.x - corner.x).signum() * cut.min(next.distance(corner)),
        corner.y + (next.y - corner.y).signum() * cut.min(next.distance(corner)),
    );
    let mut out = ring.to_vec();
    out.splice(best..=best, [p1, p2]);
    out
}

fn pin1_chamfer_solid(ring: &[Point], pin1: Point, height: f64) -> Option<Solid> {
    let (lo, hi) = ring_bounds(ring);
    let size = 0.25;
    let cx = if pin1.x <= (lo.x + hi.x) / 2.0 {
        lo.x + size
    } else {
        hi.x - size
    };
    let cy = if pin1.y >= (lo.y + hi.y) / 2.0 {
        hi.y - size
    } else {
        lo.y + size
    };
    Some(Solid {
        ring: rect_ring(cx, cy, size * 2.0, size * 2.0),
        z0: height * 0.85,
        z1: height,
    })
}

fn polarity_mark(
    pads: &[PadBox],
    component: Option<&Component>,
    height: f64,
) -> Option<Solid> {
    let kind = component.map(|c| c.kind.to_ascii_lowercase()).unwrap_or_default();
    let mark_second = kind.contains("diode") || kind.contains("led");
    let pad = if mark_second && pads.len() >= 2 {
        pads.iter().max_by(|a, b| a.centre.x.total_cmp(&b.centre.x))?
    } else {
        pads.first()?
    };
    Some(Solid {
        ring: circle_ring(pad.centre, 0.12),
        z0: height,
        z1: height + 0.08,
    })
}

fn leads_for_family(family: &str, pads: &[PadBox], body_height: f64) -> Vec<Solid> {
    let top = match family {
        "chip" => body_height,
        "qfp" | "soic" | "sot" | "qfn" => (body_height * 0.4).max(0.12),
        "dip" => 0.45,
        "header" => 0.35,
        "bga" => 0.05,
        _ => (body_height * 0.25).max(0.1),
    };
    pads
        .iter()
        .filter(|p| !thermal_pad(p, pads))
        .map(|p| {
            let w = p.width().max(0.12);
            let h = p.height().max(0.12);
            Solid {
                ring: rect_ring(p.centre.x, p.centre.y, w * 1.02, h * 1.02),
                z0: 0.0,
                z1: top,
            }
        })
        .collect()
}

fn thermal_pad(p: &PadBox, all: &[PadBox]) -> bool {
    all.len() >= 5
        && p.width() * p.height() >= 2.0 * median_pad_area(all)
        && all
            .iter()
            .filter(|q| q.centre.distance(p.centre) < 0.2)
            .count() == 1
}

fn median_pad_area(pads: &[PadBox]) -> f64 {
    let mut areas: Vec<f64> = pads.iter().map(|p| p.width() * p.height()).collect();
    areas.sort_by(f64::total_cmp);
    areas.get(pads.len() / 2).copied().unwrap_or(0.0)
}

// ------------------------------------------------------------------- pads

struct PadBox {
    centre: Point,
    lo: Point,
    hi: Point,
    through: bool,
    round: bool,
    number: String,
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
        number: pad.number.clone(),
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

fn circle_ring(centre: Point, radius: f64) -> Vec<Point> {
    let steps = 36;
    let mut points = Vec::with_capacity(steps);
    for i in 0..steps {
        let angle = std::f64::consts::TAU * (i as f64) / (steps as f64);
        points.push(Point::new(
            centre.x + radius * angle.cos(),
            centre.y + radius * angle.sin(),
        ));
    }
    points
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

fn smallest_gap(values: &[f64]) -> Option<f64> {
    values
        .windows(2)
        .map(|w| w[1] - w[0])
        .filter(|g| *g > 0.05)
        .fold(None, |best: Option<f64>, g| Some(best.map_or(g, |b| b.min(g))))
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
    let Some(c) = component else {
        return false;
    };
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

fn infer_from_pads(fp: &Footprint, component: Option<&Component>) -> Option<ParametricBody> {
    let pads: Vec<PadBox> = fp.pads.iter().filter_map(pad_box).collect();
    if pads.is_empty() {
        return None;
    }
    let n = pads.len();
    let all_points: Vec<Point> = pads.iter().flat_map(|p| [p.lo, p.hi]).collect();
    let (lo, hi) = ring_bounds(&all_points);
    let through = pads.iter().filter(|p| p.through).count() * 2 >= n;
    let generic = || {
        ParametricBody::plain(
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
        return through_body(component, &pads, &xc, &yc).or_else(generic);
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
        return Some(ParametricBody::plain(
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
            return Some(ParametricBody::plain(
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
    if n >= 8 && xc.len() >= 2 && yc.len() >= 2 {
        let (x_lo, x_hi) = (xc[0], xc[xc.len() - 1]);
        let (y_lo, y_hi) = (yc[0], yc[yc.len() - 1]);
        let count = |f: &dyn Fn(&PadBox) -> bool| signal.iter().filter(|p| f(p)).count();
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
                return Some(ParametricBody::plain("qfp", rect_ring(cx, cy, w, h), 1.4));
            }
            return Some(ParametricBody::plain(
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
            return Some(ParametricBody::plain(
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
) -> Option<ParametricBody> {
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
            return Some(ParametricBody::plain("axial", ring, diameter));
        }
        let diameter = (2.0 * c).max(4.0);
        return Some(ParametricBody::plain(
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
        return Some(ParametricBody::plain("dip", ring, 3.3));
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
        let pin_rings = centres
            .iter()
            .map(|c| rect_ring(c.x, c.y, 0.64, 0.64))
            .collect::<Vec<_>>();
        let pins = pin_rings
            .iter()
            .map(|r| Solid {
                ring: r.clone(),
                z0: 2.5,
                z1: 2.5 + POST_HEIGHT,
            })
            .collect();
        return Some(ParametricBody {
            family: "header",
            ring,
            height: 2.5,
            standoff: 0.0,
            leads: vec![],
            pins,
            pin1_chamfer: None,
            polarity: None,
        });
    }
    None
}

/// Axis-aligned bounds of all solids in footprint-local space.
pub(crate) fn local_bounds(body: &ParametricBody) -> (Point, Point, f64, f64) {
    let mut lo = Point::new(f64::MAX, f64::MAX);
    let mut hi = Point::new(f64::MIN, f64::MIN);
    let mut z_lo = f64::MAX;
    let mut z_hi = f64::MIN;
    let mut eat = |ring: &[Point], z0: f64, z1: f64| {
        let (a, b) = ring_bounds(ring);
        lo.x = lo.x.min(a.x);
        lo.y = lo.y.min(a.y);
        hi.x = hi.x.max(b.x);
        hi.y = hi.y.max(b.y);
        z_lo = z_lo.min(z0);
        z_hi = z_hi.max(z1);
    };
    eat(&body.ring, body.standoff, body.standoff + body.height);
    for s in body
        .leads
        .iter()
        .chain(&body.pins)
        .chain(&body.pin1_chamfer)
        .chain(&body.polarity)
    {
        eat(&s.ring, s.z0, s.z1);
    }
    (lo, hi, z_lo, z_hi)
}

pub(crate) fn pad_covered(pad: &PadBox, all: &[PadBox], body: &ParametricBody) -> bool {
    if thermal_pad(pad, all) {
        let (blo, bhi) = ring_bounds(&body.ring);
        return pad.lo.x >= blo.x - EPS
            && pad.hi.x <= bhi.x + EPS
            && pad.lo.y >= blo.y - EPS
            && pad.hi.y <= bhi.y + EPS;
    }
    let centre = pad.centre;
    body.leads.iter().any(|lead| {
        let (lo, hi) = ring_bounds(&lead.ring);
        centre.x >= lo.x - EPS
            && centre.x <= hi.x + EPS
            && centre.y >= lo.y - EPS
            && centre.y <= hi.y + EPS
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pcbcraft_doc::{Component, Footprint, Pad};
    use pcbcraft_library::{
        FootprintDefinition, FootprintPad, LandPatternDensity, LandPatternParams,
        generate_footprint,
    };

    fn pad_at(number: &str, x: f64, y: f64, w: f64, h: f64, shape: &str) -> Pad {
        Pad {
            number: number.into(),
            offset: Point::new(x, y),
            diameter: w.min(h),
            net: None,
            castellated: None,
            geometry: Some(FootprintPad {
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

    fn body_of(pads: Vec<Pad>) -> Result<ParametricBody, String> {
        resolve(&footprint(pads), None).ok_or_else(|| "no body".to_string())
    }

    fn size_of(body: &ParametricBody) -> (f64, f64) {
        let (lo, hi) = ring_bounds(&body.ring);
        (hi.x - lo.x, hi.y - lo.y)
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    fn nominal_params() -> LandPatternParams {
        LandPatternParams {
            name: "TestFootprint".into(),
            description: String::new(),
            keywords: String::new(),
            pitch: 1.27,
            pin_count: 8,
            body_length: 2.0,
            body_width: 1.25,
            row_spacing: 0.0,
            lead_width: 0.0,
            lead_diameter: 0.0,
            grid_rows: 0,
            grid_cols: 0,
            thermal_size: 0.0,
        }
    }

    fn courtyard_half(def: &FootprintDefinition) -> Option<(f64, f64)> {
        for g in &def.graphics {
            if let pcbcraft_library::Graphic::Rectangle {
                start, end, layer, ..
            } = g
                && layer == "pcbcraft.courtyard"
            {
                return Some((
                    start.x.abs().max(end.x.abs()),
                    start.y.abs().max(end.y.abs()),
                ));
            }
        }
        None
    }

    fn pads_from_def(def: &FootprintDefinition) -> Vec<Pad> {
        def.pads
            .iter()
            .map(|g| Pad {
                number: g.number.clone(),
                offset: g.position,
                diameter: g.size.x.min(g.size.y),
                net: None,
                castellated: None,
                geometry: Some(g.clone()),
            })
            .collect()
    }

    #[test]
    fn package_families_come_from_pad_patterns() -> Result<(), String> {
        let chip = body_of(vec![
            pad_at("1", -0.9, 0.0, 1.025, 1.4, "roundrect"),
            pad_at("2", 0.9, 0.0, 1.025, 1.4, "roundrect"),
        ])?;
        assert_eq!(chip.family, "chip");
        let (l, w) = size_of(&chip);
        assert!(close(l, 1.95) && close(w, 1.26), "{l} {w}");
        assert_eq!(chip.leads.len(), 2);
        let mut soic = vec![];
        for i in 0..4 {
            let x = -1.905 + 1.27 * i as f64;
            soic.push(pad_at(&i.to_string(), x, 2.7, 0.6, 1.55, "roundrect"));
            soic.push(pad_at(&i.to_string(), x, -2.7, 0.6, 1.55, "roundrect"));
        }
        let soic = body_of(soic)?;
        assert_eq!(soic.family, "soic");
        let sot = body_of(vec![
            pad_at("1", -1.1375, -0.95, 1.0, 0.7, "roundrect"),
            pad_at("2", -1.1375, 0.95, 1.0, 0.7, "roundrect"),
            pad_at("3", 1.1375, 0.0, 1.0, 0.7, "roundrect"),
        ])?;
        assert_eq!(sot.family, "sot");
        let mut qfp = vec![];
        for i in 0..8 {
            let t = -3.5 + i as f64;
            qfp.push(pad_at("1", -4.25, t, 1.6, 0.3, "roundrect"));
            qfp.push(pad_at("2", 4.25, t, 1.6, 0.3, "roundrect"));
            qfp.push(pad_at("3", t, -4.25, 0.3, 1.6, "roundrect"));
            qfp.push(pad_at("4", t, 4.25, 0.3, 1.6, "roundrect"));
        }
        assert_eq!(body_of(qfp)?.family, "qfp");
        let mut dip = vec![];
        for i in 0..4 {
            let x = -3.81 + 2.54 * i as f64;
            dip.push(through_pad("1", x, -3.81, 1.6, 0.8));
            dip.push(through_pad("2", x, 3.81, 1.6, 0.8));
        }
        assert_eq!(body_of(dip)?.family, "dip");
        let header = body_of(
            (0..4)
                .map(|i| through_pad("1", 2.54 * i as f64, 0.0, 1.7, 1.0))
                .collect(),
        )?;
        assert_eq!(header.family, "header");
        assert_eq!(header.pins.len(), 4);
        Ok(())
    }

    #[test]
    fn footprint_name_overrides_generic_family() -> Result<(), String> {
        let mut gen = nominal_params();
        gen.name = "SOIC-8_3.9x4.9mm_P1.27mm".into();
        gen.body_width = 3.9;
        let def = generate_footprint("soic", LandPatternDensity::Nominal, &gen)?;
        let pads = pads_from_def(&def);
        let mut comp = Component {
            id: 1,
            reference: "U1".into(),
            value: String::new(),
            kind: "ic".into(),
            position: Point::new(0.0, 0.0),
            rotation: 0.0,
            pins: vec![],
            units: vec![],
            library_symbol: None,
            library_footprint: Some(pcbcraft_doc::LibraryFootprint {
                library: "g".into(),
                definition: def.clone(),
            }),
            fields: vec![],
            sheet: 0,
        };
        let body = resolve(&footprint(pads), Some(&comp)).ok_or("missing")?;
        assert_eq!(body.family, "soic");
        comp.library_footprint.as_mut().unwrap().definition.name = "Mystery".into();
        let body2 = resolve(
            &footprint(vec![
                pad_at("1", 0.0, 0.0, 1.0, 1.0, "rect"),
                pad_at("2", 5.0, 0.0, 1.0, 1.0, "rect"),
            ]),
            Some(&comp),
        )
        .ok_or("missing")?;
        assert_eq!(body2.family, "generic");
        let _ = body;
        Ok(())
    }

    #[test]
    fn stored_body_field_sets_family_and_height() -> Result<(), String> {
        let pads = vec![
            pad_at("1", -0.9, 0.0, 1.025, 1.4, "roundrect"),
            pad_at("2", 0.9, 0.0, 1.025, 1.4, "roundrect"),
        ];
        let comp = Component {
            id: 1,
            reference: "R1".into(),
            value: String::new(),
            kind: "resistor".into(),
            position: Point::new(0.0, 0.0),
            rotation: 0.0,
            pins: vec![],
            units: vec![],
            library_symbol: None,
            library_footprint: None,
            fields: vec![pcbcraft_doc::ComponentField {
                key: "body".into(),
                value: r#"{"family":"chip","height":0.9}"#.into(),
            }],
            sheet: 0,
        };
        let body = resolve(&footprint(pads), Some(&comp)).ok_or("missing")?;
        assert_eq!(body.family, "chip");
        assert!(close(body.height, 0.9));
        Ok(())
    }

    #[test]
    fn generated_footprints_satisfy_body_properties() -> Result<(), String> {
        let families: [(&str, fn() -> LandPatternParams); 7] = [
            ("chip_2terminal", || {
                let mut p = nominal_params();
                p.name = "R_0805".into();
                p.body_length = 2.0;
                p.body_width = 1.25;
                p.pin_count = 2;
                p
            }),
            ("sot", || {
                let mut p = nominal_params();
                p.name = "SOT-23-5".into();
                p.pitch = 0.95;
                p.pin_count = 5;
                p.body_length = 2.9;
                p.body_width = 1.6;
                p
            }),
            ("soic", || {
                let mut p = nominal_params();
                p.name = "SOIC-8".into();
                p.body_width = 3.9;
                p
            }),
            ("qfp", || {
                let mut p = nominal_params();
                p.name = "QFP-48".into();
                p.pitch = 0.5;
                p.pin_count = 48;
                p.body_length = 7.0;
                p.body_width = 7.0;
                p
            }),
            ("qfn", || {
                let mut p = nominal_params();
                p.name = "QFN-32".into();
                p.pitch = 0.5;
                p.pin_count = 32;
                p.body_length = 5.0;
                p.body_width = 5.0;
                p.thermal_size = 2.5;
                p
            }),
            ("dip", || {
                let mut p = nominal_params();
                p.name = "DIP-8".into();
                p.pitch = 2.54;
                p
            }),
            ("header_single", || {
                let mut p = nominal_params();
                p.name = "PinHeader_1x04".into();
                p.pitch = 2.54;
                p.pin_count = 4;
                p
            }),
        ];
        for (family, params) in families {
            let def = generate_footprint(family, LandPatternDensity::Nominal, &params())?;
            let pads: Vec<PadBox> = pads_from_def(&def)
                .iter()
                .filter_map(pad_box)
                .collect();
            let body = resolve(&footprint(pads_from_def(&def)), None).ok_or("no body")?;
            let (blo, bhi, z0, z1) = local_bounds(&body);
            assert!(
                z0 >= -EPS,
                "{family}: bottom below seating plane {z0}"
            );
            assert!(z0 <= body.standoff + EPS, "{family}: unexpected standoff");
            if let Some((hx, hy)) = courtyard_half(&def) {
                assert!(
                    blo.x >= -hx - 0.01 && bhi.x <= hx + 0.01 && blo.y >= -hy - 0.01 && bhi.y <= hy + 0.01,
                    "{family}: body {blo:?} {bhi:?} outside courtyard {hx} {hy}"
                );
            }
            for pad in &pads {
                assert!(
                    pad_covered(pad, &pads, &body),
                    "{family}: pad {} uncovered",
                    pad.number
                );
            }
            assert!(z1 > z0, "{family}: flat body");
            let _ = body;
        }
        Ok(())
    }
}
