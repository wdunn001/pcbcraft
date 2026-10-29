//! Keyboard shortcut table: one source for dispatch, Help, and Settings.

use crate::Tab;
use egui::Key;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Stable action id stored in profiles and used for dispatch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyAction {
    Undo,
    Redo,
    Save,
    CommandPalette,
    ToggleShortcutsHelp,
    SymbolPalette,
    WireTool,
    RouteTool,
    MoveTool,
    Rotate,
    OpenInspector,
    DeleteSelection,
    ReturnToSelect,
    ZoomFitDesign,
    ZoomToSelection,
    ViaTool,
    FlipHorizontal,
    MirrorVertical,
    SelectConnected,
    ToggleNetHighlight,
    LayerNext,
    LayerPrev,
    RepeatLastCommand,
    FindInTree,
    PaletteLineUp,
    PaletteLineDown,
    PaletteRun,
    ToggleAutoSync,
    UpdateBoardFromSchematic,
    ToggleAssistant,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct KeyChord {
    pub key: Key,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) struct KeyChordStore {
    key: String,
    ctrl: bool,
    shift: bool,
    alt: bool,
}

impl KeyChord {
    fn store(self) -> KeyChordStore {
        KeyChordStore {
            key: storage_key_name(self.key),
            ctrl: self.ctrl,
            shift: self.shift,
            alt: self.alt,
        }
    }

    fn load(raw: KeyChordStore) -> Result<Self, String> {
        let key = parse_storage_key(&raw.key).ok_or_else(|| format!("unknown key {}", raw.key))?;
        Ok(Self {
            key,
            ctrl: raw.ctrl,
            shift: raw.shift,
            alt: raw.alt,
        })
    }
}

fn storage_key_name(key: Key) -> String {
    if key == Key::Slash {
        "slash".into()
    } else {
        key.name().to_string()
    }
}

fn parse_storage_key(name: &str) -> Option<Key> {
    if name == "slash" || name == "?" {
        return Some(Key::Slash);
    }
    Key::ALL.iter().find(|k| k.name() == name).copied()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ShortcutContext {
    Anywhere,
    Schematic,
    Pcb,
    CommandPalette,
}

struct ActionSpec {
    action: KeyAction,
    label: &'static str,
    tooltip: &'static str,
    context: ShortcutContext,
    rebindable: bool,
    default_chord: Option<KeyChord>,
}

const SPECS: &[ActionSpec] = &[
    ActionSpec {
        action: KeyAction::Undo,
        label: "Undo the last action",
        tooltip: "Step back one edit in the design.",
        context: ShortcutContext::Anywhere,
        rebindable: true,
        default_chord: Some(KeyChord {
            key: Key::Z,
            ctrl: true,
            shift: false,
            alt: false,
        }),
    },
    ActionSpec {
        action: KeyAction::Redo,
        label: "Redo an undone action",
        tooltip: "Replay the last edit you stepped back from.",
        context: ShortcutContext::Anywhere,
        rebindable: true,
        default_chord: Some(KeyChord {
            key: Key::Z,
            ctrl: true,
            shift: true,
            alt: false,
        }),
    },
    ActionSpec {
        action: KeyAction::Redo,
        label: "Redo an undone action",
        tooltip: "Replay the last edit you stepped back from.",
        context: ShortcutContext::Anywhere,
        rebindable: true,
        default_chord: Some(KeyChord {
            key: Key::Y,
            ctrl: true,
            shift: false,
            alt: false,
        }),
    },
    ActionSpec {
        action: KeyAction::Save,
        label: "Save the design to its file",
        tooltip: "Write the open project to disk at its current path.",
        context: ShortcutContext::Anywhere,
        rebindable: true,
        default_chord: Some(KeyChord {
            key: Key::S,
            ctrl: true,
            shift: false,
            alt: false,
        }),
    },
    ActionSpec {
        action: KeyAction::CommandPalette,
        label: "Open the command palette",
        tooltip: "Search every command by name and run it with parameters.",
        context: ShortcutContext::Anywhere,
        rebindable: true,
        default_chord: Some(KeyChord {
            key: Key::K,
            ctrl: true,
            shift: false,
            alt: false,
        }),
    },
    ActionSpec {
        action: KeyAction::ToggleShortcutsHelp,
        label: "Show or hide this shortcut list",
        tooltip: "Open or close the keyboard shortcut overlay.",
        context: ShortcutContext::Anywhere,
        rebindable: true,
        default_chord: Some(KeyChord {
            key: Key::Slash,
            ctrl: false,
            shift: false,
            alt: false,
        }),
    },
    ActionSpec {
        action: KeyAction::SymbolPalette,
        label: "Open the symbol palette",
        tooltip: "Pick a part to place on the schematic.",
        context: ShortcutContext::Schematic,
        rebindable: true,
        default_chord: Some(KeyChord {
            key: Key::A,
            ctrl: false,
            shift: false,
            alt: false,
        }),
    },
    ActionSpec {
        action: KeyAction::WireTool,
        label: "Wire tool: click two pins to connect them",
        tooltip: "Arm the wire tool on the schematic canvas.",
        context: ShortcutContext::Schematic,
        rebindable: true,
        default_chord: Some(KeyChord {
            key: Key::W,
            ctrl: false,
            shift: false,
            alt: false,
        }),
    },
    ActionSpec {
        action: KeyAction::RouteTool,
        label: "Route tool: click two pads to start a track",
        tooltip: "Arm the route tool on the board canvas.",
        context: ShortcutContext::Pcb,
        rebindable: true,
        default_chord: Some(KeyChord {
            key: Key::X,
            ctrl: false,
            shift: false,
            alt: false,
        }),
    },
    ActionSpec {
        action: KeyAction::MoveTool,
        label: "Move tool: drop the selection at the cursor",
        tooltip: "Pick up the selection and place it with the next click.",
        context: ShortcutContext::Anywhere,
        rebindable: true,
        default_chord: Some(KeyChord {
            key: Key::M,
            ctrl: false,
            shift: false,
            alt: false,
        }),
    },
    ActionSpec {
        action: KeyAction::Rotate,
        label: "Rotate the hovered or selected object by 90 degrees",
        tooltip: "Turn the selection a quarter turn clockwise.",
        context: ShortcutContext::Anywhere,
        rebindable: true,
        default_chord: Some(KeyChord {
            key: Key::R,
            ctrl: false,
            shift: false,
            alt: false,
        }),
    },
    ActionSpec {
        action: KeyAction::OpenInspector,
        label: "Load the hovered or selected object into the inspector",
        tooltip: "Show editable fields for the object under the cursor.",
        context: ShortcutContext::Anywhere,
        rebindable: true,
        default_chord: Some(KeyChord {
            key: Key::E,
            ctrl: false,
            shift: false,
            alt: false,
        }),
    },
    ActionSpec {
        action: KeyAction::ZoomFitDesign,
        label: "Zoom the view so the whole design fits",
        tooltip: "Frame the entire schematic or board in the work area.",
        context: ShortcutContext::Anywhere,
        rebindable: true,
        default_chord: Some(KeyChord {
            key: Key::Home,
            ctrl: false,
            shift: false,
            alt: false,
        }),
    },
    ActionSpec {
        action: KeyAction::ZoomFitDesign,
        label: "Zoom the view so the whole design fits",
        tooltip: "Frame the entire schematic or board in the work area.",
        context: ShortcutContext::Anywhere,
        rebindable: true,
        default_chord: Some(KeyChord {
            key: Key::F,
            ctrl: true,
            shift: false,
            alt: false,
        }),
    },
    ActionSpec {
        action: KeyAction::ZoomToSelection,
        label: "Zoom the view to the current selection",
        tooltip: "Frame just the selected symbol, footprint, track, or via.",
        context: ShortcutContext::Anywhere,
        rebindable: true,
        default_chord: Some(KeyChord {
            key: Key::Home,
            ctrl: false,
            shift: true,
            alt: false,
        }),
    },
    ActionSpec {
        action: KeyAction::DeleteSelection,
        label: "Delete the selected object",
        tooltip: "Remove the selected object from the design.",
        context: ShortcutContext::Anywhere,
        rebindable: true,
        default_chord: Some(KeyChord {
            key: Key::Delete,
            ctrl: false,
            shift: false,
            alt: false,
        }),
    },
    ActionSpec {
        action: KeyAction::ReturnToSelect,
        label: "Return to the select tool and close open palettes",
        tooltip: "Cancel the active tool and dismiss open panels.",
        context: ShortcutContext::Anywhere,
        rebindable: true,
        default_chord: Some(KeyChord {
            key: Key::Escape,
            ctrl: false,
            shift: false,
            alt: false,
        }),
    },
    ActionSpec {
        action: KeyAction::ViaTool,
        label: "Via tool: click to place a via on the board",
        tooltip: "Place a via at the cursor on the active copper layer.",
        context: ShortcutContext::Pcb,
        rebindable: true,
        default_chord: Some(KeyChord {
            key: Key::V,
            ctrl: false,
            shift: false,
            alt: false,
        }),
    },
    ActionSpec {
        action: KeyAction::FlipHorizontal,
        label: "Flip the selection horizontally",
        tooltip: "Mirror the selected footprint or symbol left to right.",
        context: ShortcutContext::Anywhere,
        rebindable: true,
        default_chord: Some(KeyChord {
            key: Key::H,
            ctrl: false,
            shift: false,
            alt: false,
        }),
    },
    ActionSpec {
        action: KeyAction::MirrorVertical,
        label: "Mirror the selection vertically",
        tooltip: "Mirror the selected footprint or symbol top to bottom.",
        context: ShortcutContext::Anywhere,
        rebindable: true,
        default_chord: Some(KeyChord {
            key: Key::V,
            ctrl: false,
            shift: true,
            alt: false,
        }),
    },
    ActionSpec {
        action: KeyAction::SelectConnected,
        label: "Step through copper on the same connection",
        tooltip: "Cycle the selection among tracks and vias on one net.",
        context: ShortcutContext::Pcb,
        rebindable: true,
        default_chord: Some(KeyChord {
            key: Key::C,
            ctrl: false,
            shift: false,
            alt: false,
        }),
    },
    ActionSpec {
        action: KeyAction::ToggleNetHighlight,
        label: "Toggle spotlight on the connection of the selection",
        tooltip: "Highlight or clear the net tied to the current selection.",
        context: ShortcutContext::Anywhere,
        rebindable: true,
        default_chord: Some(KeyChord {
            key: Key::N,
            ctrl: false,
            shift: false,
            alt: false,
        }),
    },
    ActionSpec {
        action: KeyAction::LayerNext,
        label: "Route on the next copper layer",
        tooltip: "Switch the active routing layer forward.",
        context: ShortcutContext::Pcb,
        rebindable: true,
        default_chord: Some(KeyChord {
            key: Key::CloseBracket,
            ctrl: false,
            shift: false,
            alt: false,
        }),
    },
    ActionSpec {
        action: KeyAction::LayerPrev,
        label: "Route on the previous copper layer",
        tooltip: "Switch the active routing layer backward.",
        context: ShortcutContext::Pcb,
        rebindable: true,
        default_chord: Some(KeyChord {
            key: Key::OpenBracket,
            ctrl: false,
            shift: false,
            alt: false,
        }),
    },
    ActionSpec {
        action: KeyAction::ToggleAutoSync,
        label: "Turn automatic board sync on or off",
        tooltip: "Toggle whether schematic edits update the board in the same undo step.",
        context: ShortcutContext::Anywhere,
        rebindable: true,
        default_chord: Some(KeyChord {
            key: Key::B,
            ctrl: true,
            shift: true,
            alt: false,
        }),
    },
    ActionSpec {
        action: KeyAction::UpdateBoardFromSchematic,
        label: "Update the board from the schematic",
        tooltip: "Apply pending schematic changes to the board when automatic sync is off.",
        context: ShortcutContext::Anywhere,
        rebindable: true,
        default_chord: Some(KeyChord {
            key: Key::B,
            ctrl: true,
            shift: false,
            alt: false,
        }),
    },
    ActionSpec {
        action: KeyAction::ToggleAssistant,
        label: "Show or hide the Assistant tab",
        tooltip: "Open the Assistant in the lower-right aside, or return to Properties.",
        context: ShortcutContext::Anywhere,
        rebindable: true,
        default_chord: Some(KeyChord {
            key: Key::A,
            ctrl: true,
            shift: true,
            alt: false,
        }),
    },
    ActionSpec {
        action: KeyAction::RepeatLastCommand,
        label: "Repeat the last command",
        tooltip: "Run the most recent successful edit command again.",
        context: ShortcutContext::Anywhere,
        rebindable: true,
        default_chord: Some(KeyChord {
            key: Key::Period,
            ctrl: false,
            shift: false,
            alt: false,
        }),
    },
    ActionSpec {
        action: KeyAction::FindInTree,
        label: "Focus the project tree search",
        tooltip: "Jump to the browser search box to find a part or net by name.",
        context: ShortcutContext::Anywhere,
        rebindable: true,
        default_chord: Some(KeyChord {
            key: Key::F,
            ctrl: true,
            shift: true,
            alt: false,
        }),
    },
    ActionSpec {
        action: KeyAction::PaletteLineUp,
        label: "Move the highlight up in the command list",
        tooltip: "Step the command palette selection toward the top.",
        context: ShortcutContext::CommandPalette,
        rebindable: true,
        default_chord: Some(KeyChord {
            key: Key::ArrowUp,
            ctrl: false,
            shift: false,
            alt: false,
        }),
    },
    ActionSpec {
        action: KeyAction::PaletteLineDown,
        label: "Move the highlight down in the command list",
        tooltip: "Step the command palette selection toward the bottom.",
        context: ShortcutContext::CommandPalette,
        rebindable: true,
        default_chord: Some(KeyChord {
            key: Key::ArrowDown,
            ctrl: false,
            shift: false,
            alt: false,
        }),
    },
    ActionSpec {
        action: KeyAction::PaletteRun,
        label: "Load the highlighted command into the parameter box",
        tooltip: "Copy the selected palette command so you can edit and run it.",
        context: ShortcutContext::CommandPalette,
        rebindable: true,
        default_chord: Some(KeyChord {
            key: Key::Enter,
            ctrl: false,
            shift: false,
            alt: false,
        }),
    },
];

const GESTURE_ROWS: &[(&str, &str, &str)] = &[
    ("Middle-drag", "Pan the view", "canvas"),
    ("Scroll", "Zoom the view", "canvas"),
];

fn spec(action: KeyAction) -> Option<&'static ActionSpec> {
    SPECS.iter().find(|s| s.action == action)
}

pub fn action_label(action: KeyAction) -> &'static str {
    spec(action).map(|s| s.label).unwrap_or("Unknown action")
}

pub fn action_tooltip(action: KeyAction) -> &'static str {
    spec(action)
        .map(|s| s.tooltip)
        .unwrap_or("No description yet.")
}

pub fn rebindable_actions() -> impl Iterator<Item = KeyAction> {
    SPECS
        .iter()
        .map(|s| s.action)
        .collect::<HashSet<_>>()
        .into_iter()
}

fn context_name(context: ShortcutContext) -> &'static str {
    match context {
        ShortcutContext::Anywhere => "anywhere",
        ShortcutContext::Schematic => "schematic tab",
        ShortcutContext::Pcb => "pcb tab",
        ShortcutContext::CommandPalette => "command palette",
    }
}

pub fn format_chord(chord: KeyChord) -> String {
    let mut parts: Vec<&str> = Vec::new();
    if chord.ctrl {
        parts.push("Ctrl");
    }
    if chord.shift {
        parts.push("Shift");
    }
    if chord.alt {
        parts.push("Alt");
    }
    parts.push(chord_key_name(chord.key, chord.shift));
    parts.join("+")
}

fn chord_key_name(key: Key, shift: bool) -> &'static str {
    match key {
        Key::Slash if !shift => "?",
        Key::ArrowUp => "Up",
        Key::ArrowDown => "Down",
        Key::OpenBracket => "[",
        Key::CloseBracket => "]",
        Key::Period => ".",
        Key::Home => "Home",
        Key::Delete => "Delete",
        Key::Escape => "Esc",
        Key::Enter => "Enter",
        other => other.name(),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Keymap {
    bindings: HashMap<KeyAction, KeyChord>,
    extra_chords: HashMap<KeyAction, Vec<KeyChord>>,
}

#[derive(Serialize, Deserialize)]
struct KeymapStore {
    bindings: HashMap<KeyAction, KeyChordStore>,
    #[serde(default)]
    extra_chords: HashMap<KeyAction, Vec<KeyChordStore>>,
}

impl Serialize for Keymap {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let store = KeymapStore {
            bindings: self.bindings.iter().map(|(a, c)| (*a, c.store())).collect(),
            extra_chords: self
                .extra_chords
                .iter()
                .map(|(a, list)| (*a, list.iter().map(|c| c.store()).collect::<Vec<_>>()))
                .collect(),
        };
        store.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Keymap {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let store = KeymapStore::deserialize(deserializer)?;
        let mut bindings = HashMap::new();
        for (action, raw) in store.bindings {
            let chord = KeyChord::load(raw).map_err(serde::de::Error::custom)?;
            bindings.insert(action, chord);
        }
        let mut extra_chords = HashMap::new();
        for (action, list) in store.extra_chords {
            let mut parsed = Vec::new();
            for raw in list {
                parsed.push(KeyChord::load(raw).map_err(serde::de::Error::custom)?);
            }
            extra_chords.insert(action, parsed);
        }
        Ok(Self {
            bindings,
            extra_chords,
        })
    }
}

impl Default for Keymap {
    fn default() -> Self {
        Self::default_bindings()
    }
}

impl Keymap {
    pub fn default_bindings() -> Self {
        let mut bindings: HashMap<KeyAction, KeyChord> = HashMap::new();
        let mut extra_chords: HashMap<KeyAction, Vec<KeyChord>> = HashMap::new();
        for entry in SPECS {
            let Some(chord) = entry.default_chord else {
                continue;
            };
            use std::collections::hash_map::Entry;
            match bindings.entry(entry.action) {
                Entry::Vacant(slot) => {
                    slot.insert(chord);
                }
                Entry::Occupied(_) => {
                    extra_chords.entry(entry.action).or_default().push(chord);
                }
            }
        }
        Self {
            bindings,
            extra_chords,
        }
    }

    pub fn chord_for(&self, action: KeyAction) -> Option<KeyChord> {
        self.bindings.get(&action).copied()
    }

    pub fn export_bindings(&self) -> HashMap<KeyAction, KeyChord> {
        self.bindings.clone()
    }

    pub fn chords_for(&self, action: KeyAction) -> Vec<KeyChord> {
        let mut out = Vec::new();
        if let Some(primary) = self.bindings.get(&action) {
            out.push(*primary);
        }
        if let Some(extra) = self.extra_chords.get(&action) {
            out.extend(extra.iter().copied());
        }
        out
    }

    pub fn set_binding(&mut self, action: KeyAction, chord: KeyChord) -> Result<(), String> {
        if spec(action).is_none_or(|s| !s.rebindable) {
            return Err("that action cannot be rebound".into());
        }
        if let Some(owner) = self.owner_in_scope(chord, action_context(action), None) {
            return Err(format!(
                "{} already uses {}",
                action_label(owner),
                format_chord(chord)
            ));
        }
        self.bindings.insert(action, chord);
        self.extra_chords.remove(&action);
        Ok(())
    }

    fn owner_in_scope(
        &self,
        chord: KeyChord,
        scope: ShortcutContext,
        skip: Option<KeyAction>,
    ) -> Option<KeyAction> {
        for (action, bound) in &self.bindings {
            if skip == Some(*action) {
                continue;
            }
            if *bound != chord {
                continue;
            }
            if contexts_overlap(action_context(*action), scope) {
                return Some(*action);
            }
        }
        for (action, extras) in &self.extra_chords {
            if skip == Some(*action) {
                continue;
            }
            for bound in extras {
                if *bound == chord && contexts_overlap(action_context(*action), scope) {
                    return Some(*action);
                }
            }
        }
        None
    }

    pub fn resolve(&self, chord: KeyChord, tab: Tab, command_open: bool) -> Option<KeyAction> {
        let scope = active_scope(tab, command_open);
        for (action, bound) in &self.bindings {
            if *bound != chord {
                continue;
            }
            if context_matches(action_context(*action), scope, tab, command_open) {
                return Some(*action);
            }
        }
        for (action, extras) in &self.extra_chords {
            for bound in extras {
                if *bound == chord
                    && context_matches(action_context(*action), scope, tab, command_open)
                {
                    return Some(*action);
                }
            }
        }
        None
    }

    pub fn rows_for_help(&self) -> Vec<(String, String, String)> {
        let mut rows = Vec::new();
        let mut covered = HashSet::new();
        for entry in SPECS {
            if !covered.insert(entry.action) {
                continue;
            }
            let Some(spec) = spec(entry.action) else {
                continue;
            };
            for chord in self.chords_for(entry.action) {
                rows.push((
                    format_chord(chord),
                    spec.label.to_string(),
                    context_name(spec.context).to_string(),
                ));
            }
        }
        for (keys, action, context) in GESTURE_ROWS {
            rows.push((keys.to_string(), action.to_string(), context.to_string()));
        }
        rows
    }

    pub fn sanitize(&mut self) {
        let known: HashSet<KeyAction> = SPECS.iter().map(|s| s.action).collect();
        self.bindings.retain(|action, _| known.contains(action));
        self.extra_chords.retain(|action, _| known.contains(action));
    }

    pub fn reset_to_default(&mut self) {
        *self = Self::default_bindings();
    }
}

fn action_context(action: KeyAction) -> ShortcutContext {
    spec(action)
        .map(|s| s.context)
        .unwrap_or(ShortcutContext::Anywhere)
}

fn active_scope(tab: Tab, command_open: bool) -> ShortcutContext {
    if command_open {
        return ShortcutContext::CommandPalette;
    }
    match tab {
        Tab::Schematic => ShortcutContext::Schematic,
        Tab::Pcb => ShortcutContext::Pcb,
        _ => ShortcutContext::Anywhere,
    }
}

fn context_matches(
    action_ctx: ShortcutContext,
    _scope: ShortcutContext,
    tab: Tab,
    command_open: bool,
) -> bool {
    if command_open && action_ctx == ShortcutContext::CommandPalette {
        return true;
    }
    match action_ctx {
        ShortcutContext::Anywhere => !command_open,
        ShortcutContext::Schematic => !command_open && tab == Tab::Schematic,
        ShortcutContext::Pcb => !command_open && tab == Tab::Pcb,
        ShortcutContext::CommandPalette => command_open,
    }
}

fn contexts_overlap(a: ShortcutContext, b: ShortcutContext) -> bool {
    if a == b {
        return true;
    }
    if a == ShortcutContext::Anywhere || b == ShortcutContext::Anywhere {
        return true;
    }
    false
}

pub fn chord_from_input(key: Key, modifiers: egui::Modifiers) -> KeyChord {
    KeyChord {
        key,
        ctrl: modifiers.ctrl || modifiers.command,
        shift: modifiers.shift,
        alt: modifiers.alt,
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct KeymapProfileFile {
    pub version: u32,
    pub name: String,
    pub bindings: HashMap<KeyAction, KeyChordStore>,
}

impl KeymapProfileFile {
    pub const VERSION: u32 = 1;

    pub fn from_keymap(name: String, map: &Keymap) -> Self {
        Self {
            version: Self::VERSION,
            name,
            bindings: map
                .export_bindings()
                .into_iter()
                .map(|(action, chord)| (action, chord.store()))
                .collect(),
        }
    }

    pub fn into_keymap(self) -> Result<Keymap, String> {
        if self.version != Self::VERSION {
            return Err(format!("unsupported profile version {}", self.version));
        }
        let mut map = Keymap::default_bindings();
        for (action, stored) in self.bindings {
            map.set_binding(action, KeyChord::load(stored)?)?;
        }
        Ok(map)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_bindings_have_unique_chords_per_context() -> Result<(), String> {
        let map = Keymap::default_bindings();
        let mut seen: BTreeMap<(String, bool, bool, bool, ShortcutContext), KeyAction> =
            BTreeMap::new();
        for action in rebindable_actions() {
            for chord in map.chords_for(action) {
                let ctx = action_context(action);
                let key = (
                    storage_key_name(chord.key),
                    chord.ctrl,
                    chord.shift,
                    chord.alt,
                    ctx,
                );
                if let Some(other) = seen.insert(key, action) {
                    return Err(format!(
                        "{other:?} and {action:?} both use {} in {ctx:?}",
                        format_chord(chord)
                    ));
                }
            }
        }
        Ok(())
    }

    #[test]
    fn every_action_has_non_empty_label_and_tooltip() {
        for action in rebindable_actions() {
            assert!(!action_label(action).is_empty());
            assert!(!action_tooltip(action).is_empty());
        }
    }

    #[test]
    fn help_rows_include_non_rebindable_gestures() {
        let rows = Keymap::default_bindings().rows_for_help();
        assert!(rows.iter().any(|(k, _, _)| k == "Middle-drag"));
        assert!(rows.iter().any(|(k, _, _)| k == "Scroll"));
    }

    #[test]
    fn keymap_reject_conflicting_rebind() -> Result<(), String> {
        let mut map = Keymap::default_bindings();
        let rotate = map.chord_for(KeyAction::Rotate).ok_or("rotate chord")?;
        let err = match map.set_binding(KeyAction::MoveTool, rotate) {
            Err(e) => e,
            Ok(()) => return Err("conflict".into()),
        };
        if !err.contains("Rotate") {
            return Err(err);
        }
        Ok(())
    }

    #[test]
    fn keymap_import_export_round_trip() -> Result<(), String> {
        let mut map = Keymap::default_bindings();
        let t = KeyChord {
            key: Key::T,
            ctrl: false,
            shift: false,
            alt: false,
        };
        map.set_binding(KeyAction::Rotate, t)?;
        let file = KeymapProfileFile::from_keymap("Custom".into(), &map);
        let json = serde_json::to_string(&file).map_err(|e| e.to_string())?;
        let parsed: KeymapProfileFile = serde_json::from_str(&json).map_err(|e| e.to_string())?;
        let back = parsed.into_keymap()?;
        assert_eq!(back.chord_for(KeyAction::Rotate), Some(t));
        Ok(())
    }
}
