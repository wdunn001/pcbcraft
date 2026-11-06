//! Which shape the Layout tab is editing, and the commands that change it.
//!
//! Context: the board is a free-form shape and each copper layer may have a
//! shape of its own (see `crates/doc/src/layer_shape.rs`). The active copper
//! layer decides what the outline handles on the canvas edit: its own shape
//! when it has one, the board shape when it follows the board.
//! Local rules: every change is an engine command. A layer shape goes through
//! `board.layer_shape_set` and `board.layer_shape_clear`. The board shape goes
//! through the `board.outline_*` commands and `board.set_outline`. Sub-layers
//! (silkscreen, mask, paste, pads, tracks, zones) have no shape; they follow
//! their copper layer and are never offered an action here.
//!
//! # Dependencies
//! - crates/doc/src/layer_shape.rs (`Board::shape_of`, `LayerShape`)
//! - crates/engine/src/layer_shape.rs (`board.layer_shape_set`, `board.layer_shape_clear`)
//! - crates/engine/src/outline.rs (`board.outline_set_vertex`, `board.outline_set_arc`)
//! - crates/ui-egui/src/outline_arcs.rs (the handles and drags that call these)
//!
//! # Do not
//! - Write `Board.layer_shapes` or `Board.outline` directly; run a command.
//! - Offer a shape action for a sub-layer.
use super::*;
use pcbcraft_doc::{Layer, rectangle_outline, segment_arc};
use pcbcraft_geom::Point;

/// Most corners a shape may have; the engine refuses more.
pub(crate) const MAX_CORNERS: usize = 64;

/// One shape the canvas draws besides the board shape.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PlannedShape {
    /// Copper layer key such as `F.Cu`.
    pub layer: String,
    /// The active layer's shape: drawn solid. The others are drawn dashed.
    pub active: bool,
}

/// True when the corners, in order, make an axis-aligned rectangle with no arcs.
fn is_rectangle(points: &[Point], arcs: &[Option<Point>]) -> bool {
    if points.len() != 4 || arcs.iter().any(Option::is_some) {
        return false;
    }
    let same = |a: f64, b: f64| (a - b).abs() < 1e-9;
    let horizontal: Vec<bool> = (0..4)
        .map(|i| same(points[i].y, points[(i + 1) % 4].y))
        .collect();
    let vertical: Vec<bool> = (0..4)
        .map(|i| same(points[i].x, points[(i + 1) % 4].x))
        .collect();
    (horizontal[0] && vertical[1] && horizontal[2] && vertical[3])
        || (vertical[0] && horizontal[1] && vertical[2] && horizontal[3])
}

/// The plain description of a shape: the kind and the corner count.
///
/// # Rules
/// - An empty shape is the plain four-corner rectangle of an untraced board.
/// - A closing copy of the first corner is not a corner.
pub(crate) fn shape_description(points: &[Point], arcs: &[Option<Point>]) -> String {
    if points.len() < 3 {
        return t!("tree.edge.rectangle_corners", n = 4);
    }
    let closed = points.first() == points.last();
    let ring = &points[..points.len() - usize::from(closed)];
    if is_rectangle(ring, arcs) {
        t!("tree.edge.rectangle_corners", n = ring.len())
    } else {
        t!("tree.edge.free_form", n = ring.len())
    }
}

/// The copper layer key a tree path names, for `side:top`, `side:bottom` and `inner:n`.
pub(crate) fn copper_key_of_path(board: &pcbcraft_doc::Board, path: &str) -> Option<String> {
    let count = board.copper_count();
    let first = path.split('/').next().unwrap_or("");
    if first == "side:top" {
        Some(board.copper_key(0))
    } else if first == "side:bottom" {
        Some(board.copper_key(count.saturating_sub(1)))
    } else {
        let n: u8 = first.strip_prefix("inner:")?.parse().ok()?;
        (n < count).then(|| board.copper_key(n))
    }
}

fn point_json(p: Point) -> Value {
    json!({"x": p.x, "y": p.y})
}

fn arcs_json(arcs: &[Option<Point>]) -> Vec<Value> {
    arcs.iter()
        .map(|m| m.map_or(Value::Null, point_json))
        .collect()
}

/// Midpoint of the arc over chord `a`-`b` with the given radius, on the left
/// of the direction a to b when `left`. `None` when the radius cannot span the chord.
fn mid_for_radius(a: Point, b: Point, radius: f64, left: bool) -> Option<Point> {
    let chord = a.distance(b);
    if chord < 1e-6 || radius * 2.0 <= chord * (1.0 + 1e-9) {
        return None;
    }
    let sagitta = radius - (radius * radius - chord * chord / 4.0).sqrt();
    let sign = if left { 1.0 } else { -1.0 };
    let (nx, ny) = (-(b.y - a.y) / chord, (b.x - a.x) / chord);
    Some(Point::new(
        (a.x + b.x) / 2.0 + nx * sagitta * sign,
        (a.y + b.y) / 2.0 + ny * sagitta * sign,
    ))
}

fn is_left(a: Point, b: Point, m: Point) -> bool {
    (b.x - a.x) * (m.y - a.y) - (b.y - a.y) * (m.x - a.x) > 0.0
}

impl App {
    /// Key of the active copper layer (`F.Cu`, `In1.Cu`, `B.Cu`).
    pub(crate) fn active_copper_key(&self) -> String {
        self.project
            .board
            .copper_key(self.route_layer.index().min(31))
    }

    /// The active layer's key when that layer has a shape of its own.
    pub(crate) fn active_own_shape(&self) -> Option<String> {
        let key = self.active_copper_key();
        self.project
            .board
            .layer_shapes
            .contains_key(&key)
            .then_some(key)
    }

    /// The corners and arcs the outline handles edit now.
    pub(crate) fn editing_shape(&self) -> (Vec<Point>, Vec<Option<Point>>) {
        let (points, arcs) = self.project.board.shape_of(&self.active_copper_key());
        (points.to_vec(), arcs.to_vec())
    }

    /// True while the outline handles are the thing being worked on.
    pub(crate) fn shape_edit_active(&self) -> bool {
        self.tab == Tab::Pcb
            && self.tool_is_select()
            && (self.outline_ui.segment.is_some()
                || self.outline_ui.drag_active()
                || self.tree.selected_key.as_deref().is_some_and(|k| {
                    k == "edge"
                        || k.ends_with("/edge")
                        || k.starts_with("side:")
                        || k.starts_with("inner:")
                }))
    }

    /// The footer sentence while a shape is being edited; empty otherwise.
    pub(crate) fn shape_status(&self) -> String {
        if !self.shape_edit_active() {
            return String::new();
        }
        match self.active_own_shape() {
            Some(key) => t!("shape.status.layer", layer = key),
            None => t!("shape.status.board"),
        }
    }

    fn tool_is_select(&self) -> bool {
        matches!(self.tool, Tool::Select)
    }

    /// Own shapes to draw, in stack order: the active layer's first flag set.
    /// A layer whose eye is off is left out.
    pub(crate) fn shape_paint_plan(&self) -> Vec<PlannedShape> {
        let active = self.active_copper_key();
        self.project
            .board
            .layer_shapes_in_order()
            .into_iter()
            .filter(|(_, follows, _)| !follows)
            .filter(|(key, _, _)| {
                self.project
                    .board
                    .resolve_copper_index(key)
                    .ok()
                    .and_then(|i| Layer::from_index(i).ok())
                    .is_some_and(|layer| self.layer_group_shown(layer))
            })
            .map(|(layer, _, _)| PlannedShape {
                active: layer == active,
                layer,
            })
            .collect()
    }

    /// Run one shape edit: a layer shape through the layer command, the board
    /// shape through `board.set_outline`.
    fn commit_shape(&mut self, own: Option<String>, points: &[Point], arcs: &[Option<Point>]) {
        let mut arcs = arcs.to_vec();
        arcs.resize(points.len(), None);
        let list: Vec<Value> = points.iter().map(|p| point_json(*p)).collect();
        match own {
            Some(layer) => {
                self.run(
                    "board.layer_shape_set",
                    json!({"layer": layer, "points": list, "arcs": arcs_json(&arcs)}),
                );
            }
            None => {
                self.run(
                    "board.set_outline",
                    json!({"points": list, "arcs": arcs_json(&arcs)}),
                );
            }
        }
    }

    /// Move a corner of the shape being edited, keeping the radius of the arcs that touch it.
    pub(crate) fn shape_move_corner(&mut self, i: usize, at: Point) {
        let Some(layer) = self.active_own_shape() else {
            self.run(
                "board.outline_set_vertex",
                json!({"index": i, "at": point_json(at)}),
            );
            return;
        };
        let (mut points, mut arcs) = self.editing_shape();
        arcs.resize(points.len(), None);
        let n = points.len();
        if i >= n {
            return;
        }
        let old = points.clone();
        points[i] = at;
        for seg in [(i + n - 1) % n, i] {
            let Some(mid) = arcs[seg] else {
                continue;
            };
            let (a, b) = (old[seg], old[(seg + 1) % n]);
            arcs[seg] = segment_arc(&old, &arcs, seg).and_then(|arc| {
                mid_for_radius(
                    points[seg],
                    points[(seg + 1) % n],
                    arc.radius,
                    is_left(a, b, mid),
                )
            });
        }
        self.commit_shape(Some(layer), &points, &arcs);
    }

    /// Bend edge `seg` of the shape being edited through `mid`.
    pub(crate) fn shape_bend_edge(&mut self, seg: usize, mid: Point) {
        let Some(layer) = self.active_own_shape() else {
            self.run(
                "board.outline_set_arc",
                json!({"segment": seg, "mid": point_json(mid)}),
            );
            return;
        };
        let (points, mut arcs) = self.editing_shape();
        arcs.resize(points.len(), None);
        if seg < arcs.len() {
            arcs[seg] = Some(mid);
            self.commit_shape(Some(layer), &points, &arcs);
        }
    }

    /// Bend edge `seg` to a radius, bulging outward or inward.
    pub(crate) fn shape_round_edge(&mut self, seg: usize, radius: f64, convex: bool) {
        let Some(layer) = self.active_own_shape() else {
            let side = if convex { "convex" } else { "concave" };
            self.run(
                "board.outline_set_arc",
                json!({"segment": seg, "radius": radius, "side": side}),
            );
            return;
        };
        let (points, mut arcs) = self.editing_shape();
        arcs.resize(points.len(), None);
        let n = points.len();
        if seg >= n {
            return;
        }
        let left = convex == (pcbcraft_doc::outline_signed_area(&points) < 0.0);
        match mid_for_radius(points[seg], points[(seg + 1) % n], radius, left) {
            Some(mid) => {
                arcs[seg] = Some(mid);
                self.commit_shape(Some(layer), &points, &arcs);
            }
            None => self.message = t!("shape.radius_too_small"),
        }
    }

    /// Make edge `seg` of the shape being edited straight again.
    pub(crate) fn shape_straighten_edge(&mut self, seg: usize) {
        let Some(layer) = self.active_own_shape() else {
            self.run("board.outline_straighten", json!({"segment": seg}));
            return;
        };
        let (points, mut arcs) = self.editing_shape();
        arcs.resize(points.len(), None);
        if seg < arcs.len() {
            arcs[seg] = None;
            self.commit_shape(Some(layer), &points, &arcs);
        }
    }

    /// Add a corner in the middle of edge `seg`. The two new edges are straight.
    pub(crate) fn shape_add_corner(&mut self, seg: usize) {
        let (mut points, mut arcs) = self.editing_shape();
        arcs.resize(points.len(), None);
        let n = points.len();
        if seg >= n || n >= MAX_CORNERS {
            self.message = t!("shape.corner.too_many");
            return;
        }
        let (a, b) = (points[seg], points[(seg + 1) % n]);
        let mid = Point::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
        points.insert(seg + 1, mid);
        arcs[seg] = None;
        arcs.insert(seg + 1, None);
        let own = self.active_own_shape();
        self.commit_shape(own, &points, &arcs);
    }

    /// Remove the corner where edge `seg` starts. The edge before it and this one join into one straight edge.
    pub(crate) fn shape_remove_corner(&mut self, seg: usize) {
        let (mut points, mut arcs) = self.editing_shape();
        arcs.resize(points.len(), None);
        let n = points.len();
        if seg >= n || n <= 3 {
            self.message = t!("shape.corner.too_few");
            return;
        }
        points.remove(seg);
        arcs.remove(seg);
        let before = (seg + n - 1) % n;
        let before = if before > seg { before - 1 } else { before };
        if let Some(slot) = arcs.get_mut(before) {
            *slot = None;
        }
        let own = self.active_own_shape();
        self.commit_shape(own, &points, &arcs);
    }

    /// Give copper layer `key` a shape of its own: a copy of the board shape,
    /// so it can be edited at once.
    pub(crate) fn give_layer_own_shape(&mut self, key: &str) {
        let board = &self.project.board;
        let (points, arcs) = match board.outline.as_ref().filter(|o| o.len() >= 3) {
            Some(o) => (o.clone(), board.outline_arcs.clone()),
            None => (rectangle_outline(board.width, board.height), Vec::new()),
        };
        self.commit_shape(Some(key.to_owned()), &points, &arcs);
    }

    /// Send copper layer `key` back to following the board shape.
    pub(crate) fn layer_follow_board(&mut self, key: &str) {
        self.run("board.layer_shape_clear", json!({"layer": key}));
        self.outline_ui.clear_pick();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pts(list: &[(f64, f64)]) -> Vec<Point> {
        list.iter().map(|(x, y)| Point::new(*x, *y)).collect()
    }

    #[test]
    fn a_plain_rectangle_is_named_a_rectangle() {
        let square = pts(&[(0.0, 0.0), (10.0, 0.0), (10.0, 5.0), (0.0, 5.0)]);
        assert_eq!(shape_description(&square, &[]), "Rectangle, 4 corners");
        assert_eq!(shape_description(&[], &[]), "Rectangle, 4 corners");
    }

    #[test]
    fn an_l_shape_is_free_form_with_its_corner_count() {
        let l = pts(&[
            (0.0, 0.0),
            (40.0, 0.0),
            (40.0, 10.0),
            (10.0, 10.0),
            (10.0, 30.0),
            (0.0, 30.0),
        ]);
        assert_eq!(shape_description(&l, &[]), "Free-form, 6 corners");
    }

    #[test]
    fn a_tilted_four_corner_shape_is_not_a_rectangle() {
        let kite = pts(&[(0.0, 0.0), (10.0, 2.0), (8.0, 9.0), (0.0, 6.0)]);
        assert_eq!(shape_description(&kite, &[]), "Free-form, 4 corners");
    }

    #[test]
    fn a_radius_too_small_for_the_chord_gives_no_midpoint() {
        let (a, b) = (Point::new(0.0, 0.0), Point::new(10.0, 0.0));
        assert!(mid_for_radius(a, b, 4.0, true).is_none());
        assert!(mid_for_radius(a, b, 10.0, true).is_some());
    }
}
