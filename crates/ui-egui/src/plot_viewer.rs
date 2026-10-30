//! Read-only fabrication plot viewer (folder load, layer overlay, measure).

use crate::a11y::Named;
use egui::{Color32, Pos2, Rect, Stroke, Vec2};
use pcbcraft_geom::Point;
use pcbcraft_io::{PlotPackage, distance_mm, layer_bounds, load_plot_folder};

#[derive(Clone, Copy)]
struct Camera {
    center: Point,
    zoom: f32,
}

impl Camera {
    fn screen(self, p: Point, rect: Rect) -> Pos2 {
        rect.center()
            + Vec2::new(
                (p.x - self.center.x) as f32 * self.zoom,
                (p.y - self.center.y) as f32 * self.zoom,
            )
    }

    fn world(self, p: Pos2, rect: Rect) -> Point {
        Point::new(
            self.center.x + (p.x - rect.center().x) as f64 / self.zoom as f64,
            self.center.y + (p.y - rect.center().y) as f64 / self.zoom as f64,
        )
    }
}

pub struct State {
    pub folder_path: String,
    package: Option<PlotPackage>,
    hidden_layers: Vec<String>,
    measure_mode: bool,
    measure_pts: Vec<Point>,
    camera: Camera,
    needs_fit: bool,
    hint_banner: Option<String>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            folder_path: String::new(),
            package: None,
            hidden_layers: Vec::new(),
            measure_mode: false,
            measure_pts: Vec::new(),
            camera: Camera {
                center: Point::new(30.0, 20.0),
                zoom: 8.0,
            },
            needs_fit: true,
            hint_banner: None,
        }
    }
}

impl State {
    pub fn layer_visible(&self, id: &str) -> bool {
        !self.hidden_layers.iter().any(|h| h == id)
    }

    #[cfg(test)]
    pub fn visible_layer_count(&self) -> usize {
        self.package
            .as_ref()
            .map(|p| {
                p.layers
                    .iter()
                    .filter(|l| self.layer_visible(&l.id))
                    .count()
            })
            .unwrap_or(0)
    }
}

pub struct Theme {
    pub surface: Color32,
    pub muted: Color32,
    pub accent: Color32,
}

pub fn ui(
    state: &mut State,
    ui: &mut egui::Ui,
    theme: Theme,
    message: &mut String,
    make_dir: &str,
    browser: bool,
) {
    ui.ctx().input(|i| {
        if i.key_pressed(egui::Key::Escape) && state.measure_mode {
            state.measure_mode = false;
            state.measure_pts.clear();
        }
    });

    if browser {
        ui.label(
            egui::RichText::new(crate::browser::desktop_only_panel())
                .small()
                .color(theme.muted),
        );
        ui.add_space(8.0);
    }

    if !browser {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("Plot folder")
                .small()
                .color(theme.muted),
        );
        let path_response = ui
            .add(
                crate::targets::text_field(&mut state.folder_path)
                    .desired_width(220.0)
                    .hint_text("Path to copper and drill files"),
            )
            .named("Path to copper and drill files")
            .on_hover_text(
                "Type the folder where copper and drill files were written, then load them here.",
            );
        if path_response.changed() && state.folder_path.trim().is_empty() && !make_dir.is_empty() {
            state.folder_path = make_dir.to_owned();
        }
        if ui
            .button("Load")
            .on_hover_text("Read every plot and drill file in this folder.")
            .clicked()
        {
            let path = std::path::Path::new(state.folder_path.trim());
            match load_plot_folder(path) {
                Ok(pkg) => {
                    state.hidden_layers.clear();
                    state.measure_pts.clear();
                    state.needs_fit = true;
                    state.hint_banner =
                        pkg.hints.first().map(|h| h.message.clone()).or_else(|| {
                            pkg.layers
                                .iter()
                                .find_map(|l| l.hints.first().map(|h| h.message.clone()))
                        });
                    *message = format!(
                        "Loaded {} layer file(s) from {}",
                        pkg.layers.len(),
                        state.folder_path.trim()
                    );
                    state.package = Some(pkg);
                }
                Err(e) => {
                    state.package = None;
                    *message = format!("Could not load plot folder: {e}");
                }
            }
        }
        if ui
            .button("Clear")
            .on_hover_text("Unload the current plot set and reset the viewer.")
            .clicked()
        {
            state.package = None;
            state.hidden_layers.clear();
            state.measure_pts.clear();
            state.hint_banner = None;
            *message = "Plot viewer cleared".into();
        }
    });
    }

    if let Some(hint) = &state.hint_banner {
        ui.label(egui::RichText::new(hint).small().color(theme.muted));
    }

    ui.horizontal(|ui| {
        let measure_label = if state.measure_mode {
            "Measure (on)"
        } else {
            "Measure"
        };
        if ui
            .selectable_label(state.measure_mode, measure_label)
            .on_hover_text(
                "Click two points on the canvas to read the distance in millimetres. Esc cancels.",
            )
            .clicked()
        {
            state.measure_mode = !state.measure_mode;
            state.measure_pts.clear();
        }
        if ui
            .button("Fit view")
            .on_hover_text("Frame every visible layer in the canvas.")
            .clicked()
        {
            state.needs_fit = true;
        }
        ui.label(
            egui::RichText::new("middle-drag: pan · scroll: zoom · Esc exits measure")
                .small()
                .color(theme.muted),
        );
    });

    if state.measure_pts.len() == 2 {
        let d = distance_mm(state.measure_pts[0], state.measure_pts[1]);
        ui.label(format!("Distance: {d:.3} mm"));
    }

    let Some(package) = state.package.clone() else {
        ui.centered_and_justified(|ui| {
            ui.label(
                egui::RichText::new("Load a folder of plot files to inspect layers here.")
                    .color(theme.muted),
            );
        });
        return;
    };

    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.set_min_width(180.0);
            ui.label(egui::RichText::new("Layers").small().color(theme.muted))
                .on_hover_text("Toggle which files are drawn on the canvas.");
            for layer in &package.layers {
                let visible = state.layer_visible(&layer.id);
                let label = if visible {
                    layer.id.clone()
                } else {
                    format!("{} (hidden)", layer.id)
                };
                let mut show = visible;
                if ui
                    .checkbox(&mut show, &label)
                    .on_hover_text("Show or hide this layer in the overlay.")
                    .changed()
                {
                    if show {
                        state.hidden_layers.retain(|h| h != &layer.id);
                    } else if !state.hidden_layers.iter().any(|h| h == &layer.id) {
                        state.hidden_layers.push(layer.id.clone());
                    }
                }
            }
        });
        ui.separator();
        let canvas = ui.available_rect_before_wrap();
        paint_canvas(ui, state, &package, canvas, &theme);
        ui.separator();
        ui.vertical(|ui| {
            ui.set_min_width(200.0);
            ui.label(egui::RichText::new("Apertures").small().color(theme.muted))
                .on_hover_text("Tooling definitions from the loaded gerber files.");
            egui::ScrollArea::vertical()
                .max_height(200.0)
                .show(ui, |ui| {
                    for layer in &package.layers {
                        if !state.layer_visible(&layer.id) || layer.apertures.is_empty() {
                            continue;
                        }
                        ui.label(egui::RichText::new(&layer.id).weak());
                        for ap in &layer.apertures {
                            let func = ap.function.as_deref().unwrap_or("—");
                            ui.label(format!(
                                "D{} · {} · {:.3}×{:.3} mm · {func}",
                                ap.code, ap.template, ap.width_mm, ap.height_mm
                            ));
                        }
                    }
                });
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new("Drill tools")
                    .small()
                    .color(theme.muted),
            )
            .on_hover_text("Excellon tool table: diameter and hit counts.");
            for tool in &package.drill_tools {
                ui.label(format!(
                    "T{} · {:.3} mm · {} hit(s)",
                    tool.number, tool.diameter_mm, tool.hit_count
                ));
            }
        });
    });
}

fn paint_canvas(
    ui: &mut egui::Ui,
    state: &mut State,
    package: &PlotPackage,
    rect: Rect,
    theme: &Theme,
) {
    let response = ui.allocate_rect(rect, egui::Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, theme.surface);

    if state.needs_fit {
        if let Some((min_x, min_y, max_x, max_y)) = layer_bounds(package) {
            let w = (max_x - min_x).max(1.0);
            let h = (max_y - min_y).max(1.0);
            state.camera.center = Point::new((min_x + max_x) / 2.0, (min_y + max_y) / 2.0);
            let zx = (rect.width() * 0.85) as f64 / w;
            let zy = (rect.height() * 0.85) as f64 / h;
            state.camera.zoom = zx.min(zy) as f32;
        }
        state.needs_fit = false;
    }

    if response.dragged_by(egui::PointerButton::Middle) {
        let delta = response.drag_delta();
        state.camera.center = Point::new(
            state.camera.center.x - delta.x as f64 / state.camera.zoom as f64,
            state.camera.center.y - delta.y as f64 / state.camera.zoom as f64,
        );
    }
    if response.hovered() {
        let scroll = ui.ctx().input(|i| i.smooth_scroll_delta.y);
        if scroll != 0.0 {
            let factor = (1.0 + scroll * 0.002).clamp(0.2, 5.0);
            state.camera.zoom = (state.camera.zoom * factor).clamp(0.5, 120.0);
        }
    }

    for layer in &package.layers {
        if !state.layer_visible(&layer.id) {
            continue;
        }
        let color = Color32::from_rgb(layer.color[0], layer.color[1], layer.color[2]);
        let stroke_w = (0.08 * state.camera.zoom).clamp(0.5, 3.0);
        for stroke in &layer.strokes {
            if stroke.points.len() < 2 {
                continue;
            }
            let pts: Vec<Pos2> = stroke
                .points
                .iter()
                .map(|(x, y)| state.camera.screen(Point::new(*x, *y), rect))
                .collect();
            for pair in pts.windows(2) {
                painter.line_segment([pair[0], pair[1]], Stroke::new(stroke_w, color));
            }
        }
        for region in &layer.regions {
            if region.len() < 2 {
                continue;
            }
            let pts: Vec<Pos2> = region
                .iter()
                .map(|(x, y)| state.camera.screen(Point::new(*x, *y), rect))
                .collect();
            for pair in pts.windows(2) {
                painter.line_segment(
                    [pair[0], pair[1]],
                    Stroke::new(stroke_w, color.gamma_multiply(0.7)),
                );
            }
        }
        for flash in &layer.flashes {
            let center = state
                .camera
                .screen(Point::new(flash.x_mm, flash.y_mm), rect);
            let r = (0.15 * state.camera.zoom).clamp(2.0, 12.0);
            if layer.id == "Drill hits" {
                painter.circle_stroke(center, r, Stroke::new(1.5, color));
            } else {
                painter.circle_filled(center, r, color);
            }
        }
    }

    if state.measure_mode
        && response.clicked()
        && let Some(pos) = response.interact_pointer_pos()
    {
        let world = state.camera.world(pos, rect);
        state.measure_pts.push(world);
        if state.measure_pts.len() > 2 {
            state.measure_pts.remove(0);
        }
    }
    if !state.measure_pts.is_empty() {
        for pt in &state.measure_pts {
            let sp = state.camera.screen(*pt, rect);
            painter.circle_filled(sp, 4.0, theme.accent);
        }
        if state.measure_pts.len() == 2 {
            let a = state.camera.screen(state.measure_pts[0], rect);
            let b = state.camera.screen(state.measure_pts[1], rect);
            painter.line_segment([a, b], Stroke::new(1.5, theme.accent));
        }
    }

    ui.allocate_rect(rect, egui::Sense::hover());
}
