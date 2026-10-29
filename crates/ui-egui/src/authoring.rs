use super::*;
use pcbcraft_library::{
    Drill, FootprintDefinition, FootprintPad, Graphic, SymbolDefinition, SymbolPin, UnitGraphic,
};

#[derive(Clone)]
pub(super) enum Draft {
    Symbol(SymbolDefinition),
    Footprint(FootprintDefinition),
}

pub(super) struct Editor {
    pub mode: u8,
    pub draft: Option<Draft>,
    pub path: String,
    pub notice: Option<String>,
}
impl Default for Editor {
    fn default() -> Self {
        Self {
            mode: 0,
            draft: None,
            path: "parts.pcbcraft-lib".into(),
            notice: None,
        }
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
fn new_symbol() -> SymbolDefinition {
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
fn new_footprint() -> FootprintDefinition {
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
fn point(ui: &mut egui::Ui, label: &str, p: &mut Point) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add(egui::DragValue::new(&mut p.x).speed(0.1).prefix("X "));
        ui.add(egui::DragValue::new(&mut p.y).speed(0.1).prefix("Y "));
    });
}
fn text(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.text_edit_singleline(value);
    });
}
fn number(ui: &mut egui::Ui, label: &str, value: &mut f64) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add(egui::DragValue::new(value).speed(0.1));
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
            point(ui, "Start", start);
            point(ui, "End", end);
            number(ui, "Stroke", width);
            text(ui, "Layer", layer);
        }
        Graphic::Rectangle {
            start,
            end,
            width,
            fill,
            layer,
        } => {
            point(ui, "Start", start);
            point(ui, "End", end);
            number(ui, "Stroke", width);
            text(ui, "Fill", fill);
            text(ui, "Layer", layer);
        }
        Graphic::Circle {
            center,
            radius,
            width,
            fill,
            layer,
        } => {
            point(ui, "Center", center);
            number(ui, "Radius", radius);
            number(ui, "Stroke", width);
            text(ui, "Fill", fill);
            text(ui, "Layer", layer);
        }
        Graphic::Arc {
            start,
            mid,
            end,
            width,
            layer,
        } => {
            point(ui, "Start", start);
            point(ui, "Through", mid);
            point(ui, "End", end);
            number(ui, "Stroke", width);
            text(ui, "Layer", layer);
        }
        Graphic::Polyline {
            points,
            width,
            fill,
            layer,
        } => {
            for (i, p) in points.iter_mut().enumerate().take(128) {
                point(ui, &format!("Vertex {}", i + 1), p);
            }
            if points.len() > 128 {
                ui.label("Remaining vertices are preserved.");
            }
            if ui
                .button("Add vertex")
                .on_hover_text("Append another corner to the outline.")
                .clicked()
                && points.len() < 128
            {
                points.push(Point::default());
            }
            if ui
                .button("Remove last vertex")
                .on_hover_text("Drop the last corner of the outline.")
                .clicked()
                && points.len() > 2
            {
                points.pop();
            }
            number(ui, "Stroke", width);
            text(ui, "Fill", fill);
            text(ui, "Layer", layer);
        }
        Graphic::Text {
            position,
            angle,
            text: content,
            size,
            layer,
        } => {
            point(ui, "Position", position);
            text(ui, "Text", content);
            point(ui, "Text size", size);
            number(ui, "Angle", angle);
            text(ui, "Layer", layer);
        }
    }
}
fn add_graphic(ui: &mut egui::Ui, layer: &str) -> Option<Graphic> {
    let mut result = None;
    ui.horizontal_wrapped(|ui| {
        ui.label("Add drawing:");
        if ui
            .button("Line")
            .on_hover_text("Add a straight line segment to this definition.")
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
            .button("Rectangle")
            .on_hover_text("Add a rectangle outline to this definition.")
            .clicked()
        {
            result = Some(rectangle(layer));
        }
        if ui
            .button("Circle")
            .on_hover_text("Add a circle to this definition.")
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
            .button("Arc")
            .on_hover_text("Add an arc segment to this definition.")
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
            .button("Polyline")
            .on_hover_text("Add an open multi-segment outline to this definition.")
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
            .button("Text")
            .on_hover_text("Add a text label to this definition.")
            .clicked()
        {
            result = Some(Graphic::Text {
                position: Point::default(),
                angle: 0.0,
                text: "Label".into(),
                size: Point::new(1.0, 1.0),
                layer: layer.into(),
            });
        }
    });
    result
}
impl App {
    pub(super) fn library_workspaces(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.library.editor.mode, 0, "Library");
            ui.selectable_value(&mut self.library.editor.mode, 2, "Editor");
            if ui
                .button("New symbol")
                .on_hover_text("Start a fresh empty symbol definition.")
                .clicked()
            {
                self.library.editor.draft = Some(Draft::Symbol(new_symbol()));
                self.library.editor.mode = 2;
                self.library.editor.notice = None;
            }
            if ui
                .button("New footprint")
                .on_hover_text("Start a fresh empty footprint definition.")
                .clicked()
            {
                self.library.editor.draft = Some(Draft::Footprint(new_footprint()));
                self.library.editor.mode = 2;
                self.library.editor.notice = None;
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
        ui.heading("This project's definitions");
        ui.label("Saved here, live with the design; everything above is shared collections. Placed components retain their embedded copies.");
        ui.horizontal(|ui| {
            ui.label("Collection file");
            ui.text_edit_singleline(&mut self.library.editor.path);
            if ui
                .button("Save library")
                .on_hover_text("Write the authored collections to a reusable library file.")
                .clicked()
            {
                self.tool_call("library.save", json!({"path":self.library.editor.path}));
            }
            if ui
                .button("Load library")
                .on_hover_text("Bring a saved library file into the editor.")
                .clicked()
            {
                self.run("library.load", json!({"path":self.library.editor.path}));
            }
        });
        ui.separator();
        ui.heading("Symbols");
        for def in self.project.library.symbols.clone() {
            ui.horizontal_wrapped(|ui| {
                ui.strong(&def.name);
                ui.label(format!("{} physical pins", def.pins.len()));
                if ui
                    .button("Edit")
                    .on_hover_text("Edit this definition in the panel below.")
                    .clicked()
                {
                    self.library.editor.draft = Some(Draft::Symbol(def.clone()));
                    self.library.editor.mode = 2;
                    self.library.editor.notice = None;
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
                    .add_enabled(supported, egui::Button::new("Place"))
                    .clicked()
                {
                    self.change_tab(Tab::Schematic);
                    self.tool = Tool::CustomPlace(def.name.clone(), Box::new(def.clone()));
                }
                if ui
                    .button("Remove")
                    .on_hover_text("Remove this definition from the project library.")
                    .clicked()
                {
                    self.run("library.symbol.remove", json!({"name":def.name}));
                }
            });
        }
        ui.separator();
        ui.heading("Footprints");
        self.refresh_footprint_quality();
        for def in self.project.library.footprints.clone() {
            ui.horizontal_wrapped(|ui| {
                ui.strong(&def.name);
                ui.label(format!("{} pads", def.pads.len()));
                self.quality_badge(ui, &def.name);
                if ui
                    .button("Edit")
                    .on_hover_text("Edit this definition in the panel below.")
                    .clicked()
                {
                    self.library.editor.draft = Some(Draft::Footprint(def.clone()));
                    self.library.editor.mode = 2;
                    self.library.editor.notice = None;
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
                        egui::Button::new("Assign to selection"),
                    )
                    .clicked()
                    && let Some(id) = selected.map(|c| c.id)
                {
                    self.run("footprint.assign_custom", json!({"id":id,"name":def.name}));
                }
                if ui
                    .button("Remove")
                    .on_hover_text("Remove this definition from the project library.")
                    .clicked()
                {
                    self.run("library.footprint.remove", json!({"name":def.name}));
                }
            });
        }
    }
    fn library_editor(&mut self, ui: &mut egui::Ui) {
        let Some(mut draft) = self.library.editor.draft.take() else {
            ui.heading("Library editor");
            ui.label(
                "Create a symbol or footprint, or edit a copy from the installed/project library.",
            );
            return;
        };
        let mut save = None;
        ui.label("Dimensions are millimetres; angles are counterclockwise degrees. Save commits one undoable edit.");
        match &mut draft {
            Draft::Symbol(def) => {
                ui.heading("Symbol editor");
                text(ui, "Name", &mut def.name);
                text(ui, "Reference prefix", &mut def.reference);
                text(ui, "Default value", &mut def.value);
                text(ui, "Description", &mut def.description);
                text(ui, "Suggested footprint", &mut def.footprint);
                ui.horizontal(|ui| {
                    ui.label("Set")
                        .on_hover_text("Optional group name, like Power bits. Parts that share a set are grouped together when you add parts, and the group can be hidden.");
                    ui.text_edit_singleline(&mut def.set);
                });
                self.library_preview(ui, Some(def), None);
                if let Err(e) = def.validate() {
                    ui.colored_label(Color32::LIGHT_RED, e.to_string());
                }
                if !def.unsupported.is_empty() {
                    ui.label(format!(
                        "Preserved unsupported features: {}",
                        def.unsupported.join(", ")
                    ));
                }
                if ui
                    .button("Save symbol to project library")
                    .on_hover_text("Store this symbol in the current project library.")
                    .clicked()
                {
                    save = Some(("library.symbol.upsert", json!({"definition":def})));
                }
                ui.separator();
                ui.heading("Pins");
                if ui
                    .button("Duplicate preview unit")
                    .on_hover_text("Copy the unit being edited to grow a multi-unit package.")
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
                    .button("Add pin")
                    .on_hover_text("Add a new pin to this symbol.")
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
                        ui.collapsing(format!("Pin {} · unit {}", pin.number, pin.unit), |ui| {
                            text(ui, "Number", &mut pin.number);
                            text(ui, "Name", &mut pin.name);
                            point(ui, "Connection", &mut pin.position);
                            number(ui, "Angle", &mut pin.angle);
                            number(ui, "Length", &mut pin.length);
                            ui.add(
                                egui::DragValue::new(&mut pin.unit)
                                    .range(1..=32)
                                    .prefix("Unit "),
                            );
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
                                });
                            if ui
                                .button("Delete pin")
                                .on_hover_text("Remove this pin.")
                                .clicked()
                            {
                                remove = Some(i);
                            }
                        });
                    });
                }
                if let Some(i) = remove {
                    def.pins.remove(i);
                }
                ui.separator();
                ui.heading("Drawing");
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
                        ui.collapsing(format!("Graphic {} · unit {}", i + 1, g.unit), |ui| {
                            ui.add(
                                egui::DragValue::new(&mut g.unit)
                                    .range(0..=32)
                                    .prefix("Unit (0 shared) "),
                            );
                            graphic(ui, &mut g.primitive);
                            if ui
                                .button("Delete graphic")
                                .on_hover_text("Remove this graphic.")
                                .clicked()
                            {
                                remove = Some(i);
                            }
                        });
                    });
                }
                if let Some(i) = remove {
                    def.graphics.remove(i);
                }
            }
            Draft::Footprint(def) => {
                ui.heading("Footprint editor");
                text(ui, "Name", &mut def.name);
                text(ui, "Description", &mut def.description);
                self.library_preview(ui, None, Some(def));
                if let Err(e) = def.validate() {
                    ui.colored_label(Color32::LIGHT_RED, e.to_string());
                }
                if let Err(e) = pcbcraft_doc::validate_footprint_support(def) {
                    ui.label(e);
                }
                if ui
                    .button("Save footprint to project library")
                    .on_hover_text("Store this footprint in the current project library.")
                    .clicked()
                {
                    save = Some(("library.footprint.upsert", json!({"definition":def})));
                }
                ui.separator();
                ui.heading("Pads");
                if ui
                    .button("Add pad")
                    .on_hover_text("Add a new pad to this footprint.")
                    .clicked()
                    && def.pads.len() < 64
                {
                    let mut n = 1;
                    while def.pads.iter().any(|p| p.number == n.to_string()) {
                        n += 1;
                    }
                    def.pads.push(new_pad(&n.to_string(), 0.0));
                }
                let mut remove = None;
                for (i, pad) in def.pads.iter_mut().enumerate().take(64) {
                    ui.push_id(("pad", i), |ui| {
                        ui.collapsing(format!("Pad {} · {}", pad.number, pad.pad_type), |ui| {
                            text(ui, "Number", &mut pad.number);
                            point(ui, "Position", &mut pad.position);
                            point(ui, "Copper size", &mut pad.size);
                            number(ui, "Angle", &mut pad.rotation);
                            let before = pad.pad_type.clone();
                            egui::ComboBox::from_id_salt("pad-type")
                                .selected_text(&pad.pad_type)
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(
                                        &mut pad.pad_type,
                                        "smd".into(),
                                        "Front SMD",
                                    )
                                    .on_hover_text("Soldered to the board surface; no drill hole.");
                                    ui.selectable_value(
                                        &mut pad.pad_type,
                                        "thru_hole".into(),
                                        "Plated through-hole",
                                    )
                                    .on_hover_text("Drilled and plated so legs can pass through.");
                                    ui.selectable_value(
                                        &mut pad.pad_type,
                                        "unplated".into(),
                                        "Unplated hole",
                                    )
                                    .on_hover_text(
                                        "A bare drilled hole for a locating peg or screw. It has no copper and no net.",
                                    );
                                });
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
                            egui::ComboBox::from_id_salt("pad-shape")
                                .selected_text(&pad.shape)
                                .show_ui(ui, |ui| {
                                    for shape in ["circle", "rect", "oval", "roundrect"] {
                                        ui.selectable_value(&mut pad.shape, shape.into(), shape)
                                            .on_hover_text(
                                                "Copper outline of the pad; the mask follows it.",
                                            );
                                    }
                                });
                            if pad.shape == "roundrect" {
                                ui.add(
                                    egui::DragValue::new(pad.roundrect_ratio.get_or_insert(0.25))
                                        .range(0.0..=0.5)
                                        .speed(0.01)
                                        .prefix("Corner ratio "),
                                );
                            } else {
                                pad.roundrect_ratio = None;
                            }
                            if pad.shape != "chamfered" {
                                pad.chamfer = None;
                            }
                            if pad.shape != "custom" {
                                pad.polygon.clear();
                            }
                            if let Some(drill) = &mut pad.drill {
                                point(ui, "Drill size (oval if unequal)", &mut drill.size);
                                point(ui, "Drill offset", &mut drill.offset);
                            }
                            ui.label(format!("Layers: {}", pad.layers.join(", ")));
                            if ui
                                .button("Delete pad")
                                .on_hover_text("Remove this pad.")
                                .clicked()
                            {
                                remove = Some(i);
                            }
                        });
                    });
                }
                if let Some(i) = remove {
                    def.pads.remove(i);
                }
                ui.separator();
                ui.heading("Drawing");
                if let Some(g) = add_graphic(ui, "F.SilkS") {
                    def.graphics.push(g);
                }
                let mut remove = None;
                for (i, g) in def.graphics.iter_mut().enumerate().take(256) {
                    ui.push_id(("footprint-graphic", i), |ui| {
                        ui.collapsing(format!("Graphic {}", i + 1), |ui| {
                            graphic(ui, g);
                            if ui
                                .button("Delete graphic")
                                .on_hover_text("Remove this graphic.")
                                .clicked()
                            {
                                remove = Some(i);
                            }
                        });
                    });
                }
                if let Some(i) = remove {
                    def.graphics.remove(i);
                }
            }
        }
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
    fn frame(ctx: &egui::Context, app: &mut App, events: Vec<egui::Event>) -> egui::FullOutput {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    egui::Pos2::ZERO,
                    Vec2::new(1440.0, 940.0),
                )),
                events,
                ..Default::default()
            },
            |ui| app.show(ui),
        );
        output.textures_delta.clear();
        output
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
                    if let Some(pos) = find(&output) {
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

    fn draft_symbol(app: &App) -> Result<SymbolDefinition, String> {
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
    fn editor(kind: &str) -> Result<(pcbcraft_mcp::SharedSession, App, egui::Context), String> {
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
}
