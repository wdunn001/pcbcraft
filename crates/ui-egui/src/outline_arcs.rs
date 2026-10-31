//! Draw and edit circular arcs on the board outline: canvas handles, drags,
//! and the edge inspector. Every change uses the typed `board.outline_*`
//! engine commands.

use super::*;
use pcbcraft_doc::{Project, segment_arc};
use pcbcraft_geom::Point;

/// What on the outline the pointer is over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OutlineHit {
    Vertex(usize),
    ArcMid(usize),
    Segment(usize),
}

/// In-flight drag of a vertex or arc midpoint.
#[derive(Clone, Copy, Debug)]
enum OutlineDrag {
    Vertex(usize),
    ArcMid(usize),
}

/// Selection, inspector drafts and drag state for outline editing.
#[derive(Default)]
pub(crate) struct OutlineUi {
    pub(crate) segment: Option<usize>,
    pub(crate) radius: f64,
    pub(crate) convex: bool,
    loaded: Option<usize>,
    drag: Option<OutlineDrag>,
    drag_preview: Option<Point>,
}

impl OutlineUi {
    pub(crate) fn clear_pick(&mut self) {
        self.segment = None;
        self.loaded = None;
    }

    pub(crate) fn pick(&mut self, hit: OutlineHit, project: &Project) {
        self.segment = Some(match hit {
            OutlineHit::Vertex(i) | OutlineHit::ArcMid(i) | OutlineHit::Segment(i) => i,
        });
        if self.loaded != self.segment {
            self.loaded = self.segment;
            self.radius = 5.0;
            self.convex = true;
            if let Some(seg) = self.segment {
                if let Some(points) = project.board.outline.as_ref()
                    && let Some(arc) = segment_arc(points, &project.board.outline_arcs, seg)
                {
                    self.radius = arc.radius;
                }
            }
        }
    }

    pub(crate) fn drag_active(&self) -> bool {
        self.drag.is_some()
    }

    fn sync_radius_from_project(&mut self, project: &Project) {
        let Some(seg) = self.segment else {
            return;
        };
        let Some(points) = project.board.outline.as_ref() else {
            return;
        };
        if let Some(arc) = segment_arc(points, &project.board.outline_arcs, seg) {
            self.radius = arc.radius;
        }
    }
}

fn outline_points(project: &Project) -> Option<&[Point]> {
    project.board.outline.as_deref().filter(|p| p.len() >= 3)
}

fn near(zoom: f32, dist: f64, px: f64) -> bool {
    dist * zoom as f64 <= px
}

fn segment_distance(point: Point, from: Point, to: Point) -> f64 {
    let dx = to.x - from.x;
    let dy = to.y - from.y;
    let span = dx * dx + dy * dy;
    if span < 1e-12 {
        return point.distance(from);
    }
    let along = ((point.x - from.x) * dx + (point.y - from.y) * dy) / span;
    let along = along.clamp(0.0, 1.0);
    point.distance(Point::new(from.x + dx * along, from.y + dy * along))
}

fn distance_to_segment(points: &[Point], arcs: &[Option<Point>], i: usize, pos: Point) -> f64 {
    let n = points.len();
    let from = points[i];
    let to = points[(i + 1) % n];
    if let Some(arc) = segment_arc(points, arcs, i) {
        let steps = 24;
        (0..steps)
            .map(|k| {
                let t0 = k as f64 / steps as f64;
                let t1 = (k + 1) as f64 / steps as f64;
                segment_distance(pos, arc.point_at(t0), arc.point_at(t1))
            })
            .fold(f64::INFINITY, f64::min)
    } else {
        segment_distance(pos, from, to)
    }
}

fn arc_mid_display(points: &[Point], arcs: &[Option<Point>], i: usize) -> Point {
    if let Some(mid) = arcs.get(i).and_then(|m| *m) {
        return mid;
    }
    let n = points.len();
    let a = points[i];
    let b = points[(i + 1) % n];
    Point::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0)
}

/// Closest outline feature under `pos`, if any.
pub(crate) fn pick_at(project: &Project, pos: Point, zoom: f32) -> Option<OutlineHit> {
    let points = outline_points(project)?;
    let arcs = &project.board.outline_arcs;
    let n = points.len();
    let mut best: Option<(f64, OutlineHit)> = None;
    for i in 0..n {
        let d = points[i].distance(pos);
        if near(zoom, d, 8.0) && best.as_ref().is_none_or(|(known, _)| d < *known) {
            best = Some((d, OutlineHit::Vertex(i)));
        }
    }
    if best.is_some() {
        return best.map(|(_, h)| h);
    }
    for i in 0..n {
        let mid = arc_mid_display(points, arcs, i);
        let d = mid.distance(pos);
        if near(zoom, d, 7.0) && best.as_ref().is_none_or(|(known, _)| d < *known) {
            best = Some((d, OutlineHit::ArcMid(i)));
        }
    }
    for i in 0..n {
        let d = distance_to_segment(points, arcs, i, pos);
        if near(zoom, d, 6.0) && best.as_ref().is_none_or(|(known, _)| d < *known) {
            best = Some((d, OutlineHit::Segment(i)));
        }
    }
    best.map(|(_, h)| h)
}

/// Screen polyline for one outline edge (straight or sampled arc).
pub(super) fn edge_screen_points(
    points: &[Point],
    arcs: &[Option<Point>],
    i: usize,
    camera: Camera,
    rect: Rect,
) -> Vec<egui::Pos2> {
    let n = points.len();
    let mut out = vec![camera.screen(points[i], rect)];
    if let Some(arc) = segment_arc(points, arcs, i) {
        let steps = 24;
        for k in 1..steps {
            out.push(camera.screen(arc.point_at(k as f64 / steps as f64), rect));
        }
    }
    out.push(camera.screen(points[(i + 1) % n], rect));
    out
}

impl App {
    pub(super) fn paint_outline_handles(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        camera: Camera,
    ) {
        let Some(points) = outline_points(&self.project) else {
            return;
        };
        if !matches!(self.tool, Tool::Select) {
            return;
        }
        let arcs = &self.project.board.outline_arcs;
        let n = points.len();
        let accent = self.accent();
        let stroke = Stroke::new(1.5, accent);
        for i in 0..n {
            let selected = self.outline_ui.segment == Some(i);
            if selected {
                let edge = edge_screen_points(points, arcs, i, camera, rect);
                painter.add(egui::Shape::line(edge, Stroke::new(2.5, accent)));
            }
        }
        for i in 0..n {
            let at = camera.screen(points[i], rect);
            let r = if self.outline_ui.segment == Some(i) {
                5.5
            } else {
                4.0
            };
            painter.circle_filled(at, r, accent.gamma_multiply(0.35));
            painter.circle_stroke(at, r, stroke);
        }
        for i in 0..n {
            let mid = arc_mid_display(points, arcs, i);
            let at = camera.screen(mid, rect);
            let has_arc = arcs.get(i).and_then(|m| *m).is_some();
            let size = if self.outline_ui.segment == Some(i) {
                6.0
            } else {
                5.0
            };
            if has_arc {
                painter.circle_filled(at, size, accent);
            } else {
                painter.rect_filled(
                    egui::Rect::from_center_size(at, egui::vec2(size, size)),
                    1.0,
                    accent.gamma_multiply(0.5),
                );
            }
            painter.circle_stroke(at, size, stroke);
        }
        if let Some(OutlineDrag::Vertex(i)) = self.outline_ui.drag {
            if let Some(preview) = self.outline_ui.drag_preview {
                let at = camera.screen(preview, rect);
                painter.circle_stroke(at, 6.0, Stroke::new(2.0, accent));
            } else {
                let at = camera.screen(points[i], rect);
                painter.circle_stroke(at, 6.0, Stroke::new(2.0, accent));
            }
        }
        if let Some(OutlineDrag::ArcMid(i)) = self.outline_ui.drag {
            let preview = self
                .outline_ui
                .drag_preview
                .unwrap_or_else(|| arc_mid_display(points, arcs, i));
            let edge = edge_screen_points(points, arcs, i, camera, rect);
            painter.add(egui::Shape::dashed_line(
                &edge,
                Stroke::new(1.0, accent.gamma_multiply(0.6)),
                4.0,
                3.0,
            ));
            let at = camera.screen(preview, rect);
            painter.circle_stroke(at, 6.0, Stroke::new(2.0, accent));
        }
    }

    pub(crate) fn outline_canvas_drag(
        &mut self,
        response: &egui::Response,
        cursor: Option<Point>,
        zoom: f32,
    ) {
        if !matches!(self.tool, Tool::Select) || self.tab != Tab::Pcb {
            self.outline_ui.drag = None;
            self.outline_ui.drag_preview = None;
            return;
        }
        let Some(pos) = cursor else {
            return;
        };
        if response.drag_started_by(egui::PointerButton::Primary) {
            if let Some(hit) = pick_at(&self.project, pos, zoom) {
                self.edit.press = None;
                self.select(None);
                self.outline_ui.pick(hit, &self.project);
                self.outline_ui.drag = Some(match hit {
                    OutlineHit::Vertex(i) => OutlineDrag::Vertex(i),
                    OutlineHit::ArcMid(i) | OutlineHit::Segment(i) => OutlineDrag::ArcMid(i),
                });
                self.outline_ui.drag_preview = Some(crate::canvas::snap(pos));
            }
        }
        if response.dragged_by(egui::PointerButton::Primary) {
            if self.outline_ui.drag.is_some() {
                self.outline_ui.drag_preview = Some(crate::canvas::snap(pos));
            }
        }
        if response.drag_stopped_by(egui::PointerButton::Primary) {
            let Some(drag) = self.outline_ui.drag.take() else {
                return;
            };
            let at = crate::canvas::snap(self.outline_ui.drag_preview.take().unwrap_or(pos));
            match drag {
                OutlineDrag::Vertex(i) => {
                    self.run(
                        "board.outline_set_vertex",
                        json!({"index": i, "at": {"x": at.x, "y": at.y}}),
                    );
                }
                OutlineDrag::ArcMid(i) => {
                    self.run(
                        "board.outline_set_arc",
                        json!({"segment": i, "mid": {"x": at.x, "y": at.y}}),
                    );
                }
            }
            self.outline_ui.sync_radius_from_project(&self.project);
        }
    }

    pub(crate) fn try_pick_outline(&mut self, pos: Point, zoom: f32) -> bool {
        if self.tab != Tab::Pcb || !matches!(self.tool, Tool::Select) {
            return false;
        }
        let Some(hit) = pick_at(&self.project, pos, zoom) else {
            return false;
        };
        self.select(None);
        self.outline_ui.pick(hit, &self.project);
        true
    }

    pub(crate) fn outline_inspector(&mut self, ui: &mut egui::Ui) {
        let Some(seg) = self.outline_ui.segment else {
            if outline_points(&self.project).is_some() {
                ui.label(
                    egui::RichText::new(t!("board.outline.select_hint"))
                        .small()
                        .color(self.muted()),
                );
            }
            return;
        };
        let Some(points) = self.project.board.outline.clone() else {
            return;
        };
        let n = points.len();
        let curved = self
            .project
            .board
            .outline_arcs
            .get(seg)
            .and_then(|m| *m)
            .is_some();
        ui_kit::section::section(
            ui,
            "board-edge",
            &t!("board.outline.section"),
            &t!("board.outline.section.tip"),
            |ui| {
                ui.label(t!(
                    "board.outline.segment_label",
                    index = seg + 1,
                    count = n
                ));
                ui.horizontal(|ui| {
                    ui.label(t!("board.outline.radius"));
                    ui.add(crate::number_entry::localized_active(
                        egui::DragValue::new(&mut self.outline_ui.radius)
                            .speed(0.5)
                            .range(0.1..=10_000.0)
                            .suffix(format!(" {}", t!("unit.mm"))),
                        3,
                    ))
                    .named("Board edge corner radius")
                    .on_hover_text(t!("board.outline.radius.tip"));
                });
                ui.horizontal(|ui| {
                    ui.selectable_value(
                        &mut self.outline_ui.convex,
                        true,
                        t!("board.outline.side_convex"),
                    )
                    .on_hover_text(t!("board.outline.side_convex.tip"));
                    ui.selectable_value(
                        &mut self.outline_ui.convex,
                        false,
                        t!("board.outline.side_concave"),
                    )
                    .on_hover_text(t!("board.outline.side_concave.tip"));
                });
                if ui
                    .button(t!("board.outline.apply_radius"))
                    .on_hover_text(t!("board.outline.apply_radius.tip"))
                    .clicked()
                {
                    let side = if self.outline_ui.convex {
                        "convex"
                    } else {
                        "concave"
                    };
                    self.run(
                        "board.outline_set_arc",
                        json!({
                            "segment": seg,
                            "radius": self.outline_ui.radius,
                            "side": side,
                        }),
                    );
                    self.outline_ui.sync_radius_from_project(&self.project);
                }
                if ui
                    .add_enabled(curved, egui::Button::new(t!("board.outline.straighten")))
                    .on_hover_text(if curved {
                        t!("board.outline.straighten.tip")
                    } else {
                        t!("board.outline.straighten_already")
                    })
                    .clicked()
                {
                    self.run("board.outline_straighten", json!({"segment": seg}));
                }
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pick_finds_a_vertex_before_the_open_edge() -> Result<(), String> {
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = App::create(shared, Preferences::default(), None, None);
        app.run(
            "board.set_outline",
            json!({"points": [
                {"x":0.0,"y":0.0},
                {"x":40.0,"y":0.0},
                {"x":40.0,"y":30.0},
                {"x":0.0,"y":30.0}
            ]}),
        );
        let hit = pick_at(&app.project, Point::new(40.0, 0.0), 15.0).ok_or("corner vertex")?;
        assert_eq!(hit, OutlineHit::Vertex(1));
        Ok(())
    }
}
