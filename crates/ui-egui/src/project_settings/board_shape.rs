//! Board tab, shape cards: the drawing of the board shape, what it says in words, and the
//! shape of each copper layer.
//!
//! Context: mockup `settings.png` (issue "Project settings Board tab: board shape and
//! shape of each layer"). The board is a free-form closed path a machine cuts. A rectangle
//! is only a starting template, so no width or height is a setting; "Reaches" is the read
//! only box around the shapes.
//! Local rules: the card reads the outline through `facts`, a private reader of
//! `Board.outline`; it draws a rectangle from the stored width and height when the board
//! has no traced outline. "Start from a rectangle" is the only place the two numbers are
//! typed, and it runs one `board.set_shape` command so it is one undo step.
//! Each copper layer follows the board outline until it gets a shape of its own.
//!
//! # Dependencies
//! - crates/doc/src/lib.rs (`Board`, `Point`), crates/doc/src/outline.rs (`outline_extent`,
//!   `flatten_outline`)
//! - crates/engine/src/outline.rs (`board.set_shape`)
//! - crates/io/src/formats.rs (`svg_drawing`, `dxf_drawing` import formats)
//! - crates/ui-egui/src/ui_kit/card.rs, form_row.rs, rows.rs, dialog.rs
//!
//! # Do not
//! - Offer a width or a height as a setting of the board.
//! - Compute outline vertices other than the template rectangle; the engine owns geometry.
//! - Offer a per layer control the engine cannot run yet.
use super::{NUMBER_WIDTH, mm_spec, note, num, secondary};
use crate::ui_kit::button::{self, Size};
use crate::ui_kit::card::{card, card_flush};
use crate::ui_kit::dialog::{self, Dialog, Outcome};
use crate::ui_kit::form_row::{FormWidth, form_row};
use crate::ui_kit::icons;
use crate::ui_kit::number_field::NumberSpec;
use crate::ui_kit::rows::{table_head, table_row};
use crate::{App, Tab, fonts, theme};
use egui::{Color32, Rect, Sense, Stroke, StrokeKind, pos2, vec2};
use pcbcraft_doc::{Board, flatten_outline, outline_extent};
use pcbcraft_geom::Point;
use pcbcraft_text::t;
use serde_json::json;

/// Height of the drawing of the shape.
const PREVIEW_HEIGHT: f32 = 192.0;
/// Width of the drawing of the shape.
const PREVIEW_WIDTH: f32 = 240.0;
/// Width of the rectangle dialog.
const RECT_DIALOG_WIDTH: f32 = 420.0;

/// What the shape card reads from the board.
pub(super) struct Facts {
    /// Corners of the closed path, in millimetres.
    pub points: Vec<Point>,
    /// Arc midpoints per segment, as the board stores them.
    pub arcs: Vec<Option<Point>>,
    /// Whether the path is a plain axis aligned rectangle.
    pub rectangle: bool,
    /// Lower and upper corner of the box around the shape.
    pub extent: (Point, Point),
}

/// Read the shape of the board.
///
/// Why: the card must show the real outline; a board with no traced outline is a
/// rectangle of its stored width and height.
///
/// # Rules
/// - Corners count the vertices of the path.
/// - A rectangle has four vertices, no arcs and two distinct x and y values.
///
/// # Do not
/// - Use for anything that writes; the engine owns the outline.
pub(super) fn facts(board: &Board) -> Facts {
    let (points, arcs) = match &board.outline {
        Some(points) if points.len() >= 3 => (points.clone(), board.outline_arcs.clone()),
        _ => (
            vec![
                Point::new(0.0, 0.0),
                Point::new(board.width, 0.0),
                Point::new(board.width, board.height),
                Point::new(0.0, board.height),
            ],
            Vec::new(),
        ),
    };
    let extent = outline_extent(&points, &arcs)
        .unwrap_or((Point::new(0.0, 0.0), Point::new(board.width, board.height)));
    let mut xs: Vec<u64> = points.iter().map(|p| p.x.to_bits()).collect();
    let mut ys: Vec<u64> = points.iter().map(|p| p.y.to_bits()).collect();
    xs.sort_unstable();
    xs.dedup();
    ys.sort_unstable();
    ys.dedup();
    let rectangle =
        points.len() == 4 && arcs.iter().all(Option::is_none) && xs.len() == 2 && ys.len() == 2;
    Facts {
        points,
        arcs,
        rectangle,
        extent,
    }
}

/// The copper layers the table lists, by name, in stackup order.
///
/// Why: only copper layers have a shape; silkscreen, mask, paste, pads, tracks and zones
/// follow their copper layer.
pub(super) fn copper_layer_names(board: &Board) -> Vec<String> {
    let names: Vec<String> = board
        .layer_records
        .iter()
        .filter(|r| r.kind == "copper")
        .map(|r| r.name.clone())
        .collect();
    if names.is_empty() {
        vec![
            t!("settings.project.layers.default_top"),
            t!("settings.project.layers.default_bottom"),
        ]
    } else {
        names
    }
}

/// Paint the shape into `rect`: the filled path, its edge, cut-outs and mounting holes.
///
/// Duplication: the files page paints the same drawing in `shape_preview.rs`
/// (`paint_shape`); merge the two once both are on main.
///
/// # Rules
/// - The drawing keeps the proportions of the shape and sits centred in `rect`.
/// - Concave paths are filled by scanlines, since epaint fills convex paths only.
fn paint_shape(ui: &egui::Ui, rect: Rect, board: &Board, facts: &Facts) {
    let canvas = theme::CanvasColors::for_dark(ui.ctx().global_style().visuals.dark_mode);
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, canvas.background);
    let (lo, hi) = facts.extent;
    let (w, h) = ((hi.x - lo.x).max(1e-6), (hi.y - lo.y).max(1e-6));
    let inner = rect.shrink(18.0);
    let scale = (inner.width() as f64 / w).min(inner.height() as f64 / h) as f32;
    let origin = pos2(
        inner.center().x - (w as f32 * scale) / 2.0,
        inner.center().y + (h as f32 * scale) / 2.0,
    );
    let to_screen = |p: Point| {
        pos2(
            origin.x + (p.x - lo.x) as f32 * scale,
            origin.y - (p.y - lo.y) as f32 * scale,
        )
    };
    let ring: Vec<_> = flatten_outline(&facts.points, &facts.arcs, 0.05)
        .into_iter()
        .map(to_screen)
        .collect();
    fill_path(&painter, &ring, canvas.board_fill);
    let edge = theme::tokens(ui.ctx()).gold;
    painter.add(egui::Shape::closed_line(
        ring.clone(),
        Stroke::new(1.5, edge),
    ));
    for cut in &board.cutouts {
        painter.circle_stroke(
            to_screen(cut.center),
            (cut.radius as f32 * scale).max(2.0),
            Stroke::new(1.0, edge),
        );
    }
    for hole in &board.holes {
        let radius = (hole.diameter as f32 / 2.0 * scale).max(2.5);
        painter.circle_stroke(to_screen(hole.start()), radius, Stroke::new(1.0, edge));
        if hole.is_slot() {
            painter.circle_stroke(to_screen(hole.finish()), radius, Stroke::new(1.0, edge));
        }
    }
}

/// Fill a closed path with a solid colour, concave or not, one pixel row at a time.
fn fill_path(painter: &egui::Painter, ring: &[egui::Pos2], color: Color32) {
    let (top, bottom) = ring.iter().fold((f32::MAX, f32::MIN), |(lo, hi), p| {
        (lo.min(p.y), hi.max(p.y))
    });
    if ring.len() < 3 || bottom < top {
        return;
    }
    let mut y = top.ceil() + 0.5;
    while y < bottom {
        let mut xs: Vec<f32> = Vec::new();
        for i in 0..ring.len() {
            let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
            if (a.y <= y) != (b.y <= y) {
                xs.push(a.x + (y - a.y) / (b.y - a.y) * (b.x - a.x));
            }
        }
        xs.sort_by(f32::total_cmp);
        for i in (0..xs.len().saturating_sub(1)).step_by(2) {
            painter.rect_filled(
                Rect::from_min_max(pos2(xs[i], y - 0.5), pos2(xs[i + 1], y + 0.5)),
                0.0,
                color,
            );
        }
        y += 1.0;
    }
}

/// One row of the facts: label and hint at the left, the value at the right.
fn fact_row(ui: &mut egui::Ui, label: &str, hint: Option<&str>, value: &str, mono: bool) {
    form_row(ui, FormWidth::Page, label, hint, None, |ui| {
        let text = theme::tokens(ui.ctx()).text;
        if mono {
            ui.label(fonts::mono(value).color(text));
        } else {
            ui.label(fonts::text(fonts::Role::Body, value).color(text));
        }
    });
}

impl App {
    /// The Board shape card: the drawing, the facts and the three actions.
    ///
    /// # Rules
    /// - Outline says Free-form, or a plain name when the path is a rectangle.
    /// - Reaches is read only.
    /// - Edit in Layout closes the dialog and shows the Layout tab.
    ///
    /// # Dependencies
    /// - crates/ui-egui/src/ui_kit/card.rs
    ///
    /// # Do not
    /// - Type a width or a height here; the rectangle window is the only place.
    pub(super) fn settings_board_shape(&mut self, ui: &mut egui::Ui) {
        let board = self.project.board.clone();
        let facts = facts(&board);
        let edit = std::cell::Cell::new(false);
        let mut replace = false;
        let mut rectangle = false;
        card(
            ui,
            &t!("settings.project.board.shape"),
            None,
            |ui| {
                edit.set(
                    button::secondary_button(
                        ui,
                        Some(icons::PATH),
                        &t!("settings.project.shape.edit"),
                        &t!("settings.project.shape.edit.tip"),
                        Size::Small,
                        true,
                    )
                    .clicked(),
                );
            },
            |ui| {
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = theme::SPACE[3];
                    let (rect, response) =
                        ui.allocate_exact_size(vec2(PREVIEW_WIDTH, PREVIEW_HEIGHT), Sense::hover());
                    response.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::WidgetType::Label,
                            true,
                            t!("settings.project.shape.preview"),
                        )
                    });
                    paint_shape(ui, rect, &board, &facts);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = theme::SPACE[0];
                        let name = if facts.rectangle {
                            t!("settings.project.shape.rectangle_name")
                        } else {
                            t!("settings.project.shape.free")
                        };
                        fact_row(
                            ui,
                            &t!("settings.project.shape.outline"),
                            Some(&t!("settings.project.shape.outline.hint")),
                            &name,
                            false,
                        );
                        fact_row(
                            ui,
                            &t!("settings.project.shape.corners"),
                            None,
                            &facts.points.len().to_string(),
                            false,
                        );
                        fact_row(
                            ui,
                            &t!("settings.project.shape.cutouts"),
                            Some(&t!("settings.project.shape.cutouts.hint")),
                            &board.cutouts.len().to_string(),
                            false,
                        );
                        let (lo, hi) = facts.extent;
                        let reach = t!(
                            "settings.project.shape.reach.value",
                            width = format!("{:.1}", hi.x - lo.x),
                            height = format!("{:.1}", hi.y - lo.y)
                        );
                        fact_row(
                            ui,
                            &t!("settings.project.shape.reach"),
                            Some(&t!("settings.project.shape.reach.hint")),
                            &reach,
                            true,
                        );
                        ui.add_space(theme::SPACE[1]);
                        ui.horizontal(|ui| {
                            replace = button::secondary_button(
                                ui,
                                Some(icons::DOWNLOAD),
                                &t!("settings.project.shape.replace"),
                                &t!("settings.project.shape.replace.tip"),
                                Size::Normal,
                                true,
                            )
                            .clicked();
                            rectangle = secondary(
                                ui,
                                &t!("settings.project.shape.rectangle"),
                                &t!("settings.project.shape.rectangle.tip"),
                                true,
                            )
                            .clicked();
                        });
                    });
                });
            },
        );
        if rectangle {
            let (lo, hi) = facts.extent;
            let state = &mut self.project_settings;
            state.rect_open = true;
            state.rect_width = (hi.x - lo.x).max(1.0);
            state.rect_height = (hi.y - lo.y).max(1.0);
        }
        if replace {
            self.settings_replace_from_drawing();
        }
        if edit.get() {
            self.settings_edit_in_layout();
        }
    }

    /// Keep what the dialog applied, close it and show the Layout tab.
    fn settings_edit_in_layout(&mut self) {
        self.project_settings.open = false;
        self.project_settings.rect_open = false;
        self.project_settings.snapshot = None;
        self.change_tab(Tab::Pcb);
    }

    /// Keep what the dialog applied, close it and open the import window on an SVG drawing
    /// with "Use as board outline" on.
    fn settings_replace_from_drawing(&mut self) {
        self.project_settings.open = false;
        self.project_settings.rect_open = false;
        self.project_settings.snapshot = None;
        let mut values = std::collections::BTreeMap::new();
        values.insert("Use as board outline".to_owned(), "true".to_owned());
        self.open_dialog(
            crate::file_dialogs::Mode::Import,
            Some("svg_drawing"),
            Some(values),
            vec![],
        );
    }

    /// The Shape of each layer card: one row per copper layer.
    ///
    /// # Rules
    /// - Every copper layer follows the board until the model gives layers their own shape.
    /// - Layers may differ and overlap; the note says so.
    ///
    /// # Dependencies
    /// - crates/ui-egui/src/ui_kit/rows.rs
    ///
    /// # Do not
    /// - List silkscreen, mask or paste; they follow their copper layer.
    pub(super) fn settings_layer_shapes(&mut self, ui: &mut egui::Ui) {
        let names = copper_layer_names(&self.project.board);
        let canvas = theme::CanvasColors::for_dark(ui.ctx().global_style().visuals.dark_mode);
        let weights = [1.0, 1.6];
        card_flush(
            ui,
            &t!("settings.project.layers"),
            None,
            |_| {},
            |ui| {
                table_head(
                    ui,
                    &weights,
                    &[
                        &t!("settings.project.layers.col.layer"),
                        &t!("settings.project.layers.col.shape"),
                    ],
                );
                let last = names.len().saturating_sub(1);
                for (index, name) in names.iter().enumerate() {
                    let swatch = if index == 0 {
                        canvas.top_copper
                    } else if index == last {
                        canvas.bottom_copper
                    } else {
                        theme::tokens(ui.ctx()).gold
                    };
                    table_row(
                        ui,
                        false,
                        name,
                        &t!("settings.project.layers.row.tip"),
                        &weights,
                        |ui, column| {
                            let text = theme::tokens(ui.ctx()).text;
                            if column == 0 {
                                let (dot, _) =
                                    ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
                                ui.painter().rect(
                                    dot,
                                    2.0,
                                    swatch,
                                    Stroke::NONE,
                                    StrokeKind::Inside,
                                );
                                ui.label(fonts::text(fonts::Role::Body, name).color(text));
                            } else {
                                ui.label(
                                    fonts::text(
                                        fonts::Role::Body,
                                        t!("settings.project.layers.follows"),
                                    )
                                    .color(text),
                                );
                            }
                        },
                    );
                }
                egui::Frame::new()
                    .inner_margin(egui::Margin::symmetric(12, 8))
                    .show(ui, |ui| note(ui, t!("settings.project.layers.note")));
            },
        );
    }

    /// Take Escape and Enter for the rectangle window before Project settings sees them.
    ///
    /// Why: Project settings closes on Escape and saves on Enter; with the small window
    /// on top, those keys belong to it.
    ///
    /// # Rules
    /// - Escape closes the rectangle window; Enter outside a text box replaces the shape.
    ///
    /// # Do not
    /// - Call it after Project settings is drawn.
    pub(super) fn settings_rectangle_keys(&mut self, ctx: &egui::Context) {
        if !self.project_settings.rect_open {
            return;
        }
        let typing = ctx.egui_wants_keyboard_input();
        let (esc, enter) = ctx.input_mut(|i| {
            (
                i.consume_key(egui::Modifiers::NONE, egui::Key::Escape),
                !typing && i.consume_key(egui::Modifiers::NONE, egui::Key::Enter),
            )
        });
        if esc {
            self.project_settings.rect_open = false;
        } else if enter {
            self.settings_replace_with_rectangle();
        }
    }

    /// Replace the shape with the rectangle typed in the window, as one undo step.
    fn settings_replace_with_rectangle(&mut self) {
        let (w, h) = (
            self.project_settings.rect_width,
            self.project_settings.rect_height,
        );
        self.project_settings.rect_open = false;
        self.settings_run(
            "board.set_shape",
            json!({"kind": "rectangle", "width": w, "height": h}),
        );
    }

    /// The Start from a rectangle window, when open.
    ///
    /// # Rules
    /// - Width and height live in the dialog state; they start at the box around the shape.
    /// - Replace the shape runs `board.set_shape` for a rectangle: one command, one undo step.
    /// - Escape closes only this window; Enter outside a number box replaces the shape.
    ///
    /// # Dependencies
    /// - crates/ui-egui/src/ui_kit/dialog.rs
    ///
    /// # Do not
    /// - Show it while Project settings is closed.
    pub(super) fn settings_rectangle_window(&mut self, ctx: &egui::Context) {
        if !self.project_settings.rect_open {
            return;
        }
        let title = t!("settings.project.rect.title");
        let note_text = t!("settings.project.rect.note");
        let cancel = (
            t!("settings.project.rect.cancel"),
            t!("settings.project.rect.cancel.tip"),
        );
        let apply = (
            t!("settings.project.rect.apply"),
            t!("settings.project.rect.apply.tip"),
        );
        let spec = Dialog {
            id: "board-rectangle",
            title: &title,
            width: RECT_DIALOG_WIDTH,
            tabs: &[],
            note: Some(&note_text),
            cancel: Some((&cancel.0, &cancel.1)),
            primary: (&apply.0, &apply.1),
        };
        let mut tab = 0;
        let outcome = dialog::show(ctx, &spec, &mut tab, |ui| {
            let number = NumberSpec {
                min: 0.5,
                max: 2000.0,
                step: 0.5,
                places: 2,
                width: NUMBER_WIDTH,
                ..mm_spec(0.5, 2000.0)
            };
            let name = t!("settings.project.rect.width");
            if let Some(v) = form_row(ui, FormWidth::Page, &name, None, None, |ui| {
                num(
                    ui,
                    "ps-rect-w",
                    self.project_settings.rect_width,
                    &number,
                    &name,
                    &t!("settings.project.rect.width.tip"),
                )
            }) {
                self.project_settings.rect_width = v;
            }
            let name = t!("settings.project.rect.height");
            if let Some(v) = form_row(ui, FormWidth::Page, &name, None, None, |ui| {
                num(
                    ui,
                    "ps-rect-h",
                    self.project_settings.rect_height,
                    &number,
                    &name,
                    &t!("settings.project.rect.height.tip"),
                )
            }) {
                self.project_settings.rect_height = v;
            }
        });
        match outcome {
            Outcome::Open => {}
            Outcome::Cancel => self.project_settings.rect_open = false,
            Outcome::Primary => self.settings_replace_with_rectangle(),
        }
    }
}
