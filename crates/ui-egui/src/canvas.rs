use super::*;
use egui::{Pos2, Sense};
use pcbcraft_doc::{Component, Id, Layer, PinRef, Project};
use pcbcraft_geom::point_segment;
mod board_art;
mod context;
pub(crate) mod gestures;
mod layout_art;

/// Side of the ruler strips at the canvas top and left, shared with the floating
/// view controls so they sit inside the margin the rulers leave.
pub(crate) const RULER_SIZE: f32 = 22.0;
mod sheet;
mod underlay;

/// A `#rrggbb` stroke colour, or None for empty or malformed text.
pub(crate) fn hex_colour(text: &str) -> Option<Color32> {
    let digits = text.strip_prefix('#').filter(|d| d.len() == 6)?;
    let v = u32::from_str_radix(digits, 16).ok()?;
    Some(Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8))
}

fn on_page(p: &Project, page: Id, object: Id) -> bool {
    p.on_schematic_page(page, object)
}

pub(super) enum CanvasMenuKind {
    Empty,
    Pin(PinRef),
    Pad(Id, String),
    Track(Id),
    Object(Id),
}

pub(crate) fn selection_bounds(project: &Project, id: Id) -> Option<(Point, Point)> {
    if let Some(c) = project.component(id) {
        let p = c.position;
        let pad = 3.0;
        return Some((
            Point::new(p.x - pad, p.y - pad),
            Point::new(p.x + pad, p.y + pad),
        ));
    }
    if let Some(f) = project.footprint(id) {
        let mut lo = f.position;
        let mut hi = f.position;
        for pad in &f.pads {
            if let Some(pos) = f.pad_position(&pad.number) {
                lo = Point::new(lo.x.min(pos.x), lo.y.min(pos.y));
                hi = Point::new(hi.x.max(pos.x), hi.y.max(pos.y));
            }
        }
        let pad = 1.0;
        return Some((
            Point::new(lo.x - pad, lo.y - pad),
            Point::new(hi.x + pad, hi.y + pad),
        ));
    }
    if let Some(track) = project.board.tracks.iter().find(|t| t.id == id) {
        let lo = Point::new(
            track.start.x.min(track.end.x),
            track.start.y.min(track.end.y),
        );
        let hi = Point::new(
            track.start.x.max(track.end.x),
            track.start.y.max(track.end.y),
        );
        return Some((lo, hi));
    }
    if let Some(via) = project.board.vias.iter().find(|v| v.id == id) {
        let r = via.diameter * 0.5;
        let p = via.position;
        return Some((Point::new(p.x - r, p.y - r), Point::new(p.x + r, p.y + r)));
    }
    None
}

/// The world rectangle each tab should show by default: the board outline on
/// the PCB tab, the whole drawing sheet on the schematic tab.
pub(crate) fn view_bounds(project: &Project, pcb: bool) -> (Point, Point) {
    if !pcb {
        return sheet::sheet_bounds(project);
    }
    let mut pts: Vec<Point> = Vec::new();
    if let Some(ring) = &project.board.outline {
        pts.extend(ring.iter().copied());
    }
    if pts.is_empty() {
        pts.push(Point::new(0.0, 0.0));
        pts.push(Point::new(project.board.width, project.board.height));
    }
    let mut lo = pts[0];
    let mut hi = pts[0];
    for p in &pts {
        lo = Point::new(lo.x.min(p.x), lo.y.min(p.y));
        hi = Point::new(hi.x.max(p.x), hi.y.max(p.y));
    }
    // Symbols and their pins reach well past the centres the bounds use; on the
    // board view the dimension lines and rulers need room around the board.
    let pad = if pcb { 6.0 } else { 12.0 };
    (
        Point::new(lo.x - pad, lo.y - pad),
        Point::new(hi.x + pad, hi.y + pad),
    )
}

/// Zoom and centre that fit a world rectangle inside a panel, keeping the
/// whole design visible instead of a postage stamp at the minimum zoom.
pub(crate) fn fit_camera(lo: Point, hi: Point, panel: (f32, f32)) -> Camera {
    let w = (hi.x - lo.x).max(1.0);
    let h = (hi.y - lo.y).max(1.0);
    let zoom = ((panel.0 * 0.85) as f64 / w)
        .min((panel.1 * 0.85) as f64 / h)
        .clamp(0.02, 100.0) as f32;
    Camera {
        mirror: false,
        turns: 0,
        center: Point::new((lo.x + hi.x) / 2.0, (lo.y + hi.y) / 2.0),
        zoom,
    }
}

/// Net spotlight: copper on the spotlighted net takes the accent colour and
/// everything else fades, so one click can trace a net across the board.
fn spotlight(lit: Option<Id>, net: Option<Id>, base: Color32, hot: Color32) -> Color32 {
    match lit {
        None => base,
        Some(hot_net) => {
            if net == Some(hot_net) {
                hot
            } else {
                base.gamma_multiply(0.25)
            }
        }
    }
}

pub(crate) fn snap(p: Point) -> Point {
    // Held keys that suspend snap (see tool_modifiers) leave the point as it is.
    if crate::tool_modifiers::snap_suspended() {
        return p;
    }
    Point::new((p.x * 2.0).round() / 2.0, (p.y * 2.0).round() / 2.0)
}
fn fill_polygon(painter: &egui::Painter, points: &[Pos2], color: Color32, clip: Rect) {
    let points = if points.first() == points.last() {
        &points[..points.len().saturating_sub(1)]
    } else {
        points
    };
    if points.len() < 3 {
        return;
    }
    let mut sign = 0.0_f32;
    let mut convex = true;
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        let c = points[(i + 2) % points.len()];
        let cross = (b.x - a.x) * (c.y - b.y) - (b.y - a.y) * (c.x - b.x);
        if cross.abs() > 1e-5 {
            if sign != 0.0 && sign.signum() != cross.signum() {
                convex = false;
                break;
            }
            sign = cross;
        }
    }
    if convex {
        painter.add(egui::Shape::convex_polygon(
            points.to_vec(),
            color,
            Stroke::NONE,
        ));
        return;
    }
    // epaint fills convex paths only. Concave library artwork uses a clipped
    // one-pixel scanline fill, preserving the polygon rather than a convex hull.
    let top = points
        .iter()
        .map(|p| p.y)
        .fold(f32::INFINITY, f32::min)
        .max(clip.top())
        .ceil();
    let bottom = points
        .iter()
        .map(|p| p.y)
        .fold(f32::NEG_INFINITY, f32::max)
        .min(clip.bottom())
        .floor();
    if !top.is_finite() || !bottom.is_finite() || bottom < top {
        return;
    }
    let mut intersections = Vec::new();
    for row in 0..=((bottom - top) as usize).min(8192) {
        let y = top + row as f32 + 0.5;
        intersections.clear();
        let mut previous = points[points.len() - 1];
        for current in points {
            if (current.y > y) != (previous.y > y) {
                intersections.push(
                    current.x
                        + (previous.x - current.x) * (y - current.y) / (previous.y - current.y),
                );
            }
            previous = *current;
        }
        intersections.sort_by(f32::total_cmp);
        for pair in intersections.as_chunks::<2>().0 {
            let left = pair[0].max(clip.left());
            let right = pair[1].min(clip.right());
            if right > left {
                painter.line_segment(
                    [Pos2::new(left, y), Pos2::new(right, y)],
                    Stroke::new(1.0, color),
                );
            }
        }
    }
}
/// Ray-cast point-in-polygon test for closed outlines (first vertex repeated).
pub(crate) fn polygon_contains(points: &[Point], pos: Point) -> bool {
    let mut inside = false;
    for edge in points.windows(2) {
        let (a, b) = (edge[0], edge[1]);
        if (a.y > pos.y) != (b.y > pos.y) && pos.x < (b.x - a.x) * (pos.y - a.y) / (b.y - a.y) + a.x
        {
            inside = !inside;
        }
    }
    inside
}
/// The small pictures drawn on the floating canvas controls.
#[derive(Clone, Copy)]
enum CanvasIcon {
    Fit,
    Map,
    Move,
    Rotate,
    ZoomIn,
    ZoomOut,
}
impl App {
    /// One square icon button over the canvas. Its rectangle is published for
    /// the tests under `id`.
    fn canvas_icon(
        &mut self,
        ui: &mut egui::Ui,
        id: &str,
        icon: CanvasIcon,
        tip: &str,
    ) -> egui::Response {
        let (rect, response) = ui.allocate_exact_size(Vec2::splat(28.0), Sense::click());
        let response = response.on_hover_text(tip);
        let name = match icon {
            CanvasIcon::Fit => "Fit",
            CanvasIcon::Map => "Map",
            CanvasIcon::Move => "Move",
            CanvasIcon::Rotate => "Rotate",
            CanvasIcon::ZoomIn => "Zoom in",
            CanvasIcon::ZoomOut => "Zoom out",
        };
        crate::a11y::name_control(&response, name, None);
        self.record_icon(id, rect);
        let ink = if response.hovered() {
            self.accent()
        } else {
            self.ink()
        };
        let fill = self.colours().icon_fill;
        let painter = ui.painter();
        painter.rect_filled(rect, 6.0, fill);
        painter.rect_stroke(
            rect,
            6.0,
            Stroke::new(1.0, self.ink().gamma_multiply(0.35)),
            egui::StrokeKind::Inside,
        );
        let stroke = Stroke::new(1.6, ink);
        let c = rect.center();
        let at = |x: f32, y: f32| egui::pos2(c.x + x, c.y + y);
        match icon {
            CanvasIcon::Fit => {
                // A frame: four corner brackets.
                for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                    let corner = at(sx * 7.0, sy * 7.0);
                    painter.line_segment([corner, at(sx * 7.0, sy * 3.0)], stroke);
                    painter.line_segment([corner, at(sx * 3.0, sy * 7.0)], stroke);
                }
            }
            CanvasIcon::Map => {
                // A folded map: three panels with the creases showing.
                let top = [(-8.0, -5.0), (-3.0, -7.0), (3.0, -5.0), (8.0, -7.0)];
                let bottom = [(-8.0, 7.0), (-3.0, 5.0), (3.0, 7.0), (8.0, 5.0)];
                painter.add(egui::Shape::line(
                    top.iter().map(|p| at(p.0, p.1)).collect(),
                    stroke,
                ));
                painter.add(egui::Shape::line(
                    bottom.iter().map(|p| at(p.0, p.1)).collect(),
                    stroke,
                ));
                painter.line_segment([at(-8.0, -5.0), at(-8.0, 7.0)], stroke);
                painter.line_segment([at(8.0, -7.0), at(8.0, 5.0)], stroke);
                painter.line_segment([at(-3.0, -7.0), at(-3.0, 5.0)], stroke);
                painter.line_segment([at(3.0, -5.0), at(3.0, 7.0)], stroke);
            }
            CanvasIcon::ZoomIn | CanvasIcon::ZoomOut => {
                painter.circle_stroke(at(-1.5, -1.5), 5.5, stroke);
                painter.line_segment([at(2.5, 2.5), at(7.0, 7.0)], stroke);
                painter.line_segment([at(-4.0, -1.5), at(1.0, -1.5)], stroke);
                if matches!(icon, CanvasIcon::ZoomIn) {
                    painter.line_segment([at(-1.5, -4.0), at(-1.5, 1.0)], stroke);
                }
            }
            CanvasIcon::Move => {
                painter.line_segment([at(-7.0, 0.0), at(7.0, 0.0)], stroke);
                painter.line_segment([at(0.0, -7.0), at(0.0, 7.0)], stroke);
                for (dx, dy) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
                    let tip = at(dx * 7.0, dy * 7.0);
                    let back =
                        |a: f32, b: f32| at(dx * (7.0 - a) + dy * b, dy * (7.0 - a) + dx * b);
                    painter.line_segment([tip, back(3.0, 2.5)], stroke);
                    painter.line_segment([tip, back(3.0, -2.5)], stroke);
                }
            }
            CanvasIcon::Rotate => {
                let mut points = Vec::new();
                for step in 0..=10 {
                    let a = -0.3 + step as f32 * 0.5;
                    points.push(at(7.0 * a.cos(), 7.0 * a.sin()));
                }
                painter.add(egui::Shape::line(points, stroke));
                let tip = at(7.0 * (-0.3_f32).cos(), 7.0 * (-0.3_f32).sin());
                painter.line_segment([tip, tip + Vec2::new(-4.0, -1.0)], stroke);
                painter.line_segment([tip, tip + Vec2::new(1.0, 4.5)], stroke);
            }
        }
        response
    }
}
impl App {
    /// Copper colour for one net under the current spotlight: the lit net
    /// glows in the accent colour, other copper fades away.
    fn copper(&self, net: Option<Id>, base: Color32) -> Color32 {
        spotlight(self.highlight_net, net, base, self.accent())
    }
}

impl App {
    pub(super) fn select_at(&mut self, pos: Point, zoom: f32) {
        let id = self.pick_filtered(pos, zoom);
        self.select(id);
        if self.tab == Tab::Schematic
            && let Some(c) = id.and_then(|id| self.project.component(id))
            && let Some(view) = c
                .schematic_views()
                .into_iter()
                .filter(|view| view.sheet == self.schematic_page())
                .filter(|view| {
                    let local = Point::new(pos.x - view.position.x, pos.y - view.position.y)
                        .rotate(view.rotation);
                    let points = view.drawing_points();
                    let margin = 8.0 / zoom as f64;
                    let min = points
                        .iter()
                        .fold(Point::new(f64::INFINITY, f64::INFINITY), |m, p| {
                            Point::new(m.x.min(p.x), m.y.min(p.y))
                        });
                    let max = points
                        .iter()
                        .fold(Point::new(f64::NEG_INFINITY, f64::NEG_INFINITY), |m, p| {
                            Point::new(m.x.max(p.x), m.y.max(p.y))
                        });
                    local.x >= min.x - margin
                        && local.x <= max.x + margin
                        && local.y >= min.y - margin
                        && local.y <= max.y + margin
                })
                .min_by(|a, b| {
                    a.position
                        .distance(pos)
                        .total_cmp(&b.position.distance(pos))
                })
        {
            self.selected_unit = view.library_symbol.as_ref().map(|s| s.unit).unwrap_or(1);
            self.load_properties();
        }
    }
    pub(super) fn paint_graphic(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        camera: Camera,
        graphic: &pcbcraft_library::Graphic,
        transform: (Point, f64),
        color: Color32,
    ) {
        let (origin, rotation) = transform;
        if let pcbcraft_library::Graphic::Text {
            position,
            text,
            size,
            angle,
            ..
        } = graphic
        {
            let center = camera.screen(origin.offset(position.rotate(-rotation)), rect);
            let galley = painter.layout_no_wrap(
                text.clone(),
                FontId::proportional((size.y as f32 * camera.zoom).clamp(6.0, 36.0)),
                color,
            );
            let top_left = Align2::CENTER_CENTER.anchor_size(center, galley.size()).min;
            painter.add(
                egui::epaint::TextShape::new(top_left, galley, color).with_angle_and_anchor(
                    ((-angle - rotation) * if camera.mirror { -1.0 } else { 1.0 }).to_radians()
                        as f32
                        + camera.turns as f32 * std::f32::consts::FRAC_PI_2,
                    Align2::CENTER_CENTER,
                ),
            );
        } else {
            let points = graphic.path_points();
            if points.len() > 1 {
                let (width, fill) = match graphic {
                    pcbcraft_library::Graphic::Line { width, .. }
                    | pcbcraft_library::Graphic::Arc { width, .. } => (*width, "none"),
                    pcbcraft_library::Graphic::Rectangle { width, fill, .. }
                    | pcbcraft_library::Graphic::Circle { width, fill, .. }
                    | pcbcraft_library::Graphic::Polyline { width, fill, .. } => {
                        (*width, fill.as_str())
                    }
                    _ => (0.15, "none"),
                };
                let points = points
                    .into_iter()
                    .map(|p| camera.screen(origin.offset(p.rotate(-rotation)), rect))
                    .collect::<Vec<_>>();
                if fill != "none" && fill != "no" && points.len() >= 3 {
                    fill_polygon(
                        painter,
                        &points,
                        if fill == "background" {
                            self.surface()
                        } else {
                            color
                        },
                        rect,
                    );
                }
                let stroke = Stroke::new(
                    ((if width > 0.0 { width } else { 0.15 }) as f32 * camera.zoom).max(0.7),
                    color,
                );
                painter.add(if fill != "none" && fill != "no" {
                    egui::Shape::closed_line(points, stroke)
                } else {
                    egui::Shape::line(points, stroke)
                });
            }
        }
    }
    pub(super) fn canvas(&mut self, ui: &mut egui::Ui) {
        if self.tab == Tab::Schematic {
            self.page_switcher(ui);
        }
        let (response, painter) = ui.allocate_painter(
            ui.available_size(),
            Sense::click_and_drag().union(Sense::hover()),
        );
        let rect = response.rect;
        self.canvas_rect = rect;
        let mut camera = if self.tab == Tab::Pcb {
            self.pcb
        } else {
            self.schematic
        };
        // The window and the asides settle over the first frames, so an
        // automatic fit follows the canvas size until the user moves the view.
        let fit_key = egui::Id::new(("auto-fit", self.tab == Tab::Pcb));
        let size = (rect.width().round() as i32, rect.height().round() as i32);
        let fitted_at: Option<(i32, i32)> = ui.ctx().data(|d| d.get_temp(fit_key));
        let resized = fitted_at.is_some_and(|last| last != size);
        if (self.needs_fit || resized) && rect.width() > 40.0 && rect.height() > 40.0 {
            let (lo, hi) = view_bounds(&self.project, self.tab == Tab::Pcb);
            let (keep_mirror, keep_turns) = (camera.mirror, camera.turns);
            let panel = if keep_turns % 2 == 1 {
                (rect.height(), rect.width())
            } else {
                (rect.width(), rect.height())
            };
            camera = fit_camera(lo, hi, panel);
            camera.mirror = keep_mirror;
            camera.turns = keep_turns;
            self.needs_fit = false;
            ui.ctx().data_mut(|d| d.insert_temp(fit_key, size));
        }
        if self.needs_zoom_selection
            && rect.width() > 40.0
            && rect.height() > 40.0
            && let Some(id) = self.selected
            && let Some((lo, hi)) = selection_bounds(&self.project, id)
        {
            let (keep_mirror, keep_turns) = (camera.mirror, camera.turns);
            let panel = if keep_turns % 2 == 1 {
                (rect.height(), rect.width())
            } else {
                (rect.width(), rect.height())
            };
            camera = fit_camera(lo, hi, panel);
            camera.mirror = keep_mirror;
            camera.turns = keep_turns;
            self.needs_zoom_selection = false;
        }
        let held = crate::tool_modifiers::held_now(ui.ctx());
        if response.hovered() {
            let (scroll, sideways, scale) = self.wheel_plan(ui, held);
            if sideways.abs() > 0.01 {
                camera.pan_screen(Vec2::new(sideways, 0.0));
            }
            if scroll.abs() > 0.01
                && let Some(mouse) = ui.input(|i| i.pointer.hover_pos())
            {
                let before = camera.world(mouse, rect);
                ui.ctx().data_mut(|d| d.remove_temp::<(i32, i32)>(fit_key));
                camera.zoom = (camera.zoom * (scroll * scale * 0.003).exp()).clamp(0.02, 100.0);
                let after = camera.world(mouse, rect);
                camera.center = camera
                    .center
                    .offset(Point::new(before.x - after.x, before.y - after.y));
            }
        }
        if response.dragged_by(egui::PointerButton::Middle) {
            let d = ui.input(|i| i.pointer.delta());
            ui.ctx().data_mut(|d| d.remove_temp::<(i32, i32)>(fit_key));
            camera.pan_screen(d);
        }
        if self.tab == Tab::Pcb {
            self.pcb = camera;
            self.step_layers_by_key(ui);
            let focus = self.focus_target();
            self.stack = self.stack_view(ui.ctx(), focus);
        } else {
            self.schematic = camera;
        }
        let cursor = response.hover_pos().map(|p| camera.world(p, rect));
        self.cursor = cursor;
        self.gesture_frame(ui, held, response.hovered(), cursor, camera.zoom);
        if let Some(released) = response.dnd_release_payload::<Value>() {
            let world = response
                .hover_pos()
                .or_else(|| ui.input(|i| i.pointer.hover_pos()))
                .map(|p| camera.world(p, rect));
            if let Some(world) = world
                && !self.dock_drop(ui.ctx(), &released, world)
            {
                self.place_dropped_library(&released, world);
            }
        }
        let mut visible = self.project.clone();
        if let Some((id, pos)) = self.drag {
            if let Some(c) = visible.components.iter_mut().find(|c| c.id == id) {
                if self.selected_unit == 1 {
                    c.position = pos;
                } else if let Some(unit) = c.units.iter_mut().find(|u| u.unit == self.selected_unit)
                {
                    unit.position = pos;
                }
            }
            if let Some(f) = visible.board.footprints.iter_mut().find(|f| f.id == id) {
                f.position = pos;
            }
        }
        self.press_preview(&mut visible);
        let focus = if self.tab == Tab::Pcb {
            self.focus_layer.clone()
        } else {
            None
        };
        if let Some(focus) = &focus {
            Self::apply_layer_focus(&mut visible, focus);
        }
        painter.rect_filled(
            rect,
            0.0,
            if self.tab == Tab::Pcb {
                self.art().bg
            } else {
                self.surface()
            },
        );
        if self.tab == Tab::Pcb {
            if self.preferences.grid && self.layer_shown("Grid") {
                self.paint_layout_grid(&painter, rect, camera);
            }
        } else if self.tab != Tab::Schematic && self.preferences.grid && self.layer_shown("Grid") {
            let grid = if camera.zoom < 5.0 { 5.0 } else { 1.0 };
            let step = grid * camera.zoom;
            let zero = camera.screen(Point::default(), rect);
            let x0 = rect.left() + (zero.x - rect.left()).rem_euclid(step);
            let y0 = rect.top() + (zero.y - rect.top()).rem_euclid(step);
            let color = self.muted().gamma_multiply(0.13);
            for i in 0..((rect.width() / step) as usize + 1).min(1600) {
                for j in 0..((rect.height() / step) as usize + 1).min(1000) {
                    painter.circle_filled(
                        Pos2::new(x0 + i as f32 * step, y0 + j as f32 * step),
                        0.65,
                        color,
                    );
                }
            }
        }
        if self.tab == Tab::Schematic {
            self.paint_schematic(&painter, rect, camera, &visible);
        } else {
            self.paint_board(&painter, rect, camera, &visible);
        }
        self.paint_edit_overlay(&painter, rect, camera);
        self.paint_affordance(&painter, rect, camera);
        let text = match &self.tool {
            Tool::Select => t!("board.canvas.select"),
            Tool::Place(_) => t!("board.canvas.place_symbol_click_to_commit"),
            Tool::LibraryPlace(..) | Tool::CustomPlace(..) => {
                t!("board.canvas.place_symbol_package_click_to")
            }
            Tool::Wire(None) => t!("board.canvas.wire_pick_first_pin"),
            Tool::Wire(Some(_)) => t!("board.canvas.wire_pick_second_pin"),
            Tool::Route(None) => t!("board.canvas.route_pick_first_pad"),
            Tool::Route(Some(_)) => t!("board.canvas.route_pick_second_pad"),
            Tool::Move => t!("board.canvas.move_click_to_place_selection"),
            Tool::Cutout => t!("board.canvas.cutout_click_to_place"),
            Tool::Hole => t!("board.canvas.mounting_hole_click_to_drill"),
            Tool::Keepout => t!("board.canvas.keepout_pick_corner"),
            Tool::NoConnect => t!("board.canvas.no_connect_click_a_pin"),
            Tool::Fiducial => t!("board.canvas.assembly_mark_click_a_clear"),
            Tool::DrawLayer => t!("board.canvas.draw_shape_pick_the_layer"),
            Tool::Measure => t!("board.canvas.measure_click_two_points"),
            Tool::Via => t!("board.canvas.via"),
        };
        if !matches!(self.tool, Tool::Select) {
            painter.text(
                rect.left_top() + Vec2::new(20.0, 20.0),
                Align2::LEFT_TOP,
                text,
                FontId::monospace(10.0),
                self.muted(),
            );
        }
        if let Some(focus) = &focus {
            let label = match focus.as_str() {
                "copper:front" => "Top copper".to_owned(),
                "copper:back" => "Bottom copper".to_owned(),
                _ => focus.trim_start_matches("art:").to_owned(),
            };
            painter.text(
                rect.left_top() + Vec2::new(20.0, 40.0),
                Align2::LEFT_TOP,
                t!("board.canvas.focused_on_label_tap_the", label = label),
                FontId::monospace(10.0),
                self.accent(),
            );
        }
        if let Some(hovered) = response.dnd_hover_payload::<Value>() {
            let name = hovered
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("part");
            painter.text(
                rect.left_top() + Vec2::new(20.0, 38.0),
                Align2::LEFT_TOP,
                t!("board.canvas.release_to_place_name_where", name = name),
                FontId::monospace(10.0),
                self.accent(),
            );
        }
        if let Some(pos) = cursor {
            let snapped = self.constrained(snap(pos));
            match &self.tool {
                Tool::Place(kind) => {
                    if let Some(c) =
                        pcbcraft_doc::component_template(kind, 1, "preview".into(), snapped)
                    {
                        self.symbol(&painter, rect, camera, &c, false, true);
                    }
                }
                Tool::LibraryPlace(_, _, definition) | Tool::CustomPlace(_, definition) => {
                    if let Ok(c) = pcbcraft_doc::library_component(
                        definition.as_ref().clone(),
                        "preview".into(),
                        0,
                        format!("{}?", definition.reference),
                        snapped,
                        None,
                    ) {
                        for view in c.schematic_views() {
                            self.symbol(&painter, rect, camera, &view, false, true);
                        }
                    }
                }
                Tool::Fiducial => {
                    painter.circle_stroke(
                        camera.screen(snapped, rect),
                        (self.fiducial_diameter / 2.0) as f32 * camera.zoom,
                        Stroke::new(1.5, self.accent()),
                    );
                }
                Tool::Keepout => self.paint_keepout_ghost(&painter, rect, camera, snapped),
                Tool::Hole => {
                    painter.circle_stroke(
                        camera.screen(snapped, rect),
                        (self.hole_diameter / 2.0) as f32 * camera.zoom,
                        Stroke::new(1.5, self.accent()),
                    );
                }
                Tool::Cutout if pcbcraft_geom::positive(self.cutout_radius) => {
                    painter.circle_stroke(
                        camera.screen(snapped, rect),
                        self.cutout_radius as f32 * camera.zoom,
                        Stroke::new(1.5, self.accent()),
                    );
                }
                Tool::Wire(Some(r)) => {
                    if let Some(start) = self.project.pin_position(r) {
                        let line: Vec<Pos2> = self
                            .wire_preview_points(start, snapped)
                            .into_iter()
                            .map(|q| camera.screen(q, rect))
                            .collect();
                        painter.add(egui::Shape::line(line, Stroke::new(1.5, self.accent())));
                    }
                }
                Tool::Route(Some((id, pad))) => {
                    if let Some(start) = self
                        .project
                        .footprint(*id)
                        .and_then(|f| f.pad_position(pad))
                    {
                        self.path(
                            &painter,
                            rect,
                            camera,
                            start,
                            snapped,
                            Stroke::new(3.0, self.colours().canvas.front),
                        );
                    }
                }
                Tool::DrawLayer => {
                    let stroke = Stroke::new(1.0, self.accent());
                    match self.draw_shape() {
                        DrawShape::Words => {
                            if !self.art_text.trim().is_empty() {
                                painter.text(
                                    camera.screen(snapped, rect),
                                    Align2::LEFT_TOP,
                                    &self.art_text,
                                    FontId::proportional(11.0),
                                    self.accent(),
                                );
                            }
                        }
                        DrawShape::Poly => {
                            let mut pts: Vec<Pos2> = self
                                .art_points
                                .iter()
                                .map(|pt| camera.screen(*pt, rect))
                                .collect();
                            pts.push(camera.screen(snapped, rect));
                            if pts.len() >= 2 {
                                painter.add(egui::Shape::line(pts, stroke));
                            }
                        }
                        _ => {
                            if let Some(start) = self.art_start {
                                let a = camera.screen(start, rect);
                                let b = camera.screen(snapped, rect);
                                match self.draw_shape() {
                                    DrawShape::Line => {
                                        painter.line_segment([a, b], stroke);
                                    }
                                    DrawShape::Rect => {
                                        painter.rect_stroke(
                                            egui::Rect::from_min_max(a, b),
                                            0.0,
                                            stroke,
                                            egui::StrokeKind::Inside,
                                        );
                                    }
                                    _ => {
                                        let radius = (b - a).length().max(1.0);
                                        painter.circle_stroke(a, radius, stroke);
                                    }
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        self.paint_route_drag(ui, &painter, rect, camera, cursor);
        if response.drag_started_by(egui::PointerButton::Primary)
            && let Some(pos) = cursor
        {
            self.begin_route_drag(pos, camera.zoom);
        }
        if response.drag_stopped_by(egui::PointerButton::Primary)
            && self.route_drag.active()
            && let Some(pos) = cursor
        {
            self.end_route_drag(pos, camera.zoom);
        }
        if response.drag_started_by(egui::PointerButton::Primary)
            && matches!(self.tool, Tool::Select)
            && !self.route_drag.active()
            && let Some(pos) = ui
                .input(|i| i.pointer.press_origin())
                .map(|p| camera.world(p, rect))
                .or(cursor)
        {
            // The drag starts a few points after the press: decide by what the
            // pointer went down on, not by where it is now.
            self.press_start(pos, camera.zoom, held);
        }
        self.keepout_drag(&response, cursor);
        if response.dragged_by(egui::PointerButton::Primary)
            && let Some(pos) = cursor
        {
            self.press_move(pos, held);
        }
        if response.drag_stopped_by(egui::PointerButton::Primary) {
            self.press_end(cursor, camera.zoom, held);
        }
        if ui.input(|i| i.key_pressed(egui::Key::Enter))
            && matches!(self.tool, Tool::DrawLayer)
            && self.draw_can_finish()
        {
            match (self.current_target(), self.art_layer) {
                (crate::draw_target::Target::User(_), Some(layer)) => self.commit_poly(layer),
                (crate::draw_target::Target::User(_), None) => {}
                _ => self.draw_finish(),
            }
        }
        if response.clicked() {
            self.canvas_menu = None;
            if let Some(pos) = cursor {
                self.click_dispatch(pos, camera.zoom, held);
            }
        }
        if response.double_clicked()
            && let Some(pos) = cursor
        {
            self.double_click(pos, camera.zoom, held);
        }
        self.rename_prompt(ui.ctx());
        if response.secondary_clicked()
            && let Some(pos) = cursor
            && let Some(screen) = response.interact_pointer_pos()
        {
            if matches!(self.tool, Tool::Select)
                && matches!(
                    crate::tool_modifiers::resolve(
                        &self.preferences.modifiers,
                        crate::tool_modifiers::ToolKind::Select,
                        crate::tool_modifiers::Workspace::Board,
                        held,
                        crate::tool_modifiers::Target::Part,
                        crate::tool_modifiers::Gesture::Click,
                    ),
                    crate::tool_modifiers::ClickAction::Add
                        | crate::tool_modifiers::ClickAction::Toggle
                )
            {
                self.shift_click(pos, camera.zoom);
            } else {
                self.open_canvas_menu(screen, pos, camera.zoom);
            }
        }
        // Fit and zoom stay on the canvas; board/schematic tools live in the strip under the tabs.
        let card = egui::Area::new(egui::Id::new("fit-fab"))
            .pivot(Align2::RIGHT_BOTTOM)
            .fixed_pos(rect.right_bottom() + Vec2::new(-16.0, -16.0))
            .constrain_to(rect)
            .order(egui::Order::Foreground)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style())
                    .inner_margin(egui::Margin::same(4))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let zoom_now = camera.zoom;
                            let step = |app: &mut App, factor: f32| {
                                let zoom = if app.tab == Tab::Pcb {
                                    &mut app.pcb.zoom
                                } else {
                                    &mut app.schematic.zoom
                                };
                                *zoom = (*zoom * factor).clamp(0.02, 100.0);
                            };
                            if self
                                .canvas_icon(
                                    ui,
                                    "canvas-zoom-out",
                                    CanvasIcon::ZoomOut,
                                    &t!("board.canvas.text2.tip"),
                                )
                                .clicked()
                            {
                                step(self, 1.0 / 1.15);
                            }
                            let base = if self.tab == Tab::Schematic {
                                15.0
                            } else {
                                11.0
                            };
                            ui.label(
                                egui::RichText::new(format!(
                                    "{}%",
                                    (zoom_now / base * 100.0).round() as i32
                                ))
                                .size(12.0),
                            )
                            .on_hover_text(t!("board.footer.zoom_pct.tip"));
                            if self
                                .canvas_icon(
                                    ui,
                                    "canvas-zoom-in",
                                    CanvasIcon::ZoomIn,
                                    &t!("board.canvas.text.tip"),
                                )
                                .clicked()
                            {
                                step(self, 1.15);
                            }
                            if self
                                .canvas_icon(
                                    ui,
                                    "canvas-fit",
                                    CanvasIcon::Fit,
                                    &t!("board.canvas.fit_zoom_the_view_so"),
                                )
                                .clicked()
                            {
                                self.needs_fit = true;
                            }
                            if self
                                .canvas_icon(
                                    ui,
                                    "canvas-map",
                                    CanvasIcon::Map,
                                    &t!("board.canvas.map.tip"),
                                )
                                .clicked()
                            {
                                self.minimap_toggle();
                            }
                        });
                    });
            });
        self.minimap_show(ui.ctx(), rect, card.response.rect);
        if matches!(self.tab, Tab::Schematic | Tab::Pcb)
            && let Some(id) = self.selected
        {
            let world = self
                .project
                .component(id)
                .and_then(|c| c.unit_placement(self.selected_unit).map(|p| p.0))
                .or_else(|| self.project.footprint(id).map(|f| f.position));
            if let Some(at) = world {
                let screen = camera.screen(at, rect);
                if rect.contains(screen) {
                    let flip_left = screen.x > rect.right() - 72.0;
                    let (pivot, fixed) = if flip_left {
                        (Align2::RIGHT_BOTTOM, screen + Vec2::new(-12.0, -34.0))
                    } else {
                        (Align2::LEFT_BOTTOM, screen + Vec2::new(12.0, -34.0))
                    };
                    egui::Area::new(egui::Id::new("selection-handles"))
                        .pivot(pivot)
                        .fixed_pos(fixed)
                        .constrain_to(rect)
                        .order(egui::Order::Foreground)
                        .show(ui.ctx(), |ui| {
                            ui.horizontal(|ui| {
                                if self
                                    .canvas_icon(
                                        ui,
                                        "handle-move",
                                        CanvasIcon::Move,
                                        &t!("board.canvas.move_drop_the_selected_object"),
                                    )
                                    .clicked()
                                {
                                    self.tool = Tool::Move;
                                }
                                if self
                                    .canvas_icon(
                                        ui,
                                        "handle-rotate",
                                        CanvasIcon::Rotate,
                                        &command_tip("component.rotate"),
                                    )
                                    .clicked()
                                {
                                    self.rotate();
                                }
                            });
                        });
                }
            }
        }
        if let Some((screen, kind)) = self.canvas_menu.take() {
            self.show_canvas_menu(ui.ctx(), screen, kind);
        }
    }
    /// Draw the shapes the designer put on their own layers. Cut layers read
    /// as a strong outline, perforation layers are dotted so they say
    /// "break apart here", and the rest stay quiet so copper stays loudest.
    /// Keep only what a focused tree row points at: one copper face or one
    /// drawn machine layer. Everything else falls back to plain board colour.
    fn apply_layer_focus(p: &mut Project, focus: &str) {
        let bottom = p.bottom_layer();
        let board = &mut p.board;
        match focus.strip_prefix("copper:") {
            Some("front") => {
                board.tracks.retain(|t| t.layer == Layer::Front);
                board.zones.retain(|z| z.layer == Layer::Front);
                board.texts.retain(|t| t.layer == Layer::Front);
                board.vias.clear();
            }
            Some("back") => {
                board.tracks.retain(|t| t.layer == bottom);
                board.zones.retain(|z| z.layer == bottom);
                board.texts.retain(|t| t.layer == bottom);
                board.vias.clear();
            }
            _ => {}
        }
        if let Some(name) = focus.strip_prefix("art:") {
            board.tracks.clear();
            board.zones.clear();
            board.texts.clear();
            board.vias.clear();
            board.user_layers.retain(|l| l.name == name);
        }
    }

    fn draw_user_layers(&self, painter: &egui::Painter, rect: Rect, camera: Camera, p: &Project) {
        for layer in &p.board.user_layers {
            if !layer.visible {
                continue;
            }
            let colour = match layer.kind {
                pcbcraft_doc::ArtKind::Cut => self.colours().canvas.front,
                pcbcraft_doc::ArtKind::Perforation => self.colours().canvas.pad,
                pcbcraft_doc::ArtKind::Silk => self.ink(),
                pcbcraft_doc::ArtKind::Mask => self.colours().canvas.mask,
                pcbcraft_doc::ArtKind::Other => self.ink().linear_multiply(0.55),
            };
            let stroke = Stroke::new(1.2, colour);
            let dotted = layer.dotted;
            for shape in &layer.shapes {
                match shape {
                    pcbcraft_doc::ArtShape::Line { start, end } => {
                        Self::push_art(
                            painter,
                            vec![camera.screen(*start, rect), camera.screen(*end, rect)],
                            stroke,
                            dotted,
                        );
                    }
                    pcbcraft_doc::ArtShape::Rect {
                        x,
                        y,
                        width,
                        height,
                        rotate,
                    } => {
                        let center = Point::new(x + width / 2.0, y + height / 2.0);
                        let mut ring: Vec<Pos2> = [
                            Point::new(*x, *y),
                            Point::new(x + width, *y),
                            Point::new(x + width, y + height),
                            Point::new(*x, y + height),
                        ]
                        .into_iter()
                        .map(|c| {
                            center
                                .offset(Point::new(c.x - center.x, c.y - center.y).rotate(*rotate))
                        })
                        .map(|c| camera.screen(c, rect))
                        .collect();
                        ring.push(ring[0]);
                        Self::push_art(painter, ring, stroke, dotted);
                    }
                    pcbcraft_doc::ArtShape::Circle { center, radius } => {
                        let c = camera.screen(*center, rect);
                        let r = (*radius as f32 * camera.zoom).max(1.0);
                        let ring: Vec<Pos2> = (0..=48)
                            .map(|i| {
                                let a = std::f32::consts::TAU * i as f32 / 48.0;
                                c + egui::vec2(a.cos() * r, a.sin() * r)
                            })
                            .collect();
                        Self::push_art(painter, ring, stroke, dotted);
                    }
                    pcbcraft_doc::ArtShape::Poly { points } => {
                        if points.len() < 2 {
                            continue;
                        }
                        let mut ring: Vec<Pos2> =
                            points.iter().map(|pt| camera.screen(*pt, rect)).collect();
                        ring.push(ring[0]);
                        Self::push_art(painter, ring, stroke, dotted);
                    }
                    pcbcraft_doc::ArtShape::Text { at, text, size, .. } => {
                        painter.text(
                            camera.screen(*at, rect),
                            Align2::LEFT_TOP,
                            text,
                            FontId::proportional((*size as f32 * camera.zoom).clamp(6.0, 36.0)),
                            colour,
                        );
                    }
                }
            }
        }
    }

    /// One polyline in the layer's style: plain, or broken into dashes when
    /// the layer is a perforation layer.
    fn push_art(painter: &egui::Painter, points: Vec<Pos2>, stroke: Stroke, dotted: bool) {
        if dotted {
            for shape in egui::Shape::dashed_line(&points, stroke, 2.0, 3.0) {
                painter.add(shape);
            }
        } else {
            painter.add(egui::Shape::line(points, stroke));
        }
    }

    /// Close the outline started with the outline draw tool, if enough
    /// corners exist, and hand it to the engine.
    fn commit_poly(&mut self, layer: Id) {
        let points: Vec<[f64; 2]> = self
            .art_points
            .iter()
            .map(|pt| {
                let q = snap(*pt);
                [q.x, q.y]
            })
            .collect();
        self.art_points.clear();
        self.run(
            "board.layer_poly_add",
            json!({"layer": layer, "points": points}),
        );
    }

    /// The object under `pos`, leaving out anything on a layer the Layers tree
    /// locked.
    ///
    /// # Dependencies
    /// - `crates/ui-egui/src/layers_tree.rs` `is_locked` decides what a lock covers.
    pub(super) fn pick_object(&self, pos: Point, zoom: f32) -> Option<Id> {
        self.pick_object_any(pos, zoom)
            .filter(|id| !crate::layers_tree::is_locked(&self.project, &self.tree.locked, *id))
            .filter(|id| !self.sheet_row_hidden(*id))
    }
    /// True when the sheet row an object belongs to is switched off with the eye,
    /// so the canvas draws nothing there to pick.
    pub(super) fn sheet_row_hidden(&self, id: Id) -> bool {
        crate::layers_tree::sheet_row_of(&self.project, id)
            .is_some_and(|(page, row)| !self.page_row_shown(page, row))
    }
    fn pick_object_any(&self, pos: Point, zoom: f32) -> Option<Id> {
        if self.tab == Tab::Schematic {
            let page = self.schematic_page();
            self.project
                .components
                .iter()
                .flat_map(|c| c.schematic_views())
                .filter(|c| c.sheet == page)
                .filter(|c| {
                    let local =
                        Point::new(pos.x - c.position.x, pos.y - c.position.y).rotate(c.rotation);
                    let points = c.drawing_points();
                    let margin = 8.0 / zoom as f64;
                    let min = points
                        .iter()
                        .fold(Point::new(f64::INFINITY, f64::INFINITY), |m, p| {
                            Point::new(m.x.min(p.x), m.y.min(p.y))
                        });
                    let max = points
                        .iter()
                        .fold(Point::new(f64::NEG_INFINITY, f64::NEG_INFINITY), |m, p| {
                            Point::new(m.x.max(p.x), m.y.max(p.y))
                        });
                    local.x >= min.x - margin
                        && local.x <= max.x + margin
                        && local.y >= min.y - margin
                        && local.y <= max.y + margin
                })
                .min_by(|a, b| {
                    a.position
                        .distance(pos)
                        .total_cmp(&b.position.distance(pos))
                })
                .map(|c| c.id)
        } else {
            if let Some(f) = self.project.board.footprints.iter().find(|f| {
                let local =
                    Point::new(pos.x - f.position.x, pos.y - f.position.y).rotate(f.rotation);
                let points = f.drawing_points(&self.project);
                let margin = 8.0 / zoom as f64;
                let min = points
                    .iter()
                    .fold(Point::new(f64::INFINITY, f64::INFINITY), |m, p| {
                        Point::new(m.x.min(p.x), m.y.min(p.y))
                    });
                let max = points
                    .iter()
                    .fold(Point::new(f64::NEG_INFINITY, f64::NEG_INFINITY), |m, p| {
                        Point::new(m.x.max(p.x), m.y.max(p.y))
                    });
                local.x >= min.x - margin
                    && local.x <= max.x + margin
                    && local.y >= min.y - margin
                    && local.y <= max.y + margin
            }) {
                return Some(f.id);
            }
            if let Some(c) = self
                .project
                .board
                .cutouts
                .iter()
                .find(|c| (c.center.distance(pos) - c.radius).abs() * (zoom as f64) < 10.0)
            {
                return Some(c.id);
            }
            if let Some(z) = self
                .project
                .board
                .zones
                .iter()
                .find(|z| polygon_contains(&z.outline, pos))
            {
                return Some(z.id);
            }
            self.project
                .board
                .tracks
                .iter()
                .find(|t| point_segment(pos, t.start, t.end) * (zoom as f64) < 8.0)
                .map(|t| t.id)
        }
    }
    pub(super) fn select_hover(&mut self) {
        if let Some(pos) = self.cursor {
            let zoom = if self.tab == Tab::Pcb {
                self.pcb.zoom
            } else {
                self.schematic.zoom
            };
            if self.pick_object(pos, zoom).is_some() {
                self.select_at(pos, zoom);
            }
        }
    }
    fn pick_pin(&self, pos: Point, zoom: f32) -> Option<PinRef> {
        let page = self.schematic_page();
        for c in self.project.components.iter().filter(|c| c.sheet == page) {
            for pin in &c.pins {
                if c.pin_position(&pin.number)
                    .is_some_and(|p| p.distance(pos) * (zoom as f64) < 12.0)
                {
                    return Some(PinRef {
                        component: c.id,
                        pin: pin.number.clone(),
                    });
                }
            }
        }
        None
    }
    /// When both clicked pads sit on the same net of a declared differential
    /// pair, the pad each of those footprints has on the partner net, so one
    /// drag routes both sides. None when the net is not paired or a footprint
    /// has no single partner pad.
    pub(super) fn pair_mates(
        &self,
        from: Id,
        from_pad: &str,
        to: Id,
        to_pad: &str,
    ) -> Option<(String, String)> {
        let net_of = |fp: Id, number: &str| {
            self.project
                .footprint(fp)?
                .pads
                .iter()
                .find(|p| p.number == number)?
                .net
        };
        let net = net_of(from, from_pad)?;
        if net_of(to, to_pad)? != net {
            return None;
        }
        let pair = self
            .project
            .board
            .diff_pairs
            .iter()
            .find(|d| d.net_p == net || d.net_n == net)?;
        let partner = if pair.net_p == net {
            pair.net_n
        } else {
            pair.net_p
        };
        let mate = |fp: Id| {
            let f = self.project.footprint(fp)?;
            let mut on_partner = f.pads.iter().filter(|p| p.net == Some(partner));
            let first = on_partner.next()?;
            if on_partner.next().is_some() {
                return None;
            }
            Some(first.number.clone())
        };
        Some((mate(from)?, mate(to)?))
    }
    pub(super) fn pick_pad(&self, pos: Point, zoom: f32) -> Option<(Id, String)> {
        for f in &self.project.board.footprints {
            for pad in &f.pads {
                if f.pad_position(&pad.number).is_some_and(|p| {
                    p.distance(pos) * (zoom as f64)
                        < (pad.diameter as f32 * zoom / 2.0 + 8.0) as f64
                }) {
                    return Some((f.id, pad.number.clone()));
                }
            }
        }
        None
    }
    /// The wire whose path passes nearest the click, within a fingertip's
    /// width; curved wires are checked along their bow too. Wires on a locked
    /// Layers tree row are skipped.
    fn pick_track(&self, pos: Point, zoom: f32) -> Option<Id> {
        let mut best: Option<(f64, Id)> = None;
        for track in &self.project.board.tracks {
            if !self.id_allowed(track.id) {
                continue;
            }
            let mut reach = segment_distance(pos, track.start, track.end);
            if let Some(mid) = track.arc {
                reach = reach
                    .min(segment_distance(pos, track.start, mid))
                    .min(segment_distance(pos, mid, track.end));
            }
            if reach * (zoom as f64) < 6.0 && best.is_none_or(|(known, _)| reach < known) {
                best = Some((reach, track.id));
            }
        }
        best.map(|(_, id)| id)
    }
    fn pick_via(&self, pos: Point, zoom: f32) -> Option<Id> {
        self.project
            .board
            .vias
            .iter()
            .filter(|v| self.id_allowed(v.id))
            .filter(|v| {
                pos.distance(v.position) * (zoom as f64)
                    < (v.diameter as f32 * zoom / 2.0 + 6.0) as f64
            })
            .min_by(|a, b| {
                a.position
                    .distance(pos)
                    .total_cmp(&b.position.distance(pos))
            })
            .map(|v| v.id)
    }
    fn canvas_menu_target(&self, pos: Point, zoom: f32) -> CanvasMenuKind {
        if self.tab == Tab::Schematic
            && let Some(pin) = self.pick_pin(pos, zoom)
        {
            return CanvasMenuKind::Pin(pin);
        }
        if self.tab == Tab::Pcb {
            if let Some((id, pad)) = self.pick_pad(pos, zoom) {
                return CanvasMenuKind::Pad(id, pad);
            } else if let Some(id) = self.pick_via(pos, zoom) {
                return CanvasMenuKind::Object(id);
            } else if let Some(id) = self.pick_track(pos, zoom) {
                return CanvasMenuKind::Track(id);
            }
        }
        if let Some(id) = self.pick_object(pos, zoom) {
            return CanvasMenuKind::Object(id);
        }
        CanvasMenuKind::Empty
    }
    fn open_canvas_menu(&mut self, screen: Pos2, pos: Point, zoom: f32) {
        let kind = self.canvas_menu_target(pos, zoom);
        match &kind {
            CanvasMenuKind::Empty => self.select(None),
            CanvasMenuKind::Pin(pin) => {
                self.select(Some(pin.component));
                self.load_properties();
            }
            CanvasMenuKind::Pad(id, _) => {
                self.select(Some(*id));
                self.load_properties();
            }
            CanvasMenuKind::Track(id) => {
                self.select(Some(*id));
                if let Some(track) = self.project.board.tracks.iter().find(|t| t.id == *id) {
                    self.track_width = track.width;
                }
                self.load_properties();
            }
            CanvasMenuKind::Object(id) => {
                self.select_at(pos, zoom);
                if self.selected != Some(*id) {
                    self.select(Some(*id));
                }
                self.load_properties();
            }
        }
        self.edit.menu_at = Some(pos);
        self.canvas_menu = Some((screen, kind));
    }
    pub(super) fn canvas_click(&mut self, pos: Point, zoom: f32) {
        match self.tool.clone() {
            Tool::Select => {
                self.select_at(pos, zoom);
                // A click on a pin or pad starts a wire or a track straight
                // away; no tool button has to be armed first.
                if self.tab == Tab::Schematic
                    && let Some(pin) = self.pick_pin(pos, zoom)
                {
                    self.tool = Tool::Wire(Some(pin));
                } else if self.tab == Tab::Pcb
                    && let Some((id, pad)) = self.pick_pad(pos, zoom)
                {
                    self.tool = Tool::Route(Some((id, pad)));
                } else if self.tab == Tab::Pcb
                    && self.selected.is_none()
                    && let Some(id) = self.pick_track(pos, zoom)
                {
                    self.select(Some(id));
                    if let Some(track) = self.project.board.tracks.iter().find(|t| t.id == id) {
                        self.track_width = track.width;
                    }
                    self.message = t!("board.canvas_click.wire_picked_change_its_width");
                }
            }
            Tool::Measure => self.measure_click(pos, zoom),
            Tool::Place(kind) => {
                self.run("component.add", json!({"kind":kind,"position":snap(pos)}))
            }
            Tool::LibraryPlace(path, symbol, _) => {
                self.run(
                    "component.place_library",
                    json!({"path":path,"symbol":symbol,"position":snap(pos)}),
                );
            }
            Tool::CustomPlace(name, _) => self.run(
                "component.place_custom",
                json!({"name":name,"position":snap(pos)}),
            ),
            Tool::Move => {
                if let Some(id) = self.selected {
                    let mut params = json!({"id":id,"position":snap(pos)});
                    if self.project.component(id).is_some() {
                        params["unit"] = json!(self.selected_unit);
                    }
                    self.run(
                        if self.project.footprint(id).is_some() {
                            "footprint.move"
                        } else {
                            "component.move"
                        },
                        params,
                    );
                    self.tool = Tool::Select;
                }
            }
            Tool::Wire(start) => {
                if let Some(pin) = self.pick_pin(pos, zoom) {
                    if let Some(start) = start {
                        self.run("wire.connect", json!({"from":start,"to":pin}));
                        self.tool = Tool::Select;
                    } else {
                        self.tool = Tool::Wire(Some(pin));
                    }
                } else {
                    self.message = t!("board.canvas_click.wire_endpoints_must_be_symbol");
                }
            }
            Tool::Route(start) => {
                if let Some((id, pad)) = self.pick_pad(pos, zoom) {
                    if let Some((from, from_pad)) = start {
                        self.finish_route(from, &from_pad, id, &pad);
                    } else {
                        self.tool = Tool::Route(Some((id, pad)));
                    }
                } else {
                    self.message = t!("board.canvas_click.pick_a_pad_to_start");
                }
            }
            Tool::Via => self.place_via_click(pos, zoom),
            Tool::Fiducial => self.run(
                "board.fiducial_place",
                json!({"position":snap(pos),"diameter":self.fiducial_diameter}),
            ),
            Tool::NoConnect => {
                if let Some(pin) = self.pick_pin(pos, zoom) {
                    if let Some(id) = self
                        .project
                        .no_connects
                        .iter()
                        .find(|f| f.pin == pin)
                        .map(|f| f.id)
                    {
                        self.run("object.delete", json!({"id": id}));
                    } else {
                        self.run("pin.no_connect", json!({"pin": pin}));
                    }
                }
            }
            Tool::Keepout => {}
            Tool::Hole => self.run(
                "board.hole_place",
                json!({"position":snap(pos),"diameter":self.hole_diameter}),
            ),
            Tool::Cutout => self.run(
                "board.cutout.add",
                json!({"center":snap(pos),"radius":self.cutout_radius}),
            ),
            Tool::DrawLayer
                if !matches!(self.current_target(), crate::draw_target::Target::User(_)) =>
            {
                self.draw_click(pos, zoom);
            }
            Tool::DrawLayer => {
                let Some(layer) = self.art_layer else {
                    self.message = t!("board.canvas_click.pick_or_make_a_layer");
                    return;
                };
                let at = snap(pos);
                match self.art_shape {
                    DrawShape::Words => {
                        let text = self.art_text.trim().to_owned();
                        if text.is_empty() {
                            self.message = t!("board.canvas_click.type_the_words_to_print");
                        } else {
                            self.run(
                                "board.layer_text_add",
                                json!({"layer": layer, "x": at.x, "y": at.y, "text": text}),
                            );
                        }
                    }
                    DrawShape::Poly => {
                        let closing = self
                            .art_points
                            .first()
                            .is_some_and(|first| pos.distance(*first) < 6.0 / zoom as f64);
                        if closing && self.art_points.len() >= 3 {
                            self.commit_poly(layer);
                        } else if self.art_points.len() >= 64 {
                            self.message = t!("board.canvas_click.an_outline_holds_up_to");
                        } else {
                            self.art_points.push(pos);
                        }
                    }
                    _ => match self.art_start {
                        None => self.art_start = Some(pos),
                        Some(start) => {
                            let start = snap(start);
                            match self.art_shape {
                                DrawShape::Line => self.run(
                                    "board.layer_line_add",
                                    json!({
                                        "layer": layer,
                                        "x1": start.x,
                                        "y1": start.y,
                                        "x2": at.x,
                                        "y2": at.y,
                                    }),
                                ),
                                DrawShape::Rect => self.run(
                                    "board.layer_rect_add",
                                    json!({
                                        "layer": layer,
                                        "x": start.x.min(at.x),
                                        "y": start.y.min(at.y),
                                        "width": (at.x - start.x).abs(),
                                        "height": (at.y - start.y).abs(),
                                    }),
                                ),
                                _ => self.run(
                                    "board.layer_circle_add",
                                    json!({
                                        "layer": layer,
                                        "x": start.x,
                                        "y": start.y,
                                        "radius": start.distance(at),
                                    }),
                                ),
                            }
                            self.art_start = None;
                        }
                    },
                }
            }
        }
    }
    fn path(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        camera: Camera,
        a: Point,
        b: Point,
        stroke: Stroke,
    ) {
        let mid = (a.x + b.x) / 2.0;
        painter.add(egui::Shape::line(
            vec![
                camera.screen(a, rect),
                camera.screen(Point::new(mid, a.y), rect),
                camera.screen(Point::new(mid, b.y), rect),
                camera.screen(b, rect),
            ],
            stroke,
        ));
    }
    pub(crate) fn paint_schematic(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        camera: Camera,
        p: &Project,
    ) {
        let page = self.schematic_page();
        let shown = |name: &str| self.layer_shown(name);
        let row_shown = |row: &str| self.page_row_shown(page, row);
        let sc = sheet::SheetColours::for_theme(self.colours().dark);
let mut sheet_frame = sheet::sheet_for(p, page);
        // The title block names the page being drawn, not always the first.
        sheet_frame.page = 1 + p
            .sheets
            .iter()
            .position(|s| s.id == page)
            .map_or(0, |i| i + 1);
        sheet_frame.title_shown &= row_shown("title");
        sheet::paint_sheet(
            painter,
            rect,
            camera,
            &sheet_frame,
            &sc,
            self.preferences.grid && shown("Grid"),
        );
        underlay::paint(painter, rect, camera, p, "schematic");
        let kinds = sheet::power_nets(p);
        let wire = Stroke::new(2.5, sc.wire);
        for w in p
            .wires
            .iter()
            .filter(|w| on_page(p, page, w.id))
            .filter(|_| shown("Wires"))
        {
            // A ground net is drawn as stubs and symbols, not as a wire.
            if kinds.get(&w.net) == Some(&sheet::PowerKind::Ground) {
                continue;
            }
            let Some(path) = p.wire_path(w) else {
                continue;
            };
            painter.add(egui::Shape::line(
                path.iter()
                    .map(|point| camera.screen(*point, rect))
                    .collect(),
                wire,
            ));
            let Some(run) = sheet::longest_run(&path) else {
                continue;
            };
            let fraction = if kinds.contains_key(&w.net) {
                0.72
            } else {
                0.5
            };
            let at = camera.screen(sheet::run_point(run, fraction), rect);
            let vertical = (run.0.x - run.1.x).abs() < 1e-6;
            let (anchor, offset) = if vertical {
                (Align2::LEFT_CENTER, Vec2::new(7.0, 0.0))
            } else {
                (Align2::CENTER_BOTTOM, Vec2::new(0.0, -6.0))
            };
            painter.text(
                at + offset,
                anchor,
                p.net_name(w.net),
                FontId::monospace(12.0),
                sc.wire,
            );
        }
if row_shown("power") {
            sheet::paint_power(painter, rect, camera, p, &kinds, &sc, shown("Wires"), page);
        }
        for w in p
            .drawn_wires
            .iter()
            .filter(|w| on_page(p, page, w.id))
            .filter(|_| shown("Wires"))
        {
            painter.add(egui::Shape::line(
                w.points
                    .iter()
                    .map(|point| camera.screen(*point, rect))
                    .collect(),
                wire,
            ));
        }
        for g in p
            .graphics
            .iter()
            .filter(|g| g.layer == "schematic" && on_page(p, page, g.id))
            .filter(|_| shown("Graphics") && row_shown("drawing"))
        {
            let ink = hex_colour(&g.color).unwrap_or(sc.body);
            if let Some(outline) = g.outline() {
                painter.add(egui::Shape::line(
                    outline
                        .iter()
                        .map(|point| camera.screen(*point, rect))
                        .collect(),
                    Stroke::new((g.width as f32 * camera.zoom).clamp(1.6, 12.0), ink),
                ));
            } else if let Some(anchor) = g.points.first() {
                painter.text(
                    camera.screen(*anchor, rect),
                    Align2::LEFT_TOP,
                    &g.text,
                    FontId::proportional(g.width as f32),
                    sc.text,
                );
            }
        }
        for l in p
            .labels
            .iter()
            .filter(|l| on_page(p, page, l.id))
            .filter(|l| {
                if l.kind == "power" {
                    row_shown("power")
                } else {
                    shown("Net labels")
                }
            })
        {
            let at = camera.screen(l.position, rect);
            if l.kind == "junction" {
                painter.circle_filled(at, 4.5, sc.wire);
            } else if l.kind == "power" {
                painter.circle_filled(at, 3.0, sc.wire);
                painter.text(
                    at + egui::vec2(6.0, 0.0),
                    Align2::LEFT_CENTER,
                    p.net_name(l.net),
                    crate::fonts::Role::Mono.font(),
                    sc.wire,
                );
            } else {
                let text = match l.kind.as_str() {
                    "global" => format!("{{{}}}", p.net_name(l.net)),
                    "hierarchical" => format!("/{}", p.net_name(l.net)),
                    "bus" => l.text.clone(),
                    _ => p.net_name(l.net).to_string(),
                };
                painter.text(
                    at,
                    Align2::CENTER_BOTTOM,
                    text,
                    FontId::monospace(12.0),
                    sc.wire,
                );
            }
        }
        for port in p.ports.iter().filter(|port| port.sheet == page) {
            let at = camera.screen(port.position, rect);
            painter.text(
                at,
                Align2::CENTER_CENTER,
                format!("[{}]", port.name),
                FontId::monospace(12.0),
                sc.wire,
            );
        }
        for f in p.no_connects.iter().filter(|f| on_page(p, page, f.id)) {
            if let Some(anchor) = p.pin_position(&f.pin) {
                let at = camera.screen(anchor, rect);
                painter.line_segment(
                    [at + egui::vec2(-4.0, -4.0), at + egui::vec2(4.0, 4.0)],
                    Stroke::new(1.6, self.colours().canvas.front),
                );
                painter.line_segment(
                    [at + egui::vec2(-4.0, 4.0), at + egui::vec2(4.0, -4.0)],
                    Stroke::new(1.6, self.colours().canvas.front),
                );
            }
        }
        for c in p
            .components
            .iter()
            .flat_map(|c| c.schematic_views())
            .filter(|c| c.sheet == page)
            .filter(|_| shown("Symbols") && row_shown("symbols"))
        {
            self.symbol(
                painter,
                rect,
                camera,
                &c,
                self.selected == Some(c.id)
                    && self.selected_unit == c.library_symbol.as_ref().map(|s| s.unit).unwrap_or(1),
                false,
            );
        }
        let outlines = sheet::hit_outlines(p, rect, camera, &kinds);
        sheet::hover_outline(painter, &outlines, sc.select);
        if p.components.iter().all(|c| c.sheet != page) {
            painter.text(
                rect.center() - Vec2::new(0.0, 25.0),
                Align2::CENTER_CENTER,
                t!("schematic.paint_schematic.start_your_circuit"),
                FontId::proportional(28.0),
                self.ink(),
            );
            painter.text(
                rect.center() + Vec2::new(0.0, 20.0),
                Align2::CENTER_CENTER,
                t!("schematic.paint_schematic.press_a_to_add_a"),
                FontId::proportional(14.0),
                self.muted(),
            );
        }
    }
    pub(super) fn symbol(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        camera: Camera,
        c: &Component,
        selected: bool,
        ghost: bool,
    ) {
        let sc = sheet::SheetColours::for_theme(self.colours().dark);
        let at = |p: Point| camera.screen(c.position.offset(p.rotate(-c.rotation)), rect);
        let color = if let Some(kind) = self.compare_tint.get(&c.id).copied() {
            self.compare_kind_color(kind)
        } else if selected || ghost {
            self.accent()
        } else {
            sc.body
        };
        let text_colour = if ghost { self.accent() } else { sc.text };
        let stroke = Stroke::new(2.2, color);
        let is_connector = c.library_symbol.is_none() && c.kind == "connector";
        // Half extent of the body across the pin axis, in millimetres.
        let half_height = match c.kind.as_str() {
            "connector" => 2.5,
            "capacitor" => 2.0,
            "led" => 4.0,
            _ => 1.0,
        };
        if let Some(library) = &c.library_symbol {
            for g in library
                .definition
                .graphics
                .iter()
                .filter(|g| (g.unit == 0 || g.unit == library.unit) && g.body_style <= 1)
            {
                self.paint_graphic(
                    painter,
                    rect,
                    camera,
                    &g.primitive,
                    (c.position, c.rotation),
                    color,
                );
            }
            for pin in library.definition.pins.iter().filter(|p| {
                (p.unit == 0 && library.unit == 1 || p.unit == library.unit) && p.body_style <= 1
            }) {
                if pin.visible {
                    let end = pin.stem_end();
                    painter.line_segment([at(pin.position), at(end)], stroke);
                    for (center, radius) in pin.decoration_bubbles() {
                        painter.circle_stroke(
                            at(center),
                            (radius * f64::from(camera.zoom)) as f32,
                            stroke,
                        );
                    }
                    for path in pin.decoration_paths() {
                        if path.len() >= 3 {
                            painter.add(egui::Shape::closed_line(
                                path.iter().map(|p| at(*p)).collect(),
                                stroke,
                            ));
                        } else if path.len() == 2 {
                            painter.line_segment([at(path[0]), at(path[1])], stroke);
                        }
                    }
                }
                if pin.name != "~" && !pin.name.is_empty() {
                    let label_at = if pin.visible {
                        pin.stem_end()
                    } else {
                        pin.position
                    };
                    // The name reads inside the body: right-aligned and left of the
                    // stem end for a pin on the right, hung below it for a pin on top.
                    let stem = at(pin.stem_end()) - at(pin.position);
                    let (anchor, shift) = if stem.x.abs() >= stem.y.abs() {
                        if stem.x < 0.0 {
                            (Align2::RIGHT_BOTTOM, Vec2::new(-3.0, -3.0))
                        } else {
                            (Align2::LEFT_BOTTOM, Vec2::new(3.0, -3.0))
                        }
                    } else if stem.y > 0.0 {
                        (Align2::LEFT_TOP, Vec2::new(3.0, 3.0))
                    } else {
                        (Align2::LEFT_BOTTOM, Vec2::new(3.0, -3.0))
                    };
                    painter.text(
                        at(label_at) + shift,
                        anchor,
                        &pin.name,
                        FontId::monospace(10.0),
                        sc.muted,
                    );
                }
            }
        } else {
            match c.kind.as_str() {
                "capacitor" => {
                    for x in [-0.7, 0.7] {
                        painter.line_segment(
                            [at(Point::new(x, -2.0)), at(Point::new(x, 2.0))],
                            stroke,
                        );
                    }
                    for (a, b) in [(-5.0, -0.7), (0.7, 5.0)] {
                        painter
                            .line_segment([at(Point::new(a, 0.0)), at(Point::new(b, 0.0))], stroke);
                    }
                }
                "led" => {
                    painter.add(egui::Shape::closed_line(
                        vec![
                            at(Point::new(-2.0, -2.0)),
                            at(Point::new(-2.0, 2.0)),
                            at(Point::new(2.0, 0.0)),
                        ],
                        stroke,
                    ));
                    painter.line_segment(
                        [at(Point::new(2.0, -2.0)), at(Point::new(2.0, 2.0))],
                        stroke,
                    );
                    for (a, b) in [(-5.0, -2.0), (2.0, 5.0)] {
                        painter
                            .line_segment([at(Point::new(a, 0.0)), at(Point::new(b, 0.0))], stroke);
                    }
                    // Two emission arrows leaving the diode, each with a head.
                    let arrow = Stroke::new(1.6, color);
                    for x in [0.0, 1.8] {
                        let (tail, tip) = (Point::new(x, -2.4), Point::new(x + 1.4, -3.9));
                        painter.line_segment([at(tail), at(tip)], arrow);
                        for head in [
                            Point::new(tip.x - 0.9, tip.y),
                            Point::new(tip.x, tip.y + 0.9),
                        ] {
                            painter.line_segment([at(tip), at(head)], arrow);
                        }
                    }
                    for (x, name) in [(-3.6, "A"), (3.6, "K")] {
                        painter.text(
                            at(Point::new(x, 1.4)),
                            Align2::CENTER_TOP,
                            name,
                            FontId::monospace(10.0),
                            text_colour,
                        );
                    }
                }
                "connector" => {
                    painter.add(egui::Shape::closed_line(
                        vec![
                            at(Point::new(-3.5, -2.5)),
                            at(Point::new(3.5, -2.5)),
                            at(Point::new(3.5, 2.5)),
                            at(Point::new(-3.5, 2.5)),
                        ],
                        stroke,
                    ));
                    for (a, b) in [(-5.0, -3.5), (3.5, 5.0)] {
                        painter
                            .line_segment([at(Point::new(a, 0.0)), at(Point::new(b, 0.0))], stroke);
                    }
                    for edge in [-3.5, 3.5] {
                        painter.circle_filled(at(Point::new(edge, 0.0)), 4.0, color);
                    }
                }
                _ => {
                    painter.add(egui::Shape::closed_line(
                        vec![
                            at(Point::new(-3.0, -1.0)),
                            at(Point::new(3.0, -1.0)),
                            at(Point::new(3.0, 1.0)),
                            at(Point::new(-3.0, 1.0)),
                        ],
                        stroke,
                    ));
                    for (a, b) in [(-5.0, -3.0), (3.0, 5.0)] {
                        painter
                            .line_segment([at(Point::new(a, 0.0)), at(Point::new(b, 0.0))], stroke);
                    }
                }
            }
        }
        let center = camera.screen(c.position, rect);
        for pin in &c.pins {
            let position = at(pin.offset);
            painter.circle_filled(position, 3.5, sc.sheet);
            painter.circle_stroke(position, 3.5, Stroke::new(1.6, sc.pin));
            let toward = center - position;
            let spot = if toward.x.abs() >= toward.y.abs() {
                position + Vec2::new(toward.x.signum() * 7.0, -10.0)
            } else {
                position + Vec2::new(9.0, toward.y.signum() * 9.0)
            };
            painter.text(
                spot,
                Align2::CENTER_CENTER,
                &pin.number,
                FontId::monospace(11.0),
                sc.pin,
            );
            if is_connector {
                // A connector names each pin by the net it carries.
                let generic = pin.name.len() <= 1 || pin.name.starts_with("Pin");
                let name = match pin.net {
                    Some(net) if generic => self.project.net_name(net).to_string(),
                    _ => pin.name.clone(),
                };
                let inward = toward.x.signum();
                painter.text(
                    at(Point::new(pin.offset.x.signum() * 2.7, 0.0)),
                    if inward > 0.0 {
                        Align2::LEFT_CENTER
                    } else {
                        Align2::RIGHT_CENTER
                    },
                    name,
                    FontId::monospace(10.0),
                    text_colour,
                );
            }
        }
        let (sine, cosine) = c.rotation.to_radians().sin_cos();
        let label_offset = if c.library_symbol.is_some() {
            c.drawing_points()
                .iter()
                .map(|p| p.rotate(-c.rotation).y.abs())
                .fold(0.0, f64::max) as f32
                * camera.zoom
                + 14.0
        } else {
            (5.0 * sine.abs() + half_height * cosine.abs() + 1.5) as f32 * camera.zoom
        };
        painter.text(
            center - Vec2::new(0.0, label_offset),
            Align2::CENTER_BOTTOM,
            &c.reference,
            FontId::monospace(15.0),
            text_colour,
        );
        if !ghost {
            painter.text(
                center + Vec2::new(0.0, label_offset),
                Align2::CENTER_TOP,
                &c.value,
                FontId::monospace(12.0),
                text_colour,
            );
        }
        if selected && !ghost {
            let mut bounds = Rect::from_center_size(center, Vec2::ZERO);
            for pin in &c.pins {
                bounds.extend_with(at(pin.offset));
            }
            bounds.extend_with(center - Vec2::new(0.0, label_offset + 22.0));
            bounds.extend_with(center + Vec2::new(0.0, label_offset + 20.0));
            sheet::paint_selection(painter, bounds.expand(9.0), &sc);
        }
    }
    fn paint_board(&self, painter: &egui::Painter, rect: Rect, camera: Camera, p: &Project) {
        let shown = |name: &str| self.layer_shown(name);
        let art = self.art();
        self.paint_board_body(painter, rect, camera, p);
        underlay::paint(painter, rect, camera, p, "pcb");
        for fid in &p.board.fiducials {
            let center = camera.screen(fid.position, rect);
            let radius = ((fid.diameter / 2.0) as f32 * camera.zoom).max(1.5);
            let color = if fid.layer == Layer::Front {
                self.colours().canvas.front
            } else {
                self.colours().canvas.back
            };
            painter.circle_filled(center, radius, color);
            painter.circle_stroke(center, radius, Stroke::new(1.0, self.ink()));
        }
        for h in &p.board.holes {
            let radius = ((h.diameter / 2.0) as f32 * camera.zoom).max(1.5);
            if h.is_slot() {
                // A slot is a capsule: a line of the drill width between its
                // two centres, capped with half circles at each end.
                let a = camera.screen(h.start(), rect);
                let b = camera.screen(h.finish(), rect);
                let dir = (b - a).normalized();
                let side = egui::vec2(-dir.y, dir.x) * radius;
                let (a2, b2) = (a + dir * radius, b - dir * radius);
                painter.line_segment([a, b], Stroke::new(radius * 2.0, self.surface()));
                painter.circle_filled(a, radius, self.surface());
                painter.circle_filled(b, radius, self.surface());
                let ink = Stroke::new(1.0, self.ink());
                painter.line_segment([a2 + side, b2 + side], ink);
                painter.line_segment([a2 - side, b2 - side], ink);
                painter.circle_stroke(a, radius, ink);
                painter.circle_stroke(b, radius, ink);
            } else {
                let center = camera.screen(h.position, rect);
                painter.circle_filled(center, radius, art.bg);
                painter.circle_stroke(center, radius, Stroke::new(1.5, art.edge));
                self.paint_keepout_ring(painter, center, radius);
            }
        }
        self.paint_datums(painter, rect, camera, p);
        for c in p.board.cutouts.iter().filter(|_| shown("Cutouts")) {
            let center = camera.screen(c.center, rect);
            let radius = c.radius as f32 * camera.zoom;
            painter.circle_filled(center, radius, art.bg);
            painter.circle_stroke(center, radius, Stroke::new(1.5, art.edge));
            self.paint_keepout_ring(painter, center, radius);
        }
        self.paint_ratsnest(painter, rect, camera, p);
        // Paint far to near as the viewer sees the stack; layers nearer than the
        // focused one are see-through and those behind it are dimmed.
        let stack = &self.stack;
        let copper_count = p.copper_layer_count();
        for z in stack.far_to_near(
            p.board.zones.iter().filter(|_| shown("Copper zones")),
            |z| z.layer,
        ) {
            let screen: Vec<egui::Pos2> = z
                .outline
                .iter()
                .map(|pt| camera.screen(*pt, rect))
                .collect();
            let zone_colour = if z.layer == Layer::Front {
                self.colours().canvas.front
            } else {
                self.colours().canvas.back
            };
            let rgb = [zone_colour.r(), zone_colour.g(), zone_colour.b()];
            let far = stack.copper_alpha(z.layer);
            let fill = if self.selected == Some(z.id) {
                self.accent().linear_multiply(0.35)
            } else {
                Color32::from_rgb(rgb[0], rgb[1], rgb[2]).linear_multiply(0.07 * far)
            };
            if z.filled || self.selected == Some(z.id) {
                painter.add(egui::Shape::convex_polygon(
                    screen.clone(),
                    fill,
                    Stroke::NONE,
                ));
            }
            painter.add(egui::Shape::closed_line(
                screen.clone(),
                Stroke::new(
                    1.0,
                    Color32::from_rgb(rgb[0], rgb[1], rgb[2]).linear_multiply(far),
                ),
            ));
            for cut in &z.cutouts {
                let cut_screen: Vec<egui::Pos2> =
                    cut.iter().map(|pt| camera.screen(*pt, rect)).collect();
                painter.add(egui::Shape::convex_polygon(
                    cut_screen.clone(),
                    Color32::from_rgb(25, 58, 54),
                    Stroke::NONE,
                ));
                painter.add(egui::Shape::closed_line(
                    cut_screen,
                    Stroke::new(1.0, Color32::from_rgb(rgb[0], rgb[1], rgb[2])),
                ));
            }
        }
        for t in stack.far_to_near(
            p.board.tracks.iter().filter(|t| {
                shown(if t.layer == pcbcraft_doc::Layer::Front {
                    "Top copper"
                } else {
                    "Bottom copper"
                })
            }),
            |t| t.layer,
        ) {
            painter.line_segment(
                [camera.screen(t.start, rect), camera.screen(t.end, rect)],
                Stroke::new(
                    (t.width as f32 * camera.zoom).max(1.0),
                    if self.selected == Some(t.id) {
                        self.accent()
                    } else if t.layer == Layer::Front {
                        self.copper(Some(t.net), self.colours().canvas.front)
                            .linear_multiply(stack.copper_alpha(t.layer))
                    } else {
                        self.copper(Some(t.net), self.colours().canvas.back)
                            .linear_multiply(stack.copper_alpha(t.layer))
                    },
                ),
            );
        }
        for v in p.board.vias.iter().filter(|_| shown("Vias")) {
            let at = camera.screen(v.position, rect);
            let radius = (v.diameter as f32 / 2.0 * camera.zoom).max(2.0);
            let through = (0..copper_count)
                .filter_map(|i| Layer::from_index(i).ok())
                .map(|l| stack.copper_alpha(l))
                .fold(0.0f32, f32::max);
            let base = if self.selected == Some(v.id) {
                self.accent()
            } else {
                self.copper(Some(v.net), self.colours().canvas.front)
                    .linear_multiply(through)
            };
            if via_on_layer(p, v, self.route_layer) {
                painter.circle_filled(at, radius, base);
            } else {
                // The via has no copper on the active layer: show a faint ring
                // so it is still found, without looking like copper here.
                painter.circle_stroke(at, radius, Stroke::new(1.0, base.linear_multiply(0.35)));
            }
            if v.drill > 0.0 && via_on_layer(p, v, self.route_layer) {
                painter.circle_filled(
                    at,
                    (v.drill as f32 / 2.0 * camera.zoom).max(1.0),
                    Color32::from_rgb(19, 28, 41),
                );
            }
        }
        for t in &p.board.netties {
            let half = t.diameter / 2.0;
            let axis = pcbcraft_geom::Point::new(half, 0.0).rotate(t.rotation);
            for sign in [1.0, -1.0] {
                let centre = t
                    .position
                    .offset(pcbcraft_geom::Point::new(axis.x * sign, axis.y * sign));
                painter.circle_stroke(
                    camera.screen(centre, rect),
                    (half as f32 * camera.zoom).max(2.0),
                    Stroke::new(
                        1.5,
                        if self.selected == Some(t.id) {
                            self.accent()
                        } else {
                            self.colours().canvas.net_tie
                        },
                    ),
                );
            }
        }
        for f in stack.far_to_near(p.board.footprints.iter(), |f| {
            crate::stack_view::mount_copper(copper_count, f.board_side())
        }) {
            let on_bottom = f.board_side() != Layer::Front;
            let mount = crate::stack_view::mount_copper(copper_count, f.board_side());
            let pad_alpha = stack.copper_alpha(mount);
            let points = f
                .drawing_points(p)
                .into_iter()
                .map(|point| point.rotate(-f.rotation))
                .collect::<Vec<_>>();
            let min = points
                .iter()
                .fold(Point::new(f64::INFINITY, f64::INFINITY), |m, p| {
                    Point::new(m.x.min(p.x), m.y.min(p.y))
                });
            let max = points
                .iter()
                .fold(Point::new(f64::NEG_INFINITY, f64::NEG_INFINITY), |m, p| {
                    Point::new(m.x.max(p.x), m.y.max(p.y))
                });
            let label_x = (min.x + max.x) / 2.0;
            let reference_at = camera.screen(f.position.offset(Point::new(label_x, min.y)), rect)
                - Vec2::new(0.0, 12.0);
            let value_at = camera.screen(f.position.offset(Point::new(label_x, max.y)), rect)
                + Vec2::new(0.0, 12.0);
            let silk_fade = stack.silk_alpha(on_bottom);
            let color = if let Some(kind) = self.compare_tint.get(&f.id).copied() {
                self.compare_kind_color(kind)
            } else if let Some(kind) = self.compare_tint.get(&f.component).copied() {
                self.compare_kind_color(kind)
            } else {
                art.silk.linear_multiply(silk_fade)
            };
            let boxes = Self::footprint_boxes(f, rect, camera);
            if let Some(library) = p
                .component(f.component)
                .and_then(|c| c.library_footprint.as_ref())
            {
                for g in library
                    .definition
                    .graphics
                    .iter()
                    .filter(|_| shown("Package artwork"))
                {
                    if matches!(g,pcbcraft_library::Graphic::Text {text,..} if text=="REF**" || text=="%R" || text=="${REFERENCE}" || text=="${VALUE}" || text==&library.definition.name)
                    {
                        continue;
                    }
                    let flipped;
                    let g = if on_bottom {
                        flipped = board_side::mirrored_graphic(g);
                        &flipped
                    } else {
                        g
                    };
                    self.paint_graphic(painter, rect, camera, g, (f.position, f.rotation), color);
                }
            } else if let Some((silk, _)) = &boxes {
                painter.add(egui::Shape::closed_line(
                    silk.clone(),
                    Stroke::new(1.5, color),
                ));
            }
            if let Some((_, courtyard)) = &boxes {
                layout_art::dashed_closed(
                    painter,
                    courtyard,
                    Stroke::new(1.0, art.courtyard.gamma_multiply(0.6)),
                );
            }
            for pad in f.pads.iter().filter(|_| shown("Pads")) {
                {
                    let pos = f.pad_world_position(pad);
                    let center = camera.screen(pos, rect);
                    let mechanical = is_mechanical_pad(pad);
                    if !mechanical {
                        let outline: Vec<Pos2> = pad
                            .outline_points()
                            .into_iter()
                            .map(|p| camera.screen(f.position.offset(p.rotate(-f.rotation)), rect))
                            .collect();
                        painter.add(pad_shape(
                            outline,
                            pad.outline_is_convex(),
                            self.copper(
                                pad.net,
                                if on_bottom {
                                    self.colours().canvas.back
                                } else {
                                    art.top_copper
                                },
                            )
                            .linear_multiply(pad_alpha),
                        ));
                    }
                    let hole = pad.drill_outline_points();
                    if !hole.is_empty() {
                        painter.add(egui::Shape::convex_polygon(
                            hole.into_iter()
                                .map(|p| {
                                    camera.screen(f.position.offset(p.rotate(-f.rotation)), rect)
                                })
                                .collect(),
                            art.bg,
                            if mechanical {
                                Stroke::new(1.0, color)
                            } else {
                                Stroke::NONE
                            },
                        ));
                    }
                    if self.preferences.pad_labels != PadLabels::Off {
                        let label = match self.preferences.pad_labels {
                            PadLabels::Net => p.net_name(pad.net.unwrap_or_default()).to_string(),
                            _ => pad.number.clone(),
                        };
                        if !label.is_empty() {
                            if pad.geometry.as_ref().is_some_and(|g| g.drill.is_some()) {
                                // The drill hole fills the middle, so the
                                // number sits beside the pad.
                                let reach = pad
                                    .outline_points()
                                    .iter()
                                    .fold(0.0f64, |m, q| m.max(q.x.abs()))
                                    as f32
                                    * camera.zoom;
                                painter.text(
                                    center + Vec2::new(reach + 4.0, 0.0),
                                    Align2::LEFT_CENTER,
                                    label,
                                    FontId::monospace(10.0),
                                    art.silk,
                                );
                            } else {
                                painter.text(
                                    center,
                                    Align2::CENTER_CENTER,
                                    label,
                                    crate::fonts::Role::Mono.font(),
                                    art.on_copper,
                                );
                            }
                        }
                    }
                }
            }
            if let Some(c) = p.component(f.component) {
                let (above, below) = boxes.as_ref().map_or((reference_at, value_at), |(_, y)| {
                    let top = y.iter().fold(f32::INFINITY, |m, q| m.min(q.y));
                    let bottom = y.iter().fold(f32::NEG_INFINITY, |m, q| m.max(q.y));
                    let mid = y.iter().map(|q| q.x).sum::<f32>() / y.len().max(1) as f32;
                    (Pos2::new(mid, top - 5.0), Pos2::new(mid, bottom + 4.0))
                });
                painter.text(
                    above,
                    Align2::CENTER_BOTTOM,
                    &c.reference,
                    FontId::proportional(13.0),
                    art.text.linear_multiply(stack.silk_alpha(on_bottom)),
                );
                painter.text(
                    below,
                    Align2::CENTER_TOP,
                    &c.value,
                    FontId::proportional(10.5),
                    art.muted.linear_multiply(stack.silk_alpha(on_bottom)),
                );
            }
            if self.selected == Some(f.id)
                && let Some((_, courtyard)) = &boxes
            {
                self.paint_selection_box(painter, courtyard);
            }
        }
        for t in &p.board.texts {
            let at = camera.screen(t.position, rect);
            let color = if t.layer == pcbcraft_doc::Layer::Front {
                self.ink()
            } else {
                self.ink().gamma_multiply(0.75)
            };
            painter.text(
                at,
                Align2::LEFT_BOTTOM,
                &t.text,
                FontId::proportional((t.size as f32 * camera.zoom).max(1.0)),
                color,
            );
        }
        self.draw_user_layers(painter, rect, camera, p);
        self.paint_board_graphics(painter, rect, camera, p);
        self.paint_board_dimensions(painter, rect, camera, p);
        self.paint_axis_glyph(painter, rect, camera);
        self.paint_rulers(painter, rect, camera);
    }
}

/// Filled pad copper. Convex outlines use egui's convex fill; custom polygon
/// pads may be concave, so they are ear-clip triangulated into a mesh.
fn pad_shape(mut outline: Vec<Pos2>, convex: bool, color: Color32) -> egui::Shape {
    if !convex {
        if outline.len() > 1 && outline.first() == outline.last() {
            outline.pop();
        }
        let points: Vec<Point> = outline
            .iter()
            .map(|p| Point::new(p.x as f64, p.y as f64))
            .collect();
        let triangles = pcbcraft_geom::triangulate(&points);
        if !triangles.is_empty() {
            let mut mesh = egui::Mesh::default();
            for p in &outline {
                mesh.colored_vertex(*p, color);
            }
            for [a, b, c] in triangles {
                mesh.add_triangle(a as u32, b as u32, c as u32);
            }
            return egui::Shape::mesh(mesh);
        }
    }
    egui::Shape::convex_polygon(outline, color, Stroke::NONE)
}

/// Whether a via has copper on the layer being worked on. The "Bottom" choice
/// always means the last copper layer of the board.
pub(crate) fn via_on_layer(
    p: &pcbcraft_doc::Project,
    v: &pcbcraft_doc::Via,
    active: pcbcraft_doc::Layer,
) -> bool {
    let last = p.copper_layer_count().saturating_sub(1);
    let index = if active == Layer::Back {
        last
    } else {
        active.index()
    };
    let (a, b) = p.via_span(v);
    (a.index()..=b.index()).contains(&index)
}

#[cfg(test)]
mod library_render_tests {
    use super::*;
    #[test]
    fn bounds_filter_precedes_center_distance_for_large_symbol() -> Result<(), String> {
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = App::create(shared, Preferences::default(), None, None);
        let mut large =
            pcbcraft_doc::component_template("resistor", 1, "R1".into(), Point::default())
                .ok_or("missing template")?;
        large.pins[1].offset = Point::new(10.0, 0.0);
        let nearby =
            pcbcraft_doc::component_template("resistor", 2, "R2".into(), Point::new(9.0, 4.0))
                .ok_or("missing template")?;
        app.project.components = vec![large, nearby];
        app.tab = Tab::Schematic;
        assert_eq!(app.pick_object(Point::new(9.0, 0.0), 10.0), Some(1));
        Ok(())
    }
    #[test]
    fn library_text_angle_and_filled_graphic_reach_painter() {
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let app = App::create(shared, Preferences::default(), None, None);
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(egui::RawInput::default(), |root_ui| {
            egui::CentralPanel::default().show(root_ui, |ui| {
                let rect = Rect::from_min_max(Pos2::ZERO, Pos2::new(500.0, 500.0));
                let camera = Camera {
                    mirror: false,
                    turns: 0,
                    center: Point::default(),
                    zoom: 10.0,
                };
                let text = pcbcraft_library::Graphic::Text {
                    position: Point::default(),
                    angle: 30.0,
                    text: "rotated".into(),
                    size: Point::new(1.0, 1.0),
                    layer: String::new(),
                };
                app.paint_graphic(
                    ui.painter(),
                    rect,
                    camera,
                    &text,
                    (Point::default(), 45.0),
                    Color32::WHITE,
                );
                let body = pcbcraft_library::Graphic::Rectangle {
                    start: Point::new(-1.0, -1.0),
                    end: Point::new(1.0, 1.0),
                    width: 0.2,
                    fill: "solid".into(),
                    layer: String::new(),
                };
                app.paint_graphic(
                    ui.painter(),
                    rect,
                    camera,
                    &body,
                    (Point::default(), 0.0),
                    Color32::WHITE,
                );
            });
        });
        output.textures_delta.clear();
        assert!(output.shapes.iter().any(|s|matches!(&s.shape,egui::Shape::Text(t) if (t.angle-(-75.0_f32).to_radians()).abs()<1e-5)));
        assert!(
            output
                .shapes
                .iter()
                .any(|s| matches!(&s.shape,egui::Shape::Path(p) if p.fill==Color32::WHITE))
        );
    }
    #[test]
    fn pad_display_mode_switches_between_number_net_and_nothing() -> Result<(), String> {
        let mut project = pcbcraft_doc::Project::default();
        project.nets.push(pcbcraft_doc::Net {
            id: 3,
            name: "SPOT".into(),
            ..Default::default()
        });
        project.components.push(pcbcraft_doc::Component {
            sourcing: Default::default(),
            sheet: 0,
            id: 1,
            reference: "ZZ8".into(),
            value: "9".into(),
            kind: "resistor".into(),
            position: Point::new(5.0, 5.0),
            rotation: 0.0,
            pins: vec![],
            units: vec![],
            library_symbol: None,
            library_footprint: None,
            fields: vec![],
            sourcing: Default::default(),
        });
        project.board.footprints.push(pcbcraft_doc::Footprint {
            id: 2,
            component: 1,
            position: Point::new(5.0, 5.0),
            rotation: 0.0,
            locked: None,
            side: None,
            pads: vec![pcbcraft_doc::Pad {
                number: "7".into(),
                offset: Point::new(1.0, 0.0),
                diameter: 1.5,
                net: Some(3),
                castellated: None,
                geometry: None,
            }],
        });
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = App::create(shared, Preferences::default(), None, None);
        let labels = |app: &App| -> Vec<String> {
            let ctx = egui::Context::default();
            let mut output = ctx.run_ui(egui::RawInput::default(), |root_ui| {
                egui::CentralPanel::default().show(root_ui, |ui| {
                    app.paint_board(
                        ui.painter(),
                        Rect::from_min_max(Pos2::ZERO, Pos2::new(500.0, 500.0)),
                        Camera {
                            mirror: false,
                            turns: 0,
                            center: Point::default(),
                            zoom: 20.0,
                        },
                        &project,
                    );
                });
            });
            output.textures_delta.clear();
            output
                .shapes
                .iter()
                .filter_map(|s| match &s.shape {
                    egui::Shape::Text(t) => Some(t.galley.text().to_owned()),
                    _ => None,
                })
                .collect()
        };
        assert_eq!(
            app.preferences.pad_labels,
            PadLabels::Number,
            "pad numbers are the default display mode"
        );
        let numbers = labels(&app);
        assert!(
            numbers.iter().any(|t| t == "7"),
            "pad numbers show in number mode: {numbers:?}"
        );
        assert!(
            !numbers.iter().any(|t| t == "SPOT"),
            "nets stay hidden in number mode"
        );
        app.preferences.pad_labels = PadLabels::Net;
        let nets = labels(&app);
        assert!(
            nets.iter().any(|t| t == "SPOT"),
            "net mode names the net sitting on the pad: {nets:?}"
        );
        assert!(!nets.iter().any(|t| t == "7"), "pad numbers step aside");
        app.preferences.pad_labels = PadLabels::Off;
        let bare = labels(&app);
        assert!(!bare.iter().any(|t| t == "7"), "off hides pad labels");
        assert!(
            bare.iter().any(|t| t == "ZZ8"),
            "the reference label stays in every mode"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_view_frames_the_whole_board_instead_of_a_corner_of_it() -> Result<(), String> {
        let mut s = pcbcraft_engine::Session::default();
        let bounds = view_bounds(&s.project, true);
        let camera = fit_camera(bounds.0, bounds.1, (1200.0, 700.0));
        assert!(
            camera.zoom > 1.0,
            "a default board fills the panel, not 60 px of it"
        );
        assert_eq!(camera.center, Point::new(30.0, 20.0), "the board centres");
        s.execute(
            "board.set_outline",
            serde_json::json!({"points":[{"x":0.0,"y":0.0},{"x":300.0,"y":0.0},{"x":300.0,"y":200.0},{"x":0.0,"y":200.0}]}),
            None,
        )
        .map_err(|e| e.to_string())?;
        let (lo, hi) = view_bounds(&s.project, true);
        assert!(hi.x > 290.0, "the outline drives the frame");
        let camera = fit_camera(lo, hi, (1200.0, 700.0));
        assert!(camera.zoom < 6.0, "a big board zooms out to fit");
        assert!(camera.zoom >= 0.02, "zoom stays bounded");
        let mut empty = pcbcraft_engine::Session::default();
        let (lo, hi) = view_bounds(&empty.project, false);
        assert!(hi.x > lo.x, "an empty sheet still has a frame");
        empty
            .execute(
                "component.add",
                serde_json::json!({"kind":"resistor","position":{"x":120.0,"y":60.0}}),
                None,
            )
            .map_err(|e| e.to_string())?;
        let (_lo, hi) = view_bounds(&empty.project, false);
        assert!(hi.x > 110.0, "the schematic frame follows the drawn parts");
        Ok(())
    }

    #[test]
    fn the_net_spotlight_lights_one_net_and_fades_the_rest() {
        let copper = Color32::from_rgb(245, 141, 121);
        let hot = Color32::from_rgb(255, 200, 0);
        assert_eq!(
            spotlight(None, Some(4), copper, hot),
            copper,
            "with no spotlight every net keeps its own colour"
        );
        assert_eq!(
            spotlight(Some(4), Some(4), copper, hot),
            hot,
            "the lit net glows in the accent colour"
        );
        let faded = spotlight(Some(4), Some(9), copper, hot);
        assert_ne!(faded, copper);
        assert!(
            faded.gamma_multiply(4.0) != copper,
            "other copper is dimmed, not recoloured"
        );
        assert_eq!(
            spotlight(Some(4), None, copper, hot),
            copper.gamma_multiply(0.25),
            "copper without a net fades while a net is lit"
        );
    }
}

/// Shortest distance from a point to a straight segment, in millimetres.
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

/// True for a bare drilled hole inside a footprint: no copper, no net.
pub(crate) fn is_mechanical_pad(pad: &pcbcraft_doc::Pad) -> bool {
    pad.geometry
        .as_ref()
        .is_some_and(|g| g.pad_type == "unplated")
}
