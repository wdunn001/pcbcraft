//! The Shape section of Board settings: preset board shapes and their sizes.
//!
//! Context: the board was only a width and a height. This section lists the
//! presets (rectangle, rounded rectangle, circle, ellipse, polygon, L shape,
//! chamfered rectangle); picking one applies it at once and shows only the sizes
//! that shape uses, and typing a size applies it again. It also shows the live
//! area and size of the traced outline.
//! Local rules: every change is the `board.set_shape` engine command; the draft
//! sizes live in egui memory, never in the document; a refused shape shows the
//! engine's reason in the message line and leaves the board as it was.
//!
//! # Dependencies
//! - crates/engine/src/outline.rs (`board.set_shape`)
//! - crates/ui-egui/src/ui_kit/number_field.rs
//! - crates/ui-egui/src/ui_kit/button.rs
//!
//! # Do not
//! - Compute outline vertices here; the engine owns the geometry.
use super::*;

/// Engine kinds of the presets, in button order.
const KINDS: [&str; 7] = [
    "rectangle",
    "rounded_rectangle",
    "circle",
    "ellipse",
    "polygon",
    "l_shape",
    "chamfered_rectangle",
];

/// Button label and tooltip of preset `i`, from the text catalog.
fn kind_text(i: usize) -> (String, String) {
    match KINDS.get(i).copied().unwrap_or_default() {
        "rectangle" => (t!("shape.rectangle"), t!("shape.rectangle.tip")),
        "rounded_rectangle" => (
            t!("shape.rounded_rectangle"),
            t!("shape.rounded_rectangle.tip"),
        ),
        "circle" => (t!("shape.circle"), t!("shape.circle.tip")),
        "ellipse" => (t!("shape.ellipse"), t!("shape.ellipse.tip")),
        "polygon" => (t!("shape.polygon"), t!("shape.polygon.tip")),
        "l_shape" => (t!("shape.l_shape"), t!("shape.l_shape.tip")),
        _ => (
            t!("shape.chamfered_rectangle"),
            t!("shape.chamfered_rectangle.tip"),
        ),
    }
}

/// Sizes being typed for the picked preset.
#[derive(Clone, Debug, Default)]
struct Draft {
    kind: Option<usize>,
    width: f64,
    height: f64,
    diameter: f64,
    radius: f64,
    sides: f64,
    notch_width: f64,
    notch_height: f64,
    chamfer: f64,
}

impl Draft {
    /// Sensible starting sizes taken from the board as it is now.
    fn starting_from(width: f64, height: f64, kind: usize) -> Self {
        let small = width.min(height);
        let round = |v: f64| (v * 10.0).round() / 10.0;
        Draft {
            kind: Some(kind),
            width: round(width),
            height: round(height),
            diameter: round(small),
            radius: round((small / 5.0).max(0.5)),
            sides: 6.0,
            notch_width: round(width / 3.0),
            notch_height: round(height / 3.0),
            chamfer: round((small / 8.0).max(0.5)),
        }
    }

    /// The `board.set_shape` parameters for the picked kind.
    fn params(&self) -> Value {
        let Some(kind) = self.kind.and_then(|k| KINDS.get(k).copied()) else {
            return json!({});
        };
        match kind {
            "rectangle" | "ellipse" => {
                json!({"kind": kind, "width": self.width, "height": self.height})
            }
            "rounded_rectangle" => json!({"kind": kind, "width": self.width,
                "height": self.height, "radius": self.radius}),
            "circle" => json!({"kind": kind, "diameter": self.diameter}),
            "polygon" => json!({"kind": kind, "diameter": self.diameter,
                "sides": self.sides.round() as u64}),
            "l_shape" => json!({"kind": kind, "width": self.width, "height": self.height,
                "notch_width": self.notch_width, "notch_height": self.notch_height}),
            _ => json!({"kind": kind, "width": self.width, "height": self.height,
                "chamfer": self.chamfer}),
        }
    }
}

impl App {
    /// The Shape section: preset buttons, the sizes of the picked preset, and
    /// the live area and size of the outline.
    ///
    /// # Rules
    /// - Picking a preset applies it with sizes taken from the board; a typed
    ///   size applies the preset again.
    /// - Only the sizes the picked preset uses are shown.
    ///
    /// # Dependencies
    /// - crates/engine/src/outline.rs (`board.set_shape`)
    ///
    /// # Do not
    /// - Keep the draft in the document.
    pub(crate) fn board_shape_section(&mut self, ui: &mut egui::Ui) {
        let draft_id = egui::Id::new("board-shape-draft");
        let mut draft: Draft = ui.data(|d| d.get_temp(draft_id)).unwrap_or_default();
        let (bw, bh) = (self.project.board.width, self.project.board.height);
        let mut apply = false;
        section(
            ui,
            "board-shape",
            &t!("props.shape"),
            &t!("props.shape.tip"),
            |ui| {
                ui.horizontal_wrapped(|ui| {
                    for i in 0..KINDS.len() {
                        let (label, tip) = kind_text(i);
                        if secondary_button(ui, None, &label, &tip, Size::Small, true).clicked() {
                            draft = Draft::starting_from(bw, bh, i);
                            apply = true;
                        }
                    }
                });
                if let Some(kind) = draft.kind.and_then(|k| KINDS.get(k).copied()) {
                    let uses = |names: &[&str]| names.contains(&kind);
                    let mut changed = false;
                    let mut field = |ui: &mut egui::Ui,
                                     row: String,
                                     id: &str,
                                     name: &str,
                                     tip: String,
                                     value: &mut f64,
                                     spec: &NumberSpec<'_>| {
                        changed |= form_row(ui, ROW, &row, None, None, |ui| {
                            number_field(ui, id, value, spec, name, &tip).changed()
                        });
                    };
                    let mm = NumberSpec {
                        min: 0.1,
                        max: 2000.0,
                        step: 0.5,
                        width: 110.0,
                        ..NumberSpec::default()
                    };
                    if uses(&[
                        "rectangle",
                        "rounded_rectangle",
                        "ellipse",
                        "l_shape",
                        "chamfered_rectangle",
                    ]) {
                        field(
                            ui,
                            t!("props.width"),
                            "shape-w",
                            "Shape width",
                            t!("shape.width.tip"),
                            &mut draft.width,
                            &mm,
                        );
                        field(
                            ui,
                            t!("props.height"),
                            "shape-h",
                            "Shape height",
                            t!("shape.height.tip"),
                            &mut draft.height,
                            &mm,
                        );
                    }
                    if uses(&["circle", "polygon"]) {
                        field(
                            ui,
                            t!("shape.diameter"),
                            "shape-d",
                            "Shape diameter",
                            t!("shape.diameter.tip"),
                            &mut draft.diameter,
                            &mm,
                        );
                    }
                    if uses(&["rounded_rectangle"]) {
                        field(
                            ui,
                            t!("shape.radius"),
                            "shape-r",
                            "Shape corner radius",
                            t!("shape.radius.tip"),
                            &mut draft.radius,
                            &mm,
                        );
                    }
                    if uses(&["chamfered_rectangle"]) {
                        field(
                            ui,
                            t!("shape.chamfer"),
                            "shape-c",
                            "Shape chamfer",
                            t!("shape.chamfer.tip"),
                            &mut draft.chamfer,
                            &mm,
                        );
                    }
                    if uses(&["polygon"]) {
                        let count = NumberSpec {
                            unit: "",
                            min: 3.0,
                            max: 60.0,
                            step: 1.0,
                            places: 0,
                            width: 110.0,
                        };
                        field(
                            ui,
                            t!("shape.sides"),
                            "shape-n",
                            "Shape sides",
                            t!("shape.sides.tip"),
                            &mut draft.sides,
                            &count,
                        );
                    }
                    if uses(&["l_shape"]) {
                        field(
                            ui,
                            t!("shape.notch_width"),
                            "shape-nw",
                            "Shape notch width",
                            t!("shape.notch_width.tip"),
                            &mut draft.notch_width,
                            &mm,
                        );
                        field(
                            ui,
                            t!("shape.notch_height"),
                            "shape-nh",
                            "Shape notch height",
                            t!("shape.notch_height.tip"),
                            &mut draft.notch_height,
                            &mm,
                        );
                    }
                    apply |= changed;
                }
                let ring = self.project.board.outline.as_deref().map(|pts| {
                    pcbcraft_doc::flatten_outline(pts, &self.project.board.outline_arcs, 0.01)
                });
                let area = match &ring {
                    Some(r) => pcbcraft_doc::outline_area(r),
                    None => bw * bh,
                };
                form_row(ui, ROW, &t!("shape.area"), None, None, |ui| {
                    value_text(ui, &format!("{area:.1} mm2"));
                });
                form_row(ui, ROW, &t!("shape.extent"), None, None, |ui| {
                    value_text(ui, &format!("{bw:.1} x {bh:.1} mm"));
                });
            },
        );
        if apply {
            self.run("board.set_shape", draft.params());
        }
        self.outline_vertex_table(ui);
        ui.data_mut(|d| d.insert_temp(draft_id, draft));
    }
}
