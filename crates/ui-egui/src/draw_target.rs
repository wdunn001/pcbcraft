//! The active drawing target: the one layer or sublayer the drawing tools
//! draw on, chosen in the Layers tree and read by the status bar.
//!
//! Context: `layers_tree.rs` rows carry a [`Target`]; clicking one sends
//! `Action::SetTarget`. `lib.rs` holds `App::target` and calls
//! [`App::target_label`] from the status bar. The canvas
//! hands every drawing click on the Draw tools to [`App::draw_click`].
//! Local rules: a drawing becomes an engine command (`track.add`, `zone.place`,
//! `graphic.add`, `text.place`, `board.set_outline`, `board.layer_*_add`); this
//! module never edits the document itself. The copper layer lives in
//! `App::route_layer`, so the Settings "Route on" choice and this target never
//! disagree.

use crate::canvas::snap;
use crate::{App, DrawShape};
use pcbcraft_doc::{Id, Layer};
use pcbcraft_geom::Point;
use pcbcraft_text::t;
use serde_json::json;

/// Which face of the board a sublayer is on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Side {
    Top,
    Bottom,
}

/// What the drawing tools draw on.
///
/// # Rules
/// - `Copper` names the copper layer by index; its value is kept equal to
///   `App::route_layer` by [`App::current_target`].
///
/// # Do not
/// - Add a variant without a branch in [`App::draw_click`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Target {
    Copper(Layer),
    Silk(Side),
    Mask(Side),
    Paste(Side),
    Edge,
    User(Id),
}

/// How many vertices stand in for a circle drawn as a copper zone or outline.
const CIRCLE_VERTICES: usize = 32;

impl Target {
    /// The fabrication sublayer a board graphic gets on this target.
    pub(crate) fn sublayer(self) -> Option<&'static str> {
        match self {
            Self::Silk(Side::Top) => Some("F.SilkS"),
            Self::Silk(Side::Bottom) => Some("B.SilkS"),
            Self::Mask(Side::Top) => Some("F.Mask"),
            Self::Mask(Side::Bottom) => Some("B.Mask"),
            _ => None,
        }
    }
}

/// The corners of a rectangle as a closed polygon (first vertex repeated).
fn closed_rect(a: Point, b: Point) -> Vec<Point> {
    let (x0, x1) = (a.x.min(b.x), a.x.max(b.x));
    let (y0, y1) = (a.y.min(b.y), a.y.max(b.y));
    vec![
        Point::new(x0, y0),
        Point::new(x1, y0),
        Point::new(x1, y1),
        Point::new(x0, y1),
        Point::new(x0, y0),
    ]
}

/// A circle as a closed polygon of [`CIRCLE_VERTICES`] corners.
fn closed_circle(centre: Point, radius: f64) -> Vec<Point> {
    let mut points: Vec<Point> = (0..CIRCLE_VERTICES)
        .map(|i| {
            let a = std::f64::consts::TAU * i as f64 / CIRCLE_VERTICES as f64;
            Point::new(centre.x + radius * a.cos(), centre.y + radius * a.sin())
        })
        .collect();
    if let Some(first) = points.first().copied() {
        points.push(first);
    }
    points
}

fn xy(points: &[Point]) -> Vec<serde_json::Value> {
    points.iter().map(|p| json!({"x": p.x, "y": p.y})).collect()
}

/// A figure the user finished drawing, before it becomes a command.
enum Figure {
    Line(Point, Point),
    Rect(Point, Point),
    Circle(Point, f64),
    Poly(Vec<Point>),
    Words(Point),
}

impl App {
    /// The target with its copper layer read from `route_layer`.
    ///
    /// # Dependencies
    /// - `crates/ui-egui/src/lib.rs` `App::route_layer` is the copper layer.
    pub(crate) fn current_target(&self) -> Target {
        match self.target {
            Target::Copper(_) => Target::Copper(self.route_layer),
            other => other,
        }
    }

    /// Make `target` the drawing target and keep the copper layer and the
    /// user-layer picker in step.
    pub(crate) fn set_target(&mut self, target: Target) {
        self.target = target;
        match target {
            Target::Copper(layer) => self.route_layer = layer,
            Target::User(id) => self.art_layer = Some(id),
            _ => {}
        }
        self.art_start = None;
        self.art_points.clear();
    }

    /// The target's plain name, for the Layer field and the status bar.
    pub(crate) fn target_label(&self) -> String {
        let target = self.current_target();
        match target {
            Target::Copper(layer) => {
                if layer == Layer::Front {
                    t!("tree.layer.top_copper")
                } else if layer.index() + 1 >= self.project.copper_layer_count() {
                    t!("tree.layer.bottom_copper")
                } else {
                    self.project.layer_name(layer)
                }
            }
            Target::Silk(Side::Top) => t!("tree.layer.top_silk"),
            Target::Silk(Side::Bottom) => t!("tree.layer.bottom_silk"),
            Target::Mask(Side::Top) => t!("tree.layer.top_mask"),
            Target::Mask(Side::Bottom) => t!("tree.layer.bottom_mask"),
            Target::Paste(Side::Top) => t!("tree.layer.top_paste"),
            Target::Paste(Side::Bottom) => t!("tree.layer.bottom_paste"),
            Target::Edge => t!("tree.layer.edge"),
            Target::User(id) => self
                .project
                .board
                .user_layers
                .iter()
                .find(|l| l.id == id)
                .map_or_else(
                    || t!("board.draw_tool_options.no_layer_yet"),
                    |l| l.name.clone(),
                ),
        }
    }

    /// The shape the Draw tools make on the current target: a line on the
    /// board edge is a chain of corners, because an outline needs three.
    pub(crate) fn draw_shape(&self) -> DrawShape {
        if self.current_target() == Target::Edge && self.art_shape == DrawShape::Line {
            DrawShape::Poly
        } else {
            self.art_shape
        }
    }

    /// True when a drawing is ready to finish with Enter.
    pub(crate) fn draw_can_finish(&self) -> bool {
        self.draw_shape() == DrawShape::Poly && self.art_points.len() >= 3
    }

    /// Finish the outline being drawn on the current target.
    pub(crate) fn draw_finish(&mut self) {
        let points = std::mem::take(&mut self.art_points);
        self.place_figure(Figure::Poly(points));
    }

    /// One click of a Draw tool, on any target but a user layer (those keep
    /// their own path in `canvas.rs`).
    ///
    /// # Rules
    /// - Clicks collect into a [`Figure`]; the figure becomes one engine command.
    /// - A target that cannot hold the shape says so in the status line and
    ///   draws nothing.
    ///
    /// # Dependencies
    /// - `crates/engine/src/lib.rs` commands `track.add`, `zone.place`, `graphic.add`,
    ///   `text.place` and `board.set_outline`.
    ///
    /// # Do not
    /// - Edit `self.project` here.
    pub(crate) fn draw_click(&mut self, pos: Point, zoom: f32) {
        let target = self.current_target();
        if let Target::Paste(_) = target {
            self.message = t!("target.no_paste");
            return;
        }
        let at = snap(pos);
        let figure = match self.draw_shape() {
            DrawShape::Words => Some(Figure::Words(at)),
            DrawShape::Poly => {
                let closing = self
                    .art_points
                    .first()
                    .is_some_and(|first| pos.distance(*first) < 6.0 / zoom as f64);
                if closing && self.art_points.len() >= 3 {
                    Some(Figure::Poly(std::mem::take(&mut self.art_points)))
                } else if self.art_points.len() >= 64 {
                    self.message = t!("board.canvas_click.an_outline_holds_up_to");
                    None
                } else {
                    self.art_points.push(at);
                    None
                }
            }
            shape => match self.art_start.take() {
                None => {
                    self.art_start = Some(at);
                    None
                }
                Some(start) => Some(match shape {
                    DrawShape::Line => Figure::Line(start, at),
                    DrawShape::Rect => Figure::Rect(start, at),
                    _ => Figure::Circle(start, start.distance(at)),
                }),
            },
        };
        if let Some(figure) = figure {
            self.place_figure(figure);
        }
    }

    /// The net a hand-drawn track or zone belongs to: the one lit in the
    /// tree, else the net of the selected object, else the only net.
    fn draw_net(&self) -> Option<Id> {
        let selected = self.selected.and_then(|id| {
            self.project
                .board
                .tracks
                .iter()
                .find(|t| t.id == id)
                .map(|t| t.net)
                .or_else(|| {
                    self.project
                        .board
                        .zones
                        .iter()
                        .find(|z| z.id == id)
                        .map(|z| z.net)
                })
        });
        self.highlight_net.or(selected).or_else(|| {
            (self.project.nets.len() == 1)
                .then(|| self.project.nets.first().map(|n| n.id))
                .flatten()
        })
    }

    fn place_figure(&mut self, figure: Figure) {
        let target = self.current_target();
        match target {
            Target::Copper(layer) => self.place_copper(layer, figure),
            Target::Silk(side) | Target::Mask(side) => self.place_graphic(target, side, figure),
            Target::Edge => self.place_edge(figure),
            Target::Paste(_) | Target::User(_) => {}
        }
    }

    fn place_copper(&mut self, layer: Layer, figure: Figure) {
        let Some(net) = self.draw_net() else {
            self.message = t!("target.pick_net");
            return;
        };
        let name = self.project.layer_name(layer);
        match figure {
            Figure::Line(a, b) => self.run(
                "track.add",
                json!({
                    "net": net,
                    "start": {"x": a.x, "y": a.y},
                    "end": {"x": b.x, "y": b.y},
                    "width": self.track_width,
                    "layer": name,
                }),
            ),
            Figure::Rect(a, b) => self.run(
                "zone.place",
                json!({"net": net, "layer": name, "outline": xy(&closed_rect(a, b))}),
            ),
            Figure::Circle(c, r) => self.run(
                "zone.place",
                json!({"net": net, "layer": name, "outline": xy(&closed_circle(c, r))}),
            ),
            Figure::Poly(mut points) => {
                if let Some(first) = points.first().copied() {
                    points.push(first);
                }
                self.run(
                    "zone.place",
                    json!({"net": net, "layer": name, "outline": xy(&points)}),
                );
            }
            Figure::Words(_) => self.message = t!("target.words_on_silk"),
        }
    }

    fn place_graphic(&mut self, target: Target, side: Side, figure: Figure) {
        let Some(sublayer) = target.sublayer() else {
            return;
        };
        let width = 0.15;
        let add = |app: &mut App, shape: &str, points: &[Point], width: f64, text: &str| {
            let mut params = json!({
                "layer": "board",
                "shape": shape,
                "points": xy(points),
                "width": width,
                "sublayer": sublayer,
            });
            if !text.is_empty() {
                params["text"] = json!(text);
            }
            app.run("graphic.add", params);
        };
        match figure {
            Figure::Line(a, b) => add(self, "line", &[a, b], width, ""),
            Figure::Rect(a, b) => add(self, "rect", &[a, b], width, ""),
            Figure::Circle(c, r) => add(self, "circle", &[c], r.max(0.1), ""),
            Figure::Poly(points) => add(self, "polygon", &points, width, ""),
            Figure::Words(at) => {
                let words = self.art_text.trim().to_owned();
                if words.is_empty() {
                    self.message = t!("board.canvas_click.type_the_words_to_print");
                } else if matches!(target, Target::Mask(_)) {
                    self.message = t!("target.words_on_silk");
                } else {
                    let layer = if side == Side::Top {
                        Layer::Front
                    } else {
                        Layer::Back
                    };
                    self.run(
                        "text.place",
                        json!({
                            "text": words,
                            "position": {"x": at.x, "y": at.y},
                            "size": 1.0,
                            "layer": self.project.layer_name(layer),
                        }),
                    );
                }
            }
        }
    }

    fn place_edge(&mut self, figure: Figure) {
        let points = match figure {
            Figure::Rect(a, b) => {
                let mut p = closed_rect(a, b);
                p.pop();
                p
            }
            Figure::Circle(c, r) => {
                let mut p = closed_circle(c, r);
                p.pop();
                p
            }
            Figure::Poly(p) => p,
            Figure::Line(..) | Figure::Words(_) => {
                self.message = t!("target.edge_shapes");
                return;
            }
        };
        self.run("board.set_outline", json!({"points": xy(&points)}));
    }
}
