//! The unified Layers tree of the left aside: one tree for the board, its
//! layers, parts and nets (Layout) or its sheets, parts and nets (Schematic).
//!
//! Context: `lib.rs` hosts the left aside's Layers and Library tabs, builds an
//! [`Env`] from the app each frame, calls [`show`] and applies the returned
//! [`Action`]s. The tree never edits the document and never reads `App`.
//! Look: the approved mockups in `app-design/` (design-system.md section 5.7):
//! 28-point rows, 16-point indent, colour swatch, name, Active badge, count,
//! lock cell and eye cell in a right-aligned column.
//! Local rules: rows are built lazily (only expanded branches), cached by a
//! cheap signature, and drawn virtualized, so a 2,000-part board stays
//! interactive. Visibility, lock and focus are view state, not document state.
//! Every glyph is drawn by [`glyph`]; swapping in the shared icon set is a
//! change to that one function.

use crate::a11y::{Named, name_control};
use crate::draw_target::{Side, Target};
use crate::layer_visibility::{self, SubLayer};
use crate::palette::ThemeColours;
use egui::{Align2, Color32, Pos2, Rect, Sense, Shape, Stroke, Ui, Vec2, epaint};
use pcbcraft_doc::{Id, Layer, Project};
use pcbcraft_text::t;
use std::collections::BTreeSet;

/// Height of one tree row in points; also the smallest target size.
pub(crate) const ROW_HEIGHT: f32 = 28.0;
/// Horizontal step of one nesting level in points.
const INDENT: f32 = 16.0;
/// Width of the eye and lock cells.
const CELL: f32 = 24.0;
/// Side padding of the aside content.
const PAD: f32 = 8.0;

/// Which tab's tree to build.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum TreeTab {
    /// Board: copper, silkscreen, mask and paste, board, user layers, parts, nets.
    Layout,
    /// Schematic: sheets, parts, nets.
    Schematic,
}

/// What one row stands for; picks its glyph.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum RowKind {
    Section,
    Layer,
    Group,
    Leaf,
    Part,
    Pad,
    Net,
    Track,
    Via,
    Zone,
    Text,
    Hole,
    Label,
    Wire,
    Shape,
}

/// How a row's eye reaches the drawing.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) enum Visibility {
    /// A named drawing toggle of the canvas (`hidden_layers` entry).
    Canvas(&'static str),
    /// A physical layer group (`side:top`, `side:bottom`, `inner:n`); path is in the string.
    LayerGroup(String),
    /// One sub-layer row inside a group (tracks, pads, zones, silk, text).
    SubLayer {
        side: String,
        sub: SubLayer,
    },
    /// A machine layer the designer drew; the document stores its flag.
    Art(Id),
    /// A fixed row of one schematic page (`symbols`, `power`, `drawing` or
    /// `title`); the hidden list stores [`page_row_name`].
    Page(Id, &'static str),
    /// The canvas draws nothing for this row yet; the eye is inert.
    None,
}

/// Eye, lock and focus controls of a layer or group row.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct LayerControls {
    /// The lock and focus key; empty when the row has neither.
    pub key: String,
    pub visibility: Visibility,
    /// True when the canvas can dim everything not on this row (`focus_layer`).
    pub focusable: bool,
}

/// One visible line of the tree.
#[derive(Clone, PartialEq, Debug)]
pub(crate) struct Row {
    /// Path key, unique in the tree, made of `/`-joined segments.
    pub key: String,
    pub depth: u8,
    /// Visual indent level: sections do not indent their direct children.
    pub level: u8,
    pub kind: RowKind,
    pub label: String,
    /// Muted text after the label: a value, a net or a size.
    pub sub: String,
    /// Right-aligned count text.
    pub count: String,
    /// The label is drawn in the monospace face (references, net names).
    pub mono: bool,
    /// Drawn at reduced strength: empty or inert.
    pub dim: bool,
    pub expandable: bool,
    /// The document object the canvas selects for this row.
    pub select: Option<Id>,
    /// Whether the row lights up when `select` is the canvas selection.
    pub marks: bool,
    pub controls: Option<LayerControls>,
    pub swatch: Option<Color32>,
    /// Round swatch (a net colour) instead of a square one.
    pub round: bool,
    /// The net this row stands for, which the canvas highlights.
    pub net: Option<Id>,
    /// The drawing target clicking this row sets, when it is one.
    pub target: Option<Target>,
    /// The stackup record this row stands for (a copper layer or dielectric).
    pub stack: Option<String>,
    /// The schematic page this row stands for (page 0 is Main).
    pub page: Option<Id>,
    /// The fixed row of a page this row stands for: its page and name.
    pub sheet_row: Option<(Id, &'static str)>,
}

/// What the tree asks the app to do; the app applies each in `lib.rs`.
#[derive(Clone, PartialEq, Debug)]
pub(crate) enum Action {
    /// Select a document object on the canvas and in Properties.
    Select(Id),
    /// Select a row that is not one object (the board outline, a layer, a net).
    SelectKey(String),
    /// Set a canvas drawing toggle shown (true) or hidden.
    SetShown { name: &'static str, on: bool },
    /// Show or hide a board layer group.
    SetLayerGroup { side: String, on: bool },
    /// Show or hide one sub-layer row inside a group.
    SetSubLayer {
        side: String,
        sub: SubLayer,
        on: bool,
    },
    /// Hide exactly these canvas toggles of this tab's layers, show the rest.
    SetHidden(Vec<&'static str>),
    /// Set a machine layer's stored visibility.
    SetArtShown { layer: Id, on: bool },
    /// Focus (dim the rest) on this key, or clear with None.
    Focus(Option<String>),
    /// Highlight a net on the canvas, or clear with None.
    HighlightNet(Option<Id>),
    /// Make this row the drawing target (the toolbar's Layer field follows).
    SetTarget(Target),
    /// A click on a layer row: pin or unpin its see-through view.
    PinView(Target),
    /// Add two inner copper layers below the named copper layer.
    StackAddPair(String),
    /// Rename a stackup layer.
    StackRename { name: String, new_name: String },
    /// Remove an inner copper layer.
    StackRemove(String),
    /// Centre the drawing on a part.
    ZoomTo(Id),
    /// Move a part to the other board side.
    FlipSide(Id),
    /// Put a net in a net class, or in none.
    NetClass { net: Id, class: Option<Id> },
    /// Rename a net.
    NetRename { net: Id, new_name: String },
    /// Rename a machine layer.
    ArtRename { layer: Id, new_name: String },
    /// Show a schematic page on the canvas and select it.
    SelectPage(Id),
    /// Add a schematic page.
    PageAdd,
    /// Remove a schematic page; its parts move to Main.
    PageRemove(Id),
    /// Rename a schematic page.
    PageRename { id: Id, new_name: String },
    /// Move an inner copper layer up or down.
    StackMove { name: String, up: bool },
    /// Delete a document object (same as the Delete key on the drawing).
    Delete(Id),
    /// Remove a machine layer and everything drawn on it.
    RemoveArtLayer(Id),
    /// Add a machine layer.
    NewArtLayer,
    /// Open Export on the bill of materials: all parts, or only the selected ones.
    ExportBom { selection: bool },
    /// Leave a part off the bill of materials, or put it back.
    ToggleBomExclude(Id),
/// Move an inner copper layer up or down.
    StackMove { name: String, up: bool },
    /// Delete a document object (same as the Delete key on the drawing).
    Delete(Id),
    /// Remove a machine layer and everything drawn on it.
    RemoveArtLayer(Id),
    /// Add a machine layer.
    NewArtLayer,
    /// Open Export on the bill of materials: all parts, or only the selected ones.
    ExportBom { selection: bool },
    /// Leave a part off the bill of materials, or put it back.
    ToggleBomExclude(Id),
    /// Show or hide one fixed row of a page (`symbols`, `power`, `drawing`, `title`).
    SetPageRowShown {
        page: Id,
        row: &'static str,
        on: bool,
    },
    /// Open a fixed row of a page in Properties and show that page.
    SelectSheetRow { page: Id, row: &'static str },
    /// Select every object of a fixed row of a page.
    SelectAllInRow { page: Id, row: &'static str },
    /// Switch a page's title block on or off.
    TitleShown { page: Id, on: bool },
    /// Place a power port on a page, tied to the first power net.
    AddPowerPort(Id),
}
}

/// The read-only inputs of one frame.
pub(crate) struct Env<'a> {
    pub project: &'a Project,
    pub tab: TreeTab,
    pub colours: ThemeColours,
    /// The app's `hidden_layers` list.
    pub hidden: &'a [String],
    pub focus: Option<&'a str>,
    pub selected: Option<Id>,
    /// What the drawing tools draw on; its row carries the Active badge.
    pub target: Target,
    /// Bumps on every document change; part of the row cache key.
    pub revision: u64,
    /// How many parts are selected, for the Export BOM for selection entry.
    pub parts_selected: usize,
pub tab: TreeTab,
    pub colours: ThemeColours,
    /// The app's `hidden_layers` list.
    pub hidden: &'a [String],
    pub focus: Option<&'a str>,
    pub selected: Option<Id>,
    /// What the drawing tools draw on; its row carries the Active badge.
    pub target: Target,
    /// Bumps on every document change; part of the row cache key.
    pub revision: u64,
    /// How many parts are selected, for the Export BOM for selection entry.
    pub parts_selected: usize,
    /// Show the fixed rows of a page even when they hold nothing.
    pub show_empty: bool,
}
}

/// The canvas toggles each tab's layer rows may switch.
fn layer_names(tab: TreeTab) -> &'static [&'static str] {
    match tab {
        TreeTab::Layout => &[
            "Top copper",
            "Bottom copper",
            "Package artwork",
            "Cutouts",
            "Pads",
            "Vias",
            "Copper zones",
        ],
        TreeTab::Schematic => &["Wires", "Symbols", "Net labels", "Graphics"],
    }
}

/// What a rename in progress renames.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) enum RenameFor {
    /// A stackup layer, by its current name.
    Layer(String),
    /// A net.
    Net(Id),
    /// A machine layer the designer drew.
    Art(Id),
    /// A schematic page (never page 0, the Main page).
    Page(Id),
}

/// View state owned by the app: what is open, filtered, locked and focused.
///
/// # Rules
/// - Never part of the document or the saved preferences.
///
/// # Do not
/// - Store document ids that outlive a frame; rows are rebuilt from the project.
#[derive(Clone, Debug)]
pub(crate) struct TreeView {
    pub expanded: BTreeSet<String>,
    pub query: String,
    /// Layer keys the canvas may not select from or edit.
    pub locked: BTreeSet<String>,
    /// Show the fixed rows of a page (Symbols, Power ports, Drawing, Title
    /// block) even when they hold nothing.
    pub show_empty: bool,
    /// Keyboard cursor row key.
    pub cursor: Option<String>,
    /// A row selected that is not one object: the board outline, a layer, a net.
    pub selected_key: Option<String>,
    /// The layer or sublayer under the pointer this frame, for the see-through preview.
    pub hover_target: Option<Target>,
    /// Key of the row under the pointer this frame.
    hovered_row: Option<String>,
    /// A rename in progress (F2 or a row menu): what it renames and the text
    /// so far. The first row for that thing draws the editor in place of its label.
    pub rename: Option<(RenameFor, String)>,
    /// The page row the pointer rests on, as of the last frame (Properties previews it).
    pub hover_page: Option<Id>,
    hover_now: Option<Id>,
    editing_drawn: bool,
    last_selected: Option<Id>,
    scroll_to: Option<String>,
    focus_pending: Option<String>,
    has_focus: bool,
    offset: f32,
    viewport: f32,
    cache: Option<(u64, std::rc::Rc<Vec<Row>>)>,
}

impl TreeView {
    #[cfg(test)]
    /// Put the keyboard cursor and focus on the row with this key, as if it
    /// had been reached with the arrow keys.
    pub(crate) fn focus_row(&mut self, key: &str) {
        self.cursor = Some(key.to_owned());
        self.focus_pending = Some(key.to_owned());
        self.scroll_to = Some(key.to_owned());
    }
}

impl Default for TreeView {
    fn default() -> Self {
        let expanded = [
            "side:top",
            "side:top/copper",
            "side:bottom",
            "drills",
            "edge",
            "components",
            "nets",
            "sec:sheet:0",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        Self {
            expanded,
            query: String::new(),
            show_empty: false,
            locked: BTreeSet::new(),
            cursor: None,
            selected_key: None,
            hover_target: None,
            hovered_row: None,
            rename: None,
            hover_page: None,
            hover_now: None,
            editing_drawn: false,
            last_selected: None,
            scroll_to: None,
            focus_pending: None,
            has_focus: false,
            offset: 0.0,
            viewport: 400.0,
            cache: None,
        }
    }
}

fn side_label(layer: Layer) -> String {
    if layer == Layer::Front {
        t!("tree.side.top")
    } else {
        t!("tree.side.bottom")
    }
}

fn copper_key(layer: Layer) -> &'static str {
    if layer == Layer::Front {
        "copper:front"
    } else {
        "copper:back"
    }
}

fn segment(path: &str, seg: &str) -> String {
    if path.is_empty() {
        seg.to_owned()
    } else {
        format!("{path}/{seg}")
    }
}

fn row(path: &str, seg: &str, kind: RowKind, label: String) -> Row {
    Row {
        key: segment(path, seg),
        depth: 0,
        level: 0,
        kind,
        label,
        sub: String::new(),
        count: String::new(),
        mono: false,
        dim: false,
        expandable: false,
        select: None,
        marks: true,
        controls: None,
        swatch: None,
        round: false,
        net: None,
        target: None,
        stack: None,
        page: None,
        sheet_row: None,
    }
}

/// A counted row: dimmed and not expandable when the count is zero.
fn counted(mut r: Row, n: usize, expandable: bool) -> Row {
    r.count = n.to_string();
    r.dim = n == 0;
    r.expandable = expandable && n > 0;
    r
}

fn reference_of(project: &Project, footprint: &pcbcraft_doc::Footprint) -> String {
    project
        .component(footprint.component)
        .map_or_else(|| t!("tree.part.unknown"), |c| c.reference.clone())
}

/// A stable colour for a net, from the canvas set.
fn net_colour(colours: &ThemeColours, id: Id) -> Color32 {
    let set = [
        colours.canvas.front,
        colours.canvas.back,
        colours.canvas.wire,
        colours.warn,
        colours.link,
    ];
    set[(id % set.len() as u64) as usize]
}

fn section(key: &str, label: String, n: usize) -> Row {
    let mut r = row("", key, RowKind::Section, label);
    r.expandable = true;
    r.count = n.to_string();
    r
}

#[allow(clippy::too_many_arguments)]
fn layer_row(
    path: &str,
    key: &str,
    label: String,
    swatch: Color32,
    vis: Visibility,
    focusable: bool,
    n: usize,
    expandable: bool,
) -> Row {
    let mut r = counted(
        row(path, &format!("layer:{key}"), RowKind::Layer, label),
        n,
        expandable,
    );
    // A layer with nothing on it still opens when it has fixed children.
    r.expandable = expandable;
    r.dim = n == 0 && vis == Visibility::None;
    r.swatch = Some(swatch);
    r.controls = Some(LayerControls {
        key: key.to_owned(),
        visibility: vis,
        focusable,
    });
    r
}

fn net_rows(env: &Env, path: &str, unit_key: &str) -> Vec<Row> {
    let project = env.project;
    let mut nets: Vec<&pcbcraft_doc::Net> = project.nets.iter().collect();
    nets.sort_by(|a, b| a.name.cmp(&b.name));
    nets.into_iter()
        .map(|n| {
            let count = if env.tab == TreeTab::Layout {
                project
                    .board
                    .footprints
                    .iter()
                    .flat_map(|f| &f.pads)
                    .filter(|p| p.net == Some(n.id))
                    .count()
            } else {
                project
                    .components
                    .iter()
                    .flat_map(|c| &c.pins)
                    .filter(|p| p.net == Some(n.id))
                    .count()
            };
            let mut r = row(path, &format!("net:{}", n.id), RowKind::Net, n.name.clone());
            r.count = if unit_key == "pads" {
                t!("tree.pads", n = count)
            } else {
                t!("tree.pins", n = count)
            };
            r.swatch = Some(net_colour(&env.colours, n.id));
            r.round = true;
            r.net = Some(n.id);
            r.expandable = true;
            r
        })
        .collect()
}

/// One entry of the physical stack, top to bottom.
enum Stack {
    /// A copper layer: its position (0 is the top) and its stackup name.
    Copper(usize, String),
    /// A core or prepreg layer between coppers, with its thickness in mm.
    Dielectric(usize, String, f64),
}

/// The physical stack from the stackup records. Without records the board is
/// the usual two copper layers.
fn stack(env: &Env) -> Vec<Stack> {
    let mut out = Vec::new();
    let mut coppers = 0;
    for (at, rec) in env.project.board.layer_records.iter().enumerate() {
        match rec.kind.as_str() {
            "copper" => {
                out.push(Stack::Copper(coppers, rec.name.clone()));
                coppers += 1;
            }
            "core" | "prepreg" => {
                out.push(Stack::Dielectric(at, rec.name.clone(), rec.thickness));
            }
            _ => {}
        }
    }
    if coppers == 0 {
        return vec![
            Stack::Copper(0, "F.Cu".to_owned()),
            Stack::Copper(1, "B.Cu".to_owned()),
        ];
    }
    out
}

fn copper_count(env: &Env) -> usize {
    stack(env)
        .iter()
        .filter(|s| matches!(s, Stack::Copper(..)))
        .count()
}

/// The two-way name of a net's copper on the outer side `side`: "L1" for the
/// top, "L{n}" for the bottom.
fn level(env: &Env, side: Layer) -> usize {
    if side == Layer::Front {
        1
    } else {
        copper_count(env).max(2)
    }
}

fn side_key(side: Layer) -> &'static str {
    if side == Layer::Front {
        "side:top"
    } else {
        "side:bottom"
    }
}

/// The bottom copper layer of this board (index 1 on a two-layer board).
fn bottom(env: &Env) -> Layer {
    Layer::from_index(copper_count(env).max(2) as u8 - 1).unwrap_or(Layer::Back)
}

/// The copper layer a top-level layer key (`side:top`, `inner:2`,
/// `side:bottom`) stands for.
fn path_layer(env: &Env, seg: &str) -> Layer {
    if let Some(n) = seg.strip_prefix("inner:") {
        return Layer::from_index(n.parse().unwrap_or(1)).unwrap_or(Layer::Front);
    }
    if seg.starts_with("side:bottom") {
        bottom(env)
    } else {
        Layer::Front
    }
}

/// A sublayer row: it carries the controls and is an active drawing target.
#[allow(clippy::too_many_arguments)]
fn sub_row(
    path: &str,
    seg: &str,
    ctl_key: &str,
    label: String,
    swatch: Color32,
    vis: Visibility,
    focusable: bool,
    n: usize,
    expandable: bool,
) -> Row {
    let mut r = layer_row(path, ctl_key, label, swatch, vis, focusable, n, expandable);
    r.key = segment(path, seg);
    r
}

fn top_level(env: &Env) -> Vec<Row> {
    let c = env.colours;
    let board = &env.project.board;
    let total = copper_count(env).max(2);
    let mut out = Vec::new();
    let mut root = row("", "root", RowKind::Layer, t!("tree.layer.board"));
    root.swatch = Some(c.warn);
    root.sub = t!(
        "tree.board_size",
        w = size_text(board.width),
        h = size_text(board.height)
    );
    out.push(root);
    for item in stack(env) {
        match item {
            Stack::Copper(i, name) => {
                let (key, label, sw) = if i == 0 {
                    (
                        "side:top".to_owned(),
                        t!("tree.layer.top", n = 1),
                        c.canvas.front,
                    )
                } else if i + 1 >= total {
                    (
                        "side:bottom".to_owned(),
                        t!("tree.layer.bottom", n = i + 1),
                        c.canvas.back,
                    )
                } else {
                    (
                        format!("inner:{i}"),
                        t!("tree.layer.inner", n = i + 1, name = name),
                        c.canvas.front.gamma_multiply(0.6),
                    )
                };
                let (ctl_key, focusable) = if i == 0 {
                    ("copper:front".to_owned(), true)
                } else if i + 1 >= total {
                    ("copper:back".to_owned(), true)
                } else {
                    (format!("copper:inner{i}"), false)
                };
                let mut r = row("", &key, RowKind::Layer, label);
                r.swatch = Some(sw);
                r.expandable = true;
                r.stack = Some(name);
                r.controls = Some(LayerControls {
                    key: ctl_key,
                    visibility: Visibility::LayerGroup(key.clone()),
                    focusable,
                });
                out.push(r);
            }
            Stack::Dielectric(at, name, thickness) => {
                let mut r = row("", &format!("diel:{at}"), RowKind::Leaf, name.clone());
                r.stack = Some(name);
                r.sub = format!("{thickness:.2} mm");
                r.dim = true;
                out.push(r);
            }
        }
    }
    let vias = board.vias.len();
    let holes = board.holes.len();
    let mut drills = counted(
        row("", "drills", RowKind::Layer, t!("tree.layer.drills")),
        vias + holes,
        true,
    );
    drills.swatch = Some(c.link);
    drills.controls = Some(LayerControls {
        key: String::new(),
        visibility: Visibility::Canvas("Vias"),
        focusable: false,
    });
    out.push(drills);
    let mut edge = counted(
        row("", "edge", RowKind::Layer, t!("tree.layer.edge")),
        1 + board.cutouts.len(),
        true,
    );
    edge.expandable = true;
    edge.target = Some(Target::Edge);
    edge.swatch = Some(c.warn);
    edge.controls = Some(LayerControls {
        key: "outline".to_owned(),
        visibility: Visibility::Canvas("Cutouts"),
        focusable: false,
    });
    out.push(edge);
    let mut user = counted(
        row("", "user", RowKind::Layer, t!("tree.layer.user")),
        board.user_layers.len(),
        true,
    );
    user.swatch = Some(c.link);
    out.push(user);
    let mut comps = counted(
        row("", "components", RowKind::Layer, t!("tree.section.parts")),
        if env.tab == TreeTab::Layout {
            board.footprints.len()
        } else {
            env.project.components.len()
        },
        true,
    );
    comps.swatch = Some(c.ink);
    out.push(comps);
    let mut nets = counted(
        row("", "nets", RowKind::Layer, t!("tree.section.nets")),
        env.project.nets.len(),
        true,
    );
    nets.swatch = Some(c.canvas.wire);
    out.push(nets);
    out
}

/// True when the part is soldered on the top (`front`) or bottom side.
fn on_side(f: &pcbcraft_doc::Footprint, front: bool) -> bool {
    f.side.is_none_or(|l| l == Layer::Front) == front
}

fn footprint_row(
    project: &Project,
    path: &str,
    f: &pcbcraft_doc::Footprint,
    (top, bottom): (&str, &str),
) -> Row {
    let side = if on_side(f, true) { top } else { bottom };
    let mut r = row(
        path,
        &format!("fp:{}", f.id),
        RowKind::Part,
        reference_of(project, f),
    );
    let value = project
        .component(f.component)
        .map(|c| c.value.clone())
        .unwrap_or_default();
    r.sub = format!("{value} {side}");
    r.count = f.pads.len().to_string();
    r.mono = true;
    r.select = Some(f.id);
    r.expandable = !f.pads.is_empty();
    r
}

/// Tracks, zones and pads of net `id` on one copper side, as rows.
fn net_copper(env: &Env, path: &str, id: Id, side: Layer, out: &mut Vec<Row>) {
    let project = env.project;
    let board = &project.board;
    for t in board
        .tracks
        .iter()
        .filter(|t| t.layer == side && t.net == id)
    {
        let mut r = track_row(path, t);
        r.net = Some(id);
        out.push(r);
    }
    for z in board
        .zones
        .iter()
        .filter(|z| z.layer == side && z.net == id)
    {
        let mut r = row(
            path,
            &format!("zone:{}", z.id),
            RowKind::Zone,
            t!("tree.zone", net = project.net_name(z.net)),
        );
        r.select = Some(z.id);
        r.net = Some(id);
        out.push(r);
    }
    if side == Layer::Front {
        for f in &board.footprints {
            for (i, pad) in f.pads.iter().enumerate() {
                if pad.net == Some(id) {
                    let mut r = pad_row(path, f, i, pad);
                    r.sub = reference_of(project, f);
                    out.push(r);
                }
            }
        }
    }
}

/// How many rows of copper net `id` has on `side`.
fn net_copper_count(env: &Env, id: Id, side: Layer) -> usize {
    let board = &env.project.board;
    let mut n = board
        .tracks
        .iter()
        .filter(|t| t.layer == side && t.net == id)
        .count()
        + board
            .zones
            .iter()
            .filter(|z| z.layer == side && z.net == id)
            .count();
    if side == Layer::Front {
        n += board
            .footprints
            .iter()
            .flat_map(|f| &f.pads)
            .filter(|p| p.net == Some(id))
            .count();
    }
    n
}

/// The free board graphics drawn on the silkscreen of one side.
fn silk_graphics<'a>(env: &'a Env, front: bool) -> impl Iterator<Item = &'a pcbcraft_doc::Graphic> {
    let want = if front { "F.SilkS" } else { "B.SilkS" };
    env.project
        .graphics
        .iter()
        .filter(move |g| g.layer == "board" && g.sublayer == want)
}

fn layout_children(env: &Env, key: &str) -> Vec<Row> {
    let project = env.project;
    let board = &project.board;
    let c = env.colours;
    let segs: Vec<&str> = key.split('/').collect();
    let mut out = Vec::new();
    match segs.as_slice() {
        [""] => out = top_level(env),
        [side] if side.starts_with("side:") || side.starts_with("inner:") => {
            let s = path_layer(env, side);
            let front = s == Layer::Front;
            let inner = side.starts_with("inner:");
            let tracks = board.tracks.iter().filter(|t| t.layer == s).count();
            let zones = board.zones.iter().filter(|z| z.layer == s).count();
            let pads: usize = board
                .footprints
                .iter()
                .filter(|f| on_side(f, front))
                .map(|f| f.pads.len())
                .sum();
            let ck = if inner {
                format!("copper:inner{}", s.index())
            } else if front {
                "copper:front".to_owned()
            } else {
                "copper:back".to_owned()
            };
            let side_path = (*side).to_owned();
            let sw = if front {
                c.canvas.front
            } else if inner {
                c.canvas.front.gamma_multiply(0.6)
            } else {
                c.canvas.back
            };
            let mut copper = sub_row(
                key,
                "copper",
                &ck,
                SubLayer::Tracks.label().into(),
                sw,
                Visibility::SubLayer {
                    side: side_path.clone(),
                    sub: SubLayer::Tracks,
                },
                !inner,
                tracks + zones + pads,
                true,
            );
            copper.target = Some(Target::Copper(s));
            out.push(copper);
            if inner {
                return out;
            }
            let side_t = if front { Side::Top } else { Side::Bottom };
            out.push(sub_row(
                key,
                "mask",
                if front { "mask:front" } else { "mask:back" },
                t!("tree.sub.mask"),
                c.canvas.mask,
                Visibility::None,
                false,
                pads,
                false,
            ));
            let texts = board.texts.iter().filter(|x| x.layer == s).count();
            let parts = board
                .footprints
                .iter()
                .filter(|f| on_side(f, front))
                .count();
            let gfx = silk_graphics(env, front).count();
            out.push(sub_row(
                key,
                "zones",
                if front {
                    "zones:front"
                } else {
                    "zones:back"
                },
                SubLayer::Zones.label().into(),
                sw,
                Visibility::SubLayer {
                    side: side_path.clone(),
                    sub: SubLayer::Zones,
                },
                false,
                zones,
                zones > 0,
            ));
            out.push(sub_row(
                key,
                "silk",
                if front { "silk:front" } else { "silk:back" },
                t!("tree.sub.silk"),
                c.ink,
                Visibility::SubLayer {
                    side: side_path.clone(),
                    sub: SubLayer::Silk,
                },
                false,
                parts * 2 + texts + gfx,
                parts + texts + gfx > 0,
            ));
            if texts > 0 {
                out.push(sub_row(
                    key,
                    "text",
                    if front { "text:front" } else { "text:back" },
                    SubLayer::Text.label().into(),
                    c.ink,
                    Visibility::SubLayer {
                        side: side_path.clone(),
                        sub: SubLayer::Text,
                    },
                    false,
                    texts,
                    false,
                ));
            }
            out.push(sub_row(
                key,
                "paste",
                if front { "paste:front" } else { "paste:back" },
                t!("tree.sub.paste"),
                c.muted,
                Visibility::None,
                false,
                pads,
                false,
            ));
            out.push(sub_row(
                key,
                "components",
                if front { "parts:front" } else { "parts:back" },
                SubLayer::Pads.label().into(),
                c.canvas.pad,
                Visibility::SubLayer {
                    side: side_path,
                    sub: SubLayer::Pads,
                },
                false,
                parts,
                parts > 0,
            ));
            for r in &mut out {
                r.target = match r.controls.as_ref().map(|c| c.key.as_str()) {
                    Some("mask:front" | "mask:back") => Some(Target::Mask(side_t)),
                    Some("silk:front" | "silk:back") => Some(Target::Silk(side_t)),
                    Some("paste:front" | "paste:back") => Some(Target::Paste(side_t)),
                    _ => r.target,
                };
            }
        }
        [side, "copper"] => {
            let s = path_layer(env, side);
            let mut ids: Vec<Id> = project.nets.iter().map(|n| n.id).collect();
            for id in board
                .tracks
                .iter()
                .map(|t| t.net)
                .chain(board.zones.iter().map(|z| z.net))
            {
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
            ids.sort_by(|a, b| project.net_name(*a).cmp(project.net_name(*b)));
            for id in ids {
                let n = net_copper_count(env, id, s);
                if n == 0 {
                    continue;
                }
                let mut r = row(
                    key,
                    &format!("net:{id}"),
                    RowKind::Net,
                    project.net_name(id).to_owned(),
                );
                r.count = n.to_string();
                r.swatch = Some(net_colour(&c, id));
                r.round = true;
                r.net = Some(id);
                r.expandable = true;
                out.push(r);
            }
        }
        [side, "copper", net] if net.starts_with("net:") => {
            let id: Id = net[4..].parse().unwrap_or(0);
            net_copper(env, key, id, path_layer(env, side), &mut out);
        }
        [side, "silk"] => {
            let s = path_layer(env, side);
            let parts = board
                .footprints
                .iter()
                .filter(|f| on_side(f, s == Layer::Front))
                .count();
            out.push(counted(
                row(key, "refs", RowKind::Leaf, t!("tree.silk.reference_text")),
                parts,
                false,
            ));
            out.push(counted(
                row(key, "outlines", RowKind::Leaf, t!("tree.silk.outlines")),
                parts,
                false,
            ));
            for x in board.texts.iter().filter(|x| x.layer == s) {
                let mut r = row(
                    key,
                    &format!("text:{}", x.id),
                    RowKind::Text,
                    x.text.clone(),
                );
                r.select = Some(x.id);
                out.push(r);
            }
            for g in silk_graphics(env, s == Layer::Front) {
                let mut r = row(
                    key,
                    &format!("gfx:{}", g.id),
                    RowKind::Shape,
                    g.shape.clone(),
                );
                r.select = Some(g.id);
                out.push(r);
            }
        }
        [side, "components"] => {
            let front = path_layer(env, side) == Layer::Front;
            let labels = (t!("tree.side.top"), t!("tree.side.bottom"));
            for f in board.footprints.iter().filter(|f| on_side(f, front)) {
                out.push(footprint_row(project, key, f, (&labels.0, &labels.1)));
            }
        }
        ["drills"] => {
            let mut sizes: Vec<String> = board
                .vias
                .iter()
                .map(|v| format!("{:.2}", v.drill))
                .collect();
            sizes.sort();
            sizes.dedup();
            for size in sizes {
                let n = board
                    .vias
                    .iter()
                    .filter(|v| format!("{:.2}", v.drill) == size)
                    .count();
                let mut r = row(
                    key,
                    &format!("drill:{size}"),
                    RowKind::Via,
                    t!("tree.drill", size = size),
                );
                r.count = n.to_string();
                r.expandable = true;
                out.push(r);
            }
            for h in &board.holes {
                let mut r = row(
                    key,
                    &format!("hole:{}", h.id),
                    RowKind::Hole,
                    if h.end.is_some() {
                        t!("tree.slot", size = format!("{:.2}", h.diameter))
                    } else {
                        t!("tree.mounting_hole")
                    },
                );
                r.sub = format!("{:.0}, {:.0}", h.position.x, h.position.y);
                r.select = Some(h.id);
                out.push(r);
            }
        }
        ["drills", drill] if drill.starts_with("drill:") => {
            for v in board
                .vias
                .iter()
                .filter(|v| format!("{:.2}", v.drill) == drill[6..])
            {
                out.push(via_row(key, project, v));
            }
        }
        ["edge"] => {
            let mut edge = row(
                key,
                "edge",
                RowKind::Leaf,
                t!(
                    "tree.edge",
                    w = size_text(board.width),
                    h = size_text(board.height)
                ),
            );
            edge.dim = true;
            out.push(edge);
            for cut in &board.cutouts {
                let mut r = row(
                    key,
                    &format!("cut:{}", cut.id),
                    RowKind::Hole,
                    t!("tree.cutout"),
                );
                r.sub = format!("{:.0}, {:.0}", cut.center.x, cut.center.y);
                r.select = Some(cut.id);
                out.push(r);
            }
        }
        ["user"] => {
            for art in &board.user_layers {
                let mut r = sub_row(
                    key,
                    &format!("art:{}", art.id),
                    &format!("art:{}", art.name),
                    art.name.clone(),
                    c.link,
                    Visibility::Art(art.id),
                    true,
                    art.shapes.len(),
                    !art.shapes.is_empty(),
                );
                r.dim = false;
                r.target = Some(Target::User(art.id));
                out.push(r);
            }
        }
        ["user", art] if art.starts_with("art:") => {
            let id: Id = art[4..].parse().unwrap_or(0);
            if let Some(layer) = board.user_layers.iter().find(|l| l.id == id) {
                for (i, shape) in layer.shapes.iter().enumerate() {
                    out.push(row(
                        key,
                        &format!("shape:{i}"),
                        RowKind::Shape,
                        crate::art_shape_text(shape),
                    ));
                }
            }
        }
        ["components"] => {
            let labels = (t!("tree.side.top"), t!("tree.side.bottom"));
            for f in &board.footprints {
                out.push(footprint_row(project, key, f, (&labels.0, &labels.1)));
            }
        }
        [.., fp] if fp.starts_with("fp:") => {
            let id: Id = fp[3..].parse().unwrap_or(0);
            if let Some(f) = project.footprint(id) {
                for (i, pad) in f.pads.iter().enumerate() {
                    let mut r = pad_row(key, f, i, pad);
                    r.sub = pad
                        .net
                        .map(|n| project.net_name(n).to_owned())
                        .unwrap_or_default();
                    out.push(r);
                }
            }
        }
        ["nets"] => {
            let mut nets: Vec<&pcbcraft_doc::Net> = project.nets.iter().collect();
            nets.sort_by(|a, b| a.name.cmp(&b.name));
            for n in nets {
                let mut levels = Vec::new();
                for s in [Layer::Front, bottom(env)] {
                    let has_via = s != Layer::Front && board.vias.iter().any(|v| v.net == n.id);
                    if net_copper_count(env, n.id, s) > 0 || has_via {
                        levels.push(format!("L{}", level(env, s)));
                    }
                }
                let pads = board
                    .footprints
                    .iter()
                    .flat_map(|f| &f.pads)
                    .filter(|p| p.net == Some(n.id))
                    .count();
                let mut r = row(key, &format!("net:{}", n.id), RowKind::Net, n.name.clone());
                r.sub = levels.join(" ");
                r.count = t!("tree.pads", n = pads);
                r.swatch = Some(net_colour(&c, n.id));
                r.round = true;
                r.net = Some(n.id);
                r.expandable = true;
                out.push(r);
            }
        }
        ["nets", net] => {
            let id: Id = net[4..].parse().unwrap_or(0);
            for s in [Layer::Front, bottom(env)] {
                let n = net_copper_count(env, id, s);
                if n > 0 {
                    let seg = if s == Layer::Front {
                        "layer:front"
                    } else {
                        "layer:back"
                    };
                    let mut r = row(
                        key,
                        seg,
                        RowKind::Group,
                        t!("tree.net_layer", n = level(env, s), side = side_label(s)),
                    );
                    r.count = n.to_string();
                    r.expandable = true;
                    r.net = Some(id);
                    out.push(r);
                }
            }
            for v in board.vias.iter().filter(|v| v.net == id) {
                out.push(via_row(key, project, v));
            }
        }
        ["nets", net, layer] if layer.starts_with("layer:") => {
            let id: Id = net[4..].parse().unwrap_or(0);
            let s = if layer.ends_with("front") {
                Layer::Front
            } else {
                bottom(env)
            };
            net_copper(env, key, id, s, &mut out);
        }
        _ => {}
    }
    out
}

/// The fixed rows of page `id`: Symbols, Power ports, Drawing and Title
/// block, each with its count. Rows that hold nothing are left out unless the
/// tree shows empty ones.
///
/// # Rules
/// - The set is fixed: a page always has these four categories, so none can be
///   removed. Title block counts 1 while the page shows it and 0 once switched off.
/// - Keys are the row's own segment (`layer:symbols`); the caller adds the path.
///
/// # Dependencies
/// - `crates/doc/src/lib.rs` (`Project::page_settings_of`, `Project::schematic_label_page`).
///
/// # Do not
/// - Build a lock key from the segment; use [`page_row_name`].
fn sheet_rows(env: &Env, id: Id) -> Vec<Row> {
    let project = env.project;
    let c = env.colours;
    let symbols = project.components.iter().filter(|c| c.sheet == id).count();
    let power = project
        .labels
        .iter()
        .filter(|l| l.kind == "power" && project.schematic_label_page(l) == id)
        .count();
    let drawing = if id == 0 {
        project
            .graphics
            .iter()
            .filter(|g| g.layer == "schematic")
            .count()
    } else {
        0
    };
    let title = usize::from(!project.page_settings_of(id).title_hidden);
    let all: [(&'static str, String, Color32, usize, bool); 4] = [
        ("symbols", t!("tree.sch.symbols"), c.warn, symbols, true),
        (
            "power",
            t!("tree.sch.power_ports"),
            c.canvas.wire,
            power,
            false,
        ),
        ("drawing", t!("tree.sch.drawing"), c.link, drawing, false),
        ("title", t!("tree.sch.title_block"), c.ink, title, false),
    ];
    all.into_iter()
        .filter(|(_, _, _, n, _)| *n > 0 || env.show_empty)
        .map(|(seg, label, swatch, n, open)| {
            let mut r = layer_row(
                "",
                seg,
                label,
                swatch,
                Visibility::Page(id, seg),
                false,
                n,
                open,
            );
            if let Some(ctl) = r.controls.as_mut() {
                ctl.key = page_row_name(id, seg);
            }
            r.expandable = open && n > 0;
            r.dim = n == 0;
            r.sheet_row = Some((id, seg));
            r
        })
        .collect()
}

fn schematic_children(env: &Env, key: &str) -> Vec<Row> {
    let project = env.project;
    let c = env.colours;
    let segs: Vec<&str> = key.split('/').collect();
    let mut out = Vec::new();
    match segs.as_slice() {
        [""] => {
            let mut sheets: Vec<(Id, String)> = vec![(0, t!("tree.sheet.first"))];
            sheets.extend(project.sheets.iter().map(|s| (s.id, s.name.clone())));
            for (id, name) in sheets {
                let n = sheet_rows(env, id).len();
                let mut page = section(&crate::App::page_row_key(id), name, n);
                page.expandable = n > 0;
                page.page = Some(id);
                out.push(page);
            }
            let mut comps = counted(
                row("", "components", RowKind::Layer, t!("tree.section.parts")),
                project.components.len(),
                true,
            );
            comps.swatch = Some(c.ink);
            out.push(comps);
            let mut nets = counted(
                row("", "nets", RowKind::Layer, t!("tree.section.nets")),
                project.nets.len(),
                true,
            );
            nets.swatch = Some(c.canvas.wire);
            nets.controls = Some(LayerControls {
                key: String::new(),
                visibility: Visibility::Canvas("Wires"),
                focusable: false,
            });
            out.push(nets);
        }
        [sec] if sec.starts_with("sec:sheet:") => {
            let id: Id = sec[10..].parse().unwrap_or(0);
            out = sheet_rows(env, id)
                .into_iter()
                .map(|r| {
                    let mut r = r;
                    r.key = segment(key, &r.key);
                    r
                })
                .collect();
        }
        [sec, "layer:symbols"] if sec.starts_with("sec:sheet:") => {
            let id: Id = sec[10..].parse().unwrap_or(0);
            for comp in project.components.iter().filter(|c| c.sheet == id) {
                out.push(component_row(key, comp));
            }
        }
        [sec, "layer:power"] if sec.starts_with("sec:sheet:") => {
            let id: Id = sec[10..].parse().unwrap_or(0);
            for l in project
                .labels
                .iter()
                .filter(|l| l.kind == "power" && project.schematic_label_page(l) == id)
            {
                let mut r = row(
                    key,
                    &format!("label:{}", l.id),
                    RowKind::Label,
                    project.net_name(l.net).to_owned(),
                );
                r.sub = format!("{:.1}, {:.1}", l.position.x, l.position.y);
                r.select = Some(l.id);
                out.push(r);
            }
        }
        [sec, "layer:drawing"] if sec.starts_with("sec:sheet:") => {
            for g in project.graphics.iter().filter(|g| g.layer == "schematic") {
                let mut r = row(
                    key,
                    &format!("gfx:{}", g.id),
                    RowKind::Shape,
                    g.shape.clone(),
                );
                r.select = Some(g.id);
                out.push(r);
            }
        }
        ["components"] => {
            for comp in &project.components {
                out.push(component_row(key, comp));
            }
        }
        [.., comp] if comp.starts_with("comp:") => {
            let id: Id = comp[5..].parse().unwrap_or(0);
            if let Some(cmp) = project.component(id) {
                for (i, pin) in cmp.pins.iter().enumerate() {
                    let mut r = row(
                        key,
                        &format!("pin:{i}"),
                        RowKind::Pad,
                        t!("tree.pin", number = pin.number, name = pin.name),
                    );
                    r.sub = pin
                        .net
                        .map(|n| project.net_name(n).to_owned())
                        .unwrap_or_default();
                    r.select = Some(id);
                    r.marks = false;
                    out.push(r);
                }
            }
        }
        ["nets"] => out = net_rows(env, key, "pins"),
        ["nets", net] => {
            let id: Id = net[4..].parse().unwrap_or(0);
            for w in project.wires.iter().filter(|w| w.net == id) {
                let name = |p: &pcbcraft_doc::PinRef| {
                    project
                        .component(p.component)
                        .map_or_else(|| "?".to_owned(), |c| format!("{}.{}", c.reference, p.pin))
                };
                let mut r = row(
                    key,
                    &format!("wire:{}", w.id),
                    RowKind::Wire,
                    t!("tree.wire"),
                );
                r.sub = t!("tree.wire_ends", from = name(&w.from), to = name(&w.to));
                r.select = Some(w.id);
                out.push(r);
            }
            for l in project.labels.iter().filter(|l| l.net == id) {
                let mut r = row(
                    key,
                    &format!("label:{}", l.id),
                    RowKind::Label,
                    project.net_name(id).to_owned(),
                );
                r.sub = l.kind.clone();
                r.select = Some(l.id);
                out.push(r);
            }
            for comp in &project.components {
                for pin in comp.pins.iter().filter(|p| p.net == Some(id)) {
                    let mut r = row(
                        key,
                        &format!("npin:{}:{}", comp.id, pin.number),
                        RowKind::Pad,
                        t!(
                            "tree.pin",
                            number = format!("{}.{}", comp.reference, pin.number),
                            name = pin.name
                        ),
                    );
                    r.select = Some(comp.id);
                    r.marks = false;
                    out.push(r);
                }
            }
        }
        _ => {}
    }
    out
}

/// The rows directly under `key` (empty for the virtual root). Only called
/// for expanded rows.
fn children(env: &Env, key: &str) -> Vec<Row> {
    match env.tab {
        TreeTab::Layout => layout_children(env, key),
        TreeTab::Schematic => schematic_children(env, key),
    }
}

fn component_row(path: &str, c: &pcbcraft_doc::Component) -> Row {
    let mut r = row(
        path,
        &format!("comp:{}", c.id),
        RowKind::Part,
        c.reference.clone(),
    );
    r.sub = c.value.clone();
    r.count = c.pins.len().to_string();
    r.mono = true;
    r.select = Some(c.id);
    r.expandable = !c.pins.is_empty();
    r
}

fn track_row(path: &str, t: &pcbcraft_doc::Track) -> Row {
    let mut r = row(
        path,
        &format!("track:{}", t.id),
        RowKind::Track,
        t!("tree.track", width = format!("{:.2}", t.width)),
    );
    r.sub = side_label(t.layer);
    r.select = Some(t.id);
    r
}

fn via_row(path: &str, project: &Project, v: &pcbcraft_doc::Via) -> Row {
    let mut r = row(
        path,
        &format!("via:{}", v.id),
        RowKind::Via,
        t!("tree.via", drill = format!("{:.2}", v.drill)),
    );
    r.sub = crate::via_span::via_tree_span(project, v);
    r.select = Some(v.id);
    r.net = Some(v.net);
    r
}

fn pad_row(path: &str, f: &pcbcraft_doc::Footprint, i: usize, pad: &pcbcraft_doc::Pad) -> Row {
    let mut r = row(
        path,
        &format!("pad:{}:{i}", f.id),
        RowKind::Pad,
        t!("tree.pad", number = pad.number),
    );
    r.select = Some(f.id);
    r.marks = false;
    r.net = pad.net;
    r
}

/// Walk `row` and its expanded descendants into `out`; with a search active,
/// every branch opens and only matching rows and their ancestors stay.
fn walk(env: &Env, view: &TreeView, row: Row, depth: u8, level: u8, out: &mut Vec<Row>) -> bool {
    let mut row = row;
    row.depth = depth;
    row.level = level;
    let child_level = if row.kind == RowKind::Section {
        level
    } else {
        level.saturating_add(1)
    };
    let start = out.len();
    let searching = !view.query.trim().is_empty();
    let own = searching && matches_query(&row, view.query.trim());
    let open = row.expandable && (view.expanded.contains(&row.key) || searching);
    let key = row.key.clone();
    out.push(row);
    let mut any = false;
    if open {
        for child in children(env, &key) {
            any |= walk(env, view, child, depth.saturating_add(1), child_level, out);
        }
    }
    if searching {
        if !(own || any) {
            out.truncate(start);
            return false;
        }
        if own && !any {
            out.truncate(start + 1);
        }
    }
    true
}

fn matches_query(row: &Row, query: &str) -> bool {
    let q = query.to_lowercase();
    row.label.to_lowercase().contains(&q) || row.sub.to_lowercase().contains(&q)
}

fn signature(env: &Env, view: &TreeView) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    env.revision.hash(&mut h);
    (env.tab == TreeTab::Layout).hash(&mut h);
    view.expanded.hash(&mut h);
    view.query.hash(&mut h);
    env.show_empty.hash(&mut h);
    env.project.page_settings.len().hash(&mut h);
    env.project.sheets.len().hash(&mut h);
    let b = &env.project.board;
    (
        b.tracks.len(),
        b.footprints.len(),
        b.vias.len(),
        b.zones.len(),
        b.texts.len(),
        b.holes.len(),
        b.cutouts.len(),
        b.user_layers.len(),
        env.project.nets.len(),
        env.project.components.len(),
        env.project.wires.len(),
        env.project.labels.len(),
    )
        .hash(&mut h);
    h.finish()
}

/// The visible rows for this frame: every section and each expanded branch.
///
/// # Rules
/// - Builds only expanded branches unless a search is active.
/// - Cached on a signature of the revision, view state and collection sizes.
///
/// # Do not
/// - Call it from a loop; take the slice once per frame.
pub(crate) fn rows(env: &Env, view: &mut TreeView) -> std::rc::Rc<Vec<Row>> {
    let sig = signature(env, view);
    if let Some((cached, rows)) = &view.cache
        && *cached == sig
    {
        return std::rc::Rc::clone(rows);
    }
    let mut out = Vec::new();
    for top in children(env, "") {
        walk(env, view, top, 0, 0, &mut out);
    }
    let out = std::rc::Rc::new(out);
    view.cache = Some((sig, std::rc::Rc::clone(&out)));
    out
}

/// The keys to open, outermost first, and the key of the row that stands for
/// object `id`, so a canvas selection can reveal its row.
///
/// # Rules
/// - None when no row stands for the object on this tab.
pub(crate) fn reveal_path(env: &Env, id: Id) -> Option<(Vec<String>, String)> {
    let project = env.project;
    let board = &project.board;
    let target: String = match env.tab {
        TreeTab::Layout => {
            if let Some(t) = board.tracks.iter().find(|t| t.id == id) {
                format!("{}/copper/net:{}/track:{id}", side_key(t.layer), t.net)
            } else if let Some(v) = board.vias.iter().find(|v| v.id == id) {
                format!("drills/drill:{:.2}/via:{id}", v.drill)
            } else if let Some(z) = board.zones.iter().find(|z| z.id == id) {
                format!("{}/copper/net:{}/zone:{id}", side_key(z.layer), z.net)
            } else if let Some(x) = board.texts.iter().find(|x| x.id == id) {
                format!("{}/silk/text:{id}", side_key(x.layer))
            } else if board.cutouts.iter().any(|c| c.id == id) {
                format!("edge/cut:{id}")
            } else if board.holes.iter().any(|h| h.id == id) {
                format!("drills/hole:{id}")
            } else if board.footprints.iter().any(|f| f.id == id) {
                format!("components/fp:{id}")
            } else {
                return None;
            }
        }
        TreeTab::Schematic => {
            if project.component(id).is_some() {
                format!("components/comp:{id}")
            } else if let Some(l) = project.labels.iter().find(|l| l.id == id) {
                format!("nets/net:{}/label:{id}", l.net)
            } else {
                let w = project.wires.iter().find(|w| w.id == id)?;
                format!("nets/net:{}/wire:{id}", w.net)
            }
        }
    };
    let parts: Vec<&str> = target.split('/').collect();
    let opens = (1..parts.len()).map(|n| parts[..n].join("/")).collect();
    Some((opens, target))
}

/// The stored name of the sheet row `row` (`symbols`, `power`, `drawing` or
/// `title`) of page `page`, used both as its lock key and as its entry in the
/// hidden list.
///
/// # Rules
/// - One name per page and row, so hiding Drawing on page 2 leaves page 1 alone.
pub(crate) fn page_row_name(page: Id, row: &str) -> String {
    format!("page:{page}:{row}")
}

/// The page and sheet row a schematic object belongs to: symbols, power
/// ports (labels of kind power) and drawing graphics. None for anything else.
pub(crate) fn sheet_row_of(project: &Project, id: Id) -> Option<(Id, &'static str)> {
    if let Some(c) = project.component(id) {
        return Some((c.sheet, "symbols"));
    }
    if let Some(l) = project.labels.iter().find(|l| l.id == id)
        && l.kind == "power"
    {
        return Some((project.schematic_label_page(l), "power"));
    }
    project
        .graphics
        .iter()
        .find(|g| g.id == id && g.layer == "schematic")
        .map(|_| (0, "drawing"))
}

/// True when the canvas may not select or edit object `id` because a layer
/// lock covers it.
///
/// # Rules
/// - Tracks, zones, vias and text follow their layer's lock; parts follow
///   their own lock flag in the document, not this set.
pub(crate) fn is_locked(project: &Project, locked: &BTreeSet<String>, id: Id) -> bool {
    if locked.is_empty() {
        return false;
    }
    if let Some((page, row)) = sheet_row_of(project, id) {
        return locked.contains(&page_row_name(page, row));
    }
    let board = &project.board;
    if let Some(t) = board.tracks.iter().find(|t| t.id == id) {
        return locked.contains(copper_key(t.layer));
    }
    if board.vias.iter().any(|v| v.id == id) {
        return locked.contains("copper:front") || locked.contains("copper:back");
    }
    if let Some(z) = board.zones.iter().find(|z| z.id == id) {
        return locked.contains(copper_key(z.layer));
    }
    if let Some(x) = board.texts.iter().find(|x| x.id == id) {
        return locked.contains(if x.layer == Layer::Front {
            "silk:front"
        } else {
            "silk:back"
        });
    }
    if board.cutouts.iter().any(|c| c.id == id) || board.holes.iter().any(|h| h.id == id) {
        return locked.contains("outline");
    }
    false
}

/// Small vector shapes the tree draws instead of text symbols.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Glyph {
    ChevronRight,
    ChevronDown,
    Eye,
    EyeOff,
    Lock,
    Unlock,
    Focus,
    Layer,
    Part,
    Pad,
    Net,
    Track,
    Via,
    Zone,
    Text,
    Graphic,
    Hole,
    Label,
    Plus,
    Search,
    Cross,
    Wire,
    Outline,
}

/// Draw one glyph inside `rect` (about 14 points square).
///
/// # Rules
/// - Strokes only, so it reads on any theme colour.
///
/// # Do not
/// - Add text here; the glyphs are the screen-reader-free face of a control.
pub(crate) fn glyph(painter: &egui::Painter, rect: Rect, g: Glyph, col: Color32) {
    let c = rect.center();
    let s = rect.width().min(rect.height()) / 14.0;
    let p = |x: f32, y: f32| Pos2::new(c.x + x * s, c.y + y * s);
    let stroke = Stroke::new(1.3, col);
    let line = |a: Pos2, b: Pos2| painter.line_segment([a, b], stroke);
    let poly = |pts: Vec<Pos2>, closed: bool| {
        painter.add(Shape::Path(epaint::PathShape {
            points: pts,
            closed,
            fill: Color32::TRANSPARENT,
            stroke: epaint::PathStroke::new(1.3, col),
        }));
    };
    let arc = |cx: f32, cy: f32, r: f32, a0: f32, a1: f32| -> Vec<Pos2> {
        (0..=12)
            .map(|i| {
                let a = a0 + (a1 - a0) * i as f32 / 12.0;
                p(cx + r * a.cos(), cy + r * a.sin())
            })
            .collect()
    };
    match g {
        Glyph::ChevronRight => {
            painter.add(Shape::convex_polygon(
                vec![p(-2.0, -4.0), p(3.0, 0.0), p(-2.0, 4.0)],
                col,
                Stroke::NONE,
            ));
        }
        Glyph::ChevronDown => {
            painter.add(Shape::convex_polygon(
                vec![p(-4.0, -2.0), p(4.0, -2.0), p(0.0, 3.0)],
                col,
                Stroke::NONE,
            ));
        }
        Glyph::Eye | Glyph::EyeOff => {
            let mut lens: Vec<Pos2> = (0..=10)
                .map(|i| {
                    let x = -6.5 + 13.0 * i as f32 / 10.0;
                    p(x, -4.2 * (1.0 - (x / 6.5).powi(2)))
                })
                .collect();
            lens.extend((0..=10).rev().map(|i| {
                let x = -6.5 + 13.0 * i as f32 / 10.0;
                p(x, 4.2 * (1.0 - (x / 6.5).powi(2)))
            }));
            poly(lens, true);
            if g == Glyph::Eye {
                painter.circle_filled(p(0.0, 0.0), 2.0 * s, col);
            } else {
                line(p(-5.5, 5.5), p(5.5, -5.5));
            }
        }
        Glyph::Lock | Glyph::Unlock => {
            let body = Rect::from_min_max(p(-4.5, -0.5), p(4.5, 5.5));
            if g == Glyph::Lock {
                painter.rect_filled(body, 1.5, col);
                poly(
                    arc(0.0, -1.0, 3.0, std::f32::consts::PI, std::f32::consts::TAU),
                    false,
                );
                line(p(-3.0, -1.0), p(-3.0, -0.5));
                line(p(3.0, -1.0), p(3.0, -0.5));
            } else {
                painter.rect_stroke(body, 1.5, stroke, egui::StrokeKind::Inside);
                poly(
                    arc(0.0, -3.0, 3.0, std::f32::consts::PI, std::f32::consts::TAU),
                    false,
                );
                line(p(-3.0, -3.0), p(-3.0, -0.5));
            }
        }
        Glyph::Focus => {
            painter.circle_stroke(p(0.0, 0.0), 4.0 * s, stroke);
            painter.circle_filled(p(0.0, 0.0), 1.3 * s, col);
            line(p(0.0, -7.0), p(0.0, -5.0));
            line(p(0.0, 5.0), p(0.0, 7.0));
            line(p(-7.0, 0.0), p(-5.0, 0.0));
            line(p(5.0, 0.0), p(7.0, 0.0));
        }
        Glyph::Layer => {
            poly(
                vec![p(0.0, -5.5), p(6.0, -2.5), p(0.0, 0.5), p(-6.0, -2.5)],
                true,
            );
            poly(vec![p(-6.0, 0.5), p(0.0, 3.5), p(6.0, 0.5)], false);
            poly(vec![p(-6.0, 3.5), p(0.0, 6.5), p(6.0, 3.5)], false);
        }
        Glyph::Part => {
            painter.rect_stroke(
                Rect::from_min_max(p(-4.0, -4.0), p(4.0, 4.0)),
                1.0,
                stroke,
                egui::StrokeKind::Inside,
            );
            for y in [-2.0, 2.0] {
                line(p(-6.5, y), p(-4.0, y));
                line(p(4.0, y), p(6.5, y));
            }
        }
        Glyph::Pad => {
            painter.circle_stroke(p(0.0, 0.0), 4.5 * s, stroke);
            painter.circle_filled(p(0.0, 0.0), 1.6 * s, col);
        }
        Glyph::Net => {
            line(p(-4.5, 3.5), p(0.0, -3.5));
            line(p(0.0, -3.5), p(4.5, 3.5));
            for (x, y) in [(-4.5, 3.5), (0.0, -3.5), (4.5, 3.5)] {
                painter.circle_filled(p(x, y), 1.8 * s, col);
            }
        }
        Glyph::Track => {
            painter.line_segment([p(-5.0, 4.0), p(-1.0, 4.0)], Stroke::new(2.0, col));
            painter.line_segment([p(-1.0, 4.0), p(2.0, 0.0)], Stroke::new(2.0, col));
            painter.line_segment([p(2.0, 0.0), p(5.0, 0.0)], Stroke::new(2.0, col));
            painter.circle_filled(p(5.5, 0.0), 1.6 * s, col);
        }
        Glyph::Via => {
            painter.circle_stroke(p(0.0, 0.0), 4.5 * s, Stroke::new(2.0, col));
            painter.circle_filled(p(0.0, 0.0), 1.2 * s, col);
        }
        Glyph::Zone => {
            let r = Rect::from_min_max(p(-5.0, -5.0), p(5.0, 5.0));
            painter.rect_stroke(r, 1.0, stroke, egui::StrokeKind::Inside);
            line(p(-5.0, 1.0), p(1.0, -5.0));
            line(p(-5.0, 5.0), p(5.0, -5.0));
            line(p(-1.0, 5.0), p(5.0, -1.0));
        }
        Glyph::Text => {
            line(p(-4.5, -4.0), p(4.5, -4.0));
            line(p(0.0, -4.0), p(0.0, 5.0));
        }
        Glyph::Graphic => {
            painter.circle_stroke(p(-1.5, 1.5), 3.5 * s, stroke);
            painter.rect_stroke(
                Rect::from_min_max(p(0.0, -5.5), p(5.5, 0.0)),
                0.5,
                stroke,
                egui::StrokeKind::Inside,
            );
        }
        Glyph::Hole => {
            painter.circle_stroke(p(0.0, 0.0), 4.5 * s, stroke);
        }
        Glyph::Label => {
            poly(
                vec![
                    p(-6.0, -3.5),
                    p(2.5, -3.5),
                    p(6.0, 0.0),
                    p(2.5, 3.5),
                    p(-6.0, 3.5),
                ],
                true,
            );
        }
        Glyph::Plus => {
            line(p(-5.0, 0.0), p(5.0, 0.0));
            line(p(0.0, -5.0), p(0.0, 5.0));
        }
        Glyph::Search => {
            painter.circle_stroke(p(-1.5, -1.5), 4.0 * s, stroke);
            line(p(1.5, 1.5), p(6.0, 6.0));
        }
        Glyph::Wire => {
            poly(
                vec![
                    p(-6.0, 3.0),
                    p(-2.0, 3.0),
                    p(-2.0, -3.0),
                    p(2.0, -3.0),
                    p(2.0, 3.0),
                    p(6.0, 3.0),
                ],
                false,
            );
        }
        Glyph::Outline => {
            painter.rect_stroke(
                Rect::from_min_max(p(-5.0, -4.0), p(5.0, 4.0)),
                1.0,
                stroke,
                egui::StrokeKind::Inside,
            );
        }
        Glyph::Cross => {
            line(p(-4.0, -4.0), p(4.0, 4.0));
            line(p(-4.0, 4.0), p(4.0, -4.0));
        }
    }
}

fn glyph_for(kind: RowKind) -> Glyph {
    match kind {
        RowKind::Section | RowKind::Layer => Glyph::Layer,
        RowKind::Group => Glyph::Pad,
        RowKind::Leaf => Glyph::Outline,
        RowKind::Part => Glyph::Part,
        RowKind::Pad => Glyph::Outline,
        RowKind::Net => Glyph::Net,
        RowKind::Track => Glyph::Track,
        RowKind::Via => Glyph::Via,
        RowKind::Zone => Glyph::Zone,
        RowKind::Text => Glyph::Text,
        RowKind::Hole => Glyph::Hole,
        RowKind::Label => Glyph::Label,
        RowKind::Wire => Glyph::Wire,
        RowKind::Shape => Glyph::Graphic,
    }
}

/// The glyph of a group row, by the segment its key ends in.
fn group_glyph(key: &str) -> Glyph {
    match key.rsplit('/').next().unwrap_or("") {
        "pads" => Glyph::Outline,
        "tracks" => Glyph::Track,
        "zones" => Glyph::Zone,
        "vias" => Glyph::Via,
        "text" => Glyph::Text,
        _ => Glyph::Outline,
    }
}

fn stored_visible(env: &Env, ctl: &LayerControls) -> bool {
    match &ctl.visibility {
        Visibility::Canvas(name) => !layer_visibility::hidden_contains_legacy(env.hidden, name),
        Visibility::LayerGroup(side) => layer_visibility::group_stored(env.hidden, side),
        Visibility::SubLayer { side, sub } => {
            layer_visibility::sub_stored(env.hidden, side, *sub)
        }
        Visibility::Art(id) => env
            .project
            .board
            .user_layers
            .iter()
            .find(|l| l.id == *id)
            .is_none_or(|l| l.visible),
        Visibility::Page(page, row) => {
            let name = page_row_name(*page, row);
            !env.hidden.contains(&name)
        }
        Visibility::None => true,
    }
}

fn effective_visible(env: &Env, ctl: &LayerControls) -> bool {
    match &ctl.visibility {
        Visibility::SubLayer { side, sub } => {
            layer_visibility::sub_effective(env.hidden, side, *sub)
        }
        _ => stored_visible(env, ctl),
    }
}

/// A square icon button drawn with [`glyph`], with a tooltip and a name for
/// screen readers.
#[allow(clippy::too_many_arguments)]
fn icon_button(
    ui: &mut Ui,
    id: egui::Id,
    rect: Rect,
    g: Glyph,
    col: Color32,
    hover_fill: Color32,
    name: &str,
    tip: &str,
) -> egui::Response {
    let resp = ui.interact(rect, id, Sense::click());
    if resp.hovered() || resp.has_focus() {
        ui.painter().rect_filled(rect.shrink(2.0), 4.0, hover_fill);
    }
    glyph(
        ui.painter(),
        Rect::from_center_size(rect.center(), Vec2::splat(16.0)),
        g,
        col,
    );
    name_control(&resp, name, None);
    resp.on_hover_text(tip)
}

/// A ghost button: icon and one to three words, 28 points high.
fn ghost_button(
    ui: &mut Ui,
    colours: &ThemeColours,
    g: Glyph,
    label: &str,
    tip: &str,
) -> egui::Response {
    let galley = ui.painter().layout_no_wrap(
        label.to_owned(),
        crate::fonts::Role::Body.font(),
        colours.ink,
    );
    let w = galley.size().x + 14.0 + 6.0 + 16.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, 28.0), Sense::hover());
    let resp = ui.interact(rect, ui.id().with(("ghost", label)), Sense::click());
    if resp.hovered() || resp.has_focus() {
        ui.painter().rect_filled(rect, 4.0, colours.hover);
    }
    glyph(
        ui.painter(),
        Rect::from_center_size(
            Pos2::new(rect.left() + 15.0, rect.center().y),
            Vec2::splat(14.0),
        ),
        g,
        colours.ink,
    );
    ui.painter().galley(
        Pos2::new(rect.left() + 28.0, rect.center().y - galley.size().y / 2.0),
        galley,
        colours.ink,
    );
    name_control(&resp, label, None);
    resp.on_hover_text(tip)
}

/// Draw the tree and return what the user did.
///
/// # Rules
/// - Rows are virtualized: only rows in view are laid out.
/// - Arrows move, Right expands, Left collapses, Space toggles visibility,
///   F focuses, L locks, Enter selects; keys work while a tree row has keyboard focus.
/// - A canvas selection that changed since last frame opens its ancestors and
///   scrolls its row into view.
///
/// # Dependencies
/// - `crates/ui-egui/src/lib.rs` `App::layers_tree_panel` calls it and applies the actions.
/// - `crates/ui-egui/src/a11y.rs` names the controls.
///
/// # Do not
/// - Mutate the project; return an [`Action`] instead.
pub(crate) fn show(ui: &mut Ui, env: &Env, view: &mut TreeView) -> Vec<Action> {
    let mut actions = Vec::new();
    let colours = env.colours;
    if env.selected != view.last_selected {
        view.last_selected = env.selected;
        if let Some(id) = env.selected {
            view.selected_key = None;
            if let Some((opens, target)) = reveal_path(env, id) {
                view.expanded.extend(opens);
                view.cursor = Some(target.clone());
                view.scroll_to = Some(target);
            }
        }
    }
    let all = rows(env, view);
    view.hovered_row = None;
    header(ui, env, view, &all, &colours, &mut actions);
    let n = all.len();
    keyboard(ui, env, view, &all, &mut actions);
    let mut scroll = egui::ScrollArea::vertical()
        .id_salt("layers-tree-scroll")
        .auto_shrink([false, false]);
    if let Some(target) = view.scroll_to.take()
        && let Some(index) = all.iter().position(|r| r.key == target)
    {
        let top = index as f32 * ROW_HEIGHT;
        if top < view.offset || top + ROW_HEIGHT > view.offset + view.viewport {
            scroll = scroll.vertical_scroll_offset((top - view.viewport / 2.0).max(0.0));
        }
    }
    let mut has_focus = false;
    let spacing = ui.spacing().item_spacing.y;
    ui.spacing_mut().item_spacing.y = 0.0;
    let output = scroll.show_rows(ui, ROW_HEIGHT, n, |ui, range| {
        for r in &all[range] {
            has_focus |= paint_row(ui, env, view, r, &colours, &mut actions);
        }
    });
    ui.spacing_mut().item_spacing.y = spacing;
    view.hover_target = hovered_stack_target(env, &all, view.hovered_row.as_deref());
    view.offset = output.state.offset.y;
    view.viewport = output.inner_rect.height();
    view.has_focus = has_focus;
    view.hover_page = view.hover_now.take();
    if let Some((what, _)) = &view.rename
        && !view.editing_drawn
    {
        // The row is out of view: bring the first row for it into view so its editor shows.
        match all
            .iter()
            .find(|r| rename_for(env, r).is_some_and(|(w, _)| &w == what))
        {
            Some(r) => view.scroll_to = Some(r.key.clone()),
            None => view.rename = None,
        }
    }
    view.editing_drawn = false;
    if n == 0 && !view.query.trim().is_empty() {
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new(t!("tree.no_match", query = view.query.trim()))
                .font(crate::fonts::Role::Caption.font())
                .color(colours.muted),
        );
    }
    actions
}

/// What F2, the Rename menu item and the inline editor rename for this row,
/// with the name it has now. None for rows that have no rename command.
///
/// # Rules
/// - Nets, stackup layers, machine layers and schematic pages other than Main.
///
/// # Dependencies
/// - `crates/engine/src/lib.rs` holds `net.rename`, `board.stackup_rename_layer`,
///   `board.layer_rename` and `schematic.rename_sheet`.
///
/// # Do not
/// - Add a kind here before it has a typed rename command.
fn rename_for(env: &Env, r: &Row) -> Option<(RenameFor, String)> {
    if let Some(id) = r.page {
        return (id != 0).then(|| (RenameFor::Page(id), r.label.clone()));
    }
    if let Some(name) = &r.stack {
        return Some((RenameFor::Layer(name.clone()), name.clone()));
    }
    if let Some(ctl) = &r.controls
        && let Visibility::Art(id) = ctl.visibility
    {
        return Some((RenameFor::Art(id), r.label.clone()));
    }
    if r.kind == RowKind::Net
        && let Some(net) = r.net
    {
        return Some((RenameFor::Net(net), env.project.net_name(net).to_owned()));
    }
    None
}

/// The action a finished rename turns into.
fn rename_action(what: RenameFor, new_name: String) -> Action {
    match what {
        RenameFor::Layer(name) => Action::StackRename { name, new_name },
        RenameFor::Net(net) => Action::NetRename { net, new_name },
        RenameFor::Art(layer) => Action::ArtRename { layer, new_name },
        RenameFor::Page(id) => Action::PageRename { id, new_name },
    }
}

fn header(
    ui: &mut Ui,
    env: &Env,
    view: &mut TreeView,
    all: &[Row],
    colours: &ThemeColours,
    actions: &mut Vec<Action>,
) {
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.add_space(PAD + 4.0);
        ui.spacing_mut().item_spacing.x = 8.0;
        let width = ui.available_width() - 28.0 - PAD - 12.0;
        let field = ui.add(
            crate::targets::text_field(&mut view.query)
                .hint_text(t!("tree.search"))
                .desired_width(width),
        );
        field
            .clone()
            .named(&t!("tree.search"))
            .on_hover_text(t!("tree.search.tip"));
        let r = field.rect;
        glyph(
            ui.painter(),
            Rect::from_center_size(Pos2::new(r.right() - 14.0, r.center().y), Vec2::splat(14.0)),
            if view.query.is_empty() {
                Glyph::Search
            } else {
                Glyph::Cross
            },
            colours.muted,
        );
        let (cell, _) = ui.allocate_exact_size(Vec2::splat(28.0), Sense::hover());
        let id = ui.id().with("tree-add");
        let resp = ui.interact(cell, id, Sense::click());
        ui.painter().rect_stroke(
            cell,
            4.0,
            Stroke::new(1.0, colours.muted.gamma_multiply(0.6)),
            egui::StrokeKind::Inside,
        );
        if resp.hovered() {
            ui.painter()
                .rect_filled(cell.shrink(1.0), 4.0, colours.hover);
        }
        glyph(
            ui.painter(),
            Rect::from_center_size(cell.center(), Vec2::splat(14.0)),
            Glyph::Plus,
            colours.ink,
        );
        name_control(&resp, &t!("tree.add"), None);
        if resp.on_hover_text(t!("tree.add.tip")).clicked() {
            actions.push(Action::NewArtLayer);
        }
        if !view.query.is_empty() {
            let (x, y) = (r.right() - 22.0, r.top());
            let cross = Rect::from_min_size(Pos2::new(x, y), Vec2::new(22.0, r.height()));
            let resp = ui.interact(cross, ui.id().with("tree-clear"), Sense::click());
            name_control(&resp, &t!("tree.clear"), None);
            if resp.on_hover_text(t!("tree.clear.tip")).clicked() {
                view.query.clear();
            }
        }
    });
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.add_space(PAD);
        ui.spacing_mut().item_spacing.x = 4.0;
        if ghost_button(
            ui,
            colours,
            Glyph::Eye,
            &t!("tree.show_all"),
            &t!("tree.show_all.tip"),
        )
        .clicked()
        {
            actions.push(Action::SetHidden(Vec::new()));
        }
        if ghost_button(
            ui,
            colours,
            Glyph::EyeOff,
            &t!("tree.hide_others"),
            &t!("tree.hide_others.tip"),
        )
        .clicked()
        {
            let keep = view
                .cursor
                .as_deref()
                .and_then(|k| {
                    all.iter()
                        .find(|r| r.key == k)
                        .and_then(|_| layer_visibility_name(all, k))
                })
                .or(match env.tab {
                    TreeTab::Layout => Some(match env.target {
                        Target::Copper(l) if l != Layer::Front => "Bottom copper",
                        _ => "Top copper",
                    }),
                    TreeTab::Schematic => Some("Wires"),
                });
            actions.push(Action::SetHidden(
                layer_names(env.tab)
                    .iter()
                    .copied()
                    .filter(|n| Some(*n) != keep)
                    .collect(),
            ));
        }
        if ghost_button(
            ui,
            colours,
            Glyph::Plus,
            &t!("tree.new_layer"),
            &t!("tree.add_layer.tip"),
        )
        .clicked()
        {
            actions.push(Action::NewArtLayer);
        }
        if env.tab == TreeTab::Schematic {
            let (label, tip) = if view.show_empty {
                (t!("tree.hide_empty"), t!("tree.hide_empty.tip"))
            } else {
                (t!("tree.show_empty"), t!("tree.show_empty.tip"))
            };
            if ghost_button(ui, colours, Glyph::Layer, &label, &tip).clicked() {
                view.show_empty = !view.show_empty;
            }
        }
    });
    ui.add_space(6.0);
    let (line, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
    ui.painter()
        .rect_filled(line, 0.0, colours.muted.gamma_multiply(0.22));
}

/// The canvas toggle name of the layer row that `key` sits on or under.
fn layer_visibility_name(all: &[Row], key: &str) -> Option<&'static str> {
    let mut path = key;
    loop {
        if let Some(r) = all.iter().find(|r| r.key == path)
            && let Some(ctl) = &r.controls
        {
            return match &ctl.visibility {
                Visibility::Canvas(name) => Some(name),
                Visibility::LayerGroup(side) => match side.as_str() {
                    "side:top" => Some("Top copper"),
                    "side:bottom" => Some("Bottom copper"),
                    _ => None,
                },
                _ => None,
            };
        }
        path = path.rsplit_once('/')?.0;
    }
}

/// The layer or sublayer a hovered row belongs to: its own target, or the
/// nearest ancestor's, so a net, zone or part previews the sublayer it sits on.
///
/// # Rules
/// - Edge cuts and user layers have no place in the stack, so they preview nothing.
/// - A whole-layer row (`side:` or `inner:`) previews that layer's copper.
fn hovered_stack_target(env: &Env, all: &[Row], key: Option<&str>) -> Option<Target> {
    let mut path = key?;
    loop {
        if let Some(Target::Copper(_) | Target::Silk(_) | Target::Mask(_) | Target::Paste(_)) =
            all.iter().find(|r| r.key == path).and_then(|r| r.target)
        {
            return all.iter().find(|r| r.key == path).and_then(|r| r.target);
        }
        match path.rsplit_once('/') {
            Some((parent, _)) => path = parent,
            // A whole-layer row previews that layer's copper.
            None if path.starts_with("side:") || path.starts_with("inner:") => {
                return Some(Target::Copper(path_layer(env, path)));
            }
            None => return None,
        }
    }
}

/// Paint one row; returns true when it holds keyboard focus.
fn paint_row(
    ui: &mut Ui,
    env: &Env,
    view: &mut TreeView,
    r: &Row,
    colours: &ThemeColours,
    actions: &mut Vec<Action>,
) -> bool {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, ROW_HEIGHT), Sense::hover());
    let id = egui::Id::new(("layers-tree-row", &r.key));
    let resp = ui.interact(rect, id, Sense::click());
    if view.focus_pending.as_deref() == Some(r.key.as_str()) {
        resp.request_focus();
        view.focus_pending = None;
    }
    if resp.hovered() {
        view.hovered_row = Some(r.key.clone());
    }
    let focused = resp.has_focus();
    if resp.hovered()
        && let Some(page) = r.page
    {
        view.hover_now = Some(page);
    }
    let selected = (r.marks && r.select.is_some() && r.select == env.selected)
        || view.selected_key.as_deref() == Some(r.key.as_str());
    let searching = !view.query.trim().is_empty();
    let expanded = view.expanded.contains(&r.key) || searching;
    let stored_off = r
        .controls
        .as_ref()
        .is_some_and(|c| !stored_visible(env, c));
    let drawn_off = r
        .controls
        .as_ref()
        .is_some_and(|c| !effective_visible(env, c));
    let locked = r
        .controls
        .as_ref()
        .is_some_and(|c| !c.key.is_empty() && view.locked.contains(&c.key));
    let focus_on = r
        .controls
        .as_ref()
        .is_some_and(|c| c.focusable && env.focus == Some(c.key.as_str()));
    let is_section = r.kind == RowKind::Section;
    let dim = if drawn_off || r.dim { 0.45 } else { 1.0 };
    let painter = ui.painter().clone();
    if selected {
        painter.rect_filled(rect, 0.0, colours.accent.gamma_multiply(0.16));
        painter.rect_filled(
            Rect::from_min_size(rect.min, Vec2::new(3.0, rect.height())),
            0.0,
            colours.accent,
        );
    } else if resp.hovered() && !is_section {
        painter.rect_filled(rect, 0.0, colours.hover);
    }
    if focused {
        painter.rect_stroke(
            rect.shrink(1.0),
            2.0,
            Stroke::new(2.0, colours.ink),
            egui::StrokeKind::Inside,
        );
    }
    let level = f32::from(r.level);
    let x0 = rect.left() + PAD + level * INDENT;
    let cy = rect.center().y;
    // Indent guides.
    for d in 0..(level as u32) {
        let x = rect.left() + PAD + d as f32 * INDENT + 8.0;
        painter.line_segment(
            [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
            Stroke::new(1.0, colours.muted.gamma_multiply(0.18)),
        );
    }
    // Chevron cell.
    let mut chevron_clicked = false;
    let chevron_rect = Rect::from_center_size(Pos2::new(x0 + 8.0, cy), Vec2::new(24.0, ROW_HEIGHT));
    if r.expandable {
        let cr = ui.interact(chevron_rect, id.with("chevron"), Sense::click());
        glyph(
            &painter,
            Rect::from_center_size(chevron_rect.center(), Vec2::splat(12.0)),
            if expanded {
                Glyph::ChevronDown
            } else {
                Glyph::ChevronRight
            },
            if cr.hovered() {
                colours.ink
            } else {
                colours.muted
            },
        );
        name_control(
            &cr,
            &if expanded {
                t!("tree.collapse", name = r.label)
            } else {
                t!("tree.expand", name = r.label)
            },
            None,
        );
        chevron_clicked = cr.on_hover_text(t!("tree.chevron.tip")).clicked();
    }
    // Right column: eye, lock, count (design-system 5.7).
    let mut right = rect.right() - PAD;
    let eye_cell = Rect::from_min_size(
        Pos2::new(right - CELL, rect.top() + 2.0),
        Vec2::new(CELL, ROW_HEIGHT - 4.0),
    );
    let lock_cell = eye_cell.translate(Vec2::new(-CELL, 0.0));
    let focus_cell = lock_cell.translate(Vec2::new(-CELL, 0.0));
    let hovered = resp.hovered() || focused;
    if let Some(ctl) = &r.controls {
        let can_hide = ctl.visibility != Visibility::None;
        let eye_col = if !can_hide {
            colours.muted.gamma_multiply(0.4)
        } else if stored_off {
            colours.muted.gamma_multiply(0.7)
        } else {
            colours.ink
        };
        let sub_label = match &ctl.visibility {
            Visibility::SubLayer { sub, .. } => sub.label(),
            _ => r.label.as_str(),
        };
        let layer_hides_sub = matches!(&ctl.visibility, Visibility::SubLayer { side, sub } if {
            layer_visibility::sub_stored(env.hidden, side, *sub)
                && !layer_visibility::group_stored(env.hidden, side)
        });
        let (name, tip) = if stored_off {
            (t!("tree.show", name = sub_label), t!("tree.show.tip"))
        } else if can_hide {
            (
                t!("tree.hide", name = sub_label),
                if layer_hides_sub {
                    format!("{sub_label} are hidden by the layer")
                } else {
                    t!("tree.hide.tip").to_string()
                },
            )
        } else {
            (
                t!("tree.hide", name = r.label),
                t!("tree.hide.none.tip").to_string(),
            )
        };
        let eye = icon_button(
            ui,
            id.with("eye"),
            eye_cell,
            if stored_off {
                Glyph::EyeOff
            } else {
                Glyph::Eye
            },
            eye_col,
            colours.hover,
            &name,
            &tip,
        );
        if eye.clicked() && can_hide {
            match &ctl.visibility {
                Visibility::Canvas(name) => {
                    actions.push(Action::SetShown { name, on: stored_off })
                }
                Visibility::LayerGroup(side) => actions.push(Action::SetLayerGroup {
                    side: side.clone(),
                    on: stored_off,
                }),
                Visibility::SubLayer { side, sub } => actions.push(Action::SetSubLayer {
                    side: side.clone(),
                    sub: *sub,
                    on: stored_off,
                }),
                Visibility::Art(layer) => actions.push(Action::SetArtShown {
                    layer: *layer,
                    on: stored_off,
                }),
                Visibility::Page(page, row) => actions.push(Action::SetPageRowShown {
                    page: *page,
                    row,
                    on: stored_off,
                }),
                Visibility::None => {}
            }
        }
        if !ctl.key.is_empty() {
            let lock = icon_button(
                ui,
                id.with("lock"),
                lock_cell,
                if locked { Glyph::Lock } else { Glyph::Unlock },
                if locked {
                    colours.ink
                } else {
                    colours
                        .muted
                        .gamma_multiply(if hovered { 1.0 } else { 0.55 })
                },
                colours.hover,
                &if locked {
                    t!("tree.unlock", name = r.label)
                } else {
                    t!("tree.lock", name = r.label)
                },
                &if locked {
                    t!("tree.unlock.tip")
                } else {
                    t!("tree.lock.tip")
                },
            );
            if lock.clicked() && !view.locked.remove(&ctl.key) {
                view.locked.insert(ctl.key.clone());
            }
        }
        if ctl.focusable && (hovered || focus_on) {
            let fo = icon_button(
                ui,
                id.with("focus"),
                focus_cell,
                Glyph::Focus,
                if focus_on {
                    colours.accent
                } else {
                    colours.muted
                },
                colours.hover,
                &if focus_on {
                    t!("tree.unfocus", name = r.label)
                } else {
                    t!("tree.focus", name = r.label)
                },
                &if focus_on {
                    t!("tree.unfocus.tip")
                } else {
                    t!("tree.focus.tip")
                },
            );
            if fo.clicked() {
                actions.push(Action::Focus(if focus_on {
                    None
                } else {
                    Some(ctl.key.clone())
                }));
            }
        }
        right = if ctl.key.is_empty() {
            eye_cell.left() - 8.0
        } else {
            lock_cell.left() - 8.0
        };
    } else if r.kind == RowKind::Section {
        right = rect.right() - PAD - 4.0;
    } else {
        right = eye_cell.left() - 8.0;
    }
    let mono = crate::fonts::Role::Mono.font();
    let mut label_right = right;
    if !r.count.is_empty() {
        let count_col = if is_section {
            colours.muted
        } else {
            colours.muted.gamma_multiply(dim)
        };
        let rect_c = painter.text(
            Pos2::new(right, cy),
            Align2::RIGHT_CENTER,
            &r.count,
            mono.clone(),
            count_col,
        );
        label_right = rect_c.left() - 8.0;
    }
    let active = env.tab == TreeTab::Layout && r.target == Some(env.target);
    if active {
        let badge = painter.layout_no_wrap(
            t!("tree.active"),
            crate::fonts::Role::Caption.font(),
            colours.accent,
        );
        let w = badge.size().x + 12.0;
        let b = Rect::from_min_size(Pos2::new(label_right - w, cy - 9.0), Vec2::new(w, 18.0));
        painter.rect_filled(b, 4.0, colours.accent.gamma_multiply(0.16));
        painter.galley(
            Pos2::new(b.left() + 6.0, cy - badge.size().y / 2.0),
            badge,
            colours.accent,
        );
        label_right = b.left() - 6.0;
    }
    // Icon or swatch, then the label.
    let icon_c = Pos2::new(x0 + 16.0 + 8.0 + 2.0, cy);
    let label_left;
    if is_section {
        label_left = x0 + 20.0;
    } else {
        label_left = x0 + 16.0 + 24.0;
        if let Some(sw) = r.swatch {
            let box_ = Rect::from_center_size(icon_c, Vec2::splat(12.0));
            if r.round {
                painter.circle_filled(icon_c, 5.0, sw.gamma_multiply(dim));
            } else {
                painter.rect_filled(box_, 3.0, sw.gamma_multiply(dim));
            }
        } else {
            let g = if r.kind == RowKind::Group {
                group_glyph(&r.key)
            } else {
                glyph_for(r.kind)
            };
            glyph(
                &painter,
                Rect::from_center_size(icon_c, Vec2::splat(14.0)),
                g,
                colours.muted.gamma_multiply(dim),
            );
        }
    }
    let clip = Rect::from_min_max(
        Pos2::new(label_left, rect.top()),
        Pos2::new(label_right.max(label_left), rect.bottom()),
    );
    let p = painter.with_clip_rect(clip);
    let top_level = matches!(r.kind, RowKind::Layer | RowKind::Part | RowKind::Net) || is_section;
    let ink = if is_section {
        colours.muted
    } else if top_level {
        colours.ink.gamma_multiply(dim)
    } else {
        colours.muted.gamma_multiply(dim.max(0.7))
    };
    let font = if is_section {
        crate::fonts::Role::Caption.font()
    } else if r.mono {
        crate::fonts::Role::Mono.font()
    } else {
        crate::fonts::Role::Body.font()
    };
    let text = if is_section {
        r.label.to_uppercase()
    } else {
        r.label.clone()
    };
    let galley = painter.layout_no_wrap(text, font, ink);
    let label_w = galley.size().x;
    let editing = !view.editing_drawn
        && match (&view.rename, rename_for(env, r)) {
            (Some((want, _)), Some((what, _))) => *want == what,
            _ => false,
        };
    if editing {
        view.editing_drawn = true;
        let edit_rect = Rect::from_min_max(
            Pos2::new(label_left - 4.0, rect.top() + 1.0),
            Pos2::new(label_right.max(label_left + 120.0), rect.bottom() - 1.0),
        );
        let done = ui
            .scope_builder(egui::UiBuilder::new().max_rect(edit_rect), |ui| match view
                .rename
                .as_mut()
            {
                Some((_, text)) => crate::pages::inline_name_edit(
                    ui,
                    &r.key,
                    text,
                    edit_rect.width(),
                    &t!("tree.rename.tip"),
                ),
                None => None,
            })
            .inner;
        if let Some(done) = done
            && let Some((what, text)) = view.rename.take()
            && done == crate::pages::NameEdit::Commit
        {
            let new_name = text.trim().to_owned();
            if !new_name.is_empty() {
                actions.push(rename_action(what, new_name));
            }
        }
    } else {
        p.galley(
            Pos2::new(label_left, cy - galley.size().y / 2.0),
            galley,
            ink,
        );
    }
    if !r.sub.is_empty() && !editing {
        let dx = label_left + label_w + 6.0;
        if dx < label_right {
            p.text(
                Pos2::new(dx, cy),
                Align2::LEFT_CENTER,
                &r.sub,
                crate::fonts::Role::Mono.font(),
                colours.muted.gamma_multiply(dim),
            );
        }
    }
    // Interaction.
    let state = match &r.controls {
        Some(ctl) if !ctl.key.is_empty() => format!(
            "{}, {}, {}",
            r.label,
            if stored_off {
                t!("tree.state.hidden")
            } else {
                t!("tree.state.shown")
            },
            if locked {
                t!("tree.state.locked")
            } else {
                t!("tree.state.unlocked")
            }
        ),
        _ if !r.sub.is_empty() => format!("{}, {}", r.label, r.sub),
        _ => r.label.clone(),
    };
    name_control(&resp, &state, Some(selected));
    let tip = Some(explain(env.tab, r))
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| match r.kind {
            RowKind::Section => t!("tree.section.tip"),
            RowKind::Layer => t!("tree.layer.tip"),
            _ => t!("tree.row.tip"),
        });
    let resp = resp.on_hover_text(tip);
    let flags = (stored_off, locked, focus_on);
    if let Some(act) =
        crate::ui_kit::menu::context(&resp, menu_tag(r.kind, r.controls.is_some()), || {
            row_menu_entries(env, view, r, flags)
        })
    {
        run_row_act(act, env, view, r, flags, actions);
    }
    if chevron_clicked || (resp.double_clicked() && r.expandable) {
        toggle(view, &r.key);
    } else if resp.clicked() {
        view.cursor = Some(r.key.clone());
        activate(view, r, actions);
    }
    focused
}

/// What picking a row of a tree row's right-click menu does.
#[derive(Clone, Debug, PartialEq, Eq)]
enum RowAct {
    Active,
    ToggleVisibility,
    ToggleLock,
    ToggleFocus,
    RemoveArt,
    Select,
    Delete,
    ZoomTo,
    Flip,
    Rename,
    NetClass(Option<Id>),
    Properties,
    Highlight,
    ToggleExpand,
    AddPair,
    StackRename,
    MoveUp,
    MoveDown,
    RemoveLayer,
    ExportBom,
    ExportBomSelection,
    ToggleBomExclude,
NetClass(Option<Id>),
    Properties,
    Highlight,
    ToggleExpand,
    AddPair,
    StackRename,
    MoveUp,
    MoveDown,
    RemoveLayer,
    ExportBom,
    ExportBomSelection,
    ToggleBomExclude,
    PageAdd,
    PageRemove,
    SelectAll,
    AddPowerPort,
    TitleToggle,
    /// Remove on a fixed row of a page; the menu always shows it disabled.
    Remove,
}
}

/// The kind name capture mode uses to open the first menu of this kind.
fn menu_tag(kind: RowKind, has_controls: bool) -> &'static str {
    if has_controls {
        return "tree-layer";
    }
    match kind {
        RowKind::Layer => "tree-heading",
        RowKind::Part => "tree-part",
        RowKind::Net => "tree-net",
        _ => "tree-row",
    }
}

/// One row of the menu from a catalog string pair, splitting a shortcut written after two spaces.
fn row_item(
    act: RowAct,
    label: String,
    tip: String,
    icon: crate::ui_kit::icons::Icon,
) -> crate::ui_kit::menu::Item<RowAct> {
    let (label, shortcut) = crate::ui_kit::menu::split_shortcut(&label);
    crate::ui_kit::menu::Item::new(act, label, tip)
        .icon(icon)
        .shortcut(shortcut)
}

/// The rows of a tree row's right-click menu, in four groups: object actions,
/// edit, view, then the destructive ones. Each runs the same action as its
/// shortcut or control elsewhere and shows that shortcut at the right.
///
/// # Rules
/// - Layer rows: Make active, Hide or Show, Lock or Unlock, Focus; machine
///   layers also Remove. Objects: Select, Delete. Nets: Highlight.
/// - Reads the tree state only; `run_row_act` changes it.
///
/// # Dependencies
/// - crates/ui-egui/src/ui_kit/menu.rs (`Menu`, `Item`).
/// - crates/text/strings/en.txt (`tree.menu.*`).
///
/// # Do not
/// - Call an action from here; return the row and let `run_row_act` do it.
fn row_menu_entries(
    env: &Env,
    view: &TreeView,
    r: &Row,
    (hidden, locked, focus_on): (bool, bool, bool),
) -> Vec<crate::ui_kit::menu::Entry<RowAct>> {
    use crate::ui_kit::icons as kit;
    use crate::ui_kit::menu::{Entry, Item, Menu};
    use crate::ui_kit::menu_icons as glyph;
    let mut object: Vec<Entry<RowAct>> = Vec::new();
    let mut edit: Vec<Entry<RowAct>> = Vec::new();
    let mut shown: Vec<Entry<RowAct>> = Vec::new();
    let mut destructive: Vec<Item<RowAct>> = Vec::new();
    if r.target.is_some() {
        object.push(
            row_item(
                RowAct::Active,
                t!("tree.menu.active"),
                t!("tree.menu.active.tip"),
                kit::OK,
            )
            .into(),
        );
    }
    if let Some(name) = &r.stack {
        let copper = !r.key.starts_with("diel:");
        let inner = r.key.starts_with("inner:");
        let bottom_row = r.key.starts_with("side:bottom");
        if copper && !bottom_row {
            object.push(
                row_item(
                    RowAct::AddPair,
                    t!("tree.menu.add_pair"),
                    t!("tree.menu.add_pair.tip"),
                    kit::PLUS,
                )
                .into(),
            );
        }
        edit.push(
            row_item(
                RowAct::StackRename,
                t!("tree.menu.rename"),
                t!("tree.menu.rename.tip"),
                glyph::RENAME,
            )
            .into(),
        );
        if inner {
            let layers = &env.project.board.layer_records;
            let first = layers
                .iter()
                .filter(|x| x.kind == "copper")
                .nth(1)
                .is_some_and(|x| &x.name == name);
            let last = layers
                .iter()
                .filter(|x| x.kind == "copper")
                .nth(copper_count(env).saturating_sub(2))
                .is_some_and(|x| &x.name == name);
            if !first {
                edit.push(
                    row_item(
                        RowAct::MoveUp,
                        t!("tree.menu.move_up"),
                        t!("tree.menu.move_up.tip"),
                        glyph::MOVE_UP,
                    )
                    .into(),
                );
            }
            if !last {
                edit.push(
                    row_item(
                        RowAct::MoveDown,
                        t!("tree.menu.move_down"),
                        t!("tree.menu.move_down.tip"),
                        glyph::MOVE_DOWN,
                    )
                    .into(),
                );
            }
            destructive.push(row_item(
                RowAct::RemoveLayer,
                t!("tree.menu.remove_layer"),
                t!("tree.menu.remove_layer.tip"),
                glyph::TRASH,
            ));
        }
    }
    if let Some(page) = r.page {
        let named = page != 0;
        let why = t!("schematic.pages.main.tip");
        edit.push(
            Item::new(
                RowAct::Rename,
                t!("schematic.pages.rename"),
                t!("schematic.pages.rename.tip"),
            )
            .icon(glyph::RENAME)
            .enabled_if(named, why.clone())
            .into(),
        );
        object.push(
            Item::new(
                RowAct::PageAdd,
                t!("schematic.pages.add"),
                t!("schematic.pages.add.tip"),
            )
            .icon(kit::PLUS)
            .into(),
        );
        destructive.push(
            Item::new(
                RowAct::PageRemove,
                t!("schematic.pages.remove"),
                t!("schematic.pages.remove.tip"),
            )
            .icon(glyph::TRASH)
            .enabled_if(named, why),
        );
    }
    if let Some((page, row)) = r.sheet_row {
        // A fixed row of a page: Select all, the row's own action, and a
        // Remove that is always disabled (these are categories, not user layers).
        if row != "title" {
            object.push(
                row_item(
                    RowAct::SelectAll,
                    t!("tree.menu.select_all"),
                    t!("tree.menu.select_all.tip"),
                    glyph::SELECT,
                )
                .enabled_if(!locked, t!("tree.menu.select_all.locked"))
                .into(),
            );
        }
        match row {
            "power" => edit.push(
                row_item(
                    RowAct::AddPowerPort,
                    t!("tree.menu.add_power_port"),
                    t!("tree.menu.add_power_port.tip"),
                    kit::PLUS,
                )
                .into(),
            ),
            "title" => {
                let shown = !env.project.page_settings_of(page).title_hidden;
                let (label, tip) = if shown {
                    (t!("tree.menu.title_off"), t!("tree.menu.title_off.tip"))
                } else {
                    (t!("tree.menu.title_on"), t!("tree.menu.title_on.tip"))
                };
                edit.push(row_item(RowAct::TitleToggle, label, tip, glyph::PROPERTIES).into());
            }
            _ => {}
        }
        destructive.push(
            row_item(
                RowAct::Remove,
                t!("tree.menu.remove_fixed"),
                String::new(),
                glyph::TRASH,
            )
            .enabled_if(
                false,
                if row == "title" {
                    t!("tree.menu.remove_fixed.title.tip")
                } else {
                    t!("tree.menu.remove_fixed.tip")
                },
            ),
        );
    }
    if let Some(ctl) = &r.controls {
        if ctl.visibility != Visibility::None {
            let label = if hidden {
                t!("tree.menu.show")
            } else {
                t!("tree.menu.hide")
            };
            shown.push(
                row_item(
                    RowAct::ToggleVisibility,
                    label,
                    t!("tree.menu.hide.tip"),
                    if hidden { kit::EYE } else { kit::EYE_OFF },
                )
                .into(),
            );
        }
        if !ctl.key.is_empty() {
            let label = if locked {
                t!("tree.menu.unlock")
            } else {
                t!("tree.menu.lock")
            };
            shown.push(
                row_item(
                    RowAct::ToggleLock,
                    label,
                    t!("tree.menu.lock.tip"),
                    if locked { kit::LOCK_OPEN } else { kit::LOCK },
                )
                .into(),
            );
        }
        if ctl.focusable {
            let label = if focus_on {
                t!("tree.menu.unfocus")
            } else {
                t!("tree.menu.focus")
            };
            shown.push(
                row_item(
                    RowAct::ToggleFocus,
                    label,
                    t!("tree.menu.focus.tip"),
                    glyph::FOCUS,
                )
                .into(),
            );
        }
        if matches!(ctl.visibility, Visibility::Art(_)) {
            edit.push(
                row_item(
                    RowAct::Rename,
                    t!("tree.menu.rename"),
                    t!("tree.menu.rename.tip"),
                    glyph::RENAME,
                )
                .into(),
            );
            destructive.push(row_item(
                RowAct::RemoveArt,
                t!("tree.menu.remove"),
                t!("tree.menu.remove.tip"),
                glyph::TRASH,
            ));
        }
    }
    if let Some(id) = r.select {
        object.push(
            row_item(
                RowAct::Select,
                t!("tree.menu.select"),
                t!("tree.menu.select.tip"),
                glyph::SELECT,
            )
            .into(),
        );
        if r.marks && env.selected == Some(id) {
            destructive.push(row_item(
                RowAct::Delete,
                t!("tree.menu.delete"),
                t!("tree.menu.delete.tip"),
                glyph::TRASH,
            ));
        }
        if r.kind == RowKind::Part && env.project.footprint(id).is_some() {
            shown.insert(
                0,
                row_item(
                    RowAct::ZoomTo,
                    t!("tree.menu.zoom_to"),
                    t!("tree.menu.zoom_to.tip"),
                    kit::ZOOM_IN,
                )
                .into(),
            );
            edit.push(
                row_item(
                    RowAct::Flip,
                    t!("tree.menu.flip"),
                    t!("tree.menu.flip.tip"),
                    glyph::FLIP,
                )
                .into(),
            );
        }
    }
    if r.kind == RowKind::Net
        && let Some(net) = r.net
    {
        edit.push(
            row_item(
                RowAct::Rename,
                t!("tree.menu.rename"),
                t!("tree.menu.rename.tip"),
                glyph::RENAME,
            )
            .into(),
        );
        let current = env.project.net_class_id(net);
        let mut classes: Vec<Entry<RowAct>> = env
            .project
            .net_classes
            .iter()
            .map(|class| {
                Item::new(
                    RowAct::NetClass(Some(class.id)),
                    class.name.clone(),
                    t!("tree.menu.net_class.tip"),
                )
                .checked(current == Some(class.id))
                .into()
            })
            .collect();
        classes.push(
            Item::new(
                RowAct::NetClass(None),
                t!("tree.menu.net_class.none"),
                t!("tree.menu.net_class.tip"),
            )
            .checked(current.is_none())
            .into(),
        );
        object.push(Entry::submenu(
            t!("tree.menu.net_class"),
            t!("tree.menu.net_class.tip"),
            Some(glyph::LAYERS),
            classes,
        ));
        object.push(
            row_item(
                RowAct::Highlight,
                t!("tree.menu.highlight"),
                t!("tree.menu.highlight.tip"),
                glyph::HIGHLIGHT,
            )
            .into(),
        );
    }
    object.push(
        row_item(
            RowAct::Properties,
            t!("tree.menu.properties"),
            t!("tree.menu.properties.tip"),
            glyph::PROPERTIES,
        )
        .into(),
    );
    if r.kind == RowKind::Layer && r.controls.is_none() && r.key != "root" {
        let open = view.expanded.contains(&r.key);
        shown.push(
            row_item(
                RowAct::ToggleExpand,
                if open {
                    t!("tree.menu.collapse")
                } else {
                    t!("tree.menu.expand")
                },
                t!("tree.chevron.tip"),
                if open {
                    kit::CHEVRON_DOWN
                } else {
                    kit::CHEVRON_RIGHT
                },
            )
            .into(),
        );
    }
    let mut bom: Vec<Entry<RowAct>> = Vec::new();
    if r.kind == RowKind::Part
        && let Some(id) = r.select
    {
        let component = env.project.footprint(id).map_or(id, |f| f.component);
        let excluded = env
            .project
            .component(component)
            .is_some_and(|c| !c.in_bom());
        bom.push(
            Item::new(
                RowAct::ExportBom,
                "Export BOM…",
                "A bill of materials is the shopping list for your board: every part, how many, and which part number to buy. Opens Export with all parts.",
            )
            .icon(glyph::PROPERTIES)
            .into(),
        );
        if env.parts_selected > 1 {
            bom.push(
                Item::new(
                    RowAct::ExportBomSelection,
                    "Export BOM for selection…",
                    "Write a bill of materials for only the parts you have selected.",
                )
                .icon(glyph::PROPERTIES)
                .into(),
            );
        }
        bom.push(
            Item::new(
                RowAct::ToggleBomExclude,
                if excluded { "Include in BOM" } else { "Exclude from BOM" },
                "Leave this part off the bill of materials, for example a test point or a logo. The bill is the shopping list for your board. Undo brings it back.",
            )
            .icon(glyph::PROPERTIES)
            .checked(excluded)
            .into(),
        );
    }
    let mut menu = Menu::new()
        .group(object)
        .group(edit)
        .group(bom)
        .group(shown);
    for item in destructive {
        menu = menu.destructive(item);
    }
    menu.entries()
}

/// Run the row picked in a tree row's menu.
///
/// # Rules
/// - Changes only `view` and the `actions` list; the caller applies the actions.
fn run_row_act(
    act: RowAct,
    env: &Env,
    view: &mut TreeView,
    r: &Row,
    (hidden, _locked, focus_on): (bool, bool, bool),
    actions: &mut Vec<Action>,
) {
    match act {
        RowAct::Active => {
            if let Some(target) = r.target {
                actions.push(Action::SetTarget(target));
                view.selected_key = Some(r.key.clone());
                actions.push(Action::SelectKey(r.key.clone()));
            }
        }
        RowAct::ToggleVisibility => {
            if let Some(ctl) = &r.controls {
                match &ctl.visibility {
                    Visibility::Canvas(name) => actions.push(Action::SetShown { name, on: hidden }),
                    Visibility::LayerGroup(side) => actions.push(Action::SetLayerGroup {
                        side: side.clone(),
                        on: hidden,
                    }),
                    Visibility::SubLayer { side, sub } => actions.push(Action::SetSubLayer {
                        side: side.clone(),
                        sub: *sub,
                        on: hidden,
                    }),
                    Visibility::Art(layer) => actions.push(Action::SetArtShown {
                        layer: *layer,
                        on: hidden,
                    }),
                    Visibility::Page(page, row) => actions.push(Action::SetPageRowShown {
                        page: *page,
                        row,
                        on: hidden,
                    }),
                    Visibility::None => {}
                }
            }
        }
        RowAct::ToggleLock => {
            if let Some(ctl) = &r.controls
                && !view.locked.remove(&ctl.key)
            {
                view.locked.insert(ctl.key.clone());
            }
        }
        RowAct::ToggleFocus => {
            if let Some(ctl) = &r.controls {
                actions.push(Action::Focus(if focus_on {
                    None
                } else {
                    Some(ctl.key.clone())
                }));
            }
        }
        RowAct::RemoveArt => {
            if let Some(ctl) = &r.controls
                && let Visibility::Art(id) = ctl.visibility
            {
                actions.push(Action::RemoveArtLayer(id));
            }
        }
        RowAct::Select => {
            if let Some(id) = r.select {
                actions.push(Action::Select(id));
            }
        }
        RowAct::Delete => {
            if let Some(id) = r.select {
                actions.push(Action::Delete(id));
            }
        }
        RowAct::ZoomTo => {
            if let Some(id) = r.select {
                actions.push(Action::ZoomTo(id));
            }
        }
        RowAct::Flip => {
            if let Some(id) = r.select {
                actions.push(Action::FlipSide(id));
            }
        }
        RowAct::Rename => view.rename = rename_for(env, r),
        RowAct::PageAdd => actions.push(Action::PageAdd),
        RowAct::PageRemove => {
            if let Some(page) = r.page {
                actions.push(Action::PageRemove(page));
            }
        }
        RowAct::NetClass(class) => {
            if let Some(net) = r.net {
                actions.push(Action::NetClass { net, class });
            }
        }
        RowAct::SelectAll => {
            if let Some((page, row)) = r.sheet_row {
                actions.push(Action::SelectAllInRow { page, row });
            }
        }
        RowAct::AddPowerPort => {
            if let Some((page, _)) = r.sheet_row {
                actions.push(Action::AddPowerPort(page));
            }
        }
        RowAct::TitleToggle => {
            if let Some((page, _)) = r.sheet_row {
                let shown = !env.project.page_settings_of(page).title_hidden;
                actions.push(Action::TitleShown { page, on: !shown });
            }
        }
        RowAct::Remove => {}
        RowAct::Properties => match (r.select, &r.stack) {
            (None, None) if r.sheet_row.is_some() => {
                if let Some((page, row)) = r.sheet_row {
                    view.selected_key = Some(r.key.clone());
                    actions.push(Action::SelectSheetRow { page, row });
                }
            }
            (_, Some(_)) => {
                view.selected_key = Some(r.key.clone());
                actions.push(Action::SelectKey(r.key.clone()));
            }
            (Some(id), None) => actions.push(Action::Select(id)),
            (None, None) => {
                view.selected_key = Some(r.key.clone());
                actions.push(Action::SelectKey(r.key.clone()));
            }
        },
        RowAct::Highlight => {
            if let Some(net) = r.net {
                actions.push(Action::HighlightNet(Some(net)));
            }
        }
        RowAct::ExportBom => actions.push(Action::ExportBom { selection: false }),
        RowAct::ExportBomSelection => actions.push(Action::ExportBom { selection: true }),
        RowAct::ToggleBomExclude => {
            if let Some(id) = r.select {
                actions.push(Action::ToggleBomExclude(id));
            }
        }
        RowAct::ToggleExpand => toggle(view, &r.key),
        RowAct::AddPair => {
            if let Some(name) = &r.stack {
                actions.push(Action::StackAddPair(name.clone()));
            }
        }
        RowAct::StackRename => {
            if let Some(name) = &r.stack {
                view.rename = Some((RenameFor::Layer(name.clone()), name.clone()));
            }
        }
        RowAct::MoveUp | RowAct::MoveDown => {
            if let Some(name) = &r.stack {
                actions.push(Action::StackMove {
                    name: name.clone(),
                    up: act == RowAct::MoveUp,
                });
            }
        }
        RowAct::RemoveLayer => {
            if let Some(name) = &r.stack {
                actions.push(Action::StackRemove(name.clone()));
            }
        }
    }
}

fn toggle(view: &mut TreeView, key: &str) {
    if !view.expanded.remove(key) {
        view.expanded.insert(key.to_owned());
    }
}

/// Plain words for what a tree row is and what it is for, shown when the
/// pointer rests on it. Every kind of row has one, in the string catalog.
///
/// # Rules
/// - Chosen by the row's last key segment, so a net, a pad or a layer reads
///   the same wherever it sits in the tree.
/// - Falls back to the generic row hint only for a segment no builder makes;
///   `every_row_kind_explains_itself` walks a full tree to prove there is none.
///
/// # Dependencies
/// - `crates/text/strings/en.txt` holds the `tree.explain.*` strings.
///
/// # Do not
/// - Describe the click action here; the action hints live on the controls.
pub(crate) fn explain(tab: TreeTab, r: &Row) -> String {
    let last = r.key.rsplit('/').next().unwrap_or("");
    let nested = r.key.contains('/');
    let head = last.split(':').next().unwrap_or(last);
    if r.page.is_some() {
        return t!("tree.explain.page");
    }
    match (head, last) {
        (_, "layer:symbols") => t!("tree.explain.symbols"),
        (_, "layer:power") => t!("tree.explain.power"),
        (_, "layer:drawing") => t!("tree.explain.drawing"),
        (_, "layer:title") => t!("tree.explain.title"),
        (_, "root") => t!("tree.explain.board"),
        ("side" | "inner", _) => t!("tree.explain.copper_layer"),
        ("diel", _) => t!("tree.explain.dielectric"),
        (_, "copper") => t!("tree.explain.copper"),
        (_, "mask") => t!("tree.explain.mask"),
        (_, "silk") => t!("tree.explain.silk"),
        (_, "paste") => t!("tree.explain.paste"),
        (_, "components") if tab == TreeTab::Layout && nested => t!("tree.explain.components"),
        (_, "components") if tab == TreeTab::Schematic => t!("tree.explain.parts_sheet"),
        (_, "components") => t!("tree.explain.parts"),
        (_, "drills") => t!("tree.explain.drills"),
        ("drill", _) => t!("tree.explain.drill_size"),
        ("via", _) => t!("tree.explain.via"),
        ("hole", _) => t!("tree.explain.hole"),
        (_, "edge") => t!("tree.explain.edge"),
        ("cut", _) => t!("tree.explain.cutout"),
        (_, "user") => t!("tree.explain.user"),
        ("art", _) => t!("tree.explain.art"),
        ("shape", _) | ("gfx", _) => t!("tree.explain.shape"),
        ("text", _) => t!("tree.explain.text"),
        (_, "refs") => t!("tree.explain.refs"),
        (_, "outlines") => t!("tree.explain.outlines"),
        (_, "nets") => t!("tree.explain.nets"),
        ("net", _) => t!("tree.explain.net"),
        ("fp", _) | ("comp", _) => t!("tree.explain.part"),
        ("pad", _) => t!("tree.explain.pad"),
        ("pin", _) | ("npin", _) => t!("tree.explain.pin"),
        ("track", _) => t!("tree.explain.track"),
        ("zone", _) => t!("tree.explain.zone"),
        ("wire", _) => t!("tree.explain.wire"),
        ("layer", _) => t!("tree.explain.net_layer"),
        ("label", _) if r.key.contains("layer:power") => t!("tree.explain.power_port"),
        ("label", _) => t!("tree.explain.label"),
        _ => String::new(),
    }
}

/// The result of clicking or pressing Enter on a row.
fn activate(view: &mut TreeView, r: &Row, actions: &mut Vec<Action>) {
    if let Some(page) = r.page {
        view.selected_key = Some(r.key.clone());
        actions.push(Action::SelectPage(page));
        toggle(view, &r.key);
        return;
    }
    if let Some(id) = r.select {
        view.selected_key = None;
        actions.push(Action::Select(id));
        return;
    }
    if let Some((page, row)) = r.sheet_row {
        view.selected_key = Some(r.key.clone());
        actions.push(Action::SelectSheetRow { page, row });
        if r.expandable {
            toggle(view, &r.key);
        }
        return;
    }
    if r.kind == RowKind::Section {
        toggle(view, &r.key);
        return;
    }
    view.selected_key = Some(r.key.clone());
    if let Some(target) = r.target {
        actions.push(Action::SetTarget(target));
        if !matches!(target, Target::Edge | Target::User(_)) {
            actions.push(Action::PinView(target));
        }
    }
    actions.push(Action::SelectKey(r.key.clone()));
    actions.push(Action::HighlightNet(r.net));
    if r.expandable && !r.key.ends_with("/root") {
        toggle(view, &r.key);
    }
}

fn keyboard(ui: &mut Ui, env: &Env, view: &mut TreeView, all: &[Row], actions: &mut Vec<Action>) {
    if !view.has_focus || all.is_empty() {
        return;
    }
    let current = view
        .cursor
        .as_ref()
        .and_then(|k| all.iter().position(|r| &r.key == k))
        .unwrap_or(0);
    let press =
        |ui: &mut Ui, key: egui::Key| ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, key));
    let go = |view: &mut TreeView, index: usize| {
        let index = index.min(all.len() - 1);
        let key = all[index].key.clone();
        view.cursor = Some(key.clone());
        view.focus_pending = Some(key.clone());
        view.scroll_to = Some(key);
    };
    let row = &all[current];
    if press(ui, egui::Key::ArrowDown) {
        go(view, current + 1);
    } else if press(ui, egui::Key::ArrowUp) {
        go(view, current.saturating_sub(1));
    } else if press(ui, egui::Key::Home) {
        go(view, 0);
    } else if press(ui, egui::Key::End) {
        go(view, all.len() - 1);
    } else if press(ui, egui::Key::ArrowRight) {
        if row.expandable && !view.expanded.contains(&row.key) {
            view.expanded.insert(row.key.clone());
        } else if all.get(current + 1).is_some_and(|n| n.depth > row.depth) {
            go(view, current + 1);
        }
    } else if press(ui, egui::Key::ArrowLeft) {
        if row.expandable && view.expanded.contains(&row.key) {
            view.expanded.remove(&row.key);
        } else if let Some(parent) = all[..current].iter().rposition(|p| p.depth < row.depth) {
            go(view, parent);
        }
    } else if press(ui, egui::Key::F2) {
        if let Some(rename) = rename_for(env, row) {
            view.rename = Some(rename);
        }
    } else if press(ui, egui::Key::Enter) {
        activate(view, row, actions);
    } else if press(ui, egui::Key::Space) {
        if let Some(ctl) = layer_of(all, current)
            && ctl.visibility != Visibility::None
        {
            let off = !stored_visible(env, &ctl);
            match ctl.visibility {
                Visibility::Canvas(name) => actions.push(Action::SetShown { name, on: off }),
                Visibility::LayerGroup(side) => actions.push(Action::SetLayerGroup { side, on: off }),
                Visibility::SubLayer { side, sub } => {
                    actions.push(Action::SetSubLayer { side, sub, on: off });
                }
                Visibility::Art(layer) => actions.push(Action::SetArtShown { layer, on: off }),
                Visibility::Page(page, row) => {
                    actions.push(Action::SetPageRowShown {
                        page,
                        row,
                        on: off,
                    });
                }
                Visibility::None => {}
            }
        }
    } else if press(ui, egui::Key::L)
        && let Some(ctl) = layer_of(all, current)
        && !ctl.key.is_empty()
    {
        if !view.locked.remove(&ctl.key) {
            view.locked.insert(ctl.key);
        }
    } else if press(ui, egui::Key::F)
        && let Some(ctl) = layer_of(all, current)
        && ctl.focusable
    {
        let on = env.focus == Some(ctl.key.as_str());
        actions.push(Action::Focus(if on { None } else { Some(ctl.key) }));
    }
}

/// The controls of the layer row that contains row `index` (itself or an
/// ancestor), so Space and F work from any row of a layer.
fn layer_of(all: &[Row], index: usize) -> Option<LayerControls> {
    if let Some(own) = &all[index].controls {
        return Some(own.clone());
    }
    all[..=index]
        .iter()
        .rev()
        .take_while(|r| r.kind != RowKind::Section)
        .find(|r| r.kind == RowKind::Layer && r.controls.is_some())
        .and_then(|r| r.controls.clone())
}

impl crate::App {
    /// True when a tree row should appear for the Nets list: no search is
    /// active, or the text contains the search.
    pub(crate) fn tree_matches(&self, text: &str) -> bool {
        let query = self.tree.query.trim().to_lowercase();
        query.is_empty() || text.to_lowercase().contains(&query)
    }

    /// The Layers tab of the left aside: draws the tree and applies what the
    /// user did in it.
    ///
    /// # Dependencies
    /// - `crates/ui-egui/src/layers_tree.rs` `show` builds and draws the rows.
    ///
    /// # Do not
    /// - Edit the project here except through `run` and engine commands.
    pub(crate) fn layers_tree_panel(&mut self, ui: &mut Ui) {
        let colours = self.colours();
        let tab = if self.tab == crate::Tab::Schematic {
            TreeTab::Schematic
        } else {
            TreeTab::Layout
        };
        let actions = {
            let env = Env {
                project: &self.project,
                tab,
                colours,
                hidden: &self.preferences.hidden_layers,
                focus: self.focus_layer.as_deref(),
                selected: self.selected,
                target: self.current_target(),
                revision: self.revision,
                parts_selected: self.selected_part_components().len(),
                show_empty: self.tree.show_empty,
            };
            show(ui, &env, &mut self.tree)
        };
        for action in actions {
            match action {
                Action::Select(id) => {
                    self.select(Some(id));
                    if !matches!(self.tab, crate::Tab::Pcb | crate::Tab::Schematic) {
                        self.change_tab(crate::Tab::Pcb);
                    }
                }
                Action::SelectKey(_) => self.select(None),
                Action::SetShown { name, on } => {
                    layer_visibility::set_legacy_canvas(&mut self.preferences.hidden_layers, name, on);
                }
                Action::SetLayerGroup { side, on } => {
                    layer_visibility::set_group_stored(
                        &mut self.preferences.hidden_layers,
                        &side,
                        on,
                    );
                }
                Action::SetSubLayer { side, sub, on } => {
                    layer_visibility::set_sub_stored(
                        &mut self.preferences.hidden_layers,
                        &side,
                        sub,
                        on,
                    );
                }
                Action::SetHidden(names) => {
                    let all = layer_names(tab);
                    let hidden = &mut self.preferences.hidden_layers;
                    if tab == TreeTab::Layout {
                        hidden.retain(|h| {
                            !h.starts_with("group:")
                                && layer_visibility::legacy_group_for_name(h).is_none()
                        });
                        for side in layer_visibility::layout_layer_groups() {
                            let hide = names.iter().any(|n| {
                                layer_visibility::legacy_group_for_name(n)
                                    .is_some_and(|s| s == side)
                                    || *n == side
                            });
                            layer_visibility::set_group_stored(hidden, side, !hide);
                        }
                        for name in &names {
                            if layer_visibility::legacy_group_for_name(name).is_none() {
                                layer_visibility::set_legacy_canvas(hidden, name, false);
                            }
                        }
                    } else {
                        hidden.retain(|h| !all.contains(&h.as_str()) && !h.starts_with("page:"));
                        hidden.extend(names.into_iter().map(str::to_owned));
                    }
                    let ids: Vec<(Id, bool)> = self
                        .project
                        .board
                        .user_layers
                        .iter()
                        .map(|l| (l.id, l.visible))
                        .collect();
                    if tab == TreeTab::Layout || hidden.len() < all.len() {
                        for (id, shown) in ids {
                            if !shown {
                                self.run(
                                    "board.layer_visible_set",
                                    serde_json::json!({"layer": id, "on": true}),
                                );
                            }
                        }
                    }
                }
                Action::SetArtShown { layer, on } => {
                    self.run(
                        "board.layer_visible_set",
                        serde_json::json!({"layer": layer, "on": on}),
                    );
                }
                Action::Focus(key) => self.focus_layer = key,
                Action::Delete(id) => {
                    self.select(Some(id));
                    self.delete_selection();
                }
                Action::RemoveArtLayer(id) => {
                    self.run("board.layer_delete", serde_json::json!({"layer": id}));
                }
                Action::SetTarget(target) => self.set_target(target),
                Action::PinView(target) => self.pin_view(target),
                Action::StackAddPair(name) => {
                    self.run("board.stackup_add_pair", serde_json::json!({"after": name}));
                }
                Action::StackRename { name, new_name } => {
                    self.run(
                        "board.stackup_rename_layer",
                        serde_json::json!({"name": name, "new_name": new_name}),
                    );
                }
                Action::StackRemove(name) => {
                    self.run(
                        "board.stackup_remove_layer",
                        serde_json::json!({"name": name}),
                    );
                    self.tree.selected_key = None;
                }
                Action::StackMove { name, up } => {
                    self.run(
                        "board.stackup_move_layer",
                        serde_json::json!({"name": name, "direction": if up { "up" } else { "down" }}),
                    );
                }
                Action::ZoomTo(id) => {
                    if let Some(f) = self.project.footprint(id) {
                        self.pcb.center = f.position;
                        self.pcb.zoom = self.pcb.zoom.max(24.0);
                    }
                    self.select(Some(id));
                }
                Action::ExportBom { selection } => {
                    let parts = if selection {
                        self.selected_part_components()
                    } else {
                        vec![]
                    };
                    self.open_dialog(crate::file_dialogs::Mode::Export, Some("bom"), None, parts);
                }
                Action::ToggleBomExclude(id) => {
                    let component = self.project.footprint(id).map_or(id, |f| f.component);
                    let exclude = !self.part_excluded(id);
                    self.set_bom_exclusion(&[component], exclude);
                }
                Action::FlipSide(id) => {
                    self.run("footprint.flip", serde_json::json!({"id": id}));
                }
                Action::NetClass { net, class } => {
                    self.run(
                        "netclass.assign",
                        serde_json::json!({"net": net, "class": class}),
                    );
                }
                Action::NetRename { net, new_name } => {
                    self.run(
                        "net.rename",
                        serde_json::json!({"id": net, "name": new_name}),
                    );
                }
                Action::ArtRename { layer, new_name } => {
                    self.run(
                        "board.layer_rename",
                        serde_json::json!({"layer": layer, "name": new_name}),
                    );
                }
                Action::SelectPage(id) => self.show_page(id),
                Action::PageAdd => self.add_page(),
                Action::PageRemove(id) => self.remove_page(id),
                Action::PageRename { id, new_name } => self.rename_page(id, &new_name),
                Action::HighlightNet(net) => self.highlight_net = net,
                Action::SetPageRowShown { page, row, on } => {
                    self.set_page_row_shown(page, row, on);
                }
                Action::SelectSheetRow { page, row } => self.select_sheet_row(page, row),
                Action::SelectAllInRow { page, row } => self.select_all_in_row(page, row),
                Action::TitleShown { page, on } => {
                    self.set_page_setting(page, "title_shown", if on { "true" } else { "false" });
                }
                Action::AddPowerPort(page) => self.add_power_port(page),
                Action::NewArtLayer => {
                    let name = t!(
                        "tree.new_layer_name",
                        n = self.project.board.user_layers.len() + 1
                    );
                    self.run(
                        "board.layer_add",
                        serde_json::json!({"name": name, "kind": "other"}),
                    );
                    if self.message.starts_with("board.layer_add")
                        && let Some(id) = self.selected
                    {
                        self.set_target(Target::User(id));
                    }
                }
            }
        }
    }

    /// The page whose card Properties previews while the pointer rests on its
    /// Layers row, or None. A page already selected is shown in full instead.
    ///
    /// # Dependencies
    /// - `crates/ui-egui/src/lib.rs` `inspector_body` asks before drawing the selection.
    pub(crate) fn hovered_page_preview(&self) -> Option<Id> {
        let page = self.tree.hover_page?;
        let key = Self::page_row_key(page);
        (self.tree.selected_key.as_deref() != Some(key.as_str())).then_some(page)
    }

    /// Properties for a tree row that is not one document object: the board
    /// outline (size, stackup, finish, origin) or a net (its connection editor).
    ///
    /// # Dependencies
    /// - `crates/ui-egui/src/lib.rs` `stackup_section`, `holes_section`, `datums_section`,
    ///   `fiducials_section`, `via_spans_section` and `nets_tab` draw the sections.
    pub(crate) fn tree_row_properties(&mut self, ui: &mut Ui) {
        let Some(key) = self.tree.selected_key.clone() else {
            return;
        };
        let last = key.rsplit('/').next().unwrap_or("");
        if let Some((page, row)) = key
            .strip_prefix("sec:sheet:")
            .and_then(|rest| rest.split_once("/layer:"))
        {
            let page: Id = page.parse().unwrap_or(0);
            self.sheet_row_properties(ui, page, row);
        } else if let Some(page) = key.strip_prefix("sec:sheet:") {
            self.page_properties(ui, page.parse().unwrap_or(0), false);
        } else if key == "root" || key.ends_with("/root") {
            self.board_root_properties(ui);
        } else if last.starts_with("side:")
            || last.starts_with("inner:")
            || last.starts_with("diel:")
        {
            self.stack_properties(ui, last);
        } else if last.starts_with("net:") || key == "nets" {
            ui.label(crate::fonts::title(t!("tree.net_settings")));
            self.nets_tab(ui);
        } else {
            let name = self
                .tree
                .cursor
                .as_deref()
                .and_then(|k| k.rsplit('/').next())
                .map(|s| s.rsplit(':').next().unwrap_or(s).to_owned())
                .unwrap_or_default();
            ui.label(crate::fonts::title(name));
            ui.label(
                egui::RichText::new(t!("tree.row_properties.help"))
                    .font(crate::fonts::Role::Caption.font())
                    .color(self.muted()),
            );
        }
    }
}

impl crate::App {
    /// Properties of a stackup row: its name, thickness and material, saved
    /// with `board.set_layer_record` when the field is left.
    ///
    /// # Dependencies
    /// - `crates/engine/src/lib.rs` `board.set_layer_record` stores the change.
    fn stack_properties(&mut self, ui: &mut Ui, seg: &str) {
        let records = &self.project.board.layer_records;
        let copper: Vec<&pcbcraft_doc::LayerRecord> =
            records.iter().filter(|r| r.kind == "copper").collect();
        let rec = if let Some(n) = seg.strip_prefix("diel:") {
            n.parse::<usize>().ok().and_then(|i| records.get(i))
        } else if let Some(n) = seg.strip_prefix("inner:") {
            n.parse::<usize>().ok().and_then(|i| copper.get(i).copied())
        } else if seg.starts_with("side:bottom") {
            copper.last().copied()
        } else {
            copper.first().copied()
        };
        let Some(rec) = rec.cloned() else {
            ui.heading(t!("tree.stack.default"));
            ui.label(
                egui::RichText::new(t!("tree.stack.default.help"))
                    .small()
                    .color(self.muted()),
            );
            return;
        };
        ui.heading(&rec.name);
        ui.label(
            egui::RichText::new(t!("tree.stack.help"))
                .small()
                .color(self.muted()),
        );
        ui.add_space(8.0);
        let key = egui::Id::new(("stack-edit", rec.name.clone()));
        let mut thickness: f64 = ui
            .data(|d| d.get_temp(key.with("t")))
            .unwrap_or(rec.thickness);
        let mut material: String = ui
            .data(|d| d.get_temp(key.with("m")))
            .unwrap_or_else(|| rec.material.clone().unwrap_or_default());
        ui.label(t!("tree.stack.thickness"));
        let t_resp = ui
            .add(crate::number_entry::localized_active(
                egui::DragValue::new(&mut thickness)
                    .speed(0.01)
                    .range(0.0..=100.0)
                    .suffix(format!(" {}", t!("unit.mm"))),
                3,
            ))
            .on_hover_text(t!("tree.stack.thickness.tip"))
            .named(&t!("tree.stack.thickness"));
        ui.add_space(6.0);
        ui.label(t!("tree.stack.material"));
        let m_resp = ui
            .add(crate::targets::text_field(&mut material).desired_width(f32::INFINITY))
            .on_hover_text(t!("tree.stack.material.tip"))
            .named(&t!("tree.stack.material"));
        let commit = t_resp.drag_stopped() || t_resp.lost_focus() || m_resp.lost_focus();
        if commit {
            ui.data_mut(|d| {
                d.remove::<f64>(key.with("t"));
                d.remove::<String>(key.with("m"));
            });
            let mut params = serde_json::json!({
                "name": rec.name,
                "kind": rec.kind,
                "thickness": thickness,
            });
            if !material.trim().is_empty() {
                params["material"] = serde_json::json!(material.trim());
            }
            if let Some(role) = &rec.copper_role {
                params["copper_role"] = serde_json::json!(role);
            }
            if thickness != rec.thickness
                || material.trim() != rec.material.clone().unwrap_or_default()
            {
                self.run("board.set_layer_record", params);
            }
        } else {
            ui.data_mut(|d| {
                d.insert_temp(key.with("t"), thickness);
                d.insert_temp(key.with("m"), material);
            });
        }
    }
}

/// The Layers region tab: draws the tree for the active workspace (Layout or
/// Schematic) and applies what the user did in it.
///
/// # Dependencies
/// - `crates/ui-egui/src/layers_tree.rs` `App::layers_tree_panel` does the work.
pub(crate) fn layers_tab(ui: &mut Ui, app: &mut crate::App) {
    app.layers_tree_panel(ui);
}

/// A board size in millimetres to two decimals with trailing zeros dropped,
/// so an imported 37.94230200896579 reads 37.94 and 60.0 reads 60.
fn size_text(v: f64) -> String {
    let t = format!("{v:.2}");
    t.trim_end_matches('0').trim_end_matches('.').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Project {
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = crate::App::create(shared, crate::Preferences::default(), None, None);
        app.tool_call("sample_design", serde_json::json!({}));
        app.project.clone()
    }

    fn env<'a>(project: &'a Project, hidden: &'a [String]) -> Env<'a> {
        Env {
            project,
            tab: TreeTab::Layout,
            colours: crate::palette::palette(true, [0, 200, 160]),
            hidden,
            focus: None,
            selected: None,
            target: Target::Copper(Layer::Front),
            revision: 1,
            parts_selected: 0,
show_empty: false,
            hidden,
            focus: None,
            selected: None,
            target: Target::Copper(Layer::Front),
            revision: 1,
            parts_selected: 0,
        }
    }

    #[test]
    fn renamable_rows_are_nets_layers_and_pages_only() {
        let mut project = sample();
        project.sheets.push(pcbcraft_doc::Sheet {
            interchange_uuid: None,
            id: 9001,
            name: "Power".into(),
            parent: None,
            entry: None,
        });
        let mut view = TreeView::default();
        view.expanded.insert("nets".to_owned());
        let hidden: Vec<String> = Vec::new();
        let layout = rows(&env(&project, &hidden), &mut view);
        let kind_of = |key: &str| {
            layout
                .iter()
                .find(|r| r.key == key)
                .and_then(|r| rename_for(&env(&project, &hidden), r))
                .map(|(what, _)| what)
        };
        assert!(matches!(kind_of("side:top"), Some(RenameFor::Layer(_))));
        assert!(matches!(
            kind_of("nets/net:1"),
            Some(RenameFor::Net(_)) | None
        ));
        assert_eq!(
            kind_of("edge"),
            None,
            "the board edge has no name to change"
        );
        let mut schematic = env(&project, &hidden);
        schematic.tab = TreeTab::Schematic;
        let mut view = TreeView::default();
        let list = rows(&schematic, &mut view);
        let page = |key: &str| {
            list.iter()
                .find(|r| r.key == key)
                .and_then(|r| rename_for(&schematic, r))
                .map(|(what, _)| what)
        };
        assert_eq!(page("sec:sheet:9001"), Some(RenameFor::Page(9001)));
        assert_eq!(page("sec:sheet:0"), None, "Main cannot be renamed");
    }

    #[test]
    fn the_stack_reads_top_to_bottom() {
        let project = sample();
        let mut view = TreeView::default();
        let list = rows(&env(&project, &[]), &mut view);
        let tops: Vec<&str> = list
            .iter()
            .filter(|r| r.depth == 0)
            .map(|r| r.key.as_str())
            .collect();
        assert_eq!(
            tops,
            [
                "root",
                "side:top",
                "side:bottom",
                "drills",
                "edge",
                "user",
                "components",
                "nets"
            ]
        );
        assert!(list.iter().any(|r| r.key == "side:top/copper"));
    }

    #[test]
    fn only_expanded_branches_are_built() {
        let project = sample();
        let mut view = TreeView::default();
        view.expanded.remove("components");
        let closed = rows(&env(&project, &[]), &mut view).len();
        view.expanded.insert("components".to_owned());
        let open = rows(&env(&project, &[]), &mut view).len();
        assert_eq!(open, closed + project.board.footprints.len());
    }

    #[test]
    fn a_search_keeps_matches_and_their_ancestors() {
        let project = sample();
        let net = project.nets[0].name.clone();
        let mut view = TreeView {
            query: net.clone(),
            ..Default::default()
        };
        let list = rows(&env(&project, &[]), &mut view);
        assert!(
            list.iter()
                .any(|r| r.kind == RowKind::Net && r.label == net)
        );
        assert!(list.iter().any(|r| r.key == "nets"));
        view = TreeView {
            query: "zzzz-no-such-thing".to_owned(),
            ..view
        };
        assert!(rows(&env(&project, &[]), &mut view).is_empty());
    }

    #[test]
    fn a_part_selected_on_the_canvas_opens_its_row() -> Result<(), String> {
        let project = sample();
        let id = project.board.footprints[0].id;
        let (opens, target) = reveal_path(&env(&project, &[]), id).ok_or("no row")?;
        assert_eq!(target, format!("components/fp:{id}"));
        assert!(opens.contains(&"components".to_owned()));
        let mut view = TreeView::default();
        view.expanded.extend(opens);
        assert!(
            rows(&env(&project, &[]), &mut view)
                .iter()
                .any(|r| r.key == target)
        );
        Ok(())
    }

    #[test]
    fn locking_a_layer_locks_its_tracks_only() {
        let project = sample();
        let mut locked = BTreeSet::new();
        locked.insert("copper:front".to_owned());
        if let Some(t) = project
            .board
            .tracks
            .iter()
            .find(|t| t.layer == Layer::Front)
        {
            assert!(is_locked(&project, &locked, t.id));
        }
        assert!(!is_locked(
            &project,
            &locked,
            project.board.footprints[0].id
        ));
    }

    #[test]
    fn the_board_outline_row_opens_board_settings() -> Result<(), String> {
        let project = sample();
        let mut view = TreeView::default();
        let list = rows(&env(&project, &[]), &mut view);
        let outline = list
            .iter()
            .find(|r| r.key == "root")
            .ok_or("no outline row")?;
        let mut actions = Vec::new();
        activate(&mut view, outline, &mut actions);
        assert_eq!(view.selected_key.as_deref(), Some("root"));
        assert!(actions.contains(&Action::SelectKey("root".to_owned())));
        Ok(())
    }

    /// Open every expandable row, round after round, until no new row appears.
    fn fully_open(env: &Env, view: &mut TreeView) -> Vec<Row> {
        for _ in 0..12 {
            let all = rows(env, view);
            let before = view.expanded.len();
            view.expanded
                .extend(all.iter().filter(|r| r.expandable).map(|r| r.key.clone()));
            if view.expanded.len() == before {
                break;
            }
        }
        rows(env, view).as_ref().clone()
    }

    #[test]
    fn every_row_kind_explains_itself() {
        // One of everything a row can stand for, so no builder escapes the walk.
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = crate::App::create(shared, crate::Preferences::default(), None, None);
        app.tool_call("sample_design", serde_json::json!({}));
        app.tool_call(
            "graphic.add",
            serde_json::json!({"layer":"schematic","shape":"line","points":[{"x":1.0,"y":1.0},{"x":9.0,"y":1.0}]}),
        );
        let gnd = app
            .project
            .nets
            .iter()
            .find(|n| n.name == "GND")
            .map_or(1, |n| n.id);
        app.tool_call(
            "label.place",
            serde_json::json!({"kind":"power","net":gnd,"position":{"x":5.0,"y":5.0}}),
        );
        app.tool_call(
            "board.layer_add",
            serde_json::json!({"name":"Notes","kind":"other"}),
        );
        let project = app.project.clone();
        let hidden: Vec<String> = Vec::new();
        for tab in [TreeTab::Layout, TreeTab::Schematic] {
            let mut env = env(&project, &hidden);
            env.tab = tab;
            env.show_empty = true;
            let mut view = TreeView::default();
            let all = fully_open(&env, &mut view);
            assert!(all.len() > 10, "{tab:?} tree has rows to check");
            let missing: Vec<String> = all
                .iter()
                .filter(|r| explain(tab, r).trim().is_empty())
                .map(|r| format!("{} ({})", r.key, r.label))
                .collect();
            assert!(
                missing.is_empty(),
                "rows with no explanation on {tab:?}: {missing:?}"
            );
        }
    }

    #[test]
    fn an_explanation_says_what_the_thing_is() {
        let project = sample();
        let hidden: Vec<String> = Vec::new();
        let mut env = env(&project, &hidden);
        env.tab = TreeTab::Layout;
        let mut view = TreeView::default();
        let all = fully_open(&env, &mut view);
        let mask = all
            .iter()
            .find(|r| r.key.ends_with("/mask"))
            .map(|r| explain(TreeTab::Layout, r))
            .unwrap_or_default();
        assert_eq!(
            mask,
            "The coating over the copper. Openings in it expose pads for soldering."
        );
    }

    #[test]
    fn empty_page_rows_hide_unless_shown() {
        let project = Project::default();
        let hidden: Vec<String> = Vec::new();
        let mut env = env(&project, &hidden);
        env.tab = TreeTab::Schematic;
        let page = crate::App::page_row_key(0);
        let listed: Vec<String> = children(&env, &page)
            .iter()
            .map(|r| r.label.clone())
            .collect();
        assert_eq!(
            listed,
            vec![t!("tree.sch.title_block")],
            "a page with nothing on it lists only its title block"
        );
        env.show_empty = true;
        let shown: Vec<String> = children(&env, &page)
            .iter()
            .map(|r| r.label.clone())
            .collect();
        assert_eq!(shown.len(), 4, "Show empty lists all four: {shown:?}");
    }

    #[test]
    fn two_thousand_parts_build_quickly() {
        let mut project = sample();
        let template = project.board.footprints[0].clone();
        let component = project.components[0].clone();
        for i in 0..2000u64 {
            let mut f = template.clone();
            f.id = 1_000_000 + i;
            f.component = component.id;
            project.board.footprints.push(f);
        }
        let mut view = TreeView::default();
        view.expanded.insert("components".to_owned());
        let e = env(&project, &[]);
        let start = pcbcraft_clock::instant_now();
        let list = rows(&e, &mut view);
        let first = start.elapsed();
        let start = pcbcraft_clock::instant_now();
        let again = rows(&e, &mut view);
        let cached = start.elapsed();
        assert!(list.len() > 2000 && again.len() == list.len());
        assert!(first.as_millis() < 1500, "first build took {first:?}");
        assert!(cached.as_millis() < 50, "cached build took {cached:?}");
    }
}
