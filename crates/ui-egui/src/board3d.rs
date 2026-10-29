//! The 3D View tab: the board mesh from `pcbcraft_io::board_mesh`, drawn by
//! projecting triangles on the CPU into an `egui::Mesh`. It needs no GPU
//! renderer, so it runs in headless tests and in the browser build. Camera
//! state lives here, never in the project document.

use super::*;
use egui::{Pos2, Sense};
use pcbcraft_io::{BoardMesh, MeshLayers, ModelStatus, PartKind};

mod model_cache;
use model_cache::ModelCache;

type V = [f64; 3];

/// Above this many triangles only the board and the part bodies are drawn,
/// so a huge board stays responsive.
const DRAW_BUDGET: usize = 60_000;
const FOV_TAN: f64 = 0.364; // tan(20 degrees): a 40 degree vertical view
const NEAR: f64 = 0.01;

fn sub(a: V, b: V) -> V {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: V, b: V) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V, b: V) -> V {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn unit(a: V) -> V {
    let n = dot(a, a).sqrt();
    if n < 1e-12 {
        [0.0, 0.0, 1.0]
    } else {
        [a[0] / n, a[1] / n, a[2] / n]
    }
}

/// The camera orbits `target` at `dist`, looking at it from `yaw`/`pitch`.
#[derive(Clone, Debug)]
pub(crate) struct View3d {
    pub yaw: f64,
    pub pitch: f64,
    pub dist: f64,
    pub target: V,
    mesh: Option<(u64, BoardMesh)>,
    /// Model files read off the UI thread; see `board3d/model_cache.rs`.
    models: ModelCache,
    /// How many finished loads the current mesh already includes.
    model_gen: u64,
    /// Per part: did its model file load, or why did the box stay.
    notes: std::collections::BTreeMap<Id, ModelStatus>,
    error: Option<String>,
    /// Where the viewport was painted last frame, so tests and picking agree.
    pub rect: Rect,
}
impl Default for View3d {
    fn default() -> Self {
        Self {
            yaw: -std::f64::consts::FRAC_PI_2,
            pitch: 0.6,
            dist: 100.0,
            target: [0.0; 3],
            mesh: None,
            models: ModelCache::default(),
            model_gen: 0,
            notes: Default::default(),
            error: None,
            rect: Rect::NOTHING,
        }
    }
}

struct Basis {
    eye: V,
    fwd: V,
    right: V,
    up: V,
}

impl View3d {
    fn basis(&self) -> Basis {
        let (cp, sp) = (self.pitch.cos(), self.pitch.sin());
        let dir = [cp * self.yaw.cos(), cp * self.yaw.sin(), sp];
        let eye = [
            self.target[0] + dir[0] * self.dist,
            self.target[1] + dir[1] * self.dist,
            self.target[2] + dir[2] * self.dist,
        ];
        let fwd = unit([-dir[0], -dir[1], -dir[2]]);
        let right = unit(cross(fwd, [0.0, 0.0, 1.0]));
        let up = cross(right, fwd);
        Basis {
            eye,
            fwd,
            right,
            up,
        }
    }
    fn scale(rect: Rect) -> f64 {
        f64::from(rect.height()) / 2.0 / FOV_TAN
    }
    /// Screen position and view depth of a board-space point.
    #[cfg(test)]
    pub(crate) fn project_point(&self, rect: Rect, p: V) -> Option<(Pos2, f64)> {
        self.project_with(&self.basis(), rect, p)
    }
    fn project_with(&self, b: &Basis, rect: Rect, p: V) -> Option<(Pos2, f64)> {
        let rel = sub(p, b.eye);
        let z = dot(rel, b.fwd);
        if z <= NEAR {
            return None;
        }
        let s = Self::scale(rect);
        let c = rect.center();
        Some((
            Pos2::new(
                c.x + (dot(rel, b.right) / z * s) as f32,
                c.y - (dot(rel, b.up) / z * s) as f32,
            ),
            z,
        ))
    }
    /// The ray under a screen position: origin and unit direction.
    pub(crate) fn ray(&self, rect: Rect, pos: Pos2) -> (V, V) {
        let b = self.basis();
        let s = Self::scale(rect);
        let c = rect.center();
        let x = f64::from(pos.x - c.x) / s;
        let y = -f64::from(pos.y - c.y) / s;
        let d = [
            b.fwd[0] + b.right[0] * x + b.up[0] * y,
            b.fwd[1] + b.right[1] * x + b.up[1] * y,
            b.fwd[2] + b.right[2] * x + b.up[2] * y,
        ];
        (b.eye, unit(d))
    }
    /// The nearest part with an owner under `pos`: its component and distance.
    pub(crate) fn pick(&self, rect: Rect, pos: Pos2) -> Option<(Id, f64)> {
        let (mesh, _) = self.mesh.as_ref().map(|(_, m)| (m, ()))?;
        let (o, d) = self.ray(rect, pos);
        let mut best: Option<(Id, f64)> = None;
        for part in &mesh.parts {
            let Some(id) = part.component else { continue };
            for tri in part.indices.as_chunks::<3>().0.iter() {
                let (Some(a), Some(b), Some(c)) = (
                    part.positions.get(tri[0] as usize),
                    part.positions.get(tri[1] as usize),
                    part.positions.get(tri[2] as usize),
                ) else {
                    continue;
                };
                if let Some(t) = ray_triangle(o, d, *a, *b, *c)
                    && best.is_none_or(|(_, bt)| t < bt)
                {
                    best = Some((id, t));
                }
            }
        }
        best
    }
    /// Why the mesh could not be built, if it could not.
    #[cfg(test)]
    pub(crate) fn build_error(&self) -> Option<String> {
        self.error.clone()
    }
    /// How the model file of this part loaded, when it names one.
    #[cfg(test)]
    pub(crate) fn model_status(&self, id: Id) -> Option<ModelStatus> {
        self.notes.get(&id).cloned()
    }
    /// Triangles in this part's body.
    #[cfg(test)]
    pub(crate) fn body_triangles(&self, id: Id) -> Option<usize> {
        let (_, mesh) = self.mesh.as_ref()?;
        mesh.parts
            .iter()
            .find(|p| p.kind == PartKind::Body && p.component == Some(id))
            .map(|p| p.triangle_count())
    }
    #[cfg(test)]
    pub(crate) fn body_bounds(&self, id: Id) -> Option<(V, V)> {
        let (_, mesh) = self.mesh.as_ref()?;
        mesh.parts
            .iter()
            .find(|p| p.kind == PartKind::Body && p.component == Some(id))
            .and_then(|p| p.bounds())
    }
    /// The family word the parts list shows after the reference.
    #[cfg(test)]
    pub(crate) fn body_family(&self, id: Id) -> Option<String> {
        let (_, mesh) = self.mesh.as_ref()?;
        mesh.parts
            .iter()
            .find(|p| p.kind == PartKind::Body && p.component == Some(id))
            .map(|p| p.family.clone().unwrap_or_else(|| "box".into()))
    }
    pub(crate) fn fit(&mut self) {
        if let Some((lo, hi)) = self.mesh.as_ref().and_then(|(_, m)| m.bounds()) {
            self.target = [
                (lo[0] + hi[0]) / 2.0,
                (lo[1] + hi[1]) / 2.0,
                (lo[2] + hi[2]) / 2.0,
            ];
            let diag = dot(sub(hi, lo), sub(hi, lo)).sqrt().max(1.0);
            self.dist = diag * 1.3;
        }
        self.yaw = -std::f64::consts::FRAC_PI_2;
        self.pitch = 0.6;
    }
    /// Rebuild the mesh when the design changed or a model file finished
    /// loading. Model files are only asked for here, never waited on.
    fn refresh(&mut self, project: &Project, revision: u64) {
        let generation = self.models.generation();
        let same_design = self.mesh.as_ref().is_some_and(|(r, _)| *r == revision);
        if same_design && self.model_gen == generation {
            return;
        }
        match pcbcraft_io::board_mesh(project, MeshLayers::default()) {
            Ok(mut mesh) => {
                self.error = None;
                self.model_gen = generation;
                let models = self.models.clone();
                self.notes = pcbcraft_io::with_models(&mut mesh, |p| models.lookup(p))
                    .into_iter()
                    .collect();
                self.mesh = Some((revision, mesh));
                if !same_design {
                    self.fit();
                }
            }
            Err(e) => {
                self.mesh = None;
                self.error = Some(e);
            }
        }
    }
    /// Orbit by a pointer drag in screen points.
    pub(crate) fn orbit(&mut self, dx: f64, dy: f64) {
        self.yaw -= dx * 0.01;
        self.pitch = (self.pitch + dy * 0.01).clamp(-1.5, 1.5);
    }
    fn pan(&mut self, dx: f64, dy: f64, rect: Rect) {
        let b = self.basis();
        let k = self.dist * FOV_TAN * 2.0 / f64::from(rect.height()).max(1.0);
        for i in 0..3 {
            self.target[i] += (-b.right[i] * dx + b.up[i] * dy) * k;
        }
    }
    fn zoom(&mut self, scroll: f64) {
        self.dist = (self.dist * (-scroll * 0.002).exp()).clamp(1.0, 5000.0);
    }
}

/// Moller-Trumbore, both faces. Returns the distance along `d`.
fn ray_triangle(o: V, d: V, a: V, b: V, c: V) -> Option<f64> {
    let e1 = sub(b, a);
    let e2 = sub(c, a);
    let p = cross(d, e2);
    let det = dot(e1, p);
    if det.abs() < 1e-12 {
        return None;
    }
    let inv = 1.0 / det;
    let t0 = sub(o, a);
    let u = dot(t0, p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = cross(t0, e1);
    let v = dot(d, q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = dot(e2, q) * inv;
    (t > NEAR).then_some(t)
}

fn kind_color(kind: PartKind, dark: bool) -> [f32; 3] {
    match kind {
        PartKind::Board => {
            if dark {
                [24.0, 92.0, 62.0]
            } else {
                [40.0, 130.0, 84.0]
            }
        }
        PartKind::CopperTop | PartKind::CopperBottom => [212.0, 160.0, 74.0],
        PartKind::MaskTop | PartKind::MaskBottom => [30.0, 110.0, 70.0],
        PartKind::SilkTop | PartKind::SilkBottom => [240.0, 240.0, 232.0],
        PartKind::Body => [70.0, 74.0, 84.0],
    }
}

/// What the tab needs from the app, so this module owns no app state.
pub(crate) struct Env<'a> {
    pub project: &'a Project,
    pub revision: u64,
    pub selected: Option<Id>,
    pub dark: bool,
    pub bg: Color32,
    pub muted: Color32,
}

fn paint_mesh(view: &View3d, painter: &egui::Painter, rect: Rect, env: &Env<'_>) {
    let Some((_, mesh)) = view.mesh.as_ref() else {
        return;
    };
    let basis = view.basis();
    let over = mesh.triangle_count() > DRAW_BUDGET;
    // Painter's order: the board first, then everything else far to near.
    for pass in 0..2 {
        let mut tris: Vec<(f64, [Pos2; 3], Color32)> = vec![];
        for part in &mesh.parts {
            let is_board = part.kind == PartKind::Board;
            if (pass == 0) != is_board || (over && !is_board && part.kind != PartKind::Body) {
                continue;
            }
            let base = match (part.kind, part.color) {
                (PartKind::Body, Some(c)) => [c[0] * 255.0, c[1] * 255.0, c[2] * 255.0],
                _ => kind_color(part.kind, env.dark),
            };
            let picked = part.component.is_some() && part.component == env.selected;
            for tri in part.indices.as_chunks::<3>().0.iter() {
                let (Some(a), Some(b), Some(c)) = (
                    part.positions.get(tri[0] as usize),
                    part.positions.get(tri[1] as usize),
                    part.positions.get(tri[2] as usize),
                ) else {
                    continue;
                };
                let n = unit(cross(sub(*b, *a), sub(*c, *a)));
                let to_eye = unit(sub(basis.eye, *a));
                let facing = dot(n, to_eye);
                if facing <= 0.0 {
                    continue;
                }
                let (Some((pa, za)), Some((pb, zb)), Some((pc, zc))) = (
                    view.project_with(&basis, rect, *a),
                    view.project_with(&basis, rect, *b),
                    view.project_with(&basis, rect, *c),
                ) else {
                    continue;
                };
                let light = (0.35 + 0.65 * facing) as f32;
                let mut rgb = base;
                if picked {
                    rgb = [255.0, 190.0, 40.0];
                }
                let color = Color32::from_rgb(
                    (rgb[0] * light).min(255.0) as u8,
                    (rgb[1] * light).min(255.0) as u8,
                    (rgb[2] * light).min(255.0) as u8,
                );
                tris.push(((za + zb + zc) / 3.0, [pa, pb, pc], color));
            }
        }
        tris.sort_by(|x, y| y.0.total_cmp(&x.0));
        for chunk in tris.chunks(20_000) {
            let mut m = egui::Mesh::default();
            for (_, pts, color) in chunk {
                let base = m.vertices.len() as u32;
                for p in pts {
                    m.vertices.push(egui::epaint::Vertex {
                        pos: *p,
                        uv: egui::epaint::WHITE_UV,
                        color: *color,
                    });
                }
                m.indices.extend([base, base + 1, base + 2]);
            }
            painter.add(egui::Shape::mesh(m));
        }
    }
}

/// Draw the tab. Returns a component the person picked, by click in the
/// viewport or by choosing a row in the parts list.
pub(crate) fn show(view: &mut View3d, ui: &mut egui::Ui, env: &Env<'_>) -> Option<Id> {
    view.refresh(env.project, env.revision);
    let mut picked = None;
    let mut bodies: Vec<(String, Id, String, Option<ModelStatus>)> = view
        .mesh
        .as_ref()
        .map(|(_, m)| {
            m.parts
                .iter()
                .filter(|p| p.kind == PartKind::Body)
                .filter_map(|p| {
                    let id = p.component?;
                    let name = env.project.component(id)?.reference.clone();
                    let note = p.family.clone().unwrap_or_else(|| "box".into());
                    Some((name, id, note, view.notes.get(&id).cloned()))
                })
                .collect()
        })
        .unwrap_or_default();
    bodies.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    let loading = view.models.pending() > 0;
    if loading {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(100));
    }

    egui::Panel::right("parts3d")
        .default_size(190.0)
        .min_size(150.0)
        .show(ui, |ui| {
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new(t!("board.board3d.parts"))
                    .font(crate::fonts::Role::Caption.font())
                    .color(env.muted),
            );
            egui::ScrollArea::vertical().show(ui, |ui| {
                for (name, id, note, status) in &bodies {
                    let row = ui
                        .selectable_label(
                            env.selected == Some(*id),
                            t!("board.board3d.name_note", name = name, note = note),
                        )
                        .on_hover_text(t!("board.board3d.name_note.tip"));
                    if row.clicked() {
                        picked = Some(*id);
                    }
                    let reason = match status {
                        Some(ModelStatus::Failed(why)) => Some(format!("Box kept. {why}")),
                        Some(ModelStatus::Loading) => Some("Loading model".to_string()),
                        _ => None,
                    };
                    if let Some(reason) = reason {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(&reason)
                                    .font(crate::fonts::Role::Caption.font())
                                    .color(env.muted),
                            )
                            .wrap(),
                        )
                        .on_hover_text(reason.clone());
                    }
                }
                if bodies.is_empty() {
                    ui.label(t!("board.board3d.no_parts_on_this_board"));
                }
            });
        });

    ui.add_space(6.0);
    ui.horizontal_wrapped(|ui| {
        ui.add_space(12.0);
        ui.label(
            egui::RichText::new(t!("board.board3d.drag_to_turn_the_board"))
                .font(crate::fonts::Role::Caption.font())
                .color(env.muted),
        );
    });
    if let Some(e) = &view.error {
        ui.label(t!("board.board3d.the_3d_view_could_not", e = e));
    }
    let size = ui.available_size_before_wrap();
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(size.x.max(120.0), (size.y - 28.0).max(120.0)),
        Sense::click_and_drag(),
    );
    view.rect = rect;
    let response = response.on_hover_text(t!("board.board3d.3d_board_drag_to_turn.tip"));
    let (shift, secondary, scroll) = ui.input(|i| {
        (
            crate::tool_modifiers::Held::new(i.modifiers, crate::tool_modifiers::Platform::host())
                .shift,
            i.pointer.button_down(egui::PointerButton::Secondary)
                || i.pointer.button_down(egui::PointerButton::Middle),
            i.smooth_scroll_delta.y,
        )
    });
    if response.dragged() {
        let d = response.drag_delta();
        if shift || secondary {
            view.pan(f64::from(d.x), f64::from(d.y), rect);
        } else {
            view.orbit(f64::from(d.x), f64::from(d.y));
        }
    }
    if response.hovered() && scroll != 0.0 {
        view.zoom(f64::from(scroll));
    }
    if response.clicked()
        && let Some(pos) = response.interact_pointer_pos()
        && let Some((id, _)) = view.pick(rect, pos)
    {
        picked = Some(id);
    }
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, env.bg);
    paint_mesh(view, &painter, rect, env);
    ui.label(
        egui::RichText::new(if loading {
            t!("board.board3d.loading_models_note")
        } else {
            t!("board.board3d.part_bodies_are_default_boxes")
        })
        .font(crate::fonts::Role::Caption.font())
        .color(env.muted),
    );
    picked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_target_projects_to_the_centre_and_a_ray_returns_to_it() {
        let view = View3d {
            target: [10.0, 20.0, 0.0],
            dist: 50.0,
            ..View3d::default()
        };
        let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0));
        let (pos, depth) = view
            .project_point(rect, view.target)
            .unwrap_or((Pos2::ZERO, 0.0));
        assert!((pos.x - 400.0).abs() < 0.01 && (pos.y - 300.0).abs() < 0.01);
        assert!((depth - 50.0).abs() < 1e-9);
        let (o, d) = view.ray(rect, pos);
        let t = depth;
        let hit = [o[0] + d[0] * t, o[1] + d[1] * t, o[2] + d[2] * t];
        assert!(sub(hit, view.target).iter().all(|v| v.abs() < 1e-6));
    }
}
