//! Initial electrical and geometric checks, with object-addressable diagnostics.
use pcbcraft_doc::{
    ArtKind, ArtShape, Board, FabRules, Id, Layer, PANEL_ASSEMBLER_MAX_MM, PANEL_FIDUCIAL_COUNT,
    PANEL_SCORE_COMPONENT_MIN_MM, PANEL_SCORE_COPPER_MIN_MM, PANEL_TAB_CHANNEL_MIN_MM, Pad,
    PanelSeparation, Project, Via, panel_layout, panel_score_lines,
};
use pcbcraft_geom::{Point, point_segment, segment_distance};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Serialize)]
pub struct Finding {
    pub code: &'static str,
    pub severity: String,
    pub message: String,
    pub objects: Vec<Id>,
}
#[derive(Clone, Debug, Serialize)]
pub struct Airwire {
    pub net: Id,
    pub from: Point,
    pub to: Point,
    pub from_object: Id,
    pub to_object: Id,
}
#[derive(Clone)]
struct Copper {
    id: Id,
    net: Id,
    start: Point,
    end: Point,
    radius: f64,
    contact_radius: f64,
    conservative: bool,
    outline: Option<Vec<Point>>,
    outline_error: f64,
    layer: Layer,
    pad: bool,
    /// Clearance demanded by the owning footprint's own design-rule
    /// overrides, if it carries one.
    clearance: Option<f64>,
    bridge: Option<(Id, String)>,
    hole: Option<Hole>,
    /// Closed outlines removed from this item's copper (zone cutouts).
    cutouts: Vec<Vec<Point>>,
}
#[derive(Clone)]
struct Hole {
    start: Point,
    end: Point,
    radius: f64,
}

fn copper(project: &Project) -> Vec<Copper> {
    let mut result = Vec::new();
    for f in &project.board.footprints {
        let local = project
            .component(f.component)
            .and_then(|c| c.library_footprint.as_ref())
            .and_then(|l| l.definition.rules.as_ref())
            .and_then(|r| r.clearance);
        for pad in &f.pads {
            if pad
                .geometry
                .as_ref()
                .is_some_and(|g| g.pad_type == "unplated")
            {
                let Some(drill) = pad.geometry.as_ref().and_then(|g| g.drill.as_ref()) else {
                    continue;
                };
                let position = f.pad_world_position(pad);
                let radius = drill.size.x.min(drill.size.y) / 2.0;
                if radius <= 0.0 {
                    continue;
                }
                let item = Copper {
                    id: f.id,
                    net: 0,
                    start: position,
                    end: position,
                    radius,
                    contact_radius: radius,
                    conservative: false,
                    outline: None,
                    outline_error: 0.0,
                    layer: Layer::Front,
                    pad: false,
                    clearance: None,
                    bridge: None,
                    hole: None,
                    cutouts: vec![],
                };
                let mut back = item.clone();
                back.layer = Layer::Back;
                result.push(back);
                result.push(item);
                continue;
            }
            if let Some(position) = f.pad_position(&pad.number) {
                let plated = pad
                    .geometry
                    .as_ref()
                    .is_some_and(|g| g.pad_type == "thru_hole");
                let hole = pad.geometry.as_ref().and_then(|g| {
                    g.drill.as_ref().map(|d| {
                        let radius = d.size.x.min(d.size.y) / 2.0;
                        let axis = Point::new(
                            (d.size.x / 2.0 - radius).max(0.0),
                            (d.size.y / 2.0 - radius).max(0.0),
                        );
                        let transform = |local: Point| {
                            f.position.offset(
                                pad.offset
                                    .offset(local.rotate(-g.rotation))
                                    .rotate(-f.rotation),
                            )
                        };
                        Hole {
                            start: transform(d.offset.offset(axis)),
                            end: transform(d.offset.offset(Point::new(-axis.x, -axis.y))),
                            radius,
                        }
                    })
                });
                let item = Copper {
                    id: f.id,
                    net: pad.net.unwrap_or(0),
                    start: position,
                    end: position,
                    radius: pad.bounding_radius(),
                    contact_radius: pad.contact_radius(),
                    conservative: pad.geometry.as_ref().is_some_and(|p| p.shape != "circle"),
                    outline: pad
                        .geometry
                        .as_ref()
                        .filter(|p| p.shape != "circle")
                        .map(|_| {
                            pad.outline_points()
                                .into_iter()
                                .map(|p| f.position.offset(p.rotate(-f.rotation)))
                                .collect()
                        }),
                    outline_error: pad.outline_error(),
                    layer: Layer::Front,
                    pad: true,
                    clearance: local,
                    bridge: if plated {
                        Some((f.id, pad.number.clone()))
                    } else {
                        None
                    },
                    hole,
                    cutouts: vec![],
                };
                if plated {
                    let mut back = item.clone();
                    back.layer = Layer::Back;
                    result.push(back);
                }
                result.push(item);
            }
        }
    }
    for t in &project.board.netties {
        let half = t.diameter / 2.0;
        let axis = Point::new(half, 0.0).rotate(t.rotation);
        for (net, sign) in [(t.net_a, 1.0), (t.net_b, -1.0)] {
            let centre = t.position.offset(Point::new(axis.x * sign, axis.y * sign));
            result.push(Copper {
                id: t.id,
                net,
                start: centre,
                end: centre,
                radius: half,
                contact_radius: half,
                conservative: false,
                outline: None,
                outline_error: 0.0,
                layer: Layer::Front,
                pad: true,
                clearance: None,
                bridge: Some((t.id, "net tie".into())),
                hole: None,
                cutouts: vec![],
            });
        }
    }
    for t in &project.board.tracks {
        let straight = |a: Point, b: Point, radius: f64| Copper {
            id: t.id,
            net: t.net,
            start: a,
            end: b,
            radius,
            contact_radius: radius,
            conservative: false,
            outline: None,
            outline_error: 0.0,
            layer: t.layer,
            pad: false,
            clearance: None,
            bridge: None,
            hole: None,
            cutouts: vec![],
        };
        match t.curve() {
            None => result.push(straight(t.start, t.end, t.width / 2.0)),
            Some(arc) => {
                // Sample the arc as chords, each capsule grown by the sagitta
                // so the sampled copper covers the swept band.
                let chords = 16;
                let sagitta = arc.radius * (1.0 - (arc.sweep.abs() / (chords as f64 * 2.0)).cos());
                let radius = t.width / 2.0 + sagitta;
                for i in 0..chords {
                    let a = arc.point_at(i as f64 / chords as f64);
                    let b = arc.point_at((i + 1) as f64 / chords as f64);
                    result.push(straight(a, b, radius));
                }
            }
        }
    }
    for v in &project.board.vias {
        let hole = (v.drill > 0.0).then(|| Hole {
            start: v.position,
            end: v.position,
            radius: v.drill / 2.0,
        });
        let item = Copper {
            id: v.id,
            net: v.net,
            start: v.position,
            end: v.position,
            radius: v.diameter / 2.0,
            contact_radius: v.diameter / 2.0,
            conservative: false,
            outline: None,
            outline_error: 0.0,
            layer: Layer::Front,
            pad: true,
            clearance: None,
            bridge: Some((v.id, "via".into())),
            hole,
            cutouts: vec![],
        };
        let mut back = item.clone();
        back.layer = Layer::Back;
        result.push(back);
        result.push(item);
    }
    for h in &project.board.holes {
        // An unplated drill is a keep-out shape on every layer: copper may
        // not enter the drilled area and the drill must clear the edge. A
        // slot is a capsule between its two centres, so clearance and the
        // edge check measure the drilled shape itself.
        let item = Copper {
            id: h.id,
            net: 0,
            start: h.start(),
            end: h.finish(),
            radius: h.diameter / 2.0,
            contact_radius: h.diameter / 2.0,
            conservative: false,
            outline: None,
            outline_error: 0.0,
            layer: Layer::Front,
            pad: false,
            clearance: None,
            bridge: None,
            hole: None,
            cutouts: vec![],
        };
        let mut back = item.clone();
        back.layer = Layer::Back;
        result.push(back);
        result.push(item);
    }
    for z in &project.board.zones {
        if !z.filled {
            continue;
        }
        let Some(&first) = z.outline.first() else {
            continue;
        };
        result.push(Copper {
            id: z.id,
            net: z.net,
            start: first,
            end: first,
            radius: 0.0,
            contact_radius: 0.0,
            conservative: false,
            outline: Some(z.outline.clone()),
            outline_error: 0.0,
            layer: z.layer,
            pad: false,
            clearance: None,
            bridge: None,
            hole: None,
            cutouts: z.cutouts.clone(),
        });
    }
    result
}
/// Copper a new track must keep clear of, seen exactly as the checks see it.
#[derive(Clone, Debug)]
pub struct Obstacle {
    pub id: Id,
    pub net: Id,
    pub layer: Layer,
    /// True only for straight track segments, the one kind a shove may move.
    /// Pads, vias, drilled holes, net ties, zones and arc tracks stay put.
    pub is_track: bool,
    /// Support points: the true pad outline, or both ends of a segment.
    pub points: Vec<Point>,
    /// Grow the support points by this much to get the copper edge.
    pub radius: f64,
    /// Where `Project::required_clearance_at` is asked about this copper.
    pub at: Point,
    /// Extra clearance the owning footprint demands, if any.
    pub clearance: Option<f64>,
}

/// Every copper item on `layer`, built from the same list that `drc` uses.
pub fn obstacles(project: &Project, layer: Layer) -> Vec<Obstacle> {
    copper(project)
        .into_iter()
        .filter(|c| c.layer == layer)
        .map(|c| {
            let at = Point::new((c.start.x + c.end.x) / 2.0, (c.start.y + c.end.y) / 2.0);
            let straight = project
                .board
                .tracks
                .iter()
                .any(|t| t.id == c.id && t.arc.is_none());
            let (points, radius, is_track) = match c.outline {
                Some(points) => (points, c.outline_error, false),
                None => (vec![c.start, c.end], c.radius, straight),
            };
            Obstacle {
                id: c.id,
                net: c.net,
                layer: c.layer,
                is_track,
                points,
                radius,
                at,
                clearance: c.clearance,
            }
        })
        .collect()
}
fn hole_contains(hole: &Hole, other: &Copper) -> bool {
    // The distance-to-segment function is convex. Endpoint discs contained by
    // the hole guarantee containment of the complete track capsule. Retain
    // conservative contact bounds for pad shapes rather than inventing copper.
    let radius = if other.pad {
        other.radius
    } else {
        other.contact_radius
    };
    [other.start, other.end]
        .into_iter()
        .all(|point| point_segment(point, hole.start, hole.end) + radius < hole.radius - 1e-6)
}
fn overlapping(a: &Copper, b: &Copper) -> bool {
    // A segment endpoint (or one whole outline) inside another outline means
    // real overlap, which capsule distance alone cannot see. Zone cutouts
    // remove the contact where the touching point falls inside one.
    let segment_in = |points: &[Point], seg: &Copper| {
        [seg.start, seg.end]
            .into_iter()
            .any(|p| inside(p, points) && !a.cutouts.iter().any(|c| inside(p, c)))
    };
    match (&a.outline, &b.outline) {
        (Some(a_points), Some(b_points)) => {
            let whole = |points: &[Point], other: &[Point]| {
                points
                    .first()
                    .is_some_and(|p| inside(*p, other) && !a.cutouts.iter().any(|c| inside(*p, c)))
            };
            whole(a_points, b_points) || whole(b_points, a_points)
        }
        (Some(a_points), None) => segment_in(a_points, b),
        (None, Some(b_points)) => [a.start, a.end]
            .into_iter()
            .any(|p| inside(p, b_points) && !b.cutouts.iter().any(|c| inside(p, c))),
        (None, None) => false,
    }
}
fn touching(a: &Copper, b: &Copper) -> bool {
    if a.net != b.net {
        return false;
    }
    if a.bridge.is_some() && a.bridge == b.bridge {
        return true;
    }
    if a.layer == b.layer && (overlapping(a, b) || overlapping(b, a)) {
        return true;
    }
    a.layer == b.layer
        && segment_distance(a.start, a.end, b.start, b.end) - a.contact_radius - b.contact_radius
            <= 1e-6
        && !a.hole.as_ref().is_some_and(|hole| hole_contains(hole, b))
        && !b.hole.as_ref().is_some_and(|hole| hole_contains(hole, a))
}
fn gap(a: &Copper, b: &Copper) -> f64 {
    match (&a.outline, &b.outline) {
        (Some(a_points), Some(b_points)) => {
            let mut distance = f64::INFINITY;
            if a_points.first().is_some_and(|p| inside(*p, b_points))
                || b_points.first().is_some_and(|p| inside(*p, a_points))
            {
                return -a.outline_error - b.outline_error;
            }
            for a_edge in a_points.windows(2) {
                for b_edge in b_points.windows(2) {
                    distance =
                        distance.min(segment_distance(a_edge[0], a_edge[1], b_edge[0], b_edge[1]));
                }
            }
            distance - a.outline_error - b.outline_error
        }
        (Some(points), None) => {
            polygon_segment(points, b.start, b.end) - b.radius - a.outline_error
        }
        (None, Some(points)) => {
            polygon_segment(points, a.start, a.end) - a.radius - b.outline_error
        }
        (None, None) => segment_distance(a.start, a.end, b.start, b.end) - a.radius - b.radius,
    }
}
fn inside(point: Point, points: &[Point]) -> bool {
    let mut result = false;
    for edge in points.windows(2) {
        let (a, b) = (edge[0], edge[1]);
        if (a.y > point.y) != (b.y > point.y)
            && point.x < (b.x - a.x) * (point.y - a.y) / (b.y - a.y) + a.x
        {
            result = !result;
        }
    }
    result
}
fn polygon_segment(points: &[Point], start: Point, end: Point) -> f64 {
    if inside(start, points) || inside(end, points) {
        return 0.0;
    }
    points
        .windows(2)
        .map(|edge| segment_distance(edge[0], edge[1], start, end))
        .fold(f64::INFINITY, f64::min)
}
fn root(parent: &[usize], mut node: usize) -> usize {
    for _ in 0..parent.len() {
        let Some(next) = parent.get(node).copied() else {
            return node;
        };
        if next == node {
            return node;
        }
        node = next;
    }
    node
}
pub fn airwires(project: &Project) -> Vec<Airwire> {
    let items = copper(project);
    let mut parents: Vec<_> = (0..items.len()).collect();
    for (i, a) in items.iter().enumerate() {
        for (j, b) in items.iter().enumerate().skip(i + 1) {
            if touching(a, b) {
                let ra = root(&parents, i);
                let rb = root(&parents, j);
                if let Some(p) = parents.get_mut(rb) {
                    *p = ra;
                }
            }
        }
    }
    let mut groups: BTreeMap<Id, BTreeMap<usize, Vec<&Copper>>> = BTreeMap::new();
    for (i, item) in items
        .iter()
        .enumerate()
        .filter(|(_, c)| c.pad && c.net != 0)
    {
        groups
            .entry(item.net)
            .or_default()
            .entry(root(&parents, i))
            .or_default()
            .push(item);
    }
    let mut result = vec![];
    for (net, islands) in groups {
        let Some(first) = islands.keys().next().copied() else {
            continue;
        };
        let mut reached = BTreeSet::from([first]);
        while reached.len() < islands.len() {
            let mut nearest: Option<(f64, usize, &Copper, &Copper)> = None;
            for (a_key, a_items) in &islands {
                if !reached.contains(a_key) {
                    continue;
                }
                for (b_key, b_items) in &islands {
                    if reached.contains(b_key) {
                        continue;
                    }
                    for a in a_items {
                        for b in b_items {
                            let d = a.start.distance(b.start);
                            if nearest.as_ref().is_none_or(|(best, _, _, _)| d < *best) {
                                nearest = Some((d, *b_key, a, b));
                            }
                        }
                    }
                }
            }
            let Some((_, key, a, b)) = nearest else {
                break;
            };
            reached.insert(key);
            result.push(Airwire {
                net,
                from: a.start,
                to: b.start,
                from_object: a.id,
                to_object: b.id,
            });
        }
    }
    result
}

pub fn erc(project: &Project) -> Vec<Finding> {
    let mut result = vec![];
    for c in &project.components {
        for p in &c.pins {
            if p.net.is_none()
                && !project
                    .no_connects
                    .iter()
                    .any(|f| f.pin.component == c.id && f.pin.pin == p.number)
            {
                result.push(Finding {
                    code: "unassigned_pin",
                    severity: project.rule_severity("unassigned_pin", "warning"),
                    message: format!("{} pin {} has no net", c.reference, p.number),
                    objects: vec![c.id],
                });
            }
        }
    }
    for net in &project.nets {
        let pins = project
            .components
            .iter()
            .flat_map(|c| &c.pins)
            .filter(|p| p.net == Some(net.id))
            .count();
        if pins < 2 {
            result.push(Finding {
                code: "single_pin_net",
                severity: project.rule_severity("single_pin_net", "warning"),
                message: format!("{} has {pins} connected pins", net.name),
                objects: vec![net.id],
            });
        }
    }
    result.retain(|f| {
        !project.rule_disabled(f.code)
            && !project.rule_excluded(f.code, &f.objects)
            && !project.rule_region_excluded(f.code, &f.objects)
    });
    result
}
/// True when a point is inside an implicitly closed polygon.
fn point_in_polygon(p: Point, polygon: &[Point]) -> bool {
    let mut inside = false;
    let edges = polygon
        .iter()
        .zip(polygon.iter().skip(1))
        .chain(std::iter::once((&polygon[polygon.len() - 1], &polygon[0])));
    for (a, b) in edges {
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
    }
    inside
}
/// Signed clearance from a copper segment to the board edge: positive when
/// both ends are inside, measured to the traced outline when one is set and
/// to the plain rectangle otherwise.
fn board_edge_distance(board: &Board, a: Point, b: Point) -> f64 {
    let Some(polygon) = &board.outline else {
        return [
            a.x,
            b.x,
            a.y,
            b.y,
            board.width - a.x,
            board.width - b.x,
            board.height - a.y,
            board.height - b.y,
        ]
        .into_iter()
        .fold(f64::INFINITY, f64::min);
    };
    let distance = polygon
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let q = polygon[(i + 1) % polygon.len()];
            segment_distance(a, b, *p, q)
        })
        .fold(f64::INFINITY, f64::min);
    if point_in_polygon(a, polygon) && point_in_polygon(b, polygon) {
        distance
    } else {
        -distance
    }
}

/// How far outside the board a drawn figure may sit before it counts as off
/// the board. A figure drawn on the edge is the normal card outline; only a
/// real overshoot is a finding.
const ART_EDGE_TOLERANCE: f64 = 0.01;

fn art_path(shape: &ArtShape) -> Vec<Point> {
    match shape {
        ArtShape::Line { start, end } => vec![*start, *end],
        ArtShape::Rect {
            x,
            y,
            width,
            height,
            rotate,
        } => {
            let center = Point::new(x + width / 2.0, y + height / 2.0);
            let mut ring: Vec<Point> = [
                Point::new(*x, *y),
                Point::new(x + width, *y),
                Point::new(x + width, y + height),
                Point::new(*x, y + height),
            ]
            .into_iter()
            .map(|c| center.offset(Point::new(c.x - center.x, c.y - center.y).rotate(*rotate)))
            .collect();
            ring.push(ring[0]);
            ring
        }
        ArtShape::Circle { center, radius } => (0..=48)
            .map(|i| {
                let turn = std::f64::consts::TAU * i as f64 / 48.0;
                Point::new(
                    center.x + turn.cos() * radius,
                    center.y + turn.sin() * radius,
                )
            })
            .collect(),
        ArtShape::Poly { points } => {
            let mut ring = points.clone();
            if let Some(first) = ring.first().copied() {
                ring.push(first);
            }
            ring
        }
        ArtShape::Text { at, .. } => vec![*at],
    }
}

fn art_encloses(shape: &ArtShape) -> bool {
    matches!(
        shape,
        ArtShape::Rect { .. } | ArtShape::Circle { .. } | ArtShape::Poly { .. }
    )
}

fn art_figure_name(index: usize, shape: &ArtShape) -> String {
    let kind = match shape {
        ArtShape::Line { .. } => "line",
        ArtShape::Rect { .. } => "rectangle",
        ArtShape::Circle { .. } => "circle",
        ArtShape::Poly { .. } => "outline",
        ArtShape::Text { .. } => "text",
    };
    format!("{kind} {}", index + 1)
}

fn rings_overlap(a: &[Point], b: &[Point]) -> bool {
    if a.len() < 3 || b.len() < 3 {
        return false;
    }
    a.iter().any(|p| point_in_polygon(*p, b))
        || b.iter().any(|p| point_in_polygon(*p, a))
        || b.windows(2)
            .any(|edge| polygon_segment(a, edge[0], edge[1]) <= 1e-9)
}

fn panel_findings(project: &Project) -> Vec<Finding> {
    if !project.panel.enabled {
        return Vec::new();
    }
    let settings = &project.panel;
    let Ok(layout) = panel_layout(project) else {
        return Vec::new();
    };
    let mut result = Vec::new();
    if layout.panel_width > PANEL_ASSEMBLER_MAX_MM + 1e-6
        || layout.panel_height > PANEL_ASSEMBLER_MAX_MM + 1e-6
    {
        result.push(Finding {
            code: "panel_too_large_for_assembler",
            severity: project.rule_severity("panel_too_large_for_assembler", "error"),
            message: format!(
                "Panel is {:.1} by {:.1} mm; assembly panels must fit within {:.0} by {:.0} mm",
                layout.panel_width,
                layout.panel_height,
                PANEL_ASSEMBLER_MAX_MM,
                PANEL_ASSEMBLER_MAX_MM
            ),
            objects: vec![],
        });
    }
    if project.board.fiducials.len() < PANEL_FIDUCIAL_COUNT {
        result.push(Finding {
            code: "panel_missing_fiducials",
            severity: project.rule_severity("panel_missing_fiducials", "error"),
            message: format!(
                "Panel needs {} fiducials in the rails; this board has {}",
                PANEL_FIDUCIAL_COUNT,
                project.board.fiducials.len()
            ),
            objects: vec![],
        });
    }
    if settings.separation == PanelSeparation::Tab
        && settings.tab_channel + 1e-6 < PANEL_TAB_CHANNEL_MIN_MM
    {
        result.push(Finding {
            code: "panel_tab_too_narrow",
            severity: project.rule_severity("panel_tab_too_narrow", "error"),
            message: format!(
                "Tab channel is {:.3} mm; routed tabs need at least {:.1} mm",
                settings.tab_channel, PANEL_TAB_CHANNEL_MIN_MM
            ),
            objects: vec![],
        });
    }
    if settings.separation != PanelSeparation::VScore {
        return result;
    }
    let Ok(score_lines) = panel_score_lines(project) else {
        return result;
    };
    let board = &project.board;
    for copy in &layout.copies {
        let shift = |p: Point| Point::new(p.x + copy.x, p.y + copy.y);
        for track in &board.tracks {
            let start = shift(track.start);
            let end = shift(track.end);
            let radius = track.width / 2.0;
            for (a, b) in &score_lines {
                let gap = segment_distance(start, end, *a, *b) - radius;
                if gap + 1e-6 < PANEL_SCORE_COPPER_MIN_MM {
                    result.push(Finding {
                        code: "panel_copper_near_score",
                        severity: project.rule_severity("panel_copper_near_score", "error"),
                        message: format!(
                            "Copper on copy ({}, {}) is {:.3} mm from a V-score line; keep at least {:.2} mm",
                            copy.row,
                            copy.column,
                            gap.max(0.0),
                            PANEL_SCORE_COPPER_MIN_MM
                        ),
                        objects: vec![track.id],
                    });
                    break;
                }
            }
        }
        for footprint in &board.footprints {
            let mut too_close = false;
            for pad in &footprint.pads {
                if let Some(position) = footprint.pad_position(&pad.number) {
                    let centre = shift(position);
                    let clearance = pad.bounding_radius();
                    for (a, b) in &score_lines {
                        if point_segment(centre, *a, *b) - clearance + 1e-6
                            < PANEL_SCORE_COMPONENT_MIN_MM
                        {
                            too_close = true;
                            break;
                        }
                    }
                }
                if too_close {
                    break;
                }
            }
            if too_close {
                result.push(Finding {
                    code: "panel_component_near_score",
                    severity: project.rule_severity("panel_component_near_score", "error"),
                    message: format!(
                        "A part on copy ({}, {}) sits within {:.1} mm of a V-score line",
                        copy.row, copy.column, PANEL_SCORE_COMPONENT_MIN_MM
                    ),
                    objects: vec![footprint.id],
                });
            }
        }
    }
    result
}

/// Clearance, board edge and cutout findings for the copper in `items`. When
/// `only` is set, just the findings that involve one of those object ids.
fn copper_findings(
    project: &Project,
    items: &[Copper],
    only: Option<&BTreeSet<Id>>,
) -> Vec<Finding> {
    let mut result = vec![];
    let board = &project.board;
    let wanted = |id: Id| only.is_none_or(|set| set.contains(&id));
    for (i, a) in items.iter().enumerate() {
        for b in items.iter().skip(i + 1) {
            if !wanted(a.id) && !wanted(b.id) {
                continue;
            }
            if a.bridge.is_some() && a.bridge == b.bridge {
                // A declared net tie's own pads touch on purpose; the pair is
                // the bridge, never a short.
                continue;
            }
            if (a.net != b.net || a.net == 0) && a.layer == b.layer {
                let mid = |c: &Copper| {
                    Point::new((c.start.x + c.end.x) / 2.0, (c.start.y + c.end.y) / 2.0)
                };
                let required = project
                    .required_clearance_at(a.net, b.net, mid(a), mid(b), a.layer, b.layer)
                    .max(a.clearance.unwrap_or(0.0))
                    .max(b.clearance.unwrap_or(0.0));
                if segment_distance(a.start, a.end, b.start, b.end) - a.radius - b.radius > required
                {
                    continue;
                }
                let separation = gap(a, b);
                if separation + 1e-6 < required {
                    let code = if a.conservative || b.conservative {
                        "pad_clearance_bound"
                    } else if separation <= 0.0 {
                        "copper_short"
                    } else {
                        "clearance"
                    };
                    result.push(Finding {
                        code,
                        severity: project.rule_severity(code, "error"),
                        message: format!(
                            "{} / {}: {:.3} mm clearance bound, {:.3} mm required",
                            project.net_name(a.net),
                            project.net_name(b.net),
                            separation,
                            required
                        ),
                        objects: vec![a.id, b.id],
                    });
                }
            }
        }
        if !wanted(a.id) {
            continue;
        }
        let edge = if let Some(points) = &a.outline {
            points
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    let q = points[(i + 1) % points.len()];
                    board_edge_distance(board, *p, q)
                })
                .fold(f64::INFINITY, f64::min)
                - a.outline_error
        } else {
            board_edge_distance(board, a.start, a.end) - a.radius
        };
        if edge + 1e-6 < board.clearance {
            result.push(Finding {
                code: "board_edge",
                severity: project.rule_severity("board_edge", "error"),
                message: format!("Copper is {:.3} mm from the board edge", edge),
                objects: vec![a.id],
            });
        }
        for cut in &board.cutouts {
            let clearance = if let Some(points) = &a.outline {
                polygon_segment(points, cut.center, cut.center) - cut.radius - a.outline_error
            } else {
                point_segment(cut.center, a.start, a.end) - cut.radius - a.radius
            };
            if clearance + 1e-6 < board.clearance {
                result.push(Finding {
                    code: "cutout_clearance",
                    severity: project.rule_severity("cutout_clearance", "error"),
                    message: format!("Copper violates cutout clearance ({clearance:.3} mm)"),
                    objects: vec![a.id, cut.id],
                });
            }
        }
    }
    result
}
/// Copper findings (clearance, short, board edge, cutout) that involve any of
/// the given track, via, pad or zone ids, without running the other checks.
/// Used to test a proposed edit before committing it.
pub fn copper_conflicts(project: &Project, ids: &[Id]) -> Vec<Finding> {
    let only: BTreeSet<Id> = ids.iter().copied().collect();
    let mut found = copper_findings(project, &copper(project), Some(&only));
    found.retain(|f| !project.rule_disabled(f.code));
    found
}
pub fn drc(project: &Project) -> Vec<Finding> {
    let mut result = vec![];
    let items = copper(project);
    let board = &project.board;
    result.extend(copper_findings(project, &items, None));
    for track in &board.tracks {
        if let Some(class) = project.net_class(track.net) {
            if let Some(min) = class.min_width.filter(|min| track.width < *min) {
                result.push(Finding {
                    code: "track_width",
                    severity: project.rule_severity("track_width", "error"),
                    message: format!(
                        "{} track is {:.3} mm wide, {} allows at least {:.3} mm",
                        project.net_name(track.net),
                        track.width,
                        class.name,
                        min
                    ),
                    objects: vec![track.id],
                });
            }
            if let Some(max) = class.max_width.filter(|max| track.width > *max) {
                result.push(Finding {
                    code: "track_width",
                    severity: project.rule_severity("track_width", "error"),
                    message: format!(
                        "{} track is {:.3} mm wide, {} allows at most {:.3} mm",
                        project.net_name(track.net),
                        track.width,
                        class.name,
                        max
                    ),
                    objects: vec![track.id],
                });
            }
        }
    }
    for wire in airwires(project) {
        result.push(Finding {
            code: "unrouted",
            severity: project.rule_severity("unrouted", "warning"),
            message: format!(
                "{} has disconnected copper islands",
                project.net_name(wire.net)
            ),
            objects: vec![wire.from_object, wire.to_object],
        });
    }
    for c in &project.components {
        if !board.footprints.iter().any(|f| f.component == c.id) {
            result.push(Finding {
                code: "missing_footprint",
                severity: project.rule_severity("missing_footprint", "warning"),
                message: format!("{} has no PCB footprint; sync the board", c.reference),
                objects: vec![c.id],
            });
        }
    }
    let mut lengths: BTreeMap<Id, f64> = BTreeMap::new();
    for track in &board.tracks {
        *lengths.entry(track.net).or_insert(0.0) += track.length();
    }
    for pair in &board.diff_pairs {
        for a in board
            .tracks
            .iter()
            .filter(|t| t.net == pair.net_p && t.layer == Layer::Front)
        {
            for b in board
                .tracks
                .iter()
                .filter(|t| t.net == pair.net_n && t.layer == Layer::Front)
            {
                let separation = segment_distance(a.start, a.end, b.start, b.end)
                    - a.width / 2.0
                    - b.width / 2.0;
                let required = project
                    .required_clearance_at(pair.net_p, pair.net_n, a.end, b.end, a.layer, b.layer);
                if separation + 1e-6 >= pair.gap.max(required) {
                    continue;
                }
                result.push(Finding {
                    code: "diffpair_gap",
                    severity: project.rule_severity("diffpair_gap", "error"),
                    message: format!(
                        "{} / {}: {} couples at {:.3} mm, {:.3} mm gap wanted",
                        project.net_name(pair.net_p),
                        project.net_name(pair.net_n),
                        pair.name,
                        separation,
                        pair.gap
                    ),
                    objects: vec![a.id, b.id],
                });
            }
        }
        let Some(tolerance) = pair.length_tolerance else {
            continue;
        };
        if !lengths.contains_key(&pair.net_p) && !lengths.contains_key(&pair.net_n) {
            continue;
        }
        let skew = (lengths.get(&pair.net_p).copied().unwrap_or(0.0)
            - lengths.get(&pair.net_n).copied().unwrap_or(0.0))
        .abs();
        if skew <= tolerance + 1e-9 {
            continue;
        }
        result.push(Finding {
            code: "diffpair_skew",
            severity: project.rule_severity("diffpair_skew", "error"),
            message: format!(
                "{}: {} sides differ by {:.3} mm, at most {:.3} mm allowed",
                pair.name,
                project.net_name(pair.net_p),
                skew,
                tolerance
            ),
            objects: vec![pair.net_p, pair.net_n],
        });
    }
    for group in &board.length_groups {
        let routed: Vec<(Id, f64)> = group
            .nets
            .iter()
            .filter_map(|n| lengths.get(n).map(|l| (*n, *l)))
            .collect();
        if routed.len() < 2 {
            continue;
        }
        let longest = routed
            .iter()
            .fold(routed[0], |a, b| if b.1 > a.1 { *b } else { a });
        let shortest = routed
            .iter()
            .fold(routed[0], |a, b| if b.1 < a.1 { *b } else { a });
        let spread = longest.1 - shortest.1;
        if spread <= group.tolerance + 1e-9 {
            continue;
        }
        result.push(Finding {
            code: "length_group_skew",
            severity: project.rule_severity("length_group_skew", "error"),
            message: format!(
                "{}: {} is {:.3} mm longer than {}, at most {:.3} mm allowed",
                group.name,
                project.net_name(longest.0),
                spread,
                project.net_name(shortest.0),
                group.tolerance
            ),
            objects: vec![longest.0, shortest.0],
        });
    }
    let classified: BTreeSet<Id> = project
        .net_classes
        .iter()
        .flat_map(|c| c.nets.clone())
        .collect();
    for class in &project.net_classes {
        let Some(target) = class.target_length else {
            continue;
        };
        let tolerance = class.length_tolerance.unwrap_or(0.0);
        for net in &class.nets {
            let own = project
                .nets
                .iter()
                .any(|n| n.id == *net && n.target_length.is_some());
            if own {
                // A net with its own target is judged against that, not the class.
                continue;
            }
            let Some(length) = lengths.get(net) else {
                continue;
            };
            if (length - target).abs() <= tolerance + 1e-9 {
                continue;
            }
            result.push(Finding {
                code: "track_length",
                severity: project.rule_severity("track_length", "error"),
                message: format!(
                    "{} routed to {:.3} mm, {} targets {:.3} mm within {:.3} mm",
                    project.net_name(*net),
                    length,
                    class.name,
                    target,
                    tolerance
                ),
                objects: vec![*net],
            });
        }
    }
    for net in &project.nets {
        let Some(target) = net.target_length else {
            continue;
        };
        if classified.contains(&net.id) {
            // A classified net without its own target already followed its class.
            continue;
        }
        let tolerance = net.length_tolerance.unwrap_or(0.0);
        let length = lengths.get(&net.id).copied().unwrap_or(0.0);
        if (length - target).abs() <= tolerance + 1e-9 {
            continue;
        }
        result.push(Finding {
            code: "track_length",
            severity: project.rule_severity("track_length", "error"),
            message: format!(
                "{} routed to {:.3} mm, {} targets {:.3} mm within {:.3} mm",
                net.name, length, net.name, target, tolerance
            ),
            objects: vec![net.id],
        });
    }
    result.extend(panel_findings(project));
    result.extend(fab_findings(project, board));
    // Drawn machine layers sit beside the copper.
    // not run through a via or a pad, a mask figure must not cover an exposed
    // pad, and no figure may leave the board. Drawn silk and reference art are
    // left alone: silk over pads belongs to its own check.
    for layer in &board.user_layers {
        let cutting = matches!(layer.kind, ArtKind::Cut | ArtKind::Perforation);
        let masking = layer.kind == ArtKind::Mask;
        if !cutting && !masking {
            continue;
        }
        for (index, shape) in layer.shapes.iter().enumerate() {
            let path = art_path(shape);
            let figure = art_figure_name(index, shape);
            let outside = path
                .iter()
                .map(|p| board_edge_distance(board, *p, *p))
                .fold(f64::INFINITY, f64::min);
            if outside < -ART_EDGE_TOLERANCE {
                result.push(Finding {
                    code: "art_off_board",
                    severity: project.rule_severity("art_off_board", "warning"),
                    message: format!(
                        "{figure} on {} reaches {:.2} mm past the board edge; the mill holds plus or minus 0.2 mm and a score line 0.4 to 0.5 mm",
                        layer.name,
                        -outside
                    ),
                    objects: vec![layer.id],
                });
            }
            if cutting {
                // Vias are read from the board, not from the copper list that
                // names each plated item twice: one cut crosses every layer.
                for v in &board.vias {
                    let required = v.diameter / 2.0 + board.clearance;
                    let distance = path
                        .windows(2)
                        .map(|s| point_segment(v.position, s[0], s[1]))
                        .fold(f64::INFINITY, f64::min);
                    if distance + 1e-9 >= required {
                        continue;
                    }
                    result.push(Finding {
                        code: "cut_hits_via",
                        severity: project.rule_severity("cut_hits_via", "error"),
                        message: format!(
                            "{figure} on {} passes {:.2} mm from the via at {:.2}, {:.2}; {:.2} mm is needed",
                            layer.name, distance, v.position.x, v.position.y, required
                        ),
                        objects: vec![layer.id, v.id],
                    });
                }
                // Pads are cut against their true copper outline, the same
                // rotated ring the clearance pass measures them by.
                for f in &board.footprints {
                    for pad in &f.pads {
                        let outline: Vec<Point> = pad
                            .outline_points()
                            .into_iter()
                            .map(|p| f.position.offset(p.rotate(-f.rotation)))
                            .collect();
                        let distance = path
                            .windows(2)
                            .map(|s| polygon_segment(&outline, s[0], s[1]))
                            .fold(f64::INFINITY, f64::min);
                        if distance + 1e-9 >= board.clearance {
                            continue;
                        }
                        let owner = project
                            .component(f.component)
                            .map(|c| c.reference.clone())
                            .unwrap_or_else(|| format!("footprint {}", f.id));
                        result.push(Finding {
                            code: "cut_hits_via",
                            severity: project.rule_severity("cut_hits_via", "error"),
                            message: format!(
                                "{figure} on {} passes {:.2} mm from pad {} of {owner}; {:.2} mm is needed",
                                layer.name, distance, pad.number, board.clearance
                            ),
                            objects: vec![layer.id, f.id],
                        });
                    }
                }
            }
            if masking && art_encloses(shape) {
                for f in &board.footprints {
                    for pad in &f.pads {
                        // A pad without geometry comes from a built-in
                        // template and carries an opening. Layers that name
                        // only the far side leave the top opening clear.
                        let exposed = pad.geometry.as_ref().is_none_or(|g| {
                            g.layers.iter().any(|l| l == "F.Mask" || l == "*.Mask")
                        });
                        if !exposed {
                            continue;
                        }
                        let outline: Vec<Point> = pad
                            .outline_points()
                            .into_iter()
                            .map(|p| f.position.offset(p.rotate(-f.rotation)))
                            .collect();
                        if !rings_overlap(&path, &outline) {
                            continue;
                        }
                        let owner = project
                            .component(f.component)
                            .map(|c| c.reference.clone())
                            .unwrap_or_else(|| format!("footprint {}", f.id));
                        // A drawn layer carries no side, so the message says
                        // the top side it is assumed to print on.
                        result.push(Finding {
                            code: "mask_covers_pad",
                            severity: project.rule_severity("mask_covers_pad", "warning"),
                            message: format!(
                                "{figure} on {} covers the opening of pad {} of {owner} on the top side",
                                layer.name, pad.number
                            ),
                            objects: vec![layer.id, f.id],
                        });
                    }
                }
            }
        }
    }
    result.retain(|f| {
        !project.rule_disabled(f.code)
            && !project.rule_excluded(f.code, &f.objects)
            && !project.rule_region_excluded(f.code, &f.objects)
    });
    result
}

fn pad_outer_radius(pad: &Pad) -> f64 {
    pad.geometry
        .as_ref()
        .map(|g| g.size.x.max(g.size.y) / 2.0)
        .unwrap_or(pad.diameter / 2.0)
}

fn pad_drill_diameter(pad: &Pad) -> Option<f64> {
    pad.geometry
        .as_ref()
        .and_then(|g| g.drill.as_ref())
        .map(|d| d.size.x.min(d.size.y))
}

fn via_annular_ring(rules: &FabRules, via: &Via) -> f64 {
    let tool = rules.drill_tool(via.drill);
    (via.diameter - tool) / 2.0
}

fn pad_annular_ring(rules: &FabRules, pad: &Pad) -> Option<f64> {
    let drill = pad_drill_diameter(pad)?;
    let tool = rules.drill_tool(drill);
    Some((pad_outer_radius(pad) * 2.0 - tool) / 2.0)
}

fn fab_findings(project: &Project, board: &Board) -> Vec<Finding> {
    let Some(rules) = board.rules.as_ref() else {
        return vec![];
    };
    let mut result = vec![];
    if let Some(min) = rules.min_track {
        for track in &board.tracks {
            if track.width + 1e-6 < min {
                result.push(Finding {
                    code: "min_track",
                    severity: project.rule_severity("min_track", "error"),
                    message: format!(
                        "{} track is {:.3} mm wide, fabrication needs at least {:.3} mm",
                        project.net_name(track.net),
                        track.width,
                        min
                    ),
                    objects: vec![track.id],
                });
            }
        }
    }
    if let Some(min) = rules.min_drill {
        for via in &board.vias {
            if via.drill > 0.0 && via.drill + 1e-6 < min {
                result.push(Finding {
                    code: "min_drill",
                    severity: project.rule_severity("min_drill", "error"),
                    message: format!(
                        "Via drill {:.3} mm is below the {:.3} mm fabrication minimum",
                        via.drill, min
                    ),
                    objects: vec![via.id],
                });
            }
        }
        for fp in &board.footprints {
            for pad in &fp.pads {
                if let Some(drill) = pad_drill_diameter(pad)
                    && drill + 1e-6 < min
                {
                    result.push(Finding {
                        code: "min_drill",
                        severity: project.rule_severity("min_drill", "error"),
                        message: format!(
                            "Pad drill {:.3} mm is below the {:.3} mm fabrication minimum",
                            drill, min
                        ),
                        objects: vec![fp.id],
                    });
                }
            }
        }
    }
    if let Some(min) = rules.min_ring_via {
        for via in &board.vias {
            if via.drill <= 0.0 {
                continue;
            }
            let ring = via_annular_ring(rules, via);
            if ring + 1e-6 < min {
                result.push(Finding {
                    code: "annular_ring",
                    severity: project.rule_severity("annular_ring", "error"),
                    message: format!(
                        "Via annular ring {:.3} mm is below the {:.3} mm via minimum",
                        ring, min
                    ),
                    objects: vec![via.id],
                });
            }
        }
    }
    if let Some(min) = rules.min_ring_pth {
        for fp in &board.footprints {
            for pad in &fp.pads {
                let Some(ring) = pad_annular_ring(rules, pad) else {
                    continue;
                };
                if ring + 1e-6 < min {
                    result.push(Finding {
                        code: "annular_ring",
                        severity: project.rule_severity("annular_ring", "error"),
                        message: format!(
                            "Pad annular ring {:.3} mm is below the {:.3} mm plated-hole minimum",
                            ring, min
                        ),
                        objects: vec![fp.id],
                    });
                }
            }
        }
    }
    if let Some(edge) = rules.edge_routed {
        let items = copper(project);
        for a in &items {
            let edge_dist = if let Some(points) = &a.outline {
                points
                    .iter()
                    .enumerate()
                    .map(|(i, p)| {
                        let q = points[(i + 1) % points.len()];
                        board_edge_distance(board, *p, q)
                    })
                    .fold(f64::INFINITY, f64::min)
                    - a.outline_error
            } else {
                board_edge_distance(board, a.start, a.end) - a.radius
            };
            if edge_dist + 1e-6 < edge {
                result.push(Finding {
                    code: "copper_to_edge",
                    severity: project.rule_severity("copper_to_edge", "error"),
                    message: format!("Copper is {:.3} mm from the routed edge", edge_dist),
                    objects: vec![a.id],
                });
            }
        }
    }
    if let Some(max_ratio) = rules.max_aspect_ratio {
        let mut smallest = f64::INFINITY;
        for via in &board.vias {
            if via.drill > 0.0 {
                smallest = smallest.min(via.drill);
            }
        }
        for fp in &board.footprints {
            for pad in &fp.pads {
                if let Some(d) = pad_drill_diameter(pad) {
                    smallest = smallest.min(d);
                }
            }
        }
        for hole in &board.holes {
            smallest = smallest.min(hole.diameter);
        }
        if smallest.is_finite() && board.thickness / smallest > max_ratio + 1e-6 {
            result.push(Finding {
                code: "aspect_ratio",
                severity: project.rule_severity("aspect_ratio", "error"),
                message: format!(
                    "Board {:.3} mm thick over {:.3} mm drill exceeds {:.1}:1",
                    board.thickness, smallest, max_ratio
                ),
                objects: vec![],
            });
        }
    }
    if let Some(min) = rules.min_space {
        let items = copper(project);
        for (i, a) in items.iter().enumerate() {
            for b in items.iter().skip(i + 1) {
                if a.net == b.net || a.layer != b.layer {
                    continue;
                }
                let separation = gap(a, b);
                if separation + 1e-6 < min {
                    result.push(Finding {
                        code: "min_space",
                        severity: project.rule_severity("min_space", "error"),
                        message: format!(
                            "{} / {} spacing {:.3} mm is below the {:.3} mm fabrication minimum",
                            project.net_name(a.net),
                            project.net_name(b.net),
                            separation,
                            min
                        ),
                        objects: vec![a.id, b.id],
                    });
                }
            }
        }
    }
    for layer in &board.user_layers {
        if layer.kind != ArtKind::Silk {
            continue;
        }
        for shape in &layer.shapes {
            if let ArtShape::Text { size, .. } = shape
                && let Some(min) = rules.silk_text_height
                && *size + 1e-6 < min
            {
                result.push(Finding {
                    code: "silk_text_height",
                    severity: project.rule_severity("silk_text_height", "error"),
                    message: format!(
                        "Silk text height {:.3} mm is below the {:.3} mm minimum",
                        size, min
                    ),
                    objects: vec![layer.id],
                });
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use pcbcraft_doc::{
        FabRules, Footprint, Net, NoConnectFlag, Pad, PinRef, Track, Via, component_template,
    };
    use pcbcraft_library::{Drill, FootprintPad};
    fn fixture() -> Project {
        let mut p = Project::default();
        p.nets.push(Net {
            id: 1,
            name: "signal".into(),
            ..Default::default()
        });
        for (id, x) in [(2, 10.0), (3, 30.0)] {
            if let Some(mut c) =
                component_template("resistor", id, format!("R{id}"), Point::new(x, 10.0))
            {
                for pin in &mut c.pins {
                    pin.net = Some(1);
                }
                p.components.push(c);
            }
            p.board.footprints.push(Footprint {
                id: id + 10,
                component: id,
                position: Point::new(x, 10.0),
                rotation: 0.0,
                locked: None,
                pads: vec![Pad {
                    number: "1".into(),
                    offset: Point::default(),
                    diameter: 1.0,
                    net: Some(1),
                    castellated: None,
                    geometry: None,
                }],
            });
        }
        p
    }
    fn test_copper(id: Id, point: Point, layer: Layer, net: Id, pad: bool) -> Copper {
        Copper {
            id,
            net,
            start: point,
            end: point,
            radius: if pad { 0.8 } else { 0.1 },
            contact_radius: if pad { 0.8 } else { 0.1 },
            conservative: false,
            outline: None,
            outline_error: 0.0,
            layer,
            pad,
            clearance: None,
            bridge: None,
            hole: None,
            cutouts: vec![],
        }
    }
    #[test]
    fn erc_suppresses_unassigned_pin_warnings_for_no_connect_flags() -> Result<(), String> {
        let mut p = Project::default();
        p.nets.push(Net {
            id: 1,
            name: "signal".into(),
            ..Default::default()
        });
        let mut c = component_template("resistor", 2, "R2".into(), Point::new(10.0, 10.0))
            .ok_or("resistor template missing")?;
        c.pins[0].net = Some(1);
        p.components.push(c);
        let (component, pin) = p
            .components
            .first()
            .and_then(|c| c.pins.get(1).map(|pin| (c.id, pin.number.clone())))
            .ok_or("template pins missing")?;
        assert!(erc(&p).iter().any(|f| f.code == "unassigned_pin"));
        p.no_connects.push(NoConnectFlag {
            id: 9,
            pin: PinRef { component, pin },
        });
        p.validate()?;
        assert!(!erc(&p).iter().any(|f| f.code == "unassigned_pin"));
        Ok(())
    }
    #[test]
    fn vias_join_front_and_back_copper_of_their_net() -> Result<(), String> {
        let mut p = Project::default();
        p.nets.push(Net {
            id: 1,
            name: "signal".into(),
            ..Default::default()
        });
        p.board.tracks.push(Track {
            id: 2,
            net: 1,
            start: Point::new(10.0, 10.0),
            end: Point::new(12.0, 10.0),
            width: 0.25,
            layer: Layer::Front,
            arc: None,
        });
        p.board.tracks.push(Track {
            id: 3,
            net: 1,
            start: Point::new(12.2, 10.0),
            end: Point::new(14.0, 10.0),
            width: 0.25,
            layer: Layer::Back,
            arc: None,
        });
        p.board.vias.push(Via {
            id: 4,
            net: 1,
            position: Point::new(12.1, 10.0),
            diameter: 1.4,
            drill: 0.4,
        });
        p.validate()?;
        let items = copper(&p);
        let front = items
            .iter()
            .find(|c| c.id == 4 && c.layer == Layer::Front)
            .ok_or("front via copper missing")?;
        let back = items
            .iter()
            .find(|c| c.id == 4 && c.layer == Layer::Back)
            .ok_or("back via copper missing")?;
        assert!(
            touching(front, back),
            "a plated via conducts on both layers"
        );
        let front_track = items
            .iter()
            .find(|c| c.id == 2)
            .ok_or("front track missing")?;
        assert!(
            touching(front, front_track),
            "the via pad must touch front copper it overlaps"
        );
        let back_track = items
            .iter()
            .find(|c| c.id == 3)
            .ok_or("back track missing")?;
        assert!(
            touching(back, back_track),
            "the via pad must touch back copper it overlaps"
        );
        Ok(())
    }
    #[test]
    fn zones_conduct_same_net_copper_inside_their_outline() -> Result<(), String> {
        let mut p = Project::default();
        p.nets.push(Net {
            id: 1,
            name: "GND".into(),
            ..Default::default()
        });
        p.nets.push(Net {
            id: 2,
            name: "other".into(),
            ..Default::default()
        });
        let square = vec![
            Point::new(0.0, 0.0),
            Point::new(50.0, 0.0),
            Point::new(50.0, 30.0),
            Point::new(0.0, 30.0),
            Point::new(0.0, 0.0),
        ];
        p.board.zones.push(pcbcraft_doc::CopperZone {
            id: 4,
            net: 1,
            layer: Layer::Front,
            outline: square,
            cutouts: vec![],
            filled: true,

            kind: None,
        });
        p.board.tracks.push(Track {
            id: 5,
            net: 1,
            start: Point::new(10.0, 10.0),
            end: Point::new(20.0, 10.0),
            width: 0.25,
            layer: Layer::Front,
            arc: None,
        });
        p.board.tracks.push(Track {
            id: 6,
            net: 1,
            start: Point::new(10.0, 10.0),
            end: Point::new(20.0, 10.0),
            width: 0.25,
            layer: Layer::Back,
            arc: None,
        });
        p.board.tracks.push(Track {
            id: 7,
            net: 2,
            start: Point::new(30.0, 20.0),
            end: Point::new(40.0, 20.0),
            width: 0.25,
            layer: Layer::Front,
            arc: None,
        });
        p.validate()?;
        let items = copper(&p);
        let zone = items
            .iter()
            .find(|c| c.id == 4)
            .ok_or("zone copper missing")?;
        let inside = items
            .iter()
            .find(|c| c.id == 5)
            .ok_or("front track missing")?;
        let other_layer = items
            .iter()
            .find(|c| c.id == 6)
            .ok_or("back track missing")?;
        let other_net = items
            .iter()
            .find(|c| c.id == 7)
            .ok_or("other-net track missing")?;
        assert!(
            touching(zone, inside),
            "same-net same-layer copper inside the outline conducts"
        );
        assert!(
            !touching(zone, other_layer),
            "the zone is confined to its own layer"
        );
        assert!(
            !touching(zone, other_net),
            "the zone never conducts to another net"
        );
        p.board.zones[0].cutouts.push(vec![
            Point::new(25.0, 5.0),
            Point::new(45.0, 5.0),
            Point::new(45.0, 25.0),
            Point::new(25.0, 25.0),
            Point::new(25.0, 5.0),
        ]);
        p.board.tracks.push(Track {
            id: 8,
            net: 1,
            start: Point::new(30.0, 10.0),
            end: Point::new(40.0, 10.0),
            width: 0.25,
            layer: Layer::Front,
            arc: None,
        });
        p.validate()?;
        let items = copper(&p);
        let zone = items
            .iter()
            .find(|c| c.id == 4)
            .ok_or("zone copper missing")?;
        let in_cutout = items
            .iter()
            .find(|c| c.id == 8)
            .ok_or("cutout track missing")?;
        assert!(
            !touching(zone, in_cutout) && !touching(in_cutout, zone),
            "copper inside a zone cutout never conducts through the zone"
        );
        Ok(())
    }
    #[test]
    fn a_stitched_via_grid_bridges_copper_across_layers() -> Result<(), String> {
        let mut p = Project::default();
        p.nets.push(Net {
            id: 1,
            name: "GND".into(),
            ..Default::default()
        });
        p.board.tracks.push(Track {
            id: 2,
            net: 1,
            start: Point::new(10.0, 10.0),
            end: Point::new(12.0, 10.0),
            width: 0.25,
            layer: Layer::Front,
            arc: None,
        });
        p.board.tracks.push(Track {
            id: 3,
            net: 1,
            start: Point::new(12.2, 10.0),
            end: Point::new(14.0, 10.0),
            width: 0.25,
            layer: Layer::Back,
            arc: None,
        });
        p.validate()?;
        for column in 0..5 {
            p.board.vias.push(Via {
                id: 4 + column,
                net: 1,
                position: Point::new(12.0 + column as f64 * 0.5, 10.0),
                diameter: 1.4,
                drill: 0.4,
            });
        }
        p.validate()?;
        let items = copper(&p);
        let front_track = items
            .iter()
            .find(|c| c.id == 2)
            .ok_or("front track missing")?;
        let back_track = items
            .iter()
            .find(|c| c.id == 3)
            .ok_or("back track missing")?;
        let bridges = items
            .iter()
            .filter(|c| c.id >= 4 && c.id <= 8 && c.layer == Layer::Front)
            .zip(
                items
                    .iter()
                    .filter(|c| c.id >= 4 && c.id <= 8 && c.layer == Layer::Back),
            )
            .any(|(front, back)| touching(front, front_track) && touching(back, back_track));
        assert!(
            bridges,
            "a stitched grid must contain a via bridging front and back copper of its net"
        );
        Ok(())
    }
    #[test]
    fn a_split_segment_still_conducts_across_the_break() -> Result<(), String> {
        let mut p = Project::default();
        p.nets.push(Net {
            id: 1,
            name: "SIG".into(),
            ..Default::default()
        });
        p.board.tracks.push(Track {
            id: 2,
            net: 1,
            start: Point::new(10.0, 10.0),
            end: Point::new(15.0, 10.0),
            width: 0.25,
            layer: Layer::Front,
            arc: None,
        });
        p.board.tracks.push(Track {
            id: 3,
            net: 1,
            start: Point::new(15.0, 10.0),
            end: Point::new(20.0, 10.0),
            width: 0.25,
            layer: Layer::Front,
            arc: None,
        });
        p.validate()?;
        let items = copper(&p);
        let head = items.iter().find(|c| c.id == 2).ok_or("head missing")?;
        let tail = items.iter().find(|c| c.id == 3).ok_or("tail missing")?;
        assert!(
            touching(head, tail) && touching(tail, head),
            "segments sharing an endpoint conduct through the split"
        );
        Ok(())
    }
    #[test]
    fn plated_pad_bridges_front_and_back_but_identity_and_net_must_match() {
        let mut front = test_copper(10, Point::new(10.0, 10.0), Layer::Front, 1, true);
        front.bridge = Some((10, "1".into()));
        let mut back = front.clone();
        back.layer = Layer::Back;
        assert!(touching(&front, &back));
        let mut smd = front.clone();
        smd.bridge = None;
        assert!(!touching(&smd, &back));
        let mut different_pad = back.clone();
        different_pad.bridge = Some((10, "2".into()));
        assert!(!touching(&front, &different_pad));
        let mut different_net = back;
        different_net.net = 2;
        assert!(!touching(&front, &different_net));
    }
    #[test]
    fn hole_only_stubs_do_not_touch_annulus_but_center_routes_cross_it() {
        let center = Point::new(10.0, 10.0);
        let mut pad = test_copper(10, center, Layer::Front, 1, true);
        pad.hole = Some(Hole {
            start: center,
            end: center,
            radius: 0.4,
        });
        let mut stub = test_copper(11, center, Layer::Front, 1, false);
        stub.end = Point::new(10.1, 10.0);
        assert!(!touching(&pad, &stub));
        assert!(!touching(&stub, &pad));
        stub.end = Point::new(12.0, 10.0);
        assert!(touching(&pad, &stub));
        // A slot wholly contains a short trace even when the trace is far
        // from the slot centre; test capsule containment rather than a circle.
        pad.hole = Some(Hole {
            start: Point::new(9.5, 10.0),
            end: Point::new(10.5, 10.0),
            radius: 0.25,
        });
        stub.start = Point::new(10.3, 10.0);
        stub.end = Point::new(10.4, 10.0);
        assert!(!touching(&pad, &stub));
    }
    #[test]
    fn front_track_to_plated_pad_to_back_track_connects_physical_pads() -> Result<(), String> {
        let mut p = fixture();
        let mut bridge = component_template("resistor", 4, "R4".into(), Point::new(20.0, 10.0))
            .ok_or("missing template")?;
        for pin in &mut bridge.pins {
            pin.net = Some(1);
        }
        p.components.push(bridge);
        let g = pcbcraft_library::FootprintPad {
            number: "1".into(),
            pad_type: "thru_hole".into(),
            shape: "circle".into(),
            position: Point::default(),
            rotation: 0.0,
            size: Point::new(1.6, 1.6),
            layers: vec!["*.Cu".into(), "*.Mask".into()],
            drill: Some(pcbcraft_library::Drill {
                size: Point::new(0.8, 0.8),
                offset: Point::default(),
            }),
            roundrect_ratio: None,
            chamfer: None,
            polygon: vec![],
            raw_source: String::new(),
        };
        p.board.footprints.push(Footprint {
            id: 14,
            component: 4,
            position: Point::new(20.0, 10.0),
            rotation: 0.0,
            locked: None,
            pads: vec![Pad {
                number: "1".into(),
                offset: Point::default(),
                diameter: 1.6,
                net: Some(1),
                castellated: None,
                geometry: Some(g),
            }],
        });
        // Endpoints are front SMD; their separate front approach demonstrates
        // that the only cross-layer join comes from the plated package pad.
        for (id, start, end, layer) in [
            (
                20,
                Point::new(10.0, 10.0),
                Point::new(20.0, 10.0),
                Layer::Front,
            ),
            (
                21,
                Point::new(20.0, 10.0),
                Point::new(30.0, 10.0),
                Layer::Back,
            ),
            (
                22,
                Point::new(30.0, 10.0),
                Point::new(30.0, 11.0),
                Layer::Front,
            ),
        ] {
            p.board.tracks.push(Track {
                id,
                net: 1,
                start,
                end,
                width: 0.2,
                layer,
                arc: None,
            });
        }
        // Make the far endpoint a plated pad as well, so its back approach
        // meets that package's copper instead of an unsupported layer jump.
        let mut geometry = p
            .board
            .footprints
            .last()
            .and_then(|f| f.pads.first())
            .and_then(|p| p.geometry.clone())
            .ok_or("missing bridge geometry")?;
        geometry.size = Point::new(1.0, 1.0);
        geometry.drill = Some(pcbcraft_library::Drill {
            size: Point::new(0.4, 0.4),
            offset: Point::default(),
        });
        let far = p.board.footprints.get_mut(1).ok_or("missing far pad")?;
        far.pads[0].geometry = Some(geometry);
        assert!(airwires(&p).is_empty());
        p.board
            .footprints
            .last_mut()
            .and_then(|f| f.pads.first_mut())
            .and_then(|p| p.geometry.as_mut())
            .ok_or("missing bridge")?
            .pad_type = "smd".into();
        assert!(!airwires(&p).is_empty());
        Ok(())
    }
    #[test]
    fn real_0805_copper_clearance_uses_pad_outlines() -> Result<(), String> {
        let definition = pcbcraft_library::read_footprint(include_str!(
            "../../library/tests/fixtures/R_0805_2012Metric.kicad_mod"
        ))
        .map_err(|e| e.to_string())?;
        let mut p = Project::default();
        let mut c = component_template("resistor", 1, "R1".into(), Point::default())
            .ok_or("missing template")?;
        for (i, pin) in c.pins.iter_mut().enumerate() {
            pin.net = Some(2 + i as u64);
        }
        c.library_footprint = Some(pcbcraft_doc::LibraryFootprint {
            library: "Resistor_SMD".into(),
            definition: definition.clone(),
        });
        p.components.push(c);
        p.nets = vec![
            Net {
                id: 2,
                name: "IN".into(),
                ..Default::default()
            },
            Net {
                id: 3,
                name: "OUT".into(),
                ..Default::default()
            },
        ];
        let pads = definition
            .pads
            .iter()
            .enumerate()
            .map(|(i, g)| Pad {
                number: g.number.clone(),
                offset: g.position,
                diameter: g.size.x.min(g.size.y),
                net: Some(2 + i as u64),
                castellated: None,
                geometry: Some(g.clone()),
            })
            .collect();
        p.board.footprints.push(Footprint {
            id: 4,
            component: 1,
            position: Point::new(20.0, 20.0),
            rotation: 0.0,
            locked: None,
            pads,
        });
        p.validate()?;
        assert!(
            drc(&p).is_empty(),
            "standard 0805 pads are not a short at 0.2mm clearance"
        );
        p.board.clearance = 0.9;
        assert!(drc(&p).iter().any(|f| f.code == "pad_clearance_bound"));
        p.board.clearance = 0.2;
        p.board.tracks.push(Track {
            id: 5,
            net: 2,
            start: Point::new(19.0875, 20.0),
            end: Point::new(20.9125, 20.0),
            width: 0.25,
            layer: Layer::Front,
            arc: None,
        });
        assert!(
            drc(&p)
                .iter()
                .any(|f| f.code == "pad_clearance_bound" && f.objects.contains(&5))
        );
        Ok(())
    }
    #[test]
    fn routing_connects_islands_but_back_layer_does_not() {
        let mut p = fixture();
        assert_eq!(airwires(&p).len(), 1);
        p.board.tracks.push(Track {
            id: 20,
            net: 1,
            start: Point::new(10.0, 10.0),
            end: Point::new(30.0, 10.0),
            width: 0.25,
            layer: Layer::Back,
            arc: None,
        });
        assert_eq!(airwires(&p).len(), 1);
        if let Some(t) = p.board.tracks.first_mut() {
            t.layer = Layer::Front;
        }
        assert!(airwires(&p).is_empty());
    }
    #[test]
    fn foreign_net_crossing_is_a_short() {
        let mut p = fixture();
        p.nets.push(Net {
            id: 4,
            name: "gnd".into(),
            ..Default::default()
        });
        p.board.tracks.push(Track {
            id: 20,
            net: 1,
            start: Point::new(10.0, 10.0),
            end: Point::new(30.0, 10.0),
            width: 0.25,
            layer: Layer::Front,
            arc: None,
        });
        p.board.tracks.push(Track {
            id: 21,
            net: 4,
            start: Point::new(20.0, 5.0),
            end: Point::new(20.0, 15.0),
            width: 0.25,
            layer: Layer::Front,
            arc: None,
        });
        assert!(drc(&p).iter().any(|f| f.code == "copper_short"));
    }

    fn shaped_pad_beside_foreign_track(
        geometry: pcbcraft_library::FootprintPad,
        track: (Point, Point),
        clearance: f64,
    ) -> Vec<Finding> {
        let mut p = fixture();
        p.nets.push(Net {
            id: 4,
            name: "gnd".into(),
            target_length: None,
            length_tolerance: None,
        });
        p.board.clearance = clearance;
        if let Some(pad) = p
            .board
            .footprints
            .first_mut()
            .and_then(|f| f.pads.first_mut())
        {
            pad.diameter = geometry.size.x.min(geometry.size.y);
            pad.geometry = Some(geometry);
        }
        p.board.tracks.push(Track {
            id: 30,
            net: 4,
            start: track.0,
            end: track.1,
            width: 0.2,
            layer: Layer::Front,
            arc: None,
        });
        drc(&p)
            .into_iter()
            .filter(|f| f.objects.contains(&30) && f.code != "unrouted")
            .collect()
    }
    fn square_pad(shape: &str) -> pcbcraft_library::FootprintPad {
        pcbcraft_library::FootprintPad {
            number: "1".into(),
            pad_type: "smd".into(),
            shape: shape.into(),
            position: Point::default(),
            rotation: 0.0,
            size: Point::new(2.0, 2.0),
            layers: vec!["F.Cu".into()],
            drill: None,
            roundrect_ratio: None,
            chamfer: None,
            polygon: vec![],
            raw_source: String::new(),
        }
    }
    #[test]
    fn clearance_to_a_chamfered_pad_is_measured_from_the_chamfer_edge() {
        // The pad is a 2 mm square at (10, 10) with its bottom-right corner cut
        // 0.5 mm along each side. A track leaves diagonally from just outside
        // that corner: 0.112 mm from the square's corner, 0.466 mm from the cut.
        let mut chamfered = square_pad("chamfered");
        chamfered.chamfer = Some(pcbcraft_library::PadChamfer {
            ratio: 0.25,
            corners: pcbcraft_library::ChamferCorners {
                bottom_right: true,
                ..Default::default()
            },
        });
        let track = (Point::new(11.15, 11.15), Point::new(12.15, 12.15));
        assert!(
            shaped_pad_beside_foreign_track(chamfered.clone(), track, 0.2).is_empty(),
            "the cut corner leaves room the bounding square would not"
        );
        let squared = shaped_pad_beside_foreign_track(square_pad("rect"), track, 0.2);
        assert!(
            squared.iter().any(|f| f.message.contains("0.112 mm")),
            "the uncut square really is too close: {squared:?}"
        );
        let tight = shaped_pad_beside_foreign_track(chamfered, track, 0.5);
        assert!(
            tight.iter().any(|f| f.message.contains("0.466 mm")),
            "the reported separation is the distance to the chamfer: {tight:?}"
        );
    }
    #[test]
    fn custom_polygon_pad_clearance_follows_its_notch() {
        // A 2 mm square pad at (10, 10) with a 0.7 mm notch cut from its
        // upper-right corner.
        let mut l = square_pad("custom");
        l.size = Point::new(0.4, 0.4);
        l.polygon = vec![
            Point::new(-1.0, -1.0),
            Point::new(0.3, -1.0),
            Point::new(0.3, -0.3),
            Point::new(1.0, -0.3),
            Point::new(1.0, 1.0),
            Point::new(-1.0, 1.0),
        ];
        // Track in the notch, 0.4 mm from both inner edges, so 0.3 mm of copper gap.
        let track = (Point::new(10.7, 9.3), Point::new(10.7, 9.0));
        assert!(shaped_pad_beside_foreign_track(l.clone(), track, 0.2).is_empty());
        let tight = shaped_pad_beside_foreign_track(l.clone(), track, 0.35);
        assert!(
            tight.iter().any(|f| f.message.contains("0.300 mm")),
            "{tight:?}"
        );
        let mut p = fixture();
        if let Some(pad) = p
            .board
            .footprints
            .first_mut()
            .and_then(|f| f.pads.first_mut())
        {
            pad.diameter = 0.4;
            pad.geometry = Some(l);
            assert!((pad.contact_radius() - 0.2).abs() < 1e-12);
        }
    }

    fn panel_settings_vscore() -> pcbcraft_doc::PanelSettings {
        pcbcraft_doc::PanelSettings {
            enabled: true,
            columns: 2,
            rows: 1,
            pitch_x: 0.0,
            pitch_y: 2.0,
            rail: 5.0,
            separation: PanelSeparation::VScore,
            tab_channel: 1.6,
        }
    }
    fn panel_project(settings: pcbcraft_doc::PanelSettings) -> Project {
        let mut p = fixture();
        p.board.width = 60.0;
        p.board.height = 40.0;
        for (index, fid) in [(1, 5.0), (2, 30.0), (3, 55.0)] {
            p.board.fiducials.push(pcbcraft_doc::Fiducial {
                id: 500 + index,
                position: Point::new(fid, 2.0),
                diameter: 1.0,
                layer: Layer::Front,
            });
        }
        p.panel = settings;
        p
    }
    #[test]
    fn panel_checks_skip_when_panel_disabled() {
        let mut settings = panel_settings_vscore();
        settings.enabled = false;
        let p = panel_project(settings);
        assert!(
            drc(&p).iter().all(|f| !f.code.starts_with("panel_")),
            "disabled panel must not run fabrication checks"
        );
    }
    #[test]
    fn panel_too_large_for_assembler_flags_oversized_grids() {
        let mut settings = panel_settings_vscore();
        settings.columns = 5;
        settings.rows = 5;
        settings.pitch_x = 2.0;
        settings.pitch_y = 2.0;
        let findings = drc(&panel_project(settings));
        assert!(
            findings
                .iter()
                .any(|f| f.code == "panel_too_large_for_assembler"),
            "{findings:?}"
        );
        let mut ok = panel_settings_vscore();
        ok.columns = 2;
        ok.rows = 2;
        assert!(
            !drc(&panel_project(ok))
                .iter()
                .any(|f| f.code == "panel_too_large_for_assembler")
        );
    }
    #[test]
    fn panel_missing_fiducials_counts_rail_marks() {
        let mut p = panel_project(panel_settings_vscore());
        p.board.fiducials.clear();
        let missing = drc(&p);
        assert!(
            missing.iter().any(|f| f.code == "panel_missing_fiducials"),
            "{missing:?}"
        );
        assert!(
            !drc(&panel_project(panel_settings_vscore()))
                .iter()
                .any(|f| f.code == "panel_missing_fiducials")
        );
    }
    #[test]
    fn panel_tab_too_narrow_honours_channel_width() {
        let mut narrow = panel_settings_vscore();
        narrow.separation = PanelSeparation::Tab;
        narrow.tab_channel = 1.0;
        assert!(
            drc(&panel_project(narrow))
                .iter()
                .any(|f| f.code == "panel_tab_too_narrow")
        );
        let mut ok = panel_settings_vscore();
        ok.separation = PanelSeparation::Tab;
        ok.tab_channel = 1.8;
        assert!(
            !drc(&panel_project(ok))
                .iter()
                .any(|f| f.code == "panel_tab_too_narrow")
        );
    }
    #[test]
    fn panel_copper_near_score_measures_v_score_clearance() {
        let mut p = panel_project(panel_settings_vscore());
        p.board.tracks.push(Track {
            id: 90,
            net: 1,
            start: Point::new(59.5, 10.0),
            end: Point::new(59.9, 10.0),
            width: 0.2,
            layer: Layer::Front,
            arc: None,
        });
        assert!(
            drc(&p).iter().any(|f| f.code == "panel_copper_near_score"),
            "track hugging the score line should fail"
        );
        let mut ok = panel_project(panel_settings_vscore());
        ok.board.tracks.push(Track {
            id: 91,
            net: 1,
            start: Point::new(50.0, 10.0),
            end: Point::new(55.0, 10.0),
            width: 0.25,
            layer: Layer::Front,
            arc: None,
        });
        assert!(!drc(&ok).iter().any(|f| f.code == "panel_copper_near_score"));
    }
    #[test]
    fn panel_component_near_score_measures_part_body_clearance() {
        let mut p = panel_project(panel_settings_vscore());
        if let Some(f) = p.board.footprints.first_mut() {
            f.position = Point::new(59.0, 10.0);
        }
        assert!(
            drc(&p)
                .iter()
                .any(|f| f.code == "panel_component_near_score")
        );
        let ok = panel_project(panel_settings_vscore());
        assert!(
            !drc(&ok)
                .iter()
                .any(|f| f.code == "panel_component_near_score")
        );
    }
    #[test]
    fn drc_clearance_when_track_crosses_footprint_unplated_drill() -> Result<(), String> {
        let thru = |number: &str, x: f64| pcbcraft_library::FootprintPad {
            number: number.into(),
            pad_type: "thru_hole".into(),
            shape: "circle".into(),
            position: Point::new(x, 0.0),
            rotation: 0.0,
            size: Point::new(1.6, 1.6),
            layers: vec!["*.Cu".into(), "*.Mask".into()],
            drill: Some(pcbcraft_library::Drill {
                size: Point::new(0.8, 0.8),
                offset: Point::default(),
            }),
            roundrect_ratio: None,
            chamfer: None,
            polygon: vec![],
            raw_source: String::new(),
        };
        let peg = pcbcraft_library::FootprintPad {
            number: String::new(),
            pad_type: "unplated".into(),
            shape: "circle".into(),
            position: Point::new(0.0, 0.0),
            rotation: 0.0,
            size: Point::default(),
            layers: vec![],
            drill: Some(pcbcraft_library::Drill {
                size: Point::new(3.0, 3.0),
                offset: Point::default(),
            }),
            roundrect_ratio: None,
            chamfer: None,
            polygon: vec![],
            raw_source: String::new(),
        };
        let definition = pcbcraft_library::FootprintDefinition {
            name: "PegOnly".into(),
            description: String::new(),
            keywords: String::new(),
            pads: vec![thru("1", -2.54), thru("2", 2.54), peg.clone()],
            graphics: vec![],
            model: None,
            rules: None,
            raw_source: String::new(),
            unsupported: vec![],
        };
        let mut p = Project::default();
        p.nets.push(Net {
            id: 1,
            name: "SIG".into(),
            ..Default::default()
        });
        let mut component =
            component_template("resistor", 2, "R1".into(), Point::default()).ok_or("template")?;
        component.library_footprint = Some(pcbcraft_doc::LibraryFootprint {
            library: "test".into(),
            definition: definition.clone(),
        });
        p.components.push(component);
        p.board.footprints.push(Footprint {
            id: 4,
            component: 2,
            position: Point::new(30.0, 20.0),
            rotation: 0.0,
            locked: None,
            pads: vec![
                Pad {
                    number: "1".into(),
                    offset: Point::new(-2.54, 0.0),
                    diameter: 1.6,
                    net: None,
                    castellated: None,
                    geometry: Some(definition.pads[0].clone()),
                },
                Pad {
                    number: "2".into(),
                    offset: Point::new(2.54, 0.0),
                    diameter: 1.6,
                    net: None,
                    castellated: None,
                    geometry: Some(definition.pads[1].clone()),
                },
                Pad {
                    number: String::new(),
                    offset: peg.position,
                    diameter: 0.0,
                    net: None,
                    castellated: None,
                    geometry: Some(peg),
                },
            ],
        });
        p.board.tracks.push(Track {
            id: 5,
            net: 1,
            start: Point::new(20.0, 20.0),
            end: Point::new(40.0, 20.0),
            width: 0.2,
            layer: Layer::Front,
            arc: None,
        });
        p.validate()?;
        let findings = drc(&p);
        assert!(
            findings.iter().any(|f| {
                (f.code == "clearance" || f.code == "copper_short")
                    && f.objects.contains(&4)
                    && f.objects.contains(&5)
            }),
            "a track through a peg drill is a clearance finding: {findings:?}"
        );
        Ok(())
    }

    #[test]
    fn slot_clearance_is_measured_along_its_length() -> Result<(), String> {
        let mut p = fixture();
        p.board.clearance = 0.2;
        p.board.holes.push(pcbcraft_doc::Hole {
            id: 40,
            position: Point::new(14.0, 10.0),
            diameter: 2.0,
            end: Some(Point::new(20.0, 10.0)),
        });
        p.board.tracks.push(Track {
            id: 41,
            net: 1,
            start: Point::new(21.1, 8.0),
            end: Point::new(21.1, 12.0),
            width: 0.2,
            layer: Layer::Front,
            arc: None,
        });
        p.validate()?;
        let findings = drc(&p);
        assert!(
            findings
                .iter()
                .any(|f| f.objects.contains(&40) && f.code != "board_edge"),
            "a track beside a slot's end centre is a clearance violation: {findings:?}"
        );
        p.board.tracks[0].start = Point::new(15.0, 12.0);
        p.board.tracks[0].end = Point::new(19.0, 12.0);
        let slot_only: Vec<Finding> = drc(&p)
            .into_iter()
            .filter(|f| f.objects.contains(&40))
            .collect();
        assert!(
            slot_only.is_empty(),
            "copper clear of the slot's side has room: {slot_only:?}"
        );
        Ok(())
    }

    #[test]
    fn annular_ring_uses_separate_via_and_pth_limits() -> Result<(), String> {
        let mut p = fixture();
        p.board.rules = Some(FabRules {
            min_ring_via: Some(0.2),
            min_ring_pth: Some(0.35),
            ..FabRules::default()
        });
        p.board.vias.push(Via {
            id: 99,
            net: 1,
            position: Point::new(5.0, 5.0),
            diameter: 0.8,
            drill: 0.4,
        });
        let fp_id = p.board.footprints[0].id;
        if let Some(pad) = p.board.footprints[0].pads.first_mut() {
            pad.diameter = 1.0;
            pad.geometry = Some(FootprintPad {
                number: "1".into(),
                pad_type: "thru_hole".into(),
                shape: "circle".into(),
                size: Point::new(1.0, 1.0),
                position: Point::default(),
                rotation: 0.0,
                layers: vec!["F.Cu".into(), "B.Cu".into()],
                drill: Some(Drill {
                    size: Point::new(0.4, 0.4),
                    offset: Point::default(),
                }),
                roundrect_ratio: None,
                chamfer: None,
                polygon: vec![],
                raw_source: String::new(),
            });
        }
        let findings = drc(&p);
        assert!(
            !findings
                .iter()
                .any(|f| f.code == "annular_ring" && f.objects.contains(&99)),
            "via ring passes the via limit"
        );
        assert!(
            findings
                .iter()
                .any(|f| f.code == "annular_ring" && f.objects.contains(&fp_id)),
            "the same ring fails the plated-hole limit"
        );
        Ok(())
    }

    /// A project with one drawn layer holding a single figure, for the drawn
    /// layer checks.
    fn drawn_layer(kind: ArtKind, shape: ArtShape) -> Project {
        let mut p = Project::default();
        p.nets.push(Net {
            id: 1,
            name: "signal".into(),
            ..Default::default()
        });
        p.board.user_layers.push(pcbcraft_doc::ArtLayer {
            id: 40,
            name: "Card Outline".into(),
            kind,
            dotted: false,
            dot_pitch: 1.0,
            dot_size: 0.5,
            visible: true,
            shapes: vec![shape],
        });
        p
    }
    fn codes(p: &Project) -> Vec<&'static str> {
        drc(p).into_iter().map(|f| f.code).collect()
    }
    fn test_via(p: &mut Project) {
        p.board.vias.push(Via {
            id: 20,
            net: 1,
            position: Point::new(20.0, 20.0),
            diameter: 0.8,
            drill: 0.4,
        });
    }
    /// A project holding one surface pad at (20, 20) that names `layers` and
    /// carries a 2 mm square copper outline.
    fn masked_pad(layers: &[&str]) -> Project {
        let mut p = Project::default();
        p.nets.push(Net {
            id: 1,
            name: "signal".into(),
            ..Default::default()
        });
        p.board.footprints.push(Footprint {
            id: 11,
            component: 1,
            position: Point::new(20.0, 20.0),
            rotation: 0.0,
            locked: None,
            pads: vec![Pad {
                number: "1".into(),
                offset: Point::default(),
                diameter: 2.0,
                net: Some(1),
                castellated: None,
                geometry: Some(pcbcraft_library::FootprintPad {
                    number: "1".into(),
                    pad_type: "smd".into(),
                    shape: "rect".into(),
                    position: Point::default(),
                    rotation: 0.0,
                    size: Point::new(2.0, 2.0),
                    layers: layers.iter().map(|l| (*l).to_string()).collect(),
                    drill: None,
                    roundrect_ratio: None,
                    chamfer: None,
                    polygon: vec![],
                    raw_source: String::new(),
                }),
            }],
        });
        p
    }
    #[test]
    fn cut_line_through_a_via_is_a_finding() -> Result<(), String> {
        let mut p = drawn_layer(
            ArtKind::Cut,
            ArtShape::Line {
                start: Point::new(18.0, 20.0),
                end: Point::new(22.0, 20.0),
            },
        );
        test_via(&mut p);
        let found = drc(&p);
        let hit = found
            .iter()
            .find(|f| f.code == "cut_hits_via")
            .ok_or("a cut across a via should be reported")?;
        assert_eq!(hit.severity, "error");
        assert_eq!(hit.objects, vec![40, 20]);
        assert!(
            hit.message.contains("line 1 on Card Outline"),
            "the finding names the figure and its layer: {}",
            hit.message
        );
        Ok(())
    }
    #[test]
    fn cut_line_clear_of_a_via_is_clean() {
        let mut p = drawn_layer(
            ArtKind::Cut,
            ArtShape::Line {
                start: Point::new(10.0, 30.0),
                end: Point::new(20.0, 30.0),
            },
        );
        test_via(&mut p);
        assert!(!codes(&p).contains(&"cut_hits_via"));
    }
    #[test]
    fn perforation_through_a_rotated_rect_pad_uses_the_true_outline() -> Result<(), String> {
        let pad = |shape: ArtShape| {
            let mut p = masked_pad(&["F.Cu", "F.Mask"]);
            if let Some(pad) = p
                .board
                .footprints
                .first_mut()
                .and_then(|f| f.pads.first_mut())
                && let Some(g) = pad.geometry.as_mut()
            {
                g.rotation = 45.0;
            }
            p.board.user_layers.push(pcbcraft_doc::ArtLayer {
                id: 40,
                name: "Snap Line".into(),
                kind: ArtKind::Perforation,
                dotted: true,
                dot_pitch: 1.0,
                dot_size: 0.5,
                visible: true,
                shapes: vec![shape],
            });
            p
        };
        // Straight through the middle of the diamond.
        let through = pad(ArtShape::Line {
            start: Point::new(19.0, 20.0),
            end: Point::new(21.0, 20.0),
        });
        assert!(
            codes(&through).contains(&"cut_hits_via"),
            "a perforation through the pad is reported"
        );
        // Across the empty corner of the bounding square: 0.41 mm from the
        // rotated edge, so the true outline leaves room the square would not.
        let graze = pad(ArtShape::Line {
            start: Point::new(21.0, 21.0),
            end: Point::new(21.3, 21.3),
        });
        assert!(
            !codes(&graze).contains(&"cut_hits_via"),
            "the empty corner of the square is outside the true outline"
        );
        Ok(())
    }
    #[test]
    fn cut_through_a_mounting_hole_is_clean() {
        let mut p = drawn_layer(
            ArtKind::Cut,
            ArtShape::Line {
                start: Point::new(18.0, 20.0),
                end: Point::new(22.0, 20.0),
            },
        );
        p.board.holes.push(pcbcraft_doc::Hole {
            id: 25,
            position: Point::new(20.0, 20.0),
            diameter: 3.0,
            end: None,
        });
        assert!(
            !codes(&p).contains(&"cut_hits_via"),
            "cutting through a mounting hole is normal"
        );
    }
    #[test]
    fn each_via_is_reported_once_not_per_copper_side() {
        let mut p = drawn_layer(
            ArtKind::Cut,
            ArtShape::Line {
                start: Point::new(18.0, 20.0),
                end: Point::new(22.0, 20.0),
            },
        );
        test_via(&mut p);
        let hits = drc(&p)
            .into_iter()
            .filter(|f| f.code == "cut_hits_via")
            .count();
        assert_eq!(hits, 1, "one cut crosses one via, on every layer");
    }
    #[test]
    fn mask_rect_over_an_exposed_pad_is_a_finding() -> Result<(), String> {
        let mut p = masked_pad(&["F.Cu", "F.Mask"]);
        p.board.user_layers.push(pcbcraft_doc::ArtLayer {
            id: 40,
            name: "Logo Mask".into(),
            kind: ArtKind::Mask,
            dotted: false,
            dot_pitch: 1.0,
            dot_size: 0.5,
            visible: true,
            shapes: vec![ArtShape::Rect {
                x: 19.0,
                y: 19.0,
                width: 2.0,
                height: 2.0,
                rotate: 0.0,
            }],
        });
        let hit = drc(&p)
            .into_iter()
            .find(|f| f.code == "mask_covers_pad")
            .ok_or("a mask figure over a pad opening should be reported")?;
        assert_eq!(hit.severity, "warning");
        assert_eq!(hit.objects, vec![40, 11]);
        assert!(
            hit.message.contains("rectangle 1 on Logo Mask") && hit.message.contains("top side"),
            "the finding names the figure and the side it assumes: {}",
            hit.message
        );
        Ok(())
    }
    #[test]
    fn mask_over_a_pad_without_a_mask_opening_is_clean() {
        let mut p = masked_pad(&["F.Cu"]);
        p.board.user_layers.push(pcbcraft_doc::ArtLayer {
            id: 40,
            name: "Logo Mask".into(),
            kind: ArtKind::Mask,
            dotted: false,
            dot_pitch: 1.0,
            dot_size: 0.5,
            visible: true,
            shapes: vec![ArtShape::Rect {
                x: 19.0,
                y: 19.0,
                width: 2.0,
                height: 2.0,
                rotate: 0.0,
            }],
        });
        assert!(!codes(&p).contains(&"mask_covers_pad"));
    }
    #[test]
    fn mask_line_has_no_area_and_is_ignored() {
        let mut p = masked_pad(&["F.Cu", "F.Mask"]);
        p.board.user_layers.push(pcbcraft_doc::ArtLayer {
            id: 40,
            name: "Logo Mask".into(),
            kind: ArtKind::Mask,
            dotted: false,
            dot_pitch: 1.0,
            dot_size: 0.5,
            visible: true,
            shapes: vec![ArtShape::Line {
                start: Point::new(18.0, 20.0),
                end: Point::new(22.0, 20.0),
            }],
        });
        assert!(
            !codes(&p).contains(&"mask_covers_pad"),
            "a bare mask stroke has no area to cover an opening"
        );
    }
    #[test]
    fn cut_on_the_traced_board_edge_is_not_off_board() {
        let mut p = drawn_layer(
            ArtKind::Cut,
            ArtShape::Line {
                start: Point::new(0.0, 0.0),
                end: Point::new(60.0, 0.0),
            },
        );
        p.board.outline = Some(vec![
            Point::new(0.0, 0.0),
            Point::new(60.0, 0.0),
            Point::new(60.0, 40.0),
            Point::new(0.0, 40.0),
        ]);
        assert!(
            !codes(&p).contains(&"art_off_board"),
            "a card outline drawn on the edge is the normal case"
        );
    }
    #[test]
    fn figure_outside_the_board_rectangle_is_off_board() -> Result<(), String> {
        let p = drawn_layer(
            ArtKind::Cut,
            ArtShape::Line {
                start: Point::new(58.0, 20.0),
                end: Point::new(62.0, 20.0),
            },
        );
        let hit = drc(&p)
            .into_iter()
            .find(|f| f.code == "art_off_board")
            .ok_or("a cut that leaves the board should be reported")?;
        assert_eq!(hit.severity, "warning");
        assert_eq!(hit.objects, vec![40]);
        assert!(
            hit.message.contains("past the board edge"),
            "the finding quotes the overshoot: {}",
            hit.message
        );
        Ok(())
    }
    #[test]
    fn severity_override_and_exclusion_silence_the_drawn_layer_codes() {
        let mut p = drawn_layer(
            ArtKind::Cut,
            ArtShape::Line {
                start: Point::new(18.0, 20.0),
                end: Point::new(22.0, 20.0),
            },
        );
        test_via(&mut p);
        p.rule_severity.insert("cut_hits_via".into(), "off".into());
        assert!(!codes(&p).contains(&"cut_hits_via"));
        p.rule_severity
            .insert("cut_hits_via".into(), "warning".into());
        assert!(
            drc(&p)
                .iter()
                .any(|f| f.code == "cut_hits_via" && f.severity == "warning"),
            "a re-levelled check reports at the chosen level"
        );
        p.rule_severity.remove("cut_hits_via");
        p.rule_exclusions
            .insert("cut_hits_via".into(), vec![40, 20]);
        assert!(!codes(&p).contains(&"cut_hits_via"));
        p.rule_exclusions.clear();
        let mut edge = drawn_layer(
            ArtKind::Cut,
            ArtShape::Line {
                start: Point::new(58.0, 20.0),
                end: Point::new(62.0, 20.0),
            },
        );
        edge.rule_severity
            .insert("art_off_board".into(), "off".into());
        assert!(!codes(&edge).contains(&"art_off_board"));
    }
    #[test]
    fn region_exclusion_covers_a_drawn_layer_finding() {
        let mut p = drawn_layer(
            ArtKind::Cut,
            ArtShape::Line {
                start: Point::new(18.0, 20.0),
                end: Point::new(22.0, 20.0),
            },
        );
        test_via(&mut p);
        p.rule_regions.insert(
            "cut_hits_via".into(),
            vec![pcbcraft_doc::RuleRegion {
                x: 10.0,
                y: 10.0,
                width: 20.0,
                height: 20.0,
            }],
        );
        assert!(
            !codes(&p).contains(&"cut_hits_via"),
            "the layer anchor and the via both sit inside the region"
        );
        p.rule_regions.insert(
            "cut_hits_via".into(),
            vec![pcbcraft_doc::RuleRegion {
                x: 30.0,
                y: 30.0,
                width: 5.0,
                height: 5.0,
            }],
        );
        assert!(
            codes(&p).contains(&"cut_hits_via"),
            "a region elsewhere leaves the finding alone"
        );
    }
}
