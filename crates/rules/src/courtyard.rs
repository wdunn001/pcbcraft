//! Courtyard overlap check: two parts whose keep-out rectangles intersect.
//!
//! Context: called once from `drc` in `lib.rs`, next to the fabrication checks.
//! It reads placed footprints and the courtyard graphics of their library
//! footprints; it never touches copper.
//!
//! Local rules:
//! - Courtyard layers are `pcbcraft.courtyard` and `F.CrtYd` (front) and
//!   `B.CrtYd` (back). Footprints carry no side of their own, so the layer
//!   name decides which side a courtyard is on.
//! - Overlap is a separating-axis test on the true rotated polygons, never on
//!   bounding boxes.
//! - A footprint with no courtyard uses the bounding box of
//!   `Footprint::drawing_points` grown by `FALLBACK_MARGIN_MM`, and the
//!   message says the courtyard is estimated.
//! - The margin is a constant until the owner asks for a configurable value.
//!
//! Dependencies: `crates/doc/src/lib.rs` (`Footprint`, `Project`),
//! `crates/library/src/lib.rs` (`Graphic`).
use super::Finding;
use pcbcraft_doc::{Footprint, Id, Layer, Project};
use pcbcraft_geom::Point;
use pcbcraft_library::Graphic;

/// Margin added to the drawn extent when a footprint has no courtyard, in
/// millimetres (the nominal IPC-7351 courtyard excess).
const FALLBACK_MARGIN_MM: f64 = 0.25;
/// Overlap depth below which two courtyards count as touching, in millimetres.
const TOUCH_MM: f64 = 1e-6;

struct Yard {
    footprint: Id,
    label: String,
    side: Layer,
    polygon: Vec<Point>,
    estimated: bool,
}

fn side_of(layer: &str) -> Option<Layer> {
    match layer {
        "pcbcraft.courtyard" | "F.CrtYd" => Some(Layer::Front),
        "B.CrtYd" => Some(Layer::Back),
        _ => None,
    }
}

fn to_world(f: &Footprint, p: Point) -> Point {
    f.position.offset(p.rotate(-f.rotation))
}

/// Convex hull by monotone chain; fewer than three points come back as given.
fn convex_hull(points: &[Point]) -> Vec<Point> {
    let mut sorted = points.to_vec();
    sorted.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    sorted.dedup_by(|a, b| a.distance(*b) < 1e-9);
    if sorted.len() < 3 {
        return sorted;
    }
    let cross =
        |o: Point, a: Point, b: Point| (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x);
    let mut hull: Vec<Point> = vec![];
    for pass in 0..2 {
        let start = hull.len();
        let iter: Box<dyn Iterator<Item = &Point>> = if pass == 0 {
            Box::new(sorted.iter())
        } else {
            Box::new(sorted.iter().rev())
        };
        for &p in iter {
            while hull.len() >= start + 2
                && hull
                    .get(hull.len() - 2)
                    .zip(hull.last())
                    .is_some_and(|(a, b)| cross(*a, *b, p) <= 0.0)
            {
                hull.pop();
            }
            hull.push(p);
        }
        hull.pop();
    }
    hull
}

/// Courtyards of one footprint in world coordinates.
fn yards_of(project: &Project, f: &Footprint) -> Vec<Yard> {
    let label = project
        .component(f.component)
        .map_or_else(|| format!("footprint {}", f.id), |c| c.reference.clone());
    let mut yards = vec![];
    let mut loose: Vec<(Layer, Vec<Point>)> = vec![];
    if let Some(def) = project
        .component(f.component)
        .and_then(|c| c.library_footprint.as_ref())
        .map(|l| &l.definition)
    {
        for g in &def.graphics {
            let layer = match g {
                Graphic::Line { layer, .. }
                | Graphic::Rectangle { layer, .. }
                | Graphic::Circle { layer, .. }
                | Graphic::Arc { layer, .. }
                | Graphic::Polyline { layer, .. }
                | Graphic::Text { layer, .. } => layer,
            };
            let Some(side) = side_of(layer) else { continue };
            if let Graphic::Rectangle { start, end, .. } = g {
                let corners = [
                    *start,
                    Point::new(end.x, start.y),
                    *end,
                    Point::new(start.x, end.y),
                ];
                yards.push(Yard {
                    footprint: f.id,
                    label: label.clone(),
                    side,
                    polygon: corners.iter().map(|c| to_world(f, *c)).collect(),
                    estimated: false,
                });
            } else if !matches!(g, Graphic::Text { .. }) {
                match loose.iter_mut().find(|(s, _)| *s == side) {
                    Some((_, points)) => points.extend(g.path_points()),
                    None => loose.push((side, g.path_points())),
                }
            }
        }
    }
    for (side, points) in loose {
        let hull = convex_hull(&points);
        if hull.len() >= 3 {
            yards.push(Yard {
                footprint: f.id,
                label: label.clone(),
                side,
                polygon: hull.iter().map(|p| to_world(f, *p)).collect(),
                estimated: false,
            });
        }
    }
    if yards.is_empty() {
        let points = f.drawing_points(project);
        let (mut lo, mut hi) = (
            Point::new(f64::MAX, f64::MAX),
            Point::new(f64::MIN, f64::MIN),
        );
        for p in &points {
            lo = Point::new(lo.x.min(p.x), lo.y.min(p.y));
            hi = Point::new(hi.x.max(p.x), hi.y.max(p.y));
        }
        if !points.is_empty() {
            let m = FALLBACK_MARGIN_MM;
            let corners = [
                Point::new(lo.x - m, lo.y - m),
                Point::new(hi.x + m, lo.y - m),
                Point::new(hi.x + m, hi.y + m),
                Point::new(lo.x - m, hi.y + m),
            ];
            yards.push(Yard {
                footprint: f.id,
                label,
                side: Layer::Front,
                polygon: corners.iter().map(|c| to_world(f, *c)).collect(),
                estimated: true,
            });
        }
    }
    yards
}

/// Depth by which two convex polygons overlap, or `None` when they are apart.
///
/// Separating-axis test over the edge normals of both polygons; the depth is
/// the smallest overlap found on any axis.
fn overlap_depth(a: &[Point], b: &[Point]) -> Option<f64> {
    let mut depth = f64::INFINITY;
    for poly in [a, b] {
        for (i, p) in poly.iter().enumerate() {
            let q = poly.get((i + 1) % poly.len()).copied()?;
            let (dx, dy) = (q.x - p.x, q.y - p.y);
            let len = dx.hypot(dy);
            if len < 1e-12 {
                continue;
            }
            let axis = Point::new(-dy / len, dx / len);
            let project = |poly: &[Point]| {
                poly.iter().fold((f64::MAX, f64::MIN), |(lo, hi), v| {
                    let d = v.x * axis.x + v.y * axis.y;
                    (lo.min(d), hi.max(d))
                })
            };
            let (a_lo, a_hi) = project(a);
            let (b_lo, b_hi) = project(b);
            let overlap = a_hi.min(b_hi) - a_lo.max(b_lo);
            if overlap <= TOUCH_MM {
                return None;
            }
            depth = depth.min(overlap);
        }
    }
    depth.is_finite().then_some(depth)
}

/// One `courtyard_overlap` finding per pair of footprints whose courtyards
/// intersect on the same side.
///
/// # Rules
/// Severity is `project.rule_severity("courtyard_overlap", "error")`. The
/// footprint ids are the finding's objects, lower id first.
///
/// # Dependencies
/// `crates/doc/src/lib.rs` (`Footprint::drawing_points`).
///
/// # Do not
/// Do not compare bounding boxes of rotated courtyards.
pub(crate) fn courtyard_findings(project: &Project) -> Vec<Finding> {
    let yards: Vec<Yard> = project
        .board
        .footprints
        .iter()
        .flat_map(|f| yards_of(project, f))
        .collect();
    let mut result = vec![];
    let mut seen: Vec<(Id, Id)> = vec![];
    for (i, a) in yards.iter().enumerate() {
        for b in yards.iter().skip(i + 1) {
            if a.footprint == b.footprint || a.side != b.side {
                continue;
            }
            let key = (a.footprint.min(b.footprint), a.footprint.max(b.footprint));
            if seen.contains(&key) {
                continue;
            }
            let Some(depth) = overlap_depth(&a.polygon, &b.polygon) else {
                continue;
            };
            seen.push(key);
            let mut message = format!(
                "{} and {} courtyards overlap by {depth:.3} mm",
                a.label, b.label
            );
            for y in [a, b].iter().filter(|y| y.estimated) {
                message.push_str(&format!(
                    "; the courtyard of {} is estimated from its pads and drawing plus {FALLBACK_MARGIN_MM} mm",
                    y.label
                ));
            }
            result.push(Finding {
                code: "courtyard_overlap",
                severity: project.rule_severity("courtyard_overlap", "error"),
                message,
                objects: vec![key.0, key.1],
            });
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use pcbcraft_doc::Footprint;
    use pcbcraft_doc::{LibraryFootprint, Pad, component_template};
    use pcbcraft_library::{FootprintDefinition, Graphic};

    /// A part at `at`, turned by `rotation`, with a courtyard rectangle of
    /// `half` (x, y) on `layer`, or with no courtyard when `half` is none.
    fn part(
        p: &mut Project,
        id: Id,
        at: Point,
        rotation: f64,
        half: Option<(f64, f64)>,
        layer: &str,
    ) {
        if let Some(mut c) = component_template("resistor", id, format!("R{id}"), at) {
            if let Some((hx, hy)) = half {
                c.library_footprint = Some(LibraryFootprint {
                    library: "test".into(),
                    definition: FootprintDefinition {
                        name: "T".into(),
                        description: String::new(),
                        keywords: String::new(),
                        pads: vec![],
                        graphics: vec![Graphic::Rectangle {
                            start: Point::new(-hx, -hy),
                            end: Point::new(hx, hy),
                            width: 0.05,
                            fill: String::new(),
                            layer: layer.into(),
                        }],
                        model: None,
                        rules: None,
                        raw_source: String::new(),
                        unsupported: vec![],
                    },
                });
            }
            p.components.push(c);
        }
        p.board.footprints.push(Footprint {
            id: id + 10,
            component: id,
            position: at,
            rotation,
            locked: None,
            side: None,
            pads: vec![Pad {
                number: "1".into(),
                offset: Point::default(),
                diameter: 1.0,
                net: None,
                castellated: None,
                geometry: None,
            }],
        });
    }
    fn found(p: &Project) -> Vec<Finding> {
        drc(p)
            .into_iter()
            .filter(|f| f.code == "courtyard_overlap")
            .collect()
    }

    #[test]
    fn overlapping_courtyards_name_both_footprints_with_the_depth() {
        let mut p = Project::default();
        part(
            &mut p,
            2,
            Point::new(0.0, 0.0),
            0.0,
            Some((2.0, 1.0)),
            "F.CrtYd",
        );
        part(
            &mut p,
            3,
            Point::new(3.5, 0.0),
            0.0,
            Some((2.0, 1.0)),
            "F.CrtYd",
        );
        let f = found(&p);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].objects, vec![12, 13]);
        assert_eq!(f[0].message, "R2 and R3 courtyards overlap by 0.500 mm");
        assert_eq!(f[0].severity, "error");
    }
    #[test]
    fn courtyards_on_opposite_sides_do_not_collide() {
        let mut p = Project::default();
        part(
            &mut p,
            2,
            Point::new(0.0, 0.0),
            0.0,
            Some((2.0, 1.0)),
            "F.CrtYd",
        );
        part(
            &mut p,
            3,
            Point::new(1.0, 0.0),
            0.0,
            Some((2.0, 1.0)),
            "B.CrtYd",
        );
        assert!(found(&p).is_empty());
    }
    #[test]
    fn rotated_courtyards_overlap_by_their_true_rectangles() {
        // Part 2: x -2..2, y -0.5..0.5. Part 3 is a 3 by 0.4 strip turned 45
        // degrees, running down to the right. Their bounding boxes overlap in both placements; only the
        // second placement lays the strip across part 2's corner.
        let mut p = Project::default();
        part(
            &mut p,
            2,
            Point::new(0.0, 0.0),
            0.0,
            Some((2.0, 0.5)),
            "F.CrtYd",
        );
        part(
            &mut p,
            3,
            Point::new(2.8, 1.2),
            45.0,
            Some((1.5, 0.2)),
            "F.CrtYd",
        );
        assert!(found(&p).is_empty(), "boxes overlap, shapes do not");
        p.board.footprints[1].position = Point::new(1.5, 1.0);
        let f = found(&p);
        assert_eq!(f.len(), 1, "the strip now reaches the corner");
        assert_eq!(f[0].objects, vec![12, 13]);
    }
    #[test]
    fn template_footprint_without_courtyard_uses_the_fallback_margin() {
        // No courtyard: the drawing spans x -3.75..3.75, so the estimate is
        // 4.0 mm either side. Parts 7.9 mm apart overlap by 0.1 mm; 8.1 do not.
        let mut p = Project::default();
        part(&mut p, 2, Point::new(0.0, 0.0), 0.0, None, "");
        part(&mut p, 3, Point::new(7.9, 0.0), 0.0, None, "");
        let f = found(&p);
        assert_eq!(f.len(), 1);
        assert!(f[0].message.contains("overlap by 0.100 mm"));
        assert!(f[0].message.contains("estimated"));
        p.board.footprints[1].position = Point::new(8.1, 0.0);
        assert!(found(&p).is_empty());
    }
    #[test]
    fn touching_courtyards_are_not_an_overlap() {
        let mut p = Project::default();
        part(
            &mut p,
            2,
            Point::new(0.0, 0.0),
            0.0,
            Some((2.0, 1.0)),
            "F.CrtYd",
        );
        part(
            &mut p,
            3,
            Point::new(4.0, 0.0),
            0.0,
            Some((2.0, 1.0)),
            "F.CrtYd",
        );
        assert!(found(&p).is_empty());
    }
    #[test]
    fn courtyard_overlap_follows_severity_and_exclusion_settings() {
        let mut p = Project::default();
        part(
            &mut p,
            2,
            Point::new(0.0, 0.0),
            0.0,
            Some((2.0, 1.0)),
            "F.CrtYd",
        );
        part(
            &mut p,
            3,
            Point::new(3.0, 0.0),
            0.0,
            Some((2.0, 1.0)),
            "F.CrtYd",
        );
        p.rule_severity
            .insert("courtyard_overlap".into(), "warning".into());
        assert_eq!(found(&p)[0].severity, "warning");
        p.rule_severity
            .insert("courtyard_overlap".into(), "off".into());
        assert!(found(&p).is_empty());
    }
}
