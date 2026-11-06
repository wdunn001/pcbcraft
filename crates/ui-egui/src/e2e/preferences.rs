//! Stories for Edit > Preferences: one story per page, each opening the dialog from the
//! menu like a first-time user and changing a setting with the real controls.
//!
//! Context: issue #1690. The dialog is `crates/ui-egui/src/preferences/`. Every story
//! presses controls by their names and then reads the stored `Preferences`, so the control,
//! the stored value and what the rest of the app does with it are all in the path.
use super::super::*;
use super::support::*;

/// The sample board on the Layout tab, a few frames drawn.
fn board() -> (egui::Context, App) {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    app.change_tab(Tab::Pcb);
    app.needs_fit = true;
    let ctx = egui::Context::default();
    for _ in 0..4 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    (ctx, app)
}

/// Edit > Preferences, then the page named `tab` unless it is the first one.
fn open_tab(ctx: &egui::Context, app: &mut App, tab: &str) -> Result<(), String> {
    authoring::tests::tap(ctx, app, "Edit")?;
    authoring::tests::tap(ctx, app, "Preferences")?;
    for _ in 0..3 {
        frame(ctx, app, vec![], egui::Modifiers::NONE);
    }
    if tab != "Appearance" {
        tap_id(ctx, app, &format!("tab:{tab}"))?;
    }
    Ok(())
}

/// Open a dropdown of the dialog and pick one entry by its words.
fn pick(ctx: &egui::Context, app: &mut App, dropdown: &str, entry: &str) -> Result<(), String> {
    tap_id(ctx, app, &format!("combobox:{dropdown}"))?;
    for _ in 0..3 {
        frame(ctx, app, vec![], egui::Modifiers::NONE);
    }
    let found = ["button", "radiobutton", "menuitem", "selectable"]
        .iter()
        .any(|role| tap_id(ctx, app, &format!("{role}:{entry}")).is_ok());
    if found {
        return Ok(());
    }
    authoring::tests::tap(ctx, app, entry)
}

/// Press choice `slot` of `count` in the segmented control named `group`.
fn segment(
    ctx: &egui::Context,
    app: &mut App,
    group: &str,
    slot: usize,
    count: usize,
) -> Result<(), String> {
    tap_dialog_segment(ctx, app, group, slot, count)
}

/// Press every enabled control whose gate id starts with `prefix`, once, and close what it opens.
/// Returns how many there were.
fn touch_all(ctx: &egui::Context, app: &mut App, prefix: &str) -> Result<usize, String> {
    let ids: std::collections::BTreeSet<String> = controls_tall(ctx, app)
        .into_iter()
        .filter(|id| id.starts_with(prefix))
        .collect();
    for id in &ids {
        tap_id(ctx, app, id)?;
        if prefix.starts_with("combobox") {
            tap_id(ctx, app, id)?;
        }
    }
    Ok(ids.len())
}

/// A key going down and up, as a hand does.
fn press_key(ctx: &egui::Context, app: &mut App, key: egui::Key) {
    for pressed in [true, false] {
        frame(
            ctx,
            app,
            vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            egui::Modifiers::NONE,
        );
    }
}

#[test]
fn a_newcomer_finds_preferences_in_the_edit_menu_and_sees_every_page() -> Result<(), String> {
    let (ctx, mut app) = board();
    open_tab(&ctx, &mut app, "Appearance")?;
    assert!(app.prefs_ui.open, "Edit > Preferences opens the dialog");
    for words in [
        "Preferences",
        "Appearance",
        "Language and units",
        "Keys",
        "Assistant",
        "Look",
        "Changes apply at once.",
    ] {
        assert!(
            paints_text_tall(&ctx, &mut app, words)?,
            "{words} is on screen"
        );
    }
    for tab in ["Language and units", "Keys", "Assistant", "Appearance"] {
        tap_id(&ctx, &mut app, &format!("tab:{tab}"))?;
        assert!(paints_text_tall(&ctx, &mut app, tab)?, "{tab} page opens");
    }
    tap_id(&ctx, &mut app, "button:Done")?;
    for _ in 0..2 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    assert!(!app.prefs_ui.open, "Done closes the dialog");
    assert!(
        !paints_text_tall(&ctx, &mut app, "Changes apply at once.")?,
        "nothing of the dialog stays on screen"
    );
    Ok(())
}

#[test]
fn a_newcomer_switches_the_theme_and_the_whole_app_changes_colour() -> Result<(), String> {
    let (ctx, mut app) = board();
    open_tab(&ctx, &mut app, "Appearance")?;
    assert!(theme::tokens(&ctx).is_dark(), "the app starts dark");
    let dark_panel = theme::tokens(&ctx).panel;
    segment(&ctx, &mut app, "Theme", 1, 3)?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(!app.preferences.dark, "Light is stored");
    assert!(!theme::tokens(&ctx).is_dark(), "the window colours follow");
    assert_ne!(
        theme::tokens(&ctx).panel,
        dark_panel,
        "the panel colour changed"
    );
    segment(&ctx, &mut app, "Theme", 2, 3)?;
    assert!(app.preferences.theme_system, "System is stored");
    segment(&ctx, &mut app, "Theme", 0, 3)?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(app.preferences.dark && !app.preferences.theme_system);
    assert!(
        theme::tokens(&ctx).is_dark(),
        "Dark brings the dark colours back"
    );
    Ok(())
}

#[test]
fn a_newcomer_picks_an_accent_colour_and_sets_size_motion_grid_and_pad_labels() -> Result<(), String>
{
    let (ctx, mut app) = board();
    open_tab(&ctx, &mut app, "Appearance")?;
    let before = app.accent();
    for colour in ["Copper", "Blue", "Gold", "Purple"] {
        tap_id(&ctx, &mut app, &format!("radiobutton:{colour}"))?;
    }
    tap_id(&ctx, &mut app, "radiobutton:Green")?;
    assert_eq!(app.preferences.accent, [79, 203, 140], "Green is stored");
    assert_ne!(
        app.accent(),
        before,
        "the accent the app draws with changed"
    );
    assert!(!app.preferences.reduced_motion);
    tap_id(&ctx, &mut app, "checkbox:Reduce motion")?;
    assert!(app.preferences.reduced_motion, "Reduce motion is stored");
    assert!(app.preferences.offer_tours);
    tap_id(&ctx, &mut app, "checkbox:Offer tours")?;
    assert!(!app.preferences.offer_tours, "Offer tours is stored");
    assert!(app.preferences.grid);
    tap_id(&ctx, &mut app, "checkbox:Show grid")?;
    assert!(!app.preferences.grid, "Show grid is stored");
    segment(&ctx, &mut app, "Pad labels", 1, 3)?;
    assert_eq!(app.preferences.pad_labels, PadLabels::Net, "Net is stored");
    segment(&ctx, &mut app, "Pad labels", 2, 3)?;
    assert_eq!(app.preferences.pad_labels, PadLabels::Off, "Off is stored");
    // Last, because a new size moves every control.
    type_into(&ctx, &mut app, "spinbutton:Interface size", "125")?;
    assert!(
        (app.preferences.ui_scale - 1.25).abs() < 0.001,
        "interface size is {}",
        app.preferences.ui_scale
    );
    Ok(())
}

#[test]
fn a_newcomer_pins_the_minimap_and_sets_its_size_and_opacity() -> Result<(), String> {
    let (ctx, mut app) = board();
    open_tab(&ctx, &mut app, "Appearance")?;
    tap_id(&ctx, &mut app, "checkbox:Pin the minimap")?;
    type_into(&ctx, &mut app, "spinbutton:Size", "320")?;
    type_into(&ctx, &mut app, "spinbutton:Opacity", "80")?;
    for tab in ["layout", "schematic"] {
        let look = app
            .preferences
            .minimap
            .get(tab)
            .ok_or_else(|| format!("no minimap look stored for {tab}"))?;
        assert!(look.pinned, "{tab} map is pinned");
        assert_eq!(look.width, 320.0, "{tab} map width");
        assert!((look.opacity - 0.8).abs() < 0.001, "{tab} map opacity");
    }
    assert!(app.minimap_visible(), "a pinned map is on screen");
    Ok(())
}

#[test]
fn a_newcomer_changes_the_language_and_the_units() -> Result<(), String> {
    let (ctx, mut app) = board();
    open_tab(&ctx, &mut app, "Language and units")?;
    assert_eq!(app.preferences.language, "system");
    pick(&ctx, &mut app, "Language", "Deutsch")?;
    assert_eq!(app.preferences.language, "de", "German is stored");
    assert!(
        paints_text_tall(&ctx, &mut app, "Datei")?,
        "the dialog now speaks German"
    );
    app.set_language(&ctx, "system");
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    segment(&ctx, &mut app, "Default units", 1, 3)?;
    assert_eq!(app.preferences.default_units, "mil", "mil is stored");
    type_into(&ctx, &mut app, "spinbutton:Default grid", "2.54")?;
    assert_eq!(app.preferences.default_grid_mm, 2.54, "default grid");
    type_into(&ctx, &mut app, "spinbutton:Board grid", "0.25")?;
    assert_eq!(app.preferences.board_grid_mm, 0.25, "board grid");
    segment(&ctx, &mut app, "Number format", 1, 3)?;
    assert_eq!(app.preferences.number_format, 1, "number format");
    Ok(())
}

#[test]
fn a_newcomer_searches_the_shortcuts_and_rebinds_the_route_tool() -> Result<(), String> {
    let (ctx, mut app) = board();
    open_tab(&ctx, &mut app, "Keys")?;
    assert!(
        paints_text_tall(&ctx, &mut app, "Route tool")?,
        "the Route row is listed"
    );
    assert!(
        paints_text_tall(&ctx, &mut app, "Zoom the view")?,
        "the View rows are listed"
    );
    type_into(&ctx, &mut app, "textinput:Search shortcuts", "Route tool")?;
    assert!(
        paints_text_tall(&ctx, &mut app, "Route tool")?,
        "the search keeps Route"
    );
    assert!(
        !paints_text_tall(&ctx, &mut app, "Zoom the view")?,
        "the search drops the rows that do not match"
    );
    tap_id(&ctx, &mut app, "button:Change")?;
    assert_eq!(
        app.binding_capture,
        Some(crate::keymap::KeyAction::RouteTool)
    );
    assert!(
        paints_text_tall(&ctx, &mut app, "Press a key")?,
        "the row waits for a key"
    );
    press_key(&ctx, &mut app, egui::Key::U);
    assert!(app.binding_capture.is_none(), "the key ends the capture");
    let chord = app
        .preferences
        .active_keymap()
        .chord_for(crate::keymap::KeyAction::RouteTool)
        .ok_or("Route has no key")?;
    assert_eq!(
        crate::keymap::format_chord(chord),
        "U",
        "the new key is stored"
    );
    assert!(
        paints_text_tall(&ctx, &mut app, "U")?,
        "the new key is listed"
    );
    tap_id(&ctx, &mut app, "button:Reset all")?;
    let chord = app
        .preferences
        .active_keymap()
        .chord_for(crate::keymap::KeyAction::RouteTool)
        .ok_or("Route has no key")?;
    assert_ne!(
        crate::keymap::format_chord(chord),
        "U",
        "Reset all restores the key"
    );
    tap_id(&ctx, &mut app, "combobox:Key set")?;
    tap_id(&ctx, &mut app, "combobox:Key set")?;
    Ok(())
}

#[test]
fn a_newcomer_cannot_give_one_key_to_two_actions() -> Result<(), String> {
    let (ctx, mut app) = board();
    open_tab(&ctx, &mut app, "Keys")?;
    type_into(&ctx, &mut app, "textinput:Search shortcuts", "Route tool")?;
    tap_id(&ctx, &mut app, "button:Change")?;
    // R belongs to Rotate.
    press_key(&ctx, &mut app, egui::Key::R);
    let chord = app
        .preferences
        .active_keymap()
        .chord_for(crate::keymap::KeyAction::RouteTool)
        .ok_or("Route has no key")?;
    assert_ne!(
        crate::keymap::format_chord(chord),
        "R",
        "a key in use is refused: {}",
        app.keymap_notice
    );
    assert!(!app.keymap_notice.is_empty(), "the refusal is explained");
    Ok(())
}

#[test]
fn a_newcomer_filters_shortcuts_by_area_and_keeps_a_key_set() -> Result<(), String> {
    let (ctx, mut app) = board();
    open_tab(&ctx, &mut app, "Keys")?;
    segment(&ctx, &mut app, "Area", 2, 4)?;
    assert_eq!(app.prefs_ui.keys_area, 2, "the Layers filter is chosen");
    assert!(paints_text_tall(&ctx, &mut app, "Layers")?);
    assert!(
        !paints_text_tall(&ctx, &mut app, "Route tool: click")?,
        "the Layers filter hides the Tools rows"
    );
    segment(&ctx, &mut app, "Area", 0, 4)?;
    type_into(&ctx, &mut app, "textinput:Save as", "Mine")?;
    tap_id(&ctx, &mut app, "button:Save")?;
    assert!(
        app.preferences.keymap_profiles.contains_key("Mine"),
        "the key set is stored: {}",
        app.keymap_notice
    );
    tap_id(&ctx, &mut app, "button:Export")?;
    assert!(
        app.profile_import_text.contains("Mine"),
        "the key set is exported as text"
    );
    tap_id(&ctx, &mut app, "multilinetextinput:Key set text")?;
    tap_id(&ctx, &mut app, "button:Import")?;
    assert!(app.keymap_notice.contains("Mine"), "{}", app.keymap_notice);
    tap_id(&ctx, &mut app, "button:Delete set")?;
    assert!(
        !app.preferences.keymap_profiles.contains_key("Mine"),
        "Delete set removes it"
    );
    Ok(())
}

#[test]
fn a_newcomer_changes_a_tool_key_and_a_modifier() -> Result<(), String> {
    let (ctx, mut app) = board();
    open_tab(&ctx, &mut app, "Keys")?;
    let before = app.preferences.modifiers.clone();
    let listed = touch_all(&ctx, &mut app, "combobox:")?;
    assert!(
        listed >= 6,
        "the tool keys and modifiers are listed: {listed}"
    );
    pick(&ctx, &mut app, "Wire tool", "B")?;
    assert_ne!(app.preferences.modifiers, before, "a tool key changed");
    tap_id(&ctx, &mut app, "button:Reset keys")?;
    assert_eq!(
        app.preferences.modifiers, before,
        "Reset keys restores them"
    );
    Ok(())
}

#[test]
fn a_newcomer_sets_what_the_assistant_may_do() -> Result<(), String> {
    let (ctx, mut app) = board();
    open_tab(&ctx, &mut app, "Appearance")?;
    app.open_preferences(crate::preferences::TAB_ASSISTANT);
    assert!(paints_text_tall(&ctx, &mut app, "Connections")?);
    assert!(paints_text_tall(&ctx, &mut app, "Behaviour")?);
    tap_id(&ctx, &mut app, "button:Add connection")?;
    assert!(
        paints_text_tall(&ctx, &mut app, "Start from")?,
        "the form opens in the tab"
    );
    tap_id(&ctx, &mut app, "button:Cancel")?;
    assert!(app.assistant.dialog.is_none(), "Cancel closes the form");
    segment(&ctx, &mut app, "What it may do without asking", 1, 3)?;
    assert_eq!(
        app.preferences.assistant_mode,
        pcbcraft_assistant::Mode::Edits
    );
    type_into(&ctx, &mut app, "spinbutton:Changes per message", "40")?;
    assert_eq!(app.preferences.assistant_turn_limit, 40);
    tap_id(&ctx, &mut app, "checkbox:Show assistant")?;
    assert!(
        !app.preferences.assistant_enabled,
        "the assistant switches off"
    );
    Ok(())
}

#[test]
fn a_newcomer_resets_a_page_to_its_defaults() -> Result<(), String> {
    let (ctx, mut app) = board();
    open_tab(&ctx, &mut app, "Appearance")?;
    segment(&ctx, &mut app, "Theme", 1, 3)?;
    tap_id(&ctx, &mut app, "checkbox:Show grid")?;
    tap_id(&ctx, &mut app, "button:Reset to defaults")?;
    assert!(app.preferences.dark, "Dark is back");
    assert!(app.preferences.grid, "Show grid is back");
    Ok(())
}

#[test]
fn a_newcomer_closes_preferences_with_escape() -> Result<(), String> {
    let (ctx, mut app) = board();
    open_tab(&ctx, &mut app, "Appearance")?;
    press_key(&ctx, &mut app, egui::Key::Escape);
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(!app.prefs_ui.open, "Escape closes the dialog");
    Ok(())
}
