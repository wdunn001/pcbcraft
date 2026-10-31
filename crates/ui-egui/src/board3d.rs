//! The 3D View tab: the board mesh from `pcbcraft_io::board_mesh`, drawn by the
//! depth-buffered software renderer in `board3d/raster.rs` into one image that
//! egui paints. It needs no GPU renderer of its own, so it runs in headless
//! tests and in the browser build. Camera state lives here, never in the
//! project document.
//!
//! Local rules: board coordinates run y down, like the 2D view, and z is up, so
//! the world is mirrored once on the way in ([`flip`]); that makes the top view
//! read the same as the Layout canvas. Picking and `fit` stay in board
//! coordinates.

use super::*;
use egui::{Pos2, Sense};
#[cfg(test)]
use pcbcraft_io::PartKind;
use pcbcraft_io::{BoardMesh, MeshLayers, ModelStatus};

mod model_cache;
mod raster;
use model_cache::ModelCache;

type V = [f64; 3];

const FOV_TAN: f64 = 0.3057; // tan(17 degrees): a 34 degree vertical view
const NEAR: f64 = 0.01;
/// The default angle: isometric from the front-left, a little above the board.
const FIT_YAW: f64 = -3.0 * std::f64::consts::FRAC_PI_4;
const FIT_PITCH: f64 = 0.62;

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
/// Board coordinates to the right-handed view world: y runs the other way.
fn flip(p: V) -> V {
    [p[0], -p[1], p[2]]
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
    /// Whether the list of parts without a model file is open.
    pub(crate) notes_open: bool,
    /// Where the viewport was painted last frame, so tests and picking agree.
    pub rect: Rect,
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
    /// Keep the camera when the mesh is rebuilt after an edit, instead of fitting it again.
    pub keep_camera: bool,
    /// The last rendered picture and what it was made from.
    frame: Option<Held>,
    /// An angle a capture asked for; applied once the first fit has run.
    pub(crate) start_angle: Option<(f64, f64)>,
}

/// A rendered picture with its key; `TextureHandle` has no `Debug`.
#[derive(Clone)]
struct Held(FrameKey, egui::TextureHandle);
impl std::fmt::Debug for Held {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Held").field(&self.0).finish()
    }
}

/// Everything the picture depends on; equal keys mean the image can be reused.
#[derive(Clone, Debug, PartialEq)]
struct FrameKey {
    revision: u64,
    model_gen: u64,
    camera: [f64; 6],
    size: [usize; 2],
    ss: usize,
    selected: Option<Id>,
    gold: bool,
    mask: [u8; 3],
    silk: [u8; 3],
}
}
impl Default for View3d {
    fn default() -> Self {
        Self {
            yaw: FIT_YAW,
            pitch: FIT_PITCH,
            dist: 100.0,
            target: [0.0; 3],
            mesh: None,
            models: ModelCache::default(),
            model_gen: 0,
            notes: Default::default(),
            error: None,
            notes_open: false,
            rect: Rect::NOTHING,
keep_camera: false,
            frame: None,
            start_angle: None,
        }
    }
}

pub(crate) struct Basis {
    eye: V,
    fwd: V,
    right: V,
    up: V,
}

/// Fixed camera for benchmarks and tests outside the tab.
pub struct BenchBasis(Basis);

impl BenchBasis {
    pub fn isometric(target: V, dist: f64) -> Self {
        let view = View3d {
            yaw: FIT_YAW,
            pitch: FIT_PITCH,
            dist,
            target,
            ..View3d::default()
        };
        Self(view.basis())
    }
    pub(crate) fn inner(&self) -> &Basis {
        &self.0
    }
}

/// Headless render entry for performance tests.
pub fn bench_render(
    mesh: &BoardMesh,
    basis: &BenchBasis,
    w: usize,
    h: usize,
) -> egui::ColorImage {
    raster::render(
        mesh,
        basis.inner(),
        w,
        h,
        &raster::Look {
            gold: false,
            selected: None,
            ss: 2,
        },
    )
}

impl View3d {
    fn basis(&self) -> Basis {
        let (cp, sp) = (self.pitch.cos(), self.pitch.sin());
        let dir = [cp * self.yaw.cos(), cp * self.yaw.sin(), sp];
        let at = flip(self.target);
        let eye = [
            at[0] + dir[0] * self.dist,
            at[1] + dir[1] * self.dist,
            at[2] + dir[2] * self.dist,
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
    #[cfg(test)]
    fn project_with(&self, b: &Basis, rect: Rect, p: V) -> Option<(Pos2, f64)> {
        let rel = sub(flip(p), b.eye);
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
        (flip(b.eye), flip(unit(d)))
    }
    /// The nearest part with an owner under `pos`: its component and distance.
    pub(crate) fn pick(&self, rect: Rect, pos: Pos2) -> Option<(Id, f64)> {
        let (mesh, _) = self.mesh.as_ref().map(|(_, m)| (m, ()))?;
        let (o, d) = self.ray(rect, pos);
        let mut best: Option<(Id, f64)> = None;
        for part in &mesh.parts {
            let Some(id) = part.component else { continue };
            if let (Some(proto), Some(m)) = (&part.prototype, &part.instance) {
                for tri in proto.indices.as_chunks::<3>().0.iter() {
                    let (Some(a), Some(b), Some(c)) = (
                        proto.positions.get(tri[0] as usize),
                        proto.positions.get(tri[1] as usize),
                        proto.positions.get(tri[2] as usize),
                    ) else {
                        continue;
                    };
                    let a = pcbcraft_io::apply_matrix(m, *a);
                    let b = pcbcraft_io::apply_matrix(m, *b);
                    let c = pcbcraft_io::apply_matrix(m, *c);
                    if let Some(t) = ray_triangle(o, d, a, b, c)
                        && best.is_none_or(|(_, bt)| t < bt)
                    {
                        best = Some((id, t));
                    }
                }
                continue;
            }
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
    pub(crate) fn build_error(&self) -> Option<String> {
        self.error.clone()
    }
    /// Record a part whose model file was not found, as a load would.
    #[cfg(test)]
    pub(crate) fn note_failed(&mut self, id: Id, why: &str) {
        self.notes.insert(id, ModelStatus::Failed(why.into()));
    }
    /// How the model file of this part loaded, when it names one.
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
            .map(|p| {
                p.prototype
                    .as_ref()
                    .map(|proto| proto.indices.len() / 3)
                    .unwrap_or_else(|| p.triangle_count())
            })
    }
    /// The box around this part's body in board millimetres.
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
            self.dist = diag * 1.45;
        }
        self.yaw = FIT_YAW;
        self.pitch = FIT_PITCH;
    }
    /// Rebuild the mesh when the design changed or a model file finished
    /// loading. Model files are only asked for here, never waited on.
    fn refresh(&mut self, project: &Project, revision: u64) {
        let generation = self.models.generation();
        let same_design = self.mesh.as_ref().is_some_and(|(r, _)| *r == revision);
        if same_design && self.model_gen == generation {
            return;
        }
        let had_mesh = self.mesh.is_some();
        match pcbcraft_io::board_mesh(project, MeshLayers::default()) {
            Ok(mut mesh) => {
                self.error = None;
                self.model_gen = generation;
                let models = self.models.clone();
                self.notes = pcbcraft_io::with_models(&mut mesh, |p| models.lookup(p))
                    .into_iter()
                    .collect();
                self.mesh = Some((revision, mesh));
                if !same_design && !(self.keep_camera && had_mesh) {
                    self.fit();
                    if let Some((yaw, pitch)) = self.start_angle.take() {
                        self.yaw = yaw;
                        self.pitch = pitch;
                    }
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
        let moved = [
            (-b.right[0] * dx + b.up[0] * dy) * k,
            (-b.right[1] * dx + b.up[1] * dy) * k,
            (-b.right[2] * dx + b.up[2] * dy) * k,
        ];
        let moved = flip(moved);
        for (t, m) in self.target.iter_mut().zip(moved) {
            *t += m;
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

/// What the tab needs from the app, so this module owns no app state.
pub(crate) struct Env<'a> {
    pub project: &'a Project,
    pub revision: u64,
    pub selected: Option<Id>,
    pub bg: Color32,
    pub muted: Color32,
}

/// A colour the project names in its notes (`mask_colour`, `silk_colour`): a
/// common colour word or a hex code; anything else keeps the default.
fn project_colour(project: &Project, key: &str, default: [u8; 3]) -> [u8; 3] {
    let Some(text) = project.meta.get(key) else {
        return default;
    };
    let word = text.trim().to_ascii_lowercase();
    let hex = word.trim_start_matches('#');
    if hex.len() == 6
        && let Ok(v) = u32::from_str_radix(hex, 16)
    {
        return [(v >> 16) as u8, (v >> 8) as u8, v as u8];
    }
    match word.as_str() {
        "green" => default,
        "red" => [150, 24, 28],
        "blue" => [22, 58, 132],
        "black" | "matte black" => [26, 27, 30],
        "white" => [226, 228, 224],
        "yellow" => [214, 176, 34],
        "purple" => [74, 34, 110],
        _ => default,
    }
}

/// Largest side of the rendered picture in pixels; 2048 is the smallest texture limit in use.
const MAX_PICTURE_SIDE: usize = 2048;

/// Render the mesh for the current camera, reusing the last picture while
/// nothing it depends on has changed.
///
/// Why: the software renderer is far cheaper than a frame, but a still board
/// should cost nothing; while the camera moves it draws at one sample per
/// pixel, and once it rests it redraws at two by two.
///
/// # Rules
/// - Returns none until a mesh exists or while the viewport is too small.
/// - Never asks for a texture side above `MAX_PICTURE_SIDE`.
fn picture(
    view: &mut View3d,
    ctx: &egui::Context,
    rect: Rect,
    env: &Env<'_>,
    moving: bool,
) -> Option<egui::TextureId> {
    let revision = view.mesh.as_ref().map(|(r, _)| *r)?;
    let ppp = ctx.pixels_per_point().clamp(1.0, 3.0);
    // The picture is stretched over the viewport, so a tall or wide panel never needs a texture
    // larger than the smallest side every graphics device accepts.
    let size = [
        ((rect.width() * ppp).round() as usize).min(MAX_PICTURE_SIDE),
        ((rect.height() * ppp).round() as usize).min(MAX_PICTURE_SIDE),
    ];
    if size[0] < 8 || size[1] < 8 {
        return None;
    }
    let gold = {
        let f = env.project.board.finish.to_ascii_uppercase();
        f.contains("ENIG") || f.contains("GOLD")
    };
    let mask = project_colour(env.project, "mask_colour", raster::MASK_SRGB);
    let silk = project_colour(env.project, "silk_colour", raster::SILK_SRGB);
    let key = FrameKey {
        revision,
        model_gen: view.model_gen,
        camera: [
            view.yaw,
            view.pitch,
            view.dist,
            view.target[0],
            view.target[1],
            view.target[2],
        ],
        size,
        ss: if moving { 1 } else { 2 },
        selected: env.selected,
        gold,
        mask,
        silk,
    };
    if let Some(Held(held, tex)) = &view.frame
        && *held == key
    {
        return Some(tex.id());
    }
    let (_, mesh) = view.mesh.as_ref()?;
    let image = raster::render(
        mesh,
        &view.basis(),
        size[0],
        size[1],
        &raster::Look {
            gold,
            selected: env.selected,
            ss: key.ss,
            mask,
            silk,
        },
    );
    let tex = match view.frame.take() {
        Some(Held(_, mut tex)) => {
            tex.set(image, egui::TextureOptions::LINEAR);
            tex
        }
        None => ctx.load_texture("board3d", image, egui::TextureOptions::LINEAR),
    };
    let id = tex.id();
    view.frame = Some(Held(key, tex));
    Some(id)
}

/// Draw the tab. Returns a component the person picked, by click in the
/// viewport or by choosing a row in the parts list.
pub(crate) fn show(view: &mut View3d, ui: &mut egui::Ui, env: &Env<'_>) -> Option<Id> {
    view.refresh(env.project, env.revision);
    let mut picked = None;
    // Parts are listed in the Layers tree; here only a model that did not load
    // needs saying, with the reason, so a kept box is never a mystery.
    let mut kept: Vec<String> = view
        .notes
        .iter()
        .filter_map(|(id, status)| match status {
            ModelStatus::Failed(why) => {
                let name = env
                    .project
                    .component(*id)
                    .map_or_else(|| format!("#{id}"), |c| c.reference.clone());
                Some(format!("Box kept. {name}: {why}"))
            }
            _ => None,
        })
        .collect();
    kept.sort();
    let loading = view.models.pending() > 0;
    if loading {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(100));
    }

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
    // A few notes read inline; many collapse into the footer line, with the
    // full list in a scrollable window on demand, so they never push the
    // board out of view.
    if kept.len() <= 3 {
        for note in &kept {
            ui.add(
                egui::Label::new(
                    egui::RichText::new(note)
                        .font(crate::fonts::Role::Caption.font())
                        .color(env.muted),
                )
                .wrap(),
            );
        }
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
    let moving = response.dragged() || (response.hovered() && scroll != 0.0);
    if moving {
        ui.ctx().request_repaint();
    }
    if let Some(texture) = picture(view, ui.ctx(), rect, env, moving) {
        painter.image(
            texture,
            rect,
            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            Color32::WHITE,
        );
    }
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(if loading {
                t!("board.board3d.loading_models_note")
            } else {
                t!("board.board3d.part_bodies_are_default_boxes")
            })
            .font(crate::fonts::Role::Caption.font())
            .color(env.muted),
        );
        if kept.len() > 3 {
            ui.label(
                egui::RichText::new(format!(
                    "· {}",
                    t!("board.board3d.models_missing", n = kept.len())
                ))
                .font(crate::fonts::Role::Caption.font())
                .color(env.muted),
            );
            let label = if view.notes_open {
                t!("board.board3d.models_hide")
            } else {
                t!("board.board3d.models_show")
            };
            if ui
                .small_button(label)
                .on_hover_text(t!("board.board3d.models_show.tip"))
                .clicked()
            {
                view.notes_open = !view.notes_open;
            }
        }
    });
    if kept.len() > 3 && view.notes_open {
        let mut open = true;
        egui::Window::new(t!("board.board3d.models_missing", n = kept.len()))
            .id(egui::Id::new("board3d_missing_models"))
            .open(&mut open)
            .anchor(egui::Align2::RIGHT_BOTTOM, [-12.0, -40.0])
            .default_width(360.0)
            .show(ui.ctx(), |ui| {
                egui::ScrollArea::vertical()
                    .max_height(200.0)
                    .show(ui, |ui| {
                        for note in &kept {
                            ui.label(
                                egui::RichText::new(note)
                                    .font(crate::fonts::Role::Caption.font())
                                    .color(env.muted),
                            );
                        }
                    });
            });
        if !open {
            view.notes_open = false;
        }
    }
    picked
}

/// Draw only the 3D viewport, sized `size`: the same mesh, camera and painter as the 3D
/// tab, without its parts list. The Library tab uses it to show one part on a small board.
///
/// # Rules
/// - Drag turns, shift or right drag pans, the wheel zooms, as in the tab.
/// - Model files load on the shared worker; the viewport never waits for them.
///
/// # Dependencies
/// - crates/ui-egui/src/board3d.rs (`show` shares `refresh`, `paint_mesh` and the camera with it)
///
/// # Do not
/// - Pick or select here; a one-part view has nothing to pick.
pub(crate) fn viewport(view: &mut View3d, ui: &mut egui::Ui, env: &Env<'_>, size: egui::Vec2) {
    view.refresh(env.project, env.revision);
    if view.models.pending() > 0 {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(100));
    }
    let (rect, response) = ui.allocate_exact_size(size, Sense::click_and_drag());
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
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, env.bg);
    paint_mesh(view, &painter, rect, env);
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

    /// Prints how long a still frame and a moving frame take on the showcase.
    /// Run on demand: `cargo test --release -p pcbcraft-ui-egui frame_time -- --ignored --nocapture`.
    #[test]
    #[ignore = "timing report, run on demand in release mode"]
    fn frame_time_report_on_the_showcase() -> Result<(), String> {
        let text = include_str!("../../../examples/showcase.pcbcraft");
        let project: Project = serde_json::from_str(text).map_err(|e| e.to_string())?;
        let mesh = pcbcraft_io::board_mesh(&project, MeshLayers::default())?;
        let mut view = View3d {
            mesh: Some((1, mesh)),
            ..View3d::default()
        };
        view.fit();
        let basis = view.basis();
        let (_, mesh) = view.mesh.as_ref().ok_or("no mesh")?;
        for (name, ss, w, h) in [("still", 2, 940, 780), ("moving", 1, 940, 780)] {
            let look = raster::Look {
                gold: false,
                selected: None,
                ss,
                mask: raster::MASK_SRGB,
                silk: raster::SILK_SRGB,
            };
            let mut best = f64::MAX;
            for _ in 0..12 {
                let t = std::time::Instant::now();
                std::hint::black_box(raster::render(mesh, &basis, w, h, &look));
                best = best.min(t.elapsed().as_secs_f64() * 1000.0);
            }
            println!("frame {name}: {best:.1} ms (best of 12)");
        }
        Ok(())
    }
}
