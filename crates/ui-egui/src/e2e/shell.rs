use super::super::*;
use super::support::*;

#[test]
fn a_newcomer_reads_the_status_strip() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    // The "Tool: Pick" text left the status strip with the developer wording.
    let pct = (app.schematic.zoom / 15.0 * 100.0).round() as i32;
    let zoom_label = format!("{pct}%");
    assert!(
        paints_text(&ctx, &mut app, &zoom_label)?,
        "the status strip should show the zoom level ({zoom_label}): {}",
        app.message
    );
    let mut mind = 0usize;
    for key in ["erc", "drc"] {
        if let Some(findings) = app.checks.get(key).and_then(serde_json::Value::as_array) {
            for finding in findings {
                let code = finding
                    .get("code")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("finding");
                if app.project.rule_severity(code, "warning") != "error" {
                    mind += 1;
                }
            }
        }
    }
    assert!(
        mind > 0,
        "the practice board should leave something to mind"
    );
    let mind_label = if mind == 1 {
        "1 warning".to_owned()
    } else {
        format!("{mind} warnings")
    };
    assert!(
        paints_text(&ctx, &mut app, &mind_label)?,
        "the status strip should count the warnings ({mind_label}): {}",
        app.message
    );
    authoring::tests::tap(&ctx, &mut app, &mind_label)?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        app.region().bottom_open,
        "tapping the warning count should open the findings sheet: {}",
        app.message
    );
    Ok(())
}
#[test]
fn a_newcomer_adds_assembly_marks_for_the_build_camera() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    app.tab = Tab::Pcb;
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    tap_strip(&ctx, &mut app, "button:Assembly mark")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.canvas_click(Point::new(-27.0, -17.0), 1.0);
    assert_eq!(
        app.project.board.fiducials.len(),
        1,
        "one click lays one mark: {}",
        app.message
    );
    app.canvas_click(Point::new(27.0, 17.0), 1.0);
    assert_eq!(
        app.project.board.fiducials.len(),
        2,
        "the tool stays ready for the next corner: {}",
        app.message
    );
    assert!(
        app.project
            .board
            .fiducials
            .iter()
            .all(|fid| (fid.diameter - 1.0).abs() < 1e-9),
        "both marks are the usual 1 mm disc: {}",
        app.message
    );
    app.tool = Tool::Select;
    app.tab = Tab::Schematic;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.change_tab(Tab::Pcb);
    open_tree_row(&ctx, &mut app, "root");
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        scrolled_to(&ctx, &mut app, "1.00 mm on Top")?,
        "the marks are listed with their size and side"
    );
    let last = app.project.board.fiducials[1].id;
    authoring::tests::tap(&ctx, &mut app, "Tune")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.fiducial_edit = Some((last, 1.2, false));
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Apply")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        app.project.board.fiducials.iter().any(|fid| {
            fid.id == last
                && (fid.diameter - 1.2).abs() < 1e-9
                && fid.layer == pcbcraft_doc::Layer::Back
        }),
        "the mark is resized and moved to the bottom: {}",
        app.message
    );
    // "\u{d7}" also appears in sizes like "60 \u{d7} 40 mm", so press the button by id.
    // Applying a size selects the object; go back to the board's settings.
    open_tree_row(&ctx, &mut app, "root");
    tap_id(&ctx, &mut app, "button:\u{d7}")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert_eq!(
        app.project.board.fiducials.len(),
        1,
        "the other mark comes off the board: {}",
        app.message
    );
    Ok(())
}
#[test]
fn a_newcomer_finds_everything_where_the_workbench_puts_it() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        paints_text(&ctx, &mut app, "Library")?,
        "the left dock is the library browser: {}",
        app.message
    );
    assert!(
        paints_text(&ctx, &mut app, "Layers")?,
        "the left aside leads with the layers tree: {}",
        app.message
    );
    assert!(
        paints_text(&ctx, &mut app, "Properties")?,
        "object details sit in the lower right tab: {}",
        app.message
    );
    authoring::tests::tap(&ctx, &mut app, "Files")?;
    authoring::tests::tap(&ctx, &mut app, "Project settings…")?;
    assert!(
        paints_text(&ctx, &mut app, "Project files")?,
        "build outputs moved under Files: {}",
        app.message
    );
    authoring::tests::tap(&ctx, &mut app, "Layout")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    // The strip scrolls sideways, so reach the tool the way a hand does.
    tap_strip(&ctx, &mut app, "button:Mounting hole")
        .map_err(|e| format!("board tools sit in the strip under the tabs: {e}"))?;
    Ok(())
}

/// Every `.rs` file under `src`, as `(path relative to src, text)`.
fn ui_sources(dir: &std::path::Path, base: &std::path::Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(ui_sources(&path, base));
        } else if path.extension().is_some_and(|x| x == "rs")
            && let (Ok(text), Ok(rel)) = (std::fs::read_to_string(&path), path.strip_prefix(base))
        {
            out.push((rel.to_string_lossy().replace('\\', "/"), text));
        }
    }
    out
}

#[test]
fn no_user_visible_literal_outside_the_catalog() -> Result<(), String> {
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let files = ui_sources(&src, &src);
    assert!(!files.is_empty(), "the ui sources were found");
    let problems = pcbcraft_text::scan::problems(&files);
    assert!(
        problems.is_empty(),
        "text must come from the catalog (crates/text/strings/en.txt):\n{}",
        problems.join("\n")
    );
    Ok(())
}

#[test]
fn board_layout_strings_are_all_catalog_keys() -> Result<(), String> {
    let listed = pcbcraft_text::scan::converted_fns();
    for (file, function) in [
        ("lib.rs", "stackup_section"),
        ("lib.rs", "holes_section"),
        ("lib.rs", "fiducials_section"),
        ("lib.rs", "footer"),
        ("lib.rs", "tool_strip"),
        ("lib.rs", "draw_tool_options"),
        ("lib.rs", "tool_hint"),
        ("lib.rs", "inspector"),
        ("canvas.rs", "canvas"),
        ("canvas.rs", "paint_board"),
        ("canvas.rs", "canvas_click"),
        ("board3d.rs", "show"),
    ] {
        assert!(
            listed.contains(&(file.to_string(), function.to_string())),
            "{file} {function} is listed in converted-fns.txt"
        );
    }
    Ok(())
}

#[test]
fn schematic_strings_are_all_catalog_keys() -> Result<(), String> {
    let listed = pcbcraft_text::scan::converted_fns();
    for (file, function) in [
        ("lib.rs", "nets_tab"),
        ("lib.rs", "object_detail_rows"),
        ("lib.rs", "type_note"),
        ("lib.rs", "empty_state"),
        ("canvas.rs", "paint_schematic"),
    ] {
        assert!(
            listed.contains(&(file.to_string(), function.to_string())),
            "{file} {function} is listed in converted-fns.txt"
        );
    }
    Ok(())
}

#[test]
fn library_strings_are_all_catalog_keys() -> Result<(), String> {
    let listed = pcbcraft_text::scan::converted_fns();
    for (file, function) in [
        ("library.rs", "request"),
        ("library.rs", "search"),
        ("library.rs", "poll"),
        ("library.rs", "collection_browser"),
        ("library.rs", "library_preview"),
        ("authoring.rs", "library_editor"),
        ("authoring.rs", "project_library"),
        ("authoring.rs", "library_workspaces"),
        ("authoring.rs", "graphic"),
        ("authoring.rs", "add_graphic"),
        ("lib.rs", "browser"),
        ("lib.rs", "library"),
        ("lib.rs", "files_tab"),
        ("library_quality.rs", "quality_badge"),
        ("library_quality.rs", "library_checks_section"),
        ("library_quality.rs", "refresh_footprint_quality"),
    ] {
        assert!(
            listed.contains(&(file.to_string(), function.to_string())),
            "{file} {function} is listed in converted-fns.txt"
        );
    }
    Ok(())
}

#[test]
fn rules_strings_are_all_catalog_keys() -> Result<(), String> {
    let listed = pcbcraft_text::scan::converted_fns();
    for (file, function) in [
        ("lib.rs", "rules_tab"),
        ("lib.rs", "param_help"),
        ("lib.rs", "field_help"),
    ] {
        assert!(
            listed.contains(&(file.to_string(), function.to_string())),
            "{file} {function} is listed in converted-fns.txt"
        );
    }
    Ok(())
}

#[test]
fn checks_strings_are_all_catalog_keys() -> Result<(), String> {
    let listed = pcbcraft_text::scan::converted_fns();
    for (file, function) in [
        ("lib.rs", "checks_view"),
        ("lib.rs", "human_code"),
        ("lib.rs", "custom_heading"),
        ("lib.rs", "custom_measure_line"),
        ("lib.rs", "empty_state"),
    ] {
        assert!(
            listed.contains(&(file.to_string(), function.to_string())),
            "{file} {function} is listed in converted-fns.txt"
        );
    }
    Ok(())
}

#[test]
fn settings_strings_are_all_catalog_keys() -> Result<(), String> {
    let listed = pcbcraft_text::scan::converted_fns();
    for (file, function) in [("lib.rs", "settings_tab"), ("lib.rs", "shortcuts")] {
        assert!(
            listed.contains(&(file.to_string(), function.to_string())),
            "{file} {function} is listed in converted-fns.txt"
        );
    }
    Ok(())
}

#[test]
fn docks_strings_are_all_catalog_keys() -> Result<(), String> {
    let listed = pcbcraft_text::scan::converted_fns();
    for (file, function) in [
        ("lib.rs", "dock_frame"),
        ("lib.rs", "workbench_tabs"),
        ("lib.rs", "builds_tab"),
        ("lib.rs", "shortcut_rows"),
        ("lib.rs", "command_tip"),
    ] {
        assert!(
            listed.contains(&(file.to_string(), function.to_string())),
            "{file} {function} is listed in converted-fns.txt"
        );
    }
    Ok(())
}

#[test]
fn dialogs_strings_are_all_catalog_keys() -> Result<(), String> {
    let listed = pcbcraft_text::scan::converted_fns();
    for (file, function) in [
        ("lib.rs", "start_home"),
        ("lib.rs", "execute_palette_params"),
        ("lib.rs", "params_template"),
        ("lib.rs", "run"),
        ("lib.rs", "run_and_keep"),
        ("lib.rs", "tool_call"),
        ("lib.rs", "refusal_text"),
        ("editing.rs", "shift_click"),
        ("editing.rs", "move_selection"),
        ("editing.rs", "delete_selection"),
        ("editing.rs", "duplicate_selection"),
        ("editing.rs", "copy_selection"),
        ("editing.rs", "paste_last_copy_at"),
        ("editing.rs", "paste_text_at"),
        ("editing.rs", "paste_last_copy"),
        ("editing.rs", "measure_click"),
        ("editing.rs", "cleanup_tracks"),
        ("lib.rs", "write_make_files"),
        ("lib.rs", "write_uv_print_files"),
        ("lib.rs", "write_job_file"),
        ("lib.rs", "capture"),
        ("lib.rs", "copy_plot_to_clipboard"),
    ] {
        assert!(
            listed.contains(&(file.to_string(), function.to_string())),
            "{file} {function} is listed in converted-fns.txt"
        );
    }
    Ok(())
}

#[test]
fn a_newcomer_switches_the_app_to_their_language() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(
        shared.clone(),
        Preferences {
            language: "en".into(),
            ..Preferences::default()
        },
        None,
        None,
    );
    app.refresh();
    let ctx = egui::Context::default();
    assert!(
        paints_text(&ctx, &mut app, "Settings")?,
        "the app opens in English and the top bar says Settings"
    );
    assert!(
        !paints_text(&ctx, &mut app, "Paramètres")?,
        "no French shows yet"
    );
    // The person opens the Settings page and picks a language.
    app.change_tab(Tab::Settings);
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Language")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Français (test)")?;
    assert!(
        paints_text(&ctx, &mut app, "Fichier")?,
        "the File menu now reads Fichier at once, with no restart: {}",
        app.message
    );
    assert!(
        paints_text(&ctx, &mut app, "Aide")?,
        "the Help menu reads Aide"
    );
    assert_eq!(
        app.preferences.language, "qps-fr",
        "the choice is remembered"
    );
    // Closing and reopening the app keeps the saved choice.
    let saved = app.preferences.clone();
    let mut reopened = App::create(shared, saved, None, None);
    reopened.refresh();
    assert!(
        paints_text(&ctx, &mut reopened, "Fichier")?,
        "the reopened app still reads French"
    );
    // A language tag the app does not carry quietly shows English.
    reopened.set_language("zz");
    assert!(
        paints_text(&ctx, &mut reopened, "File")?,
        "an unknown language falls back to English"
    );
    Ok(())
}

#[test]
fn a_newcomer_reads_the_app_in_spanish() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(
        shared.clone(),
        Preferences {
            language: "en".into(),
            ..Preferences::default()
        },
        None,
        None,
    );
    app.refresh();
    let ctx = egui::Context::default();
    assert!(
        paints_text(&ctx, &mut app, "File")?,
        "the app opens in English and the menu bar says File"
    );
    assert!(
        !paints_text(&ctx, &mut app, "Archivo")?,
        "no Spanish shows yet"
    );
    // The person opens the Settings page and picks Español from the list.
    app.change_tab(Tab::Settings);
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Language")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Español")?;
    assert_eq!(app.preferences.language, "es", "the choice is remembered");
    assert!(
        paints_text(&ctx, &mut app, "Archivo")?,
        "the File menu now reads Archivo at once, with no restart: {}",
        app.message
    );
    assert!(
        paints_text(&ctx, &mut app, "Edición")?,
        "the Edit menu reads Edición"
    );
    assert!(
        paints_text(&ctx, &mut app, "Ayuda")?,
        "the Help menu reads Ayuda"
    );
    // Closing and reopening the app keeps the saved language.
    let saved = app.preferences.clone();
    let mut reopened = App::create(shared, saved, None, None);
    reopened.refresh();
    assert!(
        paints_text(&ctx, &mut reopened, "Archivo")?,
        "the reopened app still reads Spanish"
    );
    Ok(())
}

#[test]
fn a_newcomer_reads_the_app_in_german() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(
        shared.clone(),
        Preferences {
            language: "en".into(),
            ..Preferences::default()
        },
        None,
        None,
    );
    app.refresh();
    let ctx = egui::Context::default();
    assert!(
        paints_text(&ctx, &mut app, "File")?,
        "the app opens in English and the menu bar says File"
    );
    assert!(
        !paints_text(&ctx, &mut app, "Datei")?,
        "no German shows yet"
    );
    // The person opens the Settings page and picks Deutsch from the list.
    app.change_tab(Tab::Settings);
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Language")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Deutsch")?;
    assert_eq!(app.preferences.language, "de", "the choice is remembered");
    assert!(
        paints_text(&ctx, &mut app, "Datei")?,
        "the File menu now reads Datei at once, with no restart: {}",
        app.message
    );
    assert!(
        paints_text(&ctx, &mut app, "Bearbeiten")?,
        "the Edit menu reads Bearbeiten"
    );
    assert!(
        paints_text(&ctx, &mut app, "Hilfe")?,
        "the Help menu reads Hilfe"
    );
    // Closing and reopening the app keeps the saved language.
    let saved = app.preferences.clone();
    let mut reopened = App::create(shared, saved, None, None);
    reopened.refresh();
    assert!(
        paints_text(&ctx, &mut reopened, "Datei")?,
        "the reopened app still reads German"
    );
    Ok(())
}

#[test]
fn a_newcomer_reads_the_app_in_french() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(
        shared.clone(),
        Preferences {
            language: "en".into(),
            ..Preferences::default()
        },
        None,
        None,
    );
    app.refresh();
    let ctx = egui::Context::default();
    assert!(
        paints_text(&ctx, &mut app, "File")?,
        "the app opens in English and the menu bar says File"
    );
    assert!(
        !paints_text(&ctx, &mut app, "Fichier")?,
        "no French shows yet"
    );
    // The person opens the Settings page and picks Français from the list.
    app.change_tab(Tab::Settings);
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Language")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Français")?;
    assert_eq!(app.preferences.language, "fr", "the choice is remembered");
    assert!(
        paints_text(&ctx, &mut app, "Fichier")?,
        "the File menu now reads Fichier at once, with no restart: {}",
        app.message
    );
    assert!(
        paints_text(&ctx, &mut app, "Édition")?,
        "the Edit menu reads Édition"
    );
    assert!(
        paints_text(&ctx, &mut app, "Aide")?,
        "the Help menu reads Aide"
    );
    // Closing and reopening the app keeps the saved language.
    let saved = app.preferences.clone();
    let mut reopened = App::create(shared, saved, None, None);
    reopened.refresh();
    assert!(
        paints_text(&ctx, &mut reopened, "Fichier")?,
        "the reopened app still reads French"
    );
    Ok(())
}

#[test]
fn a_newcomer_picks_the_move_tool_from_the_strip_icons() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Layout")?;
    tap_strip(&ctx, &mut app, "icon:tool-move")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        matches!(app.tool, Tool::Move),
        "tapping the Move icon selects the Move tool"
    );
    Ok(())
}

/// Where a text label is painted: its vertical centre, if it is on screen.
fn text_row(ctx: &egui::Context, app: &mut App, needle: &str) -> Option<f32> {
    for _ in 0..3 {
        let output = frame(ctx, app, vec![], egui::Modifiers::NONE);
        for shape in &output.shapes {
            if let egui::Shape::Text(text) = &shape.shape
                && text.galley.text() == needle
            {
                return Some(text.pos.y + text.galley.size().y / 2.0);
            }
        }
    }
    None
}

#[test]
fn a_newcomer_sees_a_slim_bar_with_only_the_brand_and_the_menus() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    let ctx = egui::Context::default();
    for needle in ["PCBCraft", "File", "Edit", "Help"] {
        let row =
            text_row(&ctx, &mut app, needle).ok_or_else(|| format!("{needle} is not on screen"))?;
        assert!(
            row < App::APP_BAR_HEIGHT,
            "{needle} sits in the 34 px bar, found at {row}"
        );
    }
    for gone in [
        "Home",
        "Report a bug",
        "Save",
        "Undo",
        "Redo",
        env!("CARGO_PKG_VERSION"),
    ] {
        assert!(
            text_row(&ctx, &mut app, gone).is_none_or(|row| row > App::APP_BAR_HEIGHT),
            "{gone} no longer sits in the bar"
        );
    }
    authoring::tests::tap(&ctx, &mut app, "Help")?;
    for needle in ["Shortcuts", "Commands", "Report a bug"] {
        assert!(paints_text(&ctx, &mut app, needle)?, "Help lists {needle}");
    }
    authoring::tests::tap(&ctx, &mut app, "File")?;
    for needle in ["New", "Open", "Save", "Settings", "Export", "Copy picture"] {
        assert!(paints_text(&ctx, &mut app, needle)?, "File lists {needle}");
    }
    Ok(())
}

#[test]
fn a_newcomer_cannot_undo_before_the_first_edit() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    let ctx = egui::Context::default();
    authoring::tests::tap(&ctx, &mut app, "Edit")?;
    authoring::tests::tap(&ctx, &mut app, "Undo")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        !app.message.starts_with("undo"),
        "Undo is greyed out with nothing to undo: {}",
        app.message
    );
    Ok(())
}

#[test]
fn a_newcomer_reads_plain_tool_names_and_closes_and_reopens_a_view() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    let ctx = egui::Context::default();
    authoring::tests::tap(&ctx, &mut app, "Layout")?;
    for word in ["Select", "Move", "Route", "Line", "Box", "Circle", "More"] {
        assert!(paints_text(&ctx, &mut app, word)?, "the strip says {word}");
    }
    // What does not fit the row waits in the More menu, under its plain name.
    authoring::tests::tap(&ctx, &mut app, "More")?;
    for word in ["Outline", "Text"] {
        assert!(paints_text(&ctx, &mut app, word)?, "More lists {word}");
    }
    frame(
        &ctx,
        &mut app,
        vec![key(egui::Key::Escape)],
        egui::Modifiers::NONE,
    );
    tap_strip(&ctx, &mut app, "icon:tool-words")?;
    assert!(
        matches!(app.tool, Tool::DrawLayer) && app.art_shape == DrawShape::Words,
        "the strip offers the Text tool"
    );
    for cryptic in ["Sel", "Mv", "Rt", "Ln", "Bx", "Cr", "Pl", "Tx"] {
        assert!(text_row(&ctx, &mut app, cryptic).is_none(), "no {cryptic}");
    }
    authoring::tests::open_page(&ctx, &mut app, "Rules")?;
    assert!(
        matches!(app.tab, Tab::Rules),
        "View > Rules opens the rules page"
    );
    authoring::tests::tap(&ctx, &mut app, "Files")?;
    authoring::tests::tap(&ctx, &mut app, "Project settings…")?;
    authoring::tests::tap(&ctx, &mut app, "Library sources")?;
    assert!(
        matches!(app.tab, Tab::Libraries),
        "Library sources stays reachable"
    );
    Ok(())
}

#[test]
fn a_newcomer_draws_from_the_tool_strip() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Layout")?;
    tap_strip(&ctx, &mut app, "icon:tool-line")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        matches!(app.tool, Tool::DrawLayer) && app.art_shape == DrawShape::Line,
        "tapping Line in the strip arms the line tool"
    );
    assert!(
        scrolled_to(&ctx, &mut app, "Line")?,
        "shape options appear in Properties: {}",
        app.message
    );
    Ok(())
}

#[test]
fn a_newcomer_narrows_the_window_and_the_canvas_buttons_stay_inside() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    app.select(Some(app.project.components[0].id));
    for width in [1440.0_f32, 1000.0] {
        let ctx = egui::Context::default();
        for _ in 0..4 {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        Vec2::new(width, 940.0),
                    )),
                    ..Default::default()
                },
                |ui| app.show(ui),
            );
            output.textures_delta.clear();
        }
        let ids: &[&str] = if width >= 1440.0 {
            &["canvas-fit", "handle-rotate"]
        } else {
            &["canvas-fit"]
        };
        for id in ids.iter().copied() {
            let rect = app
                .icon_rects
                .get(id)
                .copied()
                .ok_or_else(|| format!("{id} was not drawn at width {width}"))?;
            let aside = if width >= 1440.0 { 280.0 } else { 0.0 };
            assert!(
                rect.min.x >= 0.0 && rect.max.x <= width - aside,
                "{id} stays on the canvas at width {width}: {rect:?}"
            );
        }
    }
    Ok(())
}

fn key(key: egui::Key) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: Some(key),
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }
}
fn practice_app() -> (egui::Context, App) {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    (ctx, app)
}
/// Press Tab until the named control holds focus; the order it took is
/// returned so a story can say what a keyboard user walks past.
fn tab_to(ctx: &egui::Context, app: &mut App, name: &str) -> Result<Vec<String>, String> {
    let mut seen = Vec::new();
    for _ in 0..250 {
        let out = frame(ctx, app, vec![key(egui::Key::Tab)], egui::Modifiers::NONE);
        if let Some(label) = a11y::audit::focused_name(ctx, &out) {
            seen.push(label.clone());
            if label == name {
                return Ok(seen);
            }
        }
    }
    Err(format!("Tab never reached {name}; it passed {seen:?}"))
}

#[test]
fn a_newcomer_opens_settings_from_the_app_bar_with_the_keyboard_only() -> Result<(), String> {
    let (ctx, mut app) = practice_app();
    let seen = tab_to(&ctx, &mut app, "Edit")?;
    assert_eq!(
        seen.first().map(String::as_str),
        Some("PCBCraft start screen"),
        "the first stop is the app name: {seen:?}"
    );
    assert!(
        seen.iter().any(|n| n == "File") && seen.iter().any(|n| n == "Edit"),
        "the app bar menus come in reading order: {seen:?}"
    );
    let shown = frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        shown.shapes.iter().any(|s| matches!(&s.shape,
            egui::Shape::Rect(r) if r.stroke.width >= 2.0 && r.stroke.color == app.accent())),
        "the focused control wears a visible ring"
    );
    frame(
        &ctx,
        &mut app,
        vec![key(egui::Key::Enter)],
        egui::Modifiers::NONE,
    );
    let more = tab_to(&ctx, &mut app, "Preferences")?;
    assert!(
        more.iter().any(|n| n == "Preferences"),
        "the Edit menu items are reachable: {more:?}"
    );
    frame(
        &ctx,
        &mut app,
        vec![key(egui::Key::Enter)],
        egui::Modifiers::NONE,
    );
    assert_eq!(app.tab, Tab::Settings, "Enter presses it: {}", app.message);
    Ok(())
}

#[test]
fn a_newcomer_switches_view_with_the_keyboard_only() -> Result<(), String> {
    let (ctx, mut app) = practice_app();
    tab_to(&ctx, &mut app, "Layout")?;
    frame(
        &ctx,
        &mut app,
        vec![key(egui::Key::Space)],
        egui::Modifiers::NONE,
    );
    assert_eq!(app.tab, Tab::Pcb, "Space opens the layout: {}", app.message);
    // Tool buttons are reachable too and say what they do, not "tool-route".
    let seen = tab_to(&ctx, &mut app, "Route")?;
    assert!(
        seen.iter().any(|n| n == "Select"),
        "tool names are spoken: {seen:?}"
    );
    Ok(())
}

/// Known unnamed controls, as a count per role: `role | count | #issue | reason`.
const EXEMPTIONS: &str = include_str!("accessible-name-exemptions.txt");

/// Every screen a person can reach without typing: the start page, each tab,
/// and both pages of the lower right dock. Returns one line per unnamed
/// interactive control in the form the exemption file uses (without the
/// issue and reason), and fails if a screen exposes no controls at all.
fn unnamed_controls() -> Result<Vec<String>, String> {
    let mut found = Vec::new();
    for lower in [0u8, 1u8] {
        for tab in [
            Tab::Files,
            Tab::Schematic,
            Tab::Pcb,
            Tab::Board3d,
            Tab::Rules,
            Tab::Checks,
            Tab::Settings,
        ] {
            let (ctx, mut app) = practice_app();
            app.preferences.aside_lower_tab = lower;
            app.change_tab(tab);
            frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
            let out = frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
            let nodes = a11y::audit::nodes(&out);
            if nodes.is_empty() {
                return Err(format!("{tab:?} exposes no controls to a screen reader"));
            }
            for (id, role, name) in nodes {
                if name.is_empty() {
                    found.push(format!("{id} | {role:?}"));
                }
            }
        }
    }
    let (ctx, mut app) = practice_app();
    app.home_open = true;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let out = frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    for (id, role, name) in a11y::audit::nodes(&out) {
        if name.is_empty() {
            found.push(format!("{id} | {role:?}"));
        }
    }
    found.sort();
    found.dedup();
    Ok(found)
}

/// Unnamed controls counted per role, from the lines of the exemption file
/// (`role | count | #issue | reason`).
fn budget() -> Vec<(String, usize, String)> {
    EXEMPTIONS
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| {
            let parts: Vec<&str> = l.splitn(4, '|').map(str::trim).collect();
            (
                parts.first().copied().unwrap_or_default().to_owned(),
                parts.get(1).and_then(|n| n.parse().ok()).unwrap_or(0),
                l.to_owned(),
            )
        })
        .collect()
}

/// How many distinct unnamed controls each role has right now.
fn unnamed_by_role() -> Result<Vec<(String, usize)>, String> {
    let mut counts: Vec<(String, usize)> = Vec::new();
    for line in unnamed_controls()? {
        let role = line.split('|').nth(1).unwrap_or_default().trim().to_owned();
        match counts.iter_mut().find(|(r, _)| *r == role) {
            Some((_, n)) => *n += 1,
            None => counts.push((role, 1)),
        }
    }
    Ok(counts)
}

#[test]
fn every_interactive_control_has_an_accessible_name() -> Result<(), String> {
    let allowed = budget();
    let mut worse = Vec::new();
    for (role, now) in unnamed_by_role()? {
        let limit = allowed
            .iter()
            .find(|(r, _, _)| *r == role)
            .map_or(0, |(_, n, _)| *n);
        if now > limit {
            worse.push(format!("{role}: {now} unnamed, {limit} allowed"));
        }
    }
    assert!(
        worse.is_empty(),
        "a new control has no accessible name; name it with a11y::name_control \
         or label it, and only as a last resort raise its count in \
         accessible-name-exemptions.txt with an issue number: {worse:?}\nunnamed now: {:?}",
        unnamed_controls()?
    );
    Ok(())
}

#[test]
fn the_exemption_file_has_no_entry_for_a_control_that_is_now_named() -> Result<(), String> {
    let now = unnamed_by_role()?;
    let mut stale = Vec::new();
    for (role, limit, line) in budget() {
        let have = now.iter().find(|(r, _)| *r == role).map_or(0, |(_, n)| *n);
        if have < limit {
            stale.push(format!("{line} (only {have} unnamed now; lower the count)"));
        }
    }
    assert!(
        stale.is_empty(),
        "controls got names; shrink the list: {stale:?}"
    );
    Ok(())
}

#[test]
fn each_exemption_names_an_issue_number() -> Result<(), String> {
    for (_, _, line) in budget() {
        let fields: Vec<&str> = line.splitn(4, '|').map(str::trim).collect();
        let count_ok = fields.get(1).is_some_and(|n| n.parse::<usize>().is_ok());
        let issue_ok = fields.get(2).is_some_and(|f| {
            f.strip_prefix('#')
                .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
        });
        let reason_ok = fields.get(3).is_some_and(|r| !r.is_empty());
        assert!(
            count_ok && issue_ok && reason_ok,
            "needs `role | count | #issue | reason`: {line}"
        );
    }
    Ok(())
}

#[test]
fn preferences_ui_scale_clamps_and_persists() -> Result<(), String> {
    assert_eq!(a11y::clamp_ui_scale(9.0), a11y::MAX_UI_SCALE);
    assert_eq!(a11y::clamp_ui_scale(0.1), a11y::MIN_UI_SCALE);
    assert_eq!(a11y::clamp_ui_scale(f32::NAN), a11y::DEFAULT_UI_SCALE);
    let prefs = Preferences {
        ui_scale: 1.5,
        ..Preferences::default()
    };
    let text = serde_json::to_string(&prefs).map_err(|e| e.to_string())?;
    let back: Preferences = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    assert_eq!(back.ui_scale, 1.5, "the scale survives a restart");
    let mut old = serde_json::to_value(Preferences::default()).map_err(|e| e.to_string())?;
    if let Some(map) = old.as_object_mut() {
        map.remove("ui_scale");
    }
    let legacy: Preferences = serde_json::from_value(old).map_err(|e| e.to_string())?;
    assert_eq!(
        legacy.ui_scale, 1.0,
        "older saved settings load at normal size"
    );
    let (ctx, mut app) = practice_app();
    app.preferences.ui_scale = 1.5;
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    assert!(
        (ctx.zoom_factor() - 1.5).abs() < 1e-3,
        "the window rescales"
    );
    app.preferences.ui_scale = 40.0;
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    assert_eq!(
        ctx.zoom_factor(),
        a11y::MAX_UI_SCALE,
        "absurd values are held in range"
    );
    Ok(())
}

#[test]
fn a_newcomer_with_a_screen_reader_finds_the_fit_move_and_rotate_buttons_by_name()
-> Result<(), String> {
    let (ctx, mut app) = practice_app();
    app.change_tab(Tab::Pcb);
    // Move and Rotate appear beside a selected part.
    app.select(Some(app.project.components[0].id));
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let out = frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let nodes = a11y::audit::interactive_nodes(&out);
    for name in ["Fit", "Move", "Rotate"] {
        assert!(
            nodes
                .iter()
                .any(|(role, label)| *role == egui::accesskit::Role::Button && label == name),
            "a screen reader should find a button named {name}: {nodes:?}"
        );
    }
    Ok(())
}

#[test]
fn the_accessibility_tree_is_empty_until_the_screen_reader_is_switched_on() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    let ctx = egui::Context::default();
    // `frame` switches the tree on, so run the first pass by hand.
    let mut off = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                Vec2::new(1440.0, 940.0),
            )),
            ..Default::default()
        },
        |ui| app.show(ui),
    );
    off.textures_delta.clear();
    assert!(
        off.platform_output.accesskit_update.is_none(),
        "nothing is built for a person who has no screen reader"
    );
    ctx.enable_accesskit();
    let on = frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        on.platform_output.accesskit_update.is_some(),
        "switching a screen reader on starts the tree"
    );
    Ok(())
}

/// Text of every polite live region in this frame's accessibility tree.
fn polite_messages(out: &egui::FullOutput) -> Vec<String> {
    let Some(update) = &out.platform_output.accesskit_update else {
        return Vec::new();
    };
    update
        .nodes
        .iter()
        .filter(|(_, node)| node.live() == Some(egui::accesskit::Live::Polite))
        .filter_map(|(_, node)| node.label().map(str::to_owned))
        .collect()
}

#[test]
fn a_newcomer_with_a_screen_reader_hears_the_status_after_a_command() -> Result<(), String> {
    let (ctx, mut app) = practice_app();
    app.tool_call("sample_design", json!({}));
    assert!(!app.message.is_empty(), "the command reports itself");
    let out = frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let heard = polite_messages(&out);
    assert!(
        heard.contains(&app.message),
        "the screen reader is told '{}' without focus moving: {heard:?}",
        app.message
    );
    Ok(())
}

#[test]
fn an_unchanged_message_is_not_announced_twice() -> Result<(), String> {
    let (ctx, mut app) = practice_app();
    app.message = "net.create · committed".into();
    let first = polite_messages(&frame(&ctx, &mut app, vec![], egui::Modifiers::NONE));
    let again = polite_messages(&frame(&ctx, &mut app, vec![], egui::Modifiers::NONE));
    assert_eq!(
        first, again,
        "same words, same node text, so nothing new is read"
    );
    assert!(first.iter().any(|m| m == "net.create · committed"));
    app.message = "undo · reverted".into();
    let next = polite_messages(&frame(&ctx, &mut app, vec![], egui::Modifiers::NONE));
    assert!(
        next.iter().any(|m| m == "undo · reverted")
            && !next.iter().any(|m| m == "net.create · committed"),
        "a new message replaces the old text: {next:?}"
    );
    Ok(())
}

#[test]
fn a_newcomer_never_meets_a_control_too_small_to_hit() -> Result<(), String> {
    use crate::target_size::{exemptions, undersized_targets};
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    let ctx = egui::Context::default();
    let mut too_small = Vec::new();
    let mut seen = 0;
    for tab in [
        Tab::Files,
        Tab::Schematic,
        Tab::Pcb,
        Tab::Board3d,
        Tab::Rules,
        Tab::Checks,
        Tab::Settings,
        Tab::Libraries,
        Tab::PlotFiles,
    ] {
        app.change_tab(tab);
        let rects = interactive_rects(&ctx, &mut app);
        seen += rects.len();
        for line in undersized_targets(&rects, App::MIN_TARGET) {
            too_small.push(format!("{tab:?}: {line}"));
        }
    }
    // Selecting a part opens the properties panel and the canvas handles.
    let first = app.project.components.first().map(|c| c.id);
    app.select(first);
    for tab in [Tab::Pcb, Tab::Schematic] {
        app.change_tab(tab);
        let rects = interactive_rects(&ctx, &mut app);
        seen += rects.len();
        for line in undersized_targets(&rects, App::MIN_TARGET) {
            too_small.push(format!("{tab:?} with a part selected: {line}"));
        }
    }
    assert!(
        seen > 20,
        "the walk should find real controls, found {seen}"
    );
    let exempt = exemptions()?;
    let (allowed, failing): (Vec<_>, Vec<_>) = too_small
        .iter()
        .partition(|l| exempt.iter().any(|(e, _)| l.contains(e.as_str())));
    assert!(
        failing.is_empty(),
        "controls under 24 by 24:\n{}",
        failing
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    );
    for (name, reason) in &exempt {
        assert!(
            allowed.iter().any(|l| l.contains(name.as_str())),
            "exemption {name} ({reason}) is no longer undersized; remove it"
        );
    }
    Ok(())
}

/// Bounds of every painted image: a rectangle filled from a texture other
/// than the font atlas (the first managed texture).
fn image_rects(output: &egui::FullOutput) -> Vec<egui::Rect> {
    output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Rect(rect)
                if !matches!(rect.fill_texture_id(), egui::TextureId::Managed(0)) =>
            {
                Some(rect.visual_bounding_rect())
            }
            egui::Shape::Mesh(mesh) if !matches!(mesh.texture_id, egui::TextureId::Managed(0)) => {
                Some(mesh.calc_bounds())
            }
            _ => None,
        })
        .collect()
}

#[test]
fn a_newcomer_sees_the_brand_mark_in_the_app_bar() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let output = frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        image_rects(&output).iter().any(|r| r.min.y < 40.0),
        "the app bar should show the mark beside the name"
    );
    assert!(
        paints_text(&ctx, &mut app, "PCBCraft")?,
        "the app bar still says the name: {}",
        app.message
    );
    Ok(())
}

#[test]
fn a_newcomer_reads_about_pcbcraft_from_the_help_menu() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Help")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "About")?;
    assert!(app.about_open, "About should open from the Help menu");
    assert!(
        paints_text(&ctx, &mut app, "About PCBCraft")?,
        "the window has a plain title: {}",
        app.message
    );
    let version = format!("Version {}", env!("CARGO_PKG_VERSION"));
    assert!(
        paints_text(&ctx, &mut app, &version)?,
        "the window says which version this is"
    );
    authoring::tests::tap(&ctx, &mut app, "Close")?;
    assert!(!app.about_open, "Close puts the window away");
    Ok(())
}

#[test]
fn a_newcomer_sees_the_mark_on_the_start_page() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.home_open = true;
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let output = frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let large = image_rects(&output)
        .iter()
        .filter(|r| r.width() > 80.0)
        .count();
    assert!(large >= 1, "the start page should show the large mark");
    assert!(
        paints_text(&ctx, &mut app, "Welcome to PCBCraft")?,
        "the welcome line is still there: {}",
        app.message
    );
    Ok(())
}

#[test]
fn a_newcomer_sees_that_saving_needs_the_desktop_app() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.browser = true;
    app.refresh();
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    for tool in ["save", "open", "export"] {
        app.tool_call(tool, json!({"path": "design.pcbcraft"}));
        assert!(
            paints_text(&ctx, &mut app, "need the desktop app")?,
            "{tool} should say it needs the desktop app: {}",
            app.message
        );
        assert!(
            !std::path::Path::new("design.pcbcraft").exists(),
            "{tool} must not write a file in the browser"
        );
    }
    Ok(())
}
