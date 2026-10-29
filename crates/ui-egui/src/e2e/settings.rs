use super::super::*;
use super::support::*;

fn fresh() -> App {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app
}

#[test]
fn a_newcomer_visits_settings_and_comes_back_to_the_same_zoom() -> Result<(), String> {
    let mut app = fresh();
    let ctx = egui::Context::default();
    app.change_tab(Tab::Pcb);
    // The asides settle over the first frames; set the zoom after that.
    for _ in 0..4 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    app.pcb.zoom = 33.0;
    for tab in [Tab::Settings, Tab::Rules, Tab::Pcb] {
        app.change_tab(tab);
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    assert_eq!(app.pcb.zoom, 33.0, "the board stays as the user left it");
    Ok(())
}

#[test]
fn a_newcomer_reads_settings_with_room_around_the_text() -> Result<(), String> {
    let mut app = fresh();
    let ctx = egui::Context::default();
    for tab in [Tab::Settings, Tab::Rules] {
        app.change_tab(tab);
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        let output = frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        let left = app.page_left;
        let mut tight = Vec::new();
        for shape in &output.shapes {
            if let egui::Shape::Text(text) = &shape.shape
                && text.pos.x > left
                && text.pos.x < left + 16.0
                && text.pos.y > 80.0
            {
                tight.push(text.galley.text().to_owned());
            }
        }
        assert!(tight.is_empty(), "{tab:?} text hugs the edge: {tight:?}");
    }
    Ok(())
}

#[test]
fn a_newcomer_finds_appearance_on_the_settings_page() -> Result<(), String> {
    let mut app = fresh();
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.change_tab(Tab::Settings);
    assert!(
        paints_text(&ctx, &mut app, "APPEARANCE")?,
        "appearance lives on the Settings page"
    );
    app.change_tab(Tab::Pcb);
    assert!(
        !paints_text(&ctx, &mut app, "APPEARANCE")?,
        "the inspector no longer shows appearance"
    );
    Ok(())
}

/// Click the board width box, type `text`, press Enter, and give the frame
/// that follows.
fn type_into_the_width_box(
    ctx: &egui::Context,
    app: &mut App,
    shown: &str,
    text: &str,
) -> Result<(), String> {
    authoring::tests::tap(ctx, app, shown)?;
    let enter = egui::Event::Key {
        key: egui::Key::Enter,
        physical_key: Some(egui::Key::Enter),
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    };
    frame(
        ctx,
        app,
        vec![egui::Event::Text(text.to_string()), enter],
        egui::Modifiers::NONE,
    );
    frame(ctx, app, vec![], egui::Modifiers::NONE);
    Ok(())
}

#[test]
fn a_newcomer_types_a_decimal_comma_in_german() -> Result<(), String> {
    let mut app = fresh();
    let ctx = egui::Context::default();
    app.set_language(&ctx, "de");
    app.change_tab(Tab::Settings);
    app.width = 12.5;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        scrolled_to_at(&ctx, &mut app, "12,5", 700.0)?,
        "a German install shows the board width with a comma"
    );
    type_into_the_width_box(&ctx, &mut app, "12,5", "1,6")?;
    assert_eq!(app.width, 1.6, "typing 1,6 stores 1.6");
    assert!(paints_text(&ctx, &mut app, "1,6")?, "the box shows 1,6");
    type_into_the_width_box(&ctx, &mut app, "1,6", "2.4")?;
    assert_eq!(app.width, 2.4, "typing a point from habit also works");
    assert!(paints_text(&ctx, &mut app, "2,4")?, "the box shows 2,4");
    Ok(())
}

/// Turn the wheel once over the board and report the zoom before and after.
fn zoom_after_one_wheel_frame(reduced: bool) -> (f32, f32) {
    let mut app = fresh();
    app.preferences.reduced_motion = reduced;
    app.tool_call("sample_design", json!({}));
    let ctx = egui::Context::default();
    app.change_tab(Tab::Pcb);
    let at = egui::pos2(700.0, 450.0);
    for _ in 0..4 {
        frame(
            &ctx,
            &mut app,
            vec![egui::Event::PointerMoved(at)],
            egui::Modifiers::NONE,
        );
    }
    let before = app.pcb.zoom;
    frame(
        &ctx,
        &mut app,
        vec![egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, 100.0),
            phase: egui::TouchPhase::Move,
            modifiers: egui::Modifiers::NONE,
        }],
        egui::Modifiers::NONE,
    );
    (before, app.pcb.zoom)
}

#[test]
fn a_newcomer_turns_off_animation_in_settings() -> Result<(), String> {
    let mut app = fresh();
    let ctx = egui::Context::default();
    app.change_tab(Tab::Settings);
    assert!(
        scrolled_to_at(&ctx, &mut app, "Reduce motion", 700.0)?,
        "Settings should offer a Reduce motion choice"
    );
    assert!(!app.preferences.reduced_motion, "off until the person asks");
    authoring::tests::tap(&ctx, &mut app, "Reduce motion")?;
    assert!(
        app.preferences.reduced_motion,
        "ticking the box turns it on"
    );
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert_eq!(
        ctx.global_style().animation_time,
        0.0,
        "fades and collapses finish in the frame they start"
    );
    // A zoom step reaches its final value in one frame when motion is reduced.
    let (before, after) = zoom_after_one_wheel_frame(true);
    let target = (before * (100.0_f32 * 0.003).exp()).clamp(0.02, 100.0);
    assert!(
        (after - target).abs() < target * 1e-3,
        "one frame should reach {target}, got {after} from {before}"
    );
    // With motion on, the same wheel notch has not finished in one frame.
    let (before_on, after_on) = zoom_after_one_wheel_frame(false);
    assert!(
        (after_on - before_on).abs() < (after - before).abs(),
        "gliding zoom should lag the instant one: {before_on} to {after_on}"
    );
    Ok(())
}

fn sample_in_settings() -> Result<(App, egui::Context), String> {
    let mut app = fresh();
    app.tool_call("sample_design", json!({}));
    let ctx = egui::Context::default();
    app.change_tab(Tab::Settings);
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    Ok((app, ctx))
}

#[test]
fn a_newcomer_resizes_the_board_and_cuts_a_round_hole() -> Result<(), String> {
    let (mut app, ctx) = sample_in_settings()?;
    type_into(&ctx, &mut app, "spinbutton:Board width", "80")?;
    type_into(&ctx, &mut app, "spinbutton:Board height", "60")?;
    tap_id(&ctx, &mut app, "button:Apply board dimensions")?;
    assert!(
        (app.project.board.width - 80.0).abs() < 1e-9
            && (app.project.board.height - 60.0).abs() < 1e-9,
        "the typed size becomes the board size: {}",
        app.message
    );
    let holes = app.project.board.cutouts.len();
    type_into(&ctx, &mut app, "spinbutton:Cutout X", "20")?;
    type_into(&ctx, &mut app, "spinbutton:Cutout Y", "25")?;
    type_into(&ctx, &mut app, "spinbutton:Cutout radius", "3")?;
    tap_id(&ctx, &mut app, "button:Add hole")?;
    let hole = app
        .project
        .board
        .cutouts
        .get(holes)
        .ok_or_else(|| format!("one click adds one round hole: {}", app.message))?;
    assert!(
        (hole.center.x - 20.0).abs() < 1e-9
            && (hole.center.y - 25.0).abs() < 1e-9
            && (hole.radius - 3.0).abs() < 1e-9,
        "the hole sits where it was typed: {:?} {} {}",
        hole,
        app.cut_x,
        app.message
    );
    Ok(())
}

fn files_in(folder: &std::path::Path) -> usize {
    std::fs::read_dir(folder).map_or(0, |entries| entries.count())
}

#[test]
fn a_newcomer_pours_copper_and_prints_words_on_both_sides() -> Result<(), String> {
    let (mut app, ctx) = sample_in_settings()?;
    let pours = app.project.board.zones.len();
    type_into(&ctx, &mut app, "spinbutton:Pour X", "3")?;
    type_into(&ctx, &mut app, "spinbutton:Pour Y", "4")?;
    type_into(&ctx, &mut app, "spinbutton:Pour width", "10")?;
    type_into(&ctx, &mut app, "spinbutton:Pour height", "8")?;
    tap_id(&ctx, &mut app, "combobox:Pour connection")?;
    tap_id(&ctx, &mut app, "combobox:Pour connection")?;
    tap_id(&ctx, &mut app, "button:Pour copper")?;
    app.refresh();
    let zone = app
        .project
        .board
        .zones
        .get(pours)
        .ok_or_else(|| format!("one click pours one block of copper: {}", app.message))?;
    assert!(
        zone.outline.iter().all(|p| p.x >= 3.0 - 1e-9
            && p.x <= 13.0 + 1e-9
            && p.y >= 4.0 - 1e-9
            && p.y <= 12.0 + 1e-9)
            && zone.outline.iter().any(|p| (p.x - 13.0).abs() < 1e-9),
        "the copper covers the rectangle that was typed"
    );
    let words = app.project.board.texts.len();
    type_into(&ctx, &mut app, "textinput:what to print", "HELLO")?;
    type_into(&ctx, &mut app, "spinbutton:Text X", "5")?;
    type_into(&ctx, &mut app, "spinbutton:Text Y", "6")?;
    type_into(&ctx, &mut app, "spinbutton:Text size", "2")?;
    tap_id(&ctx, &mut app, "button:Bottom")?;
    tap_id(&ctx, &mut app, "button:Add label")?;
    let text = app
        .project
        .board
        .texts
        .get(words)
        .ok_or_else(|| format!("one click prints the words: {}", app.message))?;
    assert_eq!(text.text, "HELLO", "the words are the ones typed");
    assert!(
        (text.position.x - 5.0).abs() < 1e-9 && (text.size - 2.0).abs() < 1e-9,
        "the words sit where and as big as typed"
    );
    assert_eq!(
        text.layer,
        pcbcraft_doc::Layer::Back,
        "Bottom puts the words on the back"
    );
    tap_id(&ctx, &mut app, "button:Top")?;
    tap_id(&ctx, &mut app, "button:Add label")?;
    assert_eq!(
        app.project.board.texts.last().map(|t| t.layer),
        Some(pcbcraft_doc::Layer::Front),
        "Top puts the next words on the front"
    );
    Ok(())
}

#[test]
fn a_newcomer_makes_every_file_the_factory_asks_for() -> Result<(), String> {
    let (mut app, ctx) = sample_in_settings()?;
    let folder = std::env::temp_dir().join(format!("pcbcraftmake-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    type_into(
        &ctx,
        &mut app,
        "textinput:folder for the files (blank means \"outputs\" beside the desig",
        &folder.to_string_lossy(),
    )?;
    assert_eq!(
        app.make_dir,
        folder.to_string_lossy(),
        "the folder is the one typed"
    );
    let mut seen = 0;
    for button in [
        "button:Copper & drill files",
        "button:Parts list (CSV)",
        "button:Net list",
        "button:Pick and place file",
        "button:Design report",
        "button:Print art, bottom silkscreen",
    ] {
        tap_id(&ctx, &mut app, button)?;
        let now = files_in(&folder);
        assert!(now > seen, "{button} leaves new files: {}", app.message);
        seen = now;
    }
    let _ = std::fs::remove_dir_all(&folder);
    Ok(())
}

#[test]
fn a_newcomer_plans_a_panel_with_their_own_gaps() -> Result<(), String> {
    let (mut app, ctx) = sample_in_settings()?;
    type_into(&ctx, &mut app, "spinbutton:Copies across", "3")?;
    type_into(&ctx, &mut app, "spinbutton:Copies down", "2")?;
    type_into(&ctx, &mut app, "spinbutton:Gap between copies", "2")?;
    type_into(&ctx, &mut app, "spinbutton:Rail width", "5")?;
    tap_id(&ctx, &mut app, "button:Preview panel")?;
    assert_eq!(
        app.panel_plan.get("copies").and_then(Value::as_u64),
        Some(6),
        "three across and two down is six copies: {}",
        app.message
    );
    Ok(())
}

#[test]
fn a_newcomer_brings_in_a_drawing_and_uses_its_edge_as_the_board() -> Result<(), String> {
    let (mut app, ctx) = sample_in_settings()?;
    let file = std::env::temp_dir().join(format!("pcbcraftdraw-{}.svg", std::process::id()));
    std::fs::write(
        &file,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"40\" height=\"30\" viewBox=\"0 0 40 30\"><path d=\"M0 0 L40 0 L40 30 L0 30 Z\" fill=\"none\" stroke=\"black\"/></svg>",
    )
    .map_err(|e| e.to_string())?;
    type_into(
        &ctx,
        &mut app,
        "textinput:drawing file path (.svg)",
        &file.to_string_lossy(),
    )?;
    assert_eq!(
        app.svg_path,
        file.to_string_lossy(),
        "the path is the one typed"
    );
    type_into(&ctx, &mut app, "spinbutton:Cutout tool radius", "2.5")?;
    assert!(
        (app.cutout_radius - 2.5).abs() < 1e-9,
        "the cutout tool keeps the typed radius"
    );
    tap_id(&ctx, &mut app, "button:Import drawing")?;
    let said_import = app.message.clone();
    tap_id(&ctx, &mut app, "button:Import as board edge")?;
    assert!(
        !said_import.is_empty() && !app.message.is_empty(),
        "the app answers both imports in words"
    );
    let _ = std::fs::remove_file(&file);
    Ok(())
}

#[test]
fn a_newcomer_changes_how_the_app_looks() -> Result<(), String> {
    let (mut app, ctx) = sample_in_settings()?;
    tap_id(&ctx, &mut app, "button:Light")?;
    assert!(!app.preferences.dark, "Light switches the app to light");
    let grid = app.preferences.grid;
    tap_id(&ctx, &mut app, "checkbox:Show grid")?;
    assert_ne!(app.preferences.grid, grid, "the grid tick flips");
    for (button, labels) in [
        ("button:Pads: net", PadLabels::Net),
        ("button:Pads: number", PadLabels::Number),
        ("button:Pads: off", PadLabels::Off),
    ] {
        tap_id(&ctx, &mut app, button)?;
        assert_eq!(
            app.preferences.pad_labels, labels,
            "{button} picks what is drawn on each pad"
        );
    }
    tap_id(&ctx, &mut app, "colorwell:Accent color")?;
    assert!(
        paints_text(&ctx, &mut app, "Accent color")?,
        "the accent colour row stays on the page"
    );
    tap_id(&ctx, &mut app, "button:#%")?;
    assert!(
        paints_text(&ctx, &mut app, "%")?,
        "the status bar always shows how far the drawing is zoomed"
    );
    Ok(())
}

#[test]
fn a_newcomer_jumps_from_settings_to_the_list_of_findings() -> Result<(), String> {
    let (mut app, ctx) = sample_in_settings()?;
    tap_id(&ctx, &mut app, "button:# electrical · # board >")?;
    assert!(
        app.region().bottom_open,
        "the findings button opens the bottom sheet on Findings"
    );
    Ok(())
}

#[test]
fn a_newcomer_decides_how_much_the_assistant_may_change() -> Result<(), String> {
    let (mut app, ctx) = sample_in_settings()?;
    app.preferences.assistant_enabled = false;
    app.assistant.settings_open = true;
    tap_id(&ctx, &mut app, "checkbox:Show assistant")?;
    assert!(
        app.preferences.assistant_enabled,
        "ticking the box brings the assistant settings back"
    );
    for (button, mode) in [
        (
            "radiobutton:Auto-allow design edits",
            pcbcraft_assistant::Mode::Edits,
        ),
        (
            "radiobutton:Allow everything",
            pcbcraft_assistant::Mode::All,
        ),
        ("radiobutton:Ask every time", pcbcraft_assistant::Mode::Ask),
    ] {
        tap_id(&ctx, &mut app, button)?;
        assert_eq!(
            app.preferences.assistant_mode, mode,
            "{button} sets the permission level"
        );
    }
    type_into(&ctx, &mut app, "spinbutton:Changes per message", "7")?;
    assert_eq!(
        app.preferences.assistant_turn_limit, 7,
        "the typed limit sticks"
    );
    app.preferences
        .assistant_always_allow
        .insert("local".into(), vec!["board.resize".into()]);
    tap_id(&ctx, &mut app, "button:Forget Always allow")?;
    assert!(
        app.preferences.assistant_always_allow.is_empty(),
        "one click forgets every standing allowance"
    );
    Ok(())
}

#[test]
fn a_newcomer_opens_the_keys_section_to_pick_tool_keys() -> Result<(), String> {
    let mut app = fresh();
    let ctx = egui::Context::default();
    app.change_tab(Tab::Settings);
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        scrolled_to_at(&ctx, &mut app, "Keys", 700.0)?,
        "the settings page has a Keys section"
    );
    tap_id(&ctx, &mut app, "button:Keys")?;
    assert!(
        scrolled_to_at(&ctx, &mut app, "Reset keys", 700.0)?,
        "the section opens onto the tool keys and a way back to the defaults"
    );
    Ok(())
}

/// The fonts the installer ships next to the program; the tests read the same
/// files from the repository that the native loader reads from beside the
/// executable and the browser build fetches over HTTP.
fn font_bytes(file: &str) -> Vec<u8> {
    let path = format!("{}/assets/fonts/{file}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read(path).unwrap_or_default()
}

/// Widths of the glyphs of `words` laid out in the Title role (16 points, SemiBold family), for the characters
/// from the CJK blocks only. An empty box from Plex is far narrower than a full em.
fn boxes_in(ctx: &egui::Context, words: &str) -> Vec<String> {
    let galley = ctx.fonts_mut(|fonts| {
        fonts.layout_no_wrap(
            words.to_string(),
            crate::fonts::Role::Title.font(),
            egui::Color32::WHITE,
        )
    });
    let mut boxes = Vec::new();
    for row in &galley.rows {
        for glyph in &row.glyphs {
            if (glyph.chr as u32) >= 0x2e80 && glyph.advance_width < 13.0 {
                boxes.push(format!(
                    "{} drew only {:.1} points wide",
                    glyph.chr, glyph.advance_width
                ));
            }
        }
    }
    boxes
}

#[test]
fn a_newcomer_picks_japanese_and_chinese_and_sees_no_empty_boxes() -> Result<(), String> {
    let mut app = fresh();
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    for (tag, file, words) in [
        ("ja", "NotoSansJP-Regular.otf", "基板設計配線"),
        ("zh", "NotoSansSC-Regular.otf", "电路板设计布线"),
    ] {
        let bytes = font_bytes(file);
        assert!(!bytes.is_empty(), "{file} must ship in the repository");
        crate::fonts::add_fallback(&ctx, crate::fonts::font_stem(file), bytes);
        app.set_language(&ctx, tag);
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        let boxes = boxes_in(&ctx, words);
        assert!(
            boxes.is_empty(),
            "{tag} text still shows empty boxes: {boxes:?}"
        );
    }
    // Latin text keeps IBM Plex: the first font of the family is unchanged.
    let first = ctx.fonts(|view| {
        view.definitions()
            .families
            .get(&egui::FontFamily::Proportional)
            .and_then(|list| list.first().cloned())
    });
    assert_eq!(first.as_deref(), Some("plex-regular"));
    Ok(())
}

#[test]
fn a_missing_font_file_keeps_the_app_usable_and_says_so() -> Result<(), String> {
    let mut app = fresh();
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    // The test program has no fonts folder beside it, so the file is really missing.
    app.set_language(&ctx, "ja");
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        app.message.contains("font file") && app.message.contains("NotoSansJP-Regular.otf"),
        "the status line says which font file is missing: {}",
        app.message
    );
    app.change_tab(Tab::Settings);
    assert!(
        paints_text(&ctx, &mut app, "APPEARANCE")?,
        "the app stays usable and keeps painting text"
    );
    Ok(())
}
