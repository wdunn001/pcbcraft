//! Stories for Edit > Rules and checks: opening it, the results list and the strictness
//! of a check, all through the real menu, tabs and dropdowns.
//!
//! Context: issue #1052. The window is `crates/ui-egui/src/rules_dialog.rs`.
use super::super::*;
use super::support::*;

/// The sample board on the Layout tab, a few frames drawn.
fn board() -> (egui::Context, App) {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    app.change_tab(Tab::Pcb);
    let ctx = egui::Context::default();
    for _ in 0..4 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    (ctx, app)
}

/// Edit > Rules and checks…
fn open_from_menu(ctx: &egui::Context, app: &mut App) -> Result<(), String> {
    authoring::tests::tap(ctx, app, "Edit")?;
    authoring::tests::tap(ctx, app, "Rules and checks…")?;
    for _ in 0..3 {
        frame(ctx, app, vec![], egui::Modifiers::NONE);
    }
    Ok(())
}

/// Press and release one key.
fn key(ctx: &egui::Context, app: &mut App, key: egui::Key, modifiers: egui::Modifiers) {
    for pressed in [true, false] {
        frame(
            ctx,
            app,
            vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers,
            }],
            modifiers,
        );
    }
    frame(ctx, app, vec![], egui::Modifiers::NONE);
}

#[test]
fn a_newcomer_opens_rules_and_checks_from_the_edit_menu() -> Result<(), String> {
    let (ctx, mut app) = board();
    open_from_menu(&ctx, &mut app)?;
    assert!(app.rules_dialog.open, "the menu item opened the window");
    for words in ["Rules and checks", "Results", "Rules", "Run checks"] {
        assert!(
            paints_text(&ctx, &mut app, words)?,
            "the window shows {words}"
        );
    }
    tap_id(&ctx, &mut app, "tab:Rules")?;
    key(&ctx, &mut app, egui::Key::Escape, egui::Modifiers::NONE);
    assert!(!app.rules_dialog.open, "Escape closes the window");
    open_from_menu(&ctx, &mut app)?;
    assert!(
        paints_text(&ctx, &mut app, "Switch a check off")?,
        "it opens on the tab last used"
    );
    key(&ctx, &mut app, egui::Key::Escape, egui::Modifiers::NONE);
    let chord = egui::Modifiers {
        ctrl: true,
        shift: true,
        command: true,
        ..Default::default()
    };
    key(&ctx, &mut app, egui::Key::R, chord);
    assert!(app.rules_dialog.open, "Ctrl+Shift+R opens the window");
    Ok(())
}

#[test]
fn a_newcomer_turns_a_check_down_to_a_warning() -> Result<(), String> {
    let (ctx, mut app) = board();
    open_from_menu(&ctx, &mut app)?;
    tap_id(&ctx, &mut app, "tab:Rules")?;
    tap_id(&ctx, &mut app, "combobox:Clearance")?;
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    let picked = ["button", "radiobutton", "menuitem", "selectable"]
        .iter()
        .any(|role| tap_id(&ctx, &mut app, &format!("{role}:Warning")).is_ok());
    if !picked {
        authoring::tests::tap(&ctx, &mut app, "Warning")?;
    }
    assert_eq!(
        app.project
            .rule_severity
            .get("clearance")
            .map(String::as_str),
        Some("warning"),
        "Clearance now reports warnings: {}",
        app.message
    );
    tap_id(&ctx, &mut app, "checkbox:Clearance is on")?;
    assert_eq!(
        app.project
            .rule_severity
            .get("clearance")
            .map(String::as_str),
        Some("off"),
        "unticking switches the check off: {}",
        app.message
    );
    Ok(())
}

#[test]
fn a_newcomer_runs_checks_and_reads_the_results() -> Result<(), String> {
    let (ctx, mut app) = board();
    open_from_menu(&ctx, &mut app)?;
    tap_id(&ctx, &mut app, "button:Run checks")?;
    assert_eq!(
        app.message, "Checks ran again.",
        "the button ran the checks"
    );
    for words in [
        "Errors",
        "Warnings",
        "All areas",
        "Electrical",
        "Board",
        "Library",
    ] {
        assert!(
            paints_text(&ctx, &mut app, words)?,
            "the results show {words}"
        );
    }
    Ok(())
}

/// Issue #110: a mounting hole drilled with the real tool, a track routed close
/// to it, and a fab limit; Run checks names the problem in plain words.
#[test]
fn a_newcomer_is_told_a_mounting_hole_sits_too_close_to_copper() -> Result<(), String> {
    let (ctx, mut app) = board();
    app.run("board.fab_limits.set", json!({"hole_to_copper_npth": 0.4}));
    tap_strip(&ctx, &mut app, "button:Mounting hole")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.canvas_click(Point::new(-25.0, -17.0), 1.0);
    app.tool = Tool::Select;
    // The default drill is 3.2 mm, so its wall is 1.6 mm out. A 0.2 mm track
    // centred 1.95 mm out leaves 0.25 mm of air, under the 0.4 mm limit.
    let net = app.project.nets.first().ok_or("the sample has nets")?.id;
    app.run(
        "track.add",
        json!({"net":net,"start":{"x":-35.0,"y":-15.05},"end":{"x":-15.0,"y":-15.05},"width":0.2,"layer":"F.Cu"}),
    );
    app.refresh();
    assert!(
        pcbcraft_rules::drc(&app.project)
            .iter()
            .any(|f| f.code == "hole_to_copper_npth"),
        "the engine reports the close copper: {}",
        app.message
    );
    open_from_menu(&ctx, &mut app)?;
    tap_id(&ctx, &mut app, "button:Run checks")?;
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    assert!(
        paints_text(&ctx, &mut app, "Unplated hole to copper")?,
        "the results name the finding in plain words: {}",
        app.message
    );
    Ok(())
}
