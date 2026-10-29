//! Initial electrical and geometric checks, with object-addressable diagnostics.
use pcbcraft_doc::{
    Board, Id, Layer, PANEL_ASSEMBLER_MAX_MM, PANEL_FIDUCIAL_COUNT, PANEL_SCORE_COMPONENT_MIN_MM,
    PANEL_SCORE_COPPER_MIN_MM, PANEL_TAB_CHANNEL_MIN_MM, PanelSeparation, Project, panel_layout,
    panel_score_lines,
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
/// The largest clearance any rule in the project can demand. Two copper items
/// farther apart than this, plus their own reach, cannot produce a clearance
/// finding, which is what the broad phase below uses to skip them.
fn widest_clearance(project: &Project, items: &[Copper]) -> f64 {
    let mut widest = project.board.clearance;
    for class in &project.net_classes {
        widest = widest.max(class.clearance);
    }
    for rule in &project.board.pair_rules {
        widest = widest.max(rule.clearance);
    }
    for area in &project.board.rule_areas {
        widest = widest.max(area.clearance);
    }
    for item in items {
        if let Some(clearance) = item.clearance {
            widest = widest.max(clearance);
        }
    }
    if widest.is_finite() {
        widest.max(0.0)
    } else {
        0.0
    }
}

/// The box around one copper item's own geometry, before any growth.
fn span(item: &Copper) -> (Point, Point) {
    let mut low = Point::new(item.start.x.min(item.end.x), item.start.y.min(item.end.y));
    let mut high = Point::new(item.start.x.max(item.end.x), item.start.y.max(item.end.y));
    for point in item.outline.iter().flatten() {
        low = Point::new(low.x.min(point.x), low.y.min(point.y));
        high = Point::new(high.x.max(point.x), high.y.max(point.y));
    }
    (low, high)
}

/// Items whose grown box covers more than this many cells are compared with
/// every other item instead of being listed in each cell: a zone outline can be
/// far larger than the cell size, and listing it everywhere would cost more
/// than the pairs it saves.
const WIDE_ITEM_CELLS: usize = 4096;

/// A uniform grid over the board used to find the copper pairs that could
/// touch. Each item goes into every cell its box covers once grown by what its
/// copper reaches (radius, outline error and the widest required clearance).
/// Two grown boxes that overlap share a cell, so a pair the exact `gap` test
/// would report is never skipped, and unrelated copper is never compared.
struct Grid {
    cell: f64,
    min: Point,
    cols: usize,
    rows: usize,
    cells: Vec<Vec<usize>>,
    /// Items too large to list per cell; compared with everything.
    wide: Vec<usize>,
    /// One candidate list per item, in item order.
    near: Vec<Vec<usize>>,
}

impl Grid {
    fn new(items: &[Copper], margin: f64) -> Grid {
        let mut low = Point::new(f64::INFINITY, f64::INFINITY);
        let mut high = Point::new(f64::NEG_INFINITY, f64::NEG_INFINITY);
        let mut widest_reach: f64 = 0.0;
        for item in items {
            let (item_low, item_high) = span(item);
            low = Point::new(low.x.min(item_low.x), low.y.min(item_low.y));
            high = Point::new(high.x.max(item_high.x), high.y.max(item_high.y));
            widest_reach = widest_reach.max(item.radius + item.outline_error);
        }
        if !low.x.is_finite() || !high.x.is_finite() || !low.y.is_finite() || !high.y.is_finite() {
            low = Point::default();
            high = Point::default();
        }
        let reach = if widest_reach.is_finite() {
            widest_reach + margin
        } else {
            margin
        };
        let cell = (2.0 * reach).max(1.0);
        let cols = ((((high.x - low.x) / cell).ceil() as usize) + 1).clamp(1, 4096);
        let rows = ((((high.y - low.y) / cell).ceil() as usize) + 1).clamp(1, 4096);
        let mut grid = Grid {
            cell,
            min: low,
            cols,
            rows,
            cells: vec![Vec::new(); cols * rows],
            wide: Vec::new(),
            near: vec![Vec::new(); items.len()],
        };
        for (index, item) in items.iter().enumerate() {
            let (item_low, item_high) = span(item);
            let pad = item.radius + item.outline_error + margin;
            let grown = (
                Point::new(item_low.x - pad, item_low.y - pad),
                Point::new(item_high.x + pad, item_high.y + pad),
            );
            grid.insert(index, grown.0, grown.1);
        }
        grid.collect_neighbours();
        grid
    }

    fn column(&self, x: f64) -> usize {
        let width = self.cols.saturating_sub(1) as f64;
        (((x - self.min.x) / self.cell).floor()).clamp(0.0, width) as usize
    }

    fn row(&self, y: f64) -> usize {
        let height = self.rows.saturating_sub(1) as f64;
        (((y - self.min.y) / self.cell).floor()).clamp(0.0, height) as usize
    }

    fn insert(&mut self, index: usize, low: Point, high: Point) {
        let (first, last) = (self.column(low.x), self.column(high.x));
        let (bottom, top) = (self.row(low.y), self.row(high.y));
        let count = (last - first + 1).saturating_mul(top - bottom + 1);
        if count > WIDE_ITEM_CELLS {
            self.wide.push(index);
            return;
        }
        for y in bottom..=top {
            for x in first..=last {
                self.cells[y * self.cols + x].push(index);
            }
        }
    }

    /// Fill each item's candidate list with the items sharing a cell with it,
    /// sorted and without repeats, so the pair scan below visits candidates in
    /// the same order a full scan would and reports findings in the same order.
    fn collect_neighbours(&mut self) {
        for bucket in &self.cells {
            for (at, &item) in bucket.iter().enumerate() {
                for &other in &bucket[at + 1..] {
                    if let Some(list) = self.near.get_mut(item) {
                        list.push(other);
                    }
                    if let Some(list) = self.near.get_mut(other) {
                        list.push(item);
                    }
                }
            }
        }
        let wide = self.wide.clone();
        for &item in &wide {
            for other in 0..self.near.len() {
                if other == item {
                    continue;
                }
                if let Some(list) = self.near.get_mut(item) {
                    list.push(other);
                }
                if let Some(list) = self.near.get_mut(other) {
                    list.push(item);
                }
            }
        }
        for list in &mut self.near {
            list.sort_unstable();
            list.dedup();
        }
    }

    /// The pairs the exact test still has to look at, counted once each.
    fn pair_count(&self) -> usize {
        self.near
            .iter()
            .enumerate()
            .map(|(item, list)| list.iter().filter(|other| **other > item).count())
            .sum()
    }
}

/// How many pairs the copper broad phase hands to the exact clearance test on
/// this board, and how many copper items it holds. The large-board bench uses
/// the first to check the broad phase stays a small multiple of the second.
pub fn copper_pair_candidates(project: &Project) -> (usize, usize) {
    let items = copper(project);
    let margin = widest_clearance(project, &items);
    let grid = Grid::new(&items, margin);
    (grid.pair_count(), items.len())
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
    let grid = Grid::new(&items, 0.0);
    let mut parents: Vec<_> = (0..items.len()).collect();
    for (i, candidates) in grid.near.iter().enumerate() {
        let Some(a) = items.get(i) else { continue };
        for &j in candidates.iter().filter(|other| **other > i) {
            let Some(b) = items.get(j) else { continue };
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
        if islands.is_empty() {
            continue;
        }
        // Each pair of islands is measured once, and each unreached island
        // keeps its nearest reached one, so a board with many islands does not
        // re-measure every pair on every connection. Distances and island
        // order break ties the way the plain rescan did, so the airwires are
        // the same ones it produced.
        let count = islands.len();
        // Squared distances: only their order matters here, and the square
        // root is the most expensive part of measuring a pair.
        let squared = |a: &Copper, b: &Copper| {
            let (dx, dy) = (a.start.x - b.start.x, a.start.y - b.start.y);
            dx * dx + dy * dy
        };
        let mut nearest = vec![vec![None; count]; count];
        for (i, a_items) in islands.values().enumerate() {
            for (j, b_items) in islands.values().enumerate().skip(i + 1) {
                let mut best: Option<(f64, &Copper, &Copper)> = None;
                for a in a_items {
                    for b in b_items {
                        let d = squared(a, b);
                        if best.as_ref().is_none_or(|(seen, _, _)| d < *seen) {
                            best = Some((d, a, b));
                        }
                    }
                }
                if let Some((d, a, b)) = best {
                    nearest[i][j] = Some((d, a, b));
                    nearest[j][i] = Some((d, a, b));
                }
            }
        }
        // Prim's walk over the island graph. Each unreached island keeps the
        // nearest reached island, ties going to the lower index, so the next
        // connection is chosen exactly as the plain rescan chose it, without
        // looking at every pair again.
        let mut chosen = vec![false; count];
        let mut best: Vec<Option<(f64, usize)>> = vec![None; count];
        let welcome = |island: usize, best: &mut Vec<Option<(f64, usize)>>, chosen: &[bool]| {
            for (other, entry) in nearest[island].iter().enumerate() {
                if chosen.get(other).copied().unwrap_or(true) {
                    continue;
                }
                let Some((distance, _, _)) = entry else {
                    continue;
                };
                let keep = match best.get(other).copied().flatten() {
                    Some((seen, from)) => *distance < seen || (*distance == seen && island < from),
                    None => true,
                };
                if keep {
                    best[other] = Some((*distance, island));
                }
            }
        };
        chosen[0] = true;
        welcome(0, &mut best, &chosen);
        while chosen.iter().filter(|at| **at).count() < count {
            let mut pick: Option<(f64, usize, usize)> = None;
            for (other, entry) in best.iter().enumerate() {
                if chosen.get(other).copied().unwrap_or(true) {
                    continue;
                }
                let Some((distance, from)) = entry else {
                    continue;
                };
                if pick.is_none_or(|(best_distance, best_from, _)| {
                    *distance < best_distance || (*distance == best_distance && *from < best_from)
                }) {
                    pick = Some((*distance, *from, other));
                }
            }
            let Some((_, from, other)) = pick else {
                break;
            };
            let Some((_, a, b)) = nearest
                .get(from)
                .and_then(|row| row.get(other))
                .copied()
                .flatten()
            else {
                break;
            };
            chosen[other] = true;
            result.push(Airwire {
                net,
                from: a.start,
                to: b.start,
                from_object: a.id,
                to_object: b.id,
            });
            welcome(other, &mut best, &chosen);
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

pub fn drc(project: &Project) -> Vec<Finding> {
    let mut result = vec![];
    let items = copper(project);
    let board = &project.board;
    // The exact clearance test below is unchanged; the grid only decides which
    // pairs it is asked about, and asks in the same order a full scan would.
    let grid = Grid::new(&items, widest_clearance(project, &items));
    for (i, a) in items.iter().enumerate() {
        let candidates = grid.near.get(i).map(Vec::as_slice).unwrap_or_default();
        for &j in candidates.iter().filter(|other| **other > i) {
            let Some(b) = items.get(j) else { continue };
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
    result.retain(|f| {
        !project.rule_disabled(f.code)
            && !project.rule_excluded(f.code, &f.objects)
            && !project.rule_region_excluded(f.code, &f.objects)
    });
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use pcbcraft_doc::{
        Footprint, Net, NoConnectFlag, Pad, PinRef, Track, Via, component_template,
    };
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

    /// A small deterministic generator, so the equivalence test below covers
    /// thousands of boards without a random-number dependency.
    fn scatter(seed: u64) -> Project {
        let mut state = seed | 1;
        let mut next = |limit: f64| -> f64 {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            ((state >> 33) as f64 / (u32::MAX as f64)) * limit
        };
        let mut p = Project::default();
        for id in 1..=3 {
            p.nets.push(Net {
                id,
                name: format!("net {id}"),
                ..Default::default()
            });
        }
        for id in 10..40 {
            let net = 1 + (next(3.0) as Id);
            let start = Point::new(next(40.0), next(40.0));
            let along = next(6.0);
            let end = Point::new(start.x + along, start.y + next(2.0) - 1.0);
            let layer = if next(2.0) < 1.0 {
                Layer::Front
            } else {
                Layer::Back
            };
            p.board.tracks.push(Track {
                id,
                net,
                start,
                end,
                width: 0.2 + next(0.6),
                layer,
                arc: None,
            });
        }
        for id in 100..108 {
            let radius = 0.3 + next(0.8);
            p.board.footprints.push(Footprint {
                id,
                component: 0,
                position: Point::new(next(40.0), next(40.0)),
                rotation: 0.0,
                locked: None,
                pads: vec![Pad {
                    number: "1".into(),
                    offset: Point::default(),
                    diameter: radius * 2.0,
                    net: Some(1 + (next(3.0) as Id)),
                    castellated: None,
                    geometry: None,
                }],
            });
        }
        p
    }

    /// The grid never skips a pair the exact clearance test would report, and
    /// the checks that report such pairs produce the same findings a full scan
    /// of every pair would.
    #[test]
    fn broad_phase_finds_the_same_findings_as_the_pair_scan() -> Result<(), String> {
        for seed in 1..=64 {
            let p = scatter(seed);
            let items = copper(&p);
            let margin = widest_clearance(&p, &items);
            let grid = Grid::new(&items, margin);
            let mut reported = BTreeSet::new();
            for (i, list) in grid.near.iter().enumerate() {
                for &j in list {
                    reported.insert((i.min(j), i.max(j)));
                }
            }
            // Every pair the exact test would report is in the candidate set.
            for (i, a) in items.iter().enumerate() {
                for (j, b) in items.iter().enumerate().skip(i + 1) {
                    if a.bridge.is_some() && a.bridge == b.bridge {
                        continue;
                    }
                    if (a.net == b.net && a.net != 0) || a.layer != b.layer {
                        continue;
                    }
                    let mid = |c: &Copper| {
                        Point::new((c.start.x + c.end.x) / 2.0, (c.start.y + c.end.y) / 2.0)
                    };
                    let required = p
                        .required_clearance_at(a.net, b.net, mid(a), mid(b), a.layer, b.layer)
                        .max(a.clearance.unwrap_or(0.0))
                        .max(b.clearance.unwrap_or(0.0));
                    let violation = gap(a, b) + 1e-6 < required;
                    if violation {
                        assert!(
                            reported.contains(&(i, j)),
                            "seed {seed}: the grid skipped copper {i} and {j}"
                        );
                    }
                }
            }
            // And the findings themselves match the pair scan: every violating
            // pair appears in the report, with the same order of objects.
            let findings = drc(&p);
            for (i, list) in grid.near.iter().enumerate() {
                for &j in list {
                    if j <= i {
                        continue;
                    }
                    let (a, b) = (&items[i], &items[j]);
                    let mid = |c: &Copper| {
                        Point::new((c.start.x + c.end.x) / 2.0, (c.start.y + c.end.y) / 2.0)
                    };
                    let required = p
                        .required_clearance_at(a.net, b.net, mid(a), mid(b), a.layer, b.layer)
                        .max(a.clearance.unwrap_or(0.0))
                        .max(b.clearance.unwrap_or(0.0));
                    if a.bridge.is_some() && a.bridge == b.bridge {
                        continue;
                    }
                    if (a.net == b.net && a.net != 0) || a.layer != b.layer {
                        continue;
                    }
                    if gap(a, b) + 1e-6 < required {
                        assert!(
                            findings.iter().any(|f| f.objects == vec![a.id, b.id]),
                            "seed {seed}: no finding names copper {} and {}",
                            a.id,
                            b.id
                        );
                    }
                }
            }
            // Airwires: one per extra island per net, as a full pair scan finds.
            let airwires = airwires(&p);
            let mut parent: Vec<usize> = (0..items.len()).collect();
            for (i, a) in items.iter().enumerate() {
                for (j, b) in items.iter().enumerate().skip(i + 1) {
                    if touching(a, b) {
                        let (ra, rb) = (root(&parent, i), root(&parent, j));
                        if let Some(slot) = parent.get_mut(rb) {
                            *slot = ra;
                        }
                    }
                }
            }
            let mut islands: BTreeMap<Id, BTreeSet<usize>> = BTreeMap::new();
            for (i, item) in items
                .iter()
                .enumerate()
                .filter(|(_, c)| c.pad && c.net != 0)
            {
                islands
                    .entry(item.net)
                    .or_default()
                    .insert(root(&parent, i));
            }
            let expected: usize = islands
                .values()
                .map(|set| set.len().saturating_sub(1))
                .sum();
            assert_eq!(
                airwires.len(),
                expected,
                "seed {seed}: the grid should find the same islands a full scan does"
            );
        }
        Ok(())
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
}
