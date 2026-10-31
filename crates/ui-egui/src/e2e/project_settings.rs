//! Stories for File > Project settings: one story per tab, each opening the tab by its
//! label like a first-time user.
//!
//! Context: issue #775 (fork #617). The dialog is `crates/ui-egui/src/project_settings/`.
//! Every story edits through the real controls and then reads the project, so the typed
//! engine commands, the undo step and the re-run checks are all in the path.
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

/// File > Project settings…, then the tab named `tab` unless it is the first one.
fn open_tab(ctx: &egui::Context, app: &mut App, tab: &str) -> Result<(), String> {
    authoring::tests::tap(ctx, app, "File")?;
    authoring::tests::tap(ctx, app, "Project settings…")?;
    for _ in 0..3 {
        frame(ctx, app, vec![], egui::Modifiers::NONE);
    }
    if tab != "Board and stackup" {
        tap_id(ctx, app, &format!("tab:{tab}"))?;
    }
    Ok(())
}

/// Press a button of the dialog by its label.
fn press(ctx: &egui::Context, app: &mut App, label: &str) -> Result<(), String> {
    tap_id(ctx, app, &format!("button:{label}"))
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

/// Undo the last engine edit, as Edit > Undo does.
fn undo(app: &mut App) -> Result<(), String> {
    app.session
        .lock()
        .map_err(|_| "session is busy".to_owned())?
        .undo()
        .map_err(|e| e.to_string())?;
    app.refresh();
    Ok(())
}

#[test]
fn a_newcomer_finds_project_settings_in_the_file_menu() -> Result<(), String> {
    let (ctx, mut app) = board();
    open_tab(&ctx, &mut app, "Board and stackup")?;
    for words in [
        "Project settings",
        "Board and stackup",
        "Design rules",
        "Net classes",
        "Electrical rules",
        "Machines",
        "Title block",
        "Applies to this project and is saved with it.",
    ] {
        assert!(
            paints_text(&ctx, &mut app, words)?,
            "the dialog shows {words}"
        );
    }
    Ok(())
}

#[test]
fn a_newcomer_resizes_the_board_and_cancel_brings_it_back() -> Result<(), String> {
    let (ctx, mut app) = board();
    let before = app.project.board.width;
    open_tab(&ctx, &mut app, "Board and stackup")?;
    type_into(&ctx, &mut app, "spinbutton:Width", "77")?;
    assert_eq!(
        app.project.board.width, 77.0,
        "the board follows the box: {}",
        app.message
    );
    press(&ctx, &mut app, "Cancel")?;
    assert_eq!(app.project.board.width, before, "Cancel puts the size back");
    assert!(!app.project_settings.open, "the dialog closed");
    Ok(())
}

#[test]
fn a_newcomer_builds_a_four_layer_stackup() -> Result<(), String> {
    let (ctx, mut app) = board();
    open_tab(&ctx, &mut app, "Board and stackup")?;
    pick(&ctx, &mut app, "Start from", "Four layers")?;
    let copper = |app: &App| {
        app.project
            .board
            .layer_records
            .iter()
            .filter(|r| r.kind == "copper")
            .count()
    };
    assert_eq!(copper(&app), 4, "four copper layers: {}", app.message);
    type_into(&ctx, &mut app, "spinbutton:Core Thickness", "0.7")?;
    let core = app
        .project
        .board
        .layer_records
        .iter()
        .find(|r| r.name == "Core")
        .ok_or("the core row")?;
    assert_eq!(
        core.thickness, 0.7,
        "the core thickness took: {}",
        app.message
    );
    press(&ctx, &mut app, "Remove inner pair")?;
    assert_eq!(copper(&app), 2, "the inner pair is gone: {}", app.message);
    press(&ctx, &mut app, "Add inner pair")?;
    assert_eq!(copper(&app), 4, "the inner pair is back: {}", app.message);
    Ok(())
}

#[test]
fn a_newcomer_picks_a_fab_preset_and_changes_one_limit() -> Result<(), String> {
    let (ctx, mut app) = board();
    open_tab(&ctx, &mut app, "Design rules")?;
    let preset = pcbcraft_doc::fab_preset_list()
        .first()
        .copied()
        .ok_or("no presets")?;
    pick(&ctx, &mut app, "Board maker", preset)?;
    let rules = app
        .project
        .board
        .rules
        .clone()
        .ok_or("the preset wrote no rules")?;
    assert_eq!(
        rules.preset.as_deref(),
        Some(preset),
        "the preset is recorded"
    );
    assert!(
        paints_text(&ctx, &mut app, &format!("preset {preset}"))?,
        "each number says it came from the preset"
    );
    type_into(&ctx, &mut app, "spinbutton:Narrowest track", "0.31")?;
    let rules = app.project.board.rules.clone().ok_or("rules")?;
    assert_eq!(
        rules.min_track,
        Some(0.31),
        "the hand edit is stored: {}",
        app.message
    );
    assert!(
        paints_text(&ctx, &mut app, "set by you")?,
        "an edited number says set by you"
    );
    undo(&mut app)?;
    let rules = app.project.board.rules.clone().ok_or("rules")?;
    assert_ne!(rules.min_track, Some(0.31), "one Undo puts the number back");
    Ok(())
}

#[test]
fn a_newcomer_raises_the_clearance_and_the_checks_run_again() -> Result<(), String> {
    let (ctx, mut app) = board();
    let before = app.checks.clone();
    let revision = app.revision;
    open_tab(&ctx, &mut app, "Design rules")?;
    type_into(&ctx, &mut app, "spinbutton:Clearance", "9")?;
    assert_eq!(
        app.project.board.clearance,
        9.0,
        "clearance stored: {} {:?} open={}",
        app.message,
        app.project_settings_error(),
        app.project_settings.open
    );
    assert!(app.revision > revision, "the edit is a committed command");
    assert_ne!(app.checks, before, "the checks were run on the new rule");
    Ok(())
}

#[test]
fn a_newcomer_makes_a_net_class_and_puts_a_net_in_it() -> Result<(), String> {
    let (ctx, mut app) = board();
    open_tab(&ctx, &mut app, "Net classes")?;
    press(&ctx, &mut app, "New class")?;
    assert_eq!(
        app.project.net_classes.len(),
        1,
        "a class was added: {}",
        app.message
    );
    type_into(&ctx, &mut app, "spinbutton:Clearance", "0.45")?;
    assert_eq!(
        app.project.net_classes[0].clearance, 0.45,
        "class clearance: {}",
        app.message
    );
    press(&ctx, &mut app, "Add net")?;
    assert_eq!(
        app.project.net_classes[0].nets.len(),
        1,
        "a net joined: {}",
        app.message
    );
    assert!(
        paints_text(&ctx, &mut app, "Nets: 1")?,
        "the class card counts its nets"
    );
    Ok(())
}

#[test]
fn a_newcomer_relaxes_one_pin_pairing_and_resets_the_grid() -> Result<(), String> {
    let (ctx, mut app) = board();
    open_tab(&ctx, &mut app, "Electrical rules")?;
    tap_id(&ctx, &mut app, "button:Output with Output: Error")?;
    assert_eq!(
        app.project.pin_matrix_level("output", "output"),
        "ok",
        "the cell cycled: {} {:?}",
        app.message,
        app.project.pin_matrix
    );
    assert!(
        paints_text(&ctx, &mut app, "OK *")?,
        "a changed cell carries a star"
    );
    press(&ctx, &mut app, "Reset to default")?;
    assert!(
        app.project.pin_matrix.is_empty(),
        "the grid is back to the built-in table"
    );
    Ok(())
}

#[test]
fn a_newcomer_adds_a_machine_to_the_project() -> Result<(), String> {
    let (ctx, mut app) = board();
    open_tab(&ctx, &mut app, "Machines")?;
    assert!(
        paints_text(&ctx, &mut app, "No machines on this project yet.")?,
        "an empty project says so"
    );
    press(&ctx, &mut app, "Add machine")?;
    assert_eq!(
        app.project.machine_profiles.len(),
        1,
        "a machine was added: {}",
        app.message
    );
    type_into(&ctx, &mut app, "textinput:Name", "Bench mill")?;
    assert_eq!(
        app.project.machine_profiles[0].name, "Bench mill",
        "renamed: {}",
        app.message
    );
    Ok(())
}

#[test]
fn a_newcomer_fills_the_title_block_and_sees_it_on_the_sheet() -> Result<(), String> {
    let (ctx, mut app) = board();
    open_tab(&ctx, &mut app, "Title block")?;
    type_into(&ctx, &mut app, "textinput:Title", "Sensor Board")?;
    type_into(&ctx, &mut app, "textinput:Revision", "B")?;
    assert_eq!(
        app.project.meta.get("title").map(String::as_str),
        Some("Sensor Board")
    );
    assert_eq!(
        app.project.meta.get("revision").map(String::as_str),
        Some("B")
    );
    press(&ctx, &mut app, "Save")?;
    assert!(!app.project_settings.open, "Save closed the dialog");
    app.change_tab(Tab::Schematic);
    assert!(
        paints_text(&ctx, &mut app, "REV B")?,
        "the revision shows on the sheet"
    );
    assert!(
        paints_text(&ctx, &mut app, "Sensor Board")?,
        "the title shows on the sheet"
    );
    Ok(())
}
