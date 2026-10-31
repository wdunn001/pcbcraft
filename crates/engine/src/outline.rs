//! Board outline arc commands: set an arc by midpoint or radius, straighten
//! it, and move a vertex while its neighbouring arcs keep their radius.
//!
//! Each command edits copies of the outline and arc list, checks them with the
//! document rules, and only then writes them back, so a refused edit leaves
//! the design untouched. `Session::execute` makes each call one undo step.
//! Rules: the board origin stays put unless a bulge crosses it; width and
//! height follow the outline extent including arc bulges.
//!
//! # Dependencies
//! - crates/doc/src/outline.rs
//! - crates/geom/src/lib.rs#arc_through
use super::*;
use pcbcraft_doc::{
    flatten_outline, outline_area, outline_extent, outline_signed_area, segment_arc,
    validate_outline_arcs,
};

pub(super) fn commands() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "board.outline_set_arc",
            description: "Bend one board outline segment into a circular arc, given by a point the arc passes through, or by a radius and a side (convex bulges outward, concave inward); the outline stays closed and a crossing or half-turn arc is refused",
            input_schema: schema(
                json!({"segment":{"type":"integer","minimum":0,"maximum":63,"description":"segment i runs from vertex i to vertex i+1; the last closes to vertex 0"},"mid":point_schema(),"radius":{"type":"number","exclusiveMinimum":0,"maximum":10000},"side":{"type":"string","enum":["convex","concave"]}}),
                &["segment"],
            ),
        },
        CommandSpec {
            id: "board.outline_straighten",
            description: "Turn one arced board outline segment back into a straight line",
            input_schema: schema(
                json!({"segment":{"type":"integer","minimum":0,"maximum":63}}),
                &["segment"],
            ),
        },
        CommandSpec {
            id: "board.outline_set_vertex",
            description: "Move one board outline vertex; arcs on the two segments touching it keep their radius when the new chord still fits, and are straightened (and reported) when it does not",
            input_schema: schema(
                json!({"index":{"type":"integer","minimum":0,"maximum":63},"at":point_schema()}),
                &["index", "at"],
            ),
        },
    ]
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SetArc {
    segment: usize,
    mid: Option<Point>,
    radius: Option<f64>,
    side: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Segment {
    segment: usize,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SetVertex {
    index: usize,
    at: Point,
}

/// Dispatch for the outline commands; `None` when `command` is not one.
///
/// # Rules
/// - Every edit goes through `commit`, which validates before writing.
///
/// # Do not
/// - Do not write `p.board.outline` or `outline_arcs` anywhere else in this file.
pub(super) fn run(
    p: &mut Project,
    _next: &mut Id,
    command: &str,
    params: &Value,
) -> Option<Result<Value, Error>> {
    if !commands().iter().any(|c| c.id == command) {
        return None;
    }
    Some(match command {
        "board.outline_set_arc" => set_arc(p, params),
        "board.outline_straighten" => straighten(p, params),
        _ => set_vertex(p, params),
    })
}

fn current(p: &Project) -> Result<(Vec<Point>, Vec<Option<Point>>), Error> {
    let points = p
        .board
        .outline
        .clone()
        .ok_or_else(|| Error::Invalid("the board has no traced outline".into()))?;
    let mut arcs = p.board.outline_arcs.clone();
    arcs.resize(points.len(), None);
    Ok((points, arcs))
}

/// Midpoint of the arc over chord `a`-`b` with the given radius, bulging to
/// the left of a to b when `left`, else to the right.
fn mid_for_radius(a: Point, b: Point, radius: f64, left: bool) -> Result<Point, Error> {
    let chord = a.distance(b);
    if chord < 1e-6 || radius * 2.0 <= chord * (1.0 + 1e-9) {
        return Err(Error::Invalid(format!(
            "a radius of {radius} mm cannot span a {chord:.3} mm segment; use at least half the segment length plus a little"
        )));
    }
    let sagitta = radius - (radius * radius - chord * chord / 4.0).sqrt();
    let sign = if left { 1.0 } else { -1.0 };
    let (nx, ny) = (-(b.y - a.y) / chord, (b.x - a.x) / chord);
    Ok(Point::new(
        (a.x + b.x) / 2.0 + nx * sagitta * sign,
        (a.y + b.y) / 2.0 + ny * sagitta * sign,
    ))
}

fn is_left(a: Point, b: Point, m: Point) -> bool {
    (b.x - a.x) * (m.y - a.y) - (b.y - a.y) * (m.x - a.x) > 0.0
}

/// Check the edited outline and write it back, fitting width and height.
///
/// # Rules
/// - Vertices stay finite, distinct, in the first quadrant, with real area.
/// - A bulge that crosses the origin shifts the outline; otherwise the origin stays.
fn commit(
    p: &mut Project,
    mut points: Vec<Point>,
    mut arcs: Vec<Option<Point>>,
) -> Result<Value, Error> {
    if points.len() < 3 || points.len() > 64 {
        return Err(Error::Invalid("an outline needs 3 to 64 vertices".into()));
    }
    let distinct: std::collections::BTreeSet<(u64, u64)> = points
        .iter()
        .map(|q| (q.x.to_bits(), q.y.to_bits()))
        .collect();
    if distinct.len() != points.len() {
        return Err(Error::Invalid("outline vertices must be distinct".into()));
    }
    if !points.iter().all(|q| q.valid() && q.x >= 0.0 && q.y >= 0.0) {
        return Err(Error::Invalid(
            "outline vertices must be in range and inside the first quadrant".into(),
        ));
    }
    if arcs.iter().all(Option::is_none) {
        arcs.clear();
    }
    if outline_area(&points) < 0.01 {
        return Err(Error::Invalid("the outline has no area".into()));
    }
    validate_outline_arcs(&points, &arcs).map_err(Error::Invalid)?;
    let (lo, hi) = outline_extent(&points, &arcs)
        .ok_or_else(|| Error::Invalid("the outline is empty".into()))?;
    let snap = |v: f64| if v > -1e-6 { 0.0 } else { -v };
    let (dx, dy) = (snap(lo.x), snap(lo.y));
    if dx > 0.0 || dy > 0.0 {
        for q in points.iter_mut().chain(arcs.iter_mut().flatten()) {
            *q = Point::new(q.x + dx, q.y + dy);
        }
    }
    let (width, height) = (hi.x + dx, hi.y + dy);
    if !pcbcraft_geom::positive(width) || !pcbcraft_geom::positive(height) {
        return Err(Error::Invalid(
            "the outline is outside the board limits".into(),
        ));
    }
    let arced = arcs.iter().flatten().count();
    let ring = flatten_outline(&points, &arcs, 0.01);
    p.board.outline = Some(points);
    p.board.outline_arcs = arcs;
    p.board.width = width;
    p.board.height = height;
    Ok(json!({
        "arcs": arced,
        "width": width,
        "height": height,
        "area": (outline_area(&ring) * 100.0).round() / 100.0,
        "shift": {"x": dx, "y": dy},
    }))
}

fn segment_index(points: &[Point], segment: usize) -> Result<usize, Error> {
    if segment < points.len() {
        Ok(segment)
    } else {
        Err(Error::Invalid(format!(
            "segment {segment} does not exist; the outline has {} segments",
            points.len()
        )))
    }
}

/// `board.outline_set_arc`: bend one segment by midpoint or by radius and side.
///
/// # Rules
/// - Exactly one of `mid`, or `radius` (with optional `side`, default convex).
/// - Convex means bulging away from the enclosed area, whatever the winding.
fn set_arc(p: &mut Project, params: &Value) -> Result<Value, Error> {
    let a: SetArc = parse(params)?;
    let (points, mut arcs) = current(p)?;
    let i = segment_index(&points, a.segment)?;
    let (from, to) = (points[i], points[(i + 1) % points.len()]);
    let mid = match (a.mid, a.radius) {
        (Some(_), Some(_)) | (None, None) => {
            return Err(Error::Invalid(
                "give either a midpoint or a radius, not both".into(),
            ));
        }
        (Some(m), None) => {
            if a.side.is_some() {
                return Err(Error::Invalid("a side only goes with a radius".into()));
            }
            m
        }
        (None, Some(r)) => {
            if !(r.is_finite() && r > 0.0 && r <= 10_000.0) {
                return Err(Error::Invalid(
                    "the radius must be between 0 and 10000 mm".into(),
                ));
            }
            let convex = match a.side.as_deref() {
                None | Some("convex") => true,
                Some("concave") => false,
                Some(_) => return Err(Error::Invalid("side is convex or concave".into())),
            };
            // Counter-clockwise outlines have their outside on the right.
            let left = convex == (outline_signed_area(&points) < 0.0);
            mid_for_radius(from, to, r, left)?
        }
    };
    arcs[i] = Some(mid);
    let mut result = commit(p, points, arcs)?;
    let radius = p
        .board
        .outline
        .as_ref()
        .and_then(|pts| segment_arc(pts, &p.board.outline_arcs, i))
        .map_or(0.0, |arc| arc.radius);
    if let Some(map) = result.as_object_mut() {
        map.insert("segment".into(), json!(i));
        map.insert("radius".into(), json!(radius));
    }
    Ok(result)
}

fn straighten(p: &mut Project, params: &Value) -> Result<Value, Error> {
    let a: Segment = parse(params)?;
    let (points, mut arcs) = current(p)?;
    let i = segment_index(&points, a.segment)?;
    if arcs[i].take().is_none() {
        return Err(Error::Invalid(format!("segment {i} is already straight")));
    }
    commit(p, points, arcs)
}

/// `board.outline_set_vertex`: move a vertex, keeping neighbouring arc radii.
///
/// # Rules
/// - An arc keeps its radius and side while the new chord is shorter than
///   twice the radius; otherwise it is straightened and listed.
fn set_vertex(p: &mut Project, params: &Value) -> Result<Value, Error> {
    let a: SetVertex = parse(params)?;
    let (mut points, mut arcs) = current(p)?;
    if a.index >= points.len() {
        return Err(Error::Invalid(format!(
            "vertex {} does not exist; the outline has {} vertices",
            a.index,
            points.len()
        )));
    }
    let n = points.len();
    let before = points.clone();
    points[a.index] = a.at;
    let mut straightened = Vec::new();
    for segment in [(a.index + n - 1) % n, a.index] {
        let Some(old_mid) = arcs[segment] else {
            continue;
        };
        let Some(old) = segment_arc(&before, &arcs, segment) else {
            arcs[segment] = None;
            continue;
        };
        let (from, to) = (before[segment], before[(segment + 1) % n]);
        let left = is_left(from, to, old_mid);
        let (nf, nt) = (points[segment], points[(segment + 1) % n]);
        match mid_for_radius(nf, nt, old.radius, left) {
            Ok(m) => arcs[segment] = Some(m),
            Err(_) => {
                arcs[segment] = None;
                straightened.push(segment);
            }
        }
    }
    let mut result = commit(p, points, arcs)?;
    if let Some(map) = result.as_object_mut() {
        map.insert("straightened".into(), json!(straightened));
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(s: &mut Session) -> Result<(), Error> {
        s.execute(
            "board.set_outline",
            json!({"points":[[0.0,0.0],[40.0,0.0],[40.0,30.0],[0.0,30.0]]}),
            None,
        )?;
        Ok(())
    }
    fn arc_radius(s: &Session, segment: usize) -> Result<f64, Error> {
        let points = s.project.board.outline.clone().ok_or_else(missing)?;
        segment_arc(&points, &s.project.board.outline_arcs, segment)
            .map(|a| a.radius)
            .ok_or_else(missing)
    }

    #[test]
    fn setting_an_arc_by_radius_bulges_the_chosen_side() -> Result<(), Error> {
        let mut s = Session::default();
        square(&mut s)?;
        let r = s.execute(
            "board.outline_set_arc",
            json!({"segment":1,"radius":20.0,"side":"convex"}),
            None,
        )?["result"]
            .clone();
        assert!((r["radius"].as_f64().ok_or_else(missing)? - 20.0).abs() < 1e-6);
        assert!(
            r["width"].as_f64().ok_or_else(missing)? > 40.0,
            "an outward bulge widens the board"
        );
        s.undo()?;
        s.execute(
            "board.outline_set_arc",
            json!({"segment":1,"radius":20.0,"side":"concave"}),
            None,
        )?;
        let mid = s.project.board.outline_arcs[1].ok_or_else(missing)?;
        assert!(mid.x < 40.0, "a concave arc bows into the board");
        assert_eq!(s.project.board.width, 40.0);
        Ok(())
    }

    #[test]
    fn dragging_a_vertex_keeps_adjacent_arc_radius() -> Result<(), Error> {
        let mut s = Session::default();
        square(&mut s)?;
        s.execute(
            "board.outline_set_arc",
            json!({"segment":0,"radius":30.0,"side":"convex"}),
            None,
        )?;
        let before = arc_radius(&s, 0)?;
        let r = s.execute(
            "board.outline_set_vertex",
            json!({"index":1,"at":{"x":38.0,"y":4.0}}),
            None,
        )?["result"]
            .clone();
        assert!((arc_radius(&s, 0)? - before).abs() < 1e-6);
        assert_eq!(r["straightened"], json!([]));
        Ok(())
    }

    #[test]
    fn dragging_a_vertex_too_far_straightens_the_arc_and_says_so() -> Result<(), Error> {
        let mut s = Session::default();
        square(&mut s)?;
        s.execute(
            "board.outline_set_arc",
            json!({"segment":0,"radius":21.0,"side":"convex"}),
            None,
        )?;
        let r = s.execute(
            "board.outline_set_vertex",
            json!({"index":1,"at":{"x":60.0,"y":0.0}}),
            None,
        )?["result"]
            .clone();
        assert_eq!(r["straightened"], json!([0]));
        assert!(s.project.board.outline_arcs.is_empty());
        Ok(())
    }

    #[test]
    fn a_refused_outline_edit_leaves_the_design() -> Result<(), Error> {
        let mut s = Session::default();
        square(&mut s)?;
        let before = s.project.clone();
        for (cmd, bad) in [
            ("board.outline_set_arc", json!({"segment":9,"radius":20.0})),
            ("board.outline_set_arc", json!({"segment":0})),
            ("board.outline_set_arc", json!({"segment":0,"radius":5.0})),
            (
                "board.outline_set_arc",
                json!({"segment":0,"mid":{"x":20.0,"y":-20.0}}),
            ),
            (
                "board.outline_set_arc",
                json!({"segment":0,"mid":{"x":20.0,"y":40.0}}),
            ),
            ("board.outline_straighten", json!({"segment":0})),
            (
                "board.outline_set_vertex",
                json!({"index":7,"at":{"x":1.0,"y":1.0}}),
            ),
            (
                "board.outline_set_vertex",
                json!({"index":1,"at":{"x":-1.0,"y":0.0}}),
            ),
            (
                "board.outline_set_vertex",
                json!({"index":1,"at":{"x":0.0,"y":0.0}}),
            ),
        ] {
            assert!(s.execute(cmd, bad, None).is_err(), "{cmd} refuses it");
            assert_eq!(s.project, before, "{cmd} left the design alone");
        }
        Ok(())
    }

    #[test]
    fn outline_edits_undo_in_one_step() -> Result<(), Error> {
        let mut s = Session::default();
        square(&mut s)?;
        s.execute(
            "board.outline_set_arc",
            json!({"segment":2,"radius":25.0,"side":"convex"}),
            None,
        )?;
        s.execute("board.outline_straighten", json!({"segment":2}), None)?;
        assert!(s.project.board.outline_arcs.is_empty());
        s.undo()?;
        assert!(s.project.board.outline_arcs[2].is_some());
        s.undo()?;
        assert!(s.project.board.outline_arcs.is_empty());
        Ok(())
    }

    #[test]
    fn set_outline_takes_arcs_and_clear_outline_drops_them() -> Result<(), Error> {
        let mut s = Session::default();
        s.execute(
            "board.set_outline",
            json!({"points":[[0.0,2.0],[20.0,2.0],[20.0,22.0],[0.0,22.0]],
                   "arcs":[{"x":10.0,"y":0.0},null,null,null]}),
            None,
        )?;
        assert_eq!(s.project.board.outline_arcs.len(), 4);
        assert_eq!(s.project.board.height, 22.0, "the board follows the bulge");
        assert!(
            s.execute(
                "board.set_outline",
                json!({"points":[[0.0,0.0],[9.0,0.0],[9.0,9.0]],"arcs":[null]}),
                None
            )
            .is_err()
        );
        s.execute("board.clear_outline", json!({}), None)?;
        assert!(s.project.board.outline_arcs.is_empty());
        Ok(())
    }
}
