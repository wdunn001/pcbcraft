use super::*;
use serde_json::json;

/// Schematic sheet drawing modes wired to engine `"graphic.add"`, `"dimension.add"`,
/// and `"schematic.table"` (not board `"text.place"`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SchShape {
    Line,
    Box,
    Circle,
    Arc,
    Polygon,
    Text,
    Dimension,
    Table,
}

impl App {
    pub(crate) fn schematic_draw_reset(&mut self) {
        self.sch_start = None;
        self.sch_points.clear();
    }

    #[cfg(test)]
    pub(crate) fn test_arm_schematic(&mut self, shape: SchShape) {
        self.tab = Tab::Schematic;
        self.tool = Tool::SchematicDraw(shape);
        self.schematic_draw_reset();
        if shape == SchShape::Text && self.sch_text.trim().is_empty() {
            self.sch_text = "Title".into();
        }
    }

    pub(crate) fn schematic_click(&mut self, pos: Point, zoom: f32) {
        let Tool::SchematicDraw(shape) = self.tool else {
            return;
        };
        let at = snap(pos);
        let stroke = self.sch_stroke;
        match shape {
            SchShape::Text => {
                let text = self.sch_text.trim().to_owned();
                if text.is_empty() {
                    self.message = "Type the heading first".into();
                    return;
                }
                self.run(
                    "graphic.add",
                    json!({
                        "layer": "schematic",
                        "shape": "text",
                        "points": [{"x": at.x, "y": at.y}],
                        "width": self.sch_text_size,
                        "text": text,
                    }),
                );
                self.tool = Tool::Select;
                self.schematic_draw_reset();
            }
            SchShape::Table => {
                self.run(
                    "schematic.table",
                    json!({"position": {"x": at.x, "y": at.y}}),
                );
                self.tool = Tool::Select;
            }
            SchShape::Dimension => match self.sch_start {
                None => self.sch_start = Some(pos),
                Some(start) => {
                    let s = snap(start);
                    self.run(
                        "dimension.add",
                        json!({
                            "layer": "schematic",
                            "start": {"x": s.x, "y": s.y},
                            "end": {"x": at.x, "y": at.y},
                        }),
                    );
                    self.schematic_draw_reset();
                    self.tool = Tool::Select;
                }
            },
            SchShape::Polygon => {
                let closing = self.sch_points.first().is_some_and(|first| {
                    pos.distance(*first) < 6.0 / zoom as f64
                });
                if closing && self.sch_points.len() >= 3 {
                    self.commit_schematic_polygon(stroke);
                } else if self.sch_points.len() >= 256 {
                    self.message =
                        "An outline holds up to 256 corners; click the first point to close it"
                            .into();
                } else {
                    self.sch_points.push(pos);
                }
            }
            SchShape::Arc => {
                if self.sch_points.len() >= 2 {
                    let mut pts = self.sch_points.clone();
                    pts.push(pos);
                    let points: Vec<Value> = pts
                        .iter()
                        .map(|p| {
                            let q = snap(*p);
                            json!({"x": q.x, "y": q.y})
                        })
                        .collect();
                    self.run(
                        "graphic.add",
                        json!({
                            "layer": "schematic",
                            "shape": "arc",
                            "points": points,
                            "width": stroke,
                        }),
                    );
                    self.schematic_draw_reset();
                    self.tool = Tool::Select;
                } else {
                    self.sch_points.push(pos);
                }
            }
            SchShape::Line | SchShape::Box | SchShape::Circle => match self.sch_start {
                None => self.sch_start = Some(pos),
                Some(start) => {
                    let s = snap(start);
                    let params = match shape {
                        SchShape::Line => json!({
                            "layer": "schematic",
                            "shape": "line",
                            "points": [{"x": s.x, "y": s.y}, {"x": at.x, "y": at.y}],
                            "width": stroke,
                        }),
                        SchShape::Box => json!({
                            "layer": "schematic",
                            "shape": "rect",
                            "points": [
                                {"x": s.x.min(at.x), "y": s.y.min(at.y)},
                                {"x": s.x.max(at.x), "y": s.y.max(at.y)},
                            ],
                            "width": stroke,
                        }),
                        SchShape::Circle => json!({
                            "layer": "schematic",
                            "shape": "circle",
                            "points": [{"x": s.x, "y": s.y}],
                            "width": s.distance(at),
                        }),
                        _ => json!({}),
                    };
                    self.run("graphic.add", params);
                    self.schematic_draw_reset();
                    self.tool = Tool::Select;
                }
            },
        }
    }

    pub(crate) fn commit_schematic_polygon(&mut self, stroke: f64) {
        let points: Vec<Value> = self
            .sch_points
            .iter()
            .map(|p| {
                let q = snap(*p);
                json!({"x": q.x, "y": q.y})
            })
            .collect();
        self.schematic_draw_reset();
        self.run(
            "graphic.add",
            json!({
                "layer": "schematic",
                "shape": "polygon",
                "points": points,
                "width": stroke,
            }),
        );
        self.tool = Tool::Select;
    }

    pub(crate) fn paint_schematic_draft(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        camera: Camera,
        cursor: Point,
    ) {
        let Tool::SchematicDraw(shape) = self.tool else {
            return;
        };
        let stroke = Stroke::new(1.6, self.accent());
        let snapped = snap(cursor);
        match shape {
            SchShape::Text => {
                if !self.sch_text.trim().is_empty() {
                    painter.text(
                        camera.screen(snapped, rect),
                        Align2::LEFT_TOP,
                        &self.sch_text,
                        FontId::proportional(self.sch_text_size as f32),
                        self.accent(),
                    );
                }
            }
            SchShape::Dimension => {
                if let Some(start) = self.sch_start {
                    let a = camera.screen(snap(start), rect);
                    let b = camera.screen(snapped, rect);
                    painter.line_segment([a, b], stroke);
                }
            }
            SchShape::Polygon => {
                let mut pts: Vec<Pos2> = self
                    .sch_points
                    .iter()
                    .map(|pt| camera.screen(*pt, rect))
                    .collect();
                pts.push(camera.screen(snapped, rect));
                if pts.len() >= 2 {
                    painter.add(egui::Shape::line(pts, stroke));
                }
            }
            SchShape::Arc => {
                let mut pts: Vec<Pos2> = self
                    .sch_points
                    .iter()
                    .map(|pt| camera.screen(*pt, rect))
                    .collect();
                pts.push(camera.screen(snapped, rect));
                if pts.len() >= 2 {
                    painter.add(egui::Shape::line(pts, stroke));
                }
            }
            SchShape::Line | SchShape::Box | SchShape::Circle => {
                if let Some(start) = self.sch_start {
                    let a = camera.screen(start, rect);
                    let b = camera.screen(snapped, rect);
                    match shape {
                        SchShape::Line => painter.line_segment([a, b], stroke),
                        SchShape::Box => {
                            painter.rect_stroke(
                                egui::Rect::from_min_max(a, b),
                                0.0,
                                stroke,
                                egui::StrokeKind::Inside,
                            );
                        }
                        SchShape::Circle => {
                            let radius = (b - a).length().max(1.0);
                            painter.circle_stroke(a, radius, stroke);
                        }
                        _ => {}
                    }
                }
            }
            SchShape::Table => {}
        }
    }
}
