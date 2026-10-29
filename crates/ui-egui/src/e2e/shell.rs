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
    assert!(
        paints_text(&ctx, &mut app, "Tool: Pick")?,
        "the status strip should say what a click does: {}",
        app.message
    );
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
    let mind_label = format!("{mind} to mind");
    assert!(
        paints_text(&ctx, &mut app, &mind_label)?,
        "the status strip should count the warnings ({mind_label}): {}",
        app.message
    );
    authoring::tests::tap(&ctx, &mut app, &mind_label)?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert_eq!(
        app.tab,
        Tab::Checks,
        "tapping the warning count should open the checks list: {}",
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
    authoring::tests::tap(&ctx, &mut app, "Assembly mark")?;
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
    authoring::tests::tap(&ctx, &mut app, "Layers")?;
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
    authoring::tests::tap(&ctx, &mut app, "\u{d7}")?;
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
        "the right dock leads with the layers tree: {}",
        app.message
    );
    assert!(
        paints_text(&ctx, &mut app, "Properties")?,
        "object details sit in the lower right tab: {}",
        app.message
    );
    authoring::tests::tap(&ctx, &mut app, "Files")?;
    assert!(
        paints_text(&ctx, &mut app, "Project files")?,
        "build outputs moved under Files: {}",
        app.message
    );
    authoring::tests::tap(&ctx, &mut app, "Layout")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        paints_text(&ctx, &mut app, "Mounting hole")?,
        "board tools sit in the strip under the tabs: {}",
        app.message
    );
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
    assert_eq!(app.preferences.language, "fr", "the choice is remembered");
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
fn a_newcomer_picks_the_move_tool_from_the_strip_icons() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Layout")?;
    authoring::tests::tap_icon(&ctx, &mut app, "tool-move")?;
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
    for word in [
        "Select", "Move", "Route", "Line", "Box", "Circle", "Outline", "Text",
    ] {
        assert!(paints_text(&ctx, &mut app, word)?, "the strip says {word}");
    }
    for cryptic in ["Sel", "Mv", "Rt", "Ln", "Bx", "Cr", "Pl", "Tx"] {
        assert!(text_row(&ctx, &mut app, cryptic).is_none(), "no {cryptic}");
    }
    app.closed_tabs.push(Tab::Rules);
    assert!(
        text_row(&ctx, &mut app, "Rules").is_none(),
        "a closed view leaves the row"
    );
    // Other panels carry a "+" too; the one in the tab row is the highest.
    let output = frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let plus = output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.text() == "+" => {
                Some(text.pos + text.galley.size() / 2.0)
            }
            _ => None,
        })
        .min_by(|a, b| a.y.total_cmp(&b.y))
        .ok_or("the tab row has a + menu")?;
    for pressed in [true, false] {
        frame(
            &ctx,
            &mut app,
            vec![
                egui::Event::PointerMoved(plus),
                egui::Event::PointerButton {
                    pos: plus,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            egui::Modifiers::NONE,
        );
    }
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Rules")?;
    assert!(matches!(app.tab, Tab::Rules), "the + menu reopens it");
    authoring::tests::tap(&ctx, &mut app, "Files")?;
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
    authoring::tests::tap_icon(&ctx, &mut app, "tool-line")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        matches!(app.tool, Tool::DrawLayer) && app.art_shape == DrawShape::Line,
        "tapping Line in the strip arms the line tool"
    );
    assert!(
        scrolled_to(&ctx, &mut app, "DRAW ON LAYER")?,
        "the layer picker appears in Properties: {}",
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
    let seen = tab_to(&ctx, &mut app, "Settings")?;
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

#[test]
fn every_interactive_accesskit_node_has_a_name_on_each_tab() -> Result<(), String> {
    let (ctx, mut app) = practice_app();
    let mut gaps = Vec::new();
    for tab in [
        Tab::Files,
        Tab::Schematic,
        Tab::Pcb,
        Tab::Board3d,
        Tab::Rules,
        Tab::Checks,
        Tab::Settings,
    ] {
        app.change_tab(tab);
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        let out = frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        let nodes = a11y::audit::interactive_nodes(&out);
        assert!(
            !nodes.is_empty(),
            "{tab:?} exposes no controls to a screen reader"
        );
        let blank = a11y::audit::unnamed(&out);
        if !blank.is_empty() {
            gaps.push(format!("{tab:?}: {blank:?}"));
        }
    }
    assert!(gaps.is_empty(), "unnamed controls: {gaps:?}");
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
