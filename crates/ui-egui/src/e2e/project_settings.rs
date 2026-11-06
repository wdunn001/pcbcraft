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
    if tab != "Board" {
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
    open_tab(&ctx, &mut app, "Board")?;
    for words in [
        "Project settings",
        "Board",
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

/// Every control on screen, by its stable name.
fn control_names(ctx: &egui::Context, app: &App) -> Vec<String> {
    super::audit::controls(ctx, app)
        .into_iter()
        .filter_map(|c| c.id)
        .collect()
}

/// An empty project on the Layout tab with an L shaped outline of six corners.
fn l_board() -> Result<(egui::Context, App), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.change_tab(Tab::Pcb);
    let ctx = egui::Context::default();
    for _ in 0..4 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    app.run(
        "board.set_shape",
        json!({"kind": "l_shape", "width": 50.0, "height": 30.0,
            "notch_width": 20.0, "notch_height": 10.0}),
    );
    if app.project.board.outline.as_ref().map(Vec::len) != Some(6) {
        return Err(format!("the L shape was refused: {}", app.message));
    }
    Ok((ctx, app))
}

#[test]
fn the_board_tab_has_no_width_height_or_board_size_control() -> Result<(), String> {
    let (ctx, mut app) = board();
    open_tab(&ctx, &mut app, "Board")?;
    let names = control_names(&ctx, &app);
    for old in ["Width", "Height", "Board size", "Corner size"] {
        assert!(
            !names.iter().any(|n| n.contains(&format!(":{old}"))),
            "the Board tab still has a control named {old}: {names:?}"
        );
    }
    for words in ["Board shape", "Shape of each layer", "Reaches", "Outline"] {
        assert!(paints_text(&ctx, &mut app, words)?, "the tab shows {words}");
    }
    Ok(())
}

#[test]
fn the_shape_card_counts_the_corners_of_an_l_shaped_board() -> Result<(), String> {
    let (ctx, mut app) = l_board()?;
    open_tab(&ctx, &mut app, "Board")?;
    assert!(
        paints_text(&ctx, &mut app, "Free-form")?,
        "an L shape is free-form"
    );
    assert!(
        painted_text_at(&ctx, &mut app, "6")?.is_some(),
        "the Corners row says 6"
    );
    assert!(
        paints_text(&ctx, &mut app, "50.0 × 30.0 mm")?,
        "Reaches is the box around the L"
    );
    Ok(())
}

#[test]
fn reaches_is_read_only() -> Result<(), String> {
    let (ctx, mut app) = board();
    open_tab(&ctx, &mut app, "Board")?;
    let names = control_names(&ctx, &app);
    assert!(
        !names
            .iter()
            .any(|n| n.starts_with("spinbutton:") && n.to_lowercase().contains("reach")),
        "Reaches has no number box: {names:?}"
    );
    let rects = interactive_rects(&ctx, &mut app);
    assert!(
        !rects
            .iter()
            .any(|(n, _)| n.to_lowercase().contains("reach")),
        "Reaches is not a control: {rects:?}"
    );
    Ok(())
}

#[test]
fn a_newcomer_starts_from_a_rectangle_and_one_undo_brings_the_shape_back() -> Result<(), String> {
    let (ctx, mut app) = l_board()?;
    let l_points = app.project.board.outline.clone();
    open_tab(&ctx, &mut app, "Board")?;
    press(&ctx, &mut app, "Start from a rectangle")?;
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    assert!(
        app.project_settings.rectangle_open(),
        "the small window opened"
    );
    type_into(&ctx, &mut app, "spinbutton:Rectangle width", "77")?;
    type_into(&ctx, &mut app, "spinbutton:Rectangle height", "44")?;
    assert_eq!(
        app.project.board.outline.as_ref().map(Vec::len),
        Some(6),
        "typing alone changes nothing"
    );
    let revision = app.revision;
    press(&ctx, &mut app, "Replace the shape")?;
    assert_eq!(app.project.board.width, 77.0, "width: {}", app.message);
    assert_eq!(app.project.board.height, 44.0, "height: {}", app.message);
    assert_eq!(
        app.project.board.outline.as_ref().map(Vec::len),
        Some(4),
        "the shape has four corners"
    );
    assert_eq!(app.revision, revision + 1, "one command");
    assert!(
        paints_text(&ctx, &mut app, "Rectangle, 4 corners")?,
        "the outline is named a rectangle"
    );
    undo(&mut app)?;
    assert_eq!(
        app.project.board.outline, l_points,
        "one Undo puts the L shape back"
    );
    Ok(())
}

#[test]
fn cancelling_the_rectangle_window_leaves_the_shape_alone() -> Result<(), String> {
    let (ctx, mut app) = l_board()?;
    let before = app.project.board.outline.clone();
    open_tab(&ctx, &mut app, "Board")?;
    press(&ctx, &mut app, "Start from a rectangle")?;
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    press(&ctx, &mut app, "Keep the shape")?;
    assert!(
        !app.project_settings.rectangle_open(),
        "the small window closed"
    );
    assert!(app.project_settings.open, "Project settings stays open");
    assert_eq!(app.project.board.outline, before, "the shape is unchanged");
    Ok(())
}

#[test]
fn edit_in_layout_goes_to_the_layout_tab() -> Result<(), String> {
    let (ctx, mut app) = board();
    app.change_tab(Tab::Schematic);
    open_tab(&ctx, &mut app, "Board")?;
    press(&ctx, &mut app, "Edit in Layout")?;
    assert!(!app.project_settings.open, "the dialog closed");
    assert!(matches!(app.tab, Tab::Pcb), "Layout is showing");
    Ok(())
}

#[test]
fn replace_from_svg_or_dxf_opens_the_import_window_on_a_drawing() -> Result<(), String> {
    let (ctx, mut app) = board();
    open_tab(&ctx, &mut app, "Board")?;
    press(&ctx, &mut app, "Replace from SVG or DXF")?;
    assert!(app.edit.dialog.open, "the import window opened");
    assert_eq!(app.edit.dialog.format, "svg_drawing");
    Ok(())
}

#[test]
fn the_layer_table_lists_copper_layers_that_follow_the_board() -> Result<(), String> {
    let (ctx, mut app) = board();
    open_tab(&ctx, &mut app, "Board")?;
    for words in ["Top copper", "Bottom copper", "Follows the board"] {
        assert!(
            paints_text(&ctx, &mut app, words)?,
            "the table shows {words}"
        );
    }
    Ok(())
}

#[test]
fn a_newcomer_builds_a_four_layer_stackup() -> Result<(), String> {
    let (ctx, mut app) = board();
    open_tab(&ctx, &mut app, "Board")?;
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
    app.finish_board_check();
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
        paints_text(&ctx, &mut app, "No machines on this project yet")?,
        "an empty project says so"
    );
    press(&ctx, &mut app, "Add machine")?;
    assert_eq!(
        app.project.machine_profiles.len(),
        1,
        "a machine was added: {}",
        app.message
    );
    waits_for_control(&ctx, &mut app, "button:Remove")?;
    for _ in 0..5 {
        if app.project_settings.machine_open() {
            break;
        }
        press(&ctx, &mut app, "Edit machine")?;
        for _ in 0..3 {
            frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        }
    }
    assert!(
        app.project_settings.machine_open(),
        "Edit opens the machine fields: {}",
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
    // The title block has a REV cell: the caption above, the typed revision below.
    assert!(
        paints_text(&ctx, &mut app, "REV")?,
        "the sheet has a REV cell"
    );
    let output = frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        output.shapes.iter().any(|shape| matches!(
            &shape.shape,
            egui::Shape::Text(text) if text.galley.text() == "B"
        )),
        "the revision shows on the sheet"
    );
    assert!(
        paints_text(&ctx, &mut app, "Sensor Board")?,
        "the title shows on the sheet"
    );
    Ok(())
}

#[test]
fn a_newcomer_sets_revision_and_sees_it_on_the_schematic_title_block() -> Result<(), String> {
    let (ctx, mut app) = board();
    open_tab(&ctx, &mut app, "Title block")?;
    type_into(&ctx, &mut app, "textinput:Revision", "C")?;
    assert_eq!(
        app.project.meta.get("revision").map(String::as_str),
        Some("C"),
        "the revision is stored"
    );
    press(&ctx, &mut app, "Save")?;
    assert!(!app.project_settings.open, "Save closed the dialog");
    app.change_tab(Tab::Schematic);
    // The title block has a REV cell: the caption above, the typed revision below.
    assert!(
        paints_text(&ctx, &mut app, "REV")?,
        "the sheet has a REV cell"
    );
    let output = frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        output.shapes.iter().any(|shape| matches!(
            &shape.shape,
            egui::Shape::Text(text) if text.galley.text() == "C"
        )),
        "the revision shows on the title block"
    );
    Ok(())
}
