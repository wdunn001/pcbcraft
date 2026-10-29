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
//! - `crates/ui-egui/src/lib.rs` calls [`tip`], [`menu_hint`] and [`keys_section`].
//!
//! # Do not
//! - Hard-code a chord in a string; build it from the binding.

use crate::a11y::Named;
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

/// The line under the context menu: how modifiers change a click.
pub fn menu_hint(bindings: &Bindings, tool: ToolKind, platform: Platform) -> String {
    tool_lines(bindings, tool, platform)
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

/// The Keys section of Edit > Preferences: one row per hotkey and per modifier.
///
/// # Rules
/// - Changes apply on the next frame and are saved with the preferences.
/// - Every control has a tooltip; every row can go back to its classic default.
///
/// # Dependencies
/// - `crates/ui-egui/src/lib.rs` `Preferences::modifiers`.
///
/// # Do not
/// - Offer a hotkey that another hotkey or the editor already uses.
pub fn keys_section(app: &mut crate::App, ui: &mut egui::Ui) {
    let platform = Platform::of(ui.ctx());
    ui.add_space(16.0);
    egui::CollapsingHeader::new(t!("modifier.prefs.title"))
        .default_open(false)
        .show(ui, |ui| keys_rows(app, ui, platform))
        .header_response
        .on_hover_text(t!("modifier.prefs.title.tip"));
}

/// The rows inside the Keys section.
fn keys_rows(app: &mut crate::App, ui: &mut egui::Ui, platform: Platform) {
    ui.label(
        egui::RichText::new(t!("modifier.prefs.intro"))
            .small()
            .color(app.muted()),
    );
    ui.add_space(4.0);
    let mut message: Option<String> = None;
    for h in Hotkey::ALL {
        let current = app.preferences.modifiers.key_for(h);
        let mut chosen = current;
        ui.horizontal(|ui| {
            ui.label(hotkey_words(h));
            egui::ComboBox::from_id_salt(("hotkey", h.id()))
                .selected_text(key_name(current))
                .show_ui(ui, |ui| {
                    for key in choosable_keys(h) {
                        ui.selectable_value(&mut chosen, key, key_name(key));
                    }
                })
                .response
                .named(&hotkey_words(h))
                .on_hover_text(t!("modifier.prefs.hotkey.tip"));
            if app.preferences.modifiers.key_for(h) != h.default_key()
                && ui
                    .small_button(t!("modifier.prefs.reset"))
                    .on_hover_text(t!("modifier.prefs.reset.tip"))
                    .clicked()
            {
                app.preferences.modifiers.reset_key(h);
            }
        });
        if chosen != current
            && let Err(why) = app.preferences.modifiers.set_key(h, chosen)
        {
            message = Some(why);
        }
    }
    ui.add_space(4.0);
    for action in ModAction::ALL {
        let current = app.preferences.modifiers.chords_for(&binding_of(action));
        let shown = match current.as_slice() {
            [] => t!("modifier.chord.off"),
            list => list
                .iter()
                .map(|c| c.label_for(platform))
                .collect::<Vec<_>>()
                .join(" / "),
        };
        ui.horizontal(|ui| {
            ui.label(effect(action));
            let mut pick: Option<Option<ModifierCombo>> = None;
            egui::ComboBox::from_id_salt(("chord", action.id()))
                .selected_text(shown)
                .show_ui(ui, |ui| {
                    for c in ModifierCombo::CHOICES {
                        if ui.selectable_label(false, c.label_for(platform)).clicked() {
                            pick = Some(Some(c));
                        }
                    }
                    if ui
                        .selectable_label(false, t!("modifier.chord.off"))
                        .clicked()
                    {
                        pick = Some(None);
                    }
                })
                .response
                .named(&effect(action))
                .on_hover_text(t!("modifier.prefs.chord.tip"));
            if let Some(p) = pick {
                app.preferences.modifiers.set_chord(action, p);
            }
            if app.preferences.modifiers.is_custom(action)
                && ui
                    .small_button(t!("modifier.prefs.reset"))
                    .on_hover_text(t!("modifier.prefs.reset.tip"))
                    .clicked()
            {
                app.preferences.modifiers.reset_chord(action);
            }
        });
    }
    for (_, a, b) in tm::conflicts(&app.preferences.modifiers) {
        ui.label(
            egui::RichText::new(t!(
                "modifier.prefs.clash",
                first = effect(a),
                second = effect(b)
            ))
            .small()
            .color(egui::Color32::from_rgb(240, 90, 80)),
        );
    }
    if let Some(m) = message {
        ui.label(
            egui::RichText::new(m)
                .small()
                .color(egui::Color32::from_rgb(240, 90, 80)),
        );
    }
    if ui
        .button(t!("modifier.prefs.reset_all"))
        .on_hover_text(t!("modifier.prefs.reset_all.tip"))
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
