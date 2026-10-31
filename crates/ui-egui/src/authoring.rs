use super::*;
use crate::a11y::Named;
use egui::{Pos2, Sense};
use pcbcraft_geom::point_segment;
use pcbcraft_library::{
    ChamferCorners, Drill, FootprintDefinition, FootprintPad, Graphic, MAX_PAD_POLYGON_VERTICES,
    PadChamfer, SymbolDefinition, SymbolPin, UnitGraphic,
};

/// Most pads the editor lists one by one; a 40 by 40 BGA fits.
/// Placing a footprint still stops at 64 pads until the placement limit is lifted.
const PAD_LIST_LIMIT: usize = 1600;

#[derive(Clone)]
pub(super) enum Draft {
    Symbol(SymbolDefinition),
    Footprint(FootprintDefinition),
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum LibraryTool {
    #[default]
    Select,
    Pin,
    Pad,
    Line,
    Rectangle,
    Circle,
    Arc,
    Polyline,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LibraryPick {
    Pin(usize),
    Pad(usize),
    SymbolGraphic(usize),
    FootprintGraphic(usize),
}

pub(super) struct Editor {
    pub mode: u8,
    pub draft: Option<Draft>,
    pub path: String,
    pub notice: Option<String>,
    pub wizard: super::footprint_wizard::Wizard,
    pub tool: LibraryTool,
    pub camera: Camera,
    pub needs_fit: bool,
    pub canvas_rect: Option<Rect>,
    pick: Option<LibraryPick>,
    graphic_start: Option<Point>,
    poly_points: Vec<Point>,
    arc_points: Vec<Point>,
    drag_anchor: Option<(LibraryPick, Point)>,
}
impl Default for Editor {
    fn default() -> Self {
        Self {
            mode: 0,
            draft: None,
            path: "parts.pcbcraft-lib".into(),
            notice: None,
            wizard: Default::default(),
            tool: LibraryTool::Select,
            camera: Camera {
                mirror: false,
                turns: 0,
                center: Point::default(),
                zoom: 18.0,
            },
            needs_fit: true,
            canvas_rect: None,
            pick: None,
            graphic_start: None,
            poly_points: Vec::new(),
            arc_points: Vec::new(),
            drag_anchor: None,
        }
    }
}
impl Editor {
    fn reset_draw(&mut self) {
        self.graphic_start = None;
        self.poly_points.clear();
        self.arc_points.clear();
    }
    fn symbol_layer(&self) -> &'static str {
        ""
    }
    fn footprint_layer(&self) -> &'static str {
        "F.SilkS"
    }
}
pub(super) fn initial_editor(tab: Option<&str>, project: &Project) -> Editor {
    match tab {
        Some("project-library") => Editor {
            mode: 1,
            ..Default::default()
        },
        Some("symbol-editor") => Editor {
            mode: 2,
            draft: Some(Draft::Symbol(
                project
                    .library
                    .symbols
                    .first()
                    .cloned()
                    .unwrap_or_else(new_symbol),
            )),
            needs_fit: true,
            ..Default::default()
        },
        Some("footprint-editor") => Editor {
            mode: 2,
            draft: Some(Draft::Footprint(
                project
                    .library
                    .footprints
                    .first()
                    .cloned()
                    .unwrap_or_else(new_footprint),
            )),
            needs_fit: true,
            ..Default::default()
        },
        _ => Editor::default(),
    }
}
fn rectangle(layer: &str) -> Graphic {
    Graphic::Rectangle {
        start: Point::new(-3.0, -2.0),
        end: Point::new(3.0, 2.0),
        width: 0.15,
        fill: "none".into(),
        layer: layer.into(),
    }
}
pub(super) fn new_symbol() -> SymbolDefinition {
    SymbolDefinition {
        name: "NewSymbol".into(),
        reference: "U".into(),
        value: "NewSymbol".into(),
        description: "Custom symbol".into(),
        keywords: String::new(),
        footprint: String::new(),
        set: String::new(),
        extends: None,
        raw_source: String::new(),
        unsupported: vec![],
        pins: vec![
            new_pin("1", 1, Point::new(-5.0, 0.0), 0.0),
            new_pin("2", 1, Point::new(5.0, 0.0), 180.0),
        ],
        graphics: vec![UnitGraphic {
            unit: 0,
            body_style: 1,
            primitive: rectangle(""),
        }],
    }
}
fn new_pin(number: &str, unit: u32, position: Point, angle: f64) -> SymbolPin {
    SymbolPin {
        number: number.into(),
        name: String::new(),
        position,
        angle,
        length: 2.0,
        electrical_type: "passive".into(),
        unit,
        body_style: 1,
        graphical_style: "line".into(),
        visible: true,
    }
}
pub(crate) fn new_pad(number: &str, x: f64) -> FootprintPad {
    FootprintPad {
        number: number.into(),
        pad_type: "thru_hole".into(),
        shape: "circle".into(),
        position: Point::new(x, 0.0),
        rotation: 0.0,
        size: Point::new(1.8, 1.8),
        layers: vec!["*.Cu".into(), "*.Mask".into()],
        drill: Some(Drill {
            size: Point::new(0.8, 0.8),
            offset: Point::default(),
        }),
        roundrect_ratio: None,
        chamfer: None,
        polygon: vec![],
        raw_source: String::new(),
    }
}
pub(super) fn new_footprint() -> FootprintDefinition {
    FootprintDefinition {
        name: "NewFootprint".into(),
        description: "Custom through-hole package".into(),
        keywords: String::new(),
        pads: vec![new_pad("1", -2.5), new_pad("2", 2.5)],
        graphics: vec![rectangle("F.SilkS")],
        model: None,
        rules: None,
        raw_source: String::new(),
        unsupported: vec![],
    }
}
/// Plain label for a pad shape in the shape list.
///
/// # Rules
/// - Words come from the text catalog; unknown shapes show as stored.
///
/// # Dependencies
/// - `crates/text/strings/en.txt` keys `library.library_editor.shape_*`.
///
/// # Do not
/// - Change the stored shape names; the library and the writers match them.
fn pad_shape_label(shape: &str) -> String {
    match shape {
        "circle" => t!("library.library_editor.shape_circle"),
        "rect" => t!("library.library_editor.shape_rect"),
        "oval" => t!("library.library_editor.shape_oval"),
        "roundrect" => t!("library.library_editor.shape_roundrect"),
        "chamfered" => t!("library.library_editor.shape_chamfered"),
        "custom" => t!("library.library_editor.shape_custom"),
        other => other.to_string(),
    }
}
/// Shape list for one pad, with the extra controls the chosen shape needs.
///
/// Choosing chamfered starts a quarter-size cut on the top left corner
/// (the usual pin 1 mark). Choosing custom starts from the pad's own
/// rectangle so the outline is valid at once. Leaving a shape drops the
/// data that only that shape uses.
///
/// # Rules
/// - Every control has a tooltip and a screen reader name.
/// - Outline problems show in red from `pcbcraft_library::custom_polygon_error`.
///
/// # Dependencies
/// - `crates/library/src/lib.rs` for `PadChamfer`, `ChamferCorners`,
///   `MAX_PAD_POLYGON_VERTICES` and `custom_polygon_error`.
/// - `crates/ui-egui/src/a11y.rs` for `name_control`.
///
/// # Do not
/// - Save from here; the footprint editor's save button validates.
fn pad_shape_controls(ui: &mut egui::Ui, pad: &mut FootprintPad) {
    let before = pad.shape.clone();
    egui::ComboBox::from_id_salt("pad-shape")
        .selected_text(pad_shape_label(&pad.shape))
        .show_ui(ui, |ui| {
            for shape in ["circle", "rect", "oval", "roundrect", "chamfered", "custom"] {
                let tip = match shape {
                    "chamfered" => t!("library.library_editor.shape_chamfered.tip"),
                    "custom" => t!("library.library_editor.shape_custom.tip"),
                    _ => t!("library.library_editor.copper_outline_of_the_pad.tip"),
                };
                ui.selectable_value(&mut pad.shape, shape.into(), pad_shape_label(shape))
                    .on_hover_text(tip);
            }
        });
    if before != pad.shape {
        if pad.shape == "chamfered" && pad.chamfer.is_none() {
            pad.chamfer = Some(PadChamfer {
                ratio: 0.25,
                corners: ChamferCorners {
                    top_left: true,
                    ..ChamferCorners::default()
                },
            });
        }
        if pad.shape == "custom" && pad.polygon.is_empty() {
            let (hx, hy) = (pad.size.x / 2.0, pad.size.y / 2.0);
            pad.polygon = vec![
                Point::new(-hx, -hy),
                Point::new(hx, -hy),
                Point::new(hx, hy),
                Point::new(-hx, hy),
            ];
        }
    }
    if pad.shape == "roundrect" {
        let response = ui
            .add(crate::number_entry::localized_active(
                egui::DragValue::new(pad.roundrect_ratio.get_or_insert(0.25))
                    .range(0.0..=0.5)
                    .speed(0.01)
                    .prefix(format!("{} ", t!("library.library_editor.corner_ratio"))),
                3,
            ))
            .on_hover_text(t!("library.library_editor.corner_ratio.tip"));
        crate::a11y::name_control(&response, &t!("library.library_editor.corner_ratio"), None);
    } else {
        pad.roundrect_ratio = None;
    }
    if pad.shape == "chamfered" {
        if let Some(chamfer) = &mut pad.chamfer {
            let response = ui
                .add(crate::number_entry::localized_active(
                    egui::DragValue::new(&mut chamfer.ratio)
                        .range(0.01..=0.5)
                        .speed(0.01)
                        .prefix(format!("{} ", t!("library.library_editor.cut_size"))),
                    3,
                ))
                .on_hover_text(t!("library.library_editor.cut_size.tip"));
            crate::a11y::name_control(&response, &t!("library.library_editor.cut_size"), None);
            let corners = &mut chamfer.corners;
            ui.horizontal(|ui| {
                ui.checkbox(
                    &mut corners.top_left,
                    t!("library.library_editor.corner_top_left"),
                )
                .on_hover_text(t!("library.library_editor.corner_top_left.tip"));
                ui.checkbox(
                    &mut corners.top_right,
                    t!("library.library_editor.corner_top_right"),
                )
                .on_hover_text(t!("library.library_editor.corner_top_right.tip"));
            });
            ui.horizontal(|ui| {
                ui.checkbox(
                    &mut corners.bottom_left,
                    t!("library.library_editor.corner_bottom_left"),
                )
                .on_hover_text(t!("library.library_editor.corner_bottom_left.tip"));
                ui.checkbox(
                    &mut corners.bottom_right,
                    t!("library.library_editor.corner_bottom_right"),
                )
                .on_hover_text(t!("library.library_editor.corner_bottom_right.tip"));
            });
            if !corners.any() {
                ui.label(t!("library.library_editor.pick_a_corner"));
            }
        }
    } else {
        pad.chamfer = None;
    }
    if pad.shape == "custom" {
        ui.label(t!("library.library_editor.outline_points"));
        let mut remove = None;
        let count = pad.polygon.len();
        for (i, vertex) in pad
            .polygon
            .iter_mut()
            .enumerate()
            .take(MAX_PAD_POLYGON_VERTICES)
        {
            ui.push_id(("vertex", i), |ui| {
                point(
                    ui,
                    &t!("library.library_editor.point_v0", v0 = i + 1),
                    vertex,
                );
                if count > 3
                    && ui
                        .button(t!("library.library_editor.delete_point"))
                        .on_hover_text(t!("library.library_editor.delete_point.tip"))
                        .clicked()
                {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = remove {
            pad.polygon.remove(i);
        }
        if pad.polygon.len() < MAX_PAD_POLYGON_VERTICES
            && ui
                .button(t!("library.library_editor.add_point"))
                .on_hover_text(t!("library.library_editor.add_point.tip"))
                .clicked()
        {
            let last = pad.polygon.last().copied().unwrap_or_default();
            pad.polygon.push(Point::new(last.x + 0.1, last.y + 0.1));
        }
        if let Some(reason) = pcbcraft_library::custom_polygon_error(pad.size, &pad.polygon) {
            ui.colored_label(Color32::LIGHT_RED, reason);
        }
    } else {
        pad.polygon.clear();
    }
}
fn point(ui: &mut egui::Ui, label: &str, p: &mut Point) {
    ui.horizontal(|ui| {
        ui.label(label);
        let size = label.to_lowercase().contains("size");
        let name = label.to_lowercase();
        let (across, up) = if size {
            (
                format!("How wide the {name} is, in millimetres."),
                format!("How tall the {name} is, in millimetres."),
            )
        } else {
            (
                format!(
                    "Sideways position of the {name} in millimetres, from the drawing's origin."
                ),
                format!(
                    "Up-and-down position of the {name} in millimetres, from the drawing's origin."
                ),
            )
        };
        ui.add(crate::number_entry::localized_active(
            egui::DragValue::new(&mut p.x).speed(0.1).prefix("X "),
            3,
        ))
        .named(&format!("{label} X"))
        .on_hover_text(across);
        ui.add(crate::number_entry::localized_active(
            egui::DragValue::new(&mut p.y).speed(0.1).prefix("Y "),
            3,
        ))
        .named(&format!("{label} Y"))
        .on_hover_text(up);
    });
}
fn text(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add(crate::targets::text_field(value))
            .named(label)
            .on_hover_text(text_field_tip(label));
    });
}
/// What a text box in the symbol and package editors is for, keyed by its
/// label, so each box says what to type and what it changes.
fn text_field_tip(label: &str) -> String {
    if label == t!("library.library_editor.name") {
        t!("library.library_editor.name.tip")
    } else if label == t!("library.library_editor.reference_prefix") {
        t!("library.library_editor.reference_prefix.tip")
    } else if label == t!("library.library_editor.default_value") {
        t!("library.library_editor.default_value.tip")
    } else if label == t!("library.library_editor.description") {
        t!("library.library_editor.description.tip")
    } else if label == t!("library.library_editor.suggested_footprint") {
        t!("library.library_editor.suggested_footprint.tip")
    } else if label == t!("library.library_editor.number") {
        t!("library.library_editor.number.tip")
    } else if label == t!("library.graphic.layer") {
        t!("library.graphic.layer.tip")
    } else if label == t!("library.graphic.fill") {
        t!("library.graphic.fill.tip")
    } else if label == t!("library.graphic.text") {
        t!("library.graphic.text.tip")
    } else {
        t!("library.library_editor.field_default.tip")
    }
}
fn number(ui: &mut egui::Ui, label: &str, value: &mut f64) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add(crate::number_entry::localized_active(
            egui::DragValue::new(value).speed(0.1),
            3,
        ))
        .named(label)
        .on_hover_text(match label {
            "Angle" => "The angle in degrees. Positive turns counter-clockwise.",
            "Length" => "How long the shape is, in millimetres.",
            "Radius" => "The distance from the centre to the edge, in millimetres.",
            "Stroke" => "How thick the outline is, in millimetres. Zero draws the thinnest line.",
            _ => "Type a number for this field. It updates the draft in the editor straight away.",
        });
    });
}
fn graphic(ui: &mut egui::Ui, g: &mut Graphic) {
    match g {
        Graphic::Line {
            start,
            end,
            width,
            layer,
        } => {
            point(ui, &t!("library.graphic.start"), start);
            point(ui, &t!("library.graphic.end"), end);
            number(ui, &t!("library.graphic.stroke"), width);
            text(ui, &t!("library.graphic.layer"), layer);
        }
        Graphic::Rectangle {
            start,
            end,
            width,
            fill,
            layer,
        } => {
            point(ui, &t!("library.graphic.start"), start);
            point(ui, &t!("library.graphic.end"), end);
            number(ui, &t!("library.graphic.stroke"), width);
            text(ui, &t!("library.graphic.fill"), fill);
            text(ui, &t!("library.graphic.layer"), layer);
        }
        Graphic::Circle {
            center,
            radius,
            width,
            fill,
            layer,
        } => {
            point(ui, &t!("library.graphic.center"), center);
            number(ui, &t!("library.graphic.radius"), radius);
            number(ui, &t!("library.graphic.stroke"), width);
            text(ui, &t!("library.graphic.fill"), fill);
            text(ui, &t!("library.graphic.layer"), layer);
        }
        Graphic::Arc {
            start,
            mid,
            end,
            width,
            layer,
        } => {
            point(ui, &t!("library.graphic.start"), start);
            point(ui, &t!("library.graphic.through"), mid);
            point(ui, &t!("library.graphic.end"), end);
            number(ui, &t!("library.graphic.stroke"), width);
            text(ui, &t!("library.graphic.layer"), layer);
        }
        Graphic::Polyline {
            points,
            width,
            fill,
            layer,
        } => {
            for (i, p) in points.iter_mut().enumerate().take(128) {
                point(ui, &t!("library.graphic.vertex_v0", v0 = i + 1), p);
            }
            if points.len() > 128 {
                ui.label(t!("library.graphic.remaining_vertices_are_preserved"));
            }
            if ui
                .button(t!("library.graphic.add_vertex"))
                .on_hover_text(t!("library.graphic.add_vertex.tip"))
                .clicked()
                && points.len() < 128
            {
                points.push(Point::default());
            }
            if ui
                .button(t!("library.graphic.remove_last_vertex"))
                .on_hover_text(t!("library.graphic.remove_last_vertex.tip"))
                .clicked()
                && points.len() > 2
            {
                points.pop();
            }
            number(ui, &t!("library.graphic.stroke"), width);
            text(ui, &t!("library.graphic.fill"), fill);
            text(ui, &t!("library.graphic.layer"), layer);
        }
        Graphic::Text {
            position,
            angle,
            text: content,
            size,
            layer,
        } => {
            point(ui, &t!("library.graphic.position"), position);
            text(ui, &t!("library.graphic.text"), content);
            point(ui, &t!("library.graphic.text_size"), size);
            number(ui, &t!("library.graphic.angle"), angle);
            text(ui, &t!("library.graphic.layer"), layer);
        }
    }
}
fn add_graphic(ui: &mut egui::Ui, layer: &str) -> Option<Graphic> {
    let mut result = None;
    ui.horizontal_wrapped(|ui| {
        ui.label(t!("library.add_graphic.add_drawing"));
        if ui
            .button(t!("library.add_graphic.line"))
            .on_hover_text(t!("library.add_graphic.line.tip"))
            .clicked()
        {
            result = Some(Graphic::Line {
                start: Point::new(-3.0, 0.0),
                end: Point::new(3.0, 0.0),
                width: 0.15,
                layer: layer.into(),
            });
        }
        if ui
            .button(t!("library.add_graphic.rectangle"))
            .on_hover_text(t!("library.add_graphic.rectangle.tip"))
            .clicked()
        {
            result = Some(rectangle(layer));
        }
        if ui
            .button(t!("library.add_graphic.circle"))
            .on_hover_text(t!("library.add_graphic.circle.tip"))
            .clicked()
        {
            result = Some(Graphic::Circle {
                center: Point::default(),
                radius: 2.0,
                width: 0.15,
                fill: "none".into(),
                layer: layer.into(),
            });
        }
        if ui
            .button(t!("library.add_graphic.arc"))
            .on_hover_text(t!("library.add_graphic.arc.tip"))
            .clicked()
        {
            result = Some(Graphic::Arc {
                start: Point::new(-3.0, 0.0),
                mid: Point::new(0.0, -3.0),
                end: Point::new(3.0, 0.0),
                width: 0.15,
                layer: layer.into(),
            });
        }
        if ui
            .button(t!("library.add_graphic.polyline"))
            .on_hover_text(t!("library.add_graphic.polyline.tip"))
            .clicked()
        {
            result = Some(Graphic::Polyline {
                points: vec![
                    Point::new(-2.0, 2.0),
                    Point::new(0.0, -2.0),
                    Point::new(2.0, 2.0),
                ],
                width: 0.15,
                fill: "none".into(),
                layer: layer.into(),
            });
        }
        if ui
            .button(t!("library.add_graphic.text"))
            .on_hover_text(t!("library.add_graphic.text.tip"))
            .clicked()
        {
            result = Some(Graphic::Text {
                position: Point::default(),
                angle: 0.0,
                text: t!("library.add_graphic.label"),
                size: Point::new(1.0, 1.0),
                layer: layer.into(),
            });
        }
    });
    result
}

fn next_pin_number(def: &SymbolDefinition) -> u32 {
    let mut n = 1u32;
    while def.pins.iter().any(|p| p.number == n.to_string()) {
        n += 1;
    }
    n
}

fn next_pad_number(def: &FootprintDefinition) -> u32 {
    let mut n = 1u32;
    while def.pads.iter().any(|p| p.number == n.to_string()) {
        n += 1;
    }
    n
}

fn pin_tip(pin: &SymbolPin) -> Point {
    pin.position.offset(Point::new(
        pin.length * pin.angle.to_radians().cos(),
        -pin.length * pin.angle.to_radians().sin(),
    ))
}

fn dist_segment(pos: Point, a: Point, b: Point) -> f64 {
    point_segment(pos, a, b)
}

impl App {
    fn library_editor_click(&mut self, draft: &mut Draft, pos: Point, zoom: f32) {
        let at = canvas::snap(pos);
        let tool = self.library.editor.tool;
        match tool {
            LibraryTool::Select => {
                self.library.editor.pick = self.library_pick_at(draft, pos, zoom);
                self.library.editor.reset_draw();
            }
            LibraryTool::Pin => {
                if let Draft::Symbol(def) = draft
                    && def.pins.len() < 64
                {
                    let n = next_pin_number(def);
                    def.pins.push(new_pin(
                        &n.to_string(),
                        self.library.preview_unit.max(1),
                        at,
                        0.0,
                    ));
                }
            }
            LibraryTool::Pad => {
                if let Draft::Footprint(def) = draft
                    && def.pads.len() < 64
                {
                    let n = next_pad_number(def);
                    let mut pad = new_pad(&n.to_string(), 0.0);
                    pad.position = at;
                    def.pads.push(pad);
                }
            }
            LibraryTool::Line | LibraryTool::Rectangle | LibraryTool::Circle => {
                match self.library.editor.graphic_start {
                    None => self.library.editor.graphic_start = Some(pos),
                    Some(start) => {
                        let start = canvas::snap(start);
                        let layer = match draft {
                            Draft::Symbol(_) => self.library.editor.symbol_layer(),
                            Draft::Footprint(_) => self.library.editor.footprint_layer(),
                        };
                        let graphic = match tool {
                            LibraryTool::Line => Graphic::Line {
                                start,
                                end: at,
                                width: 0.15,
                                layer: layer.into(),
                            },
                            LibraryTool::Rectangle => Graphic::Rectangle {
                                start,
                                end: at,
                                width: 0.15,
                                fill: "none".into(),
                                layer: layer.into(),
                            },
                            LibraryTool::Circle => Graphic::Circle {
                                center: start,
                                radius: start.distance(at).max(0.25),
                                width: 0.15,
                                fill: "none".into(),
                                layer: layer.into(),
                            },
                            _ => return,
                        };
                        self.library_push_graphic(draft, graphic);
                        self.library.editor.graphic_start = None;
                    }
                }
            }
            LibraryTool::Arc => {
                self.library.editor.arc_points.push(at);
                if self.library.editor.arc_points.len() >= 3 {
                    let layer = match draft {
                        Draft::Symbol(_) => self.library.editor.symbol_layer(),
                        Draft::Footprint(_) => self.library.editor.footprint_layer(),
                    };
                    let points = self.library.editor.arc_points.clone();
                    let graphic = Graphic::Arc {
                        start: points[0],
                        mid: points[1],
                        end: points[2],
                        width: 0.15,
                        layer: layer.into(),
                    };
                    self.library_push_graphic(draft, graphic);
                    self.library.editor.arc_points.clear();
                }
            }
            LibraryTool::Polyline => {
                let closing = self
                    .library
                    .editor
                    .poly_points
                    .first()
                    .is_some_and(|first| pos.distance(*first) < 6.0 / zoom as f64);
                if closing && self.library.editor.poly_points.len() >= 3 {
                    self.library_commit_poly(draft);
                } else if self.library.editor.poly_points.len() >= 64 {
                    self.message =
                        "An outline holds up to 64 corners; click the first point to close it"
                            .into();
                } else {
                    self.library.editor.poly_points.push(at);
                }
            }
        }
    }

    fn library_push_graphic(&mut self, draft: &mut Draft, graphic: Graphic) {
        match draft {
            Draft::Symbol(def) if def.graphics.len() < 256 => {
                def.graphics.push(UnitGraphic {
                    unit: self.library.preview_unit.max(1),
                    body_style: 1,
                    primitive: graphic,
                });
            }
            Draft::Footprint(def) if def.graphics.len() < 256 => {
                def.graphics.push(graphic);
            }
            _ => {}
        }
    }

    fn library_commit_poly(&mut self, draft: &mut Draft) {
        if self.library.editor.poly_points.len() < 3 {
            return;
        }
        let layer = match draft {
            Draft::Symbol(_) => self.library.editor.symbol_layer(),
            Draft::Footprint(_) => self.library.editor.footprint_layer(),
        };
        let graphic = Graphic::Polyline {
            points: self.library.editor.poly_points.clone(),
            width: 0.15,
            fill: "none".into(),
            layer: layer.into(),
        };
        self.library_push_graphic(draft, graphic);
        self.library.editor.poly_points.clear();
    }

    fn library_pick_at(&self, draft: &Draft, pos: Point, zoom: f32) -> Option<LibraryPick> {
        let margin = 6.0 / zoom as f64;
        match draft {
            Draft::Symbol(def) => {
                let unit = self.library.preview_unit.max(1);
                let mut best: Option<(f64, LibraryPick)> = None;
                for (i, pin) in def.pins.iter().enumerate().take(64) {
                    if (pin.unit != unit && !(pin.unit == 0 && unit == 1)) || pin.body_style > 1 {
                        continue;
                    }
                    let tip = pin_tip(pin);
                    let d = dist_segment(pos, pin.position, tip)
                        .min(pos.distance(pin.position))
                        .min(pos.distance(tip));
                    if d <= margin && best.as_ref().map(|(bd, _)| d < *bd).unwrap_or(true) {
                        best = Some((d, LibraryPick::Pin(i)));
                    }
                }
                for (i, g) in def.graphics.iter().enumerate().take(256) {
                    if (g.unit != 0 && g.unit != unit) || g.body_style > 1 {
                        continue;
                    }
                    for edge in g.primitive.path_points().windows(2) {
                        let d = dist_segment(pos, edge[0], edge[1]);
                        if d <= margin && best.as_ref().map(|(bd, _)| d < *bd).unwrap_or(true) {
                            best = Some((d, LibraryPick::SymbolGraphic(i)));
                        }
                    }
                }
                best.map(|(_, p)| p)
            }
            Draft::Footprint(def) => {
                let mut best: Option<(f64, LibraryPick)> = None;
                for (i, pad) in def.pads.iter().enumerate().take(64) {
                    let outline = pcbcraft_doc::Pad {
                        number: pad.number.clone(),
                        offset: pad.position,
                        diameter: pad.size.x.min(pad.size.y),
                        net: None,
                        castellated: None,
                        geometry: Some(pad.clone()),
                    }
                    .outline_points();
                    if outline.len() >= 3
                        && canvas::polygon_contains(&outline, pos)
                        && best.as_ref().map(|(bd, _)| 0.0 < *bd).unwrap_or(true)
                    {
                        best = Some((0.0, LibraryPick::Pad(i)));
                    } else if pos.distance(pad.position) <= margin
                        && best
                            .as_ref()
                            .map(|(bd, _)| pos.distance(pad.position) < *bd)
                            .unwrap_or(true)
                    {
                        best = Some((pos.distance(pad.position), LibraryPick::Pad(i)));
                    }
                }
                for (i, g) in def.graphics.iter().enumerate().take(256) {
                    for edge in g.path_points().windows(2) {
                        let d = dist_segment(pos, edge[0], edge[1]);
                        if d <= margin && best.as_ref().map(|(bd, _)| d < *bd).unwrap_or(true) {
                            best = Some((d, LibraryPick::FootprintGraphic(i)));
                        }
                    }
                }
                best.map(|(_, p)| p)
            }
        }
    }

    fn library_move_pick(draft: &mut Draft, pick: LibraryPick, pos: Point) {
        let at = canvas::snap(pos);
        match (draft, pick) {
            (Draft::Symbol(def), LibraryPick::Pin(i)) => {
                if let Some(pin) = def.pins.get_mut(i) {
                    pin.position = at;
                }
            }
            (Draft::Symbol(def), LibraryPick::SymbolGraphic(i)) => {
                if let Some(g) = def.graphics.get_mut(i) {
                    library_translate_graphic(&mut g.primitive, at);
                }
            }
            (Draft::Footprint(def), LibraryPick::Pad(i)) => {
                if let Some(pad) = def.pads.get_mut(i) {
                    pad.position = at;
                }
            }
            (Draft::Footprint(def), LibraryPick::FootprintGraphic(i)) => {
                if let Some(g) = def.graphics.get_mut(i) {
                    library_translate_graphic(g, at);
                }
            }
            _ => {}
        }
    }

    fn library_delete_pick(draft: &mut Draft, pick: LibraryPick) {
        match (draft, pick) {
            (Draft::Symbol(def), LibraryPick::Pin(i)) if i < def.pins.len() => {
                def.pins.remove(i);
            }
            (Draft::Symbol(def), LibraryPick::SymbolGraphic(i)) if i < def.graphics.len() => {
                def.graphics.remove(i);
            }
            (Draft::Footprint(def), LibraryPick::Pad(i)) if i < def.pads.len() => {
                def.pads.remove(i);
            }
            (Draft::Footprint(def), LibraryPick::FootprintGraphic(i)) if i < def.graphics.len() => {
                def.graphics.remove(i);
            }
            _ => {}
        }
    }

    fn library_rotate_pick(draft: &mut Draft, pick: LibraryPick) {
        match (draft, pick) {
            (Draft::Symbol(def), LibraryPick::Pin(i)) => {
                if let Some(pin) = def.pins.get_mut(i) {
                    pin.angle = (pin.angle + 90.0).rem_euclid(360.0);
                }
            }
            (Draft::Footprint(def), LibraryPick::Pad(i)) => {
                if let Some(pad) = def.pads.get_mut(i) {
                    pad.rotation = (pad.rotation + 90.0).rem_euclid(360.0);
                }
            }
            _ => {}
        }
    }

    fn library_pick_position(draft: &Draft, pick: LibraryPick) -> Option<Point> {
        match (draft, pick) {
            (Draft::Symbol(def), LibraryPick::Pin(i)) => def.pins.get(i).map(|p| p.position),
            (Draft::Footprint(def), LibraryPick::Pad(i)) => def.pads.get(i).map(|p| p.position),
            _ => None,
        }
    }

    pub(super) fn library_editor_canvas(&mut self, ui: &mut egui::Ui, draft: &mut Draft) {
        let symbol = matches!(draft, Draft::Symbol(_));
        ui.horizontal_wrapped(|ui| {
            if ui
                .selectable_label(self.library.editor.tool == LibraryTool::Select, "Select")
                .on_hover_text(
                    "Pick pins, pads, or graphics. Drag to move. R rotates; Delete removes.",
                )
                .clicked()
            {
                self.library.editor.tool = LibraryTool::Select;
                self.library.editor.reset_draw();
            }
            if symbol {
                if ui
                    .selectable_label(self.library.editor.tool == LibraryTool::Pin, "Pin")
                    .on_hover_text("Click the canvas to place a pin on the 0.5 mm grid.")
                    .clicked()
                {
                    self.library.editor.tool = LibraryTool::Pin;
                    self.library.editor.reset_draw();
                }
            } else if ui
                .selectable_label(self.library.editor.tool == LibraryTool::Pad, "Pad")
                .on_hover_text("Click the canvas to place a pad on the 0.5 mm grid.")
                .clicked()
            {
                self.library.editor.tool = LibraryTool::Pad;
                self.library.editor.reset_draw();
            }
            for (tool, label, tip) in [
                (
                    LibraryTool::Line,
                    "Draw line",
                    "First click sets the start; second click finishes the segment.",
                ),
                (
                    LibraryTool::Rectangle,
                    "Draw rectangle",
                    "First click sets one corner; second click sets the opposite corner.",
                ),
                (
                    LibraryTool::Circle,
                    "Draw circle",
                    "First click sets the center; second click sets a point on the rim.",
                ),
                (
                    LibraryTool::Arc,
                    "Draw arc",
                    "Three clicks set the start, a point on the arc, and the end.",
                ),
                (
                    LibraryTool::Polyline,
                    "Draw polyline",
                    "Each click adds a corner; click the first point or press Enter to close.",
                ),
            ] {
                if ui
                    .selectable_label(self.library.editor.tool == tool, label)
                    .on_hover_text(tip)
                    .clicked()
                {
                    self.library.editor.tool = tool;
                    self.library.editor.reset_draw();
                }
            }
            if ui
                .button("Fit")
                .on_hover_text("Zoom the canvas so the whole definition fits.")
                .clicked()
            {
                self.library.editor.needs_fit = true;
            }
        });
        if let Draft::Symbol(def) = draft {
            let units = pcbcraft_doc::symbol_units(def).unwrap_or_default();
            if units.len() > 1 {
                ui.horizontal_wrapped(|ui| {
                    ui.label("Preview unit");
                    for unit in units {
                        ui.selectable_value(&mut self.library.preview_unit, unit, unit.to_string())
                            .on_hover_text("Draw and edit this unit on the canvas.");
                    }
                });
            }
        }
        let hint = match self.library.editor.tool {
            LibraryTool::Select => "Select · drag to move · R rotate · Delete remove",
            LibraryTool::Pin => "Pin · click to place",
            LibraryTool::Pad => "Pad · click to place",
            LibraryTool::Line => "Line · two clicks",
            LibraryTool::Rectangle => "Rectangle · two clicks",
            LibraryTool::Circle => "Circle · two clicks",
            LibraryTool::Arc => "Arc · three clicks",
            LibraryTool::Polyline => "Polyline · Enter or click first point to close",
        };
        ui.label(hint);
        let height = 420.0_f32.max(ui.available_height());
        let (response, painter) = ui.allocate_painter(
            Vec2::new(ui.available_width(), height),
            Sense::click_and_drag().union(Sense::hover()),
        );
        self.library.editor.canvas_rect = Some(response.rect);
        let mut camera = self.library.editor.camera;
        if self.library.editor.needs_fit
            && response.rect.width() > 40.0
            && response.rect.height() > 40.0
        {
            let (symbol, footprint) = match &draft {
                Draft::Symbol(s) => (Some(s), None),
                Draft::Footprint(f) => (None, Some(f)),
            };
            let paths = super::library::library_geometry_paths(
                symbol,
                footprint,
                self.library.preview_unit.max(1),
            );
            let (lo, hi) = super::library::library_paths_bounds(&paths);
            camera = canvas::fit_camera(lo, hi, (response.rect.width(), response.rect.height()));
            self.library.editor.needs_fit = false;
        }
        if response.hovered() {
            let scroll = motion::wheel_zoom_delta(ui, self.preferences.reduced_motion);
            if scroll.abs() > 0.01
                && let Some(mouse) = ui.input(|i| i.pointer.hover_pos())
            {
                let before = camera.world(mouse, response.rect);
                camera.zoom = (camera.zoom * (scroll * 0.003).exp()).clamp(0.02, 100.0);
                let after = camera.world(mouse, response.rect);
                camera.center = camera
                    .center
                    .offset(Point::new(before.x - after.x, before.y - after.y));
            }
        }
        if response.dragged_by(egui::PointerButton::Middle) {
            let d = ui.input(|i| i.pointer.delta());
            camera.center = camera.center.offset(Point::new(
                -d.x as f64 / camera.zoom as f64,
                -d.y as f64 / camera.zoom as f64,
            ));
        }
        self.library.editor.camera = camera;
        let cursor = response.hover_pos().map(|p| camera.world(p, response.rect));
        let snapped = cursor.map(canvas::snap);
        if response.hovered() {
            if ui.input(|i| i.key_pressed(egui::Key::Enter))
                && self.library.editor.tool == LibraryTool::Polyline
                && self.library.editor.poly_points.len() >= 3
            {
                self.library_commit_poly(draft);
            }
            if ui.input(|i| i.key_pressed(egui::Key::Delete))
                && let Some(pick) = self.library.editor.pick
            {
                Self::library_delete_pick(draft, pick);
                self.library.editor.pick = None;
            }
            if ui.input(|i| i.key_pressed(egui::Key::R))
                && let Some(pick) = self.library.editor.pick
            {
                Self::library_rotate_pick(draft, pick);
            }
        }
        if response.drag_started_by(egui::PointerButton::Primary)
            && self.library.editor.tool == LibraryTool::Select
            && let Some(origin) = ui.input(|i| i.pointer.press_origin())
        {
            let pos = camera.world(origin, response.rect);
            let pick = self
                .library_pick_at(draft, pos, camera.zoom)
                .or(self.library.editor.pick);
            self.library.editor.pick = pick;
            if let Some(pick) = pick
                && matches!(pick, LibraryPick::Pin(_) | LibraryPick::Pad(_))
                && let Some(anchor) = Self::library_pick_position(draft, pick)
            {
                self.library.editor.drag_anchor =
                    Some((pick, Point::new(anchor.x - pos.x, anchor.y - pos.y)));
            }
        }
        if response.dragged_by(egui::PointerButton::Primary)
            && self.library.editor.tool == LibraryTool::Select
            && let (Some((pick, offset)), Some(pos)) = (self.library.editor.drag_anchor, cursor)
            && matches!(pick, LibraryPick::Pin(_) | LibraryPick::Pad(_))
        {
            Self::library_move_pick(draft, pick, pos.offset(offset));
        }
        if response.drag_stopped_by(egui::PointerButton::Primary) {
            self.library.editor.drag_anchor = None;
        }
        if response.clicked()
            && let Some(pos) = cursor
            && self.library.editor.tool != LibraryTool::Select
        {
            self.library_editor_click(draft, pos, camera.zoom);
        }
        if response.clicked()
            && self.library.editor.tool == LibraryTool::Select
            && let Some(pos) = cursor
        {
            self.library.editor.pick = self.library_pick_at(draft, pos, camera.zoom);
            self.library.editor.reset_draw();
        }
        painter.rect_filled(response.rect, 6.0, self.surface());
        let accent = self.accent();
        let select_color = Color32::from_rgb(255, 214, 102);
        match draft {
            Draft::Symbol(def) => {
                let unit = self.library.preview_unit.max(1);
                for g in def
                    .graphics
                    .iter()
                    .filter(|g| (g.unit == 0 || g.unit == unit) && g.body_style <= 1)
                {
                    self.paint_graphic(
                        &painter,
                        response.rect,
                        camera,
                        &g.primitive,
                        (Point::default(), 0.0),
                        accent,
                    );
                }
                for (i, pin) in def.pins.iter().enumerate().take(64) {
                    if (pin.unit != unit && !(pin.unit == 0 && unit == 1)) || pin.body_style > 1 {
                        continue;
                    }
                    let tip = pin_tip(pin);
                    let stroke = if self.library.editor.pick == Some(LibraryPick::Pin(i)) {
                        Stroke::new(2.5, select_color)
                    } else {
                        Stroke::new(1.5, accent)
                    };
                    painter.line_segment(
                        [
                            camera.screen(pin.position, response.rect),
                            camera.screen(tip, response.rect),
                        ],
                        stroke,
                    );
                    painter.circle_filled(camera.screen(pin.position, response.rect), 3.0, accent);
                }
            }
            Draft::Footprint(def) => {
                for g in def.graphics.iter() {
                    self.paint_graphic(
                        &painter,
                        response.rect,
                        camera,
                        g,
                        (Point::default(), 0.0),
                        accent,
                    );
                }
                for (i, pad) in def.pads.iter().enumerate().take(64) {
                    let doc_pad = pcbcraft_doc::Pad {
                        number: pad.number.clone(),
                        offset: pad.position,
                        diameter: pad.size.x.min(pad.size.y),
                        net: None,
                        castellated: None,
                        geometry: Some(pad.clone()),
                    };
                    let outline = doc_pad.outline_points();
                    let color = if self.library.editor.pick == Some(LibraryPick::Pad(i)) {
                        select_color
                    } else {
                        accent
                    };
                    if outline.len() >= 3 {
                        let pts: Vec<Pos2> = outline
                            .iter()
                            .map(|p| camera.screen(*p, response.rect))
                            .collect();
                        painter.add(egui::Shape::closed_line(pts, Stroke::new(1.5, color)));
                    }
                }
            }
        }
        if let Some(snapped) = snapped {
            let stroke = Stroke::new(1.0, accent);
            match self.library.editor.tool {
                LibraryTool::Line | LibraryTool::Rectangle | LibraryTool::Circle => {
                    if let Some(start) = self.library.editor.graphic_start {
                        let a = camera.screen(start, response.rect);
                        let b = camera.screen(snapped, response.rect);
                        if self.library.editor.tool == LibraryTool::Line {
                            painter.line_segment([a, b], stroke);
                        } else if self.library.editor.tool == LibraryTool::Rectangle {
                            painter.rect_stroke(
                                egui::Rect::from_min_max(a, b),
                                0.0,
                                stroke,
                                egui::StrokeKind::Inside,
                            );
                        } else if self.library.editor.tool == LibraryTool::Circle {
                            let radius = (b - a).length().max(1.0);
                            painter.circle_stroke(a, radius, stroke);
                        }
                    }
                }
                LibraryTool::Polyline => {
                    let mut pts: Vec<Pos2> = self
                        .library
                        .editor
                        .poly_points
                        .iter()
                        .map(|pt| camera.screen(*pt, response.rect))
                        .collect();
                    pts.push(camera.screen(snapped, response.rect));
                    if pts.len() >= 2 {
                        painter.add(egui::Shape::line(pts, stroke));
                    }
                }
                LibraryTool::Arc => {
                    let mut pts = self.library.editor.arc_points.clone();
                    pts.push(snapped);
                    if pts.len() >= 2 {
                        let screen: Vec<Pos2> = pts
                            .iter()
                            .map(|p| camera.screen(*p, response.rect))
                            .collect();
                        painter.add(egui::Shape::line(screen, stroke));
                    }
                }
                _ => {}
            }
        }
    }
}

fn library_translate_graphic(graphic: &mut Graphic, target: Point) {
    let anchor = graphic_anchor(graphic);
    let delta = Point::new(target.x - anchor.x, target.y - anchor.y);
    graphic_translate(graphic, delta);
}

fn graphic_anchor(graphic: &Graphic) -> Point {
    match graphic {
        Graphic::Line { start, .. } => *start,
        Graphic::Rectangle { start, .. } => *start,
        Graphic::Circle { center, .. } => *center,
        Graphic::Arc { start, .. } => *start,
        Graphic::Polyline { points, .. } => points.first().copied().unwrap_or_default(),
        Graphic::Text { position, .. } => *position,
    }
}

fn graphic_translate(graphic: &mut Graphic, delta: Point) {
    match graphic {
        Graphic::Line { start, end, .. } => {
            *start = start.offset(delta);
            *end = end.offset(delta);
        }
        Graphic::Rectangle { start, end, .. } => {
            *start = start.offset(delta);
            *end = end.offset(delta);
        }
        Graphic::Circle { center, .. } => *center = center.offset(delta),
        Graphic::Arc {
            start, mid, end, ..
        } => {
            *start = start.offset(delta);
            *mid = mid.offset(delta);
            *end = end.offset(delta);
        }
        Graphic::Polyline { points, .. } => {
            for p in points {
                *p = p.offset(delta);
            }
        }
        Graphic::Text { position, .. } => *position = position.offset(delta),
    }
}

impl App {
    pub(super) fn library_workspaces(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.selectable_value(
                &mut self.library.editor.mode,
                0,
                t!("library.library_workspaces.library"),
            )
            .on_hover_text(t!("library.library_workspaces.library.tip"));
            ui.selectable_value(
                &mut self.library.editor.mode,
                2,
                t!("library.library_workspaces.editor"),
            )
            .on_hover_text(t!("library.library_workspaces.editor.tip"));
            if ui
                .button(t!("library.library_workspaces.new_symbol"))
                .on_hover_text(t!("library.library_workspaces.new_symbol.tip"))
                .clicked()
            {
                self.library.editor.draft = Some(Draft::Symbol(new_symbol()));
                self.library.editor.mode = 2;
                self.library.editor.notice = None;
                self.library.editor.needs_fit = true;
            }
            if ui
                .button(t!("library.library_workspaces.new_footprint"))
                .on_hover_text(t!("library.library_workspaces.new_footprint.tip"))
                .clicked()
            {
                self.library.editor.draft = Some(Draft::Footprint(new_footprint()));
                self.library.editor.mode = 2;
                self.library.editor.notice = None;
                self.library.editor.needs_fit = true;
            }
            if ui
                .button(t!("library.library_workspaces.generate_footprint"))
                .on_hover_text(t!("library.library_workspaces.generate_footprint.tip"))
                .clicked()
            {
                self.library.editor.wizard.open = true;
                self.library.editor.mode = 2;
            }
        });
        ui.separator();
        match self.library.editor.mode {
            2 => self.library_editor(ui),
            _ => {
                self.library.editor.mode = 0;
                self.unified_library(ui)
            }
        }
    }
    /// One library page: every collection from every added or installed
    /// source, then the definitions this project can edit, in one scroll.
    pub(super) fn unified_library(&mut self, ui: &mut egui::Ui) {
        self.collection_browser(ui);
        ui.separator();
        self.project_library(ui);
    }
    fn project_library(&mut self, ui: &mut egui::Ui) {
        ui.heading(t!("library.project_library.this_project_s_definitions"));
        ui.label(t!("library.project_library.saved_here_live_with_the"));
        ui.horizontal(|ui| {
            ui.label(t!("library.project_library.collection_file"));
            ui.add(crate::targets::text_field(&mut self.library.editor.path))
                .named(&t!("library.project_library.collection_file"));
            if ui
                .button(t!("library.project_library.save_library"))
                .on_hover_text(t!("library.project_library.save_library.tip"))
                .clicked()
            {
                self.tool_call("library.save", json!({"path":self.library.editor.path}));
            }
            if ui
                .button(t!("library.project_library.load_library"))
                .on_hover_text(t!("library.project_library.load_library.tip"))
                .clicked()
            {
                self.run("library.load", json!({"path":self.library.editor.path}));
            }
        });
        ui.separator();
        ui.heading(t!("library.project_library.symbols"));
        for def in self.project.library.symbols.clone() {
            ui.horizontal_wrapped(|ui| {
                ui.strong(&def.name);
                ui.label(t!(
                    "library.project_library.len_physical_pins",
                    len = def.pins.len()
                ));
                if ui
                    .button(t!("library.project_library.edit"))
                    .on_hover_text(t!("library.project_library.edit.tip"))
                    .clicked()
                {
                    self.library.editor.draft = Some(Draft::Symbol(def.clone()));
                    self.library.editor.mode = 2;
                    self.library.editor.notice = None;
                    self.library.editor.needs_fit = true;
                }
                let supported = pcbcraft_doc::library_component(
                    def.clone(),
                    "project".into(),
                    0,
                    "preview".into(),
                    Point::default(),
                    None,
                )
                .is_ok();
                if ui
                    .add_enabled(
                        supported,
                        egui::Button::new(t!("library.project_library.place")),
                    )
                    .clicked()
                {
                    self.change_tab(Tab::Schematic);
                    self.tool = Tool::CustomPlace(def.name.clone(), Box::new(def.clone()));
                }
                if ui
                    .button(t!("library.project_library.remove"))
                    .on_hover_text(t!("library.project_library.remove.tip"))
                    .clicked()
                {
                    self.run("library.symbol.remove", json!({"name":def.name}));
                }
            });
        }
        ui.separator();
        ui.heading(t!("library.project_library.footprints"));
        self.refresh_footprint_quality();
        for def in self.project.library.footprints.clone() {
            ui.horizontal_wrapped(|ui| {
                ui.strong(&def.name);
                ui.label(t!("library.project_library.len_pads", len = def.pads.len()));
                self.quality_badge(ui, &def.name);
                if ui
                    .button(t!("library.project_library.edit"))
                    .on_hover_text(t!("library.project_library.edit.tip"))
                    .clicked()
                {
                    self.library.editor.draft = Some(Draft::Footprint(def.clone()));
                    self.library.editor.mode = 2;
                    self.library.editor.notice = None;
                    self.library.editor.needs_fit = true;
                }
                let selected = self.selected.and_then(|id| self.project.component(id));
                let matching = selected.is_some_and(|c| {
                    c.pins
                        .iter()
                        .map(|p| p.number.as_str())
                        .collect::<std::collections::BTreeSet<_>>()
                        == def
                            .pads
                            .iter()
                            .map(|p| p.number.as_str())
                            .collect::<std::collections::BTreeSet<_>>()
                });
                if ui
                    .add_enabled(
                        matching && pcbcraft_doc::validate_footprint_support(&def).is_ok(),
                        egui::Button::new(t!("library.project_library.assign_to_selection")),
                    )
                    .clicked()
                    && let Some(id) = selected.map(|c| c.id)
                {
                    self.run("footprint.assign_custom", json!({"id":id,"name":def.name}));
                }
                if ui
                    .button(t!("library.project_library.remove"))
                    .on_hover_text(t!("library.project_library.remove.tip"))
                    .clicked()
                {
                    self.run("library.footprint.remove", json!({"name":def.name}));
                }
            });
        }
    }
    pub(super) fn library_editor(&mut self, ui: &mut egui::Ui) {
        self.footprint_wizard(ui);
        let Some(mut draft) = self.library.editor.draft.take() else {
            ui.heading(t!("library.library_editor.library_editor"));
            ui.label(t!("library.library_editor.create_a_symbol_or_footprint"));
            return;
        };
        let mut save = None;
        ui.label(t!(
            "library.library_editor.dimensions_are_millimetres_angles_are"
        ));
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_min_width(340.0);
                ui.set_max_width(400.0);
                match &mut draft {
            Draft::Symbol(def) => {
                ui.heading(t!("library.library_editor.symbol_editor"));
                text(ui, &t!("library.library_editor.name"), &mut def.name);
                text(ui, &t!("library.library_editor.reference_prefix"), &mut def.reference);
                text(ui, &t!("library.library_editor.default_value"), &mut def.value);
                text(ui, &t!("library.library_editor.description"), &mut def.description);
                text(ui, &t!("library.library_editor.suggested_footprint"), &mut def.footprint);
                ui.horizontal(|ui| {
                    ui.label(t!("library.library_editor.set"))
                        .on_hover_text(t!("library.library_editor.set.tip"));
                    ui.add(crate::targets::text_field(&mut def.set))
                        .named("Set name")
                        .on_hover_text(t!("library.library_editor.set_field.tip"));
                });
                if let Err(e) = def.validate() {
                    ui.colored_label(Color32::LIGHT_RED, e.to_string());
                }
                if !def.unsupported.is_empty() {
                    ui.label(t!("library.library_editor.preserved_unsupported_features_v0", v0 = def.unsupported.join(", ")));
                }
                if ui
                    .button(t!("library.library_editor.save_symbol_to_project_library"))
                    .on_hover_text(t!("library.library_editor.save_symbol_to_project_library.tip"))
                    .clicked()
                {
                    save = Some(("library.symbol.upsert", json!({"definition":def})));
                }
                ui.separator();
                ui.heading(t!("library.library_editor.pins"));
                if ui
                    .button(t!("library.library_editor.duplicate_preview_unit"))
                    .on_hover_text(t!("library.library_editor.duplicate_preview_unit.tip"))
                    .clicked()
                {
                    let unit = def
                        .pins
                        .iter()
                        .map(|p| p.unit)
                        .chain(def.graphics.iter().map(|g| g.unit))
                        .max()
                        .unwrap_or(1)
                        + 1;
                    let source = self.library.preview_unit.max(1);
                    if unit <= 32 {
                        let mut pins: Vec<_> = def
                            .pins
                            .iter()
                            .filter(|p| p.unit == source)
                            .cloned()
                            .collect();
                        let mut numbers: std::collections::BTreeSet<_> =
                            def.pins.iter().map(|p| p.number.clone()).collect();
                        for pin in &mut pins {
                            let mut n = 1;
                            while numbers.contains(&n.to_string()) {
                                n += 1;
                            }
                            pin.number = n.to_string();
                            numbers.insert(pin.number.clone());
                            pin.unit = unit;
                        }
                        def.pins.extend(pins);
                        let graphics: Vec<_> = def
                            .graphics
                            .iter()
                            .filter(|g| g.unit == source)
                            .cloned()
                            .map(|mut g| {
                                g.unit = unit;
                                g
                            })
                            .collect();
                        def.graphics.extend(graphics);
                        self.library.preview_unit = unit;
                    }
                }
                if ui
                    .button(t!("library.library_editor.add_pin"))
                    .on_hover_text(t!("library.library_editor.add_pin.tip"))
                    .clicked()
                    && def.pins.len() < 64
                {
                    let mut n = 1;
                    while def.pins.iter().any(|p| p.number == n.to_string()) {
                        n += 1;
                    }
                    def.pins.push(new_pin(
                        &n.to_string(),
                        self.library.preview_unit.max(1),
                        Point::new(-5.0, 0.0),
                        0.0,
                    ));
                }
                let mut remove = None;
                for (i, pin) in def.pins.iter_mut().enumerate().take(64) {
                    ui.push_id(i, |ui| {
                        ui.collapsing(t!("library.library_editor.pin_number_unit_unit", number = pin.number, unit = pin.unit), |ui| {
                            text(ui, &t!("library.library_editor.number"), &mut pin.number);
                            text(ui, &t!("library.library_editor.name"), &mut pin.name);
                            point(ui, &t!("library.library_editor.connection"), &mut pin.position);
                            number(ui, &t!("library.library_editor.angle"), &mut pin.angle);
                            number(ui, &t!("library.library_editor.length"), &mut pin.length);
                            ui.add(
                                egui::DragValue::new(&mut pin.unit)
                                    .range(1..=32)
                                    .prefix(format!("{} ", t!("library.library_editor.unit"))),
                            ).named("Pin unit");
                            egui::ComboBox::from_id_salt("pin-type")
                                .selected_text(&pin.electrical_type)
                                .show_ui(ui, |ui| {
                                    for t in [
                                        "passive",
                                        "input",
                                        "output",
                                        "bidirectional",
                                        "tri_state",
                                        "power_in",
                                        "power_out",
                                        "open_collector",
                                        "open_emitter",
                                        "no_connect",
                                    ] {
                                        ui.selectable_value(&mut pin.electrical_type, t.into(), t);
                                    }
                                }).response.named("Pin type");
                            if ui
                                .button(t!("library.library_editor.delete_pin"))
                                .on_hover_text(t!("library.library_editor.delete_pin.tip"))
                                .clicked()
                            {
                                remove = Some(i);
                            }
                        })
.header_response
.on_hover_text(t!("library.library_editor.show_or_hide_the_settings.tip"));
                    });
                }
                if let Some(i) = remove {
                    def.pins.remove(i);
                }
                ui.separator();
                ui.heading(t!("library.library_editor.drawing"));
                if let Some(primitive) = add_graphic(ui, "") {
                    def.graphics.push(UnitGraphic {
                        unit: self.library.preview_unit.max(1),
                        body_style: 1,
                        primitive,
                    });
                }
                let mut remove = None;
                for (i, g) in def.graphics.iter_mut().enumerate().take(256) {
                    ui.push_id(("symbol-graphic", i), |ui| {
                        ui.collapsing(t!("library.library_editor.graphic_v0_unit_unit", v0 = i + 1, unit = g.unit), |ui| {
                            ui.add(
                                egui::DragValue::new(&mut g.unit)
                                    .range(0..=32)
                                    .prefix(format!("{} ", t!("library.library_editor.unit_0_shared"))),
                            ).named("Graphic unit");
                            graphic(ui, &mut g.primitive);
                            if ui
                                .button(t!("library.library_editor.delete_graphic"))
                                .on_hover_text(t!("library.library_editor.delete_graphic.tip"))
                                .clicked()
                            {
                                remove = Some(i);
                            }
                        })
.header_response
.on_hover_text(t!("library.library_editor.show_or_hide_the_settings2.tip"));
                    });
                }
                if let Some(i) = remove {
                    def.graphics.remove(i);
                }
            }
            Draft::Footprint(def) => {
                ui.heading(t!("library.library_editor.footprint_editor"));
                text(ui, &t!("library.library_editor.name"), &mut def.name);
                text(ui, &t!("library.library_editor.description"), &mut def.description);
                if let Err(e) = def.validate() {
                    ui.colored_label(Color32::LIGHT_RED, e.to_string());
                }
                if let Err(e) = pcbcraft_doc::validate_footprint_support(def) {
                    ui.label(e);
                }
                if ui
                    .button(t!("library.library_editor.save_footprint_to_project_library"))
                    .on_hover_text(t!("library.library_editor.save_footprint_to_project_library.tip"))
                    .clicked()
                {
                    save = Some(("library.footprint.upsert", json!({"definition":def})));
                }
                ui.separator();
                ui.heading(t!("library.library_editor.pads"));
                if ui
                    .button(t!("library.library_editor.add_pad"))
                    .on_hover_text(t!("library.library_editor.add_pad.tip"))
                    .clicked()
                    && def.pads.len() < PAD_LIST_LIMIT
                {
                    let mut n = 1;
                    while def.pads.iter().any(|p| p.number == n.to_string()) {
                        n += 1;
                    }
                    def.pads.push(new_pad(&n.to_string(), 0.0));
                }
                let mut remove = None;
                for (i, pad) in def.pads.iter_mut().enumerate().take(PAD_LIST_LIMIT) {
                    ui.push_id(("pad", i), |ui| {
                        ui.collapsing(t!("library.library_editor.pad_number_pad_type", number = pad.number, pad_type = pad.pad_type), |ui| {
                            text(ui, &t!("library.library_editor.number"), &mut pad.number);
                            point(ui, &t!("library.library_editor.position"), &mut pad.position);
                            point(ui, &t!("library.library_editor.copper_size"), &mut pad.size);
                            number(ui, &t!("library.library_editor.angle"), &mut pad.rotation);
                            let before = pad.pad_type.clone();
                            egui::ComboBox::from_id_salt("pad-type")
                                .selected_text(&pad.pad_type)
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(
                                        &mut pad.pad_type,
                                        "smd".into(),
                                        t!("library.library_editor.front_smd"),
                                    )
                                    .on_hover_text(t!("library.library_editor.front_smd.tip"));
                                    ui.selectable_value(
                                        &mut pad.pad_type,
                                        "thru_hole".into(),
                                        t!("library.library_editor.plated_through_hole"),
                                    )
                                    .on_hover_text(t!("library.library_editor.plated_through_hole.tip"));
                                    ui.selectable_value(
                                        &mut pad.pad_type,
                                        "unplated".into(),
                                        t!("library.library_editor.unplated_hole"),
                                    )
                                    .on_hover_text(
                                        t!("library.library_editor.unplated_hole.tip"),
                                    );
                                }).response.named("Pad type");
                            if before != pad.pad_type {
                                if pad.pad_type == "unplated" {
                                    apply_unplated_defaults(pad);
                                } else if pad.pad_type == "thru_hole" {
                                    pad.layers = vec!["*.Cu".into(), "*.Mask".into()];
                                    pad.drill = Some(Drill {
                                        size: Point::new(0.8, 0.8),
                                        offset: Point::default(),
                                    });
                                } else {
                                    pad.layers =
                                        vec!["F.Cu".into(), "F.Paste".into(), "F.Mask".into()];
                                    pad.drill = None;
                                }
                            }
                            pad_shape_controls(ui, pad);
                            if let Some(drill) = &mut pad.drill {
                                point(ui, &t!("library.library_editor.drill_size_oval_if_unequal"), &mut drill.size);
                                point(ui, &t!("library.library_editor.drill_offset"), &mut drill.offset);
                            }
                            ui.label(t!("library.library_editor.layers_v0", v0 = pad.layers.join(", ")));
                            if ui
                                .button(t!("library.library_editor.delete_pad"))
                                .on_hover_text(t!("library.library_editor.delete_pad.tip"))
                                .clicked()
                            {
                                remove = Some(i);
                            }
                        })
.header_response
.on_hover_text(t!("library.library_editor.show_or_hide_the_settings3.tip"));
                    });
                }
                if let Some(i) = remove {
                    def.pads.remove(i);
                }
                ui.separator();
                ui.heading(t!("library.library_editor.drawing"));
                if let Some(g) = add_graphic(ui, "F.SilkS") {
                    def.graphics.push(g);
                }
                let mut remove = None;
                for (i, g) in def.graphics.iter_mut().enumerate().take(256) {
                    ui.push_id(("footprint-graphic", i), |ui| {
                        ui.collapsing(t!("library.library_editor.graphic_v0", v0 = i + 1), |ui| {
                            graphic(ui, g);
                            if ui
                                .button(t!("library.library_editor.delete_graphic"))
                                .on_hover_text(t!("library.library_editor.delete_graphic.tip"))
                                .clicked()
                            {
                                remove = Some(i);
                            }
                        })
.header_response
.on_hover_text(t!("library.library_editor.show_or_hide_the_settings2.tip"));
                    });
                }
                if let Some(i) = remove {
                    def.graphics.remove(i);
                }
            }
                }
            });
            ui.add_space(12.0);
            ui.vertical(|ui| {
                let room = ui.available_width().max(240.0);
                ui.set_min_width(room);
                ui.set_max_width(room);
                self.library_editor_canvas(ui, &mut draft);
            });
        });
        self.library.editor.draft = Some(draft);
        if let Some((command, mut params)) = save {
            if let Some(notice) = &self.library.editor.notice {
                params["notice"] = json!(format!(
                    "Derived entry saved as {}.\n{}",
                    params
                        .pointer("/definition/name")
                        .and_then(Value::as_str)
                        .unwrap_or("entry"),
                    notice
                ));
            }
            self.run(command, params);
        }
    }
}

/// Turn a library pad into an unplated mechanical hole: no number, no copper
/// layers, zero copper size, and a drill that alone defines the hole.
pub(crate) fn apply_unplated_defaults(pad: &mut pcbcraft_library::FootprintPad) {
    pad.number.clear();
    pad.layers.clear();
    let size = pad.drill.as_ref().map_or(Point::new(1.0, 1.0), |d| d.size);
    pad.size = Point::default();
    pad.drill = Some(Drill {
        size,
        offset: Point::default(),
    });
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    pub(crate) fn frame(
        ctx: &egui::Context,
        app: &mut App,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        crate::e2e::support::frame(ctx, app, events, egui::Modifiers::NONE)
    }
    #[test]
    fn editor_button_commits_one_undoable_edit_and_retains_original_nodes() -> Result<(), String> {
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = App::create(
            shared.clone(),
            Preferences::default(),
            None,
            Some("symbol-editor"),
        );
        app.refresh();
        let mut definition = new_symbol();
        definition.raw_source = "(original unknown metadata)".into();
        definition.unsupported = vec!["unknown original expression".into()];
        app.library.editor.draft = Some(Draft::Symbol(definition.clone()));
        app.library.editor.notice = Some("Original source and data license retained".into());
        let ctx = egui::Context::default();
        let output = frame(&ctx, &mut app, vec![]);
        assert_eq!(output.viewport_output.len(), 1);
        let pos = output
            .shapes
            .iter()
            .find_map(|s| {
                if let egui::Shape::Text(t) = &s.shape {
                    if t.galley.text() == "Save symbol to project library" {
                        Some(t.pos + t.galley.size() / 2.0)
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .ok_or("save button not rendered")?;
        for pressed in [true, false] {
            frame(
                &ctx,
                &mut app,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        let session = shared.lock().map_err(|_| "session unavailable")?;
        assert_eq!(session.project.library.symbols, vec![definition]);
        assert_eq!(session.project.library.notices.len(), 1);
        assert_eq!(session.revision, 1);
        drop(session);
        app.tool_call("undo", json!({}));
        assert!(app.project.library.symbols.is_empty());
        app.tool_call("redo", json!({}));
        assert_eq!(app.project.library.symbols.len(), 1);
        Ok(())
    }
    /// Opens a page the way a person does now that the pages live in menus:
    /// Settings is Edit > Preferences, Rules and Checks are in View.
    pub(crate) fn open_page(ctx: &egui::Context, app: &mut App, page: &str) -> Result<(), String> {
        let (menu, item) = match page {
            "Settings" => ("Edit", "Preferences"),
            "Rules" => ("View", "Rules"),
            "Checks" => ("View", "Checks"),
            other => return Err(format!("{other} is not a page in a menu")),
        };
        tap(ctx, app, menu)?;
        frame(ctx, app, vec![]);
        tap(ctx, app, item)
    }
    /// Find the button whose label reads exactly `label`, scrolling the page
    /// down and then up if needed, and click it like a person would.
    pub(crate) fn tap(ctx: &egui::Context, app: &mut App, label: &str) -> Result<(), String> {
        // Two quiet passes first: hit-testing always trails the painter by a
        // frame, and a scroll area only grows its scrollbar on the second
        // pass, which shifts every row a hair.
        frame(ctx, app, vec![]);
        frame(ctx, app, vec![]);
        // Where to park the pointer when a label sits off screen: the middle
        // page low and high, the settings column, and the left dock. A spot
        // that nothing moves under (a text field swallowing the wheel, or
        // the canvas zooming instead) is given up on after two still frames.
        let lanes = [
            egui::pos2(720.0, 750.0),
            egui::pos2(720.0, 400.0),
            egui::pos2(300.0, 500.0),
            egui::pos2(150.0, 500.0),
            egui::pos2(1180.0, 400.0),
        ];
        // Later shapes sit on top of earlier ones, so the last visible
        // copy of a label is the one a real click would land on.
        let find = |output: &egui::FullOutput| -> Option<egui::Pos2> {
            let mut found = None;
            for shape in &output.shapes {
                if let egui::Shape::Text(text) = &shape.shape
                    && text.galley.text() == label
                {
                    let center = text.pos + text.galley.size() / 2.0;
                    if shape.clip_rect.contains(center) {
                        found = Some(center);
                    }
                }
            }
            found
        };
        // A fingerprint of where the words sit: if it repeats, the wheel
        // is being swallowed here and there is no point turning it more.
        let witness = |output: &egui::FullOutput| -> Vec<(i32, i32)> {
            let mut marks: Vec<(i32, i32)> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) => {
                        Some((text.pos.x.round() as i32, text.pos.y.round() as i32))
                    }
                    _ => None,
                })
                .collect();
            marks.sort_unstable();
            marks.truncate(40);
            marks
        };
        for lane in lanes {
            for direction in [-60.0, 90.0] {
                let mut stills = 0;
                while stills < 2 {
                    let output = frame(ctx, app, vec![]);
                    if let Some(seen) = find(&output) {
                        // One more quiet frame: the page may still be settling,
                        // and the click must land where the label rests.
                        let pos = find(&frame(ctx, app, vec![])).unwrap_or(seen);
                        for pressed in [true, false] {
                            frame(
                                ctx,
                                app,
                                vec![
                                    egui::Event::PointerMoved(pos),
                                    egui::Event::PointerButton {
                                        pos,
                                        button: egui::PointerButton::Primary,
                                        pressed,
                                        modifiers: egui::Modifiers::NONE,
                                    },
                                ],
                            );
                        }
                        return Ok(());
                    }
                    let before = witness(&output);
                    let wheel = egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: egui::vec2(0.0, direction),
                        phase: egui::TouchPhase::Move,
                        modifiers: egui::Modifiers::NONE,
                    };
                    let after = frame(ctx, app, vec![egui::Event::PointerMoved(lane), wheel]);
                    if witness(&after) == before {
                        stills += 1;
                    } else {
                        stills = 0;
                    }
                    // A scrolled frame needs one more pass before its
                    // hit-testing matches what the painter just drew.
                    frame(ctx, app, vec![]);
                }
            }
        }
        Err(format!("{label} never appeared on screen"))
    }

    /// Click a tool-strip icon registered under `id` on the last painted frame.
    pub(crate) fn tap_icon(ctx: &egui::Context, app: &mut App, id: &str) -> Result<(), String> {
        frame(ctx, app, vec![]);
        frame(ctx, app, vec![]);
        let pos = app
            .icon_rects
            .get(id)
            .map(|rect| rect.center())
            .ok_or_else(|| format!("icon {id} was not published"))?;
        for pressed in [true, false] {
            frame(
                ctx,
                app,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        Ok(())
    }

    pub(crate) fn draft_symbol(app: &App) -> Result<SymbolDefinition, String> {
        app.library
            .editor
            .draft
            .as_ref()
            .map(|draft| match draft {
                Draft::Symbol(def) => Ok(def.clone()),
                Draft::Footprint(_) => Err("the draft is a footprint".to_string()),
            })
            .unwrap_or_else(|| Err("there is no draft".to_string()))
    }
    pub(crate) fn editor(
        kind: &str,
    ) -> Result<(pcbcraft_mcp::SharedSession, App, egui::Context), String> {
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = App::create(shared.clone(), Preferences::default(), None, Some(kind));
        app.refresh();
        Ok((shared, app, egui::Context::default()))
    }
    #[test]
    fn adding_pins_in_the_editor_then_saving_reaches_the_project_numbered_in_order()
    -> Result<(), String> {
        let (shared, mut app, ctx) = editor("symbol-editor")?;
        tap(&ctx, &mut app, "Add pin")?;
        tap(&ctx, &mut app, "Add pin")?;
        let draft = draft_symbol(&app)?;
        assert_eq!(draft.pins.len(), 4, "two clicks add two pins");
        let numbers: Vec<&str> = draft.pins.iter().map(|p| p.number.as_str()).collect();
        assert_eq!(
            numbers,
            ["1", "2", "3", "4"],
            "new pins get the next free numbers"
        );
        tap(&ctx, &mut app, "Save symbol to project library")?;
        let session = shared.lock().map_err(|_| "session unavailable")?;
        assert_eq!(session.project.library.symbols[0].pins.len(), 4);
        Ok(())
    }
    #[test]
    fn adding_a_pad_to_a_new_footprint_and_saving_stores_three_pads() -> Result<(), String> {
        let (shared, mut app, ctx) = editor("footprint-editor")?;
        tap(&ctx, &mut app, "Add pad")?;
        let Some(Draft::Footprint(draft)) = app.library.editor.draft.clone() else {
            return Err("the footprint editor lost its draft".to_string());
        };
        assert_eq!(draft.pads.len(), 3, "one click adds one pad");
        assert_eq!(
            draft.pads[2].number, "3",
            "the new pad gets the next free number"
        );
        tap(&ctx, &mut app, "Save footprint to project library")?;
        let session = shared.lock().map_err(|_| "session unavailable")?;
        assert_eq!(session.project.library.footprints[0].pads.len(), 3);
        Ok(())
    }
    #[test]
    fn duplicating_the_preview_unit_copies_the_pins_into_a_second_unit() -> Result<(), String> {
        let (_shared, mut app, ctx) = editor("symbol-editor")?;
        tap(&ctx, &mut app, "Duplicate preview unit")?;
        let draft = draft_symbol(&app)?;
        assert_eq!(draft.pins.len(), 4, "the unit copy doubles the pins");
        assert!(
            draft.pins.iter().any(|p| p.unit == 2),
            "the copies live in a new unit"
        );
        assert_eq!(
            app.library.preview_unit, 2,
            "the preview follows the new unit"
        );
        Ok(())
    }
    #[test]
    fn a_polyline_grows_a_vertex_and_gives_it_back() -> Result<(), String> {
        let (_shared, mut app, ctx) = editor("symbol-editor")?;
        tap(&ctx, &mut app, "Polyline")?;
        let count = |app: &App| -> usize {
            draft_symbol(app)
                .map(|draft| {
                    draft
                        .graphics
                        .iter()
                        .filter(|g| matches!(g.primitive, Graphic::Polyline { .. }))
                        .map(|g| match &g.primitive {
                            Graphic::Polyline { points, .. } => points.len(),
                            _ => 0,
                        })
                        .sum()
                })
                .unwrap_or(0)
        };
        assert_eq!(count(&app), 3, "a fresh polyline starts with three corners");
        tap(&ctx, &mut app, "Graphic 2 · unit 1")?;
        tap(&ctx, &mut app, "Add vertex")?;
        assert_eq!(count(&app), 4, "the button appends a corner");
        tap(&ctx, &mut app, "Remove last vertex")?;
        assert_eq!(count(&app), 3, "the other button takes it back off");
        Ok(())
    }
    #[test]
    fn edit_then_remove_on_a_saved_definition_round_trip_the_project_library() -> Result<(), String>
    {
        let (shared, mut app, ctx) = editor("symbol-editor")?;
        tap(&ctx, &mut app, "Save symbol to project library")?;
        tap(&ctx, &mut app, "Library")?;
        tap(&ctx, &mut app, "Edit")?;
        assert_eq!(
            draft_symbol(&app)?.name,
            "NewSymbol",
            "Edit reopens the very definition that was saved"
        );
        tap(&ctx, &mut app, "Library")?;
        tap(&ctx, &mut app, "Remove")?;
        let session = shared.lock().map_err(|_| "session unavailable")?;
        assert!(
            session.project.library.symbols.is_empty(),
            "Remove takes the definition out of the project"
        );
        Ok(())
    }
    #[test]
    fn saving_a_library_file_and_loading_it_back_restores_the_whole_collection()
    -> Result<(), String> {
        let file = std::env::temp_dir().join(format!(
            "pcbcraft-lib-roundtrip-{}.pcbcraft-lib",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&file);
        let (shared, mut app, ctx) = editor("symbol-editor")?;
        tap(&ctx, &mut app, "Save symbol to project library")?;
        tap(&ctx, &mut app, "Library")?;
        app.library.editor.path = file.to_string_lossy().into_owned();
        tap(&ctx, &mut app, "Save library")?;
        assert!(
            file.is_file(),
            "the collection file lands on disk; the app said {:?}",
            app.message
        );
        app.run("library.symbol.remove", json!({"name": "NewSymbol"}));
        {
            let session = shared.lock().map_err(|_| "session unavailable")?;
            assert!(session.project.library.symbols.is_empty());
        }
        let mut app2 = App::create(
            shared.clone(),
            Preferences::default(),
            None,
            Some("symbol-editor"),
        );
        app2.refresh();
        let ctx2 = egui::Context::default();
        app2.library.editor.mode = 0;
        app2.library.editor.path = file.to_string_lossy().into_owned();
        tap(&ctx2, &mut app2, "Load library")?;

        let session = shared.lock().map_err(|_| "session unavailable")?;
        assert_eq!(
            session.project.library.symbols.len(),
            1,
            "the saved collection comes back whole"
        );
        drop(session);
        let _ = std::fs::remove_file(&file);
        Ok(())
    }

    pub(crate) fn library_world_click(
        ctx: &egui::Context,
        app: &mut App,
        world: Point,
    ) -> Result<(), String> {
        frame(ctx, app, vec![]);
        let rect = app
            .library
            .editor
            .canvas_rect
            .ok_or("library canvas was not laid out")?;
        let at = app.library.editor.camera.screen(canvas::snap(world), rect);
        frame(ctx, app, vec![egui::Event::PointerMoved(rect.center())]);
        frame(ctx, app, vec![egui::Event::PointerMoved(at)]);
        frame(ctx, app, vec![]);
        let rect = app
            .library
            .editor
            .canvas_rect
            .ok_or("library canvas was not laid out")?;
        let pos = app.library.editor.camera.screen(canvas::snap(world), rect);
        for pressed in [true, false] {
            frame(
                ctx,
                app,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        Ok(())
    }

    pub(crate) fn library_world_drag(
        ctx: &egui::Context,
        app: &mut App,
        from: Point,
        to: Point,
    ) -> Result<(), String> {
        frame(ctx, app, vec![]);
        let rect = app
            .library
            .editor
            .canvas_rect
            .ok_or("library canvas was not laid out")?;
        let camera = app.library.editor.camera;
        let a = camera.screen(from, rect);
        let b = camera.screen(to, rect);
        for event in [
            egui::Event::PointerMoved(a),
            egui::Event::PointerButton {
                pos: a,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
        ] {
            frame(ctx, app, vec![event]);
        }
        let mid = egui::pos2((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
        for p in [mid, b] {
            frame(ctx, app, vec![egui::Event::PointerMoved(p)]);
            frame(ctx, app, vec![]);
        }
        frame(
            ctx,
            app,
            vec![egui::Event::PointerButton {
                pos: b,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        frame(ctx, app, vec![]);
        Ok(())
    }

    pub(crate) fn clear_symbol_draft(app: &mut App) {
        if let Some(Draft::Symbol(def)) = app.library.editor.draft.as_mut() {
            def.pins.clear();
            def.graphics.clear();
        }
    }
}
