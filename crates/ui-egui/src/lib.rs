//! One window, shared project, tabbed workspaces. Edits use the engine exclusively.
mod a11y;
mod assistant;
mod authoring;
mod board3d;
mod canvas;
mod datums;
mod editing;
mod footprint_wizard;
mod impedance_panel;
mod keymap;
mod library;
mod library_quality;
mod plot_viewer;
mod reference_panel;
mod sources;
mod via_covering;
mod via_span;
use egui::{Align2, Color32, FontId, Rect, Stroke, Vec2};
use pcbcraft_doc::{Id, PinRef, Project, TEMPLATES};
use pcbcraft_geom::Point;
use pcbcraft_mcp::SharedSession;
use pcbcraft_text::t;
use std::collections::HashMap;
#[cfg(test)]
use std::collections::BTreeMap;

/// Guidance shown when a panel section has no content yet: what belongs
/// there and the command that fills it, so a first-run user is never left
/// staring at a blank list. Each constant is a catalog key (en.txt), read
/// through `empty_state`.
#[cfg(test)]
const EMPTY_SYMBOLS: &str = "empty.symbols";
const EMPTY_NETS: &str = "empty.nets";
#[cfg(test)]
const EMPTY_BOARD: &str = "empty.board";
const EMPTY_CHECKS: &str = "empty.checks";

/// Render the empty-state line for catalog key `key` in the small muted style.
fn empty_state(ui: &mut egui::Ui, key: &str, muted: egui::Color32) {
    ui.label(
        egui::RichText::new(pcbcraft_text::lookup(key, &[]))
            .small()
            .color(muted),
    );
}

/// Help copy shown under the inspector's editable fields. Each line names
/// what the field is for and the limit the engine will enforce, so the
/// requirement is visible before the first rejected edit.
const FIELD_HELP_VALUE: &str = "field_help.value";
const FIELD_HELP_POSITION: &str = "field_help.position";
const FIELD_HELP_BOARD: &str = "field_help.board";
const FIELD_HELP_CUTOUT: &str = "field_help.cutout";

/// Show the help line for catalog key `key` under an inspector field.
fn field_help(ui: &mut egui::Ui, key: &str, muted: egui::Color32) {
    ui.label(
        egui::RichText::new(pcbcraft_text::lookup(key, &[]))
            .small()
            .color(muted)
            .weak(),
    );
}

/// The complete input reference: every key binding the app installs, plus
/// the canvas mouse gestures, with where each one applies. The overlay and
/// the tests read this one table, so the list can never claim a key the
/// handler does not install.
fn shortcut_rows() -> Vec<(String, String, String)> {
    vec![
        (
            "Ctrl+Z".to_string(),
            t!("shortcut.undo_the_last_action"),
            t!("shortcut.context.anywhere"),
        ),
        (
            "Ctrl+Shift+Z".to_string(),
            t!("shortcut.redo_an_undone_action"),
            t!("shortcut.context.anywhere"),
        ),
        (
            "Ctrl+Y".to_string(),
            t!("shortcut.redo_an_undone_action"),
            t!("shortcut.context.anywhere"),
        ),
        (
            "Ctrl+S".to_string(),
            t!("shortcut.save_the_design_to_its"),
            t!("shortcut.context.anywhere"),
        ),
        (
            "Ctrl+K".to_string(),
            t!("shortcut.open_the_command_palette"),
            t!("shortcut.context.anywhere"),
        ),
        (
            t!("shortcut.keys.up_down"),
            t!("shortcut.move_the_highlight_through_the"),
            t!("shortcut.context.command_palette"),
        ),
        (
            "Enter".to_string(),
            t!("shortcut.load_the_highlighted_command_then"),
            t!("shortcut.context.command_palette"),
        ),
        (
            "?".to_string(),
            t!("shortcut.show_or_hide_this_shortcut"),
            t!("shortcut.context.anywhere"),
        ),
        (
            "Ctrl+C".to_string(),
            t!("shortcut.copy_the_selected_objects_as"),
            t!("shortcut.context.canvas"),
        ),
        (
            "Ctrl+V".to_string(),
            t!("shortcut.paste_the_copied_objects_ready"),
            t!("shortcut.context.canvas"),
        ),
        (
            "Ctrl+A".to_string(),
            t!("shortcut.select_every_object_the_pick"),
            t!("shortcut.context.canvas"),
        ),
        (
            t!("shortcut.keys.shift_click"),
            t!("shortcut.add_an_object_to_the"),
            t!("shortcut.context.canvas"),
        ),
        (
            t!("shortcut.keys.drag_on_empty_canvas"),
            t!("shortcut.draw_a_box_and_select"),
            t!("shortcut.context.canvas"),
        ),
        (
            "U".to_string(),
            t!("shortcut.measure_tool_click_two_points"),
            t!("shortcut.context.canvas"),
        ),
        (
            "A".to_string(),
            t!("shortcut.open_the_symbol_palette"),
            t!("shortcut.context.schematic_tab"),
        ),
        (
            "W".to_string(),
            t!("shortcut.wire_tool_click_two_pins"),
            t!("shortcut.context.schematic_tab"),
        ),
        (
            "X".to_string(),
            t!("shortcut.route_tool_click_two_pads"),
            t!("shortcut.context.pcb_tab"),
        ),
        (
            "M".to_string(),
            t!("shortcut.move_tool_drop_the_selection"),
            t!("shortcut.context.anywhere"),
        ),
        (
            "R".to_string(),
            t!("shortcut.rotate_the_hovered_or_selected"),
            t!("shortcut.context.anywhere"),
        ),
        (
            "E".to_string(),
            t!("shortcut.load_the_hovered_or_selected"),
            t!("shortcut.context.anywhere"),
        ),
        (
            "Home".to_string(),
            t!("shortcut.zoom_the_view_so_the"),
            t!("shortcut.context.anywhere"),
        ),
        (
            "Delete".to_string(),
            t!("shortcut.delete_every_selected_object"),
            t!("shortcut.context.anywhere"),
        ),
        (
            "Esc".to_string(),
            t!("shortcut.return_to_the_select_tool"),
            t!("shortcut.context.anywhere"),
        ),
        (
            t!("shortcut.keys.middle_drag"),
            t!("shortcut.pan_the_view"),
            t!("shortcut.context.canvas"),
        ),
        (
            t!("shortcut.keys.scroll"),
            t!("shortcut.zoom_the_view"),
            t!("shortcut.context.canvas"),
        ),
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
    String,
    String,
    Vec<(Option<Id>, String)>,
    String,
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
    let key = format!("check.{code}");
    let named = pcbcraft_text::lookup(&key, &[]);
    if named != key {
        return named;
    }
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
/// The plain measurements a designer can write a check about, as a catalog
/// key for the words and the token the engine stores for each.
const CUSTOM_MEASURES: [(&str, &str); 5] = [
    ("checks.measure.track_width_below", "track width below"),
    ("checks.measure.track_length_above", "track length above"),
    ("checks.measure.part_count_above", "part count above"),
    ("checks.measure.via_count_above", "via count above"),
    (
        "checks.measure.net_track_count_above",
        "net track count above",
    ),
];
/// One line in plain words for a check the designer wrote, e.g. "more than
/// 5 parts" or "wires thinner than 0.20 mm".
fn custom_measure_line(measure: &str, limit: f64) -> String {
    let n = if measure.contains("count") {
        limit as i64
    } else {
        // counts print whole; wire measures keep two decimals
        return t!(
            "checks.custom_measure_line.what_limit_mm",
            what = match measure {
                "track width below" => t!("checks.custom_measure_line.wires_thinner_than"),
                "track length above" => t!("checks.custom_measure_line.wires_longer_than"),
                other => other.to_owned(),
            },
            limit = format!("{:.2}", limit)
        );
    };
    match measure {
        "part count above" => t!("checks.custom_measure_line.more_than_n_parts", n = n),
        "via count above" => t!("checks.custom_measure_line.more_than_n_vias", n = n),
        "net track count above" => t!("checks.custom_measure_line.more_than_n_wires_on", n = n),
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
        .map(|c| t!("checks.custom_heading.your_check_name", name = c.name));
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

/// The words shown beside a layer checkbox. The English name is the stored
/// id in the preferences, so only the label is translated.
fn layer_row_label(name: &str) -> String {
    match name {
        "Grid" => t!("board.layers_tab.grid"),
        "Symbols" => t!("board.layers_tab.symbols"),
        "Wires" => t!("board.layers_tab.wires"),
        "Net labels" => t!("board.layers_tab.net_labels"),
        "Graphics" => t!("board.layers_tab.graphics"),
        "Top copper" => t!("board.layers_tab.top_copper"),
        "Bottom copper" => t!("board.layers_tab.bottom_copper"),
        "Pads" => t!("board.layers_tab.pads"),
        "Vias" => t!("board.layers_tab.vias"),
        "Copper zones" => t!("board.layers_tab.copper_zones"),
        "Cutouts" => t!("board.layers_tab.cutouts"),
        "Package artwork" => t!("board.layers_tab.package_artwork"),
        other => other.to_owned(),
    }
}

/// What the mouse and keys will do next for the active tool. The strip is
/// rendered under the toolbar so the next action is always visible instead
/// of being something the user has to remember.
fn tool_hint(tool: &Tool) -> String {
    let common = format!(" {}", t!("board.tool_hint.middle_drag_pan_scroll_zoom"));
    match tool {
        Tool::Select => t!("board.tool_hint.lmb_select_an_object_drag", common = common),
        Tool::Place(_) => t!("board.tool_hint.lmb_place_the_symbol_at", common = common),
        Tool::LibraryPlace(..) | Tool::CustomPlace(..) => {
            t!(
                "board.tool_hint.lmb_place_the_symbol_package",
                common = common
            )
        }
        Tool::Wire(None) => t!("board.tool_hint.lmb_pick_the_first_pin", common = common),
        Tool::Wire(Some(_)) => {
            t!("board.tool_hint.lmb_pick_the_second_pin", common = common)
        }
        Tool::Route(None) => t!("board.tool_hint.lmb_pick_the_first_pad", common = common),
        Tool::Route(Some(_)) => {
            t!("board.tool_hint.lmb_pick_the_second_pad", common = common)
        }
        Tool::Move => t!(
            "board.tool_hint.lmb_drop_the_selected_object",
            common = common
        ),
        Tool::Cutout => t!(
            "board.tool_hint.lmb_place_a_circular_cutout",
            common = common
        ),
        Tool::Hole => t!("board.tool_hint.lmb_drill_a_mounting_hole", common = common),
        Tool::NoConnect => {
            t!("board.tool_hint.lmb_click_a_pin_to", common = common)
        }
        Tool::Fiducial => t!(
            "board.tool_hint.lmb_place_an_assembly_mark",
            common = common
        ),
        Tool::DrawLayer => {
            t!("board.tool_hint.lmb_place_the_shape_on", common = common)
        }
        Tool::Via => format!("LMB: place a via at the cursor{common}"),
        Tool::Measure => {
            t!("board.tool_hint.lmb_click_two_points_to", common = common)
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
                            None => line.push_str(&format!(" {}", t!("rules.param_help.required"))),
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
            return t!(
                "schematic.type_note.one_of_list",
                list = names.join(&format!(" {} ", t!("schematic.type_note.or")))
            );
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
                (Some(lo), Some(hi)) => t!("schematic.type_note.number_lo_to_hi", lo = lo, hi = hi),
                (Some(lo), None) => t!("schematic.type_note.number_from_lo", lo = lo),
                (None, Some(hi)) => t!("schematic.type_note.number_up_to_hi", hi = hi),
                (None, None) => t!("schematic.type_note.number"),
            }
        }
        Some("string") => t!("schematic.type_note.text"),
        Some("boolean") => t!("schematic.type_note.true_or_false"),
        Some("array") => t!("schematic.type_note.list"),
        Some("object") => t!("schematic.type_note.object"),
        _ => t!("schematic.type_note.value"),
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

/// The catalog key that stands for one engine refusal: `error.` plus the
/// refusal's words in lowercase, joined by underscores and cut at 60 characters.
fn refusal_key(engine_text: &str) -> String {
    let words: Vec<String> = engine_text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect();
    let joined = words.join("_");
    let cut: String = joined.chars().take(60).collect();
    format!("error.{cut}")
}

/// What the status strip shows for an engine refusal. The engine text is API
/// text and stays English; a language that carries an `error.*` line for it
/// shows that line instead, and every other refusal (or the same words with
/// other spelling) falls back to the engine text unchanged.
fn refusal_text(engine_text: &str) -> String {
    let key = refusal_key(engine_text);
    let known = pcbcraft_text::Catalog::english();
    if known.defines(&key) && known.get(&key) == engine_text {
        pcbcraft_text::lookup_known(&key).unwrap_or_else(|| engine_text.to_owned())
    } else {
        engine_text.to_owned()
    }
}

/// Hover text for a control that runs an engine command: the command
/// registry description is the single source of truth for what the control
/// does, so the UI explanation and the command documentation never drift.
/// A `command.<id>.tip` line in the catalog overrides it per language.
fn command_tip(id: &str) -> String {
    let key = format!("command.{id}.tip");
    let translated = pcbcraft_text::lookup(&key, &[]);
    if translated != key {
        return translated;
    }
    pcbcraft_engine::commands()
        .into_iter()
        .find(|spec| spec.id == id)
        .map_or_else(
            || t!("command.unmapped.tip"),
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
    PlotFiles,
    Libraries,
    PlotFiles,
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
    fn name(&self) -> String {
        match self {
            Self::Number => t!("pad_labels.number"),
            Self::Net => t!("pad_labels.net"),
            Self::Off => t!("pad_labels.off"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CompareKind {
    Added,
    Changed,
}
#[derive(Clone, Serialize, Deserialize)]
struct Preferences {
    dark: bool,
    grid: bool,
    accent: [u8; 3],
    pad_labels: PadLabels,
    #[serde(default)]
    hidden_layers: Vec<String>,
    #[serde(default = "default_keymap_profile")]
    active_keymap_profile: String,
    #[serde(default = "default_keymap_profiles")]
    keymap_profiles: HashMap<String, keymap::Keymap>,
    /// Vertical split in the right aside: fraction for the Layers tree (top).
    #[serde(default = "default_aside_split")]
    aside_split: f32,
    /// Lower right-aside tab: 0 = Properties, 1 = Assistant.
    #[serde(default)]
    aside_lower_tab: u8,
    #[serde(default = "default_true")]
    library_dock_open: bool,
    /// Language tag of the text catalog, or `system` to follow the computer.
    #[serde(default = "default_language")]
    language: String,
    /// Interface scale, 0.75 to 2.0; 1.0 is normal size.
    #[serde(default = "default_ui_scale")]
    ui_scale: f32,
    /// Cut routed corners at 45 degrees; off allows right-angle corners.
    #[serde(default = "default_true")]
    chamfer_corners: bool,
    /// Show the Assistant tab and its shortcut; off starts nothing.
    #[serde(default = "default_true")]
    assistant_enabled: bool,
    /// Saved model connections. Keys are never kept here, only in the system keychain.
    #[serde(default)]
    assistant_connections: Vec<pcbcraft_assistant::Connection>,
    /// Id of the connection used by default.
    #[serde(default)]
    assistant_default: String,
    /// What the assistant may do without asking.
    #[serde(default)]
    assistant_mode: pcbcraft_assistant::Mode,
    /// Per-connection override of the mode, by connection id.
    #[serde(default)]
    assistant_modes: std::collections::BTreeMap<String, pcbcraft_assistant::Mode>,
    /// Tool keys the user chose to always allow, by connection id.
    #[serde(default)]
    assistant_always_allow: std::collections::BTreeMap<String, Vec<String>>,
    /// Changes one message may make before the model is told to stop.
    #[serde(default = "default_turn_limit")]
    assistant_turn_limit: u32,
}
fn default_language() -> String {
    pcbcraft_text::SYSTEM.into()
}
fn default_keymap_profile() -> String {
    "Default".into()
}
fn default_keymap_profiles() -> HashMap<String, keymap::Keymap> {
    let mut map = HashMap::new();
    map.insert("Default".into(), keymap::Keymap::default_bindings());
    map
}
fn default_turn_limit() -> u32 {
    pcbcraft_assistant::DEFAULT_TURN_LIMIT
}
fn default_ui_scale() -> f32 {
    a11y::DEFAULT_UI_SCALE
}
fn default_aside_split() -> f32 {
    0.45
}
fn default_true() -> bool {
    true
}

impl Preferences {
    fn active_keymap(&self) -> &keymap::Keymap {
        self.keymap_profiles
            .get(&self.active_keymap_profile)
            .or_else(|| self.keymap_profiles.get("Default"))
            .unwrap_or_else(|| {
                self.keymap_profiles
                    .values()
                    .next()
                    .expect("Preferences must include at least one keymap profile")
            })
    }
    fn active_keymap_mut(&mut self) -> &mut keymap::Keymap {
        let name = if self
            .keymap_profiles
            .contains_key(&self.active_keymap_profile)
        {
            self.active_keymap_profile.clone()
        } else {
            self.active_keymap_profile = "Default".into();
            "Default".into()
        };
        self.keymap_profiles
            .entry(name)
            .or_insert_with(keymap::Keymap::default_bindings)
    }
    fn normalize_keymaps(&mut self) {
        if self.keymap_profiles.is_empty() {
            self.keymap_profiles
                .insert("Default".into(), keymap::Keymap::default_bindings());
        }
        if !self.keymap_profiles.contains_key("Default") {
            self.keymap_profiles
                .insert("Default".into(), keymap::Keymap::default_bindings());
        }
        if !self
            .keymap_profiles
            .contains_key(&self.active_keymap_profile)
        {
            self.active_keymap_profile = "Default".into();
        }
        for map in self.keymap_profiles.values_mut() {
            map.sanitize();
        }
    }
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            dark: true,
            grid: true,
            accent: [91, 224, 188],
            pad_labels: PadLabels::Number,
            hidden_layers: Vec::new(),
            active_keymap_profile: default_keymap_profile(),
            keymap_profiles: default_keymap_profiles(),
            aside_split: default_aside_split(),
            aside_lower_tab: 0,
            library_dock_open: true,
            language: default_language(),
            ui_scale: a11y::DEFAULT_UI_SCALE,
            chamfer_corners: true,
            assistant_enabled: true,
            assistant_connections: Vec::new(),
            assistant_default: String::new(),
            assistant_mode: pcbcraft_assistant::Mode::Ask,
            assistant_modes: Default::default(),
            assistant_always_allow: Default::default(),
            assistant_turn_limit: default_turn_limit(),
        }
    }
}
/// Pair name, coupling gap, both routed lengths and their skew for a net that
/// belongs to a declared differential pair; nothing for any other net.
fn pair_rows(project: &Project, net: Id) -> Vec<(String, String)> {
    let Some(pair) = project
        .board
        .diff_pairs
        .iter()
        .find(|d| d.net_p == net || d.net_n == net)
    else {
        return vec![];
    };
    let length = |n: Id| -> f64 {
        project
            .board
            .tracks
            .iter()
            .filter(|t| t.net == n)
            .map(track_length)
            .sum()
    };
    let (lp, ln) = (length(pair.net_p), length(pair.net_n));
    vec![
        ("Pair".into(), format!("{} · routed together", pair.name)),
        (
            "Pair gap".into(),
            format!("{:.3} mm between the two sides", pair.gap),
        ),
        (
            "Pair lengths".into(),
            format!(
                "{} {:.2} mm · {} {:.2} mm",
                project.net_name(pair.net_p),
                lp,
                project.net_name(pair.net_n),
                ln
            ),
        ),
        (
            "Pair skew".into(),
            format!("{:.2} mm apart", (lp - ln).abs()),
        ),
    ]
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
    Via,
    Measure,
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
pub use assistant::Host;
pub struct App {
    session: SharedSession,
    assistant: assistant::State,
    project: Project,
    revision: u64,
    checks: Value,
    tab: Tab,
    tool: Tool,
    selected: Option<Id>,
    edit: editing::EditState,
    view3d: board3d::View3d,
    highlight_net: Option<Id>,
    selected_unit: u32,
    route_layer: pcbcraft_doc::Layer,
    preferences: Preferences,
    /// Text for the language chosen in Settings.
    catalog: std::sync::Arc<pcbcraft_text::Catalog>,
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
    datum_ui: datums::DatumUi,
    fiducial_diameter: f64,
    fiducial_edit: Option<(Id, f64, bool)>,
    renumber_prefix: Option<String>,
    track_width: f64,
    chamfer_size: f64,
    impedance_ui: impedance_panel::ImpedanceUi,
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
    /// Optional workspace tabs the user closed; the + menu reopens them.
    closed_tabs: Vec<Tab>,
    notes_thick: f64,
    notes_er: f64,
    net_editor: Option<pcbcraft_doc::Id>,
    pin_pick: Option<String>,
    net_draft: Option<(pcbcraft_doc::Id, String)>,
    palette: bool,
    part_search: String,
    part_sources: sources::PartSources,
    pending_source: Option<pcbcraft_library::sources::PartResult>,
    hidden_sets: Vec<String>,
    svg_path: String,
    part_search_focus: bool,
    needs_fit: bool,
    /// Left edge of the central page, so tests can check text keeps its margin.
    page_left: f32,
    schematic: Camera,
    pcb: Camera,
    plot_viewer: plot_viewer::State,
    drag: Option<(Id, Point)>,
    drag_offset: Point,
    cursor: Option<Point>,
    canvas_rect: Rect,
    canvas_menu: Option<(egui::Pos2, canvas::CanvasMenuKind)>,
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
    shortcuts_settings_open: bool,
    binding_capture: Option<keymap::KeyAction>,
    profile_name_draft: String,
    profile_import_text: String,
    keymap_notice: String,
    connected_cycle: usize,
    last_repeat: Option<(String, Value)>,
    needs_zoom_selection: bool,
    tree_search_focus: bool,
    command_focus: bool,
    command_index: usize,
    command_scroll: bool,
    capture: Option<String>,
    frames: u32,
    library: library::Browser,
    pub(crate) compare_open: bool,
    pub(crate) compare_baseline_path: String,
    compare_report_open: bool,
    pub(crate) compare_diff: Option<pcbcraft_io::DesignDiff>,
    pub(crate) compare_tint: std::collections::BTreeMap<Id, CompareKind>,
    #[cfg(test)]
    icon_rects: BTreeMap<String, Rect>,
}
impl App {
    /// One library part offered as a small card: its name to pick, and a
    /// plain line saying what it is and what it answers to when searched.
    fn part_card(&mut self, ui: &mut egui::Ui, definition: &pcbcraft_library::SymbolDefinition) {
        if ui
            .button(&definition.name)
            .on_hover_text(t!("schematic.part_card.pick_this_saved_part_the.tip"))
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
            ("", tags) => t!("schematic.part_card.search_words_tags", tags = tags),
            (what, "") => what.to_owned(),
            (what, tags) => t!(
                "schematic.part_card.what_search_words_tags",
                what = what,
                tags = tags
            ),
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
            Some(c) if c.clearance > base => (
                c.clearance,
                t!(
                    "schematic.object_detail_rows.the_usual_gap_for_name",
                    name = c.name
                ),
            ),
            Some(c) => (
                base,
                t!(
                    "schematic.object_detail_rows.the_board_s_usual_gap",
                    name = c.name
                ),
            ),
            None => (
                base,
                t!("schematic.object_detail_rows.the_board_s_usual_gap2"),
            ),
        };
        rows.push((
            t!("schematic.object_detail_rows.clearance"),
            t!(
                "schematic.object_detail_rows.value_mm_source",
                value = format!("{:.3}", value),
                source = source
            ),
        ));
        let stricter: Vec<String> = project
            .board
            .pair_rules
            .iter()
            .filter(|r| (r.a == net || r.b == net) && r.clearance > value)
            .map(|r| {
                let other = if r.a == net { r.b } else { r.a };
                t!(
                    "schematic.object_detail_rows.clearance_mm_away_from_v1",
                    clearance = format!("{:.3}", r.clearance),
                    v1 = project.net_name(other)
                )
            })
            .collect();
        if !stricter.is_empty() {
            rows.push((
                t!("schematic.object_detail_rows.special_gaps"),
                stricter.join(" · "),
            ));
        }
    };
    let mut rows: Vec<(String, String)> = Vec::new();
    if let Some(t) = project.board.tracks.iter().find(|t| t.id == id) {
        rows.push((
            t!("schematic.object_detail_rows.net"),
            t!(
                "schematic.object_detail_rows.v0_follows_the_wiring",
                v0 = net(t.net)
            ),
        ));
        rows.push((t!("schematic.object_detail_rows.layer"), layer(&t.layer)));
        let class = project.net_class(t.net);
        let width_note = match class {
            Some(c) if (t.width - c.width).abs() < 1e-6 => {
                t!(
                    "schematic.object_detail_rows.width_mm_the_usual_width",
                    width = format!("{:.3}", t.width),
                    name = c.name
                )
            }
            Some(c) => t!(
                "schematic.object_detail_rows.width_mm_chosen_for_this",
                width = format!("{:.3}", t.width),
                width2 = format!("{:.3}", c.width),
                name = c.name
            ),
            None => t!(
                "schematic.object_detail_rows.width_mm_chosen_for_this2",
                width = format!("{:.3}", t.width)
            ),
        };
        rows.push((t!("schematic.object_detail_rows.width"), width_note));
        clearance_rows(t.net, &mut rows);
        rows.extend(pair_rows(project, t.net));
        rows.push((
            t!("schematic.object_detail_rows.from"),
            t!(
                "schematic.object_detail_rows.x_y_mm",
                x = format!("{:.2}", t.start.x),
                y = format!("{:.2}", t.start.y)
            ),
        ));
        rows.push((
            t!("schematic.object_detail_rows.to"),
            t!(
                "schematic.object_detail_rows.x_y_mm",
                x = format!("{:.2}", t.end.x),
                y = format!("{:.2}", t.end.y)
            ),
        ));
        if t.arc.is_some() {
            rows.push((
                t!("schematic.object_detail_rows.shape"),
                t!("schematic.object_detail_rows.curved_through_a_midpoint"),
            ));
        }
    } else if let Some(v) = project.board.vias.iter().find(|v| v.id == id) {
        rows.push((
            t!("schematic.object_detail_rows.net"),
            t!(
                "schematic.object_detail_rows.v0_follows_the_wiring",
                v0 = net(v.net)
            ),
        ));
        rows.push((
            t!("schematic.object_detail_rows.copper"),
            t!(
                "schematic.object_detail_rows.diameter_mm",
                diameter = format!("{:.2}", v.diameter)
            ),
        ));
        rows.push((
            t!("schematic.object_detail_rows.drill"),
            t!(
                "schematic.object_detail_rows.drill_mm",
                drill = format!("{:.2}", v.drill)
            ),
        ));
        rows.extend(via_span::via_span_rows(project, v));
        rows.push((
            t!("schematic.object_detail_rows.protection"),
            via_covering::covering_name(v.effective_covering(&project.board)).into(),
        ));
        clearance_rows(v.net, &mut rows);
        rows.push((
            t!("schematic.object_detail_rows.position"),
            t!(
                "schematic.object_detail_rows.x_y_mm",
                x = format!("{:.2}", v.position.x),
                y = format!("{:.2}", v.position.y)
            ),
        ));
    } else if let Some(z) = project.board.zones.iter().find(|z| z.id == id) {
        rows.push((t!("schematic.object_detail_rows.net"), net(z.net)));
        rows.push((t!("schematic.object_detail_rows.layer"), layer(&z.layer)));
        clearance_rows(z.net, &mut rows);
        rows.push((
            t!("schematic.object_detail_rows.copper"),
            if z.filled {
                "poured".into()
            } else {
                t!("schematic.object_detail_rows.outline_only")
            },
        ));
        rows.push((
            t!("schematic.object_detail_rows.outline"),
            t!(
                "schematic.object_detail_rows.len_vertices",
                len = z.outline.len()
            ),
        ));
        if !z.cutouts.is_empty() {
            rows.push((
                t!("schematic.object_detail_rows.cutouts"),
                format!("{}", z.cutouts.len()),
            ));
        }
    } else if let Some(l) = project.labels.iter().find(|l| l.id == id) {
        rows.push((t!("schematic.object_detail_rows.kind"), l.kind.clone()));
        rows.push((t!("schematic.object_detail_rows.net"), net(l.net)));
        rows.push((
            t!("schematic.object_detail_rows.position"),
            t!(
                "schematic.object_detail_rows.x_y_mm",
                x = format!("{:.2}", l.position.x),
                y = format!("{:.2}", l.position.y)
            ),
        ));
    } else if let Some(w) = project.drawn_wires.iter().find(|w| w.id == id) {
        rows.push((
            t!("schematic.object_detail_rows.net"),
            w.net.map(net).unwrap_or_else(|| "unbound".into()),
        ));
        rows.push((
            t!("schematic.object_detail_rows.points"),
            format!("{}", w.points.len()),
        ));
    } else if let Some(t) = project.board.texts.iter().find(|t| t.id == id) {
        rows.push((t!("schematic.object_detail_rows.text"), t.text.clone()));
        rows.push((
            t!("schematic.object_detail_rows.size"),
            t!(
                "schematic.object_detail_rows.size_mm",
                size = format!("{:.2}", t.size)
            ),
        ));
        rows.push((t!("schematic.object_detail_rows.layer"), layer(&t.layer)));
        rows.push((
            t!("schematic.object_detail_rows.angle"),
            t!(
                "schematic.object_detail_rows.rotation_degrees",
                rotation = format!("{:.0}", t.rotation)
            ),
        ));
    } else if let Some(g) = project.graphics.iter().find(|g| g.id == id) {
        rows.push((t!("schematic.object_detail_rows.shape"), g.shape.clone()));
        rows.push((t!("schematic.object_detail_rows.layer"), g.layer.clone()));
        if !g.text.is_empty() {
            rows.push((t!("schematic.object_detail_rows.text"), g.text.clone()));
        }
        rows.push((
            t!("schematic.object_detail_rows.stroke"),
            t!(
                "schematic.object_detail_rows.width_mm",
                width = format!("{:.3}", g.width)
            ),
        ));
        rows.push((
            t!("schematic.object_detail_rows.points"),
            format!("{}", g.points.len()),
        ));
    } else if let Some(gr) = project.graphic_groups.iter().find(|gr| gr.id == id) {
        rows.push((
            t!("schematic.object_detail_rows.group"),
            t!(
                "schematic.object_detail_rows.len_graphics_move_as_one",
                len = gr.graphics.len()
            ),
        ));
    }
    rows
}

impl App {
    pub fn with_project_path(mut self, path: Option<String>) -> Self {
        if let Some(path) = path {
            self.path = path;
            self.saved_revision = self.session.lock().ok().map(|s| s.revision);
            self.assistant_load();
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
        mut preferences: Preferences,
        capture: Option<String>,
        tab: Option<&str>,
    ) -> Self {
        preferences.normalize_keymaps();
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
            assistant: assistant::State::default(),
            project,
            revision: u64::MAX,
            checks: json!({}),
            tab: match tab {
                Some("pcb") => Tab::Pcb,
                Some("checks") => Tab::Checks,
                Some("plots") => Tab::PlotFiles,
                Some("3d") => Tab::Board3d,
                Some("libraries" | "project-library" | "symbol-editor" | "footprint-editor") => {
                    Tab::Libraries
                }
                _ => Tab::Schematic,
            },
            tool: Tool::Select,
            selected: None,
            edit: editing::EditState::default(),
            view3d: board3d::View3d::default(),
            highlight_net: None,
            selected_unit: 1,
            route_layer: pcbcraft_doc::Layer::Front,
            catalog: std::sync::Arc::new(pcbcraft_text::Catalog::for_language(
                &preferences.language,
            )),
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
            datum_ui: datums::DatumUi::default(),
            fiducial_diameter: 1.0,
            fiducial_edit: None,
            renumber_prefix: None,
            track_width: 0.25,
            chamfer_size: 0.25,
            impedance_ui: impedance_panel::ImpedanceUi::default(),
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
            closed_tabs: Vec::new(),
            notes_thick: 1.6,
            notes_er: 4.4,
            net_editor: None,
            pin_pick: None,
            net_draft: None,
            palette: false,
            part_search: String::new(),
            part_sources: sources::PartSources::default(),
            pending_source: None,
            hidden_sets: Vec::new(),
            svg_path: String::new(),
            part_search_focus: false,
            needs_fit: true,
            page_left: 0.0,
            schematic: Camera {
                center: Point::new(40.0, 24.0),
                zoom: 11.0,
            },
            plot_viewer: plot_viewer::State::default(),
            pcb: Camera {
                center: Point::new(30.0, 20.0),
                zoom: 15.0,
            },
            plot_viewer: plot_viewer::State::default(),
            drag: None,
            drag_offset: Point::default(),
            cursor: None,
            canvas_rect: Rect::NOTHING,
            canvas_menu: None,
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
            shortcuts_settings_open: false,
            binding_capture: None,
            profile_name_draft: String::new(),
            profile_import_text: String::new(),
            keymap_notice: String::new(),
            connected_cycle: 0,
            last_repeat: None,
            needs_zoom_selection: false,
            tree_search_focus: false,
            command_focus: false,
            command_index: 0,
            command_scroll: false,
            capture,
            frames: 0,
            library,
            compare_open: false,
            compare_baseline_path: String::new(),
            compare_report_open: false,
            compare_diff: None,
            compare_tint: std::collections::BTreeMap::new(),
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
        let repeat_params = params.clone();
        let result = self
            .session
            .lock()
            .map_err(|_| t!("dialogs.run.session_unavailable"))
            .and_then(|mut s| {
                s.execute(command, params, Some(self.revision))
                    .map_err(|e| e.to_string())
            });
        match result {
            Ok(v) => {
                if command != "object.delete" {
                    self.last_repeat = Some((command.to_string(), repeat_params));
                }
                self.message = t!("dialogs.run.command_committed", command = command);
                if let Some(skipped) = v
                    .pointer("/result/unassigned_components")
                    .and_then(Value::as_array)
                    && !skipped.is_empty()
                {
                    self.message = t!(
                        "dialogs.run.len_library_symbols_need_a",
                        len = skipped.len()
                    );
                }
                if let Some(id) = v.pointer("/result/id").and_then(Value::as_u64) {
                    if self.selected != Some(id) {
                        self.selected_unit = 1;
                    }
                    self.selected = Some(id);
                    if command == "component.add" {
                        self.refresh();
                        self.attach_pending_source(id);
                        return;
                    }
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
            .map_err(|_| t!("dialogs.run_and_keep.session_unavailable"))
            .and_then(|mut s| {
                s.execute(command, params, Some(self.revision))
                    .map_err(|e| e.to_string())
            });
        match outcome {
            Ok(v) => {
                self.message = t!("dialogs.run_and_keep.command_reported", command = command);
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
            t!("dialogs.tool_call.name_complete", name = name)
        } else {
            result
                .pointer("/content/0/text")
                .and_then(Value::as_str)
                .map_or_else(|| t!("dialogs.tool_call.operation_failed"), refusal_text)
        };
        self.refresh();
        if success && name == "save" {
            self.assistant_save();
        }
        if success && (name == "save" || name == "open") {
            pcbcraft_engine::recents::record_recent_path(&self.path);
            self.recent_designs = pcbcraft_engine::recents::load_recent_paths();
            self.saved_revision = Some(self.revision);
        }
        if success && (name == "open" || name == "new_design" || name == "sample_design") {
            self.assistant_design_opened(name);
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
        self.edit.extra.clear();
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
        self.closed_tabs.retain(|t| *t != tab);
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
            egui::RichText::new(t!("board.stackup_section.board_stackup"))
                .size(10.0)
                .color(self.muted()),
        );
        ui.label(
            egui::RichText::new(t!("board.stackup_section.the_physical_board_copper_core"))
                .small()
                .color(self.muted()),
        );
        ui.label(
            egui::RichText::new(t!("board.stackup_section.start_from_a_shape"))
                .size(10.0)
                .color(self.muted()),
        );
        ui.horizontal_wrapped(|ui| {
            const PRESETS: [StackShape; 3] = [
                (
                    "board.stackup_section.two_layers_1_6_mm",
                    "board.stackup_section.the_usual_starter_board_copper",
                    &[
                        ("Top copper", "copper", 0.035),
                        ("Core", "core", 1.53),
                        ("Bottom copper", "copper", 0.035),
                    ],
                ),
                (
                    "board.stackup_section.two_layers_thin_1_0",
                    "board.stackup_section.a_slimmer_board_for_tight",
                    &[
                        ("Top copper", "copper", 0.035),
                        ("Core", "core", 0.93),
                        ("Bottom copper", "copper", 0.035),
                    ],
                ),
                (
                    "board.stackup_section.four_layers",
                    "board.stackup_section.two_extra_inner_copper_layers",
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
                    .small_button(pcbcraft_text::lookup(title, &[]))
                    .on_hover_text(t!(
                        "board.stackup_section.help_choosing_it_replaces_whatever.tip",
                        help = pcbcraft_text::lookup(help, &[])
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
                egui::RichText::new(t!("board.stackup_section.nothing_recorded_yet_this_board"))
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
                    ui.label(t!(
                        "board.stackup_section.v0_thickness_mm",
                        v0 = role_label(&record.kind),
                        thickness = format!("{:.3}", record.thickness)
                    ))
                    .on_hover_text(t!("board.stackup_section.v0_thickness_mm.tip"));
                    if !editing
                        && ui
                            .small_button(t!("board.stackup_section.tune"))
                            .on_hover_text(t!("board.stackup_section.tune.tip"))
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
                                        .on_hover_text(t!(
                                            "board.stackup_section.what_this_layer_does_in.tip"
                                        ))
                                        .clicked()
                                    {
                                        *label = (*words).to_owned();
                                        ui.close();
                                    }
                                }
                            })
                            .response
                            .on_hover_text(t!("board.stackup_section.pick_what_this_layer_is.tip"));
                        ui.add(
                            egui::DragValue::new(thickness)
                                .speed(0.01)
                                .range(0.0..=100.0)
                                .suffix(format!(" {}", t!("unit.mm"))),
                        )
                        .on_hover_text(t!("unit.mm.tip"));
                    });
                    let apply = ui
                        .button(t!("board.stackup_section.apply"))
                        .on_hover_text(t!("board.stackup_section.apply.tip"))
                        .clicked();
                    let cancel = ui
                        .small_button(t!("board.stackup_section.cancel"))
                        .on_hover_text(t!("board.stackup_section.cancel.tip"))
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
                egui::RichText::new(t!(
                    "board.stackup_section.recorded_layers_add_up_to",
                    total = format!("{:.3}", total)
                ))
                .small()
                .color(self.muted()),
            );
        }
        ui.horizontal_wrapped(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.stack_name)
                    .desired_width(70.0)
                    .hint_text(t!("board.stackup_section.layer_name")),
            )
            .on_hover_text(t!("board.stackup_section.layer_name.tip"));
            egui::ComboBox::from_id_salt("stack-add-kind")
                .selected_text(self.stack_kind.clone())
                .show_ui(ui, |ui| {
                    for (_, words) in STACK_ROLES {
                        if ui
                            .selectable_label(self.stack_kind == words, words)
                            .on_hover_text(t!("board.stackup_section.what_this_layer_does_in.tip"))
                            .clicked()
                        {
                            self.stack_kind = (*words).to_owned();
                            ui.close();
                        }
                    }
                })
                .response
                .on_hover_text(t!("board.stackup_section.pick_what_this_layer_is.tip"));
            ui.add(
                egui::DragValue::new(&mut self.stack_thickness)
                    .speed(0.01)
                    .range(0.0..=100.0)
                    .suffix(format!(" {}", t!("unit.mm"))),
            )
            .on_hover_text(t!("unit.mm.tip"));
            if ui
                .add_enabled(
                    !self.stack_name.trim().is_empty(),
                    egui::Button::new(t!("board.stackup_section.add_layer")),
                )
                .on_hover_text(t!("board.stackup_section.add_layer.tip"))
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
                .button(t!("board.stackup_section.back_to_a_plain_two"))
                .on_hover_text(t!("board.stackup_section.back_to_a_plain_two.tip"))
                .clicked()
        {
            self.stack_edit = None;
            self.run("board.clear_layer_records", json!({}));
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label(t!("board.stackup_section.finish"));
            egui::ComboBox::from_id_salt("board-finish")
                .selected_text(self.project.board.finish.clone())
                .show_ui(ui, |ui| {
                    for finish in ["HASL", "Lead-free HASL", "ENIG", "OSP", "Immersion silver"] {
                        if ui
                            .selectable_label(self.project.board.finish == finish, finish)
                            .on_hover_text(t!(
                                "board.stackup_section.a_common_coating_for_exposed.tip"
                            ))
                            .clicked()
                        {
                            self.run("board.set_finish", json!({"finish": finish}));
                            ui.close();
                        }
                    }
                })
                .response
                .on_hover_text(t!(
                    "board.stackup_section.the_coating_on_exposed_copper.tip"
                ));
        });
        self.via_covering_default_controls(ui);
        ui.horizontal(|ui| {
            ui.label(t!("board.stackup_section.thickness"));
            ui.add(
                egui::DragValue::new(&mut self.notes_thick)
                    .speed(0.01)
                    .range(0.2..=100.0)
                    .suffix(format!(" {}", t!("unit.mm"))),
            )
            .on_hover_text(t!("unit.mm2.tip"));
            if ui
                .button(t!("board.stackup_section.set_thickness"))
                .on_hover_text(t!("board.stackup_section.set_thickness.tip"))
                .clicked()
            {
                let thickness = self.notes_thick;
                self.run("board.set_stackup", json!({"thickness": thickness}));
            }
        });
        ui.horizontal(|ui| {
            ui.label(t!("board.stackup_section.substrate"));
            ui.add(
                egui::DragValue::new(&mut self.notes_er)
                    .speed(0.05)
                    .range(1.0..=20.0),
            )
            .on_hover_text(t!(
                "board.stackup_section.how_the_material_between_layers.tip"
            ));
            if ui
                .button(t!("board.stackup_section.set_substrate"))
                .on_hover_text(t!("board.stackup_section.set_substrate.tip"))
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
            .checkbox(&mut plated, t!("board.stackup_section.plated_board_edge"))
            .on_hover_text(t!("board.stackup_section.plated_board_edge.tip"))
            .clicked()
        {
            self.run("board.set_machining", json!({"edge_plating": plated}));
        }
    }
    fn holes_section(&mut self, ui: &mut egui::Ui) {
        ui.label(
            egui::RichText::new(t!("board.holes_section.mounting_holes"))
                .size(10.0)
                .color(self.muted()),
        );
        if self.project.board.holes.is_empty() {
            ui.label(
                egui::RichText::new(t!("board.holes_section.no_drilled_holes_yet_press"))
                    .small()
                    .color(self.muted()),
            );
            return;
        }
        for hole in self.project.board.holes.clone() {
            let edit = self.hole_edit.filter(|(id, _)| *id == hole.id);
            ui.horizontal(|ui| {
                ui.label(format!("{} ", t!("board.holes_section.text")));
                if let Some((_, mut diameter)) = edit {
                    ui.add(
                        egui::DragValue::new(&mut diameter)
                            .speed(0.05)
                            .range(0.05..=25.0)
                            .suffix(format!(" {}", t!("unit.mm"))),
                    )
                    .on_hover_text(t!("unit.mm3.tip"));
                    if ui
                        .button(t!("board.holes_section.apply"))
                        .on_hover_text(t!("board.holes_section.apply.tip"))
                        .clicked()
                    {
                        self.hole_edit = None;
                        self.run(
                            "board.hole_set_size",
                            json!({"id": hole.id, "diameter": diameter}),
                        );
                    }
                    if ui
                        .button(t!("board.holes_section.cancel"))
                        .on_hover_text(t!("board.holes_section.cancel.tip"))
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
                        ui.label(t!(
                            "board.holes_section.diameter_mm_slot_length_mm",
                            diameter = format!("{:.2}", hole.diameter),
                            length = format!("{:.2}", hole.length()),
                            x = format!("{:.1}", hole.position.x),
                            y = format!("{:.1}", hole.position.y)
                        ))
                        .on_hover_text(t!("board.holes_section.diameter_mm_slot_length_mm.tip"));
                    } else {
                        ui.label(t!(
                            "board.holes_section.diameter_mm_at_x_y",
                            diameter = format!("{:.2}", hole.diameter),
                            x = format!("{:.1}", hole.position.x),
                            y = format!("{:.1}", hole.position.y)
                        ));
                    }
                    if ui
                        .small_button(t!("board.holes_section.tune"))
                        .on_hover_text(t!("board.holes_section.tune.tip"))
                        .clicked()
                    {
                        self.hole_edit = Some((hole.id, hole.diameter));
                    }
                }
                if ui
                    .small_button(t!("board.holes_section.text2"))
                    .on_hover_text(t!("board.holes_section.text2.tip"))
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
            egui::RichText::new(t!("board.fiducials_section.assembly_marks"))
                .size(10.0)
                .color(self.muted()),
        );
        if self.project.board.fiducials.is_empty() {
            ui.label(
                egui::RichText::new(t!("board.fiducials_section.no_assembly_marks_yet_press"))
                    .small()
                    .color(self.muted()),
            );
            return;
        }
        for fid in self.project.board.fiducials.clone() {
            let edit = self.fiducial_edit.filter(|(id, _, _)| *id == fid.id);
            ui.horizontal(|ui| {
                ui.label(format!("{} ", t!("board.fiducials_section.text")));
                if let Some((_, mut diameter, mut front)) = edit {
                    ui.add(
                        egui::DragValue::new(&mut diameter)
                            .speed(0.05)
                            .range(0.2..=10.0)
                            .suffix(format!(" {}", t!("unit.mm"))),
                    )
                    .on_hover_text(t!("unit.mm4.tip"));
                    if ui
                        .selectable_label(front, t!("board.fiducials_section.top"))
                        .on_hover_text(t!("board.fiducials_section.top.tip"))
                        .clicked()
                    {
                        front = true;
                    }
                    if ui
                        .selectable_label(!front, t!("board.fiducials_section.bottom"))
                        .on_hover_text(t!("board.fiducials_section.bottom.tip"))
                        .clicked()
                    {
                        front = false;
                    }
                    if ui
                        .button(t!("board.fiducials_section.apply"))
                        .on_hover_text(t!("board.fiducials_section.apply.tip"))
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
                        .button(t!("board.fiducials_section.cancel"))
                        .on_hover_text(t!("board.fiducials_section.cancel.tip"))
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
                    ui.label(t!(
                        "board.fiducials_section.diameter_mm_on_v1_at",
                        diameter = format!("{:.2}", fid.diameter),
                        v1 = if fid.layer == pcbcraft_doc::Layer::Front {
                            "Top"
                        } else {
                            "Bottom"
                        },
                        x = format!("{:.1}", fid.position.x),
                        y = format!("{:.1}", fid.position.y)
                    ));
                    if ui
                        .small_button(t!("board.fiducials_section.tune"))
                        .on_hover_text(t!("board.fiducials_section.tune.tip"))
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
                    .small_button(t!("board.fiducials_section.text2"))
                    .on_hover_text(t!("board.fiducials_section.text2.tip"))
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
            let search = ui.add(
                egui::TextEdit::singleline(&mut self.tree_query)
                    .hint_text(t!("board.layers_tab.search_the_whole_board"))
                    .desired_width(150.0),
            )
            .on_hover_text(t!("board.layers_tab.search_the_whole_board.tip"));
            if self.tree_search_focus {
                search.request_focus();
                self.tree_search_focus = false;
            }
            if !self.tree_query.is_empty()
                && ui
                    .small_button(t!("board.layers_tab.clear"))
                    .on_hover_text(t!("board.layers_tab.clear.tip"))
                    .clicked()
            {
                self.tree_query.clear();
            }
        });
        self.board_anatomy_section(ui);
        ui.separator();
        ui.label(
            egui::RichText::new(t!("board.layers_tab.show_in_the_tree"))
                .size(10.0)
                .color(self.muted()),
        );
        ui.horizontal_wrapped(|ui| {
            for (kind, label, tip) in [
                (
                    "wires",
                    t!("board.layers_tab.show_wires"),
                    t!("board.layers_tab.copper_wires_on_each_face"),
                ),
                (
                    "vias",
                    t!("board.layers_tab.show_vias"),
                    t!("board.layers_tab.plated_holes_that_carry_a"),
                ),
                (
                    "zones",
                    t!("board.layers_tab.show_pours"),
                    t!("board.layers_tab.solid_copper_poured_over_an"),
                ),
                (
                    "holes",
                    t!("board.layers_tab.show_holes"),
                    t!("board.layers_tab.drilled_holes_and_cut_outs"),
                ),
                (
                    "words",
                    t!("board.layers_tab.show_words"),
                    t!("board.layers_tab.silk_text_printed_on_the"),
                ),
                (
                    "parts",
                    t!("board.layers_tab.show_parts"),
                    t!("board.layers_tab.every_part_on_the_board"),
                ),
                (
                    "areas",
                    t!("board.layers_tab.show_rule_areas"),
                    t!("board.layers_tab.stricter_areas_and_watched_pairs"),
                ),
                (
                    "art",
                    t!("board.layers_tab.show_machine_layers"),
                    t!("board.layers_tab.your_own_layers_for_a"),
                ),
                (
                    "connections",
                    t!("board.layers_tab.show_connections"),
                    t!("board.layers_tab.every_net_by_name_add"),
                ),
                (
                    "notes",
                    t!("board.layers_tab.show_notes"),
                    t!("board.layers_tab.free_form_notes_stored_with"),
                ),
            ] {
                let shown = !self.kind_hidden(kind);
                if ui
                    .selectable_label(shown, egui::RichText::new(label).small())
                    .on_hover_text(t!(
                        "board.layers_tab.tip_untick_to_fold_this.tip",
                        tip = tip
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
        let board_header = egui::CollapsingHeader::new(t!("board.layers_tab.this_board"))
            .id_salt("anat-board-root")
            .default_open(true)
            .show(ui, |ui| {
                self.holes_section(ui);
                self.datums_section(ui);
                self.fiducials_section(ui);
                if self.tab == Tab::Pcb {
                    self.via_spans_section(ui);
                }
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
                        egui::RichText::new(t!("board.layers_tab.switch_to_the_schematic_or"))
                            .small()
                            .color(self.muted()),
                    );
                } else {
                    ui.label(
                        egui::RichText::new(t!("board.layers_tab.what_the_drawing_shows"))
                            .size(10.0)
                            .color(self.muted()),
                    );
                    for name in rows {
                        let mut shown = self.layer_shown(name);
                        if ui
                            .checkbox(&mut shown, layer_row_label(name))
                            .on_hover_text(t!(
                                "board.layers_tab.show_or_hide_name_on.tip",
                                name = name
                            ))
                            .clicked()
                        {
                            if shown {
                                self.preferences
                                    .hidden_layers
                                    .retain(|hidden| hidden != name);
                            } else {
                                self.preferences.hidden_layers.push((*name).to_string());
                            }
                        }
                    }
                }
            });
        board_header
            .header_response
            .on_hover_text(t!("board.layers_tab.the_board_itself_shape_size.tip"));
    }
    fn board_anatomy_section(&mut self, ui: &mut egui::Ui) {
        ui.add_space(16.0);
        ui.label(
            egui::RichText::new(t!("board.board_anatomy_section.board_anatomy"))
                .size(10.0)
                .color(self.muted()),
        );
        ui.label(
            egui::RichText::new(t!("board.board_anatomy_section.every_piece_on_the_board"))
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
                    t!(
                        "board.board_anatomy_section.wire_width_mm_v1_at",
                        width = format!("{:.2}", t.width),
                        v1 = self.project.net_name(t.net),
                        round = t.start.x.round(),
                        round2 = t.start.y.round()
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
                    t!(
                        "board.board_anatomy_section.wire_width_mm_v1_at",
                        width = format!("{:.2}", t.width),
                        v1 = self.project.net_name(t.net),
                        round = t.start.x.round(),
                        round2 = t.start.y.round()
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
                    t!(
                        "board.board_anatomy_section.via_at_round_round2_drill",
                        round = v.position.x.round(),
                        round2 = v.position.y.round(),
                        drill = v.drill,
                        v3 = via_span::via_tree_span(&self.project, v),
                        v4 = self.project.net_name(v.net)
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
                    t!(
                        "board.board_anatomy_section.copper_zone_v0_v1",
                        v0 = self.project.net_name(z.net),
                        v1 = if z.layer == pcbcraft_doc::Layer::Front {
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
                    t!(
                        "board.board_anatomy_section.hole_at_round_round2_v2",
                        round = c.center.x.round(),
                        round2 = c.center.y.round(),
                        v2 = c.radius * 2.0
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
                    .map_or_else(
                        || t!("board.board_anatomy_section.a_part"),
                        |c| c.reference.clone(),
                    )
                    .to_owned();
                let pads = f
                    .pads
                    .iter()
                    .map(|pad| {
                        t!(
                            "board.board_anatomy_section.pad_number_diameter_mm_v2",
                            number = pad.number,
                            diameter = pad.diameter,
                            v2 = pad.net.map_or("no net".to_owned(), |n| self
                                .project
                                .net_name(n)
                                .to_owned())
                        )
                    })
                    .collect();
                (
                    f.id,
                    t!(
                        "board.board_anatomy_section.name_len_pads",
                        name = name,
                        len = f.pads.len()
                    ),
                    pads,
                )
            })
            .filter(|(_, title, _)| self.tree_matches(title))
            .collect();
        let areas: Vec<String> = self
            .project
            .board
            .rule_areas
            .iter()
            .map(|a| {
                t!(
                    "board.board_anatomy_section.stricter_area_at_round_round2",
                    round = a.region.x.round(),
                    round2 = a.region.y.round(),
                    clearance = a.clearance,
                    v3 = a
                        .max_height
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
                t!(
                    "board.board_anatomy_section.name_v1_v2_gap_gap",
                    name = d.name,
                    v1 = self.project.net_name(d.net_p),
                    v2 = self.project.net_name(d.net_n),
                    gap = d.gap
                )
            })
            .collect();
        let picked = self.selected;
        let mut chosen: Option<Id> = None;
        let groups: [AnatGroup; 8] = [
            (
                "front",
                t!("board.board_anatomy_section.top_copper_wires"),
                t!("board.board_anatomy_section.copper_wires_drawn_on_the"),
                front
                    .into_iter()
                    .map(|(id, text)| (Some(id), text))
                    .filter(|(_, text)| self.tree_matches(text))
                    .collect(),
                t!("board.board_anatomy_section.nothing_drawn_here_yet"),
            ),
            (
                "back",
                t!("board.board_anatomy_section.bottom_copper_wires"),
                t!("board.board_anatomy_section.copper_wires_drawn_on_the2"),
                back.into_iter()
                    .map(|(id, text)| (Some(id), text))
                    .filter(|(_, text)| self.tree_matches(text))
                    .collect(),
                t!("board.board_anatomy_section.nothing_drawn_here_yet"),
            ),
            (
                "vias",
                t!("board.board_anatomy_section.via_holes"),
                t!("board.board_anatomy_section.small_plated_holes_that_carry"),
                vias.into_iter()
                    .map(|(id, text)| (Some(id), text))
                    .filter(|(_, text)| self.tree_matches(text))
                    .collect(),
                t!("board.board_anatomy_section.no_vias_yet"),
            ),
            (
                "zones",
                t!("board.board_anatomy_section.poured_copper"),
                t!("board.board_anatomy_section.solid_copper_poured_over_an"),
                zones
                    .into_iter()
                    .map(|(id, text)| (Some(id), text))
                    .filter(|(_, text)| self.tree_matches(text))
                    .collect(),
                t!("board.board_anatomy_section.no_copper_zones_yet"),
            ),
            (
                "holes",
                t!("board.board_anatomy_section.holes_and_cut_outs"),
                t!("board.board_anatomy_section.holes_drilled_through_the_board"),
                holes
                    .into_iter()
                    .map(|(id, text)| (Some(id), text))
                    .filter(|(_, text)| self.tree_matches(text))
                    .collect(),
                t!("board.board_anatomy_section.no_holes_yet"),
            ),
            (
                "words",
                t!("board.board_anatomy_section.words_on_the_board"),
                t!("board.board_anatomy_section.silk_text_printed_on_the"),
                words
                    .into_iter()
                    .map(|(id, text)| (Some(id), text))
                    .filter(|(_, text)| self.tree_matches(text))
                    .collect(),
                t!("board.board_anatomy_section.no_words_printed_yet"),
            ),
            (
                "areas",
                t!("board.board_anatomy_section.stricter_areas"),
                t!("board.board_anatomy_section.patches_of_the_board_where"),
                areas
                    .into_iter()
                    .map(|text| (None, text))
                    .filter(|(_, text)| self.tree_matches(text))
                    .collect(),
                t!("board.board_anatomy_section.the_whole_board_follows_one"),
            ),
            (
                "pairs",
                t!("board.board_anatomy_section.watched_pairs"),
                t!("board.board_anatomy_section.pairs_of_nets_that_must"),
                pairs
                    .into_iter()
                    .map(|text| (None, text))
                    .filter(|(_, text)| self.tree_matches(text))
                    .collect(),
                t!("board.board_anatomy_section.no_pairs_are_being_watched"),
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
                                t!("board.board_anatomy_section.stop_fading")
                            } else {
                                t!("board.board_anatomy_section.fade_others")
                            })
                            .on_hover_text(t!(
                                "board.board_anatomy_section.everything_not_on_this_layer.tip"
                            ))
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
                                .on_hover_text(t!(
                                    "board.board_anatomy_section.select_this_on_the_drawing.tip"
                                ))
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
        let header = egui::CollapsingHeader::new(t!("board.board_anatomy_section.parts_and_pads"))
            .id_salt("anat-parts")
            .open(Some(open))
            .show(ui, |ui| {
                if parts.is_empty() {
                    ui.label(
                        egui::RichText::new(t!(
                            "board.board_anatomy_section.no_parts_on_the_board"
                        ))
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
                                .button(t!("board.board_anatomy_section.pick_up_this_part"))
                                .on_hover_text(t!(
                                    "board.board_anatomy_section.pick_up_this_part.tip"
                                ))
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
        header.header_response.on_hover_text(t!(
            "board.board_anatomy_section.every_part_on_the_board.tip"
        ));
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
            let header = egui::CollapsingHeader::new(t!(
                "board.board_anatomy_section.machine_layers_you_draw"
            ))
            .id_salt("anat-art")
            .open(Some(open))
            .show(ui, |ui| {
                if layers.is_empty() {
                    ui.label(
                        egui::RichText::new(t!(
                            "board.board_anatomy_section.no_machine_layers_yet_add"
                        ))
                        .small()
                        .color(self.muted()),
                    );
                }
                for (id, name, kind, visible, shapes) in layers {
                    let mut shown = visible;
                    ui.horizontal(|ui| {
                        if ui
                            .checkbox(&mut shown, egui::RichText::new(&name).strong())
                            .on_hover_text(t!(
                                "board.board_anatomy_section.tick_to_show_this_layer.tip"
                            ))
                            .clicked()
                        {
                            self.run("board.layer_visible_set", json!({"layer": id, "on": shown}));
                        }
                        let focus_key = format!("art:{name}");
                        let focused = self.focus_layer.as_deref() == Some(focus_key.as_str());
                        if ui
                            .small_button(if focused {
                                t!("board.board_anatomy_section.stop_fading")
                            } else {
                                t!("board.board_anatomy_section.fade_others")
                            })
                            .on_hover_text(t!(
                                "board.board_anatomy_section.everything_not_on_this_layer.tip"
                            ))
                            .clicked()
                        {
                            self.focus_layer = if focused { None } else { Some(focus_key) };
                        }
                        ui.label(
                            egui::RichText::new(t!(
                                "board.board_anatomy_section.kind_layer_len_shapes",
                                kind = kind,
                                len = shapes.len()
                            ))
                            .small()
                            .color(self.muted()),
                        );
                    });
                    egui::CollapsingHeader::new(t!(
                        "board.board_anatomy_section.name_shapes",
                        name = name
                    ))
                    .id_salt(format!("anat-art-{id}"))
                    .default_open(false)
                    .show(ui, |ui| {
                        if shapes.is_empty() {
                            ui.label(
                                egui::RichText::new(t!(
                                    "board.board_anatomy_section.nothing_drawn_on_this_layer"
                                ))
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
                    .button(t!("board.board_anatomy_section.new_layer"))
                    .on_hover_text(t!("board.board_anatomy_section.new_layer.tip"))
                    .clicked()
                {
                    let name = t!(
                        "board.board_anatomy_section.art_v0",
                        v0 = self.project.board.user_layers.len() + 1
                    );
                    self.run("board.layer_add", json!({"name": name, "kind": "other"}));
                    if self.message.starts_with("board.layer_add") {
                        self.art_layer = self.selected;
                    }
                }
            });
            let clicked = header.header_response.clicked();
            header.header_response.on_hover_text(t!(
                "board.board_anatomy_section.layers_you_draw_yourself_cut.tip"
            ));
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
            let header = egui::CollapsingHeader::new(t!("board.board_anatomy_section.connections"))
                .id_salt("anat-connections")
                .default_open(true)
                .open(Some(open))
                .show(ui, |ui| {
                    self.nets_tab(ui);
                });
            let clicked = header.header_response.clicked();
            header.header_response.on_hover_text(t!(
                "board.board_anatomy_section.every_wire_connection_by_name.tip"
            ));
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
            let header =
                egui::CollapsingHeader::new(t!("board.board_anatomy_section.project_notes"))
                    .id_salt("anat-notes")
                    .open(Some(open))
                    .show(ui, |ui| {
                        if notes.is_empty() {
                            ui.label(
                                egui::RichText::new(t!(
                                    "board.board_anatomy_section.no_notes_are_stored_with"
                                ))
                                .small()
                                .color(self.muted()),
                            );
                        }
                        for text in notes {
                            ui.label(egui::RichText::new(text).small());
                        }
                    });
            let clicked = header.header_response.clicked();
            header.header_response.on_hover_text(t!(
                "board.board_anatomy_section.free_form_notes_kept_inside.tip"
            ));
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
    fn net_for_selection(&self, id: Id) -> Option<Id> {
        if let Some(track) = self.project.board.tracks.iter().find(|t| t.id == id) {
            return Some(track.net);
        }
        if let Some(via) = self.project.board.vias.iter().find(|v| v.id == id) {
            return Some(via.net);
        }
        if let Some(fp) = self.project.footprint(id) {
            return fp.pads.iter().find_map(|p| p.net);
        }
        None
    }

    fn dispatch_shortcut(&mut self, action: keymap::KeyAction) {
        use keymap::KeyAction;
        match action {
            KeyAction::Undo => {
                self.tool_call("undo", json!({}));
            }
            KeyAction::Redo => {
                self.tool_call("redo", json!({}));
            }
            KeyAction::Save => {
                self.tool_call("save", json!({"path": self.path}));
            }
            KeyAction::CommandPalette => {
                self.command_open = !self.command_open;
                self.command_focus = self.command_open;
                self.command_index = 0;
            }
            KeyAction::ToggleShortcutsHelp => {
                self.shortcuts_open = !self.shortcuts_open;
            }
            KeyAction::SymbolPalette => {
                self.palette = true;
            }
            KeyAction::WireTool => {
                self.tool = Tool::Wire(None);
            }
            KeyAction::RouteTool => {
                self.tool = Tool::Route(None);
            }
            KeyAction::MoveTool => {
                self.select_hover();
                self.tool = Tool::Move;
            }
            KeyAction::Rotate => {
                self.select_hover();
                self.rotate();
            }
            KeyAction::OpenInspector => {
                self.select_hover();
                self.load_properties();
            }
            KeyAction::ZoomFitDesign => {
                self.needs_fit = true;
            }
            KeyAction::ZoomToSelection => {
                self.needs_zoom_selection = true;
            }
            KeyAction::DeleteSelection => {
                if let Some(id) = self.selected {
                    self.run("object.delete", json!({"id": id}));
                    self.select(None);
                }
            }
            KeyAction::ReturnToSelect => {
                self.tool = Tool::Select;
                self.drag = None;
                self.palette = false;
                self.shortcuts_open = false;
                self.binding_capture = None;
                self.canvas_menu = None;
                self.edit.measure = None;
                self.edit.box_from = None;
                self.edit.box_to = None;
            }
            KeyAction::ViaTool => {
                self.tool = Tool::Via;
            }
            KeyAction::FlipHorizontal | KeyAction::MirrorVertical => {
                self.select_hover();
                let Some(id) = self.selected else {
                    return;
                };
                let axis = if matches!(action, KeyAction::FlipHorizontal) {
                    "horizontal"
                } else {
                    "vertical"
                };
                if self.project.footprint(id).is_some() {
                    self.run("footprint.mirror", json!({"id": id, "axis": axis}));
                } else if self.project.component(id).is_some() {
                    self.run("component.mirror", json!({"id": id, "axis": axis}));
                }
            }
            KeyAction::SelectConnected => {
                let Some(sel) = self.selected else {
                    return;
                };
                let Some(net) = self.net_for_selection(sel) else {
                    return;
                };
                let mut ids: Vec<Id> = self
                    .project
                    .board
                    .tracks
                    .iter()
                    .filter(|t| t.net == net)
                    .map(|t| t.id)
                    .collect();
                for via in &self.project.board.vias {
                    if via.net == net {
                        ids.push(via.id);
                    }
                }
                ids.sort_unstable();
                ids.dedup();
                if ids.is_empty() {
                    return;
                }
                if ids.len() == 1 {
                    self.highlight_net = Some(net);
                    self.select(Some(ids[0]));
                    return;
                }
                let at = ids.iter().position(|id| *id == sel).unwrap_or(0);
                let next = ids[(at + 1) % ids.len()];
                self.connected_cycle = at + 1;
                self.select(Some(next));
            }
            KeyAction::ToggleNetHighlight => {
                let Some(sel) = self.selected else {
                    return;
                };
                let Some(net) = self.net_for_selection(sel) else {
                    return;
                };
                self.highlight_net = if self.highlight_net == Some(net) {
                    None
                } else {
                    Some(net)
                };
            }
            KeyAction::LayerNext => {
                self.route_layer = match self.route_layer {
                    pcbcraft_doc::Layer::Front => pcbcraft_doc::Layer::Back,
                    pcbcraft_doc::Layer::Back => pcbcraft_doc::Layer::Front,
                    other => other,
                };
            }
            KeyAction::LayerPrev => {
                self.route_layer = match self.route_layer {
                    pcbcraft_doc::Layer::Front => pcbcraft_doc::Layer::Back,
                    pcbcraft_doc::Layer::Back => pcbcraft_doc::Layer::Front,
                    other => other,
                };
            }
            KeyAction::RepeatLastCommand => {
                if let Some((command, params)) = self.last_repeat.clone() {
                    self.run(&command, params);
                }
            }
            KeyAction::FindInTree => {
                self.left_tab = 1;
                self.tree_search_focus = true;
            }
            KeyAction::PaletteLineUp | KeyAction::PaletteLineDown | KeyAction::PaletteRun => {}
            KeyAction::ToggleAutoSync => {
                let next = !self.project.auto_sync;
                self.run("project.set_auto_sync", json!({"auto_sync": next}));
            }
            KeyAction::UpdateBoardFromSchematic => {
                if !self.project.auto_sync {
                    self.run("board.sync", json!({}));
                }
            }
        }
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.current_pass_index() > 0 {
            return;
        }
        if let Some(capture) = self.binding_capture {
            ctx.input(|i| {
                if i.key_pressed(egui::Key::Escape) {
                    self.binding_capture = None;
                    self.keymap_notice = "Rebind cancelled.".into();
                    return;
                }
                for event in &i.events {
                    let egui::Event::Key {
                        key,
                        pressed: true,
                        repeat: false,
                        ..
                    } = event
                    else {
                        continue;
                    };
                    if *key == egui::Key::Escape {
                        continue;
                    }
                    let chord = keymap::chord_from_input(*key, i.modifiers);
                    let mut map = self.preferences.active_keymap_mut().clone();
                    match map.set_binding(capture, chord) {
                        Ok(()) => {
                            *self.preferences.active_keymap_mut() = map;
                            self.keymap_notice = format!(
                                "{} is now {}",
                                keymap::action_label(capture),
                                keymap::format_chord(chord)
                            );
                            self.binding_capture = None;
                        }
                        Err(err) => self.keymap_notice = err,
                    }
                    return;
                }
            });
            return;
        }
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        let map = self.preferences.active_keymap().clone();
        let tab = self.tab;
        let command_open = self.command_open;
        ctx.input(|i| {
            for event in &i.events {
                let egui::Event::Key {
                    key,
                    pressed: true,
                    repeat: false,
                    ..
                } = event
                else {
                    continue;
                };
                let chord = keymap::chord_from_input(*key, i.modifiers);
                if let Some(action) = map.resolve(chord, tab, command_open) {
                    if !matches!(
                        action,
                        keymap::KeyAction::PaletteLineUp
                            | keymap::KeyAction::PaletteLineDown
                            | keymap::KeyAction::PaletteRun
                    ) {
                        self.dispatch_shortcut(action);
                    }
                }
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
    /// The slim top bar: the brand and the menus, nothing else. Save, Undo
    /// and Redo, settings, bug reports and the version live in the menus.
    fn header(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("app-bar")
            .exact_size(Self::APP_BAR_HEIGHT)
            .frame(Self::dock_frame(ui.style()))
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    let brand = ui.add(
                        egui::Label::new(
                            egui::RichText::new(t!("app.name"))
                                .size(15.0)
                                .strong()
                                .color(self.accent()),
                        )
                        .sense(egui::Sense::click()),
                    );
                    a11y::name_control(&brand, "PCBCraft start screen", None);
                    if brand.on_hover_text(t!("app.brand.tip")).clicked() {
                        self.home_open = true;
                    }
                    if self.library.editor.mode == 0 {
                        ui.separator();
                        self.file_menu(ui);
                        self.edit_menu(ui);
                        self.help_menu(ui);
                    }
                });
            });
    }
    fn workbench_tabs(&mut self, ui: &mut egui::Ui) {
        let optional = [Tab::Rules, Tab::Checks, Tab::Settings];
        let mut shown: Vec<(Tab, String)> = vec![
            (Tab::Files, t!("docks.workbench_tabs.files")),
            (Tab::Schematic, t!("docks.workbench_tabs.schematic")),
            (Tab::Pcb, t!("docks.workbench_tabs.layout")),
            (Tab::Board3d, t!("docks.workbench_tabs.3d_view")),
        ];
        if self.tab == Tab::Libraries {
            shown.push((Tab::Libraries, t!("docks.workbench_tabs.libraries")));
        }
        for (tab, label) in [
            (Tab::Rules, t!("docks.workbench_tabs.rules")),
            (Tab::Checks, t!("docks.workbench_tabs.checks")),
            (Tab::PlotFiles, t!("docks.workbench_tabs.plot_files")),
            (Tab::Settings, t!("docks.workbench_tabs.settings")),
        ] {
            if !self.closed_tabs.contains(&tab) {
                shown.push((tab, label));
            }
        }
        let used = shown.len() as f32 * 90.0 + 40.0;
        let mut close = None;
        ui.horizontal(|ui| {
            ui.add_space(((ui.available_width() - used) / 2.0).max(8.0));
            for (tab, label) in shown.iter().cloned() {
                let selected = self.tab == tab;
                let response = ui.selectable_label(selected, label.clone()).on_hover_text(
                    if optional.contains(&tab) {
                        t!(
                            "docks.workbench_tabs.switch_to_the_label_view",
                            label = label
                        )
                    } else {
                        t!(
                            "docks.workbench_tabs.switch_to_the_label_view2",
                            label = label
                        )
                    },
                );
                if response.clicked() {
                    self.change_tab(tab);
                }
                if optional.contains(&tab) {
                    response.context_menu(|ui| {
                        if ui
                            .button(t!("docks.workbench_tabs.close"))
                            .on_hover_text(t!("docks.workbench_tabs.close.tip"))
                            .clicked()
                        {
                            close = Some(tab);
                            ui.close();
                        }
                    });
                }
            }
            let closed = self.closed_tabs.clone();
            ui.menu_button(t!("docks.workbench_tabs.text"), |ui| {
                if closed.is_empty() {
                    ui.add_enabled(
                        false,
                        egui::Label::new(t!("docks.workbench_tabs.every_view_is_open")),
                    );
                }
                for tab in closed {
                    let name = match tab {
                        Tab::Rules => t!("docks.workbench_tabs.rules"),
                        Tab::Checks => t!("docks.workbench_tabs.checks"),
                        _ => t!("docks.workbench_tabs.settings"),
                    };
                    if ui
                        .button(name)
                        .on_hover_text(t!("docks.workbench_tabs.open_this_view_again.tip"))
                        .clicked()
                    {
                        self.change_tab(tab);
                        ui.close();
                    }
                }
            })
            .response
            .on_hover_text(t!("docks.workbench_tabs.open_a_view_you_closed.tip"));
        });
        if let Some(tab) = close {
            self.closed_tabs.push(tab);
            if self.tab == tab {
                self.change_tab(Tab::Schematic);
            }
        }
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
                                egui::RichText::new(t!("dialogs.start_home.welcome_to_pcbcraft"))
                                    .size(30.0)
                                    .strong()
                                    .color(self.accent()),
                            );
                            ui.label(
                                egui::RichText::new(t!(
                                    "dialogs.start_home.draw_a_circuit_place_the"
                                ))
                                .size(14.0)
                                .color(self.muted()),
                            );
                            ui.add_space(24.0);
                            if ui
                                .add_sized(
                                    [300.0, 64.0],
                                    egui::Button::new((
                                        egui::RichText::new(t!("dialogs.start_home.new_design"))
                                            .size(16.0)
                                            .strong(),
                                        egui::RichText::new(t!(
                                            "dialogs.start_home.begin_with_a_blank_page"
                                        ))
                                        .small()
                                        .color(self.muted()),
                                    )),
                                )
                                .on_hover_text(t!("dialogs.start_home.begin_with_a_blank_page.tip"))
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
                                    egui::Button::new((
                                        egui::RichText::new(t!(
                                            "dialogs.start_home.worked_example"
                                        ))
                                        .size(16.0)
                                        .strong(),
                                        egui::RichText::new(t!(
                                            "dialogs.start_home.a_small_battery_resistor_and"
                                        ))
                                        .small()
                                        .color(self.muted()),
                                    )),
                                )
                                .on_hover_text(t!(
                                    "dialogs.start_home.a_small_battery_resistor_and.tip"
                                ))
                                .clicked()
                            {
                                self.tool_call("sample_design", json!({}));
                                self.home_open = false;
                            }
                            ui.add_space(6.0);
                            if ui
                                .add_sized(
                                    [300.0, 64.0],
                                    egui::Button::new((
                                        egui::RichText::new(t!(
                                            "dialogs.start_home.open_a_design_file"
                                        ))
                                        .size(16.0)
                                        .strong(),
                                        egui::RichText::new(t!(
                                            "dialogs.start_home.type_a_path_or_pick"
                                        ))
                                        .small()
                                        .color(self.muted()),
                                    )),
                                )
                                .on_hover_text(t!("dialogs.start_home.type_a_path_or_pick.tip"))
                                .clicked()
                            {
                                self.home_path_focus = true;
                            }
                            ui.add_space(6.0);
                            ui.horizontal(|ui| {
                                let path_box = ui
                                    .add(
                                        egui::TextEdit::singleline(&mut self.path)
                                            .hint_text(t!("dialogs.start_home.design_file_path"))
                                            .desired_width(240.0),
                                    )
                                    .on_hover_text(t!("dialogs.start_home.design_file_path.tip"));
                                if self.home_path_focus {
                                    path_box.request_focus();
                                    self.home_path_focus = false;
                                }
                                if ui
                                    .button(t!("dialogs.start_home.open"))
                                    .on_hover_text(t!("dialogs.start_home.open.tip"))
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
                                    egui::RichText::new(t!(
                                        "dialogs.start_home.open_one_you_worked_on"
                                    ))
                                    .size(14.0)
                                    .strong(),
                                );
                                for design in self.recent_designs.clone() {
                                    if ui
                                        .add(
                                            egui::Button::new(egui::RichText::new(&design).small())
                                                .min_size(egui::vec2(300.0, 28.0)),
                                        )
                                        .on_hover_text(t!(
                                            "dialogs.start_home.open_this_design_again_where.tip"
                                        ))
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
                                egui::RichText::new(t!(
                                    "dialogs.start_home.press_escape_or_click_pcbcraft"
                                ))
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
    fn compare_kind_color(&self, kind: CompareKind) -> Color32 {
        match kind {
            CompareKind::Added => Color32::from_rgb(72, 187, 120),
            CompareKind::Changed => Color32::from_rgb(255, 183, 77),
        }
    }

    fn refresh_compare_tints(&mut self) {
        self.compare_tint.clear();
        let Some(diff) = self.compare_diff.clone() else {
            return;
        };
        for change in &diff.changes {
            let kind = match change.kind.as_str() {
                "added" => CompareKind::Added,
                "changed" => CompareKind::Changed,
                _ => continue,
            };
            self.compare_tint.insert(change.object, kind);
            if let Some(fp) = self
                .project
                .board
                .footprints
                .iter()
                .find(|f| f.id == change.object)
            {
                self.compare_tint.insert(fp.component, CompareKind::Changed);
            }
            if self
                .project
                .components
                .iter()
                .any(|c| c.id == change.object)
                && kind == CompareKind::Changed
            {
                if let Some(fp) = self
                    .project
                    .board
                    .footprints
                    .iter()
                    .find(|f| f.component == change.object)
                {
                    self.compare_tint.insert(fp.id, CompareKind::Changed);
                }
            }
        }
    }

    pub(crate) fn run_design_compare(&mut self) -> Result<(), String> {
        let path = self.compare_baseline_path.trim();
        if path.is_empty() {
            return Err("pick a baseline design file to compare against".into());
        }
        let baseline = pcbcraft_io::load(std::path::Path::new(path))?;
        let diff = pcbcraft_io::project_diff(&baseline, &self.project);
        let count = diff.changes.len();
        self.compare_diff = Some(diff);
        self.refresh_compare_tints();
        self.message = if count == 0 {
            "Compare: the open design matches the baseline.".into()
        } else {
            format!(
                "Compare: {count} change{} from the baseline.",
                if count == 1 { "" } else { "s" }
            )
        };
        Ok(())
    }

    fn compare_window(&mut self, ctx: &egui::Context) {
        let mut open = self.compare_open;
        egui::Window::new("Compare design")
            .open(&mut open)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.label(
                    "Compare the open design against an older saved file. Changed parts highlight on the schematic and board.",
                );
                ui.horizontal(|ui| {
                    ui.label("Baseline file");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.compare_baseline_path)
                            .hint_text("path to the older design file")
                            .desired_width(260.0),
                    )
                    .on_hover_text(
                        "The saved design you want to diff against; your open design is the newer side.",
                    );
                });
                ui.horizontal(|ui| {
                    if ui
                        .button("Run compare")
                        .on_hover_text(
                            "Load the baseline file and highlight what changed in the open design.",
                        )
                        .clicked()
                        && let Err(err) = self.run_design_compare()
                    {
                        self.message = err;
                    }
                    if ui
                        .button("Show text report")
                        .on_hover_text(
                            "Open a scrollable list of every change, for copying into a review note.",
                        )
                        .clicked()
                    {
                        self.compare_report_open = true;
                    }
                    if ui
                        .button("Stop comparing")
                        .on_hover_text("Clear highlights and return to normal editing.")
                        .clicked()
                    {
                        self.compare_diff = None;
                        self.compare_tint.clear();
                        self.compare_open = false;
                        self.compare_report_open = false;
                        self.message = "Compare ended.".into();
                    }
                });
                ui.separator();
                ui.label(
                    "Legend: green tint means added, amber tint means changed. Removed items appear in the text report only.",
                );
                if let Some(diff) = &self.compare_diff {
                    ui.label(format!(
                        "{} change record(s) in the report.",
                        diff.changes.len()
                    ));
                }
            });
        self.compare_open = open;
        if self.compare_report_open {
            let mut report_open = self.compare_report_open;
            egui::Window::new("Compare report")
                .open(&mut report_open)
                .default_width(420.0)
                .show(ctx, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        if let Some(diff) = &self.compare_diff {
                            ui.monospace(diff.to_text());
                        } else {
                            ui.label("Run compare first to build a report.");
                        }
                    });
                });
            self.compare_report_open = report_open;
        }
    }

    fn file_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button(t!("menu.file"), |ui| {
            if ui
                .button(t!("menu.file.new"))
                .on_hover_text(t!("menu.file.new.tip"))
                .clicked()
            {
                self.tool_call("new_design", json!({}));
                self.select(None);
            }
            if ui
                .button(t!("menu.file.open"))
                .on_hover_text(t!("menu.file.open.tip"))
                .clicked()
            {
                self.home_open = true;
            }
            if ui
                .button(t!("menu.file.save"))
                .on_hover_text(t!("menu.file.save.tip"))
                .clicked()
            {
                self.tool_call("save", json!({"path":self.path}));
            }
            if ui
                .button("Compare with another design…")
                .on_hover_text(
                    "Diff the open design against an older saved file; highlights show on the canvas.",
                )
                .clicked()
            {
                self.compare_open = true;
            }
            ui.separator();
            if ui
                .button(t!("menu.file.export"))
                .on_hover_text(t!("menu.file.export.tip"))
                .clicked()
            {
                let path = std::path::Path::new(&self.path)
                    .with_extension("kicad_pcb")
                    .to_string_lossy()
                    .into_owned();
                self.tool_call("export", json!({"path":path,"format":"kicad_pcb"}));
            }
            if ui
                .button(t!("menu.file.copy_picture"))
                .on_hover_text(t!("menu.file.copy_picture.tip"))
                .clicked()
            {
                let ctx = ui.ctx().clone();
                self.copy_plot_to_clipboard(&ctx);
            }
        })
        .response
        .on_hover_text(t!("menu.file.tip"));
    }
    fn edit_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button(t!("menu.edit"), |ui| {
            let (can_undo, can_redo) = self
                .session
                .lock()
                .map(|s| (s.can_undo(), s.can_redo()))
                .unwrap_or((false, false));
            if ui
                .add_enabled(can_undo, egui::Button::new(t!("menu.edit.undo")))
                .on_hover_text(if can_undo {
                    t!("menu.edit.undo.tip")
                } else {
                    t!("menu.edit.undo.none")
                })
                .on_disabled_hover_text(t!("menu.edit.undo.none"))
                .clicked()
            {
                self.tool_call("undo", json!({}));
            }
            if ui
                .add_enabled(can_redo, egui::Button::new(t!("menu.edit.redo")))
                .on_hover_text(t!("menu.edit.redo.tip"))
                .on_disabled_hover_text(t!("menu.edit.redo.none"))
                .clicked()
            {
                self.tool_call("redo", json!({}));
            }
            ui.separator();
            if ui
                .button(t!("menu.edit.copy"))
                .on_hover_text(t!("menu.edit.copy.tip"))
                .clicked()
            {
                let ctx = ui.ctx().clone();
                self.copy_selection(&ctx);
            }
            if ui
                .button(t!("menu.edit.paste"))
                .on_hover_text(t!("menu.edit.paste.tip"))
                .clicked()
            {
                self.paste_last_copy();
            }
            if self.tab == Tab::Pcb
                && ui
                    .button(t!("menu.edit.cleanup"))
                    .on_hover_text(t!("menu.edit.cleanup.tip"))
                    .clicked()
            {
                self.cleanup_tracks();
            }
            if !self.project.auto_sync {
                ui.separator();
                if ui
                    .button(t!("menu.edit.sync"))
                    .on_hover_text(t!("menu.edit.sync.tip"))
                    .clicked()
                {
                    self.run("board.sync", json!({}));
                    self.change_tab(Tab::Pcb);
                }
            }
            ui.separator();
            if let Some(lit) = self.highlight_net
                && let Some(net) = self.project.nets.iter().find(|n| n.id == lit)
            {
                let name = net.name.clone();
                if ui
                    .button(t!("menu.edit.clear_net", name = name))
                    .on_hover_text(t!("menu.edit.clear_net.tip"))
                    .clicked()
                {
                    self.run("net.unroute", json!({"net": lit}));
                }
            }
            if ui
                .button(t!("menu.edit.clear_all"))
                .on_hover_text(t!("menu.edit.clear_all.tip"))
                .clicked()
            {
                self.run("board.unroute", json!({}));
            }
        })
        .response
        .on_hover_text(t!("menu.edit.tip"));
    }
    /// Switch the app's language; the next frame paints in it.
    fn set_language(&mut self, tag: &str) {
        self.preferences.language = tag.to_string();
        self.catalog = std::sync::Arc::new(pcbcraft_text::Catalog::for_language(tag));
        pcbcraft_text::activate(&self.catalog);
    }
    fn language_menu(&mut self, ui: &mut egui::Ui) {
        let mut picked = None;
        ui.menu_button(t!("menu.settings.language"), |ui| {
            let current = self.preferences.language.clone();
            if ui
                .selectable_label(
                    current == pcbcraft_text::SYSTEM,
                    t!("menu.settings.language.system"),
                )
                .on_hover_text(t!("menu.settings.language.system.tip"))
                .clicked()
            {
                picked = Some(pcbcraft_text::SYSTEM.to_string());
            }
            for language in pcbcraft_text::shipped_languages() {
                if ui
                    .selectable_label(current == language.tag, language.name)
                    .on_hover_text(t!("menu.settings.language.pick.tip"))
                    .clicked()
                {
                    picked = Some(language.tag);
                }
            }
        })
        .response
        .on_hover_text(t!("menu.settings.language.tip"));
        if let Some(tag) = picked {
            self.set_language(&tag);
            ui.ctx().request_repaint();
        }
    }
    fn help_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button(t!("menu.help"), |ui| {
            if ui
                .button(t!("menu.help.shortcuts"))
                .on_hover_text(t!("menu.help.shortcuts.tip"))
                .clicked()
            {
                self.shortcuts_open = !self.shortcuts_open;
            }
            if ui
                .button(t!("menu.help.commands"))
                .on_hover_text(t!("menu.help.commands.tip"))
                .clicked()
            {
                self.command_open = !self.command_open;
                self.command_focus = self.command_open;
                self.command_index = 0;
            }
            ui.separator();
            if ui
                .button(t!("app.report_bug"))
                .on_hover_text(t!("app.report_bug.tip"))
                .clicked()
            {
                let url = format!(
                    "https://github.com/tangency-labs/pcbcraft/issues/new?labels=bug&title=Bug%20report&body=Version%20{}%0A",
                    env!("CARGO_PKG_VERSION")
                );
                ui.ctx().open_url(egui::OpenUrl::new_tab(url));
            }
            ui.label(
                egui::RichText::new(t!("app.version", version = env!("CARGO_PKG_VERSION")))
                    .small()
                    .color(self.muted()),
            )
            .on_hover_text(t!("app.version.tip"));
        })
        .response
        .on_hover_text(t!("menu.help.tip"));
    }
    /// The datasheet address when the field holds a plain web link.
    fn datasheet_url(field: Option<&str>) -> Option<String> {
        let url = field?.trim();
        let lower = url.to_ascii_lowercase();
        ((lower.starts_with("http://") || lower.starts_with("https://"))
            && url.len() <= 2048
            && !url.chars().any(char::is_whitespace))
        .then(|| url.to_owned())
    }
    /// How many check findings are errors, warnings and notes.
    fn finding_counts(checks: &Value) -> (usize, usize, usize) {
        let mut counts = (0, 0, 0);
        for key in ["erc", "drc"] {
            let Some(findings) = checks.get(key).and_then(Value::as_array) else {
                continue;
            };
            for f in findings {
                match f.get("severity").and_then(Value::as_str) {
                    Some("error") => counts.0 += 1,
                    Some("warning") => counts.1 += 1,
                    _ => counts.2 += 1,
                }
            }
        }
        counts
    }
    fn browser(&mut self, ui: &mut egui::Ui) {
        egui::Panel::left("left-rail")
            .exact_size(44.0)
            .resizable(false)
            .frame(
                egui::Frame::side_top_panel(ui.style())
                    .inner_margin(egui::Margin::symmetric(4, 12)),
            )
            .show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(8.0);
                    if ui
                        .selectable_label(
                            self.preferences.library_dock_open,
                            t!("library.browser.lib"),
                        )
                        .on_hover_text(t!("library.browser.lib.tip"))
                        .clicked()
                    {
                        self.preferences.library_dock_open = !self.preferences.library_dock_open;
                    }
                    ui.add_space(8.0);
                    let counts = Self::finding_counts(&self.checks);
                    for (count, name, help) in [
                        (
                            counts.0,
                            "errors",
                            t!("library.browser.mistakes_the_checks_say_you"),
                        ),
                        (
                            counts.1,
                            "warnings",
                            t!("library.browser.warnings_worth_a_look_but"),
                        ),
                        (
                            counts.2,
                            "notes",
                            t!("library.browser.notes_from_the_checks_that"),
                        ),
                    ] {
                        if ui
                            .button(t!("library.browser.count", count = count))
                            .on_hover_text(t!(
                                "library.browser.count.tip",
                                count = count,
                                name = name,
                                help = help
                            ))
                            .clicked()
                        {
                            self.change_tab(Tab::Checks);
                        }
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
                    ui.label(egui::RichText::new(t!("library.browser.library")).strong());
                    if ui
                        .small_button(t!("library.browser.hide"))
                        .on_hover_text(t!("library.browser.hide.tip"))
                        .clicked()
                    {
                        self.preferences.library_dock_open = false;
                    }
                });
                ui.add_space(8.0);
                if ui
                    .button(t!("library.browser.add_part"))
                    .on_hover_text(t!("library.browser.add_part.tip"))
                    .clicked()
                {
                    self.palette = true;
                    self.part_search_focus = true;
                }
                if self.palette {
                    ui.label(egui::RichText::new(t!("library.browser.add_a_part")).strong());
                    let query_box = ui
                        .add(
                            egui::TextEdit::singleline(&mut self.part_search)
                                .hint_text(t!("library.browser.search_parts_by_name"))
                                .desired_width(f32::INFINITY),
                        )
                        .on_hover_text(t!("library.browser.search_parts_by_name.tip"));
                    if self.part_search_focus {
                        query_box.request_focus();
                        self.part_search_focus = false;
                    }
                    if ui
                        .button(t!("library.browser.browse_component_libraries"))
                        .on_hover_text(t!("library.browser.browse_component_libraries.tip"))
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
                                    egui::RichText::new(t!(
                                        "library.browser.nothing_here_matches_trim_yet",
                                        trim = self.part_search.trim()
                                    ))
                                    .small()
                                    .color(self.muted()),
                                );
                            }
                            if !built_in.is_empty() {
                                ui.label(
                                    egui::RichText::new(t!("library.browser.common_parts"))
                                        .size(10.0)
                                        .color(self.muted()),
                                );
                                for (kind, label) in built_in {
                                    if ui
                                        .button(label)
                                        .on_hover_text(t!(
                                            "library.browser.pick_this_part_the_next.tip"
                                        ))
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
                                    egui::RichText::new(t!(
                                        "library.browser.from_this_project_s_library"
                                    ))
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
                                    let row = ui
                                        .checkbox(
                                            &mut shown,
                                            egui::RichText::new(t!(
                                                "library.browser.set_len_part_v1",
                                                set = set,
                                                len = definitions.len(),
                                                v1 = if definitions.len() == 1 { "" } else { "s" }
                                            ))
                                            .strong(),
                                        )
                                        .on_hover_text(t!("library.browser.set_len_part_v1.tip"));
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
                            self.source_section(ui);
                        });
                }
                ui.add_space(12.0);
                ui.label(
                    egui::RichText::new(t!("library.browser.components"))
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
                            t!(
                                "library.browser.reference_value",
                                reference = c.reference,
                                value = c.value
                            ),
                        )
                        .on_hover_text(t!("library.browser.reference_value.tip"))
                        .clicked()
                    {
                        self.select(Some(id));
                    }
                    let what = c
                        .field(&t!("library.browser.description"))
                        .map(str::to_owned)
                        .or_else(|| {
                            c.library_symbol
                                .as_ref()
                                .map(|ls| ls.definition.description.clone())
                        })
                        .map(|d| d.trim().to_owned())
                        .filter(|d| !d.is_empty());
                    let origin = c.library_symbol.as_ref().map_or_else(
                        || t!("library.browser.built_in"),
                        |ls| t!("library.browser.from_name", name = ls.definition.name),
                    );
                    let note = what.map_or_else(
                        || origin.clone(),
                        |d| t!("library.browser.item_origin", item = d, origin = origin),
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
                        egui::RichText::new(t!("library.browser.renumber_parts"))
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
                    let mut pick = self
                        .renumber_plan
                        .clone()
                        .map_or_else(|| self.renumber_prefix.clone(), |(prefix, _)| prefix);
                    ui.horizontal(|ui| {
                        egui::ComboBox::from_id_salt("renumber-prefix")
                            .selected_text(
                                pick.clone()
                                    .unwrap_or_else(|| t!("library.browser.all_letters")),
                            )
                            .show_ui(ui, |ui| {
                                if ui
                                    .selectable_label(
                                        pick.is_none(),
                                        t!("library.browser.all_letters"),
                                    )
                                    .on_hover_text(t!("library.browser.all_letters.tip"))
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
                                        .on_hover_text(t!(
                                            "library.browser.only_renumber_the_parts_that.tip"
                                        ))
                                        .clicked()
                                    {
                                        pick = Some(prefix.clone());
                                        ui.close();
                                    }
                                }
                            })
                            .response
                            .on_hover_text(t!("library.browser.renumber_one_kind_of_part.tip"));
                        if ui
                            .button(t!("library.browser.preview"))
                            .on_hover_text(t!("library.browser.preview.tip"))
                            .clicked()
                        {
                            let out = self.run_and_keep(
                                "components.reannotate",
                                json!({"dry_run": true, "prefix": pick}),
                            );
                            self.renumber_plan =
                                out.get("renamed").and_then(Value::as_array).map(|rows| {
                                    (
                                        pick.clone(),
                                        rows.iter()
                                            .filter_map(|row| {
                                                let from = row.get("from")?.as_str()?.to_owned();
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
                                egui::RichText::new(t!(
                                    "library.browser.from_to",
                                    from = from,
                                    to = to
                                ))
                                .small()
                                .color(self.muted()),
                            );
                        }
                        ui.horizontal(|ui| {
                            if ui
                                .button(t!("library.browser.renumber"))
                                .on_hover_text(t!("library.browser.renumber.tip"))
                                .clicked()
                            {
                                self.renumber_plan = None;
                                self.run("components.reannotate", json!({"prefix": pick}));
                            }
                            if ui
                                .button(t!("library.browser.cancel"))
                                .on_hover_text(t!("library.browser.cancel.tip"))
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
            .button(t!("schematic.nets_tab.new_net"))
            .on_hover_text(t!("schematic.nets_tab.new_net.tip"))
            .clicked()
        {
            let name = t!(
                "schematic.nets_tab.net_v0",
                v0 = self.project.nets.len() + 1
            );
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
                egui::RichText::new(t!(
                    "schematic.nets_tab.total_mm_of_copper_routed",
                    total = format!("{:.1}", total),
                    routed = routed,
                    len = self.project.nets.len()
                ))
                .small()
                .color(self.muted()),
            )
            .on_hover_text(t!("schematic.nets_tab.total_mm_of_copper_routed.tip"));
        }
        for net in self.project.nets.clone() {
            if !self.tree_matches(&net.name) {
                continue;
            }
            let lit = self.highlight_net == Some(net.id);
            let renaming = self.net_draft.as_ref().is_some_and(|(id, _)| *id == net.id);
            ui.horizontal(|ui| {
                if ui
                    .add(
                        egui::Label::new(if lit { "\u{25cf}" } else { "\u{25cb}" })
                            .sense(egui::Sense::click()),
                    )
                    .on_hover_text(t!("schematic.nets_tab.spotlight_this_net_its_copper.tip"))
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
                            .hint_text(t!("schematic.nets_tab.net_name")),
                    )
                    .on_hover_text(t!("schematic.nets_tab.net_name.tip"));
                    if ui
                        .button(t!("schematic.nets_tab.use_name"))
                        .on_hover_text(t!("schematic.nets_tab.use_name.tip"))
                        .clicked()
                    {
                        let name = draft.clone();
                        self.net_draft = None;
                        self.tool_call("net.rename", json!({"id": net.id, "name": name}));
                    }
                    if ui
                        .button(t!("schematic.nets_tab.cancel"))
                        .on_hover_text(t!("schematic.nets_tab.cancel.tip"))
                        .clicked()
                    {
                        self.net_draft = None;
                    }
                } else {
                    if ui
                        .selectable_label(lit, &net.name)
                        .on_hover_text(t!("schematic.nets_tab.spotlight_this_net_its_copper.tip"))
                        .clicked()
                    {
                        self.highlight_net = if lit { None } else { Some(net.id) };
                    }
                    if ui
                        .small_button(t!("schematic.nets_tab.rename"))
                        .on_hover_text(t!("schematic.nets_tab.rename.tip"))
                        .clicked()
                    {
                        self.net_draft = Some((net.id, net.name.clone()));
                    }
                    let open = self.net_editor == Some(net.id);
                    if ui
                        .small_button(if open {
                            t!("schematic.nets_tab.hide_pins")
                        } else {
                            t!("schematic.nets_tab.pins")
                        })
                        .on_hover_text(t!("schematic.nets_tab.show_or_hide_the_list.tip"))
                        .clicked()
                    {
                        self.net_editor = if open { None } else { Some(net.id) };
                        self.pin_pick = None;
                    }
                    if let Some((length, vias)) = lengths.get(&net.id) {
                        ui.label(
                            egui::RichText::new(t!(
                                "schematic.nets_tab.length_mm_vias_vias",
                                length = format!("{:.1}", length),
                                vias = vias
                            ))
                            .small()
                            .color(self.muted()),
                        )
                        .on_hover_text(t!("schematic.nets_tab.length_mm_vias_vias.tip"));
                    } else {
                        ui.label(
                            egui::RichText::new(t!("schematic.nets_tab.not_routed_yet"))
                                .small()
                                .color(self.muted()),
                        )
                        .on_hover_text(t!("schematic.nets_tab.not_routed_yet.tip"));
                    }
                }
            });
            if self.net_editor == Some(net.id) {
                let mut members: Vec<(String, PinRef)> = Vec::new();
                let mut spares: Vec<(String, PinRef)> = Vec::new();
                for comp in &self.project.components {
                    for pin in &comp.pins {
                        let label = if pin.name.is_empty() || pin.name == pin.number {
                            t!(
                                "schematic.nets_tab.reference_pin_number",
                                reference = comp.reference,
                                number = pin.number
                            )
                        } else {
                            t!(
                                "schematic.nets_tab.reference_pin_number_name",
                                reference = comp.reference,
                                number = pin.number,
                                name = pin.name
                            )
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
                            egui::RichText::new(t!(
                                "schematic.nets_tab.no_pins_on_this_connection"
                            ))
                            .size(10.0)
                            .color(self.muted()),
                        );
                    } else {
                        for (label, r) in &members {
                            ui.horizontal(|ui| {
                                ui.label(label);
                                if ui
                                    .small_button(t!("schematic.nets_tab.text"))
                                    .on_hover_text(t!("schematic.nets_tab.text.tip"))
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
                                t!("schematic.nets_tab.choose_a_spare_pin")
                            } else {
                                pick.clone()
                            })
                            .show_ui(ui, |ui| {
                                for (label, _) in &spares {
                                    if ui
                                        .selectable_label(&pick == label, label)
                                        .on_hover_text(t!(
                                            "schematic.nets_tab.a_pin_that_is_not.tip"
                                        ))
                                        .clicked()
                                    {
                                        pick = label.clone();
                                        ui.close();
                                    }
                                }
                            })
                            .response
                            .on_hover_text(t!("schematic.nets_tab.pick_a_pin_to_add.tip"));
                        if ui
                            .add_enabled(
                                !pick.is_empty(),
                                egui::Button::new(t!("schematic.nets_tab.connect")),
                            )
                            .on_hover_text(t!("schematic.nets_tab.connect.tip"))
                            .clicked()
                            && let Some((_, r)) = spares.iter().find(|(label, _)| *label == pick)
                        {
                            self.tool_call("net.connect", json!({"net": net.id, "pins": [r]}));
                            self.pin_pick = None;
                        }
                    });
                    if spares.is_empty() {
                        ui.label(
                            egui::RichText::new(t!("schematic.nets_tab.every_pin_in_this_design"))
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
                egui::RichText::new(t!(
                    "schematic.nets_tab.spotlighting_name_on_the_board",
                    name = net.name
                ))
                .size(10.0)
                .color(self.accent()),
            );
        }
    }
    /// Room for a full-page panel to breathe: wide side margins and one
    /// readable column, so nothing sits flush against the window edge.
    /// Padding for a full-width tab body: 16 px at the sides, 12 above.
    fn padded<T>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> T) -> T {
        egui::Frame::NONE
            .inner_margin(egui::Margin {
                left: 16,
                right: 16,
                top: 12,
                bottom: 12,
            })
            .show(ui, add)
            .inner
    }
    /// A dock section title: 8 px of air above and 4 below.
    fn section_heading(&self, ui: &mut egui::Ui, text: &str) {
        ui.add_space(8.0);
        ui.label(egui::RichText::new(text).strong().color(self.ink()));
        ui.add_space(4.0);
    }
    fn margined<T>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> T) -> T {
        egui::Frame::NONE
            .inner_margin(egui::Margin::symmetric(28, 20))
            .show(ui, |ui| {
                ui.set_max_width(720.0);
                add(ui)
            })
            .inner
    }

    fn shortcuts_settings_panel(&mut self, ui: &mut egui::Ui) {
        ui.label(
            egui::RichText::new("KEYBOARD SHORTCUTS")
                .size(10.0)
                .color(self.muted()),
        );
        if !self.shortcuts_settings_open {
            if ui
                .button("Edit shortcuts…")
                .on_hover_text("Open the shortcut list to rebind keys and manage profiles.")
                .clicked()
            {
                self.shortcuts_settings_open = true;
            }
            return;
        }
        ui.label(
            egui::RichText::new(
                "Pick a profile, change any binding, and save or export it. Conflicts are blocked until you clear the other action.",
            )
            .small()
            .color(self.muted()),
        );
        if !self.keymap_notice.is_empty() {
            ui.label(
                egui::RichText::new(&self.keymap_notice)
                    .small()
                    .color(self.accent()),
            );
        }
        ui.horizontal(|ui| {
            ui.label("Profile");
            egui::ComboBox::from_id_salt("keymap-profile")
                .selected_text(&self.preferences.active_keymap_profile)
                .show_ui(ui, |ui| {
                    for name in self.preferences.keymap_profiles.keys() {
                        if ui
                            .selectable_label(self.preferences.active_keymap_profile == *name, name)
                            .on_hover_text("Switch to this saved shortcut set.")
                            .clicked()
                        {
                            self.preferences.active_keymap_profile = name.clone();
                        }
                    }
                });
        });
        ui.horizontal(|ui| {
            ui.label("Save as");
            ui.add(
                egui::TextEdit::singleline(&mut self.profile_name_draft)
                    .hint_text("Profile name")
                    .desired_width(160.0),
            )
            .on_hover_text("Name for a new profile copied from the current bindings.");
            if ui
                .button("Save profile")
                .on_hover_text("Store the current bindings under the name you typed.")
                .clicked()
            {
                let name = self.profile_name_draft.trim();
                if name.is_empty() {
                    self.keymap_notice = "Type a profile name first.".into();
                } else if name == "Default" {
                    self.keymap_notice = "Use Reset to default for the Default profile.".into();
                } else {
                    let map = self.preferences.active_keymap().clone();
                    self.preferences
                        .keymap_profiles
                        .insert(name.to_string(), map);
                    self.preferences.active_keymap_profile = name.to_string();
                    self.keymap_notice = format!("Saved profile {name}.");
                    self.profile_name_draft.clear();
                }
            }
            if ui
                .button("Delete profile")
                .on_hover_text("Remove the active profile except Default.")
                .clicked()
            {
                if self.preferences.active_keymap_profile == "Default" {
                    self.keymap_notice = "The Default profile cannot be deleted.".into();
                } else {
                    let name = self.preferences.active_keymap_profile.clone();
                    self.preferences.keymap_profiles.remove(&name);
                    self.preferences.active_keymap_profile = "Default".into();
                    self.keymap_notice = format!("Deleted profile {name}.");
                }
            }
        });
        ui.horizontal(|ui| {
            if ui
                .button("Reset to default")
                .on_hover_text("Restore factory chords for the active profile.")
                .clicked()
            {
                self.preferences.active_keymap_mut().reset_to_default();
                self.keymap_notice = "Bindings reset to factory defaults.".into();
            }
            if ui
                .button("Export profile…")
                .on_hover_text(
                    "Copy the active profile JSON into the import box for saving to a file.",
                )
                .clicked()
            {
                let file = keymap::KeymapProfileFile::from_keymap(
                    self.preferences.active_keymap_profile.clone(),
                    self.preferences.active_keymap(),
                );
                match serde_json::to_string_pretty(&file) {
                    Ok(text) => {
                        self.profile_import_text = text;
                        self.keymap_notice = "Profile JSON copied to the import box.".into();
                    }
                    Err(e) => self.keymap_notice = e.to_string(),
                }
            }
        });
        ui.label("Import profile JSON");
        ui.add(
            egui::TextEdit::multiline(&mut self.profile_import_text)
                .desired_rows(4)
                .desired_width(f32::INFINITY),
        )
        .on_hover_text(
            "Paste a profile file; invalid versions are rejected without changing stored prefs.",
        );
        if ui
            .button("Import profile")
            .on_hover_text("Load bindings from the JSON above into a named profile.")
            .clicked()
        {
            match serde_json::from_str::<keymap::KeymapProfileFile>(&self.profile_import_text) {
                Ok(file) => {
                    let name = file.name.clone();
                    match file.into_keymap() {
                        Ok(map) => {
                            self.preferences.keymap_profiles.insert(name.clone(), map);
                            self.preferences.active_keymap_profile = name.clone();
                            self.keymap_notice = format!("Imported profile {name}.");
                        }
                        Err(e) => self.keymap_notice = e,
                    }
                }
                Err(e) => self.keymap_notice = e.to_string(),
            }
        }
        egui::ScrollArea::vertical()
            .max_height(280.0)
            .show(ui, |ui| {
                egui::Grid::new("shortcut-settings-grid")
                    .num_columns(3)
                    .spacing([12.0, 6.0])
                    .striped(true)
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new("Action").strong());
                        ui.label(egui::RichText::new("Keys").strong());
                        ui.label(egui::RichText::new("Change").strong());
                        ui.end_row();
                        for action in keymap::rebindable_actions() {
                            let label = keymap::action_label(action);
                            ui.label(label)
                                .on_hover_text(keymap::action_tooltip(action));
                            let chord = self
                                .preferences
                                .active_keymap()
                                .chord_for(action)
                                .map(keymap::format_chord)
                                .unwrap_or_else(|| "Unassigned".into());
                            ui.label(egui::RichText::new(chord).monospace());
                            let waiting = self.binding_capture == Some(action);
                            let change = ui
                                .button(if waiting { "Press keys…" } else { "Change" })
                                .on_hover_text("Click then press the new key chord. Esc cancels.");
                            if change.clicked() {
                                self.binding_capture = Some(action);
                                self.keymap_notice =
                                    format!("Press a new chord for {label}; Esc cancels.");
                            }
                            ui.end_row();
                        }
                    });
            });
        if ui
            .button("Done")
            .on_hover_text("Close the shortcut editor and return to the rest of Settings.")
            .clicked()
        {
            self.shortcuts_settings_open = false;
            self.binding_capture = None;
        }
    }

    /// Everything that used to crowd the right-hand aside: how the app looks
    /// and every setting for the project as a whole.
    fn settings_tab(&mut self, ui: &mut egui::Ui) {
        ui.heading(t!("settings.settings_tab.settings"));
        ui.label(t!("settings.settings_tab.how_the_app_looks_and"));
        ui.add_space(10.0);
        ui.label(
            egui::RichText::new(t!("settings.settings_tab.appearance"))
                .size(10.0)
                .color(self.muted()),
        );
        ui.horizontal(|ui| {
            ui.selectable_value(
                &mut self.preferences.dark,
                true,
                t!("settings.settings_tab.dark"),
            )
            .on_hover_text(t!("settings.settings_tab.dark.tip"));
            ui.selectable_value(
                &mut self.preferences.dark,
                false,
                t!("settings.settings_tab.light"),
            )
            .on_hover_text(t!("settings.settings_tab.light.tip"));
        });
        ui.checkbox(
            &mut self.preferences.grid,
            t!("settings.settings_tab.show_grid"),
        );
        ui.label(t!("settings.settings_tab.interface_size"));
        ui.horizontal(|ui| {
            for (label, scale) in a11y::UI_SCALE_STEPS {
                let on = (self.preferences.ui_scale - scale).abs() < 0.01;
                if ui
                    .selectable_label(on, label)
                    .on_hover_text(t!(
                        "settings.settings_tab.make_text_and_controls_label.tip",
                        label = label
                    ))
                    .clicked()
                {
                    self.preferences.ui_scale = scale;
                }
            }
        });
        ui.horizontal(|ui| {
            ui.label(t!("settings.settings_tab.route_on"));
            ui.selectable_value(
                &mut self.route_layer,
                pcbcraft_doc::Layer::Front,
                t!("settings.settings_tab.top"),
            )
            .on_hover_text(t!("settings.settings_tab.top.tip"));
            ui.selectable_value(
                &mut self.route_layer,
                pcbcraft_doc::Layer::Back,
                t!("settings.settings_tab.bottom"),
            )
            .on_hover_text(t!("settings.settings_tab.bottom.tip"));
        });
        ui.horizontal(|ui| {
            ui.label(t!("settings.settings_tab.pad_labels"));
            for labels in [PadLabels::Number, PadLabels::Net, PadLabels::Off] {
                ui.selectable_value(&mut self.preferences.pad_labels, labels, labels.name())
                    .on_hover_text(t!("settings.settings_tab.choose_what_is_drawn_at.tip"));
            }
        });
        ui.label(t!("settings.settings_tab.accent_color"));
        ui.color_edit_button_srgb(&mut self.preferences.accent);
        self.language_menu(ui);
        ui.label(
            egui::RichText::new(t!(
                "settings.settings_tab.appearance_is_saved_automatically"
            ))
            .small()
            .color(self.muted()),
        );
        ui.add_space(16.0);
        self.shortcuts_settings_panel(ui);
        ui.add_space(16.0);
        assistant::settings::section(self, ui);
        ui.add_space(16.0);
        ui.label(
            egui::RichText::new("SCHEMATIC AND BOARD")
                .size(10.0)
                .color(self.muted()),
        );
        let mut sync_on = self.project.auto_sync;
        if ui
            .checkbox(
                &mut sync_on,
                "Keep the board in sync with the schematic automatically",
            )
            .on_hover_text(
                "When on, every schematic edit that changes parts, footprints or nets updates the board in the same undo step. Turn off to batch changes, then use Update board from schematic in the Edit menu.",
            )
            .changed()
            && sync_on != self.project.auto_sync
        {
            let _ = self.run("project.set_auto_sync", json!({"auto_sync": sync_on}));
        }
        ui.add_space(16.0);
        ui.label(
            egui::RichText::new(t!("settings.settings_tab.routing"))
                .size(10.0)
                .color(self.muted()),
        );
        ui.checkbox(
            &mut self.preferences.chamfer_corners,
            t!("settings.settings_tab.chamfer_corners"),
        )
        .on_hover_text(t!("settings.settings_tab.chamfer_corners.tip"));
        ui.add_space(16.0);
        ui.label(
            egui::RichText::new(t!("settings.settings_tab.board_geometry"))
                .size(10.0)
                .color(self.muted()),
        );
        field_help(ui, FIELD_HELP_BOARD, self.muted());
        ui.horizontal(|ui| {
            ui.label(t!("settings.settings_tab.w"));
            ui.add(
                egui::DragValue::new(&mut self.width)
                    .speed(0.5)
                    .suffix(format!(" {}", t!("unit.mm"))),
            );
        });
        ui.horizontal(|ui| {
            ui.label(t!("settings.settings_tab.h"));
            ui.add(
                egui::DragValue::new(&mut self.height)
                    .speed(0.5)
                    .suffix(format!(" {}", t!("unit.mm"))),
            );
        });
        if ui
            .button(t!("settings.settings_tab.apply_board_dimensions"))
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
            egui::RichText::new(t!("settings.settings_tab.cut_out_hole"))
                .size(10.0)
                .color(self.muted()),
        );
        ui.label(
            egui::RichText::new(t!("settings.settings_tab.a_round_hole_anywhere_inside"))
                .small()
                .color(self.muted()),
        );
        ui.horizontal(|ui| {
            ui.label(t!("settings.settings_tab.x"));
            ui.add(
                egui::DragValue::new(&mut self.cut_x)
                    .speed(0.5)
                    .suffix(format!(" {}", t!("unit.mm"))),
            );
            ui.label(t!("settings.settings_tab.y"));
            ui.add(
                egui::DragValue::new(&mut self.cut_y)
                    .speed(0.5)
                    .suffix(format!(" {}", t!("unit.mm"))),
            );
        });
        ui.horizontal(|ui| {
            ui.label(t!("settings.settings_tab.radius"));
            ui.add(
                egui::DragValue::new(&mut self.cut_r)
                    .speed(0.1)
                    .range(0.1..=50.0)
                    .suffix(format!(" {}", t!("unit.mm"))),
            );
        });
        if ui
            .button(t!("settings.settings_tab.add_hole"))
            .on_hover_text(t!("settings.settings_tab.add_hole.tip"))
            .clicked()
        {
            self.run(
                "board.cutout.add",
                json!({"center":{"x":self.cut_x,"y":self.cut_y},"radius":self.cut_r}),
            );
        }
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new(t!("settings.settings_tab.copper_pour"))
                .size(10.0)
                .color(self.muted()),
        );
        ui.label(
            egui::RichText::new(t!("settings.settings_tab.a_solid_rectangle_of_copper"))
                .small()
                .color(self.muted()),
        );
        if !self.project.nets.is_empty() && !self.project.nets.iter().any(|n| n.id == self.pour_net)
        {
            self.pour_net = self.project.nets[0].id;
        }
        ui.horizontal(|ui| {
            ui.label(t!("settings.settings_tab.x"));
            ui.add(
                egui::DragValue::new(&mut self.pour_x)
                    .speed(0.5)
                    .suffix(format!(" {}", t!("unit.mm"))),
            );
            ui.label(t!("settings.settings_tab.y"));
            ui.add(
                egui::DragValue::new(&mut self.pour_y)
                    .speed(0.5)
                    .suffix(format!(" {}", t!("unit.mm"))),
            );
        });
        ui.horizontal(|ui| {
            ui.label(t!("settings.settings_tab.w"));
            ui.add(
                egui::DragValue::new(&mut self.pour_w)
                    .speed(0.5)
                    .range(1.0..=500.0)
                    .suffix(format!(" {}", t!("unit.mm"))),
            );
            ui.label(t!("settings.settings_tab.h"));
            ui.add(
                egui::DragValue::new(&mut self.pour_h)
                    .speed(0.5)
                    .range(1.0..=500.0)
                    .suffix(format!(" {}", t!("unit.mm"))),
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
            .selected_text(t!(
                "settings.settings_tab.copper_for_v0",
                v0 = self.project.net_name(pour_net)
            ))
            .show_ui(ui, |ui| {
                for (id, name) in net_choices {
                    ui.selectable_value(&mut self.pour_net, id, name);
                }
            });
        if ui
            .button(t!("settings.settings_tab.pour_copper"))
            .on_hover_text(t!("settings.settings_tab.pour_copper.tip"))
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
            egui::RichText::new(t!("settings.settings_tab.board_labels"))
                .size(10.0)
                .color(self.muted()),
        );
        ui.label(
            egui::RichText::new(t!("settings.settings_tab.words_printed_on_the_board"))
                .small()
                .color(self.muted()),
        );
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.label_text)
                    .desired_width(110.0)
                    .hint_text(t!("settings.settings_tab.what_to_print")),
            )
            .on_hover_text(t!("settings.settings_tab.what_to_print.tip"));
        });
        ui.horizontal(|ui| {
            ui.label(t!("settings.settings_tab.at"));
            ui.add(
                egui::DragValue::new(&mut self.label_x)
                    .speed(0.5)
                    .suffix(format!(" {}", t!("unit.mm"))),
            );
            ui.add(
                egui::DragValue::new(&mut self.label_y)
                    .speed(0.5)
                    .suffix(format!(" {}", t!("unit.mm"))),
            );
            ui.add(
                egui::DragValue::new(&mut self.label_size)
                    .range(0.3..=20.0)
                    .speed(0.1)
                    .suffix(format!(" {}", t!("unit.mm"))),
            )
            .on_hover_text(t!("unit.mm15.tip"));
        });
        ui.horizontal(|ui| {
            ui.label(t!("settings.settings_tab.side"));
            ui.selectable_value(&mut self.label_front, true, t!("settings.settings_tab.top"));
            ui.selectable_value(
                &mut self.label_front,
                false,
                t!("settings.settings_tab.bottom"),
            );
        });
        if ui
            .button(t!("settings.settings_tab.add_label"))
            .on_hover_text(t!("settings.settings_tab.add_label.tip"))
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
                egui::RichText::new(t!("settings.settings_tab.words_on_the_board"))
                    .small()
                    .color(self.muted()),
            );
            for (id, text, front) in labels {
                ui.horizontal(|ui| {
                    ui.label(t!(
                        "settings.settings_tab.text_v0",
                        text = text,
                        v0 = if front { "Top" } else { "Bottom" }
                    ));
                    if ui
                        .small_button(t!("settings.settings_tab.erase"))
                        .on_hover_text(t!("settings.settings_tab.erase.tip"))
                        .clicked()
                    {
                        self.run("object.delete", json!({"id": id}));
                    }
                });
            }
        }
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new(t!("settings.settings_tab.board_edge"))
                .size(10.0)
                .color(self.muted()),
        );
        let corners = self.project.board.outline.as_ref().map_or(0, |o| o.len());
        if corners >= 3 {
            ui.label(
                egui::RichText::new(t!(
                    "settings.settings_tab.a_free_shape_with_corners",
                    corners = corners
                ))
                .small()
                .color(self.muted()),
            );
            if ui
                .button(t!("settings.settings_tab.back_to_a_plain_rectangle"))
                .on_hover_text(t!("settings.settings_tab.back_to_a_plain_rectangle.tip"))
                .clicked()
            {
                self.run("board.clear_outline", json!({}));
            }
        }
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.svg_path)
                    .hint_text(t!("settings.settings_tab.drawing_file_path_svg"))
                    .desired_width(150.0),
            )
            .on_hover_text(t!("settings.settings_tab.drawing_file_path_svg.tip"));
        });
        if ui
            .button(t!("settings.settings_tab.import_drawing"))
            .on_hover_text(t!("settings.settings_tab.import_drawing.tip"))
            .clicked()
        {
            self.run(
                "svg.import",
                json!({"path": self.svg_path.clone(), "use_outline": false}),
            );
        }
        if ui
            .button(t!("settings.settings_tab.import_as_board_edge"))
            .on_hover_text(t!("settings.settings_tab.import_as_board_edge.tip"))
            .clicked()
        {
            self.run(
                "svg.import",
                json!({"path": self.svg_path.clone(), "use_outline": true}),
            );
        }
        ui.horizontal(|ui| {
            ui.label(t!("settings.settings_tab.cutout_radius"));
            ui.add(
                egui::DragValue::new(&mut self.cutout_radius)
                    .speed(0.1)
                    .suffix(format!(" {}", t!("unit.mm"))),
            );
        });
        field_help(ui, FIELD_HELP_CUTOUT, self.muted());
        ui.add_space(12.0);
        self.reference_image_section(ui);
        ui.add_space(12.0);
        ui.label(
            egui::RichText::new(t!("settings.settings_tab.checks"))
                .size(10.0)
                .color(self.muted()),
        );
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
            .button(t!(
                "settings.settings_tab.erc_electrical_drc_board",
                erc = erc,
                drc = drc
            ))
            .on_hover_text(t!("settings.settings_tab.erc_electrical_drc_board.tip"))
            .clicked()
        {
            self.change_tab(Tab::Checks);
        }
        ui.label(
            egui::RichText::new(t!("settings.settings_tab.initial_checks_full_erc_drc"))
                .small()
                .color(self.muted()),
        );
        ui.add_space(12.0);
        ui.label(
            egui::RichText::new(t!("settings.settings_tab.make_files"))
                .size(10.0)
                .color(self.muted()),
        );
        ui.label(
            egui::RichText::new(t!("settings.settings_tab.plain_files_for_the_board"))
                .small()
                .color(self.muted()),
        );
        ui.add(
            egui::TextEdit::singleline(&mut self.make_dir)
                .hint_text(t!("settings.settings_tab.folder_for_the_files_blank"))
                .desired_width(220.0),
        )
        .on_hover_text(t!("settings.settings_tab.folder_for_the_files_blank.tip"));
        if ui
            .button(t!("settings.settings_tab.copper_drill_files"))
            .on_hover_text(t!("settings.settings_tab.copper_drill_files.tip"))
            .clicked()
        {
            self.write_make_files("gerber.export");
        }
        if ui
            .button(t!("settings.settings_tab.parts_list_csv"))
            .on_hover_text(t!("settings.settings_tab.parts_list_csv.tip"))
            .clicked()
        {
            self.write_make_files("bom.write");
        }
        if ui
            .button(t!("settings.settings_tab.net_list"))
            .on_hover_text(t!("settings.settings_tab.net_list.tip"))
            .clicked()
        {
            self.write_make_files("netlist.write");
        }
        if ui
            .button(t!("settings.settings_tab.pick_and_place_file"))
            .on_hover_text(t!("settings.settings_tab.pick_and_place_file.tip"))
            .clicked()
        {
            self.write_make_files("placement.write");
        }
        if ui
            .button(t!("settings.settings_tab.design_report"))
            .on_hover_text(t!("settings.settings_tab.design_report.tip"))
            .clicked()
        {
            self.write_make_files("report.write");
        }
        if ui
            .button(t!("settings.settings_tab.drill_map_csv"))
            .on_hover_text(t!("settings.settings_tab.drill_map_csv.tip"))
            .clicked()
        {
            self.write_job_file(
                "drillmap",
                "drill_chart",
                &t!("settings.settings_tab.drill_map"),
            );
        }
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new(t!("settings.settings_tab.board_drawings"))
                .size(10.0)
                .color(self.muted()),
        );
        ui.label(
            egui::RichText::new(t!("settings.settings_tab.flat_drawings_and_3d_models"))
                .small()
                .color(self.muted()),
        );
        if ui
            .button(t!("settings.settings_tab.3d_model_step"))
            .on_hover_text(t!("settings.settings_tab.3d_model_step.tip"))
            .clicked()
        {
            self.write_job_file("step3d", "step", &t!("settings.settings_tab.3d_model"));
        }
        if ui
            .button(t!("settings.settings_tab.3d_viewer_file"))
            .on_hover_text(t!("settings.settings_tab.3d_viewer_file.tip"))
            .clicked()
        {
            self.write_job_file("wrl3d", "vrml", &t!("settings.settings_tab.3d_viewer_file"));
        }
        if ui
            .button(t!("settings.settings_tab.printable_drawing_pdf"))
            .on_hover_text(t!("settings.settings_tab.printable_drawing_pdf.tip"))
            .clicked()
        {
            self.write_job_file(
                "pdfdraw",
                "pdf",
                &t!("settings.settings_tab.printable_drawing"),
            );
        }
        if ui
            .button(t!("settings.settings_tab.web_drawing_svg"))
            .on_hover_text(t!("settings.settings_tab.web_drawing_svg.tip"))
            .clicked()
        {
            self.write_job_file("svgdraw", "svg", &t!("settings.settings_tab.web_drawing"));
        }
        for (side, label, tip) in [
            (
                "top",
                t!("settings.settings_tab.print_art_top_silkscreen"),
                t!("settings.settings_tab.writes_the_top_silkscreen_as"),
            ),
            (
                "bottom",
                t!("settings.settings_tab.print_art_bottom_silkscreen"),
                t!("settings.settings_tab.writes_the_bottom_silkscreen_as"),
            ),
        ] {
            if ui.button(label.clone()).on_hover_text(tip).clicked() {
                self.write_uv_print_files(side, &label);
            }
        }
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new(t!("settings.settings_tab.panel_plan"))
                .size(10.0)
                .color(self.muted()),
        );
        ui.label(
            egui::RichText::new(t!("settings.settings_tab.see_how_your_board_would"))
                .small()
                .color(self.muted()),
        );
        ui.horizontal(|ui| {
            ui.add(
                egui::DragValue::new(&mut self.panel_columns)
                    .range(1..=32)
                    .suffix(format!(" {}", t!("settings.settings_tab.across")))
                    .speed(1.0),
            )
            .on_hover_text(t!("settings.settings_tab.across.tip"));
            ui.add(
                egui::DragValue::new(&mut self.panel_rows)
                    .range(1..=32)
                    .suffix(format!(" {}", t!("settings.settings_tab.down")))
                    .speed(1.0),
            )
            .on_hover_text(t!("settings.settings_tab.down.tip"));
        });
        ui.horizontal(|ui| {
            ui.add(
                egui::DragValue::new(&mut self.panel_gap)
                    .range(0.0..=25.0)
                    .speed(0.5)
                    .suffix(format!(" {}", t!("settings.settings_tab.mm_gap"))),
            )
            .on_hover_text(t!("settings.settings_tab.mm_gap.tip"));
            ui.add(
                egui::DragValue::new(&mut self.panel_rail)
                    .range(0.0..=25.0)
                    .speed(0.5)
                    .suffix(format!(" {}", t!("settings.settings_tab.mm_rail"))),
            )
            .on_hover_text(t!("settings.settings_tab.mm_rail.tip"));
        });
        if ui
            .button(t!("settings.settings_tab.preview_panel"))
            .on_hover_text(t!("settings.settings_tab.preview_panel.tip"))
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
                egui::RichText::new(t!(
                    "settings.settings_tab.copies_copies_panel_width_height",
                    copies = copies,
                    width = format!("{:.1}", width),
                    height = format!("{:.1}", height)
                ))
                .small()
                .color(self.muted()),
            )
            .on_hover_text(t!(
                "settings.settings_tab.copies_copies_panel_width_height.tip"
            ));
        }
        ui.label(
            egui::RichText::new(t!(
                "settings.settings_tab.writing_these_files_changes_nothing"
            ))
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
            egui::RichText::new(t!("rules.rules_tab.design_rules_travel_with_the"))
                .small()
                .color(self.muted()),
        );
        if let Some(pick) = &self.class_pick
            && !self.project.nets.iter().any(|n| n.name == *pick)
        {
            self.class_pick = None;
        }
        if ui
            .button(t!("rules.rules_tab.new_class"))
            .on_hover_text(t!("rules.rules_tab.new_class.tip"))
            .clicked()
        {
            let name = t!(
                "rules.rules_tab.class_v0",
                v0 = self.project.net_classes.len() + 1
            );
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
                    ui.label(t!("rules.rules_tab.len_nets", len = class.nets.len()))
                        .on_hover_text(t!("rules.rules_tab.len_nets.tip"));
                    if !editing {
                        if ui
                            .small_button(t!("rules.rules_tab.tune"))
                            .on_hover_text(t!("rules.rules_tab.tune.tip"))
                            .clicked()
                        {
                            self.rule_draft = Some((class.id, class.clearance, class.width));
                        }
                        if ui
                            .small_button(t!("rules.rules_tab.remove"))
                            .on_hover_text(t!("rules.rules_tab.remove.tip"))
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
                        ui.label(t!("rules.rules_tab.gap"));
                        ui.add(
                            egui::DragValue::new(clearance)
                                .speed(0.01)
                                .range(0.001..=100.0)
                                .suffix(format!(" {}", t!("unit.mm"))),
                        )
                        .on_hover_text(t!("unit.mm10.tip"));
                    });
                    ui.horizontal(|ui| {
                        ui.label(t!("rules.rules_tab.wire_width"));
                        ui.add(
                            egui::DragValue::new(width)
                                .speed(0.01)
                                .range(0.001..=100.0)
                                .suffix(format!(" {}", t!("unit.mm"))),
                        )
                        .on_hover_text(t!("unit.mm11.tip"));
                    });
                    ui.horizontal(|ui| {
                        if ui
                            .button(t!("rules.rules_tab.apply"))
                            .on_hover_text(t!("rules.rules_tab.apply.tip"))
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
                            .small_button(t!("rules.rules_tab.cancel"))
                            .on_hover_text(t!("rules.rules_tab.cancel.tip"))
                            .clicked()
                        {
                            self.rule_draft = None;
                        }
                    });
                } else {
                    ui.label(
                        egui::RichText::new(t!(
                            "rules.rules_tab.gap_clearance_mm_wire_width",
                            clearance = format!("{:.3}", class.clearance),
                            width = format!("{:.3}", class.width)
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
                                t!("rules.rules_tab.choose_a_connection_to_put")
                            } else {
                                pick.clone()
                            })
                            .show_ui(ui, |ui| {
                                for (name, _) in &free {
                                    if ui
                                        .selectable_label(&pick == name, name)
                                        .on_hover_text(t!(
                                            "rules.rules_tab.a_connection_that_follows_no.tip"
                                        ))
                                        .clicked()
                                    {
                                        pick = name.clone();
                                        ui.close();
                                    }
                                }
                            })
                            .response
                            .on_hover_text(t!("rules.rules_tab.pick_a_connection_then_assign.tip"));
                        if ui
                            .add_enabled(
                                !pick.is_empty(),
                                egui::Button::new(t!("rules.rules_tab.assign")),
                            )
                            .on_hover_text(t!("rules.rules_tab.assign.tip"))
                            .clicked()
                            && let Some((_, id)) = free.iter().find(|(name, _)| *name == pick)
                        {
                            self.run("netclass.assign", json!({"net": id, "class": class.id}));
                            self.class_pick = None;
                        }
                    });
                    self.class_pick = if pick.is_empty() { None } else { Some(pick) };
                    if free.is_empty() && class.nets.is_empty() {
                        ui.label(
                            egui::RichText::new(t!(
                                "rules.rules_tab.every_connection_already_follows_a"
                            ))
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
                            ui.label(format!("{} ", t!("rules.rules_tab.text")));
                            ui.label(&name);
                            if ui
                                .small_button(t!("rules.rules_tab.text2"))
                                .on_hover_text(t!("rules.rules_tab.text2.tip"))
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
                egui::RichText::new(t!("rules.rules_tab.no_rule_sets_yet_the"))
                    .small()
                    .color(self.muted()),
            );
        }
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new(t!("rules.rules_tab.teardrops"))
                .size(10.0)
                .color(self.muted()),
        );
        ui.label(
            egui::RichText::new(t!("rules.rules_tab.stem_shaped_copper_where_wires"))
                .small()
                .color(self.muted()),
        );
        ui.horizontal(|ui| {
            ui.add(
                egui::DragValue::new(&mut self.teardrop_depth)
                    .range(0.05..=5.0)
                    .speed(0.05)
                    .suffix(format!(" {}", t!("rules.rules_tab.mm_deep"))),
            )
            .on_hover_text(t!("rules.rules_tab.mm_deep.tip"));
            if ui
                .button(t!("rules.rules_tab.harden_junctions"))
                .on_hover_text(t!("rules.rules_tab.harden_junctions.tip"))
                .clicked()
            {
                self.run("teardrop.add", json!({"depth": self.teardrop_depth}));
            }
            if ui
                .small_button(t!("rules.rules_tab.remove_teardrops"))
                .on_hover_text(t!("rules.rules_tab.remove_teardrops.tip"))
                .clicked()
            {
                self.run("teardrop.remove", json!({}));
            }
        });
        ui.add_space(12.0);
        ui.label(
            egui::RichText::new(t!("rules.rules_tab.checks_you_write"))
                .size(10.0)
                .color(self.muted()),
        );
        ui.label(
            egui::RichText::new(t!("rules.rules_tab.rules_only_you_know_about"))
                .small()
                .color(self.muted()),
        );
        for c in self.project.custom_checks.clone() {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(t!(
                        "rules.rules_tab.name_v1_v2",
                        name = c.name,
                        v1 = custom_measure_line(&c.measure, c.limit),
                        v2 = if c.severity == "error" {
                            "must fix"
                        } else {
                            "a nudge"
                        }
                    ))
                    .small(),
                )
                .on_hover_text(t!("rules.rules_tab.name_v1_v2.tip"));
                if ui
                    .small_button(t!("rules.rules_tab.remove_this_check"))
                    .on_hover_text(t!("rules.rules_tab.remove_this_check.tip"))
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
                        .hint_text(t!("rules.rules_tab.what_this_check_means_in"))
                        .desired_width(150.0),
                );
                response.on_hover_text(t!("rules.rules_tab.the_name_this_check_reports.tip"));
                egui::ComboBox::from_id_salt("custom-measure")
                    .selected_text(pcbcraft_text::lookup(
                        CUSTOM_MEASURES[self.custom_measure.min(4)].0,
                        &[],
                    ))
                    .show_ui(ui, |ui| {
                        for (i, (label, _)) in CUSTOM_MEASURES.iter().enumerate() {
                            ui.selectable_value(
                                &mut self.custom_measure,
                                i,
                                pcbcraft_text::lookup(label, &[]),
                            );
                        }
                    })
                    .response
                    .on_hover_text(t!("rules.rules_tab.the_one_thing_this_check.tip"));
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
                .on_hover_text(t!("rules.rules_tab.the_limit_millimetres_for_wire.tip"));
                if ui
                    .selectable_label(self.custom_loud, t!("rules.rules_tab.must_fix"))
                    .on_hover_text(t!("rules.rules_tab.must_fix.tip"))
                    .clicked()
                {
                    self.custom_loud = true;
                }
                if ui
                    .selectable_label(!self.custom_loud, t!("rules.rules_tab.a_nudge"))
                    .on_hover_text(t!("rules.rules_tab.a_nudge.tip"))
                    .clicked()
                {
                    self.custom_loud = false;
                }
            });
            if ui
                .button(t!("rules.rules_tab.add_this_check"))
                .on_hover_text(t!("rules.rules_tab.add_this_check.tip"))
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
            egui::RichText::new(t!("rules.rules_tab.how_strict_to_be"))
                .size(10.0)
                .color(self.muted()),
        );
        ui.label(
            egui::RichText::new(t!("rules.rules_tab.choose_what_each_automatic_check"))
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
                            t!("rules.rules_tab.treat_this_as_a_real")
                        }
                        "warning" => t!("rules.rules_tab.mention_it_but_do_not"),
                        _ => t!("rules.rules_tab.stop_reporting_this_check_at"),
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
                    .small_button(t!("rules.rules_tab.auto"))
                    .on_hover_text(t!("rules.rules_tab.auto.tip"))
                    .clicked()
                {
                    self.run("rules.severity", json!({"code": code, "severity": null}));
                }
            });
        }
        ui.add_space(10.0);
        ui.label(
            egui::RichText::new(t!("rules.rules_tab.pair_clearances"))
                .size(10.0)
                .color(self.muted()),
        );
        ui.label(
            egui::RichText::new(t!("rules.rules_tab.extra_room_between_two_connections"))
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
                .unwrap_or_else(|| t!("rules.rules_tab.net_wanted", wanted = wanted))
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
                                .on_hover_text(t!("rules.rules_tab.pick_the_first_net_of.tip"));
                        }
                    });
                ui.label(t!("rules.rules_tab.away_from"));
                egui::ComboBox::from_id_salt("pair_b")
                    .selected_text(name_of(self.pair_b))
                    .show_ui(ui, |ui| {
                        for (id, name) in &nets {
                            ui.selectable_value(&mut self.pair_b, *id, name)
                                .on_hover_text(t!("rules.rules_tab.pick_the_second_net_of.tip"));
                        }
                    });
            });
            ui.horizontal(|ui| {
                ui.label(t!("rules.rules_tab.keep_apart_by"));
                ui.add(
                    egui::DragValue::new(&mut self.pair_clearance)
                        .range(0.05..=25.0)
                        .speed(0.05)
                        .suffix(format!(" {}", t!("unit.mm"))),
                )
                .on_hover_text(t!("unit.mm12.tip"));
            });
            if ui
                .button(t!("rules.rules_tab.set_spacing"))
                .on_hover_text(t!("rules.rules_tab.set_spacing.tip"))
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
                egui::RichText::new(t!("rules.rules_tab.spacing_rules_you_have_set"))
                    .small()
                    .color(self.muted()),
            );
            for (a, b, clearance) in pairs {
                ui.horizontal(|ui| {
                    ui.label(t!(
                        "rules.rules_tab.v0_away_from_v1_clearance",
                        v0 = name_of(a),
                        v1 = name_of(b),
                        clearance = clearance
                    ));
                    if ui
                        .small_button(t!("rules.rules_tab.clear"))
                        .on_hover_text(t!("rules.rules_tab.clear.tip"))
                        .clicked()
                    {
                        self.run("board.remove_pair_rule", json!({"net_a": a, "net_b": b}));
                    }
                });
            }
        }
        ui.add_space(10.0);
        ui.label(
            egui::RichText::new(t!("rules.rules_tab.differential_pairs"))
                .size(10.0)
                .color(self.muted()),
        );
        ui.label(
            egui::RichText::new(t!("rules.rules_tab.two_connections_that_should_run"))
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
                ui.label(t!("rules.rules_tab.name"));
                ui.add(
                    egui::TextEdit::singleline(&mut self.dp_name)
                        .hint_text(t!("rules.rules_tab.usb"))
                        .desired_width(90.0),
                )
                .on_hover_text(t!("rules.rules_tab.usb.tip"));
            });
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt("dp_p")
                    .selected_text(name_of(self.dp_p))
                    .show_ui(ui, |ui| {
                        for (id, name) in &nets {
                            ui.selectable_value(&mut self.dp_p, *id, name)
                                .on_hover_text(t!("rules.rules_tab.pick_the_net_that_carries.tip"));
                        }
                    });
                ui.label(t!("rules.rules_tab.text3"));
                egui::ComboBox::from_id_salt("dp_n")
                    .selected_text(name_of(self.dp_n))
                    .show_ui(ui, |ui| {
                        for (id, name) in &nets {
                            ui.selectable_value(&mut self.dp_n, *id, name)
                                .on_hover_text(t!(
                                    "rules.rules_tab.pick_the_net_that_carries2.tip"
                                ));
                        }
                    });
            });
            ui.horizontal(|ui| {
                ui.label(t!("rules.rules_tab.run_together_with_a_gap"));
                ui.add(
                    egui::DragValue::new(&mut self.dp_gap)
                        .range(0.02..=10.0)
                        .speed(0.02)
                        .suffix(format!(" {}", t!("unit.mm"))),
                )
                .on_hover_text(t!("unit.mm13.tip"));
            });
            if ui
                .button(t!("rules.rules_tab.declare_pair"))
                .on_hover_text(t!("rules.rules_tab.declare_pair.tip"))
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
                egui::RichText::new(t!("rules.rules_tab.pairs_you_have_declared"))
                    .small()
                    .color(self.muted()),
            );
            for (id, name, p, n, gap) in pairs {
                ui.horizontal(|ui| {
                    ui.label(t!(
                        "rules.rules_tab.name_v0_v1_gap_gap",
                        name = name,
                        v0 = name_of(p),
                        v1 = name_of(n),
                        gap = gap
                    ));
                    if ui
                        .small_button(t!("rules.rules_tab.remove"))
                        .on_hover_text(t!("rules.rules_tab.remove2.tip"))
                        .clicked()
                    {
                        self.run("board.remove_diffpair", json!({"id": id}));
                    }
                });
            }
        }
        ui.add_space(10.0);
        ui.label(
            egui::RichText::new(t!("rules.rules_tab.board_areas"))
                .size(10.0)
                .color(self.muted()),
        );
        ui.label(
            egui::RichText::new(t!("rules.rules_tab.a_corner_of_the_board"))
                .small()
                .color(self.muted()),
        );
        ui.horizontal(|ui| {
            ui.label(t!("rules.rules_tab.corner"));
            ui.add(
                egui::DragValue::new(&mut self.area_x)
                    .speed(0.5)
                    .suffix(format!(" {}", t!("unit.mm"))),
            );
            ui.add(
                egui::DragValue::new(&mut self.area_y)
                    .speed(0.5)
                    .suffix(format!(" {}", t!("unit.mm"))),
            );
            ui.label(t!("rules.rules_tab.size"));
            ui.add(
                egui::DragValue::new(&mut self.area_w)
                    .speed(0.5)
                    .range(1.0..=500.0)
                    .suffix(format!(" {}", t!("unit.mm"))),
            );
            ui.add(
                egui::DragValue::new(&mut self.area_h)
                    .speed(0.5)
                    .range(1.0..=500.0)
                    .suffix(format!(" {}", t!("unit.mm"))),
            );
        });
        ui.horizontal(|ui| {
            ui.label(t!("rules.rules_tab.copper_stays_apart_by_at"));
            ui.add(
                egui::DragValue::new(&mut self.area_gap)
                    .range(0.05..=25.0)
                    .speed(0.05)
                    .suffix(format!(" {}", t!("unit.mm"))),
            )
            .on_hover_text(t!("unit.mm14.tip"));
        });
        ui.horizontal(|ui| {
            ui.checkbox(
                &mut self.area_limited,
                t!("rules.rules_tab.parts_here_stand_no_taller"),
            )
            .on_hover_text(t!("rules.rules_tab.parts_here_stand_no_taller.tip"));
            if self.area_limited {
                ui.add(
                    egui::DragValue::new(&mut self.area_height)
                        .range(0.1..=100.0)
                        .speed(0.5)
                        .suffix(format!(" {}", t!("unit.mm"))),
                );
            }
        });
        if ui
            .button(t!("rules.rules_tab.add_area"))
            .on_hover_text(t!("rules.rules_tab.add_area.tip"))
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
                        t!(
                            "rules.rules_tab.area_at_round_round2",
                            round = a.region.x.round(),
                            round2 = a.region.y.round()
                        )
                    }),
                    a.clearance,
                    a.max_height,
                )
            })
            .collect();
        if !areas.is_empty() {
            ui.label(
                egui::RichText::new(t!("rules.rules_tab.stricter_areas_on_the_board"))
                    .small()
                    .color(self.muted()),
            );
            for (id, name, clearance, max_height) in areas {
                ui.horizontal(|ui| {
                    ui.label(t!(
                        "rules.rules_tab.name_gap_clearance_mm_v0",
                        name = name,
                        clearance = clearance,
                        v0 = max_height.map_or(String::new(), |h| format!(" · up to {h} mm tall"))
                    ));
                    if ui
                        .small_button(t!("rules.rules_tab.erase_area"))
                        .on_hover_text(t!("rules.rules_tab.erase_area.tip"))
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
            egui::RichText::new(t!("docks.builds_tab.one_design_several_builds_each"))
                .small()
                .color(self.muted()),
        );
        if ui
            .button(t!("docks.builds_tab.new_build"))
            .on_hover_text(t!("docks.builds_tab.new_build.tip"))
            .clicked()
        {
            let name = t!(
                "docks.builds_tab.build_v0",
                v0 = self.project.variants.len() + 1
            );
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
                    ui.label(t!("docks.builds_tab.len_left_off", len = variant.dnp.len())).on_hover_text(
                        t!("docks.builds_tab.len_left_off.tip"),
                    );
                    if !variant.footprints.is_empty() {
                        ui.label(t!("docks.builds_tab.len_swapped", len = variant.footprints.len())).on_hover_text(
                            t!("docks.builds_tab.len_swapped.tip"),
                        );
                    }
                    if !editing {
                        if ui
                            .small_button(t!("docks.builds_tab.note"))
                            .on_hover_text(
                                t!("docks.builds_tab.note.tip"),
                            )
                            .clicked()
                        {
                            self.build_note =
                                Some((variant.name.clone(), variant.note.clone().unwrap_or_default()));
                        }
                        if ui
                            .small_button(t!("docks.builds_tab.remove"))
                            .on_hover_text(
                                t!("docks.builds_tab.remove.tip"),
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
                            .hint_text(t!("docks.builds_tab.what_this_build_changes")),
                    )
                    .on_hover_text(t!("docks.builds_tab.what_this_build_changes.tip"));
                    let save = ui
                        .button(t!("docks.builds_tab.save_note"))
                        .on_hover_text(t!("docks.builds_tab.save_note.tip"))
                        .clicked();
                    let cancel = ui
                        .small_button(t!("docks.builds_tab.cancel"))
                        .on_hover_text(t!("docks.builds_tab.cancel.tip"))
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
                            ui.label(format!("{} ", t!("docks.builds_tab.text")));
                            ui.label(&reference);
                            if ui
                                .small_button(t!("docks.builds_tab.text2"))
                                .on_hover_text(
                                    t!("docks.builds_tab.text2.tip"),
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
                                t!("docks.builds_tab.choose_a_part_to_leave")
                            } else {
                                pick.clone()
                            })
                            .show_ui(ui, |ui| {
                                for reference in &free {
                                    if ui
                                        .selectable_label(&pick == reference, reference)
                                        .on_hover_text(t!("docks.builds_tab.a_part_this_build_still.tip"))
                                        .clicked()
                                    {
                                        pick = reference.clone();
                                        ui.close();
                                    }
                                }
                            })
                            .response
                            .on_hover_text(
                                t!("docks.builds_tab.pick_a_part_then_leave.tip"),
                            );
                        if ui
                            .add_enabled(!pick.is_empty(), egui::Button::new(t!("docks.builds_tab.leave_off")))
                            .on_hover_text(
                                t!("docks.builds_tab.leave_off.tip"),
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
                            egui::RichText::new(t!("docks.builds_tab.every_part_in_this_design"))
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
                            ui.label(format!("{} ", t!("docks.builds_tab.text")));
                            ui.label(t!("docks.builds_tab.reference_fits_footprint", reference = reference, footprint = swap.footprint));
                            if ui
                                .small_button(t!("docks.builds_tab.text2"))
                                .on_hover_text(t!("docks.builds_tab.text22.tip"))
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
                                t!("docks.builds_tab.choose_a_part_to_swap")
                            } else {
                                swap.clone()
                            })
                            .show_ui(ui, |ui| {
                                for c in &self.project.components {
                                    if ui
                                        .selectable_label(swap == c.reference, &c.reference)
                                        .on_hover_text(t!("docks.builds_tab.a_part_in_this_design.tip"))
                                        .clicked()
                                    {
                                        swap = c.reference.clone();
                                        ui.close();
                                    }
                                }
                            })
                            .response
                            .on_hover_text(
                                t!("docks.builds_tab.pick_a_part_name_its.tip"),
                            );
                        ui.add(
                            egui::TextEdit::singleline(&mut self.swap_name)
                                .desired_width(80.0)
                                .hint_text(t!("docks.builds_tab.package_name")),
                        )
                        .on_hover_text(
                            t!("docks.builds_tab.package_name.tip"),
                        );
                        if ui
                            .add_enabled(
                                !swap.is_empty() && !self.swap_name.trim().is_empty(),
                                egui::Button::new(t!("docks.builds_tab.swap_package")),
                            )
                            .on_hover_text(
                                t!("docks.builds_tab.swap_package.tip"),
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
                egui::RichText::new(t!("docks.builds_tab.no_extra_builds_this_board"))
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
            self.message = t!("status.write_make_files.that_folder_could_not_be");
            return;
        }
        self.tool_call(tool, json!({"path": directory}));
    }
    /// Write the UV print artwork for one side into the make folder.
    fn write_uv_print_files(&mut self, side: &str, label: &str) {
        let directory = self.make_directory();
        if std::fs::create_dir_all(&directory).is_err() {
            self.message = t!("status.write_uv_print_files.that_folder_could_not_be");
            return;
        }
        let job = format!("uvprint-{side}");
        let _ = self.tool_call(
            "output_job.add",
            json!({"name": &job, "kinds": ["uv_print"]}),
        );
        if self.tool_call(
            "output_job.run",
            json!({"name": &job, "directory": &directory, "uv_print": {"side": side}}),
        ) {
            self.message = t!(
                "status.write_uv_print_files.label_saved_in_directory",
                label = label,
                directory = directory
            );
        }
    }
    fn write_job_file(&mut self, job: &str, kind: &str, label: &str) {
        let directory = self.make_directory();
        if std::fs::create_dir_all(&directory).is_err() {
            self.message = t!("status.write_job_file.that_folder_could_not_be");
            return;
        }
        let _ = self.tool_call("output_job.add", json!({"name": job, "kinds": [kind]}));
        if self.tool_call(
            "output_job.run",
            json!({"name": job, "directory": &directory}),
        ) {
            self.message = t!(
                "status.write_job_file.label_saved_in_directory",
                label = label,
                directory = directory
            );
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
                        self.section_heading(ui, &t!("board.inspector.layers"));
                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .show(ui, |ui| self.layers_tab(ui));
                    },
                );
                ui.add(
                    egui::Slider::new(&mut self.preferences.aside_split, 0.25..=0.75)
                        .show_value(false)
                        .text(t!("board.inspector.resize")),
                )
                .on_hover_text(t!("board.inspector.resize.tip"));
                ui.horizontal(|ui| {
                    ui.selectable_value(
                        &mut self.preferences.aside_lower_tab,
                        0,
                        t!("board.inspector.properties"),
                    )
                    .on_hover_text(t!("board.inspector.properties.tip"));
                    if assistant::shown(self) {
                        ui.selectable_value(
                            &mut self.preferences.aside_lower_tab,
                            1,
                            assistant::tab_label(self.assistant.waiting()),
                        )
                        .on_hover_text(assistant::tab_tip());
                    }
                });
                if self.preferences.aside_lower_tab == 1 && assistant::shown(self) {
                    assistant::pane::show(self, ui);
                    return;
                }
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        self.draw_tool_options(ui);
                        self.section_heading(ui, &t!("board.inspector.details"));
                        if let Some(id) = self
                            .selected
                            .filter(|id| datums::is_datum(&self.project, *id))
                        {
                            self.datum_inspector(ui, id);
                        } else if let Some(id) = self.selected {
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
                                        .selected_text(t!(
                                            "board.inspector.unit_selected_unit",
                                            selected_unit = self.selected_unit
                                        ))
                                        .show_ui(ui, |ui| {
                                            for unit in std::iter::once(1)
                                                .chain(c.units.iter().map(|u| u.unit))
                                            {
                                                ui.selectable_value(
                                                    &mut self.selected_unit,
                                                    unit,
                                                    t!("board.inspector.unit_unit", unit = unit),
                                                )
                                                .on_hover_text(t!("board.inspector.unit_unit.tip"));
                                            }
                                        });
                                    if before != self.selected_unit {
                                        self.load_properties();
                                    }
                                }
                                ui.label(&c.kind);
                                ui.label(
                                    egui::RichText::new(t!(
                                        "board.inspector.stable_id_id",
                                        id = id
                                    ))
                                    .small()
                                    .color(self.muted()),
                                );
                                ui.add_space(10.0);
                                ui.label(t!("board.inspector.value"));
                                ui.text_edit_singleline(&mut self.value);
                                field_help(ui, FIELD_HELP_VALUE, self.muted());
                                if ui
                                    .button(t!("board.inspector.apply_value"))
                                    .on_hover_text(command_tip("component.value"))
                                    .clicked()
                                {
                                    self.run(
                                        "component.value",
                                        json!({"id":c.id,"value":self.value}),
                                    );
                                }
                                ui.add_space(12.0);
                                ui.label(t!("board.inspector.position_mm"));
                                field_help(ui, FIELD_HELP_POSITION, self.muted());
                                ui.horizontal(|ui| {
                                    ui.label(t!("board.inspector.x"));
                                    ui.add(egui::DragValue::new(&mut self.x).speed(0.1));
                                    ui.label(t!("board.inspector.y"));
                                    ui.add(egui::DragValue::new(&mut self.y).speed(0.1));
                                });
                                if ui
                                    .button(t!("board.inspector.apply_position"))
                                    .on_hover_text(t!("board.inspector.apply_position.tip"))
                                    .clicked()
                                {
                                    let mut params =
                                        json!({"id":id,"position":{"x":self.x,"y":self.y}});
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
                                if ui
                                    .button(t!("board.inspector.rotate_90_r"))
                                    .on_hover_text(command_tip("component.rotate"))
                                    .clicked()
                                {
                                    self.rotate();
                                }
                                if ui
                                    .button(t!("board.inspector.duplicate"))
                                    .on_hover_text(t!("board.inspector.duplicate.tip"))
                                    .clicked()
                                {
                                    self.duplicate_selection();
                                }
                                let sheet =
                                    Self::datasheet_url(c.field(&t!("board.inspector.datasheet")));
                                if ui
                                    .add_enabled(
                                        sheet.is_some(),
                                        egui::Button::new(t!("board.inspector.view_datasheet")),
                                    )
                                    .on_hover_text(if sheet.is_some() {
                                        t!("board.inspector.open_this_part_s_datasheet")
                                    } else {
                                        t!("board.inspector.this_part_has_no_web")
                                    })
                                    .clicked()
                                    && let Some(url) = sheet
                                {
                                    ui.ctx().open_url(egui::OpenUrl::new_tab(url));
                                }
                                if self.project.footprint(id).is_some() {
                                    ui.label(t!("board.inspector.dock_to_board_edge"));
                                    field_help(
                                        ui,
                                        &t!("board.inspector.for_an_edge_connector_the"),
                                        self.muted(),
                                    );
                                    ui.horizontal(|ui| {
                                        for edge in ["left", "right", "top", "bottom"] {
                                            if ui
                                                .button(t!(
                                                    "board.inspector.dock_edge",
                                                    edge = edge
                                                ))
                                                .on_hover_text(t!(
                                                    "board.inspector.dock_edge.tip",
                                                    edge = edge
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
                                    let locked = self
                                        .project
                                        .footprint(id)
                                        .is_some_and(|fp| fp.locked == Some(true));
                                    if ui
                                        .button(if locked {
                                            t!("board.inspector.unlock")
                                        } else {
                                            t!("board.inspector.lock_in_place")
                                        })
                                        .on_hover_text(if locked {
                                            t!("board.inspector.let_this_part_move_rotate")
                                        } else {
                                            t!("board.inspector.freeze_this_part_so_no")
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
                                        .map(|f| {
                                            t!(
                                                "board.inspector.footprint_name",
                                                name = f.definition.name
                                            )
                                        })
                                        .unwrap_or_else(|| {
                                            if c.library_symbol.is_some() {
                                                t!("board.inspector.footprint_unassigned")
                                            } else {
                                                t!("board.inspector.footprint_starter_package")
                                            }
                                        }),
                                );
                                if ui
                                    .button(t!("board.inspector.assign_footprint_in_libraries"))
                                    .on_hover_text(t!(
                                        "board.inspector.assign_footprint_in_libraries.tip"
                                    ))
                                    .clicked()
                                {
                                    self.change_tab(Tab::Libraries);
                                    self.library.kind = true;
                                    self.library.pending = None;
                                    self.library.path.clear();
                                    self.library.results = None;
                                    self.library.preview = None;
                                    self.library.query.clear();
                                }
                                ui.add_space(12.0);
                                ui.label(t!("board.inspector.pin_net"));
                                for pin in &c.pins {
                                    ui.label(t!(
                                        "board.inspector.number_v1",
                                        number = pin.number,
                                        v1 = pin
                                            .net
                                            .map(|n| self.project.net_name(n))
                                            .unwrap_or("unassigned")
                                    ));
                                }
                            } else {
                                ui.heading(t!("board.inspector.selected_object"));
                                ui.label(t!("board.inspector.stable_id_id", id = id));
                                for (name, value) in object_detail_rows(&self.project, id) {
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            egui::RichText::new(name).small().color(self.muted()),
                                        );
                                        ui.label(value);
                                    });
                                }
                                self.via_span_controls(ui, id);
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
                                    let max_cut =
                                        mate.map_or(0.05, |(_, len)| len.min(own_len) * 0.45);
                                    ui.add_space(10.0);
                                    ui.label(t!("board.inspector.wire_width"));
                                    ui.add(
                                        egui::DragValue::new(&mut self.track_width)
                                            .range(0.05..=10.0)
                                            .speed(0.05)
                                            .suffix(format!(" {}", t!("unit.mm"))),
                                    )
                                    .on_hover_text(t!("unit.mm7.tip"));
                                    if ui
                                        .button(t!("board.inspector.set_width"))
                                        .on_hover_text(command_tip("track.set_width"))
                                        .clicked()
                                    {
                                        self.run(
                                            "track.set_width",
                                            json!({"id": id, "width": self.track_width}),
                                        );
                                    }
                                    if ui
                                        .button(t!("board.inspector.thicken_whole_connection"))
                                        .on_hover_text(t!(
                                            "board.inspector.thicken_whole_connection.tip"
                                        ))
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
                                            egui::Button::new(t!("board.inspector.straighten")),
                                        )
                                        .on_hover_text(if curved {
                                            t!("board.inspector.pull_this_curved_wire_into")
                                        } else {
                                            t!("board.inspector.this_wire_is_already_straight")
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
                                                .suffix(format!(" {}", t!("unit.mm"))),
                                        )
                                        .on_hover_text(t!("unit.mm8.tip"));
                                        if ui
                                            .add_enabled(
                                                corner_ready,
                                                egui::Button::new(t!("board.inspector.cut_corner")),
                                            )
                                            .on_hover_text(if corner_ready {
                                                t!("board.inspector.trade_the_sharp_angle_where")
                                            } else {
                                                t!("board.inspector.this_wire_does_not_meet")
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
                                        .button(t!("board.inspector.cut_in_half"))
                                        .on_hover_text(t!("board.inspector.cut_in_half.tip"))
                                        .clicked()
                                    {
                                        self.run(
                                            "track.split",
                                            json!({"id": id, "at": {"x": mid.0, "y": mid.1}}),
                                        );
                                    }
                                    self.impedance_section(ui, id, net);
                                }
                                if let Some(zone) = self
                                    .project
                                    .board
                                    .zones
                                    .iter()
                                    .find(|z| z.id == id)
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
                                    ui.label(t!("board.inspector.ground_stitching"));
                                    ui.add(
                                        egui::DragValue::new(&mut self.stitch_spacing)
                                            .range(0.5..=50.0)
                                            .speed(0.5)
                                            .suffix(format!(" {}", t!("unit.mm"))),
                                    )
                                    .on_hover_text(t!("unit.mm9.tip"));
                                    if ui
                                        .button(t!("board.inspector.fill_with_vias"))
                                        .on_hover_text(t!("board.inspector.fill_with_vias.tip"))
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
                            if ui
                                .button(t!("board.inspector.delete_del"))
                                .on_hover_text(command_tip("object.delete"))
                                .clicked()
                            {
                                self.delete_selection();
                            }
                            ui.separator();
                        } else {
                            ui.heading(t!("board.inspector.nothing_selected"));
                            ui.label(t!("board.inspector.select_an_item_on_the"));
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
            Tool::Select => t!("board.footer.pick").to_owned(),
            Tool::Place(kind) | Tool::LibraryPlace(_, kind, _) | Tool::CustomPlace(kind, _) => {
                t!("board.footer.place_kind", kind = kind)
            }
            Tool::Wire(_) => t!("board.footer.draw_wire").to_owned(),
            Tool::Route(_) => t!("board.footer.route").to_owned(),
            Tool::Move => t!("board.footer.move").to_owned(),
            Tool::Cutout => t!("board.footer.cut_hole").to_owned(),
            Tool::Hole => t!("board.footer.drill_hole").to_owned(),
            Tool::NoConnect => t!("board.footer.no_connection").to_owned(),
            Tool::Fiducial => t!("board.footer.fiducial_mark").to_owned(),
            Tool::DrawLayer => t!("board.footer.draw_on_a_layer").to_owned(),
            Tool::Via => "Place via".to_owned(),
            Tool::Measure => t!("board.footer.measure").to_owned(),
        };
        egui::Panel::bottom("status")
            .frame(
                egui::Frame::side_top_panel(ui.style())
                    .inner_margin(egui::Margin::symmetric(16, 4)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let status = ui.label(
                        egui::RichText::new(&self.message)
                            .small()
                            .color(self.muted()),
                    );
                    a11y::announce_status(ui.ctx(), &status, &self.message);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new(t!(
                                "board.footer.rev_revision",
                                revision = self.revision
                            ))
                            .small()
                            .color(self.muted()),
                        )
                        .on_hover_text(t!("board.footer.rev_revision.tip"));
                        if let Some(reading) = self.measure_readout() {
                            ui.label(egui::RichText::new(reading).small().strong())
                                .on_hover_text(t!(
                                    "board.footer.the_last_measurement_press_esc.tip"
                                ));
                        }
                        if let Some(p) = self.cursor {
                            ui.label(
                                egui::RichText::new(t!(
                                    "board.footer.x_y_mm",
                                    x = format!("{:.2}", p.x),
                                    y = format!("{:.2}", p.y)
                                ))
                                .small()
                                .color(self.muted()),
                            )
                            .on_hover_text(t!("board.footer.x_y_mm.tip"));
                        }
                        ui.label(
                            egui::RichText::new(t!("board.footer.zoom_pct", zoom_pct = zoom_pct))
                                .small()
                                .color(self.muted()),
                        )
                        .on_hover_text(t!("board.footer.zoom_pct.tip"));
                        let tool_btn = ui
                            .small_button(t!(
                                "board.footer.tool_tool_words",
                                tool_words = tool_words
                            ))
                            .on_hover_text(t!("board.footer.tool_tool_words.tip"));
                        if tool_btn.clicked() {
                            self.tool = Tool::Select;
                        }
                        let fix_btn = ui
                            .small_button(t!("board.footer.fix_to_fix", fix = fix))
                            .on_hover_text(t!("board.footer.fix_to_fix.tip"));
                        let mind_btn = ui
                            .small_button(t!("board.footer.mind_to_mind", mind = mind))
                            .on_hover_text(t!("board.footer.mind_to_mind.tip"));
                        if fix_btn.clicked() || mind_btn.clicked() {
                            self.change_tab(Tab::Checks);
                        }
                    });
                });
            });
        if self.command_open {
            egui::Panel::bottom("commands")
                .frame(Self::dock_frame(ui.style()))
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
                            egui::RichText::new(t!("board.footer.search"))
                                .small()
                                .color(self.accent()),
                        );
                        let search = ui.add(
                            egui::TextEdit::singleline(&mut self.command_query)
                                .hint_text(t!("board.footer.filter_command_names_and_descriptions"))
                                .desired_width(300.0),
                        );
                        if self.command_focus {
                            search.request_focus();
                            self.command_focus = false;
                        }
                        ui.label(
                            egui::RichText::new(t!("board.footer.parameters"))
                                .small()
                                .color(self.accent()),
                        );
                        let fields = ui
                            .add(
                                egui::TextEdit::multiline(&mut self.params)
                                    .hint_text(t!("board.footer.parameters_as_json_one_line"))
                                    .desired_width(380.0)
                                    .desired_rows(3)
                                    .interactive(true),
                            )
                            .on_hover_text(t!("board.footer.parameters_as_json_one_line.tip"));
                        if ui
                            .button(t!("board.footer.execute"))
                            .on_hover_text(t!("board.footer.execute.tip"))
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
                        egui::RichText::new(t!(
                            "board.footer.len_of_len2_commands_match",
                            len = matches.len(),
                            len2 = pcbcraft_engine::commands().len()
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
                                    ui.label(egui::RichText::new(line).small().color(self.muted()));
                                }
                            }
                            for (row, spec) in matches.iter().enumerate() {
                                let row_text = t!(
                                    "board.footer.id_description",
                                    id = spec.id,
                                    description = spec.description
                                );
                                let on_row = self.command == spec.id
                                    || row == step_index(self.command_index, matches.len(), 0);
                                let response = ui
                                    .selectable_label(on_row, row_text)
                                    .on_hover_text(t!("board.footer.id_description.tip"));
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
            egui::Window::new(t!("board.footer.keyboard_shortcuts"))
                .open(&mut self.shortcuts_open)
                .show(ui, |ui| {
                    egui::Grid::new("shortcut-list")
                        .num_columns(3)
                        .spacing([16.0, 4.0])
                        .show(ui, |ui| {
                            for (keys, action, context) in
                                self.preferences.active_keymap().rows_for_help()
                            {
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
        ui.collapsing(t!("library.library.original_starter_symbols"), |ui| {
            for t in TEMPLATES {
                egui::Frame::group(ui.style())
                    .inner_margin(18.0)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.label(egui::RichText::new(t.label).size(18.0).strong());
                                ui.label(t!(
                                    "library.library.prefix_value_2_pins",
                                    prefix = t.prefix,
                                    value = t.value
                                ));
                            });
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui
                                        .button(t!("library.library.place_in_schematic"))
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
        ui.heading(t!("checks.checks_view.design_checks"));
        ui.label(t!("checks.checks_view.select_a_finding_to_inspect"));
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
                t!("checks.checks_view.electrical"),
                t!("checks.checks_view.whether_the_wiring_makes_electrical"),
            ),
            (
                "drc",
                t!("checks.checks_view.board"),
                t!("checks.checks_view.whether_the_copper_can_actually"),
            ),
        ] {
            ui.label(egui::RichText::new(label).size(18.0).strong());
            ui.label(egui::RichText::new(blurb).small().color(self.muted()));
            if let Some(findings) = checks.get(key).and_then(Value::as_array) {
                if findings.is_empty() {
                    ui.label(
                        egui::RichText::new(t!(
                            "checks.checks_view.no_findings_in_the_implemented"
                        ))
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
                        format!("{} ", t!("checks.checks_view.a_check_you_wrote"))
                    } else {
                        format!("{} ", t!("checks.checks_view.the_board_is_checked_against"))
                    };
                    ui.label(title_text).on_hover_text(t!(
                        "checks.checks_view.whose_who_the_list_below.tip",
                        whose = whose,
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
                                .on_hover_text(t!(
                                    "checks.checks_view.select_this_on_the_drawing.tip"
                                ))
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
                                    .small_button(t!("checks.checks_view.hide_this"))
                                    .on_hover_text(t!("checks.checks_view.hide_this.tip"))
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
                                        .small_button(t!("checks.checks_view.quiet_this_area"))
                                        .on_hover_text(t!("checks.checks_view.quiet_this_area.tip"))
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
        self.library_checks_section(ui);
        let silenced: Vec<(String, usize)> = self
            .project
            .rule_exclusions
            .iter()
            .map(|(code, ids)| (code.clone(), ids.len()))
            .collect();
        if !silenced.is_empty() {
            ui.label(
                egui::RichText::new(t!("checks.checks_view.things_you_asked_not_to"))
                    .small()
                    .color(self.muted()),
            );
            for (code, count) in silenced {
                ui.horizontal(|ui| {
                    ui.label(t!(
                        "checks.checks_view.code_count_hidden",
                        code = code,
                        count = count
                    ));
                    if ui
                        .small_button(t!("checks.checks_view.bring_back_code", code = code))
                        .on_hover_text(t!("checks.checks_view.bring_back_code.tip"))
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
                egui::RichText::new(t!("checks.checks_view.quiet_zones_checks_that_leave"))
                    .small()
                    .color(self.muted()),
            );
            for (code, count) in zones {
                ui.horizontal(|ui| {
                    ui.label(t!(
                        "checks.checks_view.code_count_zone_s",
                        code = code,
                        count = count
                    ));
                    if ui
                        .small_button(t!("checks.checks_view.clear_code_zones", code = code))
                        .on_hover_text(t!("checks.checks_view.clear_code_zones.tip"))
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
                    self.message = t!("status.capture.capture_failed_e", e = e);
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
                self.message = t!("status.copy_plot_to_clipboard.plot_copied_to_the_clipboard");
            }
            None => self.message = t!("status.copy_plot_to_clipboard.copy_failed_no_plot_to"),
        }
    }

    fn record_icon(&mut self, _id: &str, _rect: Rect) {
        #[cfg(test)]
        self.icon_rects.insert(_id.to_owned(), _rect);
    }

    fn tool_strip_button(
        &mut self,
        ui: &mut egui::Ui,
        id: &str,
        label: String,
        tip: String,
        selected: bool,
    ) -> egui::Response {
        let response = ui
            .selectable_label(selected, label.clone())
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
                egui::RichText::new(t!("board.draw_tool_options.draw_on_layer"))
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
                .unwrap_or_else(|| t!("board.draw_tool_options.no_layer_yet"));
            egui::ComboBox::from_id_salt("art-layer-pick")
                .selected_text(current)
                .width(150.0)
                .show_ui(ui, |ui| {
                    for (id, name) in &names {
                        ui.radio_value(&mut self.art_layer, Some(*id), name.as_str())
                            .on_hover_text(t!(
                                "board.draw_tool_options.new_figures_land_on_this.tip"
                            ));
                    }
                });
            if ui
                .button(t!("board.draw_tool_options.new_layer"))
                .on_hover_text(t!("board.draw_tool_options.new_layer.tip"))
                .clicked()
            {
                let name = t!("board.draw_tool_options.art_v0", v0 = names.len() + 1);
                self.run("board.layer_add", json!({"name": name, "kind": "other"}));
                if self.message.starts_with("board.layer_add") {
                    self.art_layer = self.selected;
                }
            }
            ui.horizontal(|ui| {
                for (shape, label, tip) in [
                    (
                        DrawShape::Line,
                        t!("board.draw_tool_options.line"),
                        t!("board.draw_tool_options.two_clicks_start_and_end"),
                    ),
                    (
                        DrawShape::Rect,
                        t!("board.draw_tool_options.box"),
                        t!("board.draw_tool_options.two_clicks_on_opposite_corners"),
                    ),
                    (
                        DrawShape::Circle,
                        t!("board.draw_tool_options.circle"),
                        t!("board.draw_tool_options.centre_then_edge"),
                    ),
                    (
                        DrawShape::Poly,
                        t!("board.draw_tool_options.outline"),
                        t!("board.draw_tool_options.corners_enter_to_close"),
                    ),
                    (
                        DrawShape::Words,
                        t!("board.draw_tool_options.words"),
                        t!("board.draw_tool_options.type_below_then_click_once"),
                    ),
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
                        .hint_text(t!("board.draw_tool_options.words_to_print"))
                        .desired_width(f32::INFINITY),
                )
                .on_hover_text(t!("board.draw_tool_options.words_to_print.tip"));
            }
            ui.separator();
        }
        if matches!(self.tool, Tool::Hole) {
            ui.label(t!("board.draw_tool_options.mounting_hole_size"));
            ui.add(
                egui::DragValue::new(&mut self.hole_diameter)
                    .speed(0.05)
                    .range(0.05..=25.0)
                    .suffix(format!(" {}", t!("unit.mm"))),
            )
            .on_hover_text(t!("unit.mm5.tip"));
            ui.separator();
        }
        if matches!(self.tool, Tool::Fiducial) {
            ui.label(t!("board.draw_tool_options.assembly_mark_size"));
            ui.add(
                egui::DragValue::new(&mut self.fiducial_diameter)
                    .speed(0.05)
                    .range(0.2..=10.0)
                    .suffix(format!(" {}", t!("unit.mm"))),
            )
            .on_hover_text(t!("unit.mm6.tip"));
            ui.separator();
        }
    }

    fn tool_strip(&mut self, ui: &mut egui::Ui) {
        if !matches!(self.tab, Tab::Schematic | Tab::Pcb) {
            return;
        }
        ui.horizontal_wrapped(|ui| {
            ui.add_space(8.0);
            let select = |app: &mut App| {
                app.tool = Tool::Select;
            };
            if self
                .tool_strip_button(
                    ui,
                    "tool-select",
                    t!("board.tool_strip.select"),
                    t!("board.tool_strip.select_objects_on_the_drawing"),
                    matches!(self.tool, Tool::Select),
                )
                .clicked()
            {
                select(self);
            }
            if self
                .tool_strip_button(
                    ui,
                    "tool-move",
                    t!("board.tool_strip.move"),
                    t!("board.tool_strip.move_tool_drop_the_selection"),
                    matches!(self.tool, Tool::Move),
                )
                .clicked()
            {
                self.tool = Tool::Move;
            }
            if self.tab == Tab::Schematic {
                if self
                    .tool_strip_button(
                        ui,
                        "tool-wire",
                        t!("board.tool_strip.wire"),
                        t!("board.tool_strip.wire_tool_click_two_pins"),
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
                        t!("board.tool_strip.no_connect"),
                        t!("board.tool_strip.mark_a_pin_deliberately_unconnected"),
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
                        t!("board.tool_strip.route"),
                        t!("board.tool_strip.route_tool_click_two_pads"),
                        matches!(self.tool, Tool::Route(_)),
                    )
                    .clicked()
                {
                    self.tool = Tool::Route(None);
                }
                for (shape, id, glyph, tip) in [
                    (
                        DrawShape::Line,
                        "tool-line",
                        t!("board.tool_strip.line"),
                        t!("board.tool_strip.draw_a_straight_line_on"),
                    ),
                    (
                        DrawShape::Rect,
                        "tool-box",
                        t!("board.tool_strip.box"),
                        t!("board.tool_strip.draw_a_box_on_a"),
                    ),
                    (
                        DrawShape::Circle,
                        "tool-circle",
                        t!("board.tool_strip.circle"),
                        t!("board.tool_strip.draw_a_circle_on_a"),
                    ),
                    (
                        DrawShape::Poly,
                        "tool-outline",
                        t!("board.tool_strip.outline"),
                        t!("board.tool_strip.draw_an_outline_on_a"),
                    ),
                    (
                        DrawShape::Words,
                        "tool-words",
                        t!("board.tool_strip.text"),
                        t!("board.tool_strip.print_words_on_a_user"),
                    ),
                ] {
                    let armed = matches!(self.tool, Tool::DrawLayer) && self.art_shape == shape;
                    if self.tool_strip_button(ui, id, glyph, tip, armed).clicked() {
                        self.tool = Tool::DrawLayer;
                        self.art_shape = shape;
                        self.art_start = None;
                        self.art_points.clear();
                    }
                }
                if ui
                    .selectable_label(
                        matches!(self.tool, Tool::DrawLayer),
                        t!("board.tool_strip.draw_shape"),
                    )
                    .on_hover_text(t!("board.tool_strip.draw_shape.tip"))
                    .clicked()
                {
                    self.tool = Tool::DrawLayer;
                    self.art_shape = DrawShape::Line;
                    self.art_start = None;
                    self.art_points.clear();
                }
                if ui
                    .selectable_label(
                        matches!(self.tool, Tool::Cutout),
                        t!("board.tool_strip.cutout"),
                    )
                    .on_hover_text(command_tip("board.cutout.add"))
                    .clicked()
                {
                    self.tool = Tool::Cutout;
                }
                if ui
                    .selectable_label(
                        matches!(self.tool, Tool::Hole),
                        t!("board.tool_strip.mounting_hole"),
                    )
                    .on_hover_text(t!("board.tool_strip.mounting_hole.tip"))
                    .clicked()
                {
                    self.tool = Tool::Hole;
                }
                if ui
                    .selectable_label(
                        matches!(self.tool, Tool::Fiducial),
                        t!("board.tool_strip.assembly_mark"),
                    )
                    .on_hover_text(t!("board.tool_strip.assembly_mark.tip"))
                    .clicked()
                {
                    self.tool = Tool::Fiducial;
                }
            }
            ui.separator();
            self.editing_controls(ui);
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
            ui.heading(t!("library.files_tab.project_files"));
            ui.label(t!(
                "library.files_tab.design_file_trim",
                trim = self.path.trim()
            ));
            ui.add_space(8.0);
            if ui
                .button(t!("library.files_tab.library_sources"))
                .on_hover_text(t!("library.files_tab.library_sources.tip"))
                .clicked()
            {
                self.change_tab(Tab::Libraries);
            }
            ui.add_space(12.0);
            ui.label(
                egui::RichText::new(t!("library.files_tab.builds"))
                    .size(10.0)
                    .color(self.muted()),
            );
            self.builds_tab(ui);
        });
    }
    fn board3d_tab(&mut self, ui: &mut egui::Ui) {
        let env = board3d::Env {
            project: &self.project,
            revision: self.revision,
            selected: self.selected,
            dark: self.preferences.dark,
            bg: self.surface(),
            muted: self.muted(),
        };
        if let Some(id) = board3d::show(&mut self.view3d, ui, &env) {
            self.select(Some(id));
        }
    }

    fn show(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        pcbcraft_text::activate(&self.catalog);
        #[cfg(test)]
        self.icon_rects.clear();
        self.refresh();
        a11y::sync_ui_scale(&ctx, &mut self.preferences.ui_scale);
        self.assistant_frame(&ctx);
        self.style(&ctx);
        self.shortcuts(&ctx);
        self.edit_events(&ctx);
        self.header(ui);
        self.footer(ui);
        if self.compare_open {
            self.compare_window(&ctx);
        }
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
                                .show(ui, |ui| Self::padded(ui, |ui| self.files_tab(ui)))
                                .inner
                        }
                        Tab::Board3d => self.board3d_tab(ui),
                        Tab::Libraries => {
                            egui::ScrollArea::vertical()
                                .show(ui, |ui| Self::padded(ui, |ui| self.library(ui)))
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
                        Tab::PlotFiles => {
                            egui::ScrollArea::vertical().show(ui, |ui| {
                                Self::margined(ui, |ui| {
                                    let theme = plot_viewer::Theme {
                                        surface: self.surface(),
                                        muted: self.muted(),
                                        accent: self.accent(),
                                    };
                                    plot_viewer::ui(
                                        &mut self.plot_viewer,
                                        ui,
                                        theme,
                                        &mut self.message,
                                        self.make_dir.trim(),
                                    );
                                });
                            });
                        }
                    }
                });
        }
        a11y::paint_focus_ring(&ctx, self.accent());
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
        self.assistant_save();
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
fn pcb_screen(app: &App, world: Point) -> egui::Pos2 {
    app.pcb.screen(world, app.canvas_rect)
}

#[cfg(test)]
mod tests {
    use super::e2e::support::*;
    use super::*;
    use crate::authoring::tests::tap;

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
        tap(&ctx, &mut app, "Add folder")?;
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
        tap(&ctx, &mut app, "Remove source")?;
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
        tap(&ctx, &mut app, "File")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert!(
            !paints_text(&ctx, &mut app, "Load LED example")?,
            "the example lives on the start screen, not in the menu"
        );
        tap(&ctx, &mut app, "Save")?;
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
        tap(&ctx, &mut app, "File")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        tap(&ctx, &mut app, "New")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        assert!(
            app.project.components.is_empty(),
            "New empties the design: {} parts left",
            app.project.components.len()
        );
        tap(&ctx, &mut app, "Edit")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        tap(&ctx, &mut app, "Undo")?;
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
        tap(&ctx, &mut app, "Settings")?;
        assert!(
            scrolled_to_at(&ctx, &mut app, "BOARD EDGE", 700.0)?,
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
        tap(&ctx, &mut app, "Import as board edge")?;
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
        tap(&ctx, &mut app, "Back to a plain rectangle")?;
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
        tap(&ctx, &mut app, "Layers")?;
        assert!(
            scrolled_to(&ctx, &mut app, "Wires")?,
            "the layers page lists wires as its own row"
        );
        tap(&ctx, &mut app, "Wires")?;
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
        let fit = app
            .icon_rects
            .get("canvas-fit")
            .copied()
            .ok_or("a floating Fit icon sits in the work area")?;
        assert!(
            fit.max.x <= 1440.0 - 280.0 && fit.min.y > 60.0,
            "the Fit icon stays inside the canvas, left of the right aside: {fit:?}"
        );
        let first = app.project.components[0].id;
        let before = app.project.components[0].rotation;
        app.select(Some(first));
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        let handle = app
            .icon_rects
            .get("handle-rotate")
            .copied()
            .ok_or("a selected part grows a Rotate handle beside it")?;
        assert!(
            handle.max.x <= 1440.0 - 280.0,
            "the handle stays inside the canvas: {handle:?}"
        );
        authoring::tests::tap_icon(&ctx, &mut app, "handle-rotate")?;
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
        tap(&ctx, &mut app, "Layers")?;
        assert!(
            scrolled_to(&ctx, &mut app, "New net")?,
            "the layers tree offers a way to add a connection"
        );
        tap(&ctx, &mut app, "New net")?;
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
        tap(&ctx, &mut app, "Use name")?;
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
        tap(&ctx, &mut app, "Settings")?;
        assert!(
            scrolled_to_at(&ctx, &mut app, "MAKE FILES", 700.0)?,
            "the settings page should offer the factory files"
        );
        tap(&ctx, &mut app, "Copper & drill files")?;
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
        tap(&ctx, &mut app, "Parts list (CSV)")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        tap(&ctx, &mut app, "Net list")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        tap(&ctx, &mut app, "Pick and place file")?;
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        tap(&ctx, &mut app, "Design report")?;
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
        tap(&ctx, &mut app, "Worked example")?;
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
        tap(&ctx, &mut app, &want)?;
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
    fn an_engine_refusal_shows_in_the_language_that_names_it() {
        let english = std::sync::Arc::new(pcbcraft_text::Catalog::english());
        pcbcraft_text::activate(&english);
        assert_eq!(
            refusal_key("Nothing to undo!"),
            "error.nothing_to_undo",
            "the key is the refusal's words"
        );
        assert_eq!(
            refusal_text("Nothing to undo!"),
            "Nothing to undo!",
            "other spelling keeps the engine text"
        );
        assert_eq!(
            refusal_text("nothing to undo"),
            "nothing to undo",
            "a refusal English lists shows its own words in English"
        );
        let spanish = std::sync::Arc::new(
            pcbcraft_text::Catalog::english()
                .with_language("zz", "error.nothing_to_undo = Nada que deshacer"),
        );
        pcbcraft_text::activate(&spanish);
        assert_eq!(refusal_text("nothing to undo"), "Nada que deshacer");
        assert_eq!(
            refusal_text("Another refusal"),
            "Another refusal",
            "an unmapped refusal keeps the engine words"
        );
        pcbcraft_text::activate(&english);
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
        for key in [EMPTY_SYMBOLS, EMPTY_NETS, EMPTY_BOARD] {
            let copy = pcbcraft_text::lookup(key, &[]);
            let copy = copy.as_str();
            for name in ["component.add", "netlist.import", "board.sync"] {
                if copy.contains(name) {
                    assert!(registry.contains(&name), "{name} must exist");
                }
            }
            assert!(
                copy.contains("component.add")
                    || copy.contains("netlist.import")
                    || copy.contains("automatically")
                    || copy.contains("schematic"),
                "empty guidance must point at how to fill the panel: {copy}"
            );
        }
        assert!(pcbcraft_text::lookup(EMPTY_CHECKS, &[]).contains("automatically"));
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
        assert!(pcbcraft_text::lookup(FIELD_HELP_VALUE, &[]).contains("name or value"));
        assert!(pcbcraft_text::lookup(FIELD_HELP_POSITION, &[]).contains("origin"));
        assert!(pcbcraft_text::lookup(FIELD_HELP_BOARD, &[]).contains("larger than zero"));
        assert!(pcbcraft_text::lookup(FIELD_HELP_CUTOUT, &[]).contains("inside the board"));
    }

    #[test]
    fn shortcut_list_documents_every_binding_the_app_installs() {
        let rows = shortcut_rows();
        let keys: Vec<&str> = rows.iter().map(|(keys, _, _)| keys.as_str()).collect();
        for wanted in ["Ctrl+Z", "Ctrl+K", "?", "W", "X", "Delete", "Esc"] {
            assert!(
                keys.iter().any(|k| *k == wanted),
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
                .contains(&context.as_str()),
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
            (Tool::Via, "via"),
            (Tool::Measure, "distance"),
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
        tap(&ctx, &mut app, "Layers")?;
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
