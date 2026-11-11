//! The 3D model page: the part on a small board, its model file, and how the model is aligned
//! to the footprint.
//!
//! Context: issues #698, #702 (pad overlay) and #706 (transform editor). The viewport is the
//! board renderer (`board3d::viewport`) over a one-part project built from the footprint, so
//! the picture is what the board will show. The model's offset, rotation and scale are edited
//! through `library.footprint.set_model`, one undo step each. A top view with the pads drawn
//! over the model's outline shows whether the two line up. The file card also imports a
//! SolidCraft part as the component 3D model, and Reimport re-reads the file while keeping
//! the offset, rotation and scale already set.
//!
//! Local rules: a footprint that is in this project's library can be edited here. A footprint
//! from a read-only library shows read-only, but the project can override that with a toggle
//! to allow editing model transforms for library footprints. A part with no model file shows
//! the generated body. Nothing is read from the network. Replacing a file keeps the current
//! transform, and a Reimport never changes the transform.
//!
//! # Dependencies
//! - crates/ui-egui/src/board3d.rs (`viewport`, `View3d`)
//! - crates/engine/src/authoring.rs (`library.footprint.set_model`, `library.footprint.clear_model`)
//! - crates/io/src/board_mesh.rs (`model_matrix`, the placement the renderer applies)
//!
//! # Do not
//! - Write a model transform into a library file; only the project library is edited.
//! - Re-align the model on reimport; alignment is a separate tool.
use super::*;
use crate::ui_kit::button::ghost_button;
use crate::ui_kit::card::card;
use crate::ui_kit::number_field::{NumberSpec, number_field};
use pcbcraft_doc::{Component, Footprint, LibraryFootprint, Pad, Project};
use pcbcraft_geom::Point;
use pcbcraft_library::{FootprintDefinition, Model3D};

/// Id of the part and of its footprint in the one-part project.
const PART: u64 = 1;
const PLACED: u64 = 2;

/// Nearest millimetre fraction at which a model counts as centred on its pads.
const ALIGNED_WITHIN_MM: f64 = 0.1;
// Warn in plain words when the residual after auto-align is this large.
const WARN_ABOVE_MM: f64 = 0.15;

/// A project holding only `def`, placed at the middle of a board a little larger than it.
///
/// Why: the 3D renderer draws a project, so a one-part project is the least that shows a
/// part the way a board would.
///
/// # Rules
/// - The footprint sits at the board centre; the returned point is that centre.
///
/// # Dependencies
/// - crates/doc/src/lib.rs (`Project`, `Component`, `Footprint`, `Pad`)
pub(crate) fn part_project(def: &FootprintDefinition) -> (Project, Point) {
    let mut p = Project::default();
    let (mut lo, mut hi) = (Point::new(-2.0, -2.0), Point::new(2.0, 2.0));
    for pad in &def.pads {
        lo = Point::new(
            lo.x.min(pad.position.x - pad.size.x / 2.0),
            lo.y.min(pad.position.y - pad.size.y / 2.0),
        );
        hi = Point::new(
            hi.x.max(pad.position.x + pad.size.x / 2.0),
            hi.y.max(pad.position.y + pad.size.y / 2.0),
        );
    }
    let reach = lo.x.abs().max(hi.x.abs()).max(lo.y.abs()).max(hi.y.abs()) + 4.0;
    p.board.width = reach * 2.0;
    p.board.height = reach * 2.0;
    let centre = Point::new(reach, reach);
    p.components.push(Component {
        id: PART,
        reference: "U1".into(),
        value: String::new(),
        // Validation wants a known kind when there is no library symbol.
        kind: "resistor".into(),
        position: Point::new(0.0, 0.0),
        rotation: 0.0,
        // The board check wants one pin per pad number.
        pins: def
            .pads
            .iter()
            .map(|g| pcbcraft_doc::Pin {
                number: g.number.clone(),
                name: String::new(),
                offset: Point::new(0.0, 0.0),
                net: None,
                electrical_type: "passive".into(),
                direction: None,
                unit: 1,
                appearances: Default::default(),
            })
            .collect(),
        units: Vec::new(),
        library_symbol: None,
        library_footprint: Some(LibraryFootprint {
            library: "library".into(),
            definition: def.clone(),
        }),
        fields: Vec::new(),
        sourcing: Default::default(),
        sheet: 0,
        compliance: Default::default(),
        include_in_bom: true,
        include_in_placement: true,
        instance: Default::default(),
        part: None,
        prototype: Default::default(),
    });
    p.board.footprints.push(Footprint {
        id: PLACED,
        component: PART,
        position: centre,
        rotation: 0.0,
        locked: None,
        side: None,
        pads: def
            .pads
            .iter()
            .map(|g| Pad {
                number: g.number.clone(),
                offset: g.position,
                diameter: g.size.x.min(g.size.y),
                net: None,
                castellated: None,
                geometry: Some(g.clone()),
            })
            .collect(),
        preserved: Default::default(),
    });
    (p, centre)
}

/// How far the model's body is from the middle of the pads, in millimetres, as `(dx, dy)`.
///
/// # Rules
/// - `body` is the box around the model in board coordinates and `centre` the footprint origin.
/// - Pads with no area are ignored; no pads gives `None`.
pub(crate) fn centre_offset(
    def: &FootprintDefinition,
    centre: Point,
    body: ([f64; 3], [f64; 3]),
) -> Option<(f64, f64)> {
    if def.pads.is_empty() {
        return None;
    }
    let n = def.pads.len() as f64;
    let px = def.pads.iter().map(|p| p.position.x).sum::<f64>() / n;
    let py = def.pads.iter().map(|p| p.position.y).sum::<f64>() / n;
    let bx = (body.0[0] + body.1[0]) / 2.0 - centre.x;
    let by = (body.0[1] + body.1[1]) / 2.0 - centre.y;
    Some((bx - px, by - py))
}

/// The middle of the footprint pads, in footprint coordinates.
///
/// # Rules
/// - A footprint with no pads returns `None`.
pub(crate) fn pad_centroid(def: &FootprintDefinition) -> Option<Point> {
    if def.pads.is_empty() {
        return None;
    }
    let n = def.pads.len() as f64;
    let x = def.pads.iter().map(|p| p.position.x).sum::<f64>() / n;
    let y = def.pads.iter().map(|p| p.position.y).sum::<f64>() / n;
    Some(Point::new(x, y))
}

/// The translation that moves the model body centre to the pad centroid.
///
/// # Rules
/// - `body` is the model bounding box in board coordinates.
/// - `centre` is the footprint origin in board coordinates.
/// - Returns `None` when there are no pads.
pub(crate) fn auto_align_translation(
    def: &FootprintDefinition,
    centre: Point,
    body: ([f64; 3], [f64; 3]),
) -> Option<Point> {
    let centroid = pad_centroid(def)?;
    let body_x = (body.0[0] + body.1[0]) / 2.0 - centre.x;
    let body_y = (body.0[1] + body.1[1]) / 2.0 - centre.y;
    Some(Point::new(centroid.x - body_x, centroid.y - body_y))
}

impl App {
    /// The footprint of the showing part, and whether it is in this project's library (so
    /// its model can be edited).
    fn library_model_footprint(&self, card: &Card) -> Option<(FootprintDefinition, bool)> {
        let def = self.dock_card_footprint(card)?;
        match self
            .project
            .library
            .footprints
            .iter()
            .find(|f| f.name == def.name)
        {
            Some(own) => Some((own.clone(), true)),
            None => Some((def, self.project.library.allow_library_footprint_edits)),
        }
    }

    /// Store a new transform (or file) for the footprint's model as one undoable command.
    fn library_set_model(&mut self, name: &str, model: &Model3D) {
        self.run(
            "library.footprint.set_model",
            json!({
                "name": name, "path": model.path,
                "offset": model.offset, "scale": model.scale, "rotate": model.rotate,
            }),
        );
    }

    /// The 3D model page.
    pub(super) fn library_model_page(&mut self, ui: &mut egui::Ui, card: &Card) {
        let t = theme::tokens(ui.ctx());
        let Some((def, editable)) = self.library_model_footprint(card) else {
            ui.label(fonts::text(fonts::Role::Body, t!("libws.model.no_footprint")).color(t.muted));
            return;
        };
        // Rebuild the one-part project only when the footprint or its model changed.
        let key = format!("{}|{}|{:?}", def.name, def.pads.len(), def.model);
        if self.libws.model_key != key {
            let same_part = self.libws.model_part == def.name;
            let (project, centre) = part_project(&def);
            self.libws.model_project = project;
            self.libws.model_centre = centre;
            self.libws.model_rev += 1;
            self.libws.model_key = key;
            self.libws.model_part = def.name.clone();
            self.libws.model_view.keep_camera = same_part;
            if !same_part {
                self.libws.model_view = board3d::View3d::for_config(&self.user_config());
            }
        }
        let mut model = def.model.clone();
        ui.columns(2, |cols| {
            self.model_viewport_card(&mut cols[0], &def);
            let ui = &mut cols[1];
            ui.spacing_mut().item_spacing.y = theme::SPACE[3];
            self.model_file_card(ui, &def, editable, &mut model);
            self.model_alignment_card(ui, &def, editable, &mut model);
            self.model_overlay_card(ui, &def);
        });
    }

    fn model_viewport_card(&mut self, ui: &mut egui::Ui, def: &FootprintDefinition) {
        let t = theme::tokens(ui.ctx());
        let status = self.libws.model_view.model_status(PART);
        let caption = match (&def.model, status) {
            (None, _) => t!("libws.model.generated"),
            (Some(_), Some(pcbcraft_io::ModelStatus::Loaded)) => t!("libws.model.loaded"),
            (Some(_), Some(pcbcraft_io::ModelStatus::Failed(why))) => {
                t!("libws.model.failed", why = why)
            }
            (Some(_), _) => t!("libws.model.loading"),
        };
        card(
            ui,
            &t!("libws.model.title"),
            Some(&caption),
            |_| {},
            |ui| {
                ui.horizontal(|ui| {
                    for (label, tip, yaw, pitch) in [
                        (
                            t!("board.board3d.top_view"),
                            t!("board.board3d.top_view.tip"),
                            -std::f64::consts::FRAC_PI_2,
                            1.5,
                        ),
                        (
                            t!("workspace.front"),
                            t!("workspace.front.tip"),
                            -std::f64::consts::FRAC_PI_2,
                            0.0,
                        ),
                        (t!("workspace.side"), t!("workspace.side.tip"), 0.0, 0.0),
                    ] {
                        if secondary_button(ui, None, &label, &tip, Size::Small, true).clicked() {
                            self.libws.model_view.yaw = yaw;
                            self.libws.model_view.pitch = pitch;
                        }
                    }
                    if ghost_button(
                        ui,
                        None,
                        &t!("board.board3d.reset_view"),
                        &t!("board.board3d.reset_view.tip"),
                        Size::Small,
                        true,
                    )
                    .clicked()
                    {
                        self.libws.model_view.fit();
                    }
                });
                let mut env = board3d::Env {
                    project: &self.libws.model_project,
                    revision: self.libws.model_rev,
                    selected: None,
                    bg: self.surface(),
                    muted: t.muted,
                    design_dir: None,
                    message: None,
                };
                if let Some(why) = self.libws.model_view.build_error() {
                    ui.label(fonts::text(fonts::Role::Body, why).color(t.red));
                }
                let size = egui::vec2(ui.available_width(), 440.0);
                board3d::viewport(&mut self.libws.model_view, ui, &mut env, size);
                let status = self.libws.model_view.model_status(PART);
                let source = match (&def.model, &status) {
                    (None, _) => part3d::ModelSource::Generated,
                    (Some(_), Some(pcbcraft_io::ModelStatus::Loaded)) => part3d::ModelSource::File,
                    (Some(_), _) => part3d::ModelSource::Generated,
                };
                let why = match &status {
                    Some(pcbcraft_io::ModelStatus::Failed(why)) => Some(why.clone()),
                    _ => None,
                };
                match pcbcraft_io::board_mesh(
                    &self.libws.model_project,
                    pcbcraft_io::MeshLayers::default(),
                ) {
                    Ok(mesh) => {
                        let (low, high) = mesh.bounds().unwrap_or(([0.0; 3], [0.0; 3]));
                        self.libws.model3d.set_load_result(
                            source,
                            mesh.triangle_count(),
                            low,
                            high,
                            "",
                            why.as_deref(),
                        );
                    }
                    Err(e) => self.libws.model3d.set_load_result(
                        part3d::ModelSource::None,
                        0,
                        [0.0; 3],
                        [0.0; 3],
                        "",
                        Some(&e.to_string()),
                    ),
                }
                self.libws.model3d.under_viewport(ui);
                ui.label(fonts::caption(t!("libws.model.hint")).color(t.muted));
            },
        );
    }

    fn model_file_card(
        &mut self,
        ui: &mut egui::Ui,
        def: &FootprintDefinition,
        editable: bool,
        model: &mut Option<Model3D>,
    ) {
        let t = theme::tokens(ui.ctx());
        let key = ui.id().with("library-model-path");
        let mut path: String = ui
            .data_mut(|d| d.get_temp(key))
            .unwrap_or_else(|| model.as_ref().map(|m| m.path.clone()).unwrap_or_default());
        card(
            ui,
            &t!("libws.model.file"),
            None,
            |_| {},
            |ui| {
                let shown = match model {
                    Some(m) => m.path.clone(),
                    None => t!("libws.model.none"),
                };
                ui.label(fonts::mono(shown).color(t.text));
                ui.horizontal(|ui| {
                    ui.add_enabled(
                        editable,
                        crate::targets::text_field(&mut path)
                            .hint_text(t!("libws.model.path_hint"))
                            .desired_width(220.0),
                    )
                    .named(&t!("libws.model.path"))
                    .on_hover_text(t!("libws.model.path.tip"));
                    let ready = editable && !path.trim().is_empty();
                    if secondary_button(
                        ui,
                        None,
                        &t!("libws.model.use_file"),
                        &t!("libws.model.use_file.tip"),
                        Size::Small,
                        ready,
                    )
                    .clicked()
                    {
                        let trimmed = path.trim().to_owned();
                        let same_path = model.as_ref().is_some_and(|m| m.path == trimmed);
                        let next = Model3D {
                            path: trimmed,
                            offset: model.as_ref().map_or([0.0; 3], |m| m.offset),
                            scale: model.as_ref().map_or([1.0; 3], |m| m.scale),
                            rotate: model.as_ref().map_or([0.0; 3], |m| m.rotate),
                            raw_source: String::new(),
                            body_id: Default::default(),
                            cad_part: if same_path {
                                model.as_ref().and_then(|m| m.cad_part.clone())
                            } else {
                                Default::default()
                            },
                            embedded: Default::default(),
                        };
                        self.library_set_model(&def.name, &next);
                        self.libws.model_rev += 1;
                    }
                    if let Some(m) = model.as_ref().cloned()
                        && ghost_button(
                            ui,
                            None,
                            &t!("libws.model.reimport"),
                            &t!("libws.model.reimport.tip"),
                            Size::Small,
                            editable,
                        )
                        .clicked()
                    {
                        self.library_set_model(&def.name, &m);
                        self.libws.model_rev += 1;
                    }
                    if model.is_some()
                        && ghost_button(
                            ui,
                            None,
                            &t!("libws.model.remove"),
                            &t!("libws.model.remove.tip"),
                            Size::Small,
                            editable,
                        )
                        .clicked()
                    {
                        self.run("library.footprint.clear_model", json!({"name": def.name}));
                        path.clear();
                    }
                });
                if !editable {
                    ui.label(fonts::caption(t!("libws.model.read_only")).color(t.muted));
                    let mut allow = self.project.library.allow_library_footprint_edits;
                    if ui
                        .checkbox(&mut allow, t!("libws.model.allow_library_edits"))
                        .changed()
                    {
                        self.project.library.allow_library_footprint_edits = allow;
                    }
                    ui.label(
                        fonts::caption(t!("libws.model.allow_library_edits_tip")).color(t.muted),
                    );
                }
            },
        );
        ui.data_mut(|d| d.insert_temp(key, path));
    }

    fn model_alignment_card(
        &mut self,
        ui: &mut egui::Ui,
        def: &FootprintDefinition,
        editable: bool,
        model: &mut Option<Model3D>,
    ) {
        let t = theme::tokens(ui.ctx());
        let has_model = model.is_some();
        let mut reset = false;
        let mut snap = false;
        let mut auto_align = false;
        let mut next: Option<Model3D> = None;
        card(
            ui,
            &t!("libws.model.alignment"),
            None,
            |ui| {
                snap = ghost_button(
                    ui,
                    None,
                    &t!("libws.model.snap"),
                    &t!("libws.model.snap.tip"),
                    Size::Small,
                    editable && has_model,
                )
                .clicked();
                auto_align = ghost_button(
                    ui,
                    None,
                    &t!("libws.model.auto_align"),
                    &t!("libws.model.auto_align.tip"),
                    Size::Small,
                    editable && has_model,
                )
                .clicked();
                reset = ghost_button(
                    ui,
                    None,
                    &t!("libws.model.reset"),
                    &t!("libws.model.reset.tip"),
                    Size::Small,
                    editable && has_model,
                )
                .clicked();
            },
            |ui| {
                let Some(m) = model.as_mut() else {
                    ui.label(
                        fonts::text(fonts::Role::Body, t!("libws.model.align_needs_file"))
                            .color(t.muted),
                    );
                    return;
                };
                let before = m.clone();
                let mm = NumberSpec {
                    unit: "mm",
                    step: 0.1,
                    places: 2,
                    ..Default::default()
                };
                let deg = NumberSpec {
                    unit: "\u{b0}",
                    step: 90.0,
                    min: -360.0,
                    max: 360.0,
                    places: 0,
                    ..Default::default()
                };
                let pct = NumberSpec {
                    unit: "%",
                    step: 1.0,
                    min: 1.0,
                    max: 10000.0,
                    places: 0,
                    ..Default::default()
                };
                ui.add_enabled_ui(editable, |ui| {
                    for (i, axis) in ["X", "Y", "Z"].iter().enumerate() {
                        form_row(
                            ui,
                            FormWidth::Aside,
                            &t!("libws.model.offset", axis = axis),
                            None,
                            None,
                            |ui| {
                                number_field(
                                    ui,
                                    ("lib-model-off", i),
                                    &mut m.offset[i],
                                    &mm,
                                    &t!("libws.model.offset", axis = axis),
                                    &t!("libws.model.offset.tip", axis = axis),
                                );
                            },
                        );
                    }
                    for (i, axis) in ["X", "Y", "Z"].iter().enumerate() {
                        form_row(
                            ui,
                            FormWidth::Aside,
                            &t!("libws.model.rotation", axis = axis),
                            None,
                            None,
                            |ui| {
                                number_field(
                                    ui,
                                    ("lib-model-rot", i),
                                    &mut m.rotate[i],
                                    &deg,
                                    &t!("libws.model.rotation", axis = axis),
                                    &t!("libws.model.rotation.tip", axis = axis),
                                );
                            },
                        );
                    }
                    let mut percent = m.scale[0] * 100.0;
                    form_row(
                        ui,
                        FormWidth::Aside,
                        &t!("libws.model.scale"),
                        None,
                        None,
                        |ui| {
                            number_field(
                                ui,
                                "lib-model-scale",
                                &mut percent,
                                &pct,
                                &t!("libws.model.scale"),
                                &t!("libws.model.scale.tip"),
                            );
                        },
                    );
                    m.scale = [percent / 100.0; 3];
                });
                if *m != before {
                    next = Some(m.clone());
                }
            },
        );
        if reset && let Some(m) = model.as_ref() {
            let mut back = m.clone();
            back.offset = [0.0; 3];
            back.rotate = [0.0; 3];
            back.scale = [1.0; 3];
            next = Some(back);
        }
        if snap && let Some(m) = model.as_ref() {
            let centre = self.libws.model_centre;
            let body = self.libws.model_view.body_bounds(PART);
            if let Some(b) = body
                && let Some(trans) = snap_to_nearest_pad(def, centre, b)
            {
                let mut fixed = m.clone();
                fixed.offset[0] = trans.x;
                fixed.offset[1] = trans.y;
                next = Some(fixed);
            }
        }
        if auto_align && let Some(m) = model.as_ref() {
            let centre = self.libws.model_centre;
            let body = self.libws.model_view.body_bounds(PART);
            if let Some(b) = body
                && let Some(trans) = auto_align_translation(def, centre, b)
            {
                let mut fixed = m.clone();
                fixed.offset[0] = trans.x;
                fixed.offset[1] = trans.y;
                next = Some(fixed);
            }
        }
        if let Some(m) = next {
            self.library_set_model(&def.name, &m);
        }
    }

    fn model_overlay_card(&mut self, ui: &mut egui::Ui, def: &FootprintDefinition) {
        let t = theme::tokens(ui.ctx());
        let centre = self.libws.model_centre;
        let body = self.libws.model_view.body_bounds(PART);
        let off = body.and_then(|b| centre_offset(def, centre, b));
        let (badge_text, kind) = match off {
            Some((dx, dy)) if dx.hypot(dy) <= ALIGNED_WITHIN_MM => (
                t!("libws.model.aligned"),
                crate::ui_kit::chip::BadgeKind::Success,
            ),
            Some((dx, dy)) => (
                t!(
                    "libws.model.off_by",
                    dx = format!("{dx:.2}"),
                    dy = format!("{dy:.2}")
                ),
                crate::ui_kit::chip::BadgeKind::Warning,
            ),
            None => (
                t!("libws.model.no_overlay"),
                crate::ui_kit::chip::BadgeKind::Neutral,
            ),
        };
        card(
            ui,
            &t!("libws.model.overlay"),
            None,
            |ui| {
                crate::ui_kit::chip::badge(ui, &badge_text, kind, &t!("libws.model.overlay.tip"));
                if let Some((dx, dy)) = off
                    && dx.hypot(dy) > WARN_ABOVE_MM
                {
                    ui.label(fonts::caption(t!("libws.model.warn_large_offset")).color(t.red));
                }
            },
            |ui| {
                let (rect, _) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width(), 180.0),
                    egui::Sense::hover(),
                );
                ui.painter().rect_filled(rect, 4.0, self.surface());
                let paths = library::library_geometry_paths(None, Some(def), 1);
                let mut all = paths.clone();
                // The model's outline in the footprint's own frame.
                let outline = body.map(|(lo, hi)| {
                    let (a, b) = (
                        Point::new(lo[0] - centre.x, lo[1] - centre.y),
                        Point::new(hi[0] - centre.x, hi[1] - centre.y),
                    );
                    vec![a, Point::new(b.x, a.y), b, Point::new(a.x, b.y), a]
                });
                if let Some(o) = &outline {
                    all.push(o.clone());
                }
                if all.iter().all(Vec::is_empty) {
                    return;
                }
                let (c, extent) = library::library_paths_extent(&all);
                let camera = Camera {
                    center: c,
                    zoom: ((rect.width() - 32.0) / extent.x as f32)
                        .min((rect.height() - 32.0) / extent.y as f32)
                        .clamp(0.01, 60.0),
                    mirror: false,
                    turns: Default::default(),
                    up: Default::default(),
                };
                let painter = ui.painter_at(rect);
                library::paint_library_paths(&painter, rect, camera, &paths, t.copper);
                if let Some(o) = outline {
                    library::paint_library_paths(&painter, rect, camera, &[o], t.mask);
                }
            },
        );
    }
}

/// The translation that moves the model body centre to the nearest pad centre.
///
/// # Rules
/// - `body` is the model bounding box in board coordinates.
/// - `centre` is the footprint origin in board coordinates.
/// - Returns `None` when there are no pads.
pub(crate) fn snap_to_nearest_pad(
    def: &FootprintDefinition,
    centre: Point,
    body: ([f64; 3], [f64; 3]),
) -> Option<Point> {
    if def.pads.is_empty() {
        return None;
    }
    let body_x = (body.0[0] + body.1[0]) / 2.0 - centre.x;
    let body_y = (body.0[1] + body.1[1]) / 2.0 - centre.y;
    let mut best = None;
    let mut best_dist = f64::MAX;
    for pad in &def.pads {
        let dist = ((pad.position.x - body_x).powi(2) + (pad.position.y - body_y).powi(2)).sqrt();
        if dist < best_dist {
            best_dist = dist;
            best = Some(pad.position);
        }
    }
    best.map(|p| Point::new(p.x - body_x, p.y - body_y))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pcbcraft_library::FootprintPad;

    fn pad(n: &str, x: f64) -> FootprintPad {
        let mut p = crate::authoring::new_pad(n, x);
        p.position = Point::new(x, 0.0);
        p
    }

    fn two_pads() -> FootprintDefinition {
        let mut def = crate::authoring::new_footprint();
        def.pads = vec![pad("1", -1.5), pad("2", 1.5)];
        def
    }

    #[test]
    fn the_one_part_project_has_the_part_and_its_pads_at_the_board_centre() {
        let (p, centre) = part_project(&two_pads());
        assert_eq!(p.board.footprints.len(), 1);
        assert_eq!(p.board.footprints[0].pads.len(), 2);
        assert_eq!(p.board.footprints[0].position, centre);
        assert!(p.board.width > 6.0, "the board is larger than the pads");
    }

    #[test]
    fn the_one_part_project_builds_a_mesh_with_a_body() -> Result<(), String> {
        let (p, _) = part_project(&two_pads());
        let mesh = pcbcraft_io::board_mesh(&p, pcbcraft_io::MeshLayers::default());
        let mesh = mesh.map_err(|e| format!("mesh error: {e}"))?;
        assert!(
            mesh.parts
                .iter()
                .any(|m| m.kind == pcbcraft_io::PartKind::Body && m.component == Some(PART)),
            "the renderer makes a body for the part"
        );
        Ok(())
    }

    #[test]
    fn a_model_centred_on_the_pads_is_aligned_and_a_shifted_one_is_not() {
        let def = two_pads();
        let centre = Point::new(10.0, 10.0);
        let body = ([8.0, 9.0, 0.0], [12.0, 11.0, 1.0]);
        let (dx, dy) = centre_offset(&def, centre, body).unwrap_or((9.0, 9.0));
        assert!(dx.hypot(dy) < ALIGNED_WITHIN_MM);
        let shifted = ([9.0, 9.0, 0.0], [13.0, 11.0, 1.0]);
        let (dx, _) = centre_offset(&def, centre, shifted).unwrap_or((0.0, 0.0));
        assert!((dx - 1.0).abs() < 1e-9, "one millimetre to the right");
    }

    #[test]
    fn auto_align_recovers_a_shifted_model() -> Result<(), String> {
        let def = two_pads();
        let centre = Point::new(10.0, 10.0);
        // A model body shifted 2mm right and 1mm down from the pad centroid.
        let shifted_body = ([12.0, 10.0, 0.0], [16.0, 12.0, 1.0]);
        let trans = auto_align_translation(&def, centre, shifted_body)
            .ok_or("a footprint with pads has a translation")?;
        // The translation should move the body back to the pad centroid.
        // Pad centroid is at (0, 0) in footprint coords, body centre is at (14, 11) - (10, 10) = (4, 1).
        // So translation should be (-4, -1).
        assert!(
            (trans.x - (-4.0)).abs() < 0.05,
            "x translation recovers the shift"
        );
        assert!(
            (trans.y - (-1.0)).abs() < 0.05,
            "y translation recovers the shift"
        );
        Ok(())
    }

    #[test]
    fn pad_centroid_is_the_average_of_pad_positions() -> Result<(), String> {
        let def = two_pads();
        let centroid = pad_centroid(&def).ok_or("two pads have a centroid")?;
        // Pads at (-1.5, 0) and (1.5, 0), centroid at (0, 0).
        assert!((centroid.x - 0.0).abs() < 1e-9);
        assert!((centroid.y - 0.0).abs() < 1e-9);
        Ok(())
    }

    #[test]
    fn pad_centroid_returns_none_for_no_pads() {
        let mut def = crate::authoring::new_footprint();
        def.pads = vec![];
        assert!(pad_centroid(&def).is_none());
    }
}
