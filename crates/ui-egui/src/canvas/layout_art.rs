//! Layout canvas drawing that follows the approved Layout mockup: board body,
//! rulers, dimension lines, origin glyph, grid dots, courtyards, ratsnest and
//! the selection box.
//!
//! Context: `App::paint_board` in `crates/ui-egui/src/canvas.rs` calls these
//! helpers; the schematic canvas never does.
//!
//! Local rules:
//! - Every colour of this drawing lives in `BoardArt`, per theme. The shared
//!   token set (`crates/ui-egui/src/palette.rs`) replaces the values when it
//!   carries them; no literal colour belongs anywhere else in this file.
//! - Pure painting: nothing here reads input or edits the project.
use super::*;
use egui::epaint::TextShape;
use std::f32::consts::FRAC_PI_2;

/// Colours of the board drawing for one theme, from the design system's
/// "Canvas layer colours" table.
///
/// # Rules
/// - Constant per theme; `BoardArt::new` is the only constructor.
///
/// # Dependencies
/// - `crates/ui-egui/src/canvas.rs` calls it; the colours follow the approved design system (owner research notes, outside this repository).
///
/// # Do not
/// - Use these for window chrome; chrome uses `ThemeColours`.
#[derive(Clone, Copy)]
pub(super) struct BoardArt {
    pub bg: Color32,
    pub ruler: Color32,
    pub grid_minor: Color32,
    pub grid_major: Color32,
    pub substrate: Color32,
    pub edge: Color32,
    pub top_copper: Color32,
    pub silk: Color32,
    pub courtyard: Color32,
    pub ratsnest: Color32,
    pub selection: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub on_copper: Color32,
}

impl BoardArt {
    /// The mockup values for dark or light.
    pub(super) fn new(dark: bool) -> Self {
        let c = Color32::from_rgb;
        if dark {
            Self {
                bg: c(0x07, 0x10, 0x0D),
                ruler: c(0x0A, 0x14, 0x11),
                grid_minor: c(0x1C, 0x32, 0x28),
                grid_major: c(0x2A, 0x47, 0x3A),
                substrate: c(0x0E, 0x3B, 0x2D),
                edge: c(0xE6, 0xBE, 0x5A),
                top_copper: c(0xE8, 0x83, 0x4A),
                silk: c(0xED, 0xEB, 0xDD),
                courtyard: c(0xB5, 0x8A, 0xE0),
                ratsnest: Color32::from_rgba_unmultiplied(0xE8, 0xF0, 0xEB, 0xB3),
                selection: c(0x5C, 0xC8, 0xE8),
                text: c(0xE8, 0xF0, 0xEB),
                muted: c(0x9D, 0xB3, 0xA8),
                on_copper: c(0x0A, 0x14, 0x11),
            }
        } else {
            Self {
                bg: c(0xF1, 0xF0, 0xE6),
                ruler: c(0xE9, 0xE8, 0xDF),
                grid_minor: c(0xD6, 0xD5, 0xC6),
                grid_major: c(0xBF, 0xBE, 0xAE),
                substrate: c(0xCF, 0xE3, 0xD6),
                edge: c(0x7A, 0x5A, 0x00),
                top_copper: c(0xC2, 0x57, 0x1A),
                silk: c(0x2A, 0x2A, 0x24),
                courtyard: c(0x8A, 0x52, 0xB8),
                ratsnest: c(0x3A, 0x4A, 0x42),
                selection: c(0x0B, 0x67, 0x85),
                text: c(0x10, 0x1C, 0x17),
                muted: c(0x44, 0x57, 0x4E),
                on_copper: c(0xFF, 0xFF, 0xFF),
            }
        }
    }
}

/// Side of the ruler strips in points.
const RULER: f32 = super::RULER_SIZE;

/// The smallest "nice" step (1, 2, 5 times a power of ten) that is at least
/// `min_px` wide at this zoom.
fn nice_step(zoom: f32, min_px: f32) -> f64 {
    let want = (min_px / zoom.max(0.0001)) as f64;
    let mut base = 10f64.powf(want.log10().floor());
    loop {
        for m in [1.0, 2.0, 5.0] {
            if base * m >= want {
                return base * m;
            }
        }
        base *= 10.0;
    }
}

/// Bounding box of the board outline (or plain rectangle) in millimetres.
fn board_bounds(p: &Project) -> (Point, Point) {
    match &p.board.outline {
        Some(points) if points.len() >= 3 => points.iter().fold(
            (
                Point::new(f64::INFINITY, f64::INFINITY),
                Point::new(f64::NEG_INFINITY, f64::NEG_INFINITY),
            ),
            |(lo, hi), q| {
                (
                    Point::new(lo.x.min(q.x), lo.y.min(q.y)),
                    Point::new(hi.x.max(q.x), hi.y.max(q.y)),
                )
            },
        ),
        _ => (Point::default(), Point::new(p.board.width, p.board.height)),
    }
}

/// Dashed closed polygon.
pub(super) fn dashed_closed(painter: &egui::Painter, pts: &[Pos2], stroke: Stroke) {
    let mut closed = pts.to_vec();
    if let Some(first) = pts.first() {
        closed.push(*first);
    }
    painter.extend(egui::Shape::dashed_line(&closed, stroke, 4.0, 3.0));
}

impl App {
    /// The theme's board art colours.
    pub(super) fn art(&self) -> BoardArt {
        BoardArt::new(self.colours().dark)
    }

    /// Subtle grid dots: minor at the working grid, major every tenth line.
    ///
    /// # Rules
    /// - Capped at 1600 by 1000 dots so a deep zoom-out stays cheap.
    pub(super) fn paint_layout_grid(&self, painter: &egui::Painter, rect: Rect, camera: Camera) {
        let art = self.art();
        let grid = if camera.zoom < 5.0 { 5.0 } else { 1.0 };
        let step = grid * camera.zoom;
        if step < 3.0 {
            return;
        }
        let zero = camera.screen(Point::default(), rect);
        let nx = ((rect.width() / step) as usize + 2).min(1600);
        let ny = ((rect.height() / step) as usize + 2).min(1000);
        let first_x = ((rect.left() - zero.x) / step).floor() as i64;
        let first_y = ((rect.top() - zero.y) / step).floor() as i64;
        let per_major = (10.0 / grid) as i64;
        for i in 0..nx as i64 {
            for j in 0..ny as i64 {
                let (gx, gy) = (first_x + i, first_y + j);
                let major = gx % per_major == 0 && gy % per_major == 0;
                painter.circle_filled(
                    Pos2::new(zero.x + gx as f32 * step, zero.y + gy as f32 * step),
                    if major { 1.1 } else { 0.7 },
                    if major {
                        art.grid_major
                    } else {
                        art.grid_minor
                    },
                );
            }
        }
    }

    /// Solder-mask body and the outline in the edge colour. Follows the traced
    /// outline, so chamfered or notched boards show their real shape.
    pub(super) fn paint_board_body(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        camera: Camera,
        p: &Project,
    ) {
        let art = self.art();
        let world: Vec<Point> = match &p.board.outline {
            Some(points) if points.len() >= 3 => points.clone(),
            _ => vec![
                Point::default(),
                Point::new(p.board.width, 0.0),
                Point::new(p.board.width, p.board.height),
                Point::new(0.0, p.board.height),
            ],
        };
        let screen: Vec<Pos2> = world.iter().map(|q| camera.screen(*q, rect)).collect();
        painter.add(pad_shape(screen.clone(), false, art.substrate));
        painter.add(egui::Shape::closed_line(screen, Stroke::new(1.5, art.edge)));
    }

    /// Rulers along the top and left edges in millimetres. They follow pan and
    /// zoom because ticks come from the camera, not from the board.
    pub(super) fn paint_rulers(&self, painter: &egui::Painter, rect: Rect, camera: Camera) {
        let art = self.art();
        let line = Stroke::new(1.0, art.grid_major);
        let top = Rect::from_min_max(rect.min, Pos2::new(rect.right(), rect.top() + RULER));
        let left = Rect::from_min_max(rect.min, Pos2::new(rect.left() + RULER, rect.bottom()));
        painter.rect_filled(top, 0.0, art.ruler);
        painter.rect_filled(left, 0.0, art.ruler);
        painter.line_segment([top.left_bottom(), top.right_bottom()], line);
        painter.line_segment([left.right_top(), left.right_bottom()], line);
        let step = nice_step(camera.zoom, 56.0);
        let font = crate::fonts::Role::Mono.font();
        let origin = camera.screen(Point::default(), rect);
        // Which world axis runs along each ruler, and which way, follows the turns.
        let (ux, uy) = camera.axes();
        let across = if ux.x != 0.0 { ux.x } else { uy.x };
        let down = if ux.y != 0.0 { ux.y } else { uy.y };
        let zoom = camera.zoom as f64;
        let ends = |a: f32, b: f32, o: f32, f: f32| {
            let (p, q) = (
                (a - o) as f64 / (f as f64 * zoom) / step,
                (b - o) as f64 / (f as f64 * zoom) / step,
            );
            (p.min(q).floor() as i64, p.max(q).ceil() as i64)
        };
        let (first, last) = ends(rect.left(), rect.right(), origin.x, across);
        for k in first..=last {
            let x = origin.x + across * (k as f64 * step) as f32 * camera.zoom;
            let half = x + across * step as f32 * camera.zoom / 2.0;
            if x >= rect.left() + RULER {
                painter.line_segment(
                    [Pos2::new(x, top.bottom() - 7.0), Pos2::new(x, top.bottom())],
                    line,
                );
                painter.text(
                    Pos2::new(x + 3.0, top.top() + 2.0),
                    Align2::LEFT_TOP,
                    format_tick(k as f64 * step),
                    font.clone(),
                    art.muted,
                );
            }
            if half >= rect.left() + RULER {
                painter.line_segment(
                    [
                        Pos2::new(half, top.bottom() - 4.0),
                        Pos2::new(half, top.bottom()),
                    ],
                    line,
                );
            }
        }
        let (first, last) = ends(rect.top(), rect.bottom(), origin.y, down);
        for k in first..=last {
            let y = origin.y + down * (k as f64 * step) as f32 * camera.zoom;
            let half = y + down * step as f32 * camera.zoom / 2.0;
            if y >= rect.top() + RULER {
                painter.line_segment(
                    [Pos2::new(left.right() - 7.0, y), Pos2::new(left.right(), y)],
                    line,
                );
                painter.text(
                    Pos2::new(left.left() + 2.0, y + 2.0),
                    Align2::LEFT_TOP,
                    format_tick(k as f64 * step),
                    font.clone(),
                    art.muted,
                );
            }
            if half >= rect.top() + RULER {
                painter.line_segment(
                    [
                        Pos2::new(left.right() - 4.0, half),
                        Pos2::new(left.right(), half),
                    ],
                    line,
                );
            }
        }
        painter.rect_filled(
            Rect::from_min_size(rect.min, Vec2::splat(RULER)),
            0.0,
            art.ruler,
        );
    }

    /// Width and height dimension lines with arrowheads and values, outside
    /// the board edge.
    pub(super) fn paint_board_dimensions(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        camera: Camera,
        p: &Project,
    ) {
        if !camera.turns.is_multiple_of(4) {
            // The dimension lines are laid out for an upright board.
            return;
        }
        let art = self.art();
        let (lo, hi) = board_bounds(p);
        // Two opposite corners on screen, whichever way the camera runs y.
        let (p, q) = (camera.screen(lo, rect), camera.screen(hi, rect));
        let a = Pos2::new(p.x.min(q.x), p.y.min(q.y));
        let b = Pos2::new(p.x.max(q.x), p.y.max(q.y));
        let stroke = Stroke::new(1.0, art.muted);
        let font = crate::fonts::Role::Mono.font();
        // Width: below the board.
        let y = b.y + 44.0;
        painter.line_segment([Pos2::new(a.x, y), Pos2::new(b.x, y)], stroke);
        for (x, dir) in [(a.x, 1.0), (b.x, -1.0)] {
            painter.line_segment([Pos2::new(x, y - 6.0), Pos2::new(x, y + 6.0)], stroke);
            painter.line_segment([Pos2::new(x, y), Pos2::new(x + 6.0 * dir, y - 3.0)], stroke);
            painter.line_segment([Pos2::new(x, y), Pos2::new(x + 6.0 * dir, y + 3.0)], stroke);
        }
        painter.text(
            Pos2::new((a.x + b.x) / 2.0, y - 5.0),
            Align2::CENTER_BOTTOM,
            format!("{:.2} mm", hi.x - lo.x),
            font.clone(),
            art.muted,
        );
        // Height: left of the board, text turned to read along the line.
        let x = a.x - 44.0;
        painter.line_segment([Pos2::new(x, a.y), Pos2::new(x, b.y)], stroke);
        for (yy, dir) in [(a.y, 1.0), (b.y, -1.0)] {
            painter.line_segment([Pos2::new(x - 6.0, yy), Pos2::new(x + 6.0, yy)], stroke);
            painter.line_segment(
                [Pos2::new(x, yy), Pos2::new(x - 3.0, yy + 6.0 * dir)],
                stroke,
            );
            painter.line_segment(
                [Pos2::new(x, yy), Pos2::new(x + 3.0, yy + 6.0 * dir)],
                stroke,
            );
        }
        let galley = painter.layout_no_wrap(format!("{:.2} mm", hi.y - lo.y), font, art.muted);
        let centre = Pos2::new(x - 8.0, (a.y + b.y) / 2.0);
        let size = galley.size();
        let at = centre + Vec2::new(-size.y / 2.0, size.x / 2.0);
        painter.add(TextShape::new(at, galley, art.muted).with_angle(-FRAC_PI_2));
    }

    /// Origin glyph at the lower left of the canvas. The axes are drawn the way
    /// the camera runs them, so it turns with the rotate buttons.
    pub(super) fn paint_axis_glyph(&self, painter: &egui::Painter, rect: Rect, camera: Camera) {
        let art = self.art();
        let stroke = Stroke::new(1.2, art.muted);
        let o = Pos2::new(rect.left() + RULER + 26.0, rect.bottom() - 40.0);
        // World +x and +y as the camera shows them, so the glyph turns with the view.
        let (ux, uy) = camera.axes();
        let font = crate::fonts::Role::Mono.font();
        for (unit, name) in [(ux, "X"), (uy, "Y")] {
            let end = o + unit * 34.0;
            let side = Vec2::new(-unit.y, unit.x);
            painter.line_segment([o, end], stroke);
            painter.line_segment([end, end - unit * 5.0 + side * 3.0], stroke);
            painter.line_segment([end, end - unit * 5.0 - side * 3.0], stroke);
            painter.text(
                end + unit * 8.0,
                Align2::CENTER_CENTER,
                name,
                font.clone(),
                art.muted,
            );
        }
    }

    /// Faint dashed keepout ring around a mounting hole or cutout.
    pub(super) fn paint_keepout_ring(&self, painter: &egui::Painter, centre: Pos2, radius: f32) {
        let art = self.art();
        let r = radius * 2.0;
        let n = 48;
        let pts: Vec<Pos2> = (0..n)
            .map(|i| {
                let t = i as f32 / n as f32 * std::f32::consts::TAU;
                centre + Vec2::new(t.cos(), t.sin()) * r
            })
            .collect();
        dashed_closed(
            painter,
            &pts,
            Stroke::new(1.0, art.courtyard.gamma_multiply(0.55)),
        );
    }

    /// Ratsnest: thin dashed lines with the net name at the midpoint.
    pub(super) fn paint_ratsnest(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        camera: Camera,
        p: &Project,
    ) {
        let art = self.art();
        let Some(wires) = self.checks.get("airwires").and_then(Value::as_array) else {
            return;
        };
        for wire in wires {
            let point = |k: &str| {
                wire.get(k)
                    .and_then(|v| serde_json::from_value::<Point>(v.clone()).ok())
            };
            let (Some(a), Some(b)) = (point("from"), point("to")) else {
                continue;
            };
            let (a, b) = (camera.screen(a, rect), camera.screen(b, rect));
            painter.extend(egui::Shape::dashed_line(
                &[a, b],
                Stroke::new(1.0, art.ratsnest),
                5.0,
                4.0,
            ));
            if let Some(net) = wire.get("net").and_then(Value::as_u64) {
                let name = p.net_name(net);
                if !name.is_empty() {
                    painter.text(
                        a.lerp(b, 0.5) + Vec2::new(0.0, -7.0),
                        Align2::CENTER_BOTTOM,
                        name,
                        crate::fonts::Role::Mono.font(),
                        art.ratsnest,
                    );
                }
            }
        }
    }

    /// Courtyard and silkscreen boxes of a footprint, as screen polygons.
    ///
    /// Why: the sample parts carry no library artwork, so both boxes are
    /// derived from the pads: silk hugs them, the courtyard keeps clear.
    pub(super) fn footprint_boxes(
        f: &pcbcraft_doc::Footprint,
        rect: Rect,
        camera: Camera,
    ) -> Option<(Vec<Pos2>, Vec<Pos2>)> {
        let pts: Vec<Point> = f.pads.iter().flat_map(|p| p.outline_points()).collect();
        let first = pts.first()?;
        let (mut lo, mut hi) = (*first, *first);
        for q in &pts {
            lo = Point::new(lo.x.min(q.x), lo.y.min(q.y));
            hi = Point::new(hi.x.max(q.x), hi.y.max(q.y));
        }
        let boxed = |m: f64| -> Vec<Pos2> {
            [
                Point::new(lo.x - m, lo.y - m),
                Point::new(hi.x + m, lo.y - m),
                Point::new(hi.x + m, hi.y + m),
                Point::new(lo.x - m, hi.y + m),
            ]
            .into_iter()
            .map(|q| camera.screen(f.position.offset(q.rotate(-f.rotation)), rect))
            .collect()
        };
        Some((boxed(0.3), boxed(0.9)))
    }

    /// Selection box with corner handles and the rotate handle above it.
    pub(super) fn paint_selection_box(&self, painter: &egui::Painter, courtyard: &[Pos2]) {
        let art = self.art();
        let Some(first) = courtyard.first() else {
            return;
        };
        let mut bounds = Rect::from_min_max(*first, *first);
        for q in courtyard {
            bounds.extend_with(*q);
        }
        let bounds = bounds.expand(3.0);
        let stroke = Stroke::new(1.5, art.selection);
        painter.rect_stroke(bounds, 0.0, stroke, egui::StrokeKind::Middle);
        for corner in [
            bounds.left_top(),
            bounds.right_top(),
            bounds.right_bottom(),
            bounds.left_bottom(),
        ] {
            let h = Rect::from_center_size(corner, Vec2::splat(7.0));
            painter.rect_filled(h, 0.0, art.bg);
            painter.rect_stroke(h, 0.0, stroke, egui::StrokeKind::Middle);
        }
        let stem = Pos2::new(bounds.center().x, bounds.top() - 16.0);
        painter.line_segment([Pos2::new(bounds.center().x, bounds.top()), stem], stroke);
        painter.circle_filled(stem, 4.0, art.bg);
        painter.circle_stroke(stem, 4.0, stroke);
    }
}

/// Ruler label: whole millimetres without a decimal point.
fn format_tick(v: f64) -> String {
    if (v - v.round()).abs() < 1e-9 {
        format!("{}", v.round() as i64)
    } else {
        format!("{v}")
    }
}
