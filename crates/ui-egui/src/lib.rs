//! One window, shared project, tabbed workspaces. Edits use the engine exclusively.
mod authoring;
mod canvas;
mod library;
mod via_covering;
use craft_ui::buttons::{ButtonVisuals, IconButton, IconButtonStyle};
use egui::{Align2, Color32, FontId, Rect, Stroke, Vec2};
use pcbcraft_doc::{Id, PinRef, Project, TEMPLATES};
use pcbcraft_geom::Point;
use pcbcraft_mcp::SharedSession;
#[cfg(test)]
use std::collections::BTreeMap;

/// Guidance shown when a panel section has no content yet: what belongs
/// there and the command that fills it, so a first-run user is never left
/// staring at a blank list.
#[cfg(test)]
const EMPTY_SYMBOLS: &str =
    "No symbols yet. Press A to open the symbol palette, or run component.add to add one.";
const EMPTY_NETS: &str = "No nets yet. Nets form when pins are wired together, or import a netlist with netlist.import and board.sync will create the packages.";
#[cfg(test)]
const EMPTY_BOARD: &str =
    "No packages on the board yet. Run board.sync to place a package for every symbol.";
const EMPTY_CHECKS: &str =
    "Checks run automatically on every edit; findings are listed here as soon as one appears.";

/// Render an empty-state line in the small muted style.
fn empty_state(ui: &mut egui::Ui, text: &str, muted: egui::Color32) {
    ui.label(egui::RichText::new(text).small().color(muted));
}

/// Help copy shown under the inspector's editable fields. Each line names
/// what the field is for and the limit the engine will enforce, so the
/// requirement is visible before the first rejected edit.
const FIELD_HELP_VALUE: &str = "Free text: a name or value, like R1 or 10k. It updates the symbol or package immediately when Apply is pressed.";
const FIELD_HELP_POSITION: &str =
    "Absolute millimetres from the design origin; Apply moves just this symbol or package.";
const FIELD_HELP_BOARD: &str =
    "Board outline size in millimetres; both values must be larger than zero.";
const FIELD_HELP_CUTOUT: &str =
    "Radius of the circle the cutout tool places; the whole circle must stay inside the board.";

fn field_help(ui: &mut egui::Ui, text: &str, muted: egui::Color32) {
    ui.label(egui::RichText::new(text).small().color(muted).weak());
}

/// The complete input reference: every key binding the app installs, plus
/// the canvas mouse gestures, with where each one applies. The overlay and
/// the tests read this one table, so the list can never claim a key the
/// handler does not install.
fn shortcut_rows() -> Vec<(&'static str, &'static str, &'static str)> {
    vec![
        ("Ctrl+Z", "Undo the last action", "anywhere"),
        ("Ctrl+Shift+Z", "Redo an undone action", "anywhere"),
        ("Ctrl+Y", "Redo an undone action", "anywhere"),
        ("Ctrl+S", "Save the design to its file", "anywhere"),
        ("Ctrl+K", "Open the command palette", "anywhere"),
        (
            "Up/Down",
            "Move the highlight through the command list",
            "command palette",
        ),
        (
            "Enter",
            "Load the highlighted command, then run it from the parameters box",
            "command palette",
        ),
        ("?", "Show or hide this shortcut list", "anywhere"),
        ("A", "Open the symbol palette", "schematic tab"),
        (
            "W",
            "Wire tool: click two pins to connect them",
            "schematic tab",
        ),
        (
            "X",
            "Route tool: click two pads to start a track",
            "pcb tab",
        ),
        (
            "M",
            "Move tool: drop the selection at the cursor",
            "anywhere",
        ),
        (
            "R",
            "Rotate the hovered or selected object by 90 degrees",
            "anywhere",
        ),
        (
            "E",
            "Load the hovered or selected object into the inspector",
            "anywhere",
        ),
        ("Home", "Zoom the view so the whole design fits", "anywhere"),
        ("Delete", "Delete the selected object", "anywhere"),
        (
            "Esc",
            "Return to the select tool and close open palettes",
            "anywhere",
        ),
        ("Middle-drag", "Pan the view", "canvas"),
        ("Scroll", "Zoom the view", "canvas"),
    ]
}

/// The stackup roles the engine accepts, paired with the words a newcomer reads.
const STACK_ROLES: [(&str, &str); 7] = [
    ("copper", "copper"),
    ("core", "core"),
    ("prepreg", "prepreg"),
    ("soldermask", "solder mask"),
    ("silkscreen", "silk"),
    ("paste", "paste"),
    ("custom", "custom"),
];
type StackShape = (
    &'static str,
    &'static str,
    &'static [(&'static str, &'static str, f64)],
);
type CheckGroup = (String, Vec<(String, Vec<Id>)>);
type AnatGroup = (
    &'static str,
    &'static str,
    &'static str,
    Vec<(Option<Id>, String)>,
    &'static str,
);
/// One line, in plain words, describing a figure drawn on a machine layer.
fn art_shape_text(shape: &pcbcraft_doc::ArtShape) -> String {
    match shape {
        pcbcraft_doc::ArtShape::Line { start, end } => format!(
            "Line at {}, {} to {}, {}",
            start.x.round(),
            start.y.round(),
            end.x.round(),
            end.y.round()
        ),
        pcbcraft_doc::ArtShape::Rect {
            x,
            y,
            width,
            height,
            ..
        } => format!(
            "Box at {}, {} · {width:.2} × {height:.2} mm",
            x.round(),
            y.round()
        ),
        pcbcraft_doc::ArtShape::Circle { center, radius } => format!(
            "Circle at {}, {} · {:.2} mm across",
            center.x.round(),
            center.y.round(),
            radius * 2.0
        ),
        pcbcraft_doc::ArtShape::Poly { points } => {
            format!("Outline through {} points", points.len())
        }
        pcbcraft_doc::ArtShape::Text { text, .. } => format!("Prints “{text}”"),
    }
}

fn human_code(code: &str) -> String {
    let words = code.replace('_', " ");
    let mut chars = words.chars();
    match chars.next() {
        Some(first) => first
            .to_uppercase()
            .next()
            .into_iter()
            .chain(chars)
            .collect(),
        None => words,
    }
}
/// The plain measurements a designer can write a check about, with the
/// token the engine stores for each.
const CUSTOM_MEASURES: [(&str, &str); 5] = [
    ("A wire thinner than", "track width below"),
    ("A wire longer than", "track length above"),
    ("More parts than", "part count above"),
    ("More vias than", "via count above"),
    ("More wires on one net than", "net track count above"),
];
/// One line in plain words for a check the designer wrote, e.g. "more than
/// 5 parts" or "wires thinner than 0.20 mm".
fn custom_measure_line(measure: &str, limit: f64) -> String {
    let n = if measure.contains("count") {
        limit as i64
    } else {
        // counts print whole; wire measures keep two decimals
        return format!(
            "{} {limit:.2} mm",
            match measure {
                "track width below" => "wires thinner than",
                "track length above" => "wires longer than",
                other => other,
            }
        );
    };
    match measure {
        "part count above" => format!("more than {n} parts"),
        "via count above" => format!("more than {n} vias"),
        "net track count above" => format!("more than {n} wires on one net"),
        other => format!("{other} {n}"),
    }
}
/// Heading for a group of findings in the review: a check the designer
/// wrote shows the name they gave it, not a code.
fn custom_heading(project: &pcbcraft_doc::Project, code: &str) -> String {
    let written = code
        .strip_prefix("custom_")
        .and_then(|rest| rest.parse::<u64>().ok())
        .and_then(|id| project.custom_checks.iter().find(|c| c.id == id))
        .map(|c| format!("Your check: {}", c.name));
    written.unwrap_or_else(|| human_code(code))
}
fn role_label(kind: &str) -> String {
    STACK_ROLES
        .iter()
        .find(|(token, _)| *token == kind)
        .map_or_else(|| kind.to_owned(), |(_, label)| (*label).to_owned())
}
fn role_token(label: &str) -> Option<&'static str> {
    STACK_ROLES
        .iter()
        .find(|(_, l)| *l == label)
        .map(|(token, _)| *token)
}

/// What the mouse and keys will do next for the active tool. The strip is
/// rendered under the toolbar so the next action is always visible instead
/// of being something the user has to remember.
fn tool_hint(tool: &Tool) -> String {
    let common = " · middle-drag: pan · scroll: zoom · Esc: back to select";
    match tool {
        Tool::Select => format!("LMB: select an object · drag: move the selected object{common}"),
        Tool::Place(_) => format!("LMB: place the symbol at the cursor{common}"),
        Tool::LibraryPlace(..) | Tool::CustomPlace(..) => {
            format!("LMB: place the symbol package at the cursor{common}")
        }
        Tool::Wire(None) => format!("LMB: pick the first pin of the new wire{common}"),
        Tool::Wire(Some(_)) => {
            format!("LMB: pick the second pin to complete the wire{common}")
        }
        Tool::Route(None) => format!("LMB: pick the first pad to start the track{common}"),
        Tool::Route(Some(_)) => {
            format!("LMB: pick the second pad to complete the track{common}")
        }
        Tool::Move => format!("LMB: drop the selected object at the cursor{common}"),
        Tool::Cutout => format!("LMB: place a circular cutout at the cursor{common}"),
        Tool::Hole => format!("LMB: drill a mounting hole at the cursor{common}"),
        Tool::NoConnect => {
            format!("LMB: click a pin to mark it unused, or again to undo{common}")
        }
        Tool::Fiducial => format!("LMB: place an assembly mark at the cursor{common}"),
        Tool::DrawLayer => {
            format!("LMB: place the shape on the active layer{common}")
        }
    }
}

/// Move the palette highlight by `delta` rows, wrapping around the ends so
/// the list behaves like a ring; an empty list always lands on row 0 and a
/// stale index (after the filter shrinks the list) is pulled back in range.
fn step_index(current: usize, count: usize, delta: isize) -> usize {
    if count == 0 {
        return 0;
    }
    let base = current.min(count - 1) as isize;
    (base + delta).rem_euclid(count as isize) as usize
}

/// Case-insensitive subsequence match: every character of the query must
/// appear, in order, somewhere in the text.
fn fuzzy_match(query: &str, text: &str) -> bool {
    let mut text = text.chars();
    query
        .chars()
        .all(|q| text.any(|t| t.eq_ignore_ascii_case(&q)))
}

/// A starter parameters object for a command: every required parameter gets
/// a plausible sample value taken from its own schema (the schema default,
/// the first listed option, the schema minimum, or a neutral stand-in), so
/// picking a command in the palette immediately shows the shape its
/// parameters must have; optional parameters are left out to be added.
fn params_template(spec: &pcbcraft_engine::CommandSpec) -> String {
    template_object(&spec.input_schema).to_string()
}

/// One line per required parameter of a command, read from the command's own
/// schema: the parameter name, the shape it accepts (type, bounds, options)
/// and the schema's own description. The palette shows this under the search
/// row so the parameters are explained before a command is ever refused.
fn param_help(spec: &pcbcraft_engine::CommandSpec) -> Vec<String> {
    let schema = &spec.input_schema;
    let properties = schema.get("properties").cloned().unwrap_or(Value::Null);
    schema
        .get("required")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(Value::as_str)
                .filter_map(|key| {
                    properties.get(key).map(|field| {
                        let mut line = format!("{key} ({}):", type_note(field));
                        match field.get("description").and_then(Value::as_str) {
                            Some(text) => line.push_str(&format!(" {text}")),
                            None => line.push_str(" required"),
                        }
                        line
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Plain-words shape of one schema field: the listed options when there are
/// any, otherwise the type with the bounds the schema states.
fn type_note(field: &Value) -> String {
    if let Some(options) = field.get("enum").and_then(Value::as_array) {
        let names: Vec<String> = options
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect();
        if !names.is_empty() {
            return format!("one of {}", names.join(" or "));
        }
    }
    match field.get("type").and_then(Value::as_str) {
        Some("integer") | Some("number") => {
            let min = field
                .get("minimum")
                .or_else(|| field.get("exclusiveMinimum"))
                .and_then(Value::as_f64);
            let max = field.get("maximum").and_then(Value::as_f64);
            match (min, max) {
                (Some(lo), Some(hi)) => format!("number {lo} to {hi}"),
                (Some(lo), None) => format!("number from {lo}"),
                (None, Some(hi)) => format!("number up to {hi}"),
                (None, None) => "number".into(),
            }
        }
        Some("string") => "text".into(),
        Some("boolean") => "true or false".into(),
        Some("array") => "list".into(),
        Some("object") => "object".into(),
        _ => "value".into(),
    }
}

fn template_object(schema: &Value) -> Value {
    let properties = schema.get("properties").cloned().unwrap_or(Value::Null);
    let keys: Vec<String> = schema
        .get("required")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let mut object = serde_json::Map::new();
    for key in keys {
        if let Some(field) = properties.get(&key) {
            object.insert(key, sample_value(field));
        }
    }
    Value::Object(object)
}

fn sample_value(schema: &Value) -> Value {
    if let Some(default) = schema.get("default") {
        return default.clone();
    }
    if let Some(options) = schema.get("enum").and_then(Value::as_array)
        && let Some(first) = options.first()
    {
        return first.clone();
    }
    match schema.get("type").and_then(Value::as_str) {
        Some("integer") => schema.get("minimum").cloned().unwrap_or(json!(1)),
        Some("number") => schema.get("minimum").cloned().unwrap_or(json!(0.0)),
        Some("boolean") => json!(false),
        Some("string") => json!(""),
        Some("array") => {
            let count = schema
                .get("minItems")
                .and_then(Value::as_u64)
                .unwrap_or(2)
                .clamp(2, 8);
            let item = schema
                .get("items")
                .map(sample_value)
                .unwrap_or(json!([0.0, 0.0]));
            Value::Array(vec![item; count as usize])
        }
        _ => {
            if schema.get("properties").is_some() {
                template_object(schema)
            } else {
                json!("")
            }
        }
    }
}

/// Hover text for a control that runs an engine command: the command
/// registry description is the single source of truth for what the control
/// does, so the UI explanation and the command documentation never drift.
fn command_tip(id: &str) -> String {
    pcbcraft_engine::commands()
        .into_iter()
        .find(|spec| spec.id == id)
        .map_or_else(
            || "This control runs an engine command.".into(),
            |spec| spec.description.to_string(),
        )
}
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, PartialEq)]
enum Tab {
    Files,
    Schematic,
    Pcb,
    Board3d,
    Rules,
    Checks,
    Settings,
    Libraries,
}
/// What the board canvas draws at each pad centre.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, Default)]
enum PadLabels {
    /// The pad's own number, the default assembly view.
    #[default]
    Number,
    /// The net the pad sits on, blank for unconnected pads.
    Net,
    /// Nothing, for a clean copper picture.
    Off,
}
impl PadLabels {
    fn name(&self) -> &'static str {
        match self {
            Self::Number => "Pads: number",
            Self::Net => "Pads: net",
            Self::Off => "Pads: off",
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
struct Preferences {
    dark: bool,
    grid: bool,
    accent: [u8; 3],
    pad_labels: PadLabels,
    #[serde(default)]
    hidden_layers: Vec<String>,
    /// Vertical split in the right aside: fraction for the Layers tree (top).
    #[serde(default = "default_aside_split")]
    aside_split: f32,
    /// Lower right-aside tab: 0 = Properties, 1 = Assistant.
    #[serde(default)]
    aside_lower_tab: u8,
    #[serde(default = "default_true")]
    library_dock_open: bool,
}
fn default_aside_split() -> f32 {
    0.45
}
fn default_true() -> bool {
    true
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            dark: true,
            grid: true,
            accent: [91, 224, 188],
            pad_labels: PadLabels::Number,
            hidden_layers: Vec::new(),
            aside_split: default_aside_split(),
            aside_lower_tab: 0,
            library_dock_open: true,
        }
    }
}
/// The copper length of a track segment in millimetres, following the bow
/// of an arc when the segment carries one.
fn track_length(track: &pcbcraft_doc::Track) -> f64 {
    let straight =
        ((track.end.x - track.start.x).powi(2) + (track.end.y - track.start.y).powi(2)).sqrt();
    let Some(mid) = track.arc else {
        return straight;
    };
    let (sx, sy) = (track.start.x, track.start.y);
    let (ex, ey) = (track.end.x, track.end.y);
    let d = 2.0 * (sx * (ey - mid.y) + ex * (mid.y - sy) + mid.x * (sy - ey));
    if d.abs() < 1e-12 {
        return straight;
    }
    let s2 = sx * sx + sy * sy;
    let e2 = ex * ex + ey * ey;
    let m2 = mid.x * mid.x + mid.y * mid.y;
    let ux = (s2 * (ey - mid.y) + e2 * (mid.y - sy) + m2 * (sy - ey)) / d;
    let uy = (s2 * (mid.x - ex) + e2 * (sx - mid.x) + m2 * (ex - sx)) / d;
    let radius = ((sx - ux).powi(2) + (sy - uy).powi(2)).sqrt();
    let angle = |x: f64, y: f64| (y - uy).atan2(x - ux);
    let wrap = |mut a: f64| {
        while a > std::f64::consts::PI {
            a -= std::f64::consts::TAU;
        }
        while a < -std::f64::consts::PI {
            a += std::f64::consts::TAU;
        }
        a
    };
    radius
        * (wrap(angle(mid.x, mid.y) - angle(sx, sy)).abs()
            + wrap(angle(ex, ey) - angle(mid.x, mid.y)).abs())
}

/// A held-off renumbering: the letter filter and the planned old/new pairs.
type RenumberPlan = (Option<String>, Vec<(String, String)>);
#[derive(Clone, Debug)]
enum Tool {
    Select,
    Place(String),
    LibraryPlace(String, String, Box<pcbcraft_library::SymbolDefinition>),
    CustomPlace(String, Box<pcbcraft_library::SymbolDefinition>),
    Wire(Option<PinRef>),
    Route(Option<(Id, String)>),
    Move,
    Cutout,
    Hole,
    NoConnect,
    Fiducial,
    DrawLayer,
}
#[derive(Clone, Copy, PartialEq, Default)]
enum DrawShape {
    #[default]
    Line,
    Rect,
    Circle,
    Poly,
    Words,
}
#[derive(Clone, Copy)]
struct Camera {
    center: Point,
    zoom: f32,
}
impl Camera {
    fn screen(self, p: Point, rect: Rect) -> egui::Pos2 {
        rect.center()
            + Vec2::new(
                (p.x - self.center.x) as f32 * self.zoom,
                (p.y - self.center.y) as f32 * self.zoom,
            )
    }
    fn world(self, p: egui::Pos2, rect: Rect) -> Point {
        Point::new(
            self.center.x + (p.x - rect.center().x) as f64 / self.zoom as f64,
            self.center.y + (p.y - rect.center().y) as f64 / self.zoom as f64,
        )
    }
}
pub struct App {
    session: SharedSession,
    project: Project,
    revision: u64,
    checks: Value,
    tab: Tab,
    tool: Tool,
    selected: Option<Id>,
    highlight_net: Option<Id>,
    selected_unit: u32,
    route_layer: pcbcraft_doc::Layer,
    preferences: Preferences,
    recent_designs: Vec<String>,
    home_open: bool,
    home_path_focus: bool,
    tree_query: String,
    tree_hidden: Vec<String>,
    focus_layer: Option<String>,
    make_dir: String,
    rule_draft: Option<(pcbcraft_doc::Id, f64, f64)>,
    class_pick: Option<String>,
    build_note: Option<(String, String)>,
    build_pick: Option<String>,
    swap_pick: Option<String>,
    swap_name: String,
    hole_diameter: f64,
    art_layer: Option<Id>,
    art_shape: DrawShape,
    art_start: Option<Point>,
    art_points: Vec<Point>,
    art_text: String,
    hole_edit: Option<(Id, f64)>,
    fiducial_diameter: f64,
    fiducial_edit: Option<(Id, f64, bool)>,
    renumber_prefix: Option<String>,
    track_width: f64,
    chamfer_size: f64,
    stitch_spacing: f64,
    pair_a: Id,
    pair_b: Id,
    pair_clearance: f64,
    dp_name: String,
    dp_p: Id,
    dp_n: Id,
    dp_gap: f64,
    area_x: f64,
    area_y: f64,
    area_w: f64,
    area_h: f64,
    area_gap: f64,
    area_limited: bool,
    area_height: f64,
    label_text: String,
    label_x: f64,
    label_y: f64,
    label_size: f64,
    label_front: bool,
    cut_x: f64,
    cut_y: f64,
    cut_r: f64,
    pour_x: f64,
    pour_y: f64,
    pour_w: f64,
    pour_h: f64,
    pour_net: Id,
    teardrop_depth: f64,
    /// Draft name, measure, limit and volume for a check of your own.
    custom_name: String,
    custom_measure: usize,
    custom_limit: f64,
    custom_loud: bool,
    panel_columns: usize,
    panel_rows: usize,
    panel_gap: f64,
    panel_rail: f64,
    panel_plan: Value,
    renumber_plan: Option<RenumberPlan>,
    stack_name: String,
    stack_kind: String,
    stack_thickness: f64,
    stack_edit: Option<(String, String, f64)>,
    anat_open: Vec<String>,
    notes_thick: f64,
    notes_er: f64,
    net_editor: Option<pcbcraft_doc::Id>,
    pin_pick: Option<String>,
    net_draft: Option<(pcbcraft_doc::Id, String)>,
    palette: bool,
    part_search: String,
    hidden_sets: Vec<String>,
    svg_path: String,
    part_search_focus: bool,
    needs_fit: bool,
    /// Left edge of the central page, so tests can check text keeps its margin.
    page_left: f32,
    schematic: Camera,
    pcb: Camera,
    drag: Option<(Id, Point)>,
    drag_offset: Point,
    cursor: Option<Point>,
    message: String,
    value: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    cutout_radius: f64,
    path: String,
    saved_revision: Option<u64>,
    command: String,
    params: String,
    command_open: bool,
    command_query: String,
    shortcuts_open: bool,
    command_focus: bool,
    command_index: usize,
    command_scroll: bool,
    capture: Option<String>,
    frames: u32,
    library: library::Browser,
    #[cfg(test)]
    icon_rects: BTreeMap<String, Rect>,
}
impl App {
    /// One library part offered as a small card: its name to pick, and a
    /// plain line saying what it is and what it answers to when searched.
    fn part_card(&mut self, ui: &mut egui::Ui, definition: &pcbcraft_library::SymbolDefinition) {
        if ui
            .button(&definition.name)
            .on_hover_text("Pick this saved part; the next click on the schematic places it.")
            .clicked()
        {
            self.change_tab(Tab::Schematic);
            self.tool = Tool::CustomPlace(definition.name.clone(), Box::new(definition.clone()));
            self.palette = false;
            self.part_search.clear();
        }
        let tags: Vec<&str> = definition.keywords.split_whitespace().collect();
        let words = tags.join(" · ");
        let note = match (definition.description.trim(), words.as_str()) {
            ("", "") => String::new(),
            ("", tags) => format!("search words: {tags}"),
            (what, "") => what.to_owned(),
            (what, tags) => format!("{what} · search words: {tags}"),
        };
        if !note.is_empty() {
            ui.label(egui::RichText::new(note).small().color(self.muted()));
        }
    }
}
/// Plain-language facts about a selected board or schematic object that is not
/// a component, so the properties panel can describe tracks, vias, zones,
/// labels, wires and graphics instead of showing a bare id.
fn object_detail_rows(project: &Project, id: Id) -> Vec<(String, String)> {
    let net = |id: Id| project.net_name(id).to_string();
    let layer = |value: &pcbcraft_doc::Layer| {
        serde_json::to_value(value)
            .ok()
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_default()
    };
    let clearance_rows = |net: Id, rows: &mut Vec<(String, String)>| {
        let base = project.board.clearance;
        let class = project.net_class(net);
        let (value, source) = match class {
            Some(c) if c.clearance > base => (c.clearance, format!("the usual gap for {}", c.name)),
            Some(c) => (
                base,
                format!("the board's usual gap, {} asks no more", c.name),
            ),
            None => (base, "the board's usual gap".to_string()),
        };
        rows.push(("Clearance".into(), format!("{value:.3} mm · {source}")));
        let stricter: Vec<String> = project
            .board
            .pair_rules
            .iter()
            .filter(|r| (r.a == net || r.b == net) && r.clearance > value)
            .map(|r| {
                let other = if r.a == net { r.b } else { r.a };
                format!(
                    "{:.3} mm away from {}",
                    r.clearance,
                    project.net_name(other)
                )
            })
            .collect();
        if !stricter.is_empty() {
            rows.push(("Special gaps".into(), stricter.join(" · ")));
        }
    };
    let mut rows: Vec<(String, String)> = Vec::new();
    if let Some(t) = project.board.tracks.iter().find(|t| t.id == id) {
        rows.push(("Net".into(), format!("{} · follows the wiring", net(t.net))));
        rows.push(("Layer".into(), layer(&t.layer)));
        let class = project.net_class(t.net);
        let width_note = match class {
            Some(c) if (t.width - c.width).abs() < 1e-6 => {
                format!("{:.3} mm · the usual width for {}", t.width, c.name)
            }
            Some(c) => format!(
                "{:.3} mm · chosen for this wire, not the usual {:.3} mm for {}",
                t.width, c.width, c.name
            ),
            None => format!("{:.3} mm · chosen for this wire", t.width),
        };
        rows.push(("Width".into(), width_note));
        clearance_rows(t.net, &mut rows);
        rows.push((
            "From".into(),
            format!("{:.2}, {:.2} mm", t.start.x, t.start.y),
        ));
        rows.push(("To".into(), format!("{:.2}, {:.2} mm", t.end.x, t.end.y)));
        if t.arc.is_some() {
            rows.push(("Shape".into(), "curved through a midpoint".into()));
        }
    } else if let Some(v) = project.board.vias.iter().find(|v| v.id == id) {
        rows.push(("Net".into(), format!("{} · follows the wiring", net(v.net))));
        rows.push(("Copper".into(), format!("{:.2} mm", v.diameter)));
        rows.push(("Drill".into(), format!("{:.2} mm", v.drill)));
        rows.push((
            "Protection".into(),
            via_covering::covering_name(v.effective_covering(&project.board)).into(),
        ));
        clearance_rows(v.net, &mut rows);
        rows.push((
            "Position".into(),
            format!("{:.2}, {:.2} mm", v.position.x, v.position.y),
        ));
    } else if let Some(z) = project.board.zones.iter().find(|z| z.id == id) {
        rows.push(("Net".into(), net(z.net)));
        rows.push(("Layer".into(), layer(&z.layer)));
        clearance_rows(z.net, &mut rows);
        rows.push((
            "Copper".into(),
            if z.filled {
                "poured".into()
            } else {
                "outline only".into()
            },
        ));
        rows.push(("Outline".into(), format!("{} vertices", z.outline.len())));
        if !z.cutouts.is_empty() {
            rows.push(("Cutouts".into(), format!("{}", z.cutouts.len())));
        }
    } else if let Some(l) = project.labels.iter().find(|l| l.id == id) {
        rows.push(("Kind".into(), l.kind.clone()));
        rows.push(("Net".into(), net(l.net)));
        rows.push((
            "Position".into(),
            format!("{:.2}, {:.2} mm", l.position.x, l.position.y),
        ));
    } else if let Some(w) = project.drawn_wires.iter().find(|w| w.id == id) {
        rows.push((
            "Net".into(),
            w.net.map(net).unwrap_or_else(|| "unbound".into()),
        ));
        rows.push(("Points".into(), format!("{}", w.points.len())));
    } else if let Some(t) = project.board.texts.iter().find(|t| t.id == id) {
        rows.push(("Text".into(), t.text.clone()));
        rows.push(("Size".into(), format!("{:.2} mm", t.size)));
        rows.push(("Layer".into(), layer(&t.layer)));
        rows.push(("Angle".into(), format!("{:.0} degrees", t.rotation)));
    } else if let Some(g) = project.graphics.iter().find(|g| g.id == id) {
        rows.push(("Shape".into(), g.shape.clone()));
        rows.push(("Layer".into(), g.layer.clone()));
        if !g.text.is_empty() {
            rows.push(("Text".into(), g.text.clone()));
        }
        rows.push(("Stroke".into(), format!("{:.3} mm", g.width)));
        rows.push(("Points".into(), format!("{}", g.points.len())));
    } else if let Some(gr) = project.graphic_groups.iter().find(|gr| gr.id == id) {
        rows.push((
            "Group".into(),
            format!("{} graphics move as one", gr.graphics.len()),
        ));
    }
    rows
}

impl App {
    pub fn with_project_path(mut self, path: Option<String>) -> Self {
        if let Some(path) = path {
            self.path = path;
            self.saved_revision = self.session.lock().ok().map(|s| s.revision);
        }
        self
    }
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        session: SharedSession,
        capture: Option<String>,
        tab: Option<&str>,
        light: bool,
    ) -> Self {
        let mut preferences: Preferences = cc
            .storage
            .and_then(|s| eframe::get_value(s, "preferences"))
            .unwrap_or_default();
        if light {
            preferences.dark = false;
        }
        if let Some(text) = cc
            .storage
            .and_then(|storage| storage.get_string("recovery"))
            && text.len() <= pcbcraft_io_limit()
            && let Ok(project) = serde_json::from_str::<Project>(&text)
            && project.validate().is_ok()
            && let Ok(mut current) = session.lock()
            && current.revision == 0
            && current.project.components.is_empty()
        {
            let _ = current.replace(project);
        }
        let show_home = capture.is_none();
        let mut app = Self::create(session, preferences, capture, tab);
        app.home_open = show_home && app.project.components.is_empty();
        app
    }
    fn create(
        session: SharedSession,
        preferences: Preferences,
        capture: Option<String>,
        tab: Option<&str>,
    ) -> Self {
        let project = session
            .lock()
            .map(|s| s.project.clone())
            .unwrap_or_default();
        let width = project.board.width;
        let height = project.board.height;
        let library = library::Browser {
            editor: authoring::initial_editor(tab, &project),
            ..Default::default()
        };
        Self {
            session,
            project,
            revision: u64::MAX,
            checks: json!({}),
            tab: match tab {
                Some("pcb") => Tab::Pcb,
                Some("checks") => Tab::Checks,
                Some("libraries" | "project-library" | "symbol-editor" | "footprint-editor") => {
                    Tab::Libraries
                }
                _ => Tab::Schematic,
            },
            tool: Tool::Select,
            selected: None,
            highlight_net: None,
            selected_unit: 1,
            route_layer: pcbcraft_doc::Layer::Front,
            preferences,
            recent_designs: pcbcraft_engine::recents::load_recent_paths(),
            home_open: false,
            home_path_focus: false,
            tree_query: String::new(),
            tree_hidden: Vec::new(),
            focus_layer: None,
            make_dir: String::new(),
            rule_draft: None,
            class_pick: None,
            build_note: None,
            build_pick: None,
            swap_pick: None,
            swap_name: String::new(),
            hole_diameter: 3.2,
            art_layer: None,
            art_shape: DrawShape::Line,
            art_start: None,
            art_points: Vec::new(),
            art_text: "MY BOARD".into(),
            hole_edit: None,
            fiducial_diameter: 1.0,
            fiducial_edit: None,
            renumber_prefix: None,
            track_width: 0.25,
            chamfer_size: 0.25,
            stitch_spacing: 3.0,
            pair_a: 0,
            pair_b: 0,
            pair_clearance: 0.4,
            dp_name: "USB".into(),
            dp_p: 0,
            dp_n: 0,
            dp_gap: 0.2,
            area_x: 10.0,
            area_y: 10.0,
            area_w: 20.0,
            area_h: 12.0,
            area_gap: 0.2,
            area_limited: false,
            area_height: 5.0,
            label_text: "Rev A".into(),
            label_x: 1.0,
            label_y: 1.0,
            label_size: 1.0,
            label_front: true,
            cut_x: 25.0,
            cut_y: 15.0,
            cut_r: 1.5,
            pour_x: 10.0,
            pour_y: 10.0,
            pour_w: 20.0,
            pour_h: 12.0,
            pour_net: 0,
            teardrop_depth: 0.5,
            custom_name: String::new(),
            custom_measure: 0,
            custom_limit: 0.2,
            custom_loud: false,
            panel_columns: 2,
            panel_rows: 2,
            panel_gap: 2.0,
            panel_rail: 3.0,
            panel_plan: Value::Null,
            renumber_plan: None,
            stack_name: String::new(),
            stack_kind: "core".to_owned(),
            stack_thickness: 1.2,
            stack_edit: None,
            anat_open: vec!["connections".to_owned()],
            notes_thick: 1.6,
            notes_er: 4.4,
            net_editor: None,
            pin_pick: None,
            net_draft: None,
            palette: false,
            part_search: String::new(),
            hidden_sets: Vec::new(),
            svg_path: String::new(),
            part_search_focus: false,
            needs_fit: true,
            page_left: 0.0,
            schematic: Camera {
                center: Point::new(40.0, 24.0),
                zoom: 11.0,
            },
            pcb: Camera {
                center: Point::new(30.0, 20.0),
                zoom: 15.0,
            },
            drag: None,
            drag_offset: Point::default(),
            cursor: None,
            message: "Ready · A add · W wire · X route · M move · R rotate · E properties".into(),
            value: String::new(),
            x: 0.0,
            y: 0.0,
            width,
            height,
            cutout_radius: 1.6,
            path: "design.pcbcraft".into(),
            saved_revision: None,
            command: "component.add".into(),
            params: "{\"kind\":\"resistor\",\"position\":{\"x\":30,\"y\":20}}".into(),
            command_open: false,
            command_query: String::new(),
            shortcuts_open: false,
            command_focus: false,
            command_index: 0,
            command_scroll: false,
            capture,
            frames: 0,
            library,
            #[cfg(test)]
            icon_rects: BTreeMap::new(),
        }
    }
    fn refresh(&mut self) {
        let snapshot = self.session.lock().ok().and_then(|s| {
            if s.revision != self.revision {
                Some((s.project.clone(), s.revision, s.checks()))
            } else {
                None
            }
        });
        if let Some((p, r, checks)) = snapshot {
            self.project = p;
            self.revision = r;
            self.checks = checks;
            if self
                .selected
                .is_some_and(|id| !self.project.ids().any(|present| present == id))
            {
                self.selected = None;
            }
            self.width = self.project.board.width;
            self.height = self.project.board.height;
            self.load_properties();
        }
    }
    fn execute_palette_params(&mut self) {
        match serde_json::from_str(&self.params.clone()) {
            Ok(v) => {
                let command = self.command.clone();
                self.run(&command, v);
            }
            Err(e) => self.message = e.to_string(),
        }
    }

    /// A list entry dragged from the library tab onto the canvas: the entry's
    /// own path and name place a real symbol where the pointer let go.
    fn place_dropped_library(&mut self, payload: &Value, position: Point) -> bool {
        if payload
            .get("kind")
            .and_then(Value::as_str)
            .is_none_or(|kind| kind != "symbol")
        {
            return false;
        }
        let path = payload.get("path").and_then(Value::as_str).unwrap_or("");
        let symbol = payload.get("name").and_then(Value::as_str).unwrap_or("");
        if path.is_empty() || symbol.is_empty() {
            return false;
        }
        self.run(
            "component.place_library",
            json!({"path": path, "symbol": symbol, "position": canvas::snap(position)}),
        );
        self.change_tab(Tab::Schematic);
        true
    }

    fn run(&mut self, command: &str, params: Value) {
        let result = self
            .session
            .lock()
            .map_err(|_| "Session unavailable".to_string())
            .and_then(|mut s| {
                s.execute(command, params, Some(self.revision))
                    .map_err(|e| e.to_string())
            });
        match result {
            Ok(v) => {
                self.message = format!("{command} · committed");
                if let Some(skipped) = v
                    .pointer("/result/unassigned_components")
                    .and_then(Value::as_array)
                    && !skipped.is_empty()
                {
                    self.message = format!(
                        "{} library symbols need a footprint assignment in Libraries",
                        skipped.len()
                    );
                }
                if let Some(id) = v.pointer("/result/id").and_then(Value::as_u64) {
                    if self.selected != Some(id) {
                        self.selected_unit = 1;
                    }
                    self.selected = Some(id);
                }
            }
            Err(e) => self.message = e,
        }
        self.refresh();
    }
    /// Run a command and hand back its result object, for commands whose
    /// answer is worth reading (a dry run, a plan, a count).
    fn run_and_keep(&mut self, command: &str, params: Value) -> Value {
        let outcome = self
            .session
            .lock()
            .map_err(|_| "Session unavailable".to_string())
            .and_then(|mut s| {
                s.execute(command, params, Some(self.revision))
                    .map_err(|e| e.to_string())
            });
        match outcome {
            Ok(v) => {
                self.message = format!("{command} · reported");
                v.pointer("/result").cloned().unwrap_or(Value::Null)
            }
            Err(e) => {
                self.message = e;
                Value::Null
            }
        }
    }
    fn tool_call(&mut self, name: &str, args: Value) -> bool {
        let result = pcbcraft_mcp::call_tool(&self.session, name, &args);
        let success = result.get("isError").and_then(Value::as_bool) != Some(true);
        self.message = if success {
            format!("{name} · complete")
        } else {
            result
                .pointer("/content/0/text")
                .and_then(Value::as_str)
                .unwrap_or("operation failed")
                .into()
        };
        self.refresh();
        if success && (name == "save" || name == "open") {
            pcbcraft_engine::recents::record_recent_path(&self.path);
            self.recent_designs = pcbcraft_engine::recents::load_recent_paths();
            self.saved_revision = Some(self.revision);
        }
        if success && (name == "open" || name == "new_design" || name == "sample_design") {
            self.needs_fit = true;
        }
        success
    }

    fn accent(&self) -> Color32 {
        if !self.preferences.dark && self.preferences.accent == [91, 224, 188] {
            return Color32::from_rgb(18, 135, 110);
        }
        Color32::from_rgb(
            self.preferences.accent[0],
            self.preferences.accent[1],
            self.preferences.accent[2],
        )
    }
    fn surface(&self) -> Color32 {
        if self.preferences.dark {
            Color32::from_rgb(20, 29, 43)
        } else {
            Color32::from_rgb(245, 247, 251)
        }
    }
    fn muted(&self) -> Color32 {
        if self.preferences.dark {
            Color32::from_rgb(140, 157, 182)
        } else {
            Color32::from_rgb(88, 104, 128)
        }
    }
    fn ink(&self) -> Color32 {
        if self.preferences.dark {
            Color32::from_rgb(225, 234, 247)
        } else {
            Color32::from_rgb(34, 48, 68)
        }
    }
    fn select(&mut self, id: Option<Id>) {
        self.selected = id;
        self.selected_unit = 1;
        self.load_properties();
    }
    fn load_properties(&mut self) {
        if let Some(id) = self.selected {
            if let Some(c) = self.project.component(id) {
                if c.unit_placement(self.selected_unit).is_none() {
                    self.selected_unit = 1;
                }
                self.value = c.value.clone();
                let position = c
                    .unit_placement(self.selected_unit)
                    .map(|p| p.0)
                    .unwrap_or(c.position);
                self.x = position.x;
                self.y = position.y;
            } else if let Some(f) = self.project.footprint(id) {
                self.x = f.position.x;
                self.y = f.position.y;
                if let Some(c) = self.project.component(f.component) {
                    self.value = c.value.clone();
                }
            }
        }
    }
    fn change_tab(&mut self, tab: Tab) {
        if tab == Tab::Pcb
            && let Some(c) = self.selected.and_then(|id| self.project.component(id))
        {
            self.selected = self
                .project
                .board
                .footprints
                .iter()
                .find(|f| f.component == c.id)
                .map(|f| f.id);
        }
        if (tab == Tab::Schematic || tab == Tab::Libraries)
            && let Some(f) = self.selected.and_then(|id| self.project.footprint(id))
        {
            self.selected = Some(f.component);
        }
        // Rules, Checks and Settings are pages, not canvases: going to one or
        // coming back from one keeps the user's zoom and pan.
        let page = |t: Tab| matches!(t, Tab::Rules | Tab::Checks | Tab::Settings);
        if !page(tab) && !page(self.tab) {
            self.needs_fit = true;
        }
        self.tab = tab;
        self.tool = Tool::Select;
        self.drag = None;
        self.load_properties();
    }
    fn layer_shown(&self, name: &str) -> bool {
        !self
            .preferences
            .hidden_layers
            .iter()
            .any(|hidden| hidden == name)
    }
    fn stackup_section(&mut self, ui: &mut egui::Ui) {
        ui.label(
            egui::RichText::new("BOARD STACKUP")
                .size(10.0)
                .color(self.muted()),
        );
        ui.label(
            egui::RichText::new(
                "The physical board: copper, core and covering layers, recorded for the factory.",
            )
            .small()
            .color(self.muted()),
        );
        ui.label(
            egui::RichText::new("START FROM A SHAPE")
                .size(10.0)
                .color(self.muted()),
        );
        ui.horizontal_wrapped(|ui| {
            const PRESETS: [StackShape; 3] = [
                (
                    "Two layers · 1.6 mm",
                    "The usual starter board: copper on each side of a plain 1.6 mm core. Choose this unless the shop told you otherwise.",
                    &[
                        ("Top copper", "copper", 0.035),
                        ("Core", "core", 1.53),
                        ("Bottom copper", "copper", 0.035),
                    ],
                ),
                (
                    "Two layers · thin 1.0 mm",
                    "A slimmer board for tight cases. Same two copper sides, thinner core.",
                    &[
                        ("Top copper", "copper", 0.035),
                        ("Core", "core", 0.93),
                        ("Bottom copper", "copper", 0.035),
                    ],
                ),
                (
                    "Four layers",
                    "Two extra inner copper layers for a solid ground plane and cleaner signals. Worth it once a board has more than a handful of parts.",
                    &[
                        ("Top copper", "copper", 0.035),
                        ("Prepreg", "prepreg", 0.2),
                        ("Inner 1", "copper", 0.035),
                        ("Core", "core", 0.86),
                        ("Inner 2", "copper", 0.035),
                        ("Prepreg 2", "prepreg", 0.2),
                        ("Bottom copper", "copper", 0.035),
                    ],
                ),
            ];
            for (title, help, layers) in PRESETS {
                if ui
                    .small_button(title)
                    .on_hover_text(format!(
                        "{help} Choosing it replaces whatever stackup is recorded now."
                    ))
                    .clicked()
                {
                    self.tool_call("board.clear_layer_records", serde_json::json!({}));
                    for (name, kind, thickness) in layers {
                        self.tool_call(
                            "board.set_layer_record",
                            serde_json::json!({"name": name, "kind": kind, "thickness": thickness}),
                        );
                    }
                }
            }
        });
        if self.project.board.layer_records.is_empty() {
            ui.label(
                egui::RichText::new(
                    "Nothing recorded yet: this board leaves the shop as two copper layers, 1.6 mm.",
                )
                .small()
                .color(self.muted()),
            );
        }
        for record in self.project.board.layer_records.clone() {
            let editing = self
                .stack_edit
                .as_ref()
                .is_some_and(|(name, _, _)| *name == record.name);
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(&record.name).strong());
                    ui.label(format!(
                        "{} · {:.3} mm",
                        role_label(&record.kind),
                        record.thickness
                    ))
                    .on_hover_text("What this layer is and how thick it is, as recorded.");
                    if !editing
                        && ui
                            .small_button("Tune")
                            .on_hover_text("Change this layer's role or thickness.")
                            .clicked()
                    {
                        self.stack_edit = Some((
                            record.name.clone(),
                            role_label(&record.kind),
                            record.thickness,
                        ));
                    }
                });
                if editing {
                    let Some((_, label, thickness)) = self.stack_edit.as_mut() else {
                        return;
                    };
                    ui.horizontal(|ui| {
                        egui::ComboBox::from_id_salt(("stack-kind", record.name.clone()))
                            .selected_text(label.clone())
                            .show_ui(ui, |ui| {
                                for (_, words) in STACK_ROLES {
                                    if ui
                                        .selectable_label(label == words, words)
                                        .on_hover_text("What this layer does in the board.")
                                        .clicked()
                                    {
                                        *label = (*words).to_owned();
                                        ui.close();
                                    }
                                }
                            })
                            .response
                            .on_hover_text("Pick what this layer is made of and does.");
                        ui.add(
                            egui::DragValue::new(thickness)
                                .speed(0.01)
                                .range(0.0..=100.0)
                                .suffix(" mm"),
                        )
                        .on_hover_text("How thick this layer is, in millimetres.");
                    });
                    let apply = ui
                        .button("Apply")
                        .on_hover_text("Keep these changes for this layer.")
                        .clicked();
                    let cancel = ui
                        .small_button("Cancel")
                        .on_hover_text("Leave this layer as it was.")
                        .clicked();
                    if apply {
                        if let Some((name, label, thickness)) = self.stack_edit.take()
                            && let Some(token) = role_token(&label)
                        {
                            self.run(
                                "board.set_layer_record",
                                json!({"name": name, "kind": token, "thickness": thickness}),
                            );
                        }
                    } else if cancel {
                        self.stack_edit = None;
                    }
                }
            });
        }
        if !self.project.board.layer_records.is_empty() {
            let total: f64 = self
                .project
                .board
                .layer_records
                .iter()
                .map(|r| r.thickness)
                .sum();
            ui.label(
                egui::RichText::new(format!("Recorded layers add up to {:.3} mm.", total))
                    .small()
                    .color(self.muted()),
            );
        }
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.stack_name)
                    .desired_width(70.0)
                    .hint_text("layer name"),
            )
            .on_hover_text("Name this layer, for example Core, Prepreg or L2.");
            egui::ComboBox::from_id_salt("stack-add-kind")
                .selected_text(self.stack_kind.clone())
                .show_ui(ui, |ui| {
                    for (_, words) in STACK_ROLES {
                        if ui
                            .selectable_label(self.stack_kind == words, words)
                            .on_hover_text("What this layer does in the board.")
                            .clicked()
                        {
                            self.stack_kind = (*words).to_owned();
                            ui.close();
                        }
                    }
                })
                .response
                .on_hover_text("Pick what this layer is made of and does.");
            ui.add(
                egui::DragValue::new(&mut self.stack_thickness)
                    .speed(0.01)
                    .range(0.0..=100.0)
                    .suffix(" mm"),
            )
            .on_hover_text("How thick this layer is, in millimetres.");
            if ui
                .add_enabled(
                    !self.stack_name.trim().is_empty(),
                    egui::Button::new("Add layer"),
                )
                .on_hover_text(
                    "Record this layer in the board stackup; the factory sheets list it.",
                )
                .clicked()
                && let Some(token) = role_token(&self.stack_kind.clone())
            {
                let name = self.stack_name.trim().to_owned();
                self.run(
                    "board.set_layer_record",
                    json!({"name": name, "kind": token, "thickness": self.stack_thickness}),
                );
                self.stack_name.clear();
            }
        });
        if !self.project.board.layer_records.is_empty()
            && ui
                .button("Back to a plain two-layer board")
                .on_hover_text(
                    "Forget every recorded layer; the board itself and its wiring stay as they are.",
                )
                .clicked()
        {
            self.stack_edit = None;
            self.run("board.clear_layer_records", json!({}));
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label("Finish");
            egui::ComboBox::from_id_salt("board-finish")
                .selected_text(self.project.board.finish.clone())
                .show_ui(ui, |ui| {
                    for finish in ["HASL", "Lead-free HASL", "ENIG", "OSP", "Immersion silver"] {
                        if ui
                            .selectable_label(self.project.board.finish == finish, finish)
                            .on_hover_text("A common coating for exposed copper.")
                            .clicked()
                        {
                            self.run("board.set_finish", json!({"finish": finish}));
                            ui.close();
                        }
                    }
                })
                .response
                .on_hover_text(
                    "The coating on exposed copper; it goes on the factory notes and reports.",
                );
        });
        self.via_covering_default_controls(ui);
        ui.horizontal(|ui| {
            ui.label("Thickness");
            ui.add(
                egui::DragValue::new(&mut self.notes_thick)
                    .speed(0.01)
                    .range(0.2..=100.0)
                    .suffix(" mm"),
            )
            .on_hover_text("How thick the whole board is; the factory uses this to pick bits.");
            if ui
                .button("Set thickness")
                .on_hover_text("Record this board thickness.")
                .clicked()
            {
                let thickness = self.notes_thick;
                self.run("board.set_stackup", json!({"thickness": thickness}));
            }
        });
        ui.horizontal(|ui| {
            ui.label("Substrate");
            ui.add(
                egui::DragValue::new(&mut self.notes_er)
                    .speed(0.05)
                    .range(1.0..=20.0),
            )
            .on_hover_text(
                "How the material between layers slows signals; it tunes impedance estimates.",
            );
            if ui
                .button("Set substrate")
                .on_hover_text("Record this substrate number.")
                .clicked()
            {
                let er = self.notes_er;
                self.run("board.set_stackup", json!({"er": er}));
            }
        });
        let mut plated = self
            .project
            .board
            .machining
            .as_ref()
            .and_then(|m| m.edge_plating)
            .unwrap_or(false);
        if ui
            .checkbox(&mut plated, "Plated board edge")
            .on_hover_text(
                "Copper plating around the board's rim, for shielding and soldering to a frame.",
            )
            .clicked()
        {
            self.run("board.set_machining", json!({"edge_plating": plated}));
        }
    }
    fn holes_section(&mut self, ui: &mut egui::Ui) {
        ui.label(
            egui::RichText::new("MOUNTING HOLES")
                .size(10.0)
                .color(self.muted()),
        );
        if self.project.board.holes.is_empty() {
            ui.label(
                egui::RichText::new(
                    "No drilled holes yet. Press Mounting hole at the top of the board, then click where a screw goes.",
                )
                .small()
                .color(self.muted()),
            );
            return;
        }
        for hole in self.project.board.holes.clone() {
            let edit = self.hole_edit.filter(|(id, _)| *id == hole.id);
            ui.horizontal(|ui| {
                ui.label("· ");
                if let Some((_, mut diameter)) = edit {
                    ui.add(
                        egui::DragValue::new(&mut diameter)
                            .speed(0.05)
                            .range(0.05..=25.0)
                            .suffix(" mm"),
                    )
                    .on_hover_text("How wide this hole is drilled.");
                    if ui
                        .button("Apply")
                        .on_hover_text("Resize this hole to that drill size.")
                        .clicked()
                    {
                        self.hole_edit = None;
                        self.run(
                            "board.hole_set_size",
                            json!({"id": hole.id, "diameter": diameter}),
                        );
                    }
                    if ui
                        .button("Cancel")
                        .on_hover_text("Leave this hole the size it is.")
                        .clicked()
                    {
                        self.hole_edit = None;
                    }
                    if let Some((held_id, held)) = &mut self.hole_edit
                        && *held_id == hole.id
                    {
                        *held = diameter;
                    }
                } else {
                    if hole.is_slot() {
                        ui.label(format!(
                            "{:.2} mm slot {:.2} mm long at {:.1}, {:.1}",
                            hole.diameter,
                            hole.length(),
                            hole.position.x,
                            hole.position.y
                        ))
                        .on_hover_text(
                            "A routed slot: its width, its overall length end to end, and where it starts.",
                        );
                    } else {
                        ui.label(format!(
                            "{:.2} mm at {:.1}, {:.1}",
                            hole.diameter, hole.position.x, hole.position.y
                        ));
                    }
                    if ui
                        .small_button("Tune")
                        .on_hover_text("Change this hole's drill size.")
                        .clicked()
                    {
                        self.hole_edit = Some((hole.id, hole.diameter));
                    }
                }
                if ui
                    .small_button("\u{d7}")
                    .on_hover_text("Take this hole out of the board.")
                    .clicked()
                {
                    self.hole_edit = None;
                    self.run("object.delete", json!({"id": hole.id}));
                }
            });
        }
    }

    fn fiducials_section(&mut self, ui: &mut egui::Ui) {
        ui.label(
            egui::RichText::new("ASSEMBLY MARKS")
                .size(10.0)
                .color(self.muted()),
        );
        if self.project.board.fiducials.is_empty() {
            ui.label(
                egui::RichText::new(
                    "No assembly marks yet. Press Assembly mark at the top of the board, then click a clear corner the build camera can find.",
                )
                .small()
                .color(self.muted()),
            );
            return;
        }
        for fid in self.project.board.fiducials.clone() {
            let edit = self.fiducial_edit.filter(|(id, _, _)| *id == fid.id);
            ui.horizontal(|ui| {
                ui.label("· ");
                if let Some((_, mut diameter, mut front)) = edit {
                    ui.add(
                        egui::DragValue::new(&mut diameter)
                            .speed(0.05)
                            .range(0.2..=10.0)
                            .suffix(" mm"),
                    )
                    .on_hover_text("How wide the bare copper disc is.");
                    if ui
                        .selectable_label(front, "Top")
                        .on_hover_text("Put the mark on the top side of the board.")
                        .clicked()
                    {
                        front = true;
                    }
                    if ui
                        .selectable_label(!front, "Bottom")
                        .on_hover_text("Put the mark on the bottom side of the board.")
                        .clicked()
                    {
                        front = false;
                    }
                    if ui
                        .button("Apply")
                        .on_hover_text("Resize this mark and move it to the chosen side.")
                        .clicked()
                    {
                        self.fiducial_edit = None;
                        self.run(
                            "board.fiducial_set",
                            json!({
                                "id": fid.id,
                                "diameter": diameter,
                                "layer": if front { "F.Cu" } else { "B.Cu" },
                            }),
                        );
                    }
                    if ui
                        .button("Cancel")
                        .on_hover_text("Leave this mark as it is.")
                        .clicked()
                    {
                        self.fiducial_edit = None;
                    }
                    if let Some((held_id, held, held_front)) = &mut self.fiducial_edit
                        && *held_id == fid.id
                    {
                        *held = diameter;
                        *held_front = front;
                    }
                } else {
                    ui.label(format!(
                        "{:.2} mm on {} at {:.1}, {:.1}",
                        fid.diameter,
                        if fid.layer == pcbcraft_doc::Layer::Front {
                            "Top"
                        } else {
                            "Bottom"
                        },
                        fid.position.x,
                        fid.position.y
                    ));
                    if ui
                        .small_button("Tune")
                        .on_hover_text("Change this mark's size or side.")
                        .clicked()
                    {
                        self.fiducial_edit = Some((
                            fid.id,
                            fid.diameter,
                            fid.layer == pcbcraft_doc::Layer::Front,
                        ));
                    }
                }
                if ui
                    .small_button("\u{d7}")
                    .on_hover_text("Take this mark off the board.")
                    .clicked()
                {
                    self.fiducial_edit = None;
                    self.run("board.fiducial_remove", json!({"id": fid.id}));
                }
            });
        }
    }

    /// True when a tree row should appear: no search is active, or the row
    /// text contains the search.
    fn tree_matches(&self, text: &str) -> bool {
        let query = self.tree_query.trim().to_lowercase();
        query.is_empty() || text.to_lowercase().contains(&query)
    }
    /// True when a whole group of the tree is switched off by the filter row.
    fn kind_hidden(&self, kind: &str) -> bool {
        self.tree_hidden.iter().any(|hidden| hidden == kind)
    }
    fn layers_tab(&mut self, ui: &mut egui::Ui) {
        self.stackup_section(ui);
        ui.separator();
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.tree_query)
                    .hint_text("Search the whole board")
                    .desired_width(150.0),
            )
            .on_hover_text(
                "Type a part reference, a net name or a layer name; the tree keeps only matching rows.",
            );
            if !self.tree_query.is_empty()
                && ui
                    .small_button("Clear")
                    .on_hover_text("Show every row again.")
                    .clicked()
            {
                self.tree_query.clear();
            }
        });
        self.board_anatomy_section(ui);
        ui.separator();
        ui.label(
            egui::RichText::new("SHOW IN THE TREE")
                .size(10.0)
                .color(self.muted()),
        );
        ui.horizontal_wrapped(|ui| {
            for (kind, label, tip) in [
                ("wires", "Show wires", "Copper wires on each face of the board."),
                ("vias", "Show vias", "Plated holes that carry a connection between the two faces."),
                ("zones", "Show pours", "Solid copper poured over an area."),
                ("holes", "Show holes", "Drilled holes and cut-outs."),
                ("words", "Show words", "Silk text printed on the board."),
                ("parts", "Show parts", "Every part on the board, with its pads."),
                ("areas", "Show rule areas", "Stricter areas and watched pairs."),
                ("art", "Show machine layers", "Your own layers for a laser, mill or printer."),
                ("connections", "Show connections", "Every net by name: add, rename or spotlight one."),
                ("notes", "Show notes", "Free-form notes stored with the project."),
            ] {
                let shown = !self.kind_hidden(kind);
                if ui
                    .selectable_label(shown, egui::RichText::new(label).small())
                    .on_hover_text(format!(
                        "{tip} Untick to fold this group away; unticking changes the tree, never the design."
                    ))
                    .clicked()
                {
                    if shown {
                        self.tree_hidden.push(kind.to_owned());
                    } else {
                        self.tree_hidden.retain(|hidden| hidden != kind);
                    }
                }
            }
        });
        ui.add_space(8.0);
        let board_header = egui::CollapsingHeader::new("This board")
            .id_salt("anat-board-root")
            .default_open(true)
            .show(ui, |ui| {
            self.holes_section(ui);
            self.fiducials_section(ui);
            ui.add_space(16.0);
            let rows: &[&str] = match self.tab {
                Tab::Schematic => &["Grid", "Symbols", "Wires", "Net labels", "Graphics"],
                Tab::Pcb => &[
                    "Grid",
                    "Top copper",
                    "Bottom copper",
                    "Pads",
                    "Vias",
                    "Copper zones",
                    "Cutouts",
                    "Package artwork",
                ],
                _ => &[],
            };
            if rows.is_empty() {
                ui.label(
                    egui::RichText::new(
                        "Switch to the schematic or the board to show or hide parts of the drawing.",
                    )
                    .small()
                    .color(self.muted()),
                );
            } else {
                ui.label(
                    egui::RichText::new("WHAT THE DRAWING SHOWS")
                        .size(10.0)
                        .color(self.muted()),
                );
                for name in rows {
                    let mut shown = self.layer_shown(name);
                    if ui
                        .checkbox(&mut shown, *name)
                        .on_hover_text(format!(
                            "Show or hide {name} on the drawing. Hiding changes only the view, never the design."
                        ))
                        .clicked()
                    {
                        if shown {
                            self.preferences.hidden_layers.retain(|hidden| hidden != name);
                        } else {
                            self.preferences.hidden_layers.push((*name).to_string());
                        }
                    }
                }
            }
            });
        board_header.header_response.on_hover_text(
            "The board itself: shape, size, surface finish, holes and what the drawing shows.",
        );
    }
    fn board_anatomy_section(&mut self, ui: &mut egui::Ui) {
        ui.add_space(16.0);
        ui.label(
            egui::RichText::new("BOARD ANATOMY")
                .size(10.0)
                .color(self.muted()),
        );
        ui.label(
            egui::RichText::new(
                "Every piece on the board, sorted into its layer. Open a layer to see its pieces; pick one to select it on the drawing.",
            )
            .small()
            .color(self.muted()),
        );
        let front: Vec<(Id, String)> = self
            .project
            .board
            .tracks
            .iter()
            .filter(|t| t.layer == pcbcraft_doc::Layer::Front)
            .map(|t| {
                (
                    t.id,
                    format!(
                        "Wire · {:.2} mm · {} · at {}, {}",
                        t.width,
                        self.project.net_name(t.net),
                        t.start.x.round(),
                        t.start.y.round()
                    ),
                )
            })
            .collect();
        let back: Vec<(Id, String)> = self
            .project
            .board
            .tracks
            .iter()
            .filter(|t| t.layer == pcbcraft_doc::Layer::Back)
            .map(|t| {
                (
                    t.id,
                    format!(
                        "Wire · {:.2} mm · {} · at {}, {}",
                        t.width,
                        self.project.net_name(t.net),
                        t.start.x.round(),
                        t.start.y.round()
                    ),
                )
            })
            .collect();
        let vias: Vec<(Id, String)> = self
            .project
            .board
            .vias
            .iter()
            .map(|v| {
                (
                    v.id,
                    format!(
                        "Via at {}, {} · {} mm hole · Top to Bottom · {}",
                        v.position.x.round(),
                        v.position.y.round(),
                        v.drill,
                        self.project.net_name(v.net)
                    ),
                )
            })
            .collect();
        let zones: Vec<(Id, String)> = self
            .project
            .board
            .zones
            .iter()
            .map(|z| {
                (
                    z.id,
                    format!(
                        "Copper zone · {} · {}",
                        self.project.net_name(z.net),
                        if z.layer == pcbcraft_doc::Layer::Front {
                            "top"
                        } else {
                            "bottom"
                        }
                    ),
                )
            })
            .collect();
        let holes: Vec<(Id, String)> = self
            .project
            .board
            .cutouts
            .iter()
            .map(|c| {
                (
                    c.id,
                    format!(
                        "Hole at {}, {} · {} mm across",
                        c.center.x.round(),
                        c.center.y.round(),
                        c.radius * 2.0
                    ),
                )
            })
            .collect();
        let words: Vec<(Id, String)> = self
            .project
            .board
            .texts
            .iter()
            .map(|t| {
                (
                    t.id,
                    format!(
                        "\"{}\" · {}",
                        t.text,
                        if t.layer == pcbcraft_doc::Layer::Front {
                            "top"
                        } else {
                            "bottom"
                        }
                    ),
                )
            })
            .collect();
        let parts: Vec<(Id, String, Vec<String>)> = self
            .project
            .board
            .footprints
            .iter()
            .map(|f| {
                let name = self
                    .project
                    .component(f.component)
                    .map_or("a part", |c| c.reference.as_str())
                    .to_owned();
                let pads = f
                    .pads
                    .iter()
                    .map(|pad| {
                        format!(
                            "Pad {} · {} mm · {}",
                            pad.number,
                            pad.diameter,
                            pad.net.map_or("no net".to_owned(), |n| self
                                .project
                                .net_name(n)
                                .to_owned())
                        )
                    })
                    .collect();
                (f.id, format!("{name} · {} pads", f.pads.len()), pads)
            })
            .filter(|(_, title, _)| self.tree_matches(title))
            .collect();
        let areas: Vec<String> = self
            .project
            .board
            .rule_areas
            .iter()
            .map(|a| {
                format!(
                    "Stricter area at {}, {} · gap {} mm{}",
                    a.region.x.round(),
                    a.region.y.round(),
                    a.clearance,
                    a.max_height
                        .map_or(String::new(), |h| format!(" · up to {h} mm tall"))
                )
            })
            .collect();
        let pairs: Vec<String> = self
            .project
            .board
            .diff_pairs
            .iter()
            .map(|d| {
                format!(
                    "{} · {} + {} · gap {} mm",
                    d.name,
                    self.project.net_name(d.net_p),
                    self.project.net_name(d.net_n),
                    d.gap
                )
            })
            .collect();
        let picked = self.selected;
        let mut chosen: Option<Id> = None;
        let groups: [AnatGroup; 8] = [
            (
                "front",
                "Top copper wires",
                "Copper wires drawn on the top face of the board. Pick one to select it on the drawing.",
                front
                    .into_iter()
                    .map(|(id, text)| (Some(id), text))
                    .filter(|(_, text)| self.tree_matches(text))
                    .collect(),
                "Nothing drawn here yet.",
            ),
            (
                "back",
                "Bottom copper wires",
                "Copper wires drawn on the bottom face of the board. Pick one to select it on the drawing.",
                back.into_iter()
                    .map(|(id, text)| (Some(id), text))
                    .filter(|(_, text)| self.tree_matches(text))
                    .collect(),
                "Nothing drawn here yet.",
            ),
            (
                "vias",
                "Via holes",
                "Small plated holes that carry a connection between the two copper faces. Pick one to select it on the drawing.",
                vias.into_iter()
                    .map(|(id, text)| (Some(id), text))
                    .filter(|(_, text)| self.tree_matches(text))
                    .collect(),
                "No vias yet.",
            ),
            (
                "zones",
                "Poured copper",
                "Solid copper poured over an area, usually ground or power. Pick one to select it on the drawing.",
                zones
                    .into_iter()
                    .map(|(id, text)| (Some(id), text))
                    .filter(|(_, text)| self.tree_matches(text))
                    .collect(),
                "No copper zones yet.",
            ),
            (
                "holes",
                "Holes and cut-outs",
                "Holes drilled through the board, such as mounting holes. Pick one to select it on the drawing.",
                holes
                    .into_iter()
                    .map(|(id, text)| (Some(id), text))
                    .filter(|(_, text)| self.tree_matches(text))
                    .collect(),
                "No holes yet.",
            ),
            (
                "words",
                "Words on the board",
                "Silk text printed on the board, such as names and labels. Pick one to select it on the drawing.",
                words
                    .into_iter()
                    .map(|(id, text)| (Some(id), text))
                    .filter(|(_, text)| self.tree_matches(text))
                    .collect(),
                "No words printed yet.",
            ),
            (
                "areas",
                "Stricter areas",
                "Patches of the board where the rules are stricter than everywhere else.",
                areas
                    .into_iter()
                    .map(|text| (None, text))
                    .filter(|(_, text)| self.tree_matches(text))
                    .collect(),
                "The whole board follows one set of rules.",
            ),
            (
                "pairs",
                "Watched pairs",
                "Pairs of nets that must run side by side with a fixed gap between them.",
                pairs
                    .into_iter()
                    .map(|text| (None, text))
                    .filter(|(_, text)| self.tree_matches(text))
                    .collect(),
                "No pairs are being watched.",
            ),
        ];
        for (key, title, tip, rows, empty) in groups {
            let kind = match key {
                "front" | "back" => "wires",
                "vias" => "vias",
                "zones" => "zones",
                "holes" => "holes",
                "words" => "words",
                "areas" | "pairs" => "areas",
                _ => "parts",
            };
            if self.kind_hidden(kind) {
                continue;
            }
            let open = self.anat_open.iter().any(|k| k == key);
            let header = egui::CollapsingHeader::new(title)
                .id_salt(format!("anat-{key}"))
                .open(Some(open))
                .show(ui, |ui| {
                    if matches!(key, "front" | "back") {
                        let focus_key = if key == "front" {
                            "copper:front"
                        } else {
                            "copper:back"
                        };
                        let focused = self.focus_layer.as_deref() == Some(focus_key);
                        if ui
                            .small_button(if focused {
                                "Stop fading"
                            } else {
                                "Fade others"
                            })
                            .on_hover_text(
                                "Everything not on this layer turns translucent on the drawing; click again to bring it back.",
                            )
                            .clicked()
                        {
                            self.focus_layer = if focused {
                                None
                            } else {
                                Some(focus_key.to_owned())
                            };
                        }
                        ui.separator();
                    }
                    if rows.is_empty() {
                        ui.label(egui::RichText::new(empty).small().color(self.muted()));
                    }
                    for (id, text) in rows {
                        let mut row = egui::RichText::new(text);
                        if id.is_none() {
                            row = row.small();
                        }
                        let clicked = match id {
                            Some(id) => ui
                                .selectable_label(picked == Some(id), row)
                                .on_hover_text("Select this on the drawing.")
                                .clicked(),
                            None => {
                                ui.label(row);
                                false
                            }
                        };
                        if clicked && let Some(id) = id {
                            chosen = Some(id);
                        }
                    }
                });
            let clicked = header.header_response.clicked();
            header.header_response.on_hover_text(tip);
            if clicked {
                if open {
                    self.anat_open.retain(|k| k != key);
                } else {
                    self.anat_open.push(key.to_owned());
                }
            }
        }
        let open = self.anat_open.iter().any(|k| k == "parts");
        let header = egui::CollapsingHeader::new("Parts and pads")
            .id_salt("anat-parts")
            .open(Some(open))
            .show(ui, |ui| {
                if parts.is_empty() {
                    ui.label(
                        egui::RichText::new("No parts on the board yet.")
                            .small()
                            .color(self.muted()),
                    );
                }
                for (id, title, pads) in parts {
                    egui::CollapsingHeader::new(title)
                        .id_salt(format!("anat-fp-{id}"))
                        .default_open(false)
                        .show(ui, |ui| {
                            if ui
                                .button("Pick up this part")
                                .on_hover_text(
                                    "Select this part so its properties open on the right.",
                                )
                                .clicked()
                            {
                                chosen = Some(id);
                            }
                            for pad in pads {
                                ui.label(egui::RichText::new(pad).small());
                            }
                        });
                }
            });
        let clicked = header.header_response.clicked();
        header
            .header_response
            .on_hover_text("Every part on the board. Open one to see its pads.");
        if clicked {
            if open {
                self.anat_open.retain(|k| k != "parts");
            } else {
                self.anat_open.push("parts".to_owned());
            }
        }
        // Machine layers the designer draws for their own laser, mill or printer.
        if !self.kind_hidden("art") {
            let layers: Vec<(Id, String, &'static str, bool, Vec<String>)> = self
                .project
                .board
                .user_layers
                .iter()
                .filter(|l| self.tree_matches(&l.name))
                .map(|l| {
                    let kind = match l.kind {
                        pcbcraft_doc::ArtKind::Cut => "cut",
                        pcbcraft_doc::ArtKind::Perforation => "perforation",
                        pcbcraft_doc::ArtKind::Silk => "silk",
                        pcbcraft_doc::ArtKind::Mask => "mask",
                        pcbcraft_doc::ArtKind::Other => "general",
                    };
                    (
                        l.id,
                        l.name.clone(),
                        kind,
                        l.visible,
                        l.shapes.iter().map(art_shape_text).collect(),
                    )
                })
                .collect();
            let open = self.anat_open.iter().any(|k| k == "art");
            let header = egui::CollapsingHeader::new("Machine layers you draw")
                .id_salt("anat-art")
                .open(Some(open))
                .show(ui, |ui| {
                    if layers.is_empty() {
                        ui.label(
                            egui::RichText::new(
                                "No machine layers yet. Add one for your laser cutter, mill or printer, then draw on it with Draw shape.",
                            )
                            .small()
                            .color(self.muted()),
                        );
                    }
                    for (id, name, kind, visible, shapes) in layers {
                        let mut shown = visible;
                        ui.horizontal(|ui| {
                            if ui
                                .checkbox(&mut shown, egui::RichText::new(&name).strong())
                                .on_hover_text(
                                    "Tick to show this layer on the drawing. Hiding changes only the view, never the design.",
                                )
                                .clicked()
                            {
                                self.run(
                                    "board.layer_visible_set",
                                    json!({"layer": id, "on": shown}),
                                );
                            }
                            let focus_key = format!("art:{name}");
                            let focused =
                                self.focus_layer.as_deref() == Some(focus_key.as_str());
                            if ui
                                .small_button(if focused {
                                    "Stop fading"
                                } else {
                                    "Fade others"
                                })
                                .on_hover_text(
                                    "Everything not on this layer turns translucent on the drawing; click again to bring it back.",
                                )
                                .clicked()
                            {
                                self.focus_layer = if focused { None } else { Some(focus_key) };
                            }
                            ui.label(
                                egui::RichText::new(format!(
                                    "{kind} layer · {} shapes",
                                    shapes.len()
                                ))
                                .small()
                                .color(self.muted()),
                            );
                        });
                        egui::CollapsingHeader::new(format!("{name} shapes"))
                            .id_salt(format!("anat-art-{id}"))
                            .default_open(false)
                            .show(ui, |ui| {
                                if shapes.is_empty() {
                                    ui.label(
                                        egui::RichText::new("Nothing drawn on this layer yet.")
                                            .small()
                                            .color(self.muted()),
                                    );
                                }
                                for text in shapes {
                                    ui.label(egui::RichText::new(text).small());
                                }
                            });
                    }
                    if ui
                        .button("New layer")
                        .on_hover_text(
                            "Add a layer for a laser, mill or printer to follow; give it a real name any time.",
                        )
                        .clicked()
                    {
                        let name = format!("Art {}", self.project.board.user_layers.len() + 1);
                        self.run("board.layer_add", json!({"name": name, "kind": "other"}));
                        if self.message.starts_with("board.layer_add") {
                            self.art_layer = self.selected;
                        }
                    }
                });
            let clicked = header.header_response.clicked();
            header.header_response.on_hover_text(
                "Layers you draw yourself: cut lines, dotted perforations and printed marks for your own machines.",
            );
            if clicked {
                if open {
                    self.anat_open.retain(|k| k != "art");
                } else {
                    self.anat_open.push("art".to_owned());
                }
            }
        }
        // Every connection by name, with room to add, rename and spotlight one.
        if !self.kind_hidden("connections") {
            let open = self.anat_open.iter().any(|k| k == "connections");
            let header = egui::CollapsingHeader::new("Connections")
                .id_salt("anat-connections")
                .default_open(true)
                .open(Some(open))
                .show(ui, |ui| {
                    self.nets_tab(ui);
                });
            let clicked = header.header_response.clicked();
            header.header_response.on_hover_text(
                "Every wire connection by name: add one, rename one, or light one up on the drawing.",
            );
            if clicked {
                if open {
                    self.anat_open.retain(|k| k != "connections");
                } else {
                    self.anat_open.push("connections".to_owned());
                }
            }
        }
        // Notes stored inside the design file.
        if !self.kind_hidden("notes") {
            let notes: Vec<String> = self
                .project
                .meta
                .iter()
                .map(|(key, value)| format!("{key} · {value}"))
                .filter(|text| self.tree_matches(text))
                .collect();
            let open = self.anat_open.iter().any(|k| k == "notes");
            let header = egui::CollapsingHeader::new("Project notes")
                .id_salt("anat-notes")
                .open(Some(open))
                .show(ui, |ui| {
                    if notes.is_empty() {
                        ui.label(
                            egui::RichText::new("No notes are stored with this project yet.")
                                .small()
                                .color(self.muted()),
                        );
                    }
                    for text in notes {
                        ui.label(egui::RichText::new(text).small());
                    }
                });
            let clicked = header.header_response.clicked();
            header.header_response.on_hover_text(
                "Free-form notes kept inside the design file: title, revision, or anything a maker should know.",
            );
            if clicked {
                if open {
                    self.anat_open.retain(|k| k != "notes");
                } else {
                    self.anat_open.push("notes".to_owned());
                }
            }
        }

        if let Some(id) = chosen {
            self.select(Some(id));
            if self.tab != Tab::Pcb {
                self.change_tab(Tab::Pcb);
            }
        }
    }
    fn rotate(&mut self) {
        if let Some(id) = self.selected {
            if let Some(c) = self.project.component(id) {
                self.run(
                    "component.rotate",
                    json!({"id":id,"unit":self.selected_unit,"degrees":c.unit_placement(self.selected_unit).map(|p|p.1).unwrap_or(c.rotation)+90.0}),
                );
            } else if let Some(f) = self.project.footprint(id) {
                self.run(
                    "footprint.rotate",
                    json!({"id":id,"degrees":f.rotation+90.0}),
                );
            }
        }
    }
    fn shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() || ctx.current_pass_index() > 0 {
            return;
        }
        ctx.input(|i| {
            if i.modifiers.command {
                if i.key_pressed(egui::Key::F) {
                    self.needs_fit = true;
                }
                if i.key_pressed(egui::Key::Z) {
                    self.tool_call(if i.modifiers.shift { "redo" } else { "undo" }, json!({}));
                }
                if i.key_pressed(egui::Key::Y) {
                    self.tool_call("redo", json!({}));
                }
                if i.key_pressed(egui::Key::S) {
                    self.tool_call("save", json!({"path":self.path}));
                }
                if i.key_pressed(egui::Key::K) {
                    self.command_open = !self.command_open;
                    self.command_focus = self.command_open;
                    self.command_index = 0;
                }
                return;
            }
            if i.key_pressed(egui::Key::A) && self.tab == Tab::Schematic {
                self.palette = true;
            }
            if i.key_pressed(egui::Key::W) && self.tab == Tab::Schematic {
                self.tool = Tool::Wire(None);
            }
            if i.key_pressed(egui::Key::X) && self.tab == Tab::Pcb {
                self.tool = Tool::Route(None);
            }
            if i.key_pressed(egui::Key::M) {
                self.select_hover();
                self.tool = Tool::Move;
            }
            if i.key_pressed(egui::Key::R) {
                self.select_hover();
                self.rotate();
            }
            if i.key_pressed(egui::Key::E) {
                self.select_hover();
                self.load_properties();
            }
            if i.key_pressed(egui::Key::Home) {
                self.needs_fit = true;
            }
            if i.key_pressed(egui::Key::Escape) {
                self.tool = Tool::Select;
                self.drag = None;
                self.palette = false;
                self.shortcuts_open = false;
            }
            if i.key_pressed(egui::Key::Slash) {
                self.shortcuts_open = !self.shortcuts_open;
            }
            if i.key_pressed(egui::Key::Delete)
                && let Some(id) = self.selected
            {
                self.run("object.delete", json!({"id":id}));
                self.select(None);
            }
        });
    }
    fn style(&self, ctx: &egui::Context) {
        let mut visuals = if self.preferences.dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        };
        visuals.panel_fill = if self.preferences.dark {
            Color32::from_rgb(17, 24, 36)
        } else {
            Color32::from_rgb(235, 239, 245)
        };
        visuals.window_fill = self.surface();
        visuals.selection.bg_fill = self.accent().gamma_multiply(0.25);
        visuals.selection.stroke = Stroke::new(1.0, self.accent());
        craft_ui::theme::WidgetPalette {
            noninteractive: self.surface(),
            inactive: self.surface(),
            hovered: if self.preferences.dark {
                Color32::from_rgb(37, 53, 73)
            } else {
                Color32::from_rgb(215, 225, 236)
            },
            active: self.accent().gamma_multiply(0.25),
            open: self.surface(),
        }
        .apply_to(
            &mut visuals.widgets,
            egui::CornerRadius::same(6),
            Stroke::new(1.0, self.ink()),
        );
        ctx.set_visuals(visuals);
        ctx.style_mut_of(ctx.theme(), |s| {
            s.spacing.item_spacing = Vec2::new(8.0, 8.0);
            s.spacing.button_padding = Vec2::new(10.0, 6.0);
            s.spacing.window_margin = egui::Margin::same(12);
            s.spacing.menu_margin = egui::Margin::same(12);
        });
    }
    fn dock_frame(style: &egui::Style) -> egui::Frame {
        egui::Frame::side_top_panel(style).inner_margin(egui::Margin::symmetric(16, 12))
    }
    const APP_BAR_HEIGHT: f32 = 34.0;
    fn header(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("app-bar")
            .exact_size(Self::APP_BAR_HEIGHT)
            .frame(Self::dock_frame(ui.style()))
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        let brand = ui.add(
                            egui::Label::new(
                                egui::RichText::new("PCBCraft")
                                    .size(15.0)
                                    .strong()
                                    .color(self.accent()),
                            )
                            .sense(egui::Sense::click()),
                        );
                        if brand
                            .on_hover_text("Back to the start screen, where you open or begin a design.")
                            .clicked()
                        {
                            self.home_open = true;
                        }
                        if ui
                            .button("Home")
                            .on_hover_text("Open the start screen.")
                            .clicked()
                        {
                            self.home_open = true;
                        }
                        if self.library.editor.mode == 0 {
                            ui.separator();
                            self.file_menu(ui);
                            self.edit_menu(ui);
                            self.help_menu(ui);
                            if ui
                                .button("Save")
                                .on_hover_text("Write the design to its file on disk.")
                                .clicked()
                            {
                                self.tool_call("save", json!({"path": self.path}));
                            }
                            if ui
                                .button("Undo")
                                .on_hover_text(command_tip("undo"))
                                .clicked()
                            {
                                self.tool_call("undo", json!({}));
                            }
                            if ui
                                .button("Redo")
                                .on_hover_text(command_tip("redo"))
                                .clicked()
                            {
                                self.tool_call("redo", json!({}));
                            }
                        }
                        ui.separator();
                        let tab_label = self.project.name.clone();
                        ui.label(
                            egui::RichText::new(tab_label)
                                .strong()
                                .color(self.ink()),
                        )
                        .on_hover_text("This document tab names the open design.");
                        if ui
                            .button("+")
                            .on_hover_text("Start another design from the New section on the start screen.")
                            .clicked()
                        {
                            self.home_open = true;
                        }
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new(env!("CARGO_PKG_VERSION"))
                                .small()
                                .color(self.muted()),
                        )
                        .on_hover_text("Application version.");
                        if ui
                            .button("Report a bug")
                            .on_hover_text("Open a prefilled issue with version and environment details.")
                            .clicked()
                        {
                            let url = format!(
                                "https://github.com/tangency-labs/pcbcraft/issues/new?labels=bug&title=Bug%20report&body=Version%20{}%0A",
                                env!("CARGO_PKG_VERSION")
                            );
                            ui.ctx().open_url(egui::OpenUrl::same_tab(url));
                        }
                        if ui
                            .button("Settings")
                            .on_hover_text("Project settings, appearance, and library sources.")
                            .clicked()
                        {
                            self.change_tab(Tab::Settings);
                        }
                    });
                });
            });
    }
    fn workbench_tabs(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            for (tab, label) in [
                (Tab::Files, "Files"),
                (Tab::Schematic, "Schematic"),
                (Tab::Pcb, "Layout"),
                (Tab::Board3d, "3D View"),
                (Tab::Rules, "Rules"),
                (Tab::Checks, "Checks"),
                (Tab::Settings, "Settings"),
            ] {
                let selected = self.tab == tab;
                if ui
                    .selectable_label(selected, label)
                    .on_hover_text(format!("Switch to the {label} view."))
                    .clicked()
                {
                    self.change_tab(tab);
                }
            }
        });
        if !matches!(self.tool, Tool::Select) && !matches!(self.tab, Tab::Schematic | Tab::Pcb) {
            ui.horizontal(|ui| {
                ui.add_space(16.0);
                ui.label(
                    egui::RichText::new(tool_hint(&self.tool))
                        .small()
                        .color(self.muted()),
                );
            });
        }
    }
    fn start_home(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(self.surface()))
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.vertical_centered(|ui| {
                            ui.add_space(36.0);
                            ui.label(
                                egui::RichText::new("Welcome to PCBCraft")
                                    .size(30.0)
                                    .strong()
                                    .color(self.accent()),
                            );
                            ui.label(
                                egui::RichText::new(
                                    "Draw a circuit, place the parts, and finish with a board ready to order.",
                                )
                                .size(14.0)
                                .color(self.muted()),
                            );
                            ui.add_space(24.0);
                            if ui
                                .add_sized(
                                    [300.0, 64.0],
                                    egui::Button::new(
                                        (
                                            egui::RichText::new("New design")
                                                .size(16.0)
                                                .strong(),
                                            egui::RichText::new("Begin with a blank page.")
                                                .small()
                                                .color(self.muted()),
                                        ),
                                    ),
                                )
                                .on_hover_text("Start a fresh empty design.")
                                .clicked()
                            {
                                self.tool_call("new_design", json!({}));
                                self.select(None);
                                self.home_open = false;
                            }
                            ui.add_space(6.0);
                            if ui
                                .add_sized(
                                    [300.0, 64.0],
                                    egui::Button::new(
                                        (
                                            egui::RichText::new("Worked example")
                                                .size(16.0)
                                                .strong(),
                                            egui::RichText::new(
                                                "A small battery, resistor and LED to explore.",
                                            )
                                            .small()
                                            .color(self.muted()),
                                        ),
                                    ),
                                )
                                .on_hover_text(
                                    "Start from a small worked example: a battery, resistor and LED.",
                                )
                                .clicked()
                            {
                                self.tool_call("sample_design", json!({}));
                                self.home_open = false;
                            }
                            ui.add_space(6.0);
                            if ui
                                .add_sized(
                                    [300.0, 64.0],
                                    egui::Button::new(
                                        (
                                            egui::RichText::new("Open a design file…")
                                                .size(16.0)
                                                .strong(),
                                            egui::RichText::new(
                                                "Type a path, or pick one from the list below.",
                                            )
                                            .small()
                                            .color(self.muted()),
                                        ),
                                    ),
                                )
                                .on_hover_text(
                                    "Load a saved design file from the path beside this button.",
                                )
                                .clicked()
                            {
                                self.home_path_focus = true;
                            }
                            ui.add_space(6.0);
                            ui.horizontal(|ui| {
                                let path_box = ui
                                    .add(
                                        egui::TextEdit::singleline(&mut self.path)
                                            .hint_text("design file path")
                                            .desired_width(240.0),
                                    )
                                    .on_hover_text(
                                        "The file that Open on this page, and in the File menu, uses.",
                                    );
                                if self.home_path_focus {
                                    path_box.request_focus();
                                    self.home_path_focus = false;
                                }
                                if ui
                                    .button("Open")
                                    .on_hover_text(
                                        "Load the design file named in the box beside this button.",
                                    )
                                    .clicked()
                                {
                                    self.tool_call("open", json!({"path": self.path}));
                                    if self.message.starts_with("open") {
                                        self.home_open = false;
                                    }
                                }
                            });
                            if !self.recent_designs.is_empty() {
                                ui.add_space(18.0);
                                ui.label(
                                    egui::RichText::new("Open one you worked on recently")
                                        .size(14.0)
                                        .strong(),
                                );
                                for design in self.recent_designs.clone() {
                                    if ui
                                        .add(
                                            egui::Button::new(
                                                egui::RichText::new(&design).small(),
                                            )
                                            .min_size(egui::vec2(300.0, 28.0)),
                                        )
                                        .on_hover_text(
                                            "Open this design again where you left off.",
                                        )
                                        .clicked()
                                    {
                                        self.path = design;
                                        self.tool_call("open", json!({"path": self.path}));
                                        if self.message.starts_with("open") {
                                            self.home_open = false;
                                        }
                                    }
                                }
                            }
                            ui.add_space(14.0);
                            ui.label(
                                egui::RichText::new(
                                    "Press Escape, or click PCBCraft in the corner, to return here any time.",
                                )
                                .small()
                                .color(self.muted()),
                            );
                            ui.add_space(24.0);
                        });
                    });
            });
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.home_open = false;
        }
    }
    fn file_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("File", |ui| {
            if ui
                .button("New")
                .on_hover_text("Start a fresh empty design.")
                .clicked()
            {
                self.tool_call("new_design", json!({}));
                self.select(None);
            }
            if ui
                .button("Open")
                .on_hover_text(
                    "Choose a design to open; your recent designs are listed on the screen that appears.",
                )
                .clicked()
            {
                self.home_open = true;
            }
            if ui
                .button("Save")
                .on_hover_text("Write the design to its file on disk. (Ctrl+S works too.)")
                .clicked()
            {
                self.tool_call("save", json!({"path":self.path}));
            }
            ui.separator();
            if ui
                .button("Export")
                .on_hover_text("Write the board out so other design tools can open it.")
                .clicked()
            {
                let path = std::path::Path::new(&self.path)
                    .with_extension("kicad_pcb")
                    .to_string_lossy()
                    .into_owned();
                self.tool_call("export", json!({"path":path,"format":"kicad_pcb"}));
            }
            if ui
                .button("Copy picture")
                .on_hover_text("Copy a PNG snapshot of the current view so it can be pasted into a document.")
                .clicked()
            {
                let ctx = ui.ctx().clone();
                self.copy_plot_to_clipboard(&ctx);
            }
        })
        .response
        .on_hover_text("Start, open, save, export and share this design.");
    }
    fn edit_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("Edit", |ui| {
            if ui
                .button("Undo")
                .on_hover_text("Step back one action; every edit here is one undoable step.")
                .clicked()
            {
                self.tool_call("undo", json!({}));
            }
            if ui
                .button("Redo")
                .on_hover_text("Step forward to the action you undid.")
                .clicked()
            {
                self.tool_call("redo", json!({}));
            }
            ui.separator();
            if ui
                .button("Update board from schematic")
                .on_hover_text(
                    "Copy every symbol, wire and net onto the board, then show the board.",
                )
                .clicked()
            {
                self.run("board.sync", json!({}));
                self.change_tab(Tab::Pcb);
            }
            ui.separator();
            if let Some(lit) = self.highlight_net
                && let Some(net) = self.project.nets.iter().find(|n| n.id == lit)
            {
                let name = net.name.clone();
                if ui
                    .button(format!("Clear wiring on {name}"))
                    .on_hover_text(
                        "Lift every piece of copper drawn for the lit connection. The connection itself stays.",
                    )
                    .clicked()
                {
                    self.run("net.unroute", json!({"net": lit}));
                }
            }
            if ui
                .button("Clear all wiring")
                .on_hover_text(
                    "Start the copper over on the whole board. Parts, connections and copper zones stay.",
                )
                .clicked()
            {
                self.run("board.unroute", json!({}));
            }
        })
        .response
        .on_hover_text(
            "Step back and forward, sync the board, and start the wiring over on one connection or the whole board.",
        );
    }
    fn help_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("Help", |ui| {
            if ui
                .button("Keyboard shortcuts")
                .on_hover_text("Show every keyboard shortcut and mouse gesture in one list.")
                .clicked()
            {
                self.shortcuts_open = !self.shortcuts_open;
            }
            if ui
                .button("Commands  Ctrl+K")
                .on_hover_text(
                    "Search every command by name and run it, with its parameters explained.",
                )
                .clicked()
            {
                self.command_open = !self.command_open;
                self.command_focus = self.command_open;
                self.command_index = 0;
            }
        })
        .response
        .on_hover_text("Shortcuts list and the command search.");
    }
    fn browser(&mut self, ui: &mut egui::Ui) {
        egui::Panel::left("left-rail")
            .default_size(44.0)
            .min_size(44.0)
            .max_size(44.0)
            .resizable(false)
            .frame(Self::dock_frame(ui.style()))
            .show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(8.0);
                    if ui
                        .selectable_label(self.preferences.library_dock_open, "Lib")
                        .on_hover_text("Show or hide the component library.")
                        .clicked()
                    {
                        self.preferences.library_dock_open = !self.preferences.library_dock_open;
                    }
                    let mut fix = 0usize;
                    let mut mind = 0usize;
                    let checks = self.checks.clone();
                    for key in ["erc", "drc"] {
                        if let Some(findings) = checks.get(key).and_then(Value::as_array) {
                            for f in findings {
                                let loud =
                                    f.get("severity").and_then(Value::as_str) == Some("error");
                                if loud {
                                    fix += 1;
                                } else {
                                    mind += 1;
                                }
                            }
                        }
                    }
                    if fix > 0
                        && ui
                            .button(format!("{fix}"))
                            .on_hover_text("Open the Checks tab for errors you must fix.")
                            .clicked()
                    {
                        self.change_tab(Tab::Checks);
                    }
                    if mind > 0
                        && ui
                            .button(format!("{mind}"))
                            .on_hover_text("Open the Checks tab for warnings worth a look.")
                            .clicked()
                    {
                        self.change_tab(Tab::Checks);
                    }
                });
            });
        if !self.preferences.library_dock_open {
            return;
        }
        egui::Panel::left("library-dock")
            .default_size(280.0)
            .min_size(260.0)
            .frame(Self::dock_frame(ui.style()))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Library").strong());
                    if ui
                        .small_button("Hide")
                        .on_hover_text("Close the library dock.")
                        .clicked()
                    {
                        self.preferences.library_dock_open = false;
                    }
                });
                ui.add_space(8.0);
                if ui
                    .button("Add part")
                    .on_hover_text("Search for a part to place on the schematic.")
                    .clicked()
                {
                    self.palette = true;
                    self.part_search_focus = true;
                }
                if self.palette {
                    ui.label(egui::RichText::new("Add a part").strong());
                    let query_box = ui
                        .add(
                            egui::TextEdit::singleline(&mut self.part_search)
                                .hint_text("Search parts by name")
                                .desired_width(f32::INFINITY),
                        )
                        .on_hover_text("Type any part name; the list below narrows as you type.");
                    if self.part_search_focus {
                        query_box.request_focus();
                        self.part_search_focus = false;
                    }
                    if ui
                        .button("Browse component libraries →")
                        .on_hover_text(
                            "Open library sources in Settings to browse symbol and footprint collections.",
                        )
                        .clicked()
                    {
                        self.change_tab(Tab::Settings);
                        self.palette = false;
                    }
                    let query = self.part_search.trim().to_lowercase();
                    let built_in: Vec<(&str, &str)> = TEMPLATES
                        .iter()
                        .filter(|t| {
                            query.is_empty()
                                || t.label.to_lowercase().contains(&query)
                                || t.kind.contains(query.as_str())
                        })
                        .map(|t| (t.kind, t.label))
                        .collect();
                    let saved: Vec<pcbcraft_library::SymbolDefinition> = self
                        .project
                        .library
                        .symbols
                        .iter()
                        .filter(|d| {
                            query.is_empty()
                                || d.name.to_lowercase().contains(&query)
                                || d.keywords.to_lowercase().contains(query.as_str())
                                || d.set.to_lowercase().contains(query.as_str())
                        })
                        .cloned()
                        .collect();
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            if built_in.is_empty() && saved.is_empty() {
                                ui.label(
                                    egui::RichText::new(format!(
                                        "Nothing here matches \"{}\" yet.",
                                        self.part_search.trim()
                                    ))
                                    .small()
                                    .color(self.muted()),
                                );
                            }
                            if !built_in.is_empty() {
                                ui.label(
                                    egui::RichText::new("Common parts")
                                        .size(10.0)
                                        .color(self.muted()),
                                );
                                for (kind, label) in built_in {
                                    if ui
                                        .button(label)
                                        .on_hover_text(
                                            "Pick this part; the next click on the schematic places it.",
                                        )
                                        .clicked()
                                    {
                                        self.change_tab(Tab::Schematic);
                                        self.tool = Tool::Place(kind.into());
                                        self.palette = false;
                                        self.part_search.clear();
                                    }
                                }
                            }
                            if !saved.is_empty() {
                                ui.label(
                                    egui::RichText::new("From this project's library")
                                        .size(10.0)
                                        .color(self.muted()),
                                );
                                let mut sets: std::collections::BTreeMap<
                                    String,
                                    Vec<pcbcraft_library::SymbolDefinition>,
                                > = std::collections::BTreeMap::new();
                                for definition in saved {
                                    sets.entry(definition.set.trim().to_owned())
                                        .or_default()
                                        .push(definition);
                                }
                                let loose = sets.remove("");
                                for (set, definitions) in &sets {
                                    let mut shown = !self.hidden_sets.iter().any(|h| h == set);
                                    let row = ui.checkbox(
                                        &mut shown,
                                        egui::RichText::new(format!(
                                            "{set} ({} part{})",
                                            definitions.len(),
                                            if definitions.len() == 1 {
                                                ""
                                            } else {
                                                "s"
                                            }
                                        ))
                                        .strong(),
                                    )
                                    .on_hover_text(
                                        "Parts grouped under this name. Untick to hide the whole group while you add parts.",
                                    );
                                    if row.changed() {
                                        if shown {
                                            self.hidden_sets.retain(|h| h != set);
                                        } else {
                                            self.hidden_sets.push(set.clone());
                                        }
                                    }
                                    if !shown {
                                        continue;
                                    }
                                    for definition in definitions {
                                        self.part_card(ui, definition);
                                    }
                                }
                                for definition in loose.unwrap_or_default() {
                                    self.part_card(ui, &definition);
                                }
                            }
                        });
                }
                ui.add_space(12.0);
                ui.label(
                    egui::RichText::new("COMPONENTS")
                        .size(10.0)
                        .color(self.muted()),
                );
                for c in self.project.components.clone() {
                    let id = if self.tab == Tab::Pcb {
                        self.project
                            .board
                            .footprints
                            .iter()
                            .find(|f| f.component == c.id)
                            .map(|f| f.id)
                            .unwrap_or(c.id)
                    } else {
                        c.id
                    };
                    if ui
                        .selectable_label(
                            self.selected == Some(id),
                            format!("{}   {}", c.reference, c.value),
                        )
                        .on_hover_text("Select this component and show its properties.")
                        .clicked()
                    {
                        self.select(Some(id));
                    }
                    let what = c
                        .field("Description")
                        .map(str::to_owned)
                        .or_else(|| {
                            c.library_symbol.as_ref().map(|ls| ls.definition.description.clone())
                        })
                        .map(|d| d.trim().to_owned())
                        .filter(|d| !d.is_empty());
                    let origin = c.library_symbol.as_ref().map_or_else(
                        || "built in".to_string(),
                        |ls| format!("from {}", ls.definition.name),
                    );
                    let note = what.map_or_else(
                        || origin.clone(),
                        |d| format!("{d} · {origin}"),
                    );
                    ui.label(
                        egui::RichText::new(format!("    {note}"))
                            .small()
                            .color(self.muted()),
                    );
                }
                if self.project.components.len() >= 2 {
                    ui.add_space(12.0);
                    ui.label(
                        egui::RichText::new("RENUMBER PARTS")
                            .size(10.0)
                            .color(self.muted()),
                    );
                    let mut prefixes: Vec<String> = self
                        .project
                        .components
                        .iter()
                        .map(|c| {
                            c.reference
                                .chars()
                                .take_while(char::is_ascii_alphabetic)
                                .collect::<String>()
                        })
                        .filter(|p| !p.is_empty())
                        .collect();
                    prefixes.sort();
                    prefixes.dedup();
                    let mut pick = self.renumber_plan.clone().map_or_else(
                        || self.renumber_prefix.clone(),
                        |(prefix, _)| prefix,
                    );
                    ui.horizontal(|ui| {
                        egui::ComboBox::from_id_salt("renumber-prefix")
                            .selected_text(pick.clone().unwrap_or_else(|| "all letters".into()))
                            .show_ui(ui, |ui| {
                                if ui
                                    .selectable_label(pick.is_none(), "all letters")
                                    .on_hover_text(
                                        "Number every kind of part again, one run of numbers per letter.",
                                    )
                                    .clicked()
                                {
                                    pick = None;
                                    ui.close();
                                }
                                for prefix in &prefixes {
                                    if ui
                                        .selectable_label(
                                            pick.as_deref() == Some(prefix.as_str()),
                                            prefix,
                                        )
                                        .on_hover_text(
                                            "Only renumber the parts that start with this letter.",
                                        )
                                        .clicked()
                                    {
                                        pick = Some(prefix.clone());
                                        ui.close();
                                    }
                                }
                            })
                            .response
                            .on_hover_text(
                                "Renumber one kind of part, or every part at once.",
                            );
                        if ui
                            .button("Preview")
                            .on_hover_text(
                                "Show the new numbers, in placement order, before anything changes.",
                            )
                            .clicked()
                        {
                            let out = self.run_and_keep(
                                "components.reannotate",
                                json!({"dry_run": true, "prefix": pick}),
                            );
                            self.renumber_plan = out
                                .get("renamed")
                                .and_then(Value::as_array)
                                .map(|rows| {
                                    (
                                        pick.clone(),
                                        rows.iter()
                                            .filter_map(|row| {
                                                let from =
                                                    row.get("from")?.as_str()?.to_owned();
                                                let to = row.get("to")?.as_str()?.to_owned();
                                                Some((from, to))
                                            })
                                            .collect(),
                                    )
                                });
                        }
                    });
                    if self.renumber_plan.is_none() {
                        self.renumber_prefix = pick.clone();
                    }
                    if let Some((_, plan)) = self.renumber_plan.clone() {
                        for (from, to) in plan.iter().take(12) {
                            ui.label(
                                egui::RichText::new(format!("{from}  →  {to}"))
                                    .small()
                                    .color(self.muted()),
                            );
                        }
                        ui.horizontal(|ui| {
                            if ui
                                .button("Renumber")
                                .on_hover_text(
                                    "Write these numbers now; Undo puts the old ones back.",
                                )
                                .clicked()
                            {
                                self.renumber_plan = None;
                                self.run(
                                    "components.reannotate",
                                    json!({"prefix": pick}),
                                );
                            }
                            if ui
                                .button("Cancel")
                                .on_hover_text("Drop this plan; nothing changes.")
                                .clicked()
                            {
                                self.renumber_plan = None;
                            }
                        });
                    }
                }
            });
    }
    /// The Nets tab of the left dock: add, spotlight and rename connections.
    fn nets_tab(&mut self, ui: &mut egui::Ui) {
        if let Some(lit) = self.highlight_net
            && !self.project.nets.iter().any(|n| n.id == lit)
        {
            self.highlight_net = None;
        }
        if self
            .net_draft
            .as_ref()
            .is_some_and(|(id, _)| !self.project.nets.iter().any(|n| n.id == *id))
        {
            self.net_draft = None;
        }
        if self
            .net_editor
            .is_some_and(|id| !self.project.nets.iter().any(|n| n.id == id))
        {
            self.net_editor = None;
            self.pin_pick = None;
        }
        if ui
            .button("New net")
            .on_hover_text("Add a blank connection you can name and draw wires onto.")
            .clicked()
        {
            let name = format!("Net {}", self.project.nets.len() + 1);
            self.tool_call("net.create", json!({"name": name}));
            self.net_editor = self
                .project
                .nets
                .iter()
                .rev()
                .find(|n| n.name == name)
                .map(|n| n.id);
        }
        ui.add_space(4.0);
        let mut lengths: std::collections::BTreeMap<Id, (f64, usize)> = Default::default();
        for track in &self.project.board.tracks {
            let entry = lengths.entry(track.net).or_insert((0.0, 0));
            entry.0 += track_length(track);
        }
        for via in &self.project.board.vias {
            let entry = lengths.entry(via.net).or_insert((0.0, 0));
            entry.1 += 1;
        }
        if !self.project.nets.is_empty() {
            let total: f64 = lengths.values().map(|(length, _)| *length).sum();
            let routed = lengths.len();
            ui.label(
                egui::RichText::new(format!(
                    "{total:.1} mm of copper \u{b7} {routed} of {} nets routed",
                    self.project.nets.len()
                ))
                .small()
                .color(self.muted()),
            )
            .on_hover_text(
                "Total routed copper on the board, and how many connections still need tracks.",
            );
        }
        for net in self.project.nets.clone() {
            if !self.tree_matches(&net.name) {
                continue;
            }
            let lit = self.highlight_net == Some(net.id);
            let renaming = self.net_draft.as_ref().is_some_and(|(id, _)| *id == net.id);
            ui.horizontal(|ui| {
                if ui
                    .add(egui::Label::new(if lit { "\u{25cf}" } else { "\u{25cb}" }).sense(
                        egui::Sense::click(),
                    ))
                    .on_hover_text(
                        "Spotlight this net: its copper, vias and pads light up on the board while everything else fades. Click again to clear the spotlight.",
                    )
                    .clicked()
                {
                    self.highlight_net = if lit { None } else { Some(net.id) };
                }
                if renaming {
                    let Some((_, draft)) = self.net_draft.as_mut() else {
                        return;
                    };
                    ui.add(
                        egui::TextEdit::singleline(draft)
                            .desired_width(120.0)
                            .hint_text("net name"),
                    )
                    .on_hover_text("1 to 128 characters; every net needs its own name.");
                    if ui
                        .button("Use name")
                        .on_hover_text("Keep this name for the connection.")
                        .clicked()
                    {
                        let name = draft.clone();
                        self.net_draft = None;
                        self.tool_call("net.rename", json!({"id": net.id, "name": name}));
                    }
                    if ui
                        .button("Cancel")
                        .on_hover_text("Leave the name as it was.")
                        .clicked()
                    {
                        self.net_draft = None;
                    }
                } else {
                    if ui
                        .selectable_label(lit, &net.name)
                        .on_hover_text(
                            "Spotlight this net: its copper, vias and pads light up on the board while everything else fades. Click again to clear the spotlight.",
                        )
                        .clicked()
                    {
                        self.highlight_net = if lit { None } else { Some(net.id) };
                    }
                    if ui
                        .small_button("Rename")
                        .on_hover_text("Type a new name for this connection.")
                        .clicked()
                    {
                        self.net_draft = Some((net.id, net.name.clone()));
                    }
                    let open = self.net_editor == Some(net.id);
                    if ui
                        .small_button(if open { "Hide pins" } else { "Pins" })
                        .on_hover_text("Show or hide the list of pins on this connection.")
                        .clicked()
                    {
                        self.net_editor = if open { None } else { Some(net.id) };
                        self.pin_pick = None;
                    }
                    if let Some((length, vias)) = lengths.get(&net.id) {
                        ui.label(
                            egui::RichText::new(format!("{length:.1} mm \u{b7} {vias} vias"))
                                .small()
                                .color(self.muted()),
                        )
                        .on_hover_text(
                            "Copper already routed for this net on the board.",
                        );
                    } else {
                        ui.label(
                            egui::RichText::new("not routed yet")
                                .small()
                                .color(self.muted()),
                        )
                        .on_hover_text(
                            "This connection has no copper on the board yet; the dashed lines on the sheet show where it needs to go.",
                        );
                    }
                }
            });
            if self.net_editor == Some(net.id) {
                let mut members: Vec<(String, PinRef)> = Vec::new();
                let mut spares: Vec<(String, PinRef)> = Vec::new();
                for comp in &self.project.components {
                    for pin in &comp.pins {
                        let label = if pin.name.is_empty() || pin.name == pin.number {
                            format!("{} pin {}", comp.reference, pin.number)
                        } else {
                            format!("{} pin {} ({})", comp.reference, pin.number, pin.name)
                        };
                        let r = PinRef {
                            component: comp.id,
                            pin: pin.number.clone(),
                        };
                        if pin.net == Some(net.id) {
                            members.push((label, r));
                        } else if pin.net.is_none() {
                            spares.push((label, r));
                        }
                    }
                }
                let mut pick = self.pin_pick.clone().unwrap_or_default();
                ui.indent(("net-pins", net.id), |ui| {
                    if members.is_empty() {
                        ui.label(
                            egui::RichText::new("No pins on this connection yet.")
                                .size(10.0)
                                .color(self.muted()),
                        );
                    } else {
                        for (label, r) in &members {
                            ui.horizontal(|ui| {
                                ui.label(label);
                                if ui
                                    .small_button("\u{d7}")
                                    .on_hover_text(
                                        "Take this pin off the connection. Nothing moves on the board.",
                                    )
                                    .clicked()
                                {
                                    self.tool_call(
                                        "net.disconnect",
                                        json!({"net": net.id, "pins": [r]}),
                                    );
                                    self.pin_pick = None;
                                }
                            });
                        }
                    }
                    ui.horizontal(|ui| {
                        egui::ComboBox::from_id_salt(("pin-pick", net.id))
                            .selected_text(if pick.is_empty() {
                                "choose a spare pin".to_owned()
                            } else {
                                pick.clone()
                            })
                            .show_ui(ui, |ui| {
                                for (label, _) in &spares {
                                    if ui
                                        .selectable_label(&pick == label, label)
                                        .on_hover_text(
                                            "A pin that is not on any connection yet.",
                                        )
                                        .clicked()
                                    {
                                        pick = label.clone();
                                        ui.close();
                                    }
                                }
                            })
                            .response
                            .on_hover_text("Pick a pin to add to this connection.");
                        if ui
                            .add_enabled(
                                !pick.is_empty(),
                                egui::Button::new("Connect"),
                            )
                            .on_hover_text("Add the pin chosen above to this connection.")
                            .clicked()
                            && let Some((_, r)) =
                                spares.iter().find(|(label, _)| *label == pick)
                        {
                            self.tool_call("net.connect", json!({"net": net.id, "pins": [r]}));
                            self.pin_pick = None;
                        }
                    });
                    if spares.is_empty() {
                        ui.label(
                            egui::RichText::new(
                                "Every pin in this design is already on a connection.",
                            )
                            .size(10.0)
                            .color(self.muted()),
                        );
                    }
                });
                self.pin_pick = if pick.is_empty() { None } else { Some(pick) };
            }
        }
        if self.project.nets.is_empty() {
            empty_state(ui, EMPTY_NETS, self.muted());
        }
        if let Some(lit) = self.highlight_net
            && let Some(net) = self.project.nets.iter().find(|n| n.id == lit)
        {
            ui.label(
                egui::RichText::new(format!("Spotlighting {} on the board.", net.name))
                    .size(10.0)
                    .color(self.accent()),
            );
        }
    }
    /// Room for a full-page panel to breathe: wide side margins and one
    /// readable column, so nothing sits flush against the window edge.
    fn margined<T>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> T) -> T {
        egui::Frame::NONE
            .inner_margin(egui::Margin::symmetric(28, 20))
            .show(ui, |ui| {
                ui.set_max_width(720.0);
                add(ui)
            })
            .inner
    }

    /// Everything that used to crowd the right-hand aside: how the app looks
    /// and every setting for the project as a whole.
    fn settings_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading("Settings");
        ui.label("How the app looks, and everything that applies to the whole project.");
        ui.add_space(10.0);
        ui.label(
            egui::RichText::new("APPEARANCE")
                .size(10.0)
                .color(self.muted()),
        );
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.preferences.dark, true, "Dark")
                .on_hover_text("Dark canvas and panels, the usual working look.");
            ui.selectable_value(&mut self.preferences.dark, false, "Light")
                .on_hover_text("Light canvas and panels, easier on printed previews.");
        });
        ui.checkbox(&mut self.preferences.grid, "Show grid");
        ui.horizontal(|ui| {
            ui.label("Route on");
            ui.selectable_value(&mut self.route_layer, pcbcraft_doc::Layer::Front, "Top")
                .on_hover_text("New tracks are drawn on the top copper layer.");
            ui.selectable_value(&mut self.route_layer, pcbcraft_doc::Layer::Back, "Bottom")
                .on_hover_text("New tracks are drawn on the bottom copper layer.");
        });
        ui.horizontal(|ui| {
            ui.label("Pad labels");
            for labels in [PadLabels::Number, PadLabels::Net, PadLabels::Off] {
                ui.selectable_value(&mut self.preferences.pad_labels, labels, labels.name())
                    .on_hover_text("Choose what is drawn at each pad centre on the board.");
            }
        });
        ui.label("Accent color");
        ui.color_edit_button_srgb(&mut self.preferences.accent);
        ui.label(
            egui::RichText::new("Appearance is saved automatically.")
                .small()
                .color(self.muted()),
        );
        ui.add_space(16.0);
        ui.label(
            egui::RichText::new("BOARD GEOMETRY")
                .size(10.0)
                .color(self.muted()),
        );
        field_help(ui, FIELD_HELP_BOARD, self.muted());
        ui.horizontal(|ui| {
            ui.label("W");
            ui.add(
                egui::DragValue::new(&mut self.width)
                    .speed(0.5)
                    .suffix(" mm"),
            );
        });
        ui.horizontal(|ui| {
            ui.label("H");
            ui.add(
                egui::DragValue::new(&mut self.height)
                    .speed(0.5)
                    .suffix(" mm"),
            );
        });
        if ui
            .button("Apply board dimensions")
            .on_hover_text(command_tip("board.resize"))
            .clicked()
        {
            self.run(
                "board.resize",
                json!({"width":self.width,"height":self.height}),
            );
        }
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new("CUT-OUT HOLE")
                .size(10.0)
                .color(self.muted()),
        );
        ui.label(
            egui::RichText::new(
                "A round hole anywhere inside the board, for a sensor, a cable or a screw bigger than a drill bit.",
            )
            .small()
            .color(self.muted()),
        );
        ui.horizontal(|ui| {
            ui.label("X");
            ui.add(
                egui::DragValue::new(&mut self.cut_x)
                    .speed(0.5)
                    .suffix(" mm"),
            );
            ui.label("Y");
            ui.add(
                egui::DragValue::new(&mut self.cut_y)
                    .speed(0.5)
                    .suffix(" mm"),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Radius");
            ui.add(
                egui::DragValue::new(&mut self.cut_r)
                    .speed(0.1)
                    .range(0.1..=50.0)
                    .suffix(" mm"),
            );
        });
        if ui
            .button("Add hole")
            .on_hover_text(
                "Cuts this round hole into the board. The checks keep copper and other holes clear of it, and undo takes the hole out again.",
            )
            .clicked()
        {
            self.run(
                "board.cutout.add",
                json!({"center":{"x":self.cut_x,"y":self.cut_y},"radius":self.cut_r}),
            );
        }
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new("COPPER POUR")
                .size(10.0)
                .color(self.muted()),
        );
        ui.label(
            egui::RichText::new(
                "A solid rectangle of copper that joins every wire and pad inside it to one connection.",
            )
            .small()
            .color(self.muted()),
        );
        if !self.project.nets.is_empty() && !self.project.nets.iter().any(|n| n.id == self.pour_net)
        {
            self.pour_net = self.project.nets[0].id;
        }
        ui.horizontal(|ui| {
            ui.label("X");
            ui.add(
                egui::DragValue::new(&mut self.pour_x)
                    .speed(0.5)
                    .suffix(" mm"),
            );
            ui.label("Y");
            ui.add(
                egui::DragValue::new(&mut self.pour_y)
                    .speed(0.5)
                    .suffix(" mm"),
            );
        });
        ui.horizontal(|ui| {
            ui.label("W");
            ui.add(
                egui::DragValue::new(&mut self.pour_w)
                    .speed(0.5)
                    .range(1.0..=500.0)
                    .suffix(" mm"),
            );
            ui.label("H");
            ui.add(
                egui::DragValue::new(&mut self.pour_h)
                    .speed(0.5)
                    .range(1.0..=500.0)
                    .suffix(" mm"),
            );
        });
        let pour_net = self.pour_net;
        let net_choices: Vec<(Id, String)> = self
            .project
            .nets
            .iter()
            .take(64)
            .map(|n| (n.id, self.project.net_name(n.id).to_owned()))
            .collect();
        egui::ComboBox::from_id_salt("pour_net")
            .selected_text(format!("Copper for: {}", self.project.net_name(pour_net)))
            .show_ui(ui, |ui| {
                for (id, name) in net_choices {
                    ui.selectable_value(&mut self.pour_net, id, name);
                }
            });
        if ui
            .button("Pour copper")
            .on_hover_text(
                "Pours this rectangle of copper onto the board. It joins every wire and pad it touches to the chosen connection, and undo pours it away again.",
            )
            .clicked()
        {
            self.run(
                "zone.place",
                json!({"net": pour_net, "layer":"F.Cu","outline":[
                    {"x":self.pour_x,"y":self.pour_y},
                    {"x":self.pour_x+self.pour_w,"y":self.pour_y},
                    {"x":self.pour_x+self.pour_w,"y":self.pour_y+self.pour_h},
                    {"x":self.pour_x,"y":self.pour_y+self.pour_h},
                    {"x":self.pour_x,"y":self.pour_y},
                ]}),
            );
        }
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new("BOARD LABELS")
                .size(10.0)
                .color(self.muted()),
        );
        ui.label(
            egui::RichText::new("Words printed on the board: a version tag, a name, a warning.")
                .small()
                .color(self.muted()),
        );
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.label_text)
                    .desired_width(110.0)
                    .hint_text("what to print"),
            )
            .on_hover_text("The words to print on the board.");
        });
        ui.horizontal(|ui| {
            ui.label("At");
            ui.add(
                egui::DragValue::new(&mut self.label_x)
                    .speed(0.5)
                    .suffix(" mm"),
            );
            ui.add(
                egui::DragValue::new(&mut self.label_y)
                    .speed(0.5)
                    .suffix(" mm"),
            );
            ui.add(
                egui::DragValue::new(&mut self.label_size)
                    .range(0.3..=20.0)
                    .speed(0.1)
                    .suffix(" mm"),
            )
            .on_hover_text("How tall the letters are.");
        });
        ui.horizontal(|ui| {
            ui.label("Side");
            ui.selectable_value(&mut self.label_front, true, "Top");
            ui.selectable_value(&mut self.label_front, false, "Bottom");
        });
        if ui
            .button("Add label")
            .on_hover_text(
                "Prints these words on the board at the chosen spot. Undo erases it again.",
            )
            .clicked()
        {
            let text = self.label_text.trim().to_owned();
            if !text.is_empty() {
                self.run(
                    "text.place",
                    json!({
                        "text": text,
                        "position": {"x": self.label_x, "y": self.label_y},
                        "size": self.label_size,
                        "layer": if self.label_front { "F.Cu" } else { "B.Cu" },
                    }),
                );
            }
        }
        let labels: Vec<(Id, String, bool)> = self
            .project
            .board
            .texts
            .iter()
            .map(|t| (t.id, t.text.clone(), t.layer == pcbcraft_doc::Layer::Front))
            .collect();
        if !labels.is_empty() {
            ui.label(
                egui::RichText::new("Words on the board")
                    .small()
                    .color(self.muted()),
            );
            for (id, text, front) in labels {
                ui.horizontal(|ui| {
                    ui.label(format!("{text} · {}", if front { "Top" } else { "Bottom" }));
                    if ui
                        .small_button("Erase")
                        .on_hover_text("Wipe this word off the board.")
                        .clicked()
                    {
                        self.run("object.delete", json!({"id": id}));
                    }
                });
            }
        }
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new("BOARD EDGE")
                .size(10.0)
                .color(self.muted()),
        );
        let corners = self.project.board.outline.as_ref().map_or(0, |o| o.len());
        if corners >= 3 {
            ui.label(
                egui::RichText::new(format!("A free shape with {corners} corners."))
                    .small()
                    .color(self.muted()),
            );
            if ui
                .button("Back to a plain rectangle")
                .on_hover_text(
                    "Trade the drawn board edge for the simple rectangle sized by W and H above.",
                )
                .clicked()
            {
                self.run("board.clear_outline", json!({}));
            }
        }
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.svg_path)
                    .hint_text("drawing file path (.svg)")
                    .desired_width(150.0),
            )
            .on_hover_text(
                "A vector drawing whose edge can become the board outline, or whose shapes can be laid onto the board as artwork.",
            );
        });
        if ui
            .button("Import drawing")
            .on_hover_text(
                "Lay the shapes from the drawing file onto the board as editable artwork.",
            )
            .clicked()
        {
            self.run(
                "svg.import",
                json!({"path": self.svg_path.clone(), "use_outline": false}),
            );
        }
        if ui
            .button("Import as board edge")
            .on_hover_text("Trace the closed shape in the drawing file into the board outline.")
            .clicked()
        {
            self.run(
                "svg.import",
                json!({"path": self.svg_path.clone(), "use_outline": true}),
            );
        }
        ui.horizontal(|ui| {
            ui.label("Cutout radius");
            ui.add(
                egui::DragValue::new(&mut self.cutout_radius)
                    .speed(0.1)
                    .suffix(" mm"),
            );
        });
        field_help(ui, FIELD_HELP_CUTOUT, self.muted());
        ui.add_space(12.0);
        ui.label(egui::RichText::new("CHECKS").size(10.0).color(self.muted()));
        let erc = self
            .checks
            .get("erc")
            .and_then(Value::as_array)
            .map(Vec::len)
            .unwrap_or(0);
        let drc = self
            .checks
            .get("drc")
            .and_then(Value::as_array)
            .map(Vec::len)
            .unwrap_or(0);
        if ui
            .button(format!("{erc} electrical · {drc} board >"))
            .on_hover_text("Open the checks tab for the full list of findings.")
            .clicked()
        {
            self.change_tab(Tab::Checks);
        }
        ui.label(
            egui::RichText::new("Initial checks; full ERC/DRC is in development.")
                .small()
                .color(self.muted()),
        );
        ui.add_space(12.0);
        ui.label(
            egui::RichText::new("MAKE FILES")
                .size(10.0)
                .color(self.muted()),
        );
        ui.label(
            egui::RichText::new(
                "Plain files for the board factory, written beside your design unless you name another folder.",
            )
            .small()
            .color(self.muted()),
        );
        ui.add(
            egui::TextEdit::singleline(&mut self.make_dir)
                .hint_text("folder for the files (blank means \"outputs\" beside the design)")
                .desired_width(220.0),
        )
        .on_hover_text(
            "Where the make files land. Leave blank to use a folder named outputs next to the design file.",
        );
        if ui
            .button("Copper & drill files")
            .on_hover_text(
                "The Gerber copper layers, board outline and drill file a board house asks for.",
            )
            .clicked()
        {
            self.write_make_files("gerber.export");
        }
        if ui
            .button("Parts list (CSV)")
            .on_hover_text(
                "A spreadsheet of every part: one row per unique part with all its references.",
            )
            .clicked()
        {
            self.write_make_files("bom.write");
        }
        if ui
            .button("Net list")
            .on_hover_text("A plain text list of every connection and the pins on it.")
            .clicked()
        {
            self.write_make_files("netlist.write");
        }
        if ui
            .button("Pick and place file")
            .on_hover_text(
                "Where every part sits, which way it faces and which side it goes on, for the assembly machine.",
            )
            .clicked()
        {
            self.write_make_files("placement.write");
        }
        if ui
            .button("Design report")
            .on_hover_text(
                "A readable summary of the whole design: board, every part and every connection.",
            )
            .clicked()
        {
            self.write_make_files("report.write");
        }
        if ui
            .button("Drill map (CSV)")
            .on_hover_text(
                "Every hole on the board counted and grouped by size, so you can sanity-check the drill sizes before the board goes away.",
            )
            .clicked()
        {
            self.write_job_file("drillmap", "drill_chart", "Drill map");
        }
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new("BOARD DRAWINGS")
                .size(10.0)
                .color(self.muted()),
        );
        ui.label(
            egui::RichText::new(
                "Flat drawings and 3D models, for your records, your fab or a web page.",
            )
            .small()
            .color(self.muted()),
        );
        if ui
            .button("3D model (Step)")
            .on_hover_text(
                "The standard 3D file for mechanical programs and free viewers. It lands in the make folder.",
            )
            .clicked()
        {
            self.write_job_file("step3d", "step", "3D model");
        }
        if ui
            .button("3D viewer file")
            .on_hover_text(
                "A lighter 3D file that simple viewers open quickly. It lands in the make folder.",
            )
            .clicked()
        {
            self.write_job_file("wrl3d", "vrml", "3D viewer file");
        }
        if ui
            .button("Printable drawing (PDF)")
            .on_hover_text(
                "A one-page drawing of the finished board you can print or email. It lands in the make folder.",
            )
            .clicked()
        {
            self.write_job_file("pdfdraw", "pdf", "Printable drawing");
        }
        if ui
            .button("Web drawing (SVG)")
            .on_hover_text(
                "A crisp picture of the board that any web browser can show, at any size. It lands in the make folder.",
            )
            .clicked()
        {
            self.write_job_file("svgdraw", "svg", "Web drawing");
        }
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new("PANEL PLAN")
                .size(10.0)
                .color(self.muted()),
        );
        ui.label(
            egui::RichText::new(
                "See how your board would sit repeated on a bigger manufacturing panel.",
            )
            .small()
            .color(self.muted()),
        );
        ui.horizontal(|ui| {
            ui.add(
                egui::DragValue::new(&mut self.panel_columns)
                    .range(1..=32)
                    .suffix(" across")
                    .speed(1.0),
            )
            .on_hover_text("How many copies of the board sit side by side.");
            ui.add(
                egui::DragValue::new(&mut self.panel_rows)
                    .range(1..=32)
                    .suffix(" down")
                    .speed(1.0),
            )
            .on_hover_text("How many copies of the board sit top to bottom.");
        });
        ui.horizontal(|ui| {
            ui.add(
                egui::DragValue::new(&mut self.panel_gap)
                    .range(0.0..=25.0)
                    .speed(0.5)
                    .suffix(" mm gap"),
            )
            .on_hover_text(
                "Empty space between neighbouring copies, room for cutting them apart later.",
            );
            ui.add(
                egui::DragValue::new(&mut self.panel_rail)
                    .range(0.0..=25.0)
                    .speed(0.5)
                    .suffix(" mm rail"),
            )
            .on_hover_text(
                "Solid border left around the edge of the panel, where the factory grips it.",
            );
        });
        if ui
            .button("Preview panel")
            .on_hover_text(
                "Work out the panel size and where every copy lands, using the board's own measurements. The design itself is left alone.",
            )
            .clicked()
        {
            self.panel_plan = self.run_and_keep(
                "board.panel",
                json!({
                    "columns": self.panel_columns,
                    "rows": self.panel_rows,
                    "pitch_x": self.panel_gap,
                    "pitch_y": self.panel_gap,
                    "rail": self.panel_rail,
                }),
            );
        }
        if let Some(plan) = self.panel_plan.get("panel") {
            let copies = self
                .panel_plan
                .get("copies")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let width = plan.get("width").and_then(Value::as_f64).unwrap_or(0.0);
            let height = plan.get("height").and_then(Value::as_f64).unwrap_or(0.0);
            ui.label(
                egui::RichText::new(format!(
                    "{copies} copies \u{b7} panel {width:.1} \u{d7} {height:.1} mm"
                ))
                .small()
                .color(self.muted()),
            )
            .on_hover_text("The finished panel: its overall size and how many boards fit on it.");
        }
        ui.label(
            egui::RichText::new("Writing these files changes nothing in the design.")
                .small()
                .color(self.muted()),
        );
    }

    fn rules_tab(&mut self, ui: &mut egui::Ui) {
        if self
            .rule_draft
            .is_some_and(|(id, _, _)| !self.project.net_classes.iter().any(|k| k.id == id))
        {
            self.rule_draft = None;
        }
        ui.label(
            egui::RichText::new(
                "Design rules travel with the design and steer the checks and any new wiring.",
            )
            .small()
            .color(self.muted()),
        );
        if let Some(pick) = &self.class_pick
            && !self.project.nets.iter().any(|n| n.name == *pick)
        {
            self.class_pick = None;
        }
        if ui
            .button("New class")
            .on_hover_text(
                "Add a rule set with a starting gap and wire width; rename and tune it from its row.",
            )
            .clicked()
        {
            let name = format!("Class {}", self.project.net_classes.len() + 1);
            self.run(
                "netclass.add",
                json!({"name": name, "clearance": 0.2, "width": 0.25}),
            );
        }
        ui.add_space(4.0);
        for class in self.project.net_classes.clone() {
            let editing = self.rule_draft.is_some_and(|(id, _, _)| id == class.id);
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(&class.name).strong());
                    ui.label(format!("{} nets", class.nets.len())).on_hover_text(
                        "Connections that follow this rule set. Put them there with the box below.",
                    );
                    if !editing {
                        if ui
                            .small_button("Tune")
                            .on_hover_text("Change this rule set's gap and wire width.")
                            .clicked()
                        {
                            self.rule_draft =
                                Some((class.id, class.clearance, class.width));
                        }
                        if ui
                            .small_button("Remove")
                            .on_hover_text(
                                "Delete this rule set; its connections fall back to the default rules.",
                            )
                            .clicked()
                        {
                            self.run("netclass.remove", json!({"class": class.id}));
                        }
                    }
                });
                if editing {
                    let Some((_, clearance, width)) = self.rule_draft.as_mut() else {
                        return;
                    };
                    ui.horizontal(|ui| {
                        ui.label("Gap");
                        ui.add(
                            egui::DragValue::new(clearance)
                                .speed(0.01)
                                .range(0.001..=100.0)
                                .suffix(" mm"),
                        )
                        .on_hover_text(
                            "Smallest allowed clear space between this group's copper and everything else.",
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.label("Wire width");
                        ui.add(
                            egui::DragValue::new(width)
                                .speed(0.01)
                                .range(0.001..=100.0)
                                .suffix(" mm"),
                        )
                        .on_hover_text(
                            "Width new tracks on these connections start with.",
                        );
                    });
                    ui.horizontal(|ui| {
                        if ui
                            .button("Apply")
                            .on_hover_text("Keep these numbers for this rule set.")
                            .clicked()
                            && let Some((id, clearance, width)) = self.rule_draft.take()
                        {
                            self.run(
                                "netclass.clearance",
                                json!({"class": id, "clearance": clearance}),
                            );
                            self.run("netclass.set", json!({"class": id, "width": width}));
                        }
                        if ui
                            .small_button("Cancel")
                            .on_hover_text("Leave this rule set as it was.")
                            .clicked()
                        {
                            self.rule_draft = None;
                        }
                    });
                } else {
                    ui.label(
                        egui::RichText::new(format!(
                            "Gap {:.3} mm · wire {:.3} mm",
                            class.clearance, class.width
                        ))
                        .small()
                        .color(self.muted()),
                    );
                    let taken: Vec<pcbcraft_doc::Id> = self
                        .project
                        .net_classes
                        .iter()
                        .flat_map(|k| k.nets.clone())
                        .collect();
                    let free: Vec<(String, pcbcraft_doc::Id)> = self
                        .project
                        .nets
                        .iter()
                        .filter(|n| !taken.contains(&n.id))
                        .map(|n| (n.name.clone(), n.id))
                        .collect();
                    let mut pick = self.class_pick.clone().unwrap_or_default();
                    ui.horizontal(|ui| {
                        egui::ComboBox::from_id_salt(("class-pick", class.id))
                            .selected_text(if pick.is_empty() {
                                "choose a connection to put here".to_owned()
                            } else {
                                pick.clone()
                            })
                            .show_ui(ui, |ui| {
                                for (name, _) in &free {
                                    if ui
                                        .selectable_label(&pick == name, name)
                                        .on_hover_text(
                                            "A connection that follows no rule set yet.",
                                        )
                                        .clicked()
                                    {
                                        pick = name.clone();
                                        ui.close();
                                    }
                                }
                            })
                            .response
                            .on_hover_text(
                                "Pick a connection, then Assign: its wiring follows this rule set.",
                            );
                        if ui
                            .add_enabled(
                                !pick.is_empty(),
                                egui::Button::new("Assign"),
                            )
                            .on_hover_text(
                                "Put the chosen connection under this rule set.",
                            )
                            .clicked()
                            && let Some((_, id)) = free.iter().find(|(name, _)| *name == pick)
                        {
                            self.run("netclass.assign", json!({"net": id, "class": class.id}));
                            self.class_pick = None;
                        }
                    });
                    self.class_pick = if pick.is_empty() {
                        None
                    } else {
                        Some(pick)
                    };
                    if free.is_empty() && class.nets.is_empty() {
                        ui.label(
                            egui::RichText::new("Every connection already follows a rule set.")
                                .small()
                                .color(self.muted()),
                        );
                    }
                    for net_id in &class.nets {
                        let Some(name) = self
                            .project
                            .nets
                            .iter()
                            .find(|n| n.id == *net_id)
                            .map(|n| n.name.clone())
                        else {
                            continue;
                        };
                        ui.horizontal(|ui| {
                            ui.label("· ");
                            ui.label(&name);
                            if ui
                                .small_button("\u{d7}")
                                .on_hover_text(
                                    "Let this connection follow the default rules again.",
                                )
                                .clicked()
                            {
                                self.run("netclass.assign", json!({"net": net_id, "class": null}));
                            }
                        });
                    }
                }
            });
        }
        if self.project.net_classes.is_empty() {
            ui.label(
                egui::RichText::new(
                    "No rule sets yet; the default rules apply to every connection.",
                )
                .small()
                .color(self.muted()),
            );
        }
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new("TEARDROPS")
                .size(10.0)
                .color(self.muted()),
        );
        ui.label(
            egui::RichText::new(
                "Stem-shaped copper where wires meet pads and vias, so the joints survive etching and soldering.",
            )
            .small()
            .color(self.muted()),
        );
        ui.horizontal(|ui| {
            ui.add(
                egui::DragValue::new(&mut self.teardrop_depth)
                    .range(0.05..=5.0)
                    .speed(0.05)
                    .suffix(" mm deep"),
            )
            .on_hover_text(
                "How far each teardrop runs along the wire from the joint. Bigger stems are sturdier but eat board space.",
            );
            if ui
                .button("Harden junctions")
                .on_hover_text(
                    "Grow a teardrop at every pad and via junction in one go. Undo brings the plain wires back.",
                )
                .clicked()
            {
                self.run("teardrop.add", json!({"depth": self.teardrop_depth}));
            }
            if ui
                .small_button("Remove teardrops")
                .on_hover_text(
                    "Take the teardrop copper away again, leaving the ordinary wires exactly as they were.",
                )
                .clicked()
            {
                self.run("teardrop.remove", json!({}));
            }
        });
        ui.add_space(12.0);
        ui.label(
            egui::RichText::new("CHECKS YOU WRITE")
                .size(10.0)
                .color(self.muted()),
        );
        ui.label(
            egui::RichText::new(
                "Rules only you know about: pick one plain measurement, set one limit, and it runs with the other checks after every edit. It travels inside the design, so anyone who opens it is covered too.",
            )
            .small()
            .color(self.muted()),
        );
        for c in self.project.custom_checks.clone() {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "{} · {} · {}",
                        c.name,
                        custom_measure_line(&c.measure, c.limit),
                        if c.severity == "error" {
                            "must fix"
                        } else {
                            "a nudge"
                        }
                    ))
                    .small(),
                )
                .on_hover_text(
                    "A check you wrote. It is saved with the design and re-run after every edit.",
                );
                if ui
                    .small_button("Remove this check")
                    .on_hover_text(
                        "Stop running this check. Nothing about the design itself changes.",
                    )
                    .clicked()
                {
                    self.run("checks.custom_remove", json!({"id": c.id}));
                }
            });
        }
        if self.project.custom_checks.len() < 32 {
            ui.horizontal(|ui| {
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.custom_name)
                        .hint_text("What this check means, in your words")
                        .desired_width(150.0),
                );
                response
                    .on_hover_text("The name this check reports under, e.g. \"keep power wires meaty\".");
                egui::ComboBox::from_id_salt("custom-measure")
                    .selected_text(CUSTOM_MEASURES[self.custom_measure.min(4)].0)
                    .show_ui(ui, |ui| {
                        for (i, (label, _)) in CUSTOM_MEASURES.iter().enumerate() {
                            ui.selectable_value(&mut self.custom_measure, i, *label);
                        }
                    })
                    .response
                    .on_hover_text(
                        "The one thing this check measures. Every check stays a plain sentence, never a puzzle.",
                    );
                let unit = if CUSTOM_MEASURES[self.custom_measure.min(4)]
                    .1
                    .contains("count")
                {
                    ""
                } else {
                    " mm"
                };
                ui.add(
                    egui::DragValue::new(&mut self.custom_limit)
                        .range(0.01..=100000.0)
                        .speed(0.05)
                        .suffix(unit),
                )
                .on_hover_text(
                    "The limit: millimetres for wire measures, a plain number of things otherwise.",
                );
                if ui
                    .selectable_label(self.custom_loud, "must fix")
                    .on_hover_text("Treat a breach as a real problem, counted under \"to fix\".")
                    .clicked()
                {
                    self.custom_loud = true;
                }
                if ui
                    .selectable_label(!self.custom_loud, "a nudge")
                    .on_hover_text("Mention a breach, but do not call the board a failure.")
                    .clicked()
                {
                    self.custom_loud = false;
                }
            });
            if ui
                .button("Add this check")
                .on_hover_text(
                    "Start running this rule after every edit. It lives inside the design; Undo takes it away again.",
                )
                .clicked()
                && !self.custom_name.trim().is_empty()
            {
                let measure = CUSTOM_MEASURES[self.custom_measure.min(4)].1;
                self.run(
                    "checks.custom_add",
                    json!({
                        "name": self.custom_name,
                        "measure": measure,
                        "limit": self.custom_limit,
                        "severity": if self.custom_loud { "error" } else { "warning" },
                    }),
                );
                self.custom_name.clear();
            }
        }
        ui.add_space(12.0);
        ui.label(
            egui::RichText::new("HOW STRICT TO BE")
                .size(10.0)
                .color(self.muted()),
        );
        ui.label(
            egui::RichText::new(
                "Choose what each automatic check does when it spots something: stop you, warn you, or stay quiet.",
            )
            .small()
            .color(self.muted()),
        );
        for code in [
            "unassigned_pin",
            "single_pin_net",
            "clearance",
            "copper_short",
            "board_edge",
            "track_width",
            "track_length",
            "unrouted",
            "missing_footprint",
        ] {
            let current = self.project.rule_severity(code, "warning");
            let nice = code.replace('_', " ");
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(&nice).small().strong());
                for level in ["error", "warning", "off"] {
                    let tip = match level {
                        "error" => {
                            "Treat this as a real problem to fix before the board goes away."
                        }
                        "warning" => "Mention it, but do not call the board a failure.",
                        _ => "Stop reporting this check at all.",
                    };
                    if ui
                        .selectable_label(current == level, level)
                        .on_hover_text(tip)
                        .clicked()
                    {
                        self.run("rules.severity", json!({"code": code, "severity": level}));
                    }
                }
                if ui
                    .small_button("Auto")
                    .on_hover_text("Go back to what this check does out of the box.")
                    .clicked()
                {
                    self.run("rules.severity", json!({"code": code, "severity": null}));
                }
            });
        }
        ui.add_space(10.0);
        ui.label(
            egui::RichText::new("PAIR CLEARANCES")
                .size(10.0)
                .color(self.muted()),
        );
        ui.label(
            egui::RichText::new(
                "Extra room between two connections that must keep their distance, like a noisy one next to a sensitive one.",
            )
            .small()
            .color(self.muted()),
        );
        let nets: Vec<(Id, String)> = self
            .project
            .nets
            .iter()
            .take(64)
            .map(|n| (n.id, self.project.net_name(n.id).to_owned()))
            .collect();
        let name_of = |wanted: Id| {
            nets.iter()
                .find(|(id, _)| *id == wanted)
                .map(|(_, name)| name.clone())
                .unwrap_or_else(|| format!("net {wanted}"))
        };
        if nets.len() >= 2 {
            if !nets.iter().any(|(id, _)| *id == self.pair_a) {
                self.pair_a = nets[0].0;
            }
            if !nets.iter().any(|(id, _)| *id == self.pair_b) || self.pair_b == self.pair_a {
                self.pair_b = nets
                    .iter()
                    .map(|(id, _)| *id)
                    .find(|id| *id != self.pair_a)
                    .unwrap_or(self.pair_b);
            }
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt("pair_a")
                    .selected_text(name_of(self.pair_a))
                    .show_ui(ui, |ui| {
                        for (id, name) in &nets {
                            ui.selectable_value(&mut self.pair_a, *id, name)
                                .on_hover_text("Pick the first net of the pair.");
                        }
                    });
                ui.label("away from");
                egui::ComboBox::from_id_salt("pair_b")
                    .selected_text(name_of(self.pair_b))
                    .show_ui(ui, |ui| {
                        for (id, name) in &nets {
                            ui.selectable_value(&mut self.pair_b, *id, name)
                                .on_hover_text("Pick the second net of the pair.");
                        }
                    });
            });
            ui.horizontal(|ui| {
                ui.label("Keep apart by");
                ui.add(
                    egui::DragValue::new(&mut self.pair_clearance)
                        .range(0.05..=25.0)
                        .speed(0.05)
                        .suffix(" mm"),
                )
                .on_hover_text(
                    "The least distance the two connections' copper may sit from each other.",
                );
            });
            if ui
                .button("Set spacing")
                .on_hover_text(
                    "Tell the checks these two connections must stay this far apart everywhere on the board. Undo takes the rule away.",
                )
                .clicked()
            {
                self.run(
                    "board.set_pair_rule",
                    json!({"net_a": self.pair_a, "net_b": self.pair_b, "clearance": self.pair_clearance}),
                );
            }
        }
        let pairs: Vec<(Id, Id, f64)> = self
            .project
            .board
            .pair_rules
            .iter()
            .map(|r| (r.a, r.b, r.clearance))
            .collect();
        if !pairs.is_empty() {
            ui.label(
                egui::RichText::new("Spacing rules you have set between connections")
                    .small()
                    .color(self.muted()),
            );
            for (a, b, clearance) in pairs {
                ui.horizontal(|ui| {
                    ui.label(format!(
                        "{} away from {} · {clearance} mm",
                        name_of(a),
                        name_of(b)
                    ));
                    if ui
                        .small_button("Clear")
                        .on_hover_text(
                            "Let these two connections fall back to the normal spacing rules.",
                        )
                        .clicked()
                    {
                        self.run("board.remove_pair_rule", json!({"net_a": a, "net_b": b}));
                    }
                });
            }
        }
        ui.add_space(10.0);
        ui.label(
            egui::RichText::new("DIFFERENTIAL PAIRS")
                .size(10.0)
                .color(self.muted()),
        );
        ui.label(
            egui::RichText::new(
                "Two connections that should run side by side as a team, like the data lines of a socket.",
            )
            .small()
            .color(self.muted()),
        );
        if nets.len() >= 2 {
            if !nets.iter().any(|(id, _)| *id == self.dp_p) {
                self.dp_p = nets[0].0;
            }
            if !nets.iter().any(|(id, _)| *id == self.dp_n) || self.dp_n == self.dp_p {
                self.dp_n = nets
                    .iter()
                    .map(|(id, _)| *id)
                    .find(|id| *id != self.dp_p)
                    .unwrap_or(self.dp_n);
            }
            ui.horizontal(|ui| {
                ui.label("Name");
                ui.add(
                    egui::TextEdit::singleline(&mut self.dp_name)
                        .hint_text("USB")
                        .desired_width(90.0),
                )
                .on_hover_text("A name you will recognise, like USB or HDMI.");
            });
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt("dp_p")
                    .selected_text(name_of(self.dp_p))
                    .show_ui(ui, |ui| {
                        for (id, name) in &nets {
                            ui.selectable_value(&mut self.dp_p, *id, name)
                                .on_hover_text("Pick the net that carries the positive side.");
                        }
                    });
                ui.label("+");
                egui::ComboBox::from_id_salt("dp_n")
                    .selected_text(name_of(self.dp_n))
                    .show_ui(ui, |ui| {
                        for (id, name) in &nets {
                            ui.selectable_value(&mut self.dp_n, *id, name)
                                .on_hover_text("Pick the net that carries the negative side.");
                        }
                    });
            });
            ui.horizontal(|ui| {
                ui.label("Run together with a gap of");
                ui.add(
                    egui::DragValue::new(&mut self.dp_gap)
                        .range(0.02..=10.0)
                        .speed(0.02)
                        .suffix(" mm"),
                )
                .on_hover_text(
                    "How closely the two sides should hug each other; copper nearer than this gets reported.",
                );
            });
            if ui
                .button("Declare pair")
                .on_hover_text(
                    "Tell the checks these two connections are a team: they watch the gap between them. Reusing the name updates the same pair, and undo takes it away.",
                )
                .clicked()
            {
                self.run(
                    "board.set_diffpair",
                    json!({"name": self.dp_name, "net_p": self.dp_p, "net_n": self.dp_n, "gap": self.dp_gap}),
                );
            }
        }
        let pairs: Vec<(Id, String, Id, Id, f64)> = self
            .project
            .board
            .diff_pairs
            .iter()
            .map(|d| (d.id, d.name.clone(), d.net_p, d.net_n, d.gap))
            .collect();
        if !pairs.is_empty() {
            ui.label(
                egui::RichText::new("Pairs you have declared")
                    .small()
                    .color(self.muted()),
            );
            for (id, name, p, n, gap) in pairs {
                ui.horizontal(|ui| {
                    ui.label(format!(
                        "{name} · {} + {} · gap {gap} mm",
                        name_of(p),
                        name_of(n)
                    ));
                    if ui
                        .small_button("Remove")
                        .on_hover_text("Stop watching this pair; the lines stay as they are.")
                        .clicked()
                    {
                        self.run("board.remove_diffpair", json!({"id": id}));
                    }
                });
            }
        }
        ui.add_space(10.0);
        ui.label(
            egui::RichText::new("BOARD AREAS")
                .size(10.0)
                .color(self.muted()),
        );
        ui.label(
            egui::RichText::new(
                "A corner of the board that plays by stricter rules: wider gaps, and optionally a height limit for what fits under a cover.",
            )
            .small()
            .color(self.muted()),
        );
        ui.horizontal(|ui| {
            ui.label("Corner");
            ui.add(
                egui::DragValue::new(&mut self.area_x)
                    .speed(0.5)
                    .suffix(" mm"),
            );
            ui.add(
                egui::DragValue::new(&mut self.area_y)
                    .speed(0.5)
                    .suffix(" mm"),
            );
            ui.label("Size");
            ui.add(
                egui::DragValue::new(&mut self.area_w)
                    .speed(0.5)
                    .range(1.0..=500.0)
                    .suffix(" mm"),
            );
            ui.add(
                egui::DragValue::new(&mut self.area_h)
                    .speed(0.5)
                    .range(1.0..=500.0)
                    .suffix(" mm"),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Copper stays apart by at least");
            ui.add(
                egui::DragValue::new(&mut self.area_gap)
                    .range(0.05..=25.0)
                    .speed(0.05)
                    .suffix(" mm"),
            )
            .on_hover_text(
                "The minimum separation demanded between copper pieces inside this area.",
            );
        });
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.area_limited, "Parts here stand no taller than")
                .on_hover_text(
                    "For boards that must fit under a cover: anything placed here taller than the limit is reported.",
                );
            if self.area_limited {
                ui.add(
                    egui::DragValue::new(&mut self.area_height)
                        .range(0.1..=100.0)
                        .speed(0.5)
                        .suffix(" mm"),
                );
            }
        });
        if ui
            .button("Add area")
            .on_hover_text(
                "Marks this rectangle on the board with its own stricter rules. The checks watch it from now on, and undo erases the mark.",
            )
            .clicked()
        {
            let mut args = json!({
                "clearance": self.area_gap,
                "x": self.area_x,
                "y": self.area_y,
                "width": self.area_w,
                "height": self.area_h,
            });
            if self.area_limited {
                args["max_height"] = json!(self.area_height);
            }
            self.run("rules.area_add", args);
        }
        let areas: Vec<(Id, String, f64, Option<f64>)> = self
            .project
            .board
            .rule_areas
            .iter()
            .map(|a| {
                (
                    a.id,
                    a.name.clone().unwrap_or_else(|| {
                        format!("area at {}, {}", a.region.x.round(), a.region.y.round())
                    }),
                    a.clearance,
                    a.max_height,
                )
            })
            .collect();
        if !areas.is_empty() {
            ui.label(
                egui::RichText::new("Stricter areas on the board")
                    .small()
                    .color(self.muted()),
            );
            for (id, name, clearance, max_height) in areas {
                ui.horizontal(|ui| {
                    ui.label(format!(
                        "{name} · gap {clearance} mm{}",
                        max_height.map_or(String::new(), |h| format!(" · up to {h} mm tall"))
                    ));
                    if ui
                        .small_button("Erase area")
                        .on_hover_text(
                            "This corner goes back to the board's normal rules; the copper stays as it is.",
                        )
                        .clicked()
                    {
                        self.run("rules.area_remove", json!({"id": id}));
                    }
                });
            }
        }
    }
    fn builds_tab(&mut self, ui: &mut egui::Ui) {
        if let Some((name, _)) = &self.build_note
            && !self.project.variants.iter().any(|v| v.name == *name)
        {
            self.build_note = None;
        }
        if let Some(pick) = &self.build_pick
            && !self.project.components.iter().any(|c| c.reference == *pick)
        {
            self.build_pick = None;
        }
        if let Some(pick) = &self.swap_pick
            && !self.project.components.iter().any(|c| c.reference == *pick)
        {
            self.swap_pick = None;
        }
        ui.label(
            egui::RichText::new(
                "One design, several builds: each extra build lists parts it leaves off the board.",
            )
            .small()
            .color(self.muted()),
        );
        if ui
            .button("New build")
            .on_hover_text(
                "Start an extra version of this board; say which parts to leave off from its row.",
            )
            .clicked()
        {
            let name = format!("Build {}", self.project.variants.len() + 1);
            self.run("variant.create", json!({"name": name}));
        }
        ui.add_space(4.0);
        for variant in self.project.variants.clone() {
            let editing = self
                .build_note
                .as_ref()
                .is_some_and(|(name, _)| *name == variant.name);
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(&variant.name).strong());
                    ui.label(format!("{} left off", variant.dnp.len())).on_hover_text(
                        "Parts this build skips; the bill of materials leaves them out too.",
                    );
                    if !variant.footprints.is_empty() {
                        ui.label(format!("{} swapped", variant.footprints.len())).on_hover_text(
                            "Parts this build fits in a different package.",
                        );
                    }
                    if !editing {
                        if ui
                            .small_button("Note")
                            .on_hover_text(
                                "Write a short reminder of what makes this build different.",
                            )
                            .clicked()
                        {
                            self.build_note =
                                Some((variant.name.clone(), variant.note.clone().unwrap_or_default()));
                        }
                        if ui
                            .small_button("Remove")
                            .on_hover_text(
                                "Delete this extra build; the design itself does not change.",
                            )
                            .clicked()
                        {
                            self.run("variant.remove", json!({"name": variant.name}));
                        }
                    }
                });
                if let Some(note) = &variant.note
                    && !editing
                {
                    ui.label(
                        egui::RichText::new(note).small().color(self.muted()),
                    );
                }
                if editing {
                    let mut text = self
                        .build_note
                        .as_ref()
                        .map(|(_, note)| note.clone())
                        .unwrap_or_default();
                    ui.add(
                        egui::TextEdit::multiline(&mut text)
                            .desired_width(200.0)
                            .hint_text("what this build changes"),
                    )
                    .on_hover_text("A sentence or two, saved with the design.");
                    let save = ui
                        .button("Save note")
                        .on_hover_text("Keep this reminder on the build.")
                        .clicked();
                    let cancel = ui
                        .small_button("Cancel")
                        .on_hover_text("Leave the build's reminder as it was.")
                        .clicked();
                    if save {
                        self.build_note = None;
                        let note = if text.trim().is_empty() {
                            None
                        } else {
                            Some(text.trim().to_owned())
                        };
                        self.run("variant.set_note", json!({"name": variant.name, "note": note}));
                    } else if cancel {
                        self.build_note = None;
                    } else if let Some((_, held)) = self.build_note.as_mut() {
                        *held = text;
                    }
                } else {
                    for id in &variant.dnp {
                        let Some(reference) = self
                            .project
                            .components
                            .iter()
                            .find(|c| c.id == *id)
                            .map(|c| c.reference.clone())
                        else {
                            continue;
                        };
                        ui.horizontal(|ui| {
                            ui.label("· ");
                            ui.label(&reference);
                            if ui
                                .small_button("\u{d7}")
                                .on_hover_text(
                                    "Put this part back on for this build.",
                                )
                                .clicked()
                            {
                                self.run(
                                    "variant.set_dnp",
                                    json!({"name": variant.name, "ids": [id], "dnp": false}),
                                );
                            }
                        });
                    }
                    let free: Vec<String> = self
                        .project
                        .components
                        .iter()
                        .filter(|c| !variant.dnp.contains(&c.id))
                        .map(|c| c.reference.clone())
                        .collect();
                    let mut pick = self.build_pick.clone().unwrap_or_default();
                    ui.horizontal(|ui| {
                        egui::ComboBox::from_id_salt(("build-pick", variant.name.clone()))
                            .selected_text(if pick.is_empty() {
                                "choose a part to leave off".to_owned()
                            } else {
                                pick.clone()
                            })
                            .show_ui(ui, |ui| {
                                for reference in &free {
                                    if ui
                                        .selectable_label(&pick == reference, reference)
                                        .on_hover_text("A part this build still populates.")
                                        .clicked()
                                    {
                                        pick = reference.clone();
                                        ui.close();
                                    }
                                }
                            })
                            .response
                            .on_hover_text(
                                "Pick a part, then Leave off: this build skips it and its parts list leaves it out.",
                            );
                        if ui
                            .add_enabled(!pick.is_empty(), egui::Button::new("Leave off"))
                            .on_hover_text(
                                "This build skips the chosen part; nothing is deleted.",
                            )
                            .clicked()
                            && let Some(component) = self
                                .project
                                .components
                                .iter()
                                .find(|c| c.reference == pick)
                                .map(|c| c.id)
                        {
                            self.run(
                                "variant.set_dnp",
                                json!({"name": variant.name, "ids": [component], "dnp": true}),
                            );
                            self.build_pick = None;
                            pick.clear();
                        }
                    });
                    self.build_pick = if pick.is_empty() {
                        None
                    } else {
                        Some(pick)
                    };
                    if free.is_empty() {
                        ui.label(
                            egui::RichText::new("Every part in this design is left off.")
                                .small()
                                .color(self.muted()),
                        );
                    }
                    for swap in &variant.footprints {
                        let Some(reference) = self
                            .project
                            .components
                            .iter()
                            .find(|c| c.id == swap.component)
                            .map(|c| c.reference.clone())
                        else {
                            continue;
                        };
                        ui.horizontal(|ui| {
                            ui.label("· ");
                            ui.label(format!("{reference} fits {}", swap.footprint));
                            if ui
                                .small_button("\u{d7}")
                                .on_hover_text("Fit this part in its original package again.")
                                .clicked()
                            {
                                self.run(
                                    "variant.clear_footprint",
                                    json!({"name": variant.name, "component": swap.component}),
                                );
                            }
                        });
                    }
                    let mut swap = self.swap_pick.clone().unwrap_or_default();
                    ui.horizontal(|ui| {
                        egui::ComboBox::from_id_salt(("build-swap", variant.name.clone()))
                            .selected_text(if swap.is_empty() {
                                "choose a part to swap".to_owned()
                            } else {
                                swap.clone()
                            })
                            .show_ui(ui, |ui| {
                                for c in &self.project.components {
                                    if ui
                                        .selectable_label(swap == c.reference, &c.reference)
                                        .on_hover_text("A part in this design.")
                                        .clicked()
                                    {
                                        swap = c.reference.clone();
                                        ui.close();
                                    }
                                }
                            })
                            .response
                            .on_hover_text(
                                "Pick a part, name its other package, then Swap package.",
                            );
                        ui.add(
                            egui::TextEdit::singleline(&mut self.swap_name)
                                .desired_width(80.0)
                                .hint_text("package name"),
                        )
                        .on_hover_text(
                            "The package this build uses instead, for example a smaller size.",
                        );
                        if ui
                            .add_enabled(
                                !swap.is_empty() && !self.swap_name.trim().is_empty(),
                                egui::Button::new("Swap package"),
                            )
                            .on_hover_text(
                                "This build fits the chosen part in that package; the design keeps its own.",
                            )
                            .clicked()
                            && let Some(component) = self
                                .project
                                .components
                                .iter()
                                .find(|c| c.reference == swap)
                                .map(|c| c.id)
                        {
                            let footprint = self.swap_name.trim().to_owned();
                            self.run(
                                "variant.set_footprint",
                                json!({"name": variant.name, "component": component, "footprint": footprint}),
                            );
                            self.swap_pick = None;
                            self.swap_name.clear();
                            swap.clear();
                        }
                    });
                    self.swap_pick = if swap.is_empty() {
                        None
                    } else {
                        Some(swap)
                    };
                }
            });
        }
        if self.project.variants.is_empty() {
            ui.label(
                egui::RichText::new("No extra builds; this board is built exactly as designed.")
                    .small()
                    .color(self.muted()),
            );
        }
    }
    /// The folder make files land in: the typed one, or "outputs" beside the design.
    fn make_directory(&self) -> String {
        if !self.make_dir.trim().is_empty() {
            return self.make_dir.trim().to_owned();
        }
        let parent = std::path::Path::new(&self.path)
            .parent()
            .map_or_else(|| std::path::PathBuf::from("."), |p| p.to_path_buf());
        parent.join("outputs").to_string_lossy().into_owned()
    }
    /// Run a write-only make file tool into the make folder, creating it first.
    fn write_make_files(&mut self, tool: &str) {
        let directory = self.make_directory();
        if std::fs::create_dir_all(&directory).is_err() {
            self.message = "that folder could not be created".into();
            return;
        }
        self.tool_call(tool, json!({"path": directory}));
    }
    fn write_job_file(&mut self, job: &str, kind: &str, label: &str) {
        let directory = self.make_directory();
        if std::fs::create_dir_all(&directory).is_err() {
            self.message = "that folder could not be created".into();
            return;
        }
        let _ = self.tool_call("output_job.add", json!({"name": job, "kinds": [kind]}));
        if self.tool_call(
            "output_job.run",
            json!({"name": job, "directory": &directory}),
        ) {
            self.message = format!("{label} saved in {directory}");
        }
    }
    fn inspector(&mut self, ui: &mut egui::Ui) {
        egui::Panel::right("inspector")
            .default_size(280.0)
            .min_size(280.0)
            .frame(Self::dock_frame(ui.style()))
            .show(ui, |ui| {
                let split = self.preferences.aside_split.clamp(0.25, 0.75);
                let top_h = (ui.available_height() * split).min(ui.available_height() - 160.0);
                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), top_h),
                    egui::Layout::top_down(egui::Align::LEFT),
                    |ui| {
                        ui.label(
                            egui::RichText::new("Layers")
                                .size(10.0)
                                .color(self.muted()),
                        );
                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .show(ui, |ui| self.layers_tab(ui));
                    },
                );
                ui.add(
                    egui::Slider::new(&mut self.preferences.aside_split, 0.25..=0.75)
                        .show_value(false)
                        .text("Resize"),
                )
                .on_hover_text("Drag to give the Layers tree more or less room.");
                ui.horizontal(|ui| {
                    ui.selectable_value(
                        &mut self.preferences.aside_lower_tab,
                        0,
                        "Properties",
                    )
                    .on_hover_text("Details for whatever is selected on the drawing.");
                    ui.selectable_value(
                        &mut self.preferences.aside_lower_tab,
                        1,
                        "Assistant",
                    )
                    .on_hover_text("How to connect an external agent through MCP.");
                });
                if self.preferences.aside_lower_tab == 1 {
                    ui.add_space(12.0);
                    ui.label(
                        "Connect an agent through the live MCP bridge described in docs/mcp.md. \
                         The desktop session exposes the same typed commands the UI uses.",
                    );
                    return;
                }
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                ui.add_space(12.0);
                self.draw_tool_options(ui);
                ui.label(
                    egui::RichText::new("PROPERTIES")
                        .size(10.0)
                        .color(self.muted()),
                );
                if let Some(id) = self.selected {
                    let c = self.project.component(id).cloned().or_else(|| {
                        self.project
                            .footprint(id)
                            .and_then(|f| self.project.component(f.component))
                            .cloned()
                    });
                    if let Some(c) = c {
                        ui.heading(&c.reference);
                        if !c.units.is_empty() && self.project.component(id).is_some() {
                            let before = self.selected_unit;
                            egui::ComboBox::from_id_salt("selected-symbol-unit")
                                .selected_text(format!("Unit {}", self.selected_unit))
                                .show_ui(ui, |ui| {
                                    for unit in
                                        std::iter::once(1).chain(c.units.iter().map(|u| u.unit))
                                    {
                                        ui.selectable_value(
                                            &mut self.selected_unit,
                                            unit,
                                            format!("Unit {unit}"),
                                        )
                                        .on_hover_text("Edit or move this half of the part on its own.");
                                    }
                                });
                            if before != self.selected_unit {
                                self.load_properties();
                            }
                        }
                        ui.label(&c.kind);
                        ui.label(
                            egui::RichText::new(format!("Stable ID {id}"))
                                .small()
                                .color(self.muted()),
                        );
                        ui.add_space(10.0);
                        ui.label("Value");
                        ui.text_edit_singleline(&mut self.value);
                        field_help(ui, FIELD_HELP_VALUE, self.muted());
                        if ui.button("Apply value").on_hover_text(command_tip("component.value")).clicked() {
                            self.run("component.value", json!({"id":c.id,"value":self.value}));
                        }
                        ui.add_space(12.0);
                        ui.label("Position · mm");
                        field_help(ui, FIELD_HELP_POSITION, self.muted());
                        ui.horizontal(|ui| {
                            ui.label("X");
                            ui.add(egui::DragValue::new(&mut self.x).speed(0.1));
                            ui.label("Y");
                            ui.add(egui::DragValue::new(&mut self.y).speed(0.1));
                        });
                        if ui.button("Apply position").on_hover_text("Move the selected symbol or footprint to the position typed above.").clicked() {
                            let mut params = json!({"id":id,"position":{"x":self.x,"y":self.y}});
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
                        }
                        if ui.button("Rotate 90°  R").on_hover_text(command_tip("component.rotate")).clicked() {
                            self.rotate();
                        }
                        if ui
                            .button("Duplicate")
                            .on_hover_text(
                                "Places a copy of this part nearby, with its own new reference number. Undo removes the copy.",
                            )
                            .clicked()
                        {
                            let target =
                                self.project.footprint(id).map_or(id, |fp| fp.component);
                            self.run(
                                "object.duplicate",
                                json!({"ids": [target], "offset": {"x": 2.0, "y": 2.0}}),
                            );
                        }
                        if self.project.footprint(id).is_some() {
                            ui.label("Dock to board edge");
                            field_help(
                                ui,
                                "For an edge connector: the part slides until its copper just touches that edge, and keeps facing the way it faces.",
                                self.muted(),
                            );
                            ui.horizontal(|ui| {
                                for edge in ["left", "right", "top", "bottom"] {
                                    if ui
                                        .button(format!("Dock {}", edge))
                                        .on_hover_text(format!(
                                            "Slide this part until its copper sits flush against the {edge} board edge. Undo slides it back."
                                        ))
                                        .clicked()
                                    {
                                        self.run(
                                            "footprint.dock_edge",
                                            json!({"id": id, "edge": edge}),
                                        );
                                    }
                                }
                            });
                            let locked =
                                self.project.footprint(id).is_some_and(|fp| fp.locked == Some(true));
                            if ui
                                .button(if locked { "Unlock" } else { "Lock in place" })
                                .on_hover_text(if locked {
                                    "Let this part move, rotate and dock again."
                                } else {
                                    "Freeze this part so no accidental click, key or dock can nudge it off where you placed it."
                                })
                                .clicked()
                            {
                                self.run(
                                    "footprint.set_locked",
                                    json!({"id": id, "locked": !locked}),
                                );
                            }
                        }
                        ui.label(
                            c.library_footprint
                                .as_ref()
                                .map(|f| format!("Footprint: {}", f.definition.name))
                                .unwrap_or_else(|| {
                                    if c.library_symbol.is_some() {
                                        "Footprint: unassigned".into()
                                    } else {
                                        "Footprint: starter package".into()
                                    }
                                }),
                        );
                        if ui.button("Assign footprint in Libraries →").on_hover_text("Jump to the library panel to bind a footprint package to this component.").clicked() {
                            self.change_tab(Tab::Libraries);
                            self.library.kind = true;
                            self.library.pending = None;
                            self.library.path.clear();
                            self.library.results = None;
                            self.library.preview = None;
                            self.library.query.clear();
                        }
                        ui.add_space(12.0);
                        ui.label("Pin / net");
                        for pin in &c.pins {
                            ui.label(format!(
                                "{} · {}",
                                pin.number,
                                pin.net
                                    .map(|n| self.project.net_name(n))
                                    .unwrap_or("unassigned")
                            ));
                        }
                    } else {
                        ui.heading("Selected object");
                        ui.label(format!("Stable ID {id}"));
                        for (name, value) in object_detail_rows(&self.project, id) {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new(name).small().color(self.muted()));
                                ui.label(value);
                            });
                        }
                        self.via_covering_controls(ui, id);
                        if let Some(track) =
                            self.project.board.tracks.iter().find(|t| t.id == id)
                        {
                            let curved = track.arc.is_some();
                            let net = track.net;
                            let mid = (
                                (track.start.x + track.end.x) / 2.0,
                                (track.start.y + track.end.y) / 2.0,
                            );
                            let near = |a: Point, b: Point| {
                                (a.x - b.x).abs() < 1e-4 && (a.y - b.y).abs() < 1e-4
                            };
                            let span = |a: Point, b: Point| {
                                ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt()
                            };
                            let own_len = span(track.start, track.end);
                            let mate = self
                                .project
                                .board
                                .tracks
                                .iter()
                                .filter(|o| o.id != id && o.net == track.net)
                                .filter_map(|o| {
                                    let joined = near(o.start, track.start)
                                        || near(o.start, track.end)
                                        || near(o.end, track.start)
                                        || near(o.end, track.end);
                                    joined.then(|| (o.id, span(o.start, o.end)))
                                })
                                .min_by(|a, b| a.1.total_cmp(&b.1));
                            let max_cut = mate.map_or(0.05, |(_, len)| len.min(own_len) * 0.45);
                            ui.add_space(10.0);
                            ui.label("Wire width");
                            ui.add(
                                egui::DragValue::new(&mut self.track_width)
                                    .range(0.05..=10.0)
                                    .speed(0.05)
                                    .suffix(" mm"),
                            )
                            .on_hover_text(
                                "How wide this stretch of copper is. Thicker wires carry more current.",
                            );
                            if ui
                                .button("Set width")
                                .on_hover_text(command_tip("track.set_width"))
                                .clicked()
                            {
                                self.run(
                                    "track.set_width",
                                    json!({"id": id, "width": self.track_width}),
                                );
                            }
                            if ui
                                .button("Thicken whole connection")
                                .on_hover_text(
                                    "Make every stretch of this connection the width above, in one step, for a run that has to carry real current.",
                                )
                                .clicked()
                            {
                                self.run(
                                    "track.set_width_net",
                                    json!({"net": net, "width": self.track_width}),
                                );
                            }
                            if ui
                                .add_enabled(
                                    curved,
                                    egui::Button::new("Straighten"),
                                )
                                .on_hover_text(if curved {
                                "Pull this curved wire into a straight line. Undo brings the curve back."
                            } else {
                                "This wire is already straight; there is nothing to pull."
                            })
                            .clicked()
                            {
                                self.run("track.straighten", json!({"id": id}));
                            }
                            let corner_ready = mate.is_some();
                            ui.horizontal(|ui| {
                                ui.add_enabled(
                                    corner_ready,
                                    egui::DragValue::new(&mut self.chamfer_size)
                                        .range(0.05..=1.5)
                                        .speed(0.05)
                                        .suffix(" mm"),
                                )
                                .on_hover_text(
                                    "How far back from the sharp corner the diagonal cut starts.",
                                );
                                if ui
                                    .add_enabled(corner_ready, egui::Button::new("Cut corner"))
                                    .on_hover_text(if corner_ready {
                                        "Trade the sharp angle where two wires meet for a diagonal, so the etch comes out cleaner. Undo brings the corner back."
                                    } else {
                                        "This wire does not meet another wire of the same connection end to end, so there is no corner to cut."
                                    })
                                    .clicked()
                                    && let Some((other, _)) = mate
                                {
                                    let size = self.chamfer_size.min(max_cut);
                                    self.run(
                                        "track.chamfer",
                                        json!({"a": id, "b": other, "size": size}),
                                    );
                                }
                            });
                            if ui
                                .button("Cut in half")
                                .on_hover_text(
                                    "Break this stretch of wire into two equal halves so you can bend, branch or re-route part of it. Undo joins it back into one.",
                                )
                                .clicked()
                            {
                                self.run(
                                    "track.split",
                                    json!({"id": id, "at": {"x": mid.0, "y": mid.1}}),
                                );
                            }
                        }
                        if let Some(zone) =
                            self.project.board.zones.iter().find(|z| z.id == id)
                                .filter(|z| z.kind.as_deref() != Some("teardrop"))
                        {
                            let net = zone.net;
                            let (mut min_x, mut max_x, mut min_y, mut max_y) =
                                (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
                            for p in &zone.outline {
                                min_x = min_x.min(p.x);
                                max_x = max_x.max(p.x);
                                min_y = min_y.min(p.y);
                                max_y = max_y.max(p.y);
                            }
                            ui.add_space(10.0);
                            ui.label("Ground stitching");
                            ui.add(
                                egui::DragValue::new(&mut self.stitch_spacing)
                                    .range(0.5..=50.0)
                                    .speed(0.5)
                                    .suffix(" mm"),
                            )
                            .on_hover_text(
                                "How far apart the little connecting holes sit. Smaller numbers pack more in.",
                            );
                            if ui
                                .button("Fill with vias")
                                .on_hover_text(
                                    "Drops a grid of small plated holes across this copper so it stays at one potential from every corner. Undo sweeps them up.",
                                )
                                .clicked()
                            {
                                self.run(
                                    "via.stitch",
                                    json!({
                                        "net": net,
                                        "bounds": {
                                            "x": min_x,
                                            "y": min_y,
                                            "width": max_x - min_x,
                                            "height": max_y - min_y,
                                        },
                                        "spacing": self.stitch_spacing,
                                        "diameter": 0.6,
                                        "drill": 0.3,
                                    }),
                                );
                            }
                        }
                    }
                    ui.add_space(10.0);
                    if ui.button("Delete  Del").on_hover_text(command_tip("object.delete")).clicked() {
                        self.run("object.delete", json!({"id":id}));
                        self.select(None);
                    }
                    ui.separator();
                } else {
                    ui.heading("Nothing selected");
                    ui.label(
                        "Select an item on the drawing to inspect and edit it. Everything for the whole project lives in the Settings tab up top.",
                    );
                    ui.add_space(12.0);
                }
                    });
            });
    }
    fn footer(&mut self, ui: &mut egui::Ui) {
        let mut fix = 0usize;
        let mut mind = 0usize;
        let checks = self.checks.clone();
        for key in ["erc", "drc"] {
            if let Some(findings) = checks.get(key).and_then(Value::as_array) {
                for f in findings {
                    // Every finding carries the loudness its check resolved to,
                    // so the footer agrees with the review list instead of
                    // guessing a default per code.
                    let loud = f.get("severity").and_then(Value::as_str) == Some("error");
                    if loud {
                        fix += 1;
                    } else {
                        mind += 1;
                    }
                }
            }
        }
        let (base, zoom) = match self.tab {
            Tab::Schematic => (15.0, self.schematic.zoom),
            _ => (11.0, self.pcb.zoom),
        };
        let zoom_pct = (zoom / base * 100.0).round() as i32;
        let tool_words: String = match &self.tool {
            Tool::Select => "Pick".to_owned(),
            Tool::Place(kind) | Tool::LibraryPlace(_, kind, _) | Tool::CustomPlace(kind, _) => {
                format!("Place {kind}")
            }
            Tool::Wire(_) => "Draw wire".to_owned(),
            Tool::Route(_) => "Route".to_owned(),
            Tool::Move => "Move".to_owned(),
            Tool::Cutout => "Cut hole".to_owned(),
            Tool::Hole => "Drill hole".to_owned(),
            Tool::NoConnect => "No connection".to_owned(),
            Tool::Fiducial => "Fiducial mark".to_owned(),
            Tool::DrawLayer => "Draw on a layer".to_owned(),
        };
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(&self.message)
                        .small()
                        .color(self.muted()),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(format!("rev {}", self.revision))
                            .small()
                            .color(self.muted()),
                    )
                    .on_hover_text(
                        "Every edit raises this number; undo and redo step it back and forth.",
                    );
                    if let Some(p) = self.cursor {
                        ui.label(
                            egui::RichText::new(format!("{:.2}, {:.2} mm", p.x, p.y))
                                .small()
                                .color(self.muted()),
                        )
                        .on_hover_text("Where the mouse points on the board, in millimetres.");
                    }
                    ui.label(
                        egui::RichText::new(format!("{zoom_pct}%"))
                            .small()
                            .color(self.muted()),
                    )
                    .on_hover_text("How close the drawing is zoomed in. Scroll to change it.");
                    let tool_btn = ui
                        .small_button(format!("Tool: {tool_words}"))
                        .on_hover_text(
                            "What a click on the drawing does right now. Click here to go back to picking things, or press Esc.",
                        );
                    if tool_btn.clicked() {
                        self.tool = Tool::Select;
                    }
                    let fix_btn = ui
                        .small_button(format!("{fix} to fix"))
                        .on_hover_text("Mistakes the checks say you must fix. Click to see the list.");
                    let mind_btn = ui
                        .small_button(format!("{mind} to mind"))
                        .on_hover_text(
                            "Warnings worth a look, but not blockers. Click to see the list.",
                        );
                    if fix_btn.clicked() || mind_btn.clicked() {
                        self.change_tab(Tab::Checks);
                    }
                });
            });
        });
        if self.command_open {
            egui::Panel::bottom("commands")
                .resizable(true)
                .default_size(220.0)
                .show(ui, |ui| {
                    let query = self.command_query.clone();
                    let matches: Vec<_> = pcbcraft_engine::commands()
                        .into_iter()
                        .filter(|spec| {
                            query.is_empty()
                                || fuzzy_match(&query, spec.id)
                                || fuzzy_match(&query, spec.description)
                        })
                        .collect();
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("SEARCH")
                                .small()
                                .color(self.accent()),
                        );
                        let search = ui.add(
                            egui::TextEdit::singleline(&mut self.command_query)
                                .hint_text("filter command names and descriptions")
                                .desired_width(300.0),
                        );
                        if self.command_focus {
                            search.request_focus();
                            self.command_focus = false;
                        }
                        ui.label(
                            egui::RichText::new("PARAMETERS")
                                .small()
                                .color(self.accent()),
                        );
                        let fields = ui.add(
                            egui::TextEdit::multiline(&mut self.params)
                                .hint_text("parameters as JSON, one line per value reads easiest")
                                .desired_width(380.0)
                                .desired_rows(3)
                                .interactive(true),
                        )
                        .on_hover_text(
                            "Edit the parameters here; Enter adds a new line so long lists stay readable, and Ctrl+Enter or Execute runs the command.",
                        );
                        if ui
                            .button("Execute")
                            .on_hover_text(
                                "Run the command with the parameters you typed; selecting a command fills a starter parameter set.",
                            )
                            .clicked()
                        {
                            self.execute_palette_params();
                        }
                        let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                        if query != self.command_query {
                            self.command_index = 0;
                        }
                        let stepped = ui.input(|i| {
                            i.key_pressed(egui::Key::ArrowDown) as isize
                                - i.key_pressed(egui::Key::ArrowUp) as isize
                        });
                        if stepped != 0 {
                            self.command_index =
                                step_index(self.command_index, matches.len(), stepped);
                            self.command_scroll = true;
                            if !search.has_focus() {
                                search.request_focus();
                            }
                        }
                        if enter
                            && search.lost_focus()
                            && let Some(spec) =
                                matches.get(step_index(self.command_index, matches.len(), 0))
                        {
                            self.command = spec.id.into();
                            self.params = params_template(spec);
                            fields.request_focus();
                        } else if enter
                            && ((ui.input(|i| i.modifiers.ctrl || i.modifiers.command)
                                && fields.has_focus())
                                || fields.lost_focus())
                        {
                            self.execute_palette_params();
                        }
                    });
                    ui.label(
                        egui::RichText::new(format!(
                            "{} of {} commands match; click one to load it with a starter parameter set, or step the highlight with Up and Down and press Enter in the search box to load it. In the parameter box, Enter writes a new line and Ctrl+Enter runs the command.",
                            matches.len(),
                            pcbcraft_engine::commands().len()
                        ))
                        .small()
                        .color(self.muted()),
                    );
                    egui::ScrollArea::vertical()
                        .max_height(180.0)
                        .show(ui, |ui| {
                            if let Some(spec) = pcbcraft_engine::commands()
                                .into_iter()
                                .find(|spec| spec.id == self.command)
                            {
                                for line in param_help(&spec) {
                                    ui.label(
                                        egui::RichText::new(line)
                                            .small()
                                            .color(self.muted()),
                                    );
                                }
                            }
                            for (row, spec) in matches.iter().enumerate() {
                                let response = ui
                                    .selectable_label(
                                        self.command == spec.id
                                            || row == step_index(
                                                self.command_index,
                                                matches.len(),
                                                0,
                                            ),
                                        format!("{}   {}", spec.id, spec.description),
                                    )
                                    .on_hover_text("Load this command with a starter parameter set.");
                                if row == self.command_index && self.command_scroll {
                                    response.scroll_to_me(Some(egui::Align::Center));
                                }
                                if response.clicked() {
                                    self.command = spec.id.into();
                                    self.params = params_template(spec);
                                    self.command_index = row;
                                }
                            }
                            self.command_scroll = false;
                        });
                });
        }
        if self.shortcuts_open {
            let muted = self.muted();
            egui::Window::new("Keyboard shortcuts")
                .open(&mut self.shortcuts_open)
                .show(ui, |ui| {
                    egui::Grid::new("shortcut-list")
                        .num_columns(3)
                        .spacing([16.0, 4.0])
                        .show(ui, |ui| {
                            for (keys, action, context) in shortcut_rows() {
                                ui.label(egui::RichText::new(keys).monospace().strong());
                                ui.label(action);
                                ui.label(egui::RichText::new(context).small().color(muted));
                                ui.end_row();
                            }
                        });
                });
        }
    }
    fn library(&mut self, ui: &mut egui::Ui) {
        self.library_workspaces(ui);
        ui.collapsing("Original starter symbols", |ui| {
            for t in TEMPLATES {
                egui::Frame::group(ui.style())
                    .inner_margin(18.0)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.label(egui::RichText::new(t.label).size(18.0).strong());
                                ui.label(format!("{} · {} · 2 pins", t.prefix, t.value));
                            });
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui
                                        .button("Place in schematic >")
                                        .on_hover_text(command_tip("component.place_library"))
                                        .clicked()
                                    {
                                        self.change_tab(Tab::Schematic);
                                        self.tool = Tool::Place(t.kind.into());
                                    }
                                },
                            );
                        });
                    });
                ui.add_space(10.0);
            }
        });
    }
    fn checks_view(&mut self, ui: &mut egui::Ui) {
        ui.add_space(24.0);
        ui.heading("Design checks");
        ui.label("Select a finding to inspect its affected object in the editor.");
        if self
            .checks
            .get("erc")
            .and_then(Value::as_array)
            .is_none_or(|v| v.is_empty())
            && self
                .checks
                .get("drc")
                .and_then(Value::as_array)
                .is_none_or(|v| v.is_empty())
        {
            empty_state(ui, EMPTY_CHECKS, self.muted());
        }
        ui.add_space(16.0);
        let checks = self.checks.clone();
        for (key, label, blurb) in [
            (
                "erc",
                "Electrical",
                "Whether the wiring makes electrical sense: pins with no connection, parts missing footprints, and power pins left floating.",
            ),
            (
                "drc",
                "Board",
                "Whether the copper can actually be made: gaps too small, wires not joined, and widths the factory cannot etch.",
            ),
        ] {
            ui.label(egui::RichText::new(label).size(18.0).strong());
            ui.label(egui::RichText::new(blurb).small().color(self.muted()));
            if let Some(findings) = checks.get(key).and_then(Value::as_array) {
                if findings.is_empty() {
                    ui.label(
                        egui::RichText::new("No findings in the implemented checks")
                            .color(self.accent()),
                    );
                }
                let mut groups: Vec<CheckGroup> = Vec::new();
                for f in findings {
                    let code = f
                        .get("code")
                        .and_then(Value::as_str)
                        .unwrap_or("finding")
                        .to_owned();
                    let objects: Vec<Id> = f
                        .get("objects")
                        .and_then(Value::as_array)
                        .map(|v| v.iter().filter_map(|x| x.as_u64()).collect())
                        .unwrap_or_default();
                    let message = f
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_owned();
                    if let Some(group) = groups.iter_mut().find(|(c, _)| *c == code) {
                        group.1.push((message, objects));
                    } else {
                        groups.push((code, vec![(message, objects)]));
                    }
                }
                groups.sort_by(|a, b| {
                    let rank =
                        |code: &str| match self.project.rule_severity(code, "warning").as_str() {
                            "error" => 0,
                            "info" => 2,
                            _ => 1,
                        };
                    rank(&a.0)
                        .cmp(&rank(&b.0))
                        .then(b.1.len().cmp(&a.1.len()))
                        .then(a.0.cmp(&b.0))
                });
                let louds: Vec<bool> = groups
                    .iter()
                    .map(|(code, _)| self.project.rule_severity(code, "warning") == "error")
                    .collect();
                for ((code, rows), loud) in groups.into_iter().zip(louds) {
                    let title = format!(
                        "{} · {} {}",
                        custom_heading(&self.project, &code),
                        rows.len(),
                        if rows.len() == 1 { "place" } else { "places" }
                    );
                    let title_text = if loud {
                        egui::RichText::new(title).strong()
                    } else {
                        egui::RichText::new(title)
                    };
                    ui.add_space(6.0);
                    let whose = if code.starts_with("custom_") {
                        "A check you wrote. "
                    } else {
                        "The board is checked against this rule after every edit. "
                    };
                    ui.label(title_text).on_hover_text(format!(
                        "{whose}{who}The list below shows every place it fired; click one to jump to it.",
                        who = if loud {
                            "It must be fixed before the board is built. "
                        } else {
                            "It is worth a look, but does not block the build. "
                        }
                    ));
                    for (message, objects) in rows {
                        ui.horizontal(|ui| {
                            ui.add_space(14.0);
                            if ui
                                .button(message)
                                .on_hover_text("Select this on the drawing.")
                                .clicked()
                                && let Some(id) = objects.first().copied()
                            {
                                self.change_tab(if key == "erc" {
                                    Tab::Schematic
                                } else {
                                    Tab::Pcb
                                });
                                self.select(Some(id));
                            }
                            if !objects.is_empty()
                                && ui
                                    .small_button("Hide this")
                                    .on_hover_text(
                                        "You have looked at this and it is fine: hide just this finding. Everything you hide can be brought back below.",
                                    )
                                    .clicked()
                            {
                                self.run(
                                    "rules.exclude",
                                    json!({"code": code.clone(), "ids": objects}),
                                );
                            }
                            if !objects.is_empty() {
                                let spots: Vec<Point> = objects
                                    .iter()
                                    .filter_map(|o| self.project.object_position(*o))
                                    .collect();
                                if spots.len() == objects.len()
                                    && let (Some(min_x), Some(max_x), Some(min_y), Some(max_y)) = (
                                        spots.iter().map(|p| p.x).reduce(f64::min),
                                        spots.iter().map(|p| p.x).reduce(f64::max),
                                        spots.iter().map(|p| p.y).reduce(f64::min),
                                        spots.iter().map(|p| p.y).reduce(f64::max),
                                    )
                                {
                                    let pad = 2.0;
                                    if ui
                                        .small_button("Quiet this area")
                                        .on_hover_text(
                                            "This corner of the board is the way you want it: ask the check to leave anything inside a box drawn around these items alone. You can clear the box below.",
                                        )
                                        .clicked()
                                    {
                                        self.run(
                                            "rules.exclude_region",
                                            json!({
                                                "code": code,
                                                "x": min_x - pad,
                                                "y": min_y - pad,
                                                "width": (max_x - min_x) + pad * 2.0,
                                                "height": (max_y - min_y) + pad * 2.0,
                                            }),
                                        );
                                    }
                                }
                            }
                        });
                    }
                }
            }
            ui.add_space(20.0);
        }
        let silenced: Vec<(String, usize)> = self
            .project
            .rule_exclusions
            .iter()
            .map(|(code, ids)| (code.clone(), ids.len()))
            .collect();
        if !silenced.is_empty() {
            ui.label(
                egui::RichText::new("Things you asked not to be told about")
                    .small()
                    .color(self.muted()),
            );
            for (code, count) in silenced {
                ui.horizontal(|ui| {
                    ui.label(format!("{code} · {count} hidden"));
                    if ui
                        .small_button(format!("Bring back {code}"))
                        .on_hover_text(
                            "Show every hidden finding from this check again; each one can be hidden afresh.",
                        )
                        .clicked()
                    {
                        self.run("rules.include", json!({"code": code}));
                    }
                });
            }
            ui.add_space(12.0);
        }
        let zones: Vec<(String, usize)> = self
            .project
            .rule_regions
            .iter()
            .map(|(code, regions)| (code.clone(), regions.len()))
            .collect();
        if !zones.is_empty() {
            ui.label(
                egui::RichText::new("Quiet zones: checks that leave these corners alone")
                    .small()
                    .color(self.muted()),
            );
            for (code, count) in zones {
                ui.horizontal(|ui| {
                    ui.label(format!("{code} · {count} zone(s)"));
                    if ui
                        .small_button(format!("Clear {code} zones"))
                        .on_hover_text(
                            "Remove every quiet box for this check, so it reports the whole board again.",
                        )
                        .clicked()
                    {
                        self.run("rules.include_region", json!({"code": code}));
                    }
                });
            }
            ui.add_space(12.0);
        }
        ui.separator();
        ui.label(
            egui::RichText::new(checks.get("scope").and_then(Value::as_str).unwrap_or(""))
                .small()
                .color(self.muted()),
        );
    }
    fn capture(&mut self, ctx: &egui::Context) {
        self.frames = self.frames.saturating_add(1);
        let library_ready = self.tab != Tab::Libraries
            || self.library.editor.mode != 0
            || (self.library.catalog.is_some()
                && (self.library.results.is_some() || self.library.path.is_empty())
                && self.library.pending.is_none());
        if self.capture.is_some() && self.frames >= 5 && library_ready {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
        }
        if let Some(path) = self.capture.clone() {
            let images = ctx.input(|i| {
                i.events
                    .iter()
                    .filter_map(|e| {
                        if let egui::Event::Screenshot { image, .. } = e {
                            Some(image.clone())
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
            });
            for image in images {
                let bytes: Vec<u8> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
                if let Err(e) = image::save_buffer(
                    &path,
                    &bytes,
                    image.size[0] as u32,
                    image.size[1] as u32,
                    image::ColorType::Rgba8,
                ) {
                    self.message = format!("capture failed: {e}");
                    eprintln!("{}", self.message);
                }
                self.capture = None;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            ctx.request_repaint();
        }
    }
}
impl App {
    fn copy_plot_to_clipboard(&mut self, ctx: &egui::Context) {
        let view = if matches!(self.tab, Tab::Schematic) {
            "schematic"
        } else {
            "pcb"
        };
        let bytes = self
            .session
            .lock()
            .ok()
            .and_then(|session| pcbcraft_io::preview_png(&session.project, view).ok())
            .and_then(|bytes| {
                let mut reader = std::io::Cursor::new(bytes);
                image::load(&mut reader, image::ImageFormat::Png)
                    .map(|image| image.into_rgba8())
                    .ok()
            });
        match bytes {
            Some(image) => {
                ctx.copy_image(egui::ColorImage::from_rgba_unmultiplied(
                    [image.width() as usize, image.height() as usize],
                    image.as_raw(),
                ));
                self.message = "plot copied to the clipboard".into();
            }
            None => self.message = "copy failed: no plot to copy".into(),
        }
    }

    fn record_icon(&mut self, _id: &str, _rect: Rect) {
        #[cfg(test)]
        self.icon_rects.insert(_id.to_owned(), _rect);
    }

    fn icon_button_style(&self) -> IconButtonStyle {
        let stroke = Stroke::new(1.0, self.ink().gamma_multiply(0.35));
        let fill = if self.preferences.dark {
            Color32::from_rgb(28, 40, 56)
        } else {
            Color32::from_rgb(220, 228, 238)
        };
        let hover = if self.preferences.dark {
            Color32::from_rgb(40, 58, 80)
        } else {
            Color32::from_rgb(200, 212, 228)
        };
        IconButtonStyle {
            normal: ButtonVisuals {
                fill,
                stroke,
                icon: self.ink(),
            },
            hovered: ButtonVisuals {
                fill: hover,
                stroke,
                icon: self.ink(),
            },
            pressed: None,
            selected: ButtonVisuals {
                fill: self.accent().gamma_multiply(0.35),
                stroke: Stroke::new(1.0, self.accent()),
                icon: self.ink(),
            },
            corner_radius: egui::CornerRadius::same(6),
            focus_stroke: Stroke::new(1.0, self.accent()),
        }
    }

    fn tool_strip_button(
        &mut self,
        ui: &mut egui::Ui,
        id: &str,
        glyph: &str,
        tip: &str,
        selected: bool,
    ) -> egui::Response {
        let style = self.icon_button_style();
        let response = IconButton::new(id, Vec2::splat(28.0), &style)
            .selected(selected)
            .show(ui, |ui, rect, tint| {
                ui.painter().text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    glyph,
                    FontId::proportional(13.0),
                    tint,
                );
            })
            .on_hover_text(tip);
        self.record_icon(id, response.rect);
        response
    }

    fn draw_tool_options(&mut self, ui: &mut egui::Ui) {
        if self.tab != Tab::Pcb {
            return;
        }
        if matches!(self.tool, Tool::DrawLayer) {
            ui.label(
                egui::RichText::new("DRAW ON LAYER")
                    .size(10.0)
                    .color(self.muted()),
            );
            let names: Vec<(Id, String)> = self
                .project
                .board
                .user_layers
                .iter()
                .map(|l| (l.id, l.name.clone()))
                .collect();
            let current = self
                .art_layer
                .and_then(|id| names.iter().find(|(i, _)| *i == id).map(|(_, n)| n.clone()))
                .unwrap_or_else(|| "no layer yet".into());
            egui::ComboBox::from_id_salt("art-layer-pick")
                .selected_text(current)
                .width(150.0)
                .show_ui(ui, |ui| {
                    for (id, name) in &names {
                        ui.radio_value(&mut self.art_layer, Some(*id), name.as_str())
                            .on_hover_text("New figures land on this layer.");
                    }
                });
            if ui
                .button("New layer")
                .on_hover_text(
                    "Make a new drawn layer and switch to it; give it a real name in the layers list.",
                )
                .clicked()
            {
                let name = format!("Art {}", names.len() + 1);
                self.run("board.layer_add", json!({"name": name, "kind": "other"}));
                if self.message.starts_with("board.layer_add") {
                    self.art_layer = self.selected;
                }
            }
            ui.horizontal(|ui| {
                for (shape, label, tip) in [
                    (DrawShape::Line, "Line", "Two clicks: start and end."),
                    (DrawShape::Rect, "Box", "Two clicks on opposite corners."),
                    (DrawShape::Circle, "Circle", "Centre, then edge."),
                    (DrawShape::Poly, "Outline", "Corners; Enter to close."),
                    (DrawShape::Words, "Words", "Type below, then click once."),
                ] {
                    if ui
                        .selectable_label(self.art_shape == shape, label)
                        .on_hover_text(tip)
                        .clicked()
                    {
                        self.art_shape = shape;
                        self.art_start = None;
                        self.art_points.clear();
                    }
                }
            });
            if self.art_shape == DrawShape::Words {
                ui.add(
                    egui::TextEdit::singleline(&mut self.art_text)
                        .hint_text("words to print")
                        .desired_width(f32::INFINITY),
                )
                .on_hover_text("The words a click will print on the active layer.");
            }
            ui.separator();
        }
        if matches!(self.tool, Tool::Hole) {
            ui.label("Mounting hole size");
            ui.add(
                egui::DragValue::new(&mut self.hole_diameter)
                    .speed(0.05)
                    .range(0.05..=25.0)
                    .suffix(" mm"),
            )
            .on_hover_text("Drill size for new holes; an M3 spacer usually wants 3.2 mm.");
            ui.separator();
        }
        if matches!(self.tool, Tool::Fiducial) {
            ui.label("Assembly mark size");
            ui.add(
                egui::DragValue::new(&mut self.fiducial_diameter)
                    .speed(0.05)
                    .range(0.2..=10.0)
                    .suffix(" mm"),
            )
            .on_hover_text("Disc size for new marks; 1 mm is the usual size.");
            ui.separator();
        }
    }

    fn tool_strip(&mut self, ui: &mut egui::Ui) {
        if !matches!(self.tab, Tab::Schematic | Tab::Pcb) {
            return;
        }
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            let select = |app: &mut App| {
                app.tool = Tool::Select;
            };
            if self
                .tool_strip_button(ui, "tool-select", "Sel", "Select objects on the drawing.", matches!(self.tool, Tool::Select))
                .clicked()
            {
                select(self);
            }
            if self
                .tool_strip_button(ui, "tool-move", "Mv", "Move tool: drop the selection at the cursor.", matches!(self.tool, Tool::Move))
                .clicked()
            {
                self.tool = Tool::Move;
            }
            if self.tab == Tab::Schematic {
                if self
                    .tool_strip_button(
                        ui,
                        "tool-wire",
                        "W",
                        "Wire tool: click two pins to connect them.",
                        matches!(self.tool, Tool::Wire(_)),
                    )
                    .clicked()
                {
                    self.tool = Tool::Wire(None);
                }
                if self
                    .tool_strip_button(
                        ui,
                        "tool-noconnect",
                        "No connect",
                        "Mark a pin deliberately unconnected.",
                        matches!(self.tool, Tool::NoConnect),
                    )
                    .clicked()
                {
                    self.tool = Tool::NoConnect;
                }
            }
            if self.tab == Tab::Pcb {
                if self
                    .tool_strip_button(
                        ui,
                        "tool-route",
                        "Rt",
                        "Route tool: click two pads to start a track.",
                        matches!(self.tool, Tool::Route(_)),
                    )
                    .clicked()
                {
                    self.tool = Tool::Route(None);
                }
                for (shape, id, glyph, tip) in [
                    (DrawShape::Line, "tool-line", "Ln", "Draw a straight line on a user layer."),
                    (DrawShape::Rect, "tool-box", "Bx", "Draw a box on a user layer."),
                    (DrawShape::Circle, "tool-circle", "Cr", "Draw a circle on a user layer."),
                    (DrawShape::Poly, "tool-outline", "Pl", "Draw an outline on a user layer."),
                    (DrawShape::Words, "tool-words", "Tx", "Print words on a user layer."),
                ] {
                    let armed =
                        matches!(self.tool, Tool::DrawLayer) && self.art_shape == shape;
                    if self.tool_strip_button(ui, id, glyph, tip, armed).clicked() {
                        self.tool = Tool::DrawLayer;
                        self.art_shape = shape;
                        self.art_start = None;
                        self.art_points.clear();
                    }
                }
                if ui
                    .selectable_label(matches!(self.tool, Tool::DrawLayer), "Draw shape")
                    .on_hover_text(
                        "Draw cut lines, perforations or printed marks on your own layer.",
                    )
                    .clicked()
                {
                    self.tool = Tool::DrawLayer;
                    self.art_shape = DrawShape::Line;
                    self.art_start = None;
                    self.art_points.clear();
                }
                if ui
                    .selectable_label(matches!(self.tool, Tool::Cutout), "Cutout")
                    .on_hover_text(command_tip("board.cutout.add"))
                    .clicked()
                {
                    self.tool = Tool::Cutout;
                }
                if ui
                    .selectable_label(matches!(self.tool, Tool::Hole), "Mounting hole")
                    .on_hover_text(
                        "Drill an unplated hole for a screw or spacer; then click the board where it goes.",
                    )
                    .clicked()
                {
                    self.tool = Tool::Hole;
                }
                if ui
                    .selectable_label(matches!(self.tool, Tool::Fiducial), "Assembly mark")
                    .on_hover_text(
                        "A bare copper disc the pick-and-place camera finds the board by; then click a clear corner.",
                    )
                    .clicked()
                {
                    self.tool = Tool::Fiducial;
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if !matches!(self.tool, Tool::Select) {
                    ui.label(
                        egui::RichText::new(tool_hint(&self.tool))
                            .small()
                            .color(self.muted()),
                    );
                }
            });
        });
        ui.separator();
    }

    fn files_tab(&mut self, ui: &mut egui::Ui) {
        Self::margined(ui, |ui| {
            ui.heading("Project files");
            ui.label(format!("Design file: {}", self.path.trim()));
            ui.add_space(12.0);
            ui.label(egui::RichText::new("Builds").size(10.0).color(self.muted()));
            self.builds_tab(ui);
        });
    }
    fn board3d_tab(&mut self, ui: &mut egui::Ui) {
        Self::margined(ui, |ui| {
            ui.heading("3D view");
            ui.label(
                "A Rust-native 3D board viewer is on the way. Until then, use fabrication previews and mesh export from Settings.",
            );
        });
    }

    fn show(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        #[cfg(test)]
        self.icon_rects.clear();
        self.refresh();
        self.style(&ctx);
        self.shortcuts(&ctx);
        self.header(ui);
        self.footer(ui);
        if self.home_open {
            self.start_home(ui);
        } else {
            self.browser(ui);
            self.inspector(ui);
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE.fill(self.surface()))
                .show(ui, |ui| {
                    self.workbench_tabs(ui);
                    self.tool_strip(ui);
                    self.page_left = ui.max_rect().left();
                    match self.tab {
                        Tab::Schematic | Tab::Pcb => self.canvas(ui),
                        Tab::Files => {
                            egui::ScrollArea::vertical()
                                .show(ui, |ui| self.files_tab(ui))
                                .inner
                        }
                        Tab::Board3d => {
                            egui::ScrollArea::vertical()
                                .show(ui, |ui| self.board3d_tab(ui))
                                .inner
                        }
                        Tab::Libraries => {
                            egui::ScrollArea::vertical()
                                .show(ui, |ui| self.library(ui))
                                .inner
                        }
                        Tab::Rules => {
                            egui::ScrollArea::vertical()
                                .show(ui, |ui| Self::margined(ui, |ui| self.rules_tab(ui)));
                        }
                        Tab::Checks => {
                            egui::ScrollArea::vertical()
                                .show(ui, |ui| Self::margined(ui, |ui| self.checks_view(ui)));
                        }
                        Tab::Settings => {
                            egui::ScrollArea::vertical()
                                .show(ui, |ui| Self::margined(ui, |ui| self.settings_tab(ui)));
                        }
                    }
                });
        }
        self.capture(&ctx);
        ctx.request_repaint_after(std::time::Duration::from_millis(200));
    }
}
impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
        self.show(ui);
    }
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, "preferences", &self.preferences);
        if let Ok(session) = self.session.lock()
            && let Ok(text) = serde_json::to_string(&session.project)
            && text.len() <= pcbcraft_io_limit()
        {
            storage.set_string("recovery", text);
        }
    }
}
fn pcbcraft_io_limit() -> usize {
    8 * 1024 * 1024
}

#[cfg(test)]
mod tests {
    use super::e2e::support::*;
    use super::*;

    #[test]
    fn selected_board_objects_describe_themselves_in_the_properties_panel() -> Result<(), String> {
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let made = |command: &str, params: Value| -> Result<u64, String> {
            shared
                .lock()
                .map_err(|_| "session lock unavailable")?
                .execute(command, params, None)
                .map_err(|e| e.to_string())?["result"]["id"]
                .as_u64()
                .ok_or_else(|| format!("{command} returned no id"))
        };
        let net = made("net.create", json!({"name":"PWR"}))?;
        let track = made(
            "track.add",
            json!({"net":net,"start":{"x":0.0,"y":0.0},"end":{"x":10.0,"y":0.0},"width":0.3}),
        )?;
        let via = made(
            "via.place",
            json!({"net":net,"position":{"x":5.0,"y":5.0},"diameter":0.7,"drill":0.3}),
        )?;
        let zone = made(
            "zone.place",
            json!({"net":net,"layer":"F.Cu","outline":[{"x":0.0,"y":0.0},{"x":10.0,"y":0.0},{"x":10.0,"y":10.0},{"x":0.0,"y":10.0},{"x":0.0,"y":0.0}]}),
        )?;
        let text = made(
            "text.place",
            json!({"text":"REV A","position":{"x":20.0,"y":20.0},"size":1.5,"layer":"F.Cu"}),
        )?;
        let label = made(
            "label.place",
            json!({"kind":"label","net":net,"position":{"x":1.0,"y":1.0}}),
        )?;
        let project = shared
            .lock()
            .map_err(|_| "session lock unavailable")?
            .project
            .clone();
        let find = |id: u64, name: &str| {
            object_detail_rows(&project, id)
                .into_iter()
                .find(|(row, _)| row == name)
                .map(|(_, value)| value)
        };
        assert_eq!(
            find(track, "Net").as_deref(),
            Some("PWR · follows the wiring"),
            "a track names its net and says where it comes from"
        );
        assert_eq!(
            find(track, "Width").as_deref(),
            Some("0.300 mm · chosen for this wire"),
            "a track states its width and says it was picked on purpose"
        );
        assert_eq!(
            find(via, "Drill").as_deref(),
            Some("0.30 mm"),
            "a via states its drill"
        );
        assert_eq!(
            find(zone, "Copper").as_deref(),
            Some("poured"),
            "a zone states its pour"
        );
        assert_eq!(
            find(zone, "Outline").as_deref(),
            Some("5 vertices"),
            "a zone counts its outline"
        );
        assert_eq!(
            find(text, "Text").as_deref(),
            Some("REV A"),
            "text shows its string"
        );
        assert_eq!(
            find(label, "Kind").as_deref(),
            Some("label"),
            "a schematic label states its kind"
        );
        assert!(
            object_detail_rows(&project, 999).is_empty(),
            "an unknown id stays quiet"
        );
        Ok(())
    }

    #[test]
    fn a_folder_added_from_anywhere_joins_the_list_and_removal_forgets_it_again()
    -> Result<(), String> {
        let _turn = pcbcraft_engine::library::SOURCES_TEST_LOCK
            .lock()
            .map_err(|_| "source test lock unavailable")?;
        let file =
            std::env::temp_dir().join(format!("pcbcraft-source-e2e-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&file);
        {
            let mut slot = pcbcraft_engine::library::SOURCES_PATH_TEST
                .lock()
                .map_err(|_| "source path slot unavailable")?;
            *slot = Some(file.clone());
        }
        let outcome = source_round_trip(&file);
        {
            let mut slot = pcbcraft_engine::library::SOURCES_PATH_TEST
                .lock()
                .map_err(|_| "source path slot unavailable")?;
            *slot = None;
        }
        let _ = std::fs::remove_file(&file);
        outcome
    }
    /// The visible loop a newcomer follows: type a folder, press Add folder,
    /// watch its collections appear, select one, press Remove source, and
    /// watch both the list and the remembered config forget it.
    fn source_round_trip(file: &std::path::Path) -> Result<(), String> {
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = App::create(shared, Preferences::default(), None, None);
        app.change_tab(Tab::Libraries);
        app.refresh();
        let ctx = egui::Context::default();
        for _ in 0..80 {
            frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
            if app.library.catalog.is_some() && app.library.sources.is_some() {
                break;
            }
        }
        assert!(
            app.library.catalog.is_some(),
            "the catalog arrives on its own"
        );
        let shelf = std::fs::canonicalize(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../library/tests/fixtures"),
        )
        .map_err(|e| format!("fixture shelf missing: {e}"))?
        .to_string_lossy()
        .into_owned();
        app.library.new_source = shelf.clone();
        authoring::tests::tap(&ctx, &mut app, "Add folder")?;
        let mut added = false;
        for _ in 0..80 {
            frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
            if app.library.notice.contains("Added") {
                added = true;
                break;
            }
        }
        assert!(
            added,
            "adding the folder reports back: {}",
            app.library.notice
        );
        let saved =
            std::fs::read_to_string(file).map_err(|e| format!("config not written: {e}"))?;
        assert!(
            saved.contains(&shelf),
            "the folder is remembered in the config file"
        );
        let mut listed = false;
        for _ in 0..80 {
            frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
            listed = app
                .library
                .catalog
                .as_ref()
                .and_then(|catalog| catalog.get("symbol_libraries"))
                .and_then(Value::as_array)
                .is_some_and(|libs| {
                    libs.iter().any(|lib| {
                        lib.get("path")
                            .and_then(Value::as_str)
                            .is_some_and(|path| path.starts_with(&shelf))
                    })
                });
            if listed {
                break;
            }
        }
        assert!(
            listed,
            "the added folder's collections join the unified list"
        );
        let added_path = app
            .library
            .catalog
            .as_ref()
            .and_then(|catalog| catalog.get("symbol_libraries"))
            .and_then(Value::as_array)
            .and_then(|libs| {
                libs.iter().find_map(|lib| {
                    lib.get("path")
                        .and_then(Value::as_str)
                        .filter(|path| path.starts_with(&shelf))
                        .map(str::to_owned)
                })
            })
            .ok_or("the added collection vanished from the list")?;
        app.library.path = added_path;
        authoring::tests::tap(&ctx, &mut app, "Remove source")?;
        let mut removed = false;
        for _ in 0..80 {
            frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
            if app.library.notice.contains("Removed") {
                removed = true;
                break;
            }
        }
        assert!(removed, "removal reports back: {}", app.library.notice);
        let saved = std::fs::read_to_string(file).map_err(|_| "config vanished")?;
        assert!(
            !saved.contains(&shelf),
            "the forgotten folder is out of the config file"
        );
        Ok(())
    }

    #[test]
    fn the_file_menu_is_where_you_open_save_and_start_a_design() -> Result<(), String> {
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = App::create(shared, Preferences::default(), None, None);
        app.refresh();
        let file = std::env::temp_dir()
            .join(format!(
                "pcbcraft-menu-save-{}.pcbcraft",
                std::process::id()
            ))
            .to_string_lossy()
            .into_owned();
        app.path = file.clone();
        let ctx = egui::Context::default();
        for _ in 0..4 {
            frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        }
        assert!(
            !paints_text(&ctx, &mut app, "Project file")?,
            "the old bottom file strip is gone; file jobs live in the File menu"
        );
        app.tool_call("sample_design", json!({}));
        authoring::tests::tap(&ctx, &mut app, "File")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert!(
            !paints_text(&ctx, &mut app, "Load LED example")?,
            "the example lives on the start screen, not in the menu"
        );
        authoring::tests::tap(&ctx, &mut app, "Save")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert!(
            app.message.starts_with("save"),
            "saving answers: {}",
            app.message
        );
        assert!(
            std::fs::metadata(&file).is_ok_and(|m| m.len() > 0),
            "the design lands in the file the menu names"
        );
        authoring::tests::tap(&ctx, &mut app, "File")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        authoring::tests::tap(&ctx, &mut app, "New")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert!(
            app.project.components.is_empty(),
            "New empties the design: {} parts left",
            app.project.components.len()
        );
        authoring::tests::tap(&ctx, &mut app, "Edit")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        authoring::tests::tap(&ctx, &mut app, "Undo")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert!(
            app.message.starts_with("undo"),
            "undo answers: {}",
            app.message
        );
        assert!(
            !app.project.components.is_empty(),
            "undo brings the design back"
        );
        let _ = std::fs::remove_file(&file);
        Ok(())
    }

    #[test]
    fn a_drawn_shape_from_a_file_becomes_the_board_edge_and_back_again() -> Result<(), String> {
        let file =
            std::env::temp_dir().join(format!("pcbcraft-board-edge-{}.svg", std::process::id()));
        std::fs::write(
            &file,
            r#"<svg width="30mm" height="20mm" stroke="none"><polygon points="1,1 29,1 29,19 15,19 1,12" stroke="edge"/><line x1="2mm" y1="2mm" x2="28mm" y2="2mm" stroke="artwork"/></svg>"#,
        )
        .map_err(|e| e.to_string())?;
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = App::create(shared, Preferences::default(), None, None);
        app.refresh();
        let ctx = egui::Context::default();
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        authoring::tests::tap(&ctx, &mut app, "Settings")?;
        assert!(
            scrolled_to_at(&ctx, &mut app, "BOARD EDGE", 300.0)?,
            "the board's edge lives in its own plain-language section"
        );
        app.svg_path = file.to_string_lossy().into_owned();
        for _ in 0..2 {
            frame(
                &ctx,
                &mut app,
                vec![
                    egui::Event::PointerMoved(egui::pos2(1300.0, 500.0)),
                    egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Line,
                        delta: egui::vec2(0.0, -4.0),
                        phase: egui::TouchPhase::Move,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                egui::Modifiers::NONE,
            );
        }
        authoring::tests::tap(&ctx, &mut app, "Import as board edge")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        let corners = app.project.board.outline.as_ref().map_or(0, |o| o.len());
        assert!(
            corners >= 3,
            "the drawn shape became the board edge: {corners} corners"
        );
        assert!(
            paints_text(&ctx, &mut app, "Back to a plain rectangle")?,
            "the shape explains itself and offers a way back"
        );
        authoring::tests::tap(&ctx, &mut app, "Back to a plain rectangle")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert!(
            app.project.board.outline.is_none_or(|o| o.len() < 3),
            "the board is a plain rectangle again"
        );
        let _ = std::fs::remove_file(&file);
        Ok(())
    }

    #[test]
    fn switching_a_layer_off_hides_it_on_the_sheet_without_touching_the_design()
    -> Result<(), String> {
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = App::create(shared, Preferences::default(), None, None);
        app.refresh();
        app.tool_call("sample_design", json!({}));
        let revision = {
            let session = app.session.lock().map_err(|_| "session unavailable")?;
            session.revision
        };
        let wire_count = |app: &mut App, ctx: &egui::Context| -> Result<usize, String> {
            let output = frame(ctx, app, vec![], egui::Modifiers::NONE);
            Ok(output
                .shapes
                .iter()
                .filter(|shape| {
                    matches!(
                        &shape.shape,
                        egui::Shape::Path(path) if (path.stroke.width - 1.8).abs() < 0.01
                    )
                })
                .count())
        };
        let ctx = egui::Context::default();
        let before = wire_count(&mut app, &ctx)?;
        assert!(before > 0, "the worked example is wired up");
        authoring::tests::tap(&ctx, &mut app, "Layers")?;
        assert!(
            scrolled_to(&ctx, &mut app, "Wires")?,
            "the layers page lists wires as its own row"
        );
        authoring::tests::tap(&ctx, &mut app, "Wires")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert!(
            app.preferences.hidden_layers.iter().any(|h| h == "Wires"),
            "unticking the row hides the wires layer"
        );
        let after = wire_count(&mut app, &ctx)?;
        assert!(
            after < before,
            "the wires vanish from the sheet: {before} strokes became {after}"
        );
        let now = {
            let session = app.session.lock().map_err(|_| "session unavailable")?;
            session.revision
        };
        assert_eq!(
            now, revision,
            "hiding a layer is only a view change; the design itself is untouched"
        );
        Ok(())
    }

    #[test]
    fn selection_grows_handles_and_the_old_button_bar_is_gone() -> Result<(), String> {
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = App::create(shared, Preferences::default(), None, None);
        app.refresh();
        app.tool_call("sample_design", json!({}));
        let ctx = egui::Context::default();
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert!(
            !paints_text(&ctx, &mut app, "Select  Esc")?,
            "the wall of tool buttons is gone from the top"
        );
        assert!(
            !paints_text(&ctx, &mut app, "Sync to PCB >")?,
            "sync moved into the Edit menu"
        );
        assert!(
            paints_text(&ctx, &mut app, "Fit")?,
            "a floating Fit control sits in the work area"
        );
        let first = app.project.components[0].id;
        let before = app.project.components[0].rotation;
        app.select(Some(first));
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert!(
            paints_text(&ctx, &mut app, "Rotate")?,
            "a selected part grows a Rotate handle beside it"
        );
        authoring::tests::tap(&ctx, &mut app, "Rotate")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        let after = app
            .project
            .component(first)
            .map(|c| c.rotation)
            .unwrap_or(before);
        assert!(
            (after - before).abs() > 1e-6,
            "the handle turns the part: {before} -> {after}"
        );
        Ok(())
    }

    #[test]
    fn the_nets_tab_adds_names_and_shines_a_spotlight() -> Result<(), String> {
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = App::create(shared, Preferences::default(), None, None);
        app.refresh();
        app.tool_call("sample_design", json!({}));
        let before = app.project.nets.len();
        let ctx = egui::Context::default();
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        authoring::tests::tap(&ctx, &mut app, "Layers")?;
        assert!(
            scrolled_to(&ctx, &mut app, "New net")?,
            "the layers tree offers a way to add a connection"
        );
        authoring::tests::tap(&ctx, &mut app, "New net")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert_eq!(
            app.project.nets.len(),
            before + 1,
            "one click adds one named connection"
        );
        let added = app.project.nets.len() - 1;
        let fresh = app.project.nets[added].name.clone();
        assert!(
            scrolled_to(&ctx, &mut app, &fresh)?,
            "the new connection appears in the tree: {fresh}"
        );
        authoring::tests::tap(&ctx, &mut app, &fresh)?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert_eq!(
            app.highlight_net,
            Some(app.project.nets[added].id),
            "clicking a net lights it up on the board"
        );
        let fresh_id = app.project.nets[added].id;
        app.net_draft = Some((fresh_id, "Flashlight".into()));
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        authoring::tests::tap(&ctx, &mut app, "Use name")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert!(
            app.project
                .nets
                .iter()
                .any(|n| n.id == fresh_id && n.name == "Flashlight"),
            "the rename sticks: {}",
            app.message
        );
        Ok(())
    }

    #[test]
    fn the_make_files_section_writes_the_factory_folder() -> Result<(), String> {
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = App::create(shared, Preferences::default(), None, None);
        app.refresh();
        app.tool_call("sample_design", json!({}));
        let directory = std::env::temp_dir().join(format!("pcbcraft-make-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        app.make_dir = directory.to_string_lossy().into_owned();
        let revision = app.revision;
        let ctx = egui::Context::default();
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        for _ in 0..10 {
            frame(
                &ctx,
                &mut app,
                vec![
                    egui::Event::PointerMoved(egui::pos2(1300.0, 500.0)),
                    egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Line,
                        delta: egui::vec2(0.0, -4.0),
                        phase: egui::TouchPhase::Move,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                egui::Modifiers::NONE,
            );
        }
        authoring::tests::tap(&ctx, &mut app, "Settings")?;
        assert!(
            scrolled_to_at(&ctx, &mut app, "MAKE FILES", 300.0)?,
            "the settings page should offer the factory files"
        );
        authoring::tests::tap(&ctx, &mut app, "Copper & drill files")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        let listed = |directory: &std::path::Path| -> Result<Vec<String>, String> {
            std::fs::read_dir(directory)
                .map_err(|e| format!("the folder should exist: {e}"))?
                .filter_map(|entry| entry.ok())
                .map(|entry| Ok(entry.file_name().to_string_lossy().into_owned()))
                .collect()
        };
        let names = listed(&directory)?;
        assert!(
            !names.is_empty(),
            "copper and drill files should land in the folder: {}",
            app.message
        );
        authoring::tests::tap(&ctx, &mut app, "Parts list (CSV)")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        authoring::tests::tap(&ctx, &mut app, "Net list")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        authoring::tests::tap(&ctx, &mut app, "Pick and place file")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        authoring::tests::tap(&ctx, &mut app, "Design report")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        let names = listed(&directory)?;
        assert!(
            names.iter().any(|n| n.ends_with(".csv"))
                && names.iter().any(|n| n.ends_with(".net"))
                && names.iter().any(|n| n == "placement.txt")
                && names.iter().any(|n| n.ends_with(".txt")),
            "every factory file should land in the folder: {names:?}"
        );
        assert_eq!(
            app.revision, revision,
            "writing files leaves the design itself untouched"
        );
        let _ = std::fs::remove_dir_all(&directory);
        Ok(())
    }

    #[test]
    fn the_start_screen_is_the_first_thing_a_newcomer_sees() -> Result<(), String> {
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = App::create(shared, Preferences::default(), None, None);
        app.refresh();
        app.home_open = true;
        let ctx = egui::Context::default();
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert!(
            paints_text(&ctx, &mut app, "New design")?,
            "the start screen offers a blank page"
        );
        assert!(
            paints_text(&ctx, &mut app, "Worked example")?,
            "the start screen offers something to explore"
        );
        authoring::tests::tap(&ctx, &mut app, "Worked example")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert!(
            !app.home_open,
            "picking the example leaves the start screen"
        );
        assert!(
            !app.project.components.is_empty(),
            "the example fills the design from one click"
        );
        Ok(())
    }

    #[test]
    fn the_start_screen_lists_designs_you_can_open_again() -> Result<(), String> {
        let _turn = pcbcraft_engine::recents::RECENTS_TEST_LOCK
            .lock()
            .map_err(|_| "test lock unavailable")?;
        let directory =
            std::env::temp_dir().join(format!("pcbcraft-home-recents-{}", std::process::id()));
        std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
        *pcbcraft_engine::recents::RECENTS_PATH_TEST
            .lock()
            .map_err(|_| "test lock unavailable")? = Some(directory.join("recent-designs.json"));
        pcbcraft_engine::recents::clear_recent_paths();
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = App::create(shared, Preferences::default(), None, None);
        app.refresh();
        let file = directory
            .join("flashlight.pcbcraft")
            .to_string_lossy()
            .into_owned();
        app.path = file.clone();
        app.tool_call("sample_design", json!({}));
        app.tool_call("save", json!({"path": file}));
        app.home_open = true;
        let ctx = egui::Context::default();
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert!(
            paints_text(&ctx, &mut app, "flashlight.pcbcraft")?,
            "a design you saved is listed for one-click return"
        );
        let want = app.path.clone();
        authoring::tests::tap(&ctx, &mut app, &want)?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert!(
            !app.home_open,
            "clicking a recent design opens it and leaves the start screen"
        );
        pcbcraft_engine::recents::clear_recent_paths();
        *pcbcraft_engine::recents::RECENTS_PATH_TEST
            .lock()
            .map_err(|_| "test lock unavailable")? = None;
        let _ = std::fs::remove_dir_all(&directory);
        Ok(())
    }

    #[test]
    fn the_library_split_pages_now_open_one_unified_page() -> Result<(), String> {
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = App::create(shared, Preferences::default(), None, None);
        app.refresh();
        app.change_tab(Tab::Libraries);
        app.library.editor.mode = 1; // an old split-view selection
        let ctx = egui::Context::default();
        let output = frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert_eq!(
            app.library.editor.mode, 0,
            "the separate project page is gone; one library shows everything"
        );
        assert_eq!(output.viewport_output.len(), 1, "the unified page paints");
        Ok(())
    }

    #[test]
    fn the_library_browser_groups_collections_by_the_source_they_came_from() {
        let catalog = json!({"symbol_libraries":[
            {"name":"Device","path":"/installed/Device.kicad_sym","source":"Installed collections"},
            {"name":"DigiKeyParts","path":"/home/me/digikey/DigiKeyParts.kicad_sym","source":"Digi-Key shelf"},
            {"name":"R","path":"/installed/R.kicad_sym"}
        ]});
        let items = catalog["symbol_libraries"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        assert_eq!(
            library::entry_source(&items[2]),
            "Installed collections",
            "an entry without a source tag still answers with one"
        );
        assert_eq!(
            library::distinct_sources(&items),
            vec!["Digi-Key shelf", "Installed collections"],
            "every source is named once, in list order"
        );
    }

    #[test]
    fn the_libraries_tab_paints_the_unified_list_from_added_sources() -> Result<(), String> {
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = App::create(shared, Preferences::default(), None, None);
        app.refresh();
        app.change_tab(Tab::Libraries);
        app.library.catalog = Some(json!({"symbol_libraries":[
            {"name":"Device","path":"/installed/Device.kicad_sym","source":"Installed collections"},
            {"name":"DigiKeyParts","path":"/home/me/digikey/DigiKeyParts.kicad_sym","source":"Digi-Key shelf"}
        ],"footprint_libraries":[],"scan_truncated":false}));
        app.library.sources = Some(json!({"sources":[
            {"name":"Digi-Key shelf","path":"/home/me/digikey","kind":"added","symbol_libraries":1,"footprint_libraries":0},
            {"name":"Installed collections","path":"/usr/share/design-collections","kind":"built-in","symbol_libraries":1,"footprint_libraries":0}
        ]}));
        app.library.results = Some(json!({"total":2,"entries":[
            {"name":"Device","path":"/installed/Device.kicad_sym","kind":"symbol"},
            {"name":"DigiKeyParts","path":"/home/me/digikey/DigiKeyParts.kicad_sym","kind":"symbol"}
        ]}));
        let ctx = egui::Context::default();
        let output = frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert_eq!(
            output.viewport_output.len(),
            1,
            "the unified library view paints"
        );
        Ok(())
    }

    #[test]
    fn library_filter_narrows_listed_entries_by_name_or_description() {
        let led = json!({"name":"LED","description":"light emitting diode"});
        let resistor = json!({"name":"R","description":"resistor"});
        assert!(
            library::entry_matches(&led, ""),
            "an empty filter shows everything"
        );
        assert!(
            library::entry_matches(&led, "lig"),
            "descriptions match too"
        );
        assert!(
            library::entry_matches(&resistor, "RESIST"),
            "the filter ignores case"
        );
        assert!(!library::entry_matches(&resistor, "led"));
        assert!(!library::entry_matches(&led, "9"));
    }
    #[test]
    fn control_tooltips_come_from_the_command_registry() -> Result<(), String> {
        let registry = pcbcraft_engine::commands();
        for id in [
            "component.add",
            "board.sync",
            "board.resize",
            "object.delete",
        ] {
            let want = registry
                .iter()
                .find(|spec| spec.id == id)
                .map(|spec| spec.description)
                .unwrap_or_default();
            assert!(
                !want.is_empty() && command_tip(id) == want,
                "the tooltip for {id} must be the registry description"
            );
        }
        assert_eq!(
            command_tip("no.such_command"),
            "This control runs an engine command.",
            "an unmapped id still explains itself"
        );
        Ok(())
    }

    #[test]
    fn empty_states_name_the_command_that_fills_the_panel() {
        // Guidance must reference commands that actually exist, so a novice
        // who follows the hint always finds a real command to run.
        let registry: Vec<&str> = pcbcraft_engine::commands()
            .iter()
            .map(|spec| spec.id)
            .collect();
        for copy in [EMPTY_SYMBOLS, EMPTY_NETS, EMPTY_BOARD] {
            for name in ["component.add", "netlist.import", "board.sync"] {
                if copy.contains(name) {
                    assert!(registry.contains(&name), "{name} must exist");
                }
            }
            assert!(
                copy.contains("component.add")
                    || copy.contains("netlist.import")
                    || copy.contains("board.sync"),
                "empty guidance must name a filling command: {copy}"
            );
        }
        assert!(EMPTY_CHECKS.contains("automatically"));
    }

    #[test]
    fn no_control_asks_the_user_to_know_a_tool_brand() {
        let brand = ["Ki", "Cad"].concat();
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut offenders = Vec::new();
        for file in ["lib.rs", "canvas.rs", "library.rs", "authoring.rs"] {
            let text = std::fs::read_to_string(dir.join(file)).unwrap_or_default();
            for line in text.lines() {
                let code = line.split("//").next().unwrap_or("");
                if !code.contains('"') || !code.contains(&brand) {
                    continue;
                }
                let allowed = code.contains("credits the")
                    || code.contains("-derived data")
                    || code.contains("LICENSE-")
                    || code.contains(".contains(&brand)")
                    || code.contains("code.contains");
                if !allowed {
                    offenders.push(format!("{file}: {}", code.trim()));
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "buttons and labels must speak plainly, not name other tools: {offenders:?}"
        );
    }

    #[test]
    fn dragging_a_list_entry_onto_the_sheet_places_that_real_symbol() -> Result<(), String> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../library/tests/fixtures/device.kicad_sym")
            .to_string_lossy()
            .into_owned();
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = App::create(shared.clone(), Preferences::default(), None, None);
        app.refresh();
        assert!(
            !app.place_dropped_library(&json!({"kind": "footprint"}), Point::new(20.0, 15.0)),
            "a footprint entry cannot be dropped on the schematic"
        );
        assert!(app.place_dropped_library(
            &json!({"kind": "symbol", "path": path, "name": "R"}),
            Point::new(20.0, 15.0)
        ));
        let placed = shared
            .lock()
            .map_err(|_| "session unavailable")?
            .project
            .components
            .first()
            .cloned();
        let placed = placed.ok_or("the dropped entry never arrived")?;
        assert!(
            placed.library_symbol.is_some(),
            "the drop places the library's own definition, not a template"
        );
        assert!(
            (placed.position.x - 20.0).abs() < 2.0,
            "the part lands where the pointer let go"
        );
        assert!(
            matches!(app.tab, Tab::Schematic),
            "the sheet shows the drop"
        );
        Ok(())
    }

    #[test]
    fn designs_opened_on_this_computer_are_remembered_for_the_start_screen() -> Result<(), String> {
        let _turn = pcbcraft_engine::recents::RECENTS_TEST_LOCK
            .lock()
            .map_err(|_| "test lock unavailable")?;
        let directory =
            std::env::temp_dir().join(format!("pcbcraft-ui-recents-{}", std::process::id()));
        std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
        *pcbcraft_engine::recents::RECENTS_PATH_TEST
            .lock()
            .map_err(|_| "test lock unavailable")? = Some(directory.join("recent-designs.json"));
        pcbcraft_engine::recents::clear_recent_paths();
        pcbcraft_engine::recents::record_recent_path("/designs/flashlight.board.json");
        pcbcraft_engine::recents::record_recent_path("/designs/radio.board.json");
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = App::create(shared, Preferences::default(), None, None);
        app.refresh();
        assert_eq!(
            app.recent_designs,
            vec![
                "/designs/radio.board.json",
                "/designs/flashlight.board.json"
            ],
            "the newest design leads the list"
        );
        let ctx = egui::Context::default();
        let output = frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert_eq!(
            output.viewport_output.len(),
            1,
            "the app keeps the recent list ready for the Start screen"
        );
        pcbcraft_engine::recents::clear_recent_paths();
        *pcbcraft_engine::recents::RECENTS_PATH_TEST
            .lock()
            .map_err(|_| "test lock unavailable")? = None;
        let _ = std::fs::remove_dir_all(&directory);
        Ok(())
    }

    #[test]
    fn every_control_hovers_a_plain_sentence_for_a_first_time_user() {
        let click = [".button", "("].concat();
        let pick = [".selectable_", "label("].concat();
        let choice = [".selectable_", "value("].concat();
        let tip = ["on_hover", "_text"].concat();
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut offenders = Vec::new();
        for file in ["lib.rs", "canvas.rs", "library.rs", "authoring.rs"] {
            let text = std::fs::read_to_string(dir.join(file)).unwrap_or_default();
            let lines: Vec<&str> = text.lines().collect();
            for (n, line) in lines.iter().enumerate() {
                let code = line.split("//").next().unwrap_or("");
                if !(code.contains(&click) || code.contains(&pick) || code.contains(&choice)) {
                    continue;
                }
                let window: String = lines[n.min(n)..(n + 10).min(lines.len())]
                    .iter()
                    .map(|l| l.split("//").next().unwrap_or(""))
                    .collect();
                if !window.contains(&tip) {
                    offenders.push(format!("{file}:{}: {}", n + 1, code.trim()));
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "every clickable control must explain itself on hover: {offenders:?}"
        );
    }

    #[test]
    fn every_editable_field_explains_its_bounds_and_effect() {
        // The help copy for the inspector's free-form fields must state the
        // same limits the engine enforces, so a novice never has to guess
        // why a value was refused.
        assert!(FIELD_HELP_VALUE.contains("name or value"));
        assert!(FIELD_HELP_POSITION.contains("origin"));
        assert!(FIELD_HELP_BOARD.contains("larger than zero"));
        assert!(FIELD_HELP_CUTOUT.contains("inside the board"));
    }

    #[test]
    fn shortcut_list_documents_every_binding_the_app_installs() {
        let rows = shortcut_rows();
        let keys: Vec<&str> = rows.iter().map(|(keys, _, _)| *keys).collect();
        for wanted in ["Ctrl+Z", "Ctrl+K", "?", "W", "X", "Delete", "Esc"] {
            assert!(
                keys.contains(&wanted),
                "the shortcut list must mention {wanted}"
            );
        }
        let mut seen: Vec<&str> = keys.clone();
        seen.sort_unstable();
        let unique = {
            let mut v = seen.clone();
            v.dedup();
            v
        };
        assert_eq!(seen.len(), unique.len(), "no key is listed twice");
        for (keys, action, context) in &rows {
            assert!(!keys.is_empty() && !action.is_empty());
            assert!(
                [
                    "anywhere",
                    "schematic tab",
                    "pcb tab",
                    "canvas",
                    "command palette",
                ]
                .contains(context),
                "{keys} claims an unknown context {context}"
            );
        }
    }

    #[test]
    fn tool_hints_name_the_next_action_for_every_tool() {
        let cases = [
            (Tool::Place("resistor".into()), "symbol"),
            (Tool::Wire(None), "first pin"),
            (
                Tool::Wire(Some(PinRef {
                    component: 1,
                    pin: "1".into(),
                })),
                "second pin",
            ),
            (Tool::Route(None), "first pad"),
            (Tool::Route(Some((1, "1".into()))), "second pad"),
            (Tool::Move, "drop"),
            (Tool::Cutout, "cutout"),
            (Tool::Hole, "mounting hole"),
            (Tool::NoConnect, "unused"),
            (Tool::Fiducial, "assembly"),
        ];
        for (tool, verb) in cases {
            let hint = tool_hint(&tool);
            assert!(
                hint.contains(verb) && hint.contains("LMB") && hint.contains("Esc"),
                "{verb} must be spelled out for the active tool: {hint}"
            );
        }
    }

    #[test]
    fn the_nets_tab_reports_the_copper_each_net_carries() -> Result<(), String> {
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = App::create(shared, Preferences::default(), None, None);
        app.refresh();
        app.tool_call("sample_design", json!({}));
        app.run(
            "track.route",
            json!({"from":{"footprint":10,"pad":"1"},"to":{"footprint":11,"pad":"1"},"layer":"F.Cu"}),
        );
        let ctx = egui::Context::default();
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        authoring::tests::tap(&ctx, &mut app, "Layers")?;
        assert!(
            scrolled_to(&ctx, &mut app, "of 3 nets routed")?,
            "the dock totals the routing at a glance: {}",
            app.message
        );
        assert!(
            scrolled_to(&ctx, &mut app, "not routed yet")?,
            "a connection without copper says so plainly: {}",
            app.message
        );
        assert!(
            paints_text(&ctx, &mut app, "mm")?,
            "routed lengths are spelled out in millimetres: {}",
            app.message
        );
        Ok(())
    }

    #[test]
    fn clicking_a_pad_starts_a_track_without_any_button() -> Result<(), String> {
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = App::create(shared, Preferences::default(), None, None);
        app.refresh();
        app.tool_call("sample_design", json!({}));
        app.tab = Tab::Pcb;
        let from = app
            .project
            .footprint(10)
            .and_then(|f| f.pad_position("1"))
            .ok_or("the sample should have a footprint 10 with a pad 1")?;
        let to = app
            .project
            .footprint(11)
            .and_then(|f| f.pad_position("1"))
            .ok_or("the sample should have a footprint 11 with a pad 1")?;
        let before = app.project.board.tracks.len();
        app.canvas_click(from, 1.0);
        assert!(
            matches!(&app.tool, Tool::Route(Some((id, pad))) if *id == 10 && pad == "1"),
            "clicking a pad starts a track: {:?}",
            app.tool
        );
        app.canvas_click(to, 1.0);
        assert!(
            app.project.board.tracks.len() > before,
            "the two pads are joined by a track: {} -> {} · {}",
            before,
            app.project.board.tracks.len(),
            app.message
        );
        assert!(
            matches!(app.tool, Tool::Select),
            "finishing a track hands the mouse straight back: {:?}",
            app.tool
        );
        Ok(())
    }

    #[test]
    fn clicking_a_pin_starts_a_wire_without_any_button() -> Result<(), String> {
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = App::create(shared, Preferences::default(), None, None);
        app.refresh();
        app.tab = Tab::Schematic;
        app.tool_call(
            "component.add",
            json!({"kind":"resistor","position":{"x":0.0,"y":0.0}}),
        );
        app.tool_call(
            "component.add",
            json!({"kind":"resistor","position":{"x":20.0,"y":0.0}}),
        );
        let first = app.project.components[0].clone();
        let second = app.project.components[1].clone();
        let from = first
            .pin_position("1")
            .ok_or("a resistor should have a pin 1")?;
        let to = second
            .pin_position("1")
            .ok_or("a resistor should have a pin 1")?;
        app.canvas_click(from, 1.0);
        assert!(
            matches!(&app.tool, Tool::Wire(Some(pin)) if pin.component == first.id && pin.pin == "1"),
            "clicking a pin starts a wire: {:?}",
            app.tool
        );
        app.canvas_click(to, 1.0);
        assert_eq!(
            app.project.wires.len(),
            1,
            "the two pins are wired together: {}",
            app.message
        );
        assert!(
            matches!(app.tool, Tool::Select),
            "finishing a wire hands the mouse straight back: {:?}",
            app.tool
        );
        Ok(())
    }

    #[test]
    fn an_idle_board_shows_no_mouse_legend() -> Result<(), String> {
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = App::create(shared, Preferences::default(), None, None);
        app.refresh();
        app.tool_call("sample_design", json!({}));
        let ctx = egui::Context::default();
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert!(
            !paints_text(&ctx, &mut app, "LMB:")?,
            "the mouse legend only appears while a tool waits for a click"
        );
        Ok(())
    }

    #[test]
    fn palette_highlight_walks_the_list_as_a_ring() {
        assert_eq!(step_index(0, 5, 1), 1);
        assert_eq!(
            step_index(4, 5, 1),
            0,
            "walking past the last row wraps to the first"
        );
        assert_eq!(
            step_index(0, 5, -1),
            4,
            "walking above the first row wraps to the last"
        );
        assert_eq!(step_index(9, 3, 0), 2, "a stale index is pulled back");
        assert_eq!(step_index(0, 0, 1), 0, "an empty list stays put");
    }

    #[test]
    fn palette_filtering_matches_subsequences_of_names_and_descriptions() {
        assert!(fuzzy_match("sync", "board.sync"));
        assert!(fuzzy_match("bre", "board.resize"));
        assert!(fuzzy_match("PAD", "board.pad_stack"));
        assert!(
            !fuzzy_match("xyz", "board.sync"),
            "characters that are not there never match"
        );
        assert!(
            !fuzzy_match("copperplating", "component.value"),
            "order matters in a subsequence"
        );
    }

    #[test]
    fn palette_help_explains_every_required_parameter() -> Result<(), String> {
        let spec = pcbcraft_engine::commands()
            .into_iter()
            .find(|spec| spec.id == "padstack.define")
            .ok_or_else(|| "padstack.define must be a command".to_string())?;
        let help = param_help(&spec);
        assert_eq!(
            help.len(),
            spec.input_schema
                .get("required")
                .and_then(Value::as_array)
                .map_or(0, |list| list.len()),
            "one help line per required parameter"
        );
        let joined = help.join("\n");
        assert!(joined.contains("name (text)"), "name is text: {joined}");
        assert!(
            joined.contains("circle or rect or oval or roundrect"),
            "the shape options are listed: {joined}"
        );
        assert!(
            joined.contains("number 0.05 to 100"),
            "the copper bounds are stated before the first refusal: {joined}"
        );
        for spec in pcbcraft_engine::commands() {
            assert_eq!(
                param_help(&spec).len(),
                spec.input_schema
                    .get("required")
                    .and_then(Value::as_array)
                    .map_or(0, |list| list.len()),
                "{} must explain exactly its required parameters",
                spec.id
            );
        }
        Ok(())
    }

    #[test]
    fn palette_templates_hold_exactly_the_required_parameters() -> Result<(), String> {
        let mut checked = 0usize;
        for spec in pcbcraft_engine::commands() {
            let template: Value =
                serde_json::from_str(&params_template(&spec)).map_err(|e| e.to_string())?;
            let object = template
                .as_object()
                .ok_or_else(|| format!("{} template is not an object", spec.id))?;
            let required = spec
                .input_schema
                .get("required")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            assert_eq!(
                object.len(),
                required.len(),
                "{} template must hold exactly the required parameters",
                spec.id
            );
            for key in required.iter().filter_map(Value::as_str) {
                assert!(
                    object.contains_key(key),
                    "{} template misses the required parameter {key}",
                    spec.id
                );
            }
            checked += 1;
        }
        assert!(checked >= 150, "the sweep covered the whole registry");
        Ok(())
    }

    #[test]
    fn a_beginners_first_session_places_two_parts_and_walks_every_tab() -> Result<(), String> {
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = App::create(shared.clone(), Preferences::default(), None, None);
        app.refresh();
        let ctx = egui::Context::default();
        let key = |key, modifiers| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        };
        assert!(
            matches!(app.tab, Tab::Schematic),
            "the app opens on the schematic"
        );
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);

        // Step 1: press A, pick the first part from the palette, click the sheet.
        frame(
            &ctx,
            &mut app,
            vec![key(egui::Key::A, egui::Modifiers::NONE)],
            egui::Modifiers::NONE,
        );
        assert!(app.palette, "A opens the symbol palette");
        app.palette = false;
        app.tool = Tool::Place(TEMPLATES[0].kind.into());
        for pressed in [true, false] {
            frame(
                &ctx,
                &mut app,
                vec![
                    egui::Event::PointerMoved(egui::Pos2::new(680.0, 360.0)),
                    egui::Event::PointerButton {
                        pos: egui::Pos2::new(680.0, 360.0),
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                egui::Modifiers::NONE,
            );
        }
        assert_eq!(
            shared
                .lock()
                .map_err(|_| "session unavailable")?
                .project
                .components
                .len(),
            1,
            "the first part lands on the sheet"
        );

        // Step 2: open the command console with Ctrl+K and add a second part.
        let ctrl = egui::Modifiers {
            ctrl: true,
            command: true,
            ..Default::default()
        };
        frame(&ctx, &mut app, vec![key(egui::Key::K, ctrl)], ctrl);
        assert!(app.command_open, "Ctrl+K opens the command console");
        app.command_query = "component.add".into();
        app.params = json!({"kind":"resistor","position":{"x":40.0,"y":30.0}}).to_string();
        app.execute_palette_params();
        assert_eq!(
            shared
                .lock()
                .map_err(|_| "session unavailable")?
                .project
                .components
                .len(),
            2,
            "the console adds the second part"
        );
        assert!(
            app.message.contains("committed"),
            "the console tells the user what happened"
        );

        // Step 3: every tab paints, and each one shows something useful.
        for tab in [Tab::Pcb, Tab::Libraries, Tab::Checks, Tab::Schematic] {
            app.change_tab(tab);
            let output = frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
            assert_eq!(
                output.viewport_output.len(),
                1,
                "the tab a beginner just clicked must paint, not throw"
            );
        }
        assert!(
            app.checks.get("erc").is_some() && app.checks.get("drc").is_some(),
            "the checks tab has real results to show"
        );
        Ok(())
    }

    #[test]
    fn a_command_with_a_multi_line_parameter_box_still_runs() -> Result<(), String> {
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = App::create(shared.clone(), Preferences::default(), None, None);
        app.refresh();
        app.command = "board.set_outline".into();
        app.params = "{\n  \"points\": [\n    { \"x\": 0.0, \"y\": 0.0 },\n    { \"x\": 80.0, \"y\": 0.0 },\n    { \"x\": 80.0, \"y\": 50.0 },\n    { \"x\": 0.0, \"y\": 50.0 }\n  ]\n}".into();
        app.execute_palette_params();
        assert!(
            app.message.contains("committed"),
            "a boxed outline typed over several lines runs just the same"
        );
        let width = shared
            .lock()
            .map_err(|_| "session unavailable")?
            .project
            .board
            .width;
        assert!(width > 70.0, "the outline from the box sized the board");
        Ok(())
    }

    #[test]
    fn copy_plot_renders_the_current_view_for_the_clipboard() -> Result<(), String> {
        let session =
            pcbcraft_mcp::shared(pcbcraft_engine::Session::sample().map_err(|e| e.to_string())?);
        let mut app = App::create(session, Preferences::default(), None, None);
        app.refresh();
        let ctx = egui::Context::default();
        app.copy_plot_to_clipboard(&ctx);
        assert_eq!(app.message, "plot copied to the clipboard");
        app.tab = Tab::Schematic;
        app.copy_plot_to_clipboard(&ctx);
        assert_eq!(app.message, "plot copied to the clipboard");
        Ok(())
    }

    #[test]
    fn tab_click_preserves_component_identity_and_rotation_undo_works() -> Result<(), String> {
        let session =
            pcbcraft_mcp::shared(pcbcraft_engine::Session::sample().map_err(|e| e.to_string())?);
        let mut app = App::create(session, Preferences::default(), None, None);
        app.refresh();
        app.select(Some(2));
        let ctx = egui::Context::default();
        let output = frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert_eq!(
            output.viewport_output.len(),
            1,
            "editor must have only its root window"
        );
        authoring::tests::tap(&ctx, &mut app, "Layout")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert!(matches!(app.tab, Tab::Pcb));
        assert_eq!(app.selected, Some(11));
        let key = |key, modifiers| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        };
        frame(
            &ctx,
            &mut app,
            vec![key(egui::Key::R, egui::Modifiers::NONE)],
            egui::Modifiers::NONE,
        );
        assert_eq!(app.project.footprint(11).map(|f| f.rotation), Some(90.0));
        let ctrl = egui::Modifiers {
            ctrl: true,
            command: true,
            ..Default::default()
        };
        frame(&ctx, &mut app, vec![key(egui::Key::Z, ctrl)], ctrl);
        assert_eq!(app.project.footprint(11).map(|f| f.rotation), Some(0.0));
        Ok(())
    }
    #[test]
    fn library_placement_uses_shared_engine_and_single_viewport() -> Result<(), String> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../library/tests/fixtures/device.kicad_sym")
            .to_string_lossy()
            .into_owned();
        let def = pcbcraft_engine::library::symbol(std::path::Path::new(&path), "R")?;
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = App::create(shared.clone(), Preferences::default(), None, None);
        app.refresh();
        app.tool = Tool::LibraryPlace(path, "R".into(), Box::new(def));
        let ctx = egui::Context::default();
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        let pos = egui::Pos2::new(680.0, 360.0);
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
                egui::Modifiers::NONE,
            );
        }
        let project = shared
            .lock()
            .map_err(|_| "session unavailable")?
            .project
            .clone();
        assert_eq!(project.components.len(), 1);
        assert!(
            project
                .components
                .first()
                .is_some_and(|c| c.library_symbol.is_some())
        );
        app.change_tab(Tab::Libraries);
        assert_eq!(app.selected, project.components.first().map(|c| c.id));
        let output = frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert_eq!(output.viewport_output.len(), 1);
        Ok(())
    }
    #[test]
    fn selecting_units_and_new_placement_preserves_correct_target() -> Result<(), String> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../library/tests/fixtures/lm358.kicad_sym")
            .to_string_lossy()
            .into_owned();
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = App::create(shared, Preferences::default(), None, None);
        app.refresh();
        app.run(
            "component.place_library",
            json!({"path":path,"symbol":"LM358","position":{"x":20,"y":15}}),
        );
        let pos = app
            .project
            .component(1)
            .and_then(|c| c.unit_placement(2))
            .ok_or("missing unit")?
            .0;
        app.select_at(pos, 11.0);
        assert_eq!(app.selected, Some(1));
        assert_eq!(app.selected_unit, 2);
        app.rotate();
        assert_eq!(
            app.project
                .component(1)
                .and_then(|c| c.unit_placement(2))
                .map(|p| p.1),
            Some(90.0)
        );
        assert_eq!(app.project.component(1).map(|c| c.rotation), Some(0.0));
        app.run(
            "component.add",
            json!({"kind":"resistor","position":{"x":60,"y":40}}),
        );
        assert_eq!(app.selected_unit, 1);
        app.rotate();
        assert_eq!(app.project.component(2).map(|c| c.rotation), Some(90.0));
        Ok(())
    }
    #[test]
    fn agent_changes_refresh_the_displayed_document() -> Result<(), String> {
        let shared =
            pcbcraft_mcp::shared(pcbcraft_engine::Session::sample().map_err(|e| e.to_string())?);
        let mut app = App::create(shared.clone(), Preferences::default(), None, None);
        app.refresh();
        let result =
            pcbcraft_mcp::call_tool(&shared, "component.value", &json!({"id":2,"value":"470 Ω"}));
        assert_eq!(result.get("isError"), Some(&json!(false)));
        app.refresh();
        assert_eq!(
            app.project.component(2).map(|c| c.value.as_str()),
            Some("470 Ω")
        );
        Ok(())
    }
}

#[cfg(test)]
mod e2e;
