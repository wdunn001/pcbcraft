//! Tool modifiers: the one place that turns raw Shift, Ctrl and Alt state, the
//! thing under the pointer and the active tool into an action.
//!
//! Three layers share one table. (1) Click-target actions: with the Select tool
//! the object you press decides the action (drag from a pin starts a wire, drag
//! empty canvas box-selects). (2) Hotkeys pick tools (W wire, X route, V via,
//! T text, Esc select) and are remappable. (3) Per-tool modifier semantics
//! (Shift constrains angle, Ctrl or Cmd suspends snap, Alt toggles a second
//! option, Ctrl+click deletes in the Wire and Route tools).
//!
//! The design follows the owner's sibling app SolidCraft (its `tool_modifiers`
//! module: one resolver, classic defaults, Ctrl as primary off macOS and Cmd on
//! macOS). The action table is PCB and schematic specific and coded fresh here.
//!
//! # Rules
//! - Canvas and tool code never read `modifiers.shift`, `.ctrl` or `.alt`; they
//!   build a [`Held`] with [`Held::new`] or [`held_now`] and ask this module.
//!   A scan test (`no_raw_modifier_reads_outside_resolver`) enforces it.
//! - Action ids are persisted in the preferences: never rename one, only add.
//! - Pure logic: no `App`, no painting. egui is used only for `Modifiers`,
//!   `Key` and `Context::os`.
//!
//! # Dependencies
//! - `crates/ui-egui/src/modifier_ui.rs` words, tooltips, preferences page.
//! - `crates/ui-egui/src/canvas/gestures.rs` applies the resolved actions.
//! - `crates/ui-egui/src/lib.rs` `Preferences::modifiers` stores [`Bindings`].
//!
//! # Do not
//! - Add a binding without a row in `tool_table_covers_every_tool` and a
//!   tooltip line in `modifier_ui.rs`.

use std::cell::Cell;
use std::collections::BTreeMap;

use egui::{Key, Modifiers};
use serde::{Deserialize, Serialize};

/// Runtime platform for primary-key normalisation and labels.
///
/// Prefer [`Platform::of`] over `cfg!(target_os)` so the browser build on a
/// Mac still says Cmd.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    Mac,
    Other,
}

impl Platform {
    /// The platform the egui context reports.
    pub fn of(ctx: &egui::Context) -> Self {
        match ctx.os() {
            egui::os::OperatingSystem::Mac | egui::os::OperatingSystem::IOS => Self::Mac,
            _ => Self::Other,
        }
    }
    /// Compile-time fallback for unit tests and call sites without a context.
    pub fn host() -> Self {
        if cfg!(target_os = "macos") {
            Self::Mac
        } else {
            Self::Other
        }
    }
}

/// Held keys, normalised for chord matches.
///
/// `primary` is Ctrl off macOS and Cmd on macOS. On macOS the physical Ctrl
/// key is `other` and never counts as primary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Held {
    pub shift: bool,
    pub primary: bool,
    pub alt: bool,
    /// A key no chord names (physical Ctrl on macOS). Fails exact matches.
    pub other: bool,
}

impl Held {
    /// Normalise egui modifiers for a platform.
    pub fn new(m: Modifiers, platform: Platform) -> Self {
        let primary = m.command || (platform != Platform::Mac && m.ctrl);
        Self {
            shift: m.shift,
            primary,
            alt: m.alt,
            other: platform == Platform::Mac && m.ctrl && !m.command,
        }
    }
    /// True when no modifier is held.
    pub fn none(self) -> bool {
        !self.shift && !self.primary && !self.alt
    }
}

/// The keys held right now, from the egui input of this frame.
pub fn held_now(ctx: &egui::Context) -> Held {
    Held::new(ctx.input(|i| i.modifiers), Platform::of(ctx))
}

/// A set of modifier keys for a binding. `ctrl` means primary (Ctrl, or Cmd on
/// macOS); the serde name stays `ctrl` so saved preferences stay valid.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ModifierCombo {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
}

impl ModifierCombo {
    pub const SHIFT: Self = Self {
        shift: true,
        ctrl: false,
        alt: false,
    };
    pub const CTRL: Self = Self {
        shift: false,
        ctrl: true,
        alt: false,
    };
    pub const ALT: Self = Self {
        shift: false,
        ctrl: false,
        alt: true,
    };
    pub const SHIFT_CTRL: Self = Self {
        shift: true,
        ctrl: true,
        alt: false,
    };
    pub const SHIFT_ALT: Self = Self {
        shift: true,
        ctrl: false,
        alt: true,
    };
    /// The chords the preferences page offers for any action.
    pub const CHOICES: [Self; 5] = [
        Self::SHIFT,
        Self::CTRL,
        Self::ALT,
        Self::SHIFT_CTRL,
        Self::SHIFT_ALT,
    ];

    /// How many keys the chord names; the more keys, the more specific.
    pub const fn key_count(self) -> u8 {
        self.shift as u8 + self.ctrl as u8 + self.alt as u8
    }
    /// True when at least these keys are held.
    pub fn matches(self, held: Held) -> bool {
        !held.other
            && (!self.shift || held.shift)
            && (!self.ctrl || held.primary)
            && (!self.alt || held.alt)
    }
    /// Words for the chord on a platform, such as `Ctrl/Cmd+Shift`.
    pub fn label_for(self, platform: Platform) -> String {
        let mut parts: Vec<&str> = Vec::new();
        if self.shift {
            parts.push("Shift");
        }
        if self.ctrl {
            parts.push(primary_name(platform));
        }
        if self.alt {
            parts.push(alt_name(platform));
        }
        if parts.is_empty() {
            "None".into()
        } else {
            parts.join("+")
        }
    }
}

/// "Ctrl" off macOS, "Cmd" on macOS.
pub fn primary_name(platform: Platform) -> &'static str {
    match platform {
        Platform::Mac => "Cmd",
        Platform::Other => "Ctrl",
    }
}
/// "Alt" off macOS, "Option" on macOS.
pub fn alt_name(platform: Platform) -> &'static str {
    match platform {
        Platform::Mac => "Option",
        Platform::Other => "Alt",
    }
}

/// The tools the resolver knows, mirroring the app's tool enum without data.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ToolKind {
    Select,
    Place,
    Wire,
    Route,
    Via,
    Move,
    Cutout,
    Hole,
    NoConnect,
    Fiducial,
    DrawLayer,
    Measure,
}

impl ToolKind {
    pub const ALL: [Self; 12] = [
        Self::Select,
        Self::Place,
        Self::Wire,
        Self::Route,
        Self::Via,
        Self::Move,
        Self::Cutout,
        Self::Hole,
        Self::NoConnect,
        Self::Fiducial,
        Self::DrawLayer,
        Self::Measure,
    ];
}

/// Which editor the pointer is over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Workspace {
    Schematic,
    Board,
}

/// Semantic modifier actions. `id()` is persisted in the preferences.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum ModAction {
    SelectAdd = 0,
    SelectToggle = 1,
    PickUnder = 2,
    ConstrainAngle = 3,
    SuspendSnap = 4,
    ConstrainAxis = 5,
    DuplicateDrag = 6,
    DragNoSnap = 7,
    PanHorizontal = 8,
    FineZoom = 9,
    DeleteConnection = 10,
    RemoveSegment = 11,
    WireCorner = 12,
    RouteCorner = 13,
    ShoveToggle = 14,
}

impl ModAction {
    pub const ALL: [Self; 15] = [
        Self::SelectAdd,
        Self::SelectToggle,
        Self::PickUnder,
        Self::ConstrainAngle,
        Self::SuspendSnap,
        Self::ConstrainAxis,
        Self::DuplicateDrag,
        Self::DragNoSnap,
        Self::PanHorizontal,
        Self::FineZoom,
        Self::DeleteConnection,
        Self::RemoveSegment,
        Self::WireCorner,
        Self::RouteCorner,
        Self::ShoveToggle,
    ];
    /// Persisted id. Never rename; only add.
    pub const fn id(self) -> &'static str {
        match self {
            Self::SelectAdd => "select_add",
            Self::SelectToggle => "select_toggle",
            Self::PickUnder => "pick_under",
            Self::ConstrainAngle => "constrain_angle",
            Self::SuspendSnap => "suspend_snap",
            Self::ConstrainAxis => "constrain_axis",
            Self::DuplicateDrag => "duplicate_drag",
            Self::DragNoSnap => "drag_no_snap",
            Self::PanHorizontal => "pan_horizontal",
            Self::FineZoom => "fine_zoom",
            Self::DeleteConnection => "delete_connection",
            Self::RemoveSegment => "remove_segment",
            Self::WireCorner => "wire_corner",
            Self::RouteCorner => "route_corner",
            Self::ShoveToggle => "shove_toggle",
        }
    }
    /// The action with this persisted id.
    #[cfg(test)]
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|a| a.id() == id)
    }
}

/// When a binding is sampled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// Latched at the click or press.
    Click,
    /// While a drag is under way.
    Drag,
    /// Every frame the tool is active.
    Live,
    /// While the wheel turns.
    Wheel,
}

/// One binding: an action, its classic default chords and when it applies.
#[derive(Clone, Copy, Debug)]
pub struct Binding {
    pub action: ModAction,
    /// Any one of these chords activates the action.
    pub defaults: &'static [ModifierCombo],
    pub phase: Phase,
}

const fn bind(action: ModAction, defaults: &'static [ModifierCombo], phase: Phase) -> Binding {
    Binding {
        action,
        defaults,
        phase,
    }
}

const S: &[ModifierCombo] = &[ModifierCombo::SHIFT];
const C: &[ModifierCombo] = &[ModifierCombo::CTRL];
const A: &[ModifierCombo] = &[ModifierCombo::ALT];

/// Bindings every tool shares: the wheel.
pub const WHEEL_BINDINGS: &[Binding] = &[
    bind(ModAction::PanHorizontal, S, Phase::Wheel),
    bind(ModAction::FineZoom, C, Phase::Wheel),
];

const SELECT_BINDINGS: &[Binding] = &[
    bind(ModAction::SelectAdd, S, Phase::Click),
    bind(ModAction::SelectToggle, C, Phase::Click),
    bind(ModAction::PickUnder, A, Phase::Click),
    bind(ModAction::ConstrainAxis, S, Phase::Drag),
    bind(ModAction::DuplicateDrag, C, Phase::Drag),
    bind(ModAction::DragNoSnap, A, Phase::Drag),
];
const WIRE_BINDINGS: &[Binding] = &[
    bind(ModAction::DeleteConnection, C, Phase::Click),
    bind(ModAction::ConstrainAngle, S, Phase::Live),
    bind(ModAction::WireCorner, A, Phase::Live),
    bind(ModAction::SuspendSnap, C, Phase::Live),
];
const ROUTE_BINDINGS: &[Binding] = &[
    bind(ModAction::RemoveSegment, C, Phase::Click),
    bind(ModAction::RouteCorner, S, Phase::Live),
    bind(ModAction::ShoveToggle, A, Phase::Live),
    bind(ModAction::SuspendSnap, C, Phase::Live),
];
const MOVE_BINDINGS: &[Binding] = &[
    bind(ModAction::ConstrainAxis, S, Phase::Live),
    bind(ModAction::DuplicateDrag, C, Phase::Live),
    bind(ModAction::DragNoSnap, A, Phase::Live),
];
const PLACE_BINDINGS: &[Binding] = &[
    bind(ModAction::ConstrainAngle, S, Phase::Live),
    bind(ModAction::SuspendSnap, C, Phase::Live),
];
const SNAP_ONLY_BINDINGS: &[Binding] = &[bind(ModAction::SuspendSnap, C, Phase::Live)];
const NONE: &[Binding] = &[];

/// How far a tool's modifier bindings have been reviewed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Audit {
    Implemented,
    /// Reviewed: this tool has no modifier behaviour beyond the wheel.
    VerifiedNoOp,
}

/// The modifier bindings of one tool.
pub fn tool_bindings(tool: ToolKind) -> (&'static [Binding], Audit) {
    match tool {
        ToolKind::Select => (SELECT_BINDINGS, Audit::Implemented),
        ToolKind::Wire => (WIRE_BINDINGS, Audit::Implemented),
        ToolKind::Route => (ROUTE_BINDINGS, Audit::Implemented),
        ToolKind::Move => (MOVE_BINDINGS, Audit::Implemented),
        ToolKind::DrawLayer | ToolKind::Measure => (PLACE_BINDINGS, Audit::Implemented),
        ToolKind::Place
        | ToolKind::Via
        | ToolKind::Cutout
        | ToolKind::Hole
        | ToolKind::Fiducial => (SNAP_ONLY_BINDINGS, Audit::Implemented),
        ToolKind::NoConnect => (NONE, Audit::VerifiedNoOp),
    }
}

/// The user's remapped chords and hotkeys. Empty means classic defaults.
///
/// Stored in the preferences, so a change takes effect on the next frame and
/// survives a restart.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bindings {
    /// Action id to chords; an empty list switches the action off.
    #[serde(default)]
    chords: BTreeMap<String, Vec<ModifierCombo>>,
    /// Hotkey action id to the key's name.
    #[serde(default)]
    keys: BTreeMap<String, String>,
}

impl Bindings {
    /// The chords for an action: the user's, or the binding's classic defaults.
    pub fn chords_for(&self, binding: &Binding) -> Vec<ModifierCombo> {
        self.chords
            .get(binding.action.id())
            .cloned()
            .unwrap_or_else(|| binding.defaults.to_vec())
    }
    /// True when the user changed this action from its default.
    pub fn is_custom(&self, action: ModAction) -> bool {
        self.chords.contains_key(action.id())
    }
    /// Give an action one chord, or `None` to switch it off.
    pub fn set_chord(&mut self, action: ModAction, chord: Option<ModifierCombo>) {
        self.chords
            .insert(action.id().into(), chord.into_iter().collect());
    }
    /// Back to the classic default for one action.
    pub fn reset_chord(&mut self, action: ModAction) {
        self.chords.remove(action.id());
    }
    /// The key that picks a tool: the user's, or the default.
    pub fn key_for(&self, hotkey: Hotkey) -> Key {
        self.keys
            .get(hotkey.id())
            .and_then(|name| Key::from_name(name))
            .unwrap_or_else(|| hotkey.default_key())
    }
    /// Remap a hotkey. Refused when another hotkey or a reserved key owns it.
    ///
    /// # Rules
    /// - Returns the reason in words so the preferences page can show it.
    pub fn set_key(&mut self, hotkey: Hotkey, key: Key) -> Result<(), String> {
        if RESERVED_KEYS.contains(&key) {
            return Err(format!("{} is already used by the editor.", key.name()));
        }
        for other in Hotkey::ALL {
            if other != hotkey && self.key_for(other) == key {
                return Err(format!(
                    "{} already picks the {} tool.",
                    key.name(),
                    other.id()
                ));
            }
        }
        self.keys.insert(hotkey.id().into(), key.name().into());
        Ok(())
    }
    /// Back to the default key for one hotkey.
    pub fn reset_key(&mut self, hotkey: Hotkey) {
        self.keys.remove(hotkey.id());
    }
    /// Back to the classic defaults everywhere.
    pub fn reset_all(&mut self) {
        self.chords.clear();
        self.keys.clear();
    }
}

/// Keys the editor already uses, so a hotkey cannot shadow them.
pub const RESERVED_KEYS: [Key; 8] = [
    Key::A,
    Key::M,
    Key::R,
    Key::E,
    Key::Home,
    Key::Slash,
    Key::Delete,
    Key::Enter,
];

/// The actions active for a tool and phase with these held keys.
pub fn active(bindings: &Bindings, tool: ToolKind, held: Held, phase: Phase) -> ActionSet {
    let mut set = ActionSet::default();
    let wheel = WHEEL_BINDINGS.iter().filter(|_| phase == Phase::Wheel);
    for b in tool_bindings(tool)
        .0
        .iter()
        .filter(|b| b.phase == phase || (phase == Phase::Drag && b.phase == Phase::Live))
        .chain(wheel)
    {
        if bindings.chords_for(b).iter().any(|c| c.matches(held)) {
            set.insert(b.action);
        }
    }
    set
}

/// The size of the chord that fired `action`, for most-specific-wins.
fn matched_keys(bindings: &Bindings, tool: ToolKind, held: Held, action: ModAction) -> u8 {
    tool_bindings(tool)
        .0
        .iter()
        .chain(WHEEL_BINDINGS)
        .filter(|b| b.action == action)
        .flat_map(|b| bindings.chords_for(b))
        .filter(|c| c.matches(held))
        .map(|c| c.key_count())
        .max()
        .unwrap_or(0)
}

/// Bit set of active [`ModAction`]s.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ActionSet(u32);

impl ActionSet {
    pub fn insert(&mut self, a: ModAction) {
        self.0 |= 1 << (a as u8);
    }
    pub fn contains(self, a: ModAction) -> bool {
        self.0 & (1 << (a as u8)) != 0
    }
    #[cfg(test)]
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
}

/// What the pointer is over.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Target {
    Empty,
    /// A symbol on the schematic or a footprint on the board.
    Part,
    /// A schematic pin.
    Pin,
    /// A board pad.
    Pad,
    WireSegment,
    TrackSegment,
    /// The end of a track.
    TrackEnd,
    Via,
    /// A vertex of a drawn wire.
    Node,
    Junction,
    NetLabel,
    /// Anything else selectable: zone, cutout, graphic, text.
    Other,
}

/// The pointer gesture being resolved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gesture {
    Click,
    Drag,
    DoubleClick,
}

/// What a gesture on a target does right now.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ClickAction {
    /// A click replaces the selection.
    Replace,
    /// Shift+click adds to the selection.
    Add,
    /// Ctrl or Cmd+click toggles membership.
    Toggle,
    /// Alt+click picks the item underneath, cycling overlaps.
    PickUnder,
    StartWire,
    StartRoute,
    ContinueWire,
    ContinueRoute,
    MoveSegment,
    MovePart,
    BoxSelect,
    RenameLabel,
    DeleteConnection,
    RemoveSegment,
    /// The active tool's own click (place, drill, measure ...).
    ToolDefault,
    Nothing,
}

impl ClickAction {}

/// One row of the click-target table.
#[derive(Clone, Copy, Debug)]
pub struct Row {
    pub tool: ToolKind,
    /// `None` matches either workspace.
    pub workspace: Option<Workspace>,
    /// Empty matches every target.
    pub targets: &'static [Target],
    pub gesture: Gesture,
    /// The modifier action that must be active; `None` is the plain gesture.
    pub needs: Option<ModAction>,
    pub out: ClickAction,
}

const fn row(
    tool: ToolKind,
    workspace: Option<Workspace>,
    targets: &'static [Target],
    gesture: Gesture,
    needs: Option<ModAction>,
    out: ClickAction,
) -> Row {
    Row {
        tool,
        workspace,
        targets,
        gesture,
        needs,
        out,
    }
}

use ClickAction as CA;
use Gesture::{Click, DoubleClick, Drag};
use ModAction as MA;
use Target as T;
use ToolKind as TK;
use Workspace::{Board, Schematic};

const ANY: &[Target] = &[];

/// The click-target table: first the modifier rows, then the plain gestures.
/// The most specific held chord wins; ties go to the earlier row.
pub const ROWS: &[Row] = &[
    // Select tool, selection modifiers on a click.
    row(
        TK::Select,
        None,
        ANY,
        Click,
        Some(MA::PickUnder),
        CA::PickUnder,
    ),
    row(
        TK::Select,
        None,
        ANY,
        Click,
        Some(MA::SelectToggle),
        CA::Toggle,
    ),
    row(TK::Select, None, ANY, Click, Some(MA::SelectAdd), CA::Add),
    // Select tool, plain click.
    row(
        TK::Select,
        Some(Schematic),
        &[T::Pin],
        Click,
        None,
        CA::StartWire,
    ),
    row(
        TK::Select,
        Some(Board),
        &[T::Pad],
        Click,
        None,
        CA::StartRoute,
    ),
    row(TK::Select, None, ANY, Click, None, CA::Replace),
    // Select tool, press and drag: what you press on decides.
    row(
        TK::Select,
        Some(Schematic),
        &[T::Pin],
        Drag,
        None,
        CA::StartWire,
    ),
    row(
        TK::Select,
        Some(Board),
        &[T::Pad, T::TrackEnd],
        Drag,
        None,
        CA::StartRoute,
    ),
    row(
        TK::Select,
        None,
        &[T::WireSegment, T::TrackSegment],
        Drag,
        None,
        CA::MoveSegment,
    ),
    row(
        TK::Select,
        None,
        &[T::Part, T::Via],
        Drag,
        None,
        CA::MovePart,
    ),
    row(TK::Select, None, &[T::Empty], Drag, None, CA::BoxSelect),
    row(
        TK::Select,
        None,
        &[T::NetLabel],
        DoubleClick,
        None,
        CA::RenameLabel,
    ),
    // Wire tool.
    row(
        TK::Wire,
        Some(Schematic),
        &[T::Node, T::Junction],
        Click,
        Some(MA::DeleteConnection),
        CA::DeleteConnection,
    ),
    row(
        TK::Wire,
        Some(Schematic),
        &[T::Pin],
        Click,
        None,
        CA::ContinueWire,
    ),
    row(TK::Wire, None, ANY, Click, None, CA::ToolDefault),
    // Route tool.
    row(
        TK::Route,
        Some(Board),
        &[T::TrackEnd, T::Via],
        Click,
        Some(MA::RemoveSegment),
        CA::RemoveSegment,
    ),
    row(
        TK::Route,
        Some(Board),
        &[T::Pad],
        Click,
        None,
        CA::ContinueRoute,
    ),
    row(TK::Route, None, ANY, Click, None, CA::ToolDefault),
    // Every other tool: its own click.
    row(TK::Place, None, ANY, Click, None, CA::ToolDefault),
    row(TK::Via, None, ANY, Click, None, CA::ToolDefault),
    row(TK::Move, None, ANY, Click, None, CA::ToolDefault),
    row(TK::Cutout, None, ANY, Click, None, CA::ToolDefault),
    row(TK::Hole, None, ANY, Click, None, CA::ToolDefault),
    row(TK::NoConnect, None, ANY, Click, None, CA::ToolDefault),
    row(TK::Fiducial, None, ANY, Click, None, CA::ToolDefault),
    row(TK::DrawLayer, None, ANY, Click, None, CA::ToolDefault),
    row(TK::Measure, None, ANY, Click, None, CA::ToolDefault),
];

/// What a gesture on a target does, for the active tool, workspace and keys.
///
/// Why: this is the only place that decides it, so the canvas, the status bar,
/// the cursor and the tooltips can never disagree.
///
/// # Rules
/// - Among matching rows the one whose modifier chord has the most keys wins;
///   ties go to the earlier row in [`ROWS`].
/// - No matching row means [`ClickAction::Nothing`].
///
/// # Dependencies
/// - `crates/ui-egui/src/canvas/gestures.rs` calls it for press, click and hover.
///
/// # Do not
/// - Read egui modifiers in the caller; pass a [`Held`].
pub fn resolve(
    bindings: &Bindings,
    tool: ToolKind,
    workspace: Workspace,
    held: Held,
    target: Target,
    gesture: Gesture,
) -> ClickAction {
    let phase = match gesture {
        Click | DoubleClick => Phase::Click,
        Drag => Phase::Drag,
    };
    let set = active(bindings, tool, held, phase);
    let mut best: Option<(u8, ClickAction)> = None;
    for r in ROWS {
        if r.tool != tool
            || r.gesture != gesture
            || r.workspace.is_some_and(|w| w != workspace)
            || !(r.targets.is_empty() || r.targets.contains(&target))
        {
            continue;
        }
        let score = match r.needs {
            None => 0,
            Some(a) if set.contains(a) => matched_keys(bindings, tool, held, a),
            Some(_) => continue,
        };
        if best.is_none_or(|(s, _)| score > s) {
            best = Some((score, r.out));
        }
    }
    best.map_or(ClickAction::Nothing, |(_, out)| out)
}

/// The tools a hotkey picks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Hotkey {
    Select,
    Wire,
    Route,
    Via,
    Text,
}

impl Hotkey {
    pub const ALL: [Self; 5] = [Self::Select, Self::Wire, Self::Route, Self::Via, Self::Text];
    pub const fn id(self) -> &'static str {
        match self {
            Self::Select => "select",
            Self::Wire => "wire",
            Self::Route => "route",
            Self::Via => "via",
            Self::Text => "text",
        }
    }
    /// The classic key.
    pub const fn default_key(self) -> Key {
        match self {
            Self::Select => Key::Escape,
            Self::Wire => Key::W,
            Self::Route => Key::X,
            Self::Via => Key::V,
            Self::Text => Key::T,
        }
    }
    /// Which workspace the hotkey works in; `None` is both.
    pub const fn workspace(self) -> Option<Workspace> {
        match self {
            Self::Select => None,
            Self::Wire => Some(Workspace::Schematic),
            Self::Route | Self::Via | Self::Text => Some(Workspace::Board),
        }
    }
}

/// The hotkey a pressed key means in a workspace, if any.
#[cfg(test)]
pub fn hotkey_for_key(bindings: &Bindings, key: Key, workspace: Workspace) -> Option<Hotkey> {
    Hotkey::ALL
        .into_iter()
        .find(|h| bindings.key_for(*h) == key && h.workspace().is_none_or(|w| w == workspace))
}

/// Two bindings of one tool that share a chord and would fire together.
pub fn conflicts(bindings: &Bindings) -> Vec<(ToolKind, ModAction, ModAction)> {
    let mut out = Vec::new();
    for tool in ToolKind::ALL {
        let list = tool_bindings(tool).0;
        for (i, a) in list.iter().enumerate() {
            for b in &list[i + 1..] {
                // Click bindings resolve by most-specific chord, and different
                // phases never fire together, so only same-phase live pairs clash.
                if a.phase != b.phase || a.phase == Phase::Click {
                    continue;
                }
                let overlap = bindings
                    .chords_for(a)
                    .iter()
                    .any(|c| bindings.chords_for(b).contains(c));
                if overlap && a.action != b.action {
                    out.push((tool, a.action, b.action));
                }
            }
        }
    }
    out
}

thread_local! {
    static SNAP_SUSPENDED: Cell<bool> = const { Cell::new(false) };
}

/// Set once per frame by the canvas before anything snaps.
pub fn set_snap_suspended(on: bool) {
    SNAP_SUSPENDED.with(|c| c.set(on));
}
/// True while the held keys suspend grid and object snap.
pub fn snap_suspended() -> bool {
    SNAP_SUSPENDED.with(Cell::get)
}

/// Whether snap is suspended: the tool's live and drag snap actions.
pub fn suspends_snap(bindings: &Bindings, tool: ToolKind, held: Held, dragging: bool) -> bool {
    let live = active(bindings, tool, held, Phase::Live);
    let drag = if dragging {
        active(bindings, tool, held, Phase::Drag)
    } else {
        ActionSet::default()
    };
    live.contains(ModAction::SuspendSnap)
        || live.contains(ModAction::DragNoSnap)
        || drag.contains(ModAction::DragNoSnap)
}

/// Snap `to` to the nearest 45 degree direction from `from`, keeping the
/// distance along that direction.
pub fn constrain_angle(from: (f64, f64), to: (f64, f64)) -> (f64, f64) {
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    if dx == 0.0 && dy == 0.0 {
        return to;
    }
    let step = std::f64::consts::FRAC_PI_4;
    let angle = (dy.atan2(dx) / step).round() * step;
    let along = dx * angle.cos() + dy * angle.sin();
    (from.0 + along * angle.cos(), from.1 + along * angle.sin())
}

/// Keep only the larger of the two offsets from `origin`: a horizontal or
/// vertical move.
pub fn constrain_axis(origin: (f64, f64), to: (f64, f64)) -> (f64, f64) {
    if (to.0 - origin.0).abs() >= (to.1 - origin.1).abs() {
        (to.0, origin.1)
    } else {
        (origin.0, to.1)
    }
}

/// How the wheel changes the view: pan sideways, zoom, or zoom in small steps.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WheelPlan {
    pub pan_horizontal: bool,
    /// Multiplier on the zoom rate; 1.0 normal, below 1.0 fine.
    pub zoom_scale: f32,
}

/// Plan the wheel for the held keys.
pub fn wheel_plan(bindings: &Bindings, tool: ToolKind, held: Held) -> WheelPlan {
    let set = active(bindings, tool, held, Phase::Wheel);
    WheelPlan {
        pan_horizontal: set.contains(ModAction::PanHorizontal),
        zoom_scale: if set.contains(ModAction::FineZoom) {
            0.25
        } else {
            1.0
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn held(shift: bool, primary: bool, alt: bool) -> Held {
        Held {
            shift,
            primary,
            alt,
            other: false,
        }
    }
    fn go(tool: ToolKind, ws: Workspace, h: Held, t: Target, g: Gesture) -> ClickAction {
        resolve(&Bindings::default(), tool, ws, h, t, g)
    }

    #[test]
    fn every_row_resolves_to_its_output() {
        for r in ROWS {
            let ws = r.workspace.unwrap_or(Workspace::Board);
            let targets: Vec<Target> = if r.targets.is_empty() {
                vec![Target::Empty]
            } else {
                r.targets.to_vec()
            };
            let h = match r.needs {
                None => Held::default(),
                Some(a) => {
                    let (list, _) = tool_bindings(r.tool);
                    let chord = list.iter().find(|b| b.action == a).map(|b| b.defaults[0]);
                    let c = chord.unwrap_or_default();
                    held(c.shift, c.ctrl, c.alt)
                }
            };
            for t in targets {
                assert_eq!(
                    go(r.tool, ws, h, t, r.gesture),
                    r.out,
                    "{:?} {:?} {:?} {:?}",
                    r.tool,
                    t,
                    r.gesture,
                    r.needs
                );
            }
        }
    }

    #[test]
    fn a_click_replaces_adds_toggles_and_picks_under() {
        let w = Workspace::Board;
        let c = |h| go(ToolKind::Select, w, h, Target::Part, Click);
        assert_eq!(c(Held::default()), CA::Replace);
        assert_eq!(c(held(true, false, false)), CA::Add);
        assert_eq!(c(held(false, true, false)), CA::Toggle);
        assert_eq!(c(held(false, false, true)), CA::PickUnder);
        // Several keys: the one with the most specific chord wins; ties go to the first row.
        assert_eq!(c(held(true, true, false)), CA::Toggle);
    }

    #[test]
    fn what_you_press_on_decides_the_drag() {
        let d = |ws, t| go(ToolKind::Select, ws, Held::default(), t, Drag);
        assert_eq!(d(Workspace::Schematic, Target::Pin), CA::StartWire);
        assert_eq!(d(Workspace::Board, Target::Pad), CA::StartRoute);
        assert_eq!(d(Workspace::Board, Target::TrackEnd), CA::StartRoute);
        assert_eq!(d(Workspace::Board, Target::TrackSegment), CA::MoveSegment);
        assert_eq!(
            d(Workspace::Schematic, Target::WireSegment),
            CA::MoveSegment
        );
        assert_eq!(d(Workspace::Schematic, Target::Part), CA::MovePart);
        assert_eq!(d(Workspace::Board, Target::Empty), CA::BoxSelect);
        assert_eq!(
            go(
                ToolKind::Select,
                Workspace::Schematic,
                Held::default(),
                Target::NetLabel,
                DoubleClick
            ),
            CA::RenameLabel
        );
    }

    #[test]
    fn wire_tool_ctrl_click_on_a_node_deletes_the_connection() {
        let w = Workspace::Schematic;
        let ctrl = held(false, true, false);
        assert_eq!(
            go(ToolKind::Wire, w, ctrl, Target::Node, Click),
            CA::DeleteConnection
        );
        assert_eq!(
            go(ToolKind::Wire, w, ctrl, Target::Junction, Click),
            CA::DeleteConnection
        );
        assert_eq!(
            go(ToolKind::Wire, w, Held::default(), Target::Node, Click),
            CA::ToolDefault
        );
        assert_eq!(
            go(ToolKind::Wire, w, Held::default(), Target::Pin, Click),
            CA::ContinueWire
        );
        // Ctrl on a pin does not delete; it only suspends snap.
        assert_eq!(
            go(ToolKind::Wire, w, ctrl, Target::Pin, Click),
            CA::ContinueWire
        );
    }

    #[test]
    fn route_tool_ctrl_click_removes_a_segment() {
        let b = Workspace::Board;
        let ctrl = held(false, true, false);
        assert_eq!(
            go(ToolKind::Route, b, ctrl, Target::TrackEnd, Click),
            CA::RemoveSegment
        );
        assert_eq!(
            go(ToolKind::Route, b, ctrl, Target::Via, Click),
            CA::RemoveSegment
        );
        assert_eq!(
            go(ToolKind::Route, b, Held::default(), Target::Pad, Click),
            CA::ContinueRoute
        );
    }

    #[test]
    fn the_tool_table_covers_every_tool() {
        for tool in ToolKind::ALL {
            let (list, audit) = tool_bindings(tool);
            if list.is_empty() {
                assert_eq!(audit, Audit::VerifiedNoOp, "{tool:?}");
            }
            assert!(
                ROWS.iter().any(|r| r.tool == tool),
                "{tool:?} needs a click row"
            );
        }
        for a in ModAction::ALL {
            assert_eq!(ModAction::from_id(a.id()), Some(a));
        }
    }

    #[test]
    fn live_modifiers_per_tool() {
        let b = Bindings::default();
        let live = |tool, h| active(&b, tool, h, Phase::Live);
        let shift = held(true, false, false);
        let ctrl = held(false, true, false);
        let alt = held(false, false, true);
        assert!(live(ToolKind::Wire, shift).contains(ModAction::ConstrainAngle));
        assert!(live(ToolKind::Wire, alt).contains(ModAction::WireCorner));
        assert!(live(ToolKind::Wire, ctrl).contains(ModAction::SuspendSnap));
        assert!(live(ToolKind::Route, shift).contains(ModAction::RouteCorner));
        assert!(live(ToolKind::Route, alt).contains(ModAction::ShoveToggle));
        assert!(live(ToolKind::Route, ctrl).contains(ModAction::SuspendSnap));
        assert!(live(ToolKind::Move, shift).contains(ModAction::ConstrainAxis));
        assert!(live(ToolKind::Move, ctrl).contains(ModAction::DuplicateDrag));
        assert!(live(ToolKind::Move, alt).contains(ModAction::DragNoSnap));
        assert!(live(ToolKind::DrawLayer, shift).contains(ModAction::ConstrainAngle));
        assert!(live(ToolKind::Place, ctrl).contains(ModAction::SuspendSnap));
        assert!(live(ToolKind::NoConnect, held(true, true, true)).is_empty());
        let drag = active(&b, ToolKind::Select, shift, Phase::Drag);
        assert!(drag.contains(ModAction::ConstrainAxis));
        assert!(active(&b, ToolKind::Select, ctrl, Phase::Drag).contains(ModAction::DuplicateDrag));
        assert!(active(&b, ToolKind::Select, alt, Phase::Drag).contains(ModAction::DragNoSnap));
    }

    #[test]
    fn the_wheel_pans_with_shift_and_zooms_finely_with_ctrl() {
        let b = Bindings::default();
        let p = |h| wheel_plan(&b, ToolKind::Select, h);
        assert_eq!(
            p(Held::default()),
            WheelPlan {
                pan_horizontal: false,
                zoom_scale: 1.0
            }
        );
        assert!(p(held(true, false, false)).pan_horizontal);
        assert!(p(held(false, true, false)).zoom_scale < 1.0);
    }

    #[test]
    fn cmd_is_primary_on_macos_and_ctrl_is_not() {
        let cmd = Modifiers {
            command: true,
            mac_cmd: true,
            ..Modifiers::NONE
        };
        let ctrl = Modifiers {
            ctrl: true,
            ..Modifiers::NONE
        };
        assert!(Held::new(cmd, Platform::Mac).primary);
        assert!(!Held::new(ctrl, Platform::Mac).primary);
        assert!(Held::new(ctrl, Platform::Mac).other);
        let toggle = |m, p| {
            resolve(
                &Bindings::default(),
                ToolKind::Select,
                Workspace::Board,
                Held::new(m, p),
                Target::Part,
                Click,
            )
        };
        assert_eq!(toggle(cmd, Platform::Mac), CA::Toggle);
        assert_eq!(toggle(ctrl, Platform::Mac), CA::Replace);
        // Off macOS a plain Ctrl is primary.
        assert_eq!(toggle(ctrl, Platform::Other), CA::Toggle);
        assert_eq!(ModifierCombo::CTRL.label_for(Platform::Mac), "Cmd");
        assert_eq!(ModifierCombo::ALT.label_for(Platform::Mac), "Option");
        assert_eq!(
            ModifierCombo::SHIFT_CTRL.label_for(Platform::Other),
            "Shift+Ctrl"
        );
    }

    #[test]
    fn remapping_takes_effect_and_survives_a_restart() {
        let mut b = Bindings::default();
        b.set_chord(ModAction::SelectAdd, Some(ModifierCombo::ALT));
        b.set_chord(ModAction::PickUnder, Some(ModifierCombo::SHIFT));
        let c = |b: &Bindings, h| {
            resolve(
                b,
                ToolKind::Select,
                Workspace::Board,
                h,
                Target::Part,
                Click,
            )
        };
        assert_eq!(c(&b, held(false, false, true)), CA::Add);
        assert_eq!(c(&b, held(true, false, false)), CA::PickUnder);
        b.set_chord(ModAction::SelectToggle, None);
        assert_eq!(c(&b, held(false, true, false)), CA::Replace);
        let saved = serde_json::to_string(&b).unwrap_or_default();
        let back: Bindings = serde_json::from_str(&saved).unwrap_or_default();
        assert_eq!(back, b);
        assert_eq!(c(&back, held(false, false, true)), CA::Add);
        b.reset_chord(ModAction::SelectAdd);
        assert_eq!(c(&b, held(true, false, false)), CA::PickUnder);
        b.reset_all();
        assert_eq!(b, Bindings::default());
    }

    #[test]
    fn hotkeys_pick_tools_and_can_be_remapped() {
        let mut b = Bindings::default();
        let k = |b: &Bindings, key, ws| hotkey_for_key(b, key, ws);
        assert_eq!(k(&b, Key::W, Workspace::Schematic), Some(Hotkey::Wire));
        assert_eq!(k(&b, Key::W, Workspace::Board), None);
        assert_eq!(k(&b, Key::X, Workspace::Board), Some(Hotkey::Route));
        assert_eq!(k(&b, Key::V, Workspace::Board), Some(Hotkey::Via));
        assert_eq!(k(&b, Key::T, Workspace::Board), Some(Hotkey::Text));
        assert_eq!(k(&b, Key::Escape, Workspace::Board), Some(Hotkey::Select));
        assert!(b.set_key(Hotkey::Wire, Key::Q).is_ok());
        assert_eq!(k(&b, Key::Q, Workspace::Schematic), Some(Hotkey::Wire));
        assert_eq!(k(&b, Key::W, Workspace::Schematic), None);
        assert!(b.set_key(Hotkey::Via, Key::X).is_err());
        assert!(b.set_key(Hotkey::Via, Key::M).is_err());
        let saved = serde_json::to_string(&b).unwrap_or_default();
        let back: Bindings = serde_json::from_str(&saved).unwrap_or_default();
        assert_eq!(back.key_for(Hotkey::Wire), Key::Q);
    }

    #[test]
    fn default_bindings_do_not_clash() {
        assert!(conflicts(&Bindings::default()).is_empty());
        let mut b = Bindings::default();
        b.set_chord(ModAction::WireCorner, Some(ModifierCombo::SHIFT));
        assert!(!conflicts(&b).is_empty());
    }

    #[test]
    fn angle_and_axis_constraints() {
        let (x, y) = constrain_angle((0.0, 0.0), (10.0, 1.0));
        assert!((x - 10.0).abs() < 1e-9 && y.abs() < 1e-9 + 0.0);
        let (x, y) = constrain_angle((0.0, 0.0), (5.0, 6.0));
        assert!((x - y).abs() < 1e-9, "45 degrees");
        assert_eq!(constrain_axis((0.0, 0.0), (5.0, 2.0)), (5.0, 0.0));
        assert_eq!(constrain_axis((0.0, 0.0), (1.0, 4.0)), (0.0, 4.0));
    }

    #[test]
    fn no_raw_modifier_reads_outside_resolver() {
        fn walk(dir: &std::path::Path, hits: &mut Vec<String>) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            for e in entries.flatten() {
                let path = e.path();
                if path.is_dir() {
                    walk(&path, hits);
                } else if path.extension().is_some_and(|x| x == "rs")
                    && path.file_name().is_some_and(|n| n != "tool_modifiers.rs")
                {
                    let text = std::fs::read_to_string(&path).unwrap_or_default();
                    for (n, line) in text.lines().enumerate() {
                        let flat: String = line.split_whitespace().collect();
                        if ["modifiers.shift", "modifiers.ctrl", "modifiers.alt"]
                            .iter()
                            .any(|p| flat.contains(p))
                        {
                            hits.push(format!("{}:{}", path.display(), n + 1));
                        }
                    }
                }
            }
        }
        let mut hits = Vec::new();
        walk(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
            &mut hits,
        );
        assert!(hits.is_empty(), "raw modifier reads: {hits:?}");
    }
}
