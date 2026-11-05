//! Words for tool modifiers: tooltip lines, the status hint, the context-menu
//! hint and the Keys section of the preferences.
//!
//! The same command can appear in three places (toolbar tooltip, context menu
//! hint, status bar while held). All three read the one resolver table in
//! `tool_modifiers.rs`, so the words can never disagree with what a click does.
//!
//! # Rules
//! - Text comes from the catalog (`modifier.*` keys in `crates/text/strings`).
//! - Chord names follow the platform: Ctrl off macOS, Cmd on macOS, and Option
//!   for Alt on macOS.
//!
//! # Dependencies
//! - `crates/ui-egui/src/tool_modifiers.rs` bindings, rows and hotkeys.
//! - `crates/ui-egui/src/canvas/gestures.rs` calls [`status_hint`].
//! - `crates/ui-egui/src/preferences/keys.rs` calls [`keys_card`]; `lib.rs` calls [`tip`] and [`menu_rows`].
//!
//! # Do not
//! - Hard-code a chord in a string; build it from the binding.

use crate::ui_kit::button::{self, Size};
use crate::ui_kit::form_row::{FormWidth, form_row};
use crate::ui_kit::icons;
use crate::ui_kit::select::select;
use crate::{fonts, theme};
use egui::Key;
use pcbcraft_text::t;

use crate::tool_modifiers::{
    self as tm, Bindings, ClickAction, Held, Hotkey, ModAction, ModifierCombo, Phase, Platform,
    ToolKind, Workspace,
};

/// Short effect of a modifier action, such as `45° only`.
pub fn effect(action: ModAction) -> String {
    match action {
        ModAction::SelectAdd => t!("modifier.effect.select_add"),
        ModAction::SelectToggle => t!("modifier.effect.select_toggle"),
        ModAction::PickUnder => t!("modifier.effect.pick_under"),
        ModAction::ConstrainAngle => t!("modifier.effect.constrain_angle"),
        ModAction::SuspendSnap => t!("modifier.effect.suspend_snap"),
        ModAction::ConstrainAxis => t!("modifier.effect.constrain_axis"),
        ModAction::DuplicateDrag => t!("modifier.effect.duplicate_drag"),
        ModAction::DragNoSnap => t!("modifier.effect.drag_no_snap"),
        ModAction::PanHorizontal => t!("modifier.effect.pan_horizontal"),
        ModAction::FineZoom => t!("modifier.effect.fine_zoom"),
        ModAction::DeleteConnection => t!("modifier.effect.delete_connection"),
        ModAction::RemoveSegment => t!("modifier.effect.remove_segment"),
        ModAction::WireCorner => t!("modifier.effect.wire_corner"),
        ModAction::RouteCorner => t!("modifier.effect.route_corner"),
        ModAction::ShoveToggle => t!("modifier.effect.shove_toggle"),
    }
}

/// What a click or drag does, as a sentence start; `what` names the target.
pub fn action_words(action: ClickAction, what: &str) -> String {
    let what = what.to_owned();
    match action {
        ClickAction::Replace => t!("modifier.click.replace", what = what),
        ClickAction::Add => t!("modifier.click.add", what = what),
        ClickAction::Toggle => t!("modifier.click.toggle", what = what),
        ClickAction::PickUnder => t!("modifier.click.pick_under", what = what),
        ClickAction::StartWire => t!("modifier.click.start_wire", what = what),
        ClickAction::StartRoute => t!("modifier.click.start_route", what = what),
        ClickAction::ContinueWire => t!("modifier.click.continue_wire", what = what),
        ClickAction::ContinueRoute => t!("modifier.click.continue_route", what = what),
        ClickAction::MoveSegment => t!("modifier.click.move_segment"),
        ClickAction::MovePart => t!("modifier.click.move_part", what = what),
        ClickAction::MoveWireEnd => t!("modifier.click.move_wire_end", what = what),
        ClickAction::BoxSelect => t!("modifier.click.box_select"),
        ClickAction::RenameLabel => t!("modifier.click.rename_label", what = what),
        ClickAction::DeleteConnection => t!("modifier.click.delete_connection"),
        ClickAction::RemoveSegment => t!("modifier.click.remove_segment"),
        ClickAction::ToolDefault | ClickAction::Nothing => String::new(),
    }
}

/// The chord for an action as words, for the platform.
fn chord_label(
    bindings: &Bindings,
    action: ModAction,
    tool: ToolKind,
    platform: Platform,
) -> String {
    tm::tool_bindings(tool)
        .0
        .iter()
        .chain(tm::WHEEL_BINDINGS)
        .find(|b| b.action == action)
        .map(|b| {
            bindings
                .chords_for(b)
                .iter()
                .map(|c| c.label_for(platform))
                .collect::<Vec<_>>()
                .join(" / ")
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| t!("modifier.chord.off"))
}

/// The modifier lines of a tool: `Shift: 45° only, Ctrl: no snap`.
pub fn tool_lines(bindings: &Bindings, tool: ToolKind, platform: Platform) -> String {
    tm::tool_bindings(tool)
        .0
        .iter()
        .filter(|b| !bindings.chords_for(b).is_empty())
        .map(|b| {
            let chord = chord_label(bindings, b.action, tool, platform);
            let plus = if b.phase == Phase::Click {
                "+click"
            } else {
                ""
            };
            format!("{chord}{plus}: {}", effect(b.action))
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// A toolbar tooltip: the base text, the tool's hotkey and its modifiers.
///
/// # Rules
/// - Adds one short line; the base text stays first.
///
/// # Dependencies
/// - `crates/ui-egui/src/lib.rs` tool strip buttons.
pub fn tip(
    base: String,
    tool: ToolKind,
    hotkey: Option<Hotkey>,
    bindings: &Bindings,
    platform: Platform,
) -> String {
    let mut extra = Vec::new();
    if let Some(h) = hotkey {
        extra.push(t!("modifier.tip.key", key = key_name(bindings.key_for(h))));
    }
    let lines = tool_lines(bindings, tool, platform);
    if !lines.is_empty() {
        extra.push(lines);
    }
    if extra.is_empty() {
        base
    } else {
        format!("{base}\n{}", extra.join(". "))
    }
}

/// The rows under the context menu: one per modifier click, as (key, action).
///
/// Why: the menu shows a short key list. Drag modifiers are left out; the status
/// bar names them while a drag is under way.
///
/// # Rules
/// - Only click bindings; the action starts with a capital letter.
///
/// # Dependencies
/// - crates/ui-egui/src/ui_kit/menu.rs (`Menu::footer`).
pub fn menu_rows(bindings: &Bindings, tool: ToolKind, platform: Platform) -> Vec<(String, String)> {
    tm::tool_bindings(tool)
        .0
        .iter()
        .filter(|b| b.phase == Phase::Click && !bindings.chords_for(b).is_empty())
        .map(|b| {
            let key = format!("{}+click", chord_label(bindings, b.action, tool, platform));
            let mut action = effect(b.action);
            if let Some(first) = action.get(..1) {
                action = format!("{}{}", first.to_uppercase(), &action[1..]);
            }
            (key, action)
        })
        .collect()
}

/// The status-bar text for the hovered target and held keys.
///
/// Why: the owner wants the status bar to say what a click will do right now,
/// and which held modifier is responsible, for example
/// `Click: start wire from R1.2` or `Ctrl+click: delete connection`.
///
/// # Rules
/// - A click that differs from the plain click names the held chord.
/// - Held keys that change a live action add `Shift: 45° only` style words.
#[allow(clippy::too_many_arguments)]
pub fn status_hint(
    action: ClickAction,
    plain: ClickAction,
    drag: ClickAction,
    what: &str,
    tool: ToolKind,
    _workspace: Workspace,
    held: Held,
    bindings: &Bindings,
    platform: Platform,
) -> String {
    let mut parts: Vec<String> = Vec::new();
    let click = action_words(action, what);
    if !click.is_empty() {
        let lead = if action == plain || held.none() {
            t!("modifier.status.click")
        } else {
            t!(
                "modifier.status.chord_click",
                chord = held_label(held, platform)
            )
        };
        parts.push(format!("{lead}: {click}"));
    }
    if matches!(
        drag,
        ClickAction::MovePart
            | ClickAction::MoveSegment
            | ClickAction::StartWire
            | ClickAction::StartRoute
            | ClickAction::BoxSelect
    ) && drag != action
    {
        parts.push(format!(
            "{}: {}",
            t!("modifier.status.drag"),
            action_words(drag, what)
        ));
    }
    for phase in [Phase::Live, Phase::Drag] {
        let set = tm::active(bindings, tool, held, phase);
        for a in ModAction::ALL {
            let in_phase = tm::tool_bindings(tool)
                .0
                .iter()
                .any(|b| b.action == a && b.phase == phase);
            if set.contains(a) && in_phase {
                let line = format!(
                    "{}: {}",
                    chord_label(bindings, a, tool, platform),
                    effect(a)
                );
                if !parts.contains(&line) {
                    parts.push(line);
                }
            }
        }
    }
    parts.join("   ")
}

/// The names of the held keys, such as `Shift+Ctrl`.
fn held_label(held: Held, platform: Platform) -> String {
    ModifierCombo {
        shift: held.shift,
        ctrl: held.primary,
        alt: held.alt,
    }
    .label_for(platform)
}

/// A key as the preferences show it.
pub fn key_name(key: Key) -> String {
    if key == Key::Escape {
        "Esc".into()
    } else {
        key.name().to_owned()
    }
}

/// The tool modifiers card of Edit > Preferences > Keys: one row per tool hotkey and per
/// modifier action.
///
/// # Rules
/// - Changes apply on the next frame and are saved with the preferences.
/// - Every control has a tooltip; every row can go back to its classic default.
///
/// # Dependencies
/// - `crates/ui-egui/src/lib.rs` `Preferences::modifiers`.
/// - crates/ui-egui/src/ui_kit/form_row.rs
/// - crates/ui-egui/src/ui_kit/select.rs
///
/// # Do not
/// - Offer a hotkey that another hotkey or the editor already uses.
pub fn keys_card(app: &mut crate::App, ui: &mut egui::Ui) {
    let platform = Platform::of(ui.ctx());
    let tk = theme::tokens(ui.ctx());
    ui.label(fonts::caption(t!("modifier.prefs.intro")).color(tk.muted));
    ui.add_space(theme::SPACE[0]);
    let mut message: Option<String> = None;
    for h in Hotkey::ALL {
        let current = app.preferences.modifiers.key_for(h);
        let keys = choosable_keys(h);
        let names: Vec<String> = keys.iter().map(|k| key_name(*k)).collect();
        let options: Vec<&str> = names.iter().map(String::as_str).collect();
        let mut at = keys.iter().position(|k| *k == current).unwrap_or(0);
        let before = at;
        let words = hotkey_words(h);
        form_row(ui, FormWidth::Page, &words, None, None, |ui| {
            select(
                ui,
                ("hotkey", h.id()),
                &mut at,
                &options,
                &words,
                &t!("modifier.prefs.hotkey.tip"),
                120.0,
            );
            if current != h.default_key()
                && button::ghost_button(
                    ui,
                    Some(icons::REFRESH),
                    &t!("modifier.prefs.reset"),
                    &t!("modifier.prefs.reset.tip"),
                    Size::Small,
                    true,
                )
                .clicked()
            {
                app.preferences.modifiers.reset_key(h);
            }
        });
        if at != before
            && let Some(chosen) = keys.get(at)
            && let Err(why) = app.preferences.modifiers.set_key(h, *chosen)
        {
            message = Some(why);
        }
    }
    for action in ModAction::ALL {
        let current = app.preferences.modifiers.chords_for(&binding_of(action));
        let mut labels: Vec<String> = ModifierCombo::CHOICES
            .iter()
            .map(|c| c.label_for(platform))
            .collect();
        labels.push(t!("modifier.chord.off"));
        let options: Vec<&str> = labels.iter().map(String::as_str).collect();
        let off = ModifierCombo::CHOICES.len();
        let mut at = current
            .first()
            .and_then(|first| ModifierCombo::CHOICES.iter().position(|c| c == first))
            .unwrap_or(off);
        let before = at;
        let words = effect(action);
        form_row(ui, FormWidth::Page, &words, None, None, |ui| {
            select(
                ui,
                ("chord", action.id()),
                &mut at,
                &options,
                &words,
                &t!("modifier.prefs.chord.tip"),
                200.0,
            );
            if app.preferences.modifiers.is_custom(action)
                && button::ghost_button(
                    ui,
                    Some(icons::REFRESH),
                    &t!("modifier.prefs.reset"),
                    &t!("modifier.prefs.reset.tip"),
                    Size::Small,
                    true,
                )
                .clicked()
            {
                app.preferences.modifiers.reset_chord(action);
            }
        });
        if at != before {
            let pick = ModifierCombo::CHOICES.get(at).copied();
            app.preferences.modifiers.set_chord(action, pick);
        }
    }
    for (_, a, b) in tm::conflicts(&app.preferences.modifiers) {
        ui.label(
            fonts::caption(t!(
                "modifier.prefs.clash",
                first = effect(a),
                second = effect(b)
            ))
            .color(tk.red),
        );
    }
    if let Some(m) = message {
        ui.label(fonts::caption(m).color(tk.red));
    }
    ui.add_space(theme::SPACE[1]);
    if button::secondary_button(
        ui,
        None,
        &t!("modifier.prefs.reset_all"),
        &t!("modifier.prefs.reset_all.tip"),
        Size::Normal,
        true,
    )
    .clicked()
    {
        app.preferences.modifiers.reset_all();
    }
}

/// The first binding of an action, for reading its current chords.
fn binding_of(action: ModAction) -> tm::Binding {
    for tool in ToolKind::ALL {
        if let Some(b) = tm::tool_bindings(tool)
            .0
            .iter()
            .find(|b| b.action == action)
        {
            return *b;
        }
    }
    tm::WHEEL_BINDINGS
        .iter()
        .copied()
        .find(|b| b.action == action)
        .unwrap_or(tm::WHEEL_BINDINGS[0])
}

fn hotkey_words(h: Hotkey) -> String {
    match h {
        Hotkey::Select => t!("modifier.hotkey.select"),
        Hotkey::Wire => t!("modifier.hotkey.wire"),
        Hotkey::Route => t!("modifier.hotkey.route"),
        Hotkey::Via => t!("modifier.hotkey.via"),
        Hotkey::Text => t!("modifier.hotkey.text"),
    }
}

/// Keys a hotkey may take: letters, and Escape for the Select tool.
fn choosable_keys(h: Hotkey) -> Vec<Key> {
    let mut keys: Vec<Key> = Key::ALL
        .iter()
        .copied()
        .filter(|k| k.name().len() == 1 && k.name().chars().all(|c| c.is_ascii_uppercase()))
        .filter(|k| !tm::RESERVED_KEYS.contains(k))
        .collect();
    if h == Hotkey::Select {
        keys.insert(0, Key::Escape);
    }
    keys
}
