//! Stories for the Files page rebuilt to the approved mockup: the project files tree, the
//! Board shape card, the Shape of each layer row and the Properties aside.
//!
//! Context: `files_home/` and `workspace.rs`. The board is a free-form 2D shape; a rectangle is
//! only a template, and there is no Width or Height control on the Files page. Every story
//! names the control a person sees.
use super::super::*;
use super::support::*;

/// A new project showing the Files tab.
fn blank_files() -> Result<(egui::Context, App), String> {
    let mut app = App::create(
        pcbcraft_mcp::shared(pcbcraft_engine::Session::default()),
        Preferences::default(),
        None,
        None,
    );
    app.refresh();
    app.change_tab(Tab::Files);
    let ctx = egui::Context::default();
    for _ in 0..4 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    Ok((ctx, app))
}

/// The same, with an L shaped outline of six corners.
fn l_shaped_files() -> Result<(egui::Context, App), String> {
    let (ctx, mut app) = blank_files()?;
    app.run(
        "board.set_outline",
        json!({"points": [
            {"x":0.0,"y":0.0},
            {"x":60.0,"y":0.0},
            {"x":60.0,"y":20.0},
            {"x":30.0,"y":20.0},
            {"x":30.0,"y":40.0},
            {"x":0.0,"y":40.0}
        ]}),
    );
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    Ok((ctx, app))
}

/// Every text painted below the tool bar this frame, with its left edge.
fn texts(ctx: &egui::Context, app: &mut App) -> Vec<(String, f32)> {
    let output = frame(ctx, app, vec![], egui::Modifiers::NONE);
    output
        .shapes
        .iter()
        .filter_map(|s| match &s.shape {
            egui::Shape::Text(t) => {
                (t.pos.y > 120.0).then(|| (t.galley.text().to_string(), t.pos.x))
            }
            _ => None,
        })
        .collect()
}

/// Names of the controls on screen, such as `Button Save`.
fn control_names(ctx: &egui::Context, app: &mut App) -> Vec<String> {
    interactive_rects(ctx, app)
        .into_iter()
        .map(|(name, _)| name)
        .collect()
}

#[test]
fn a_newcomer_sees_no_layers_tab_and_no_layer_rows_on_the_files_tab() -> Result<(), String> {
    let (ctx, mut app) = blank_files()?;
    let ids: Vec<&str> = app.workspace().left_tabs().iter().map(|t| t.id).collect();
    assert_eq!(
        ids,
        ["files", "history"],
        "the aside tabs are Files and History"
    );
    let left = left_aside();
    let aside: Vec<String> = texts(&ctx, &mut app)
        .into_iter()
        .filter(|(_, x)| *x < left)
        .map(|(t, _)| t)
        .collect();
    for word in ["Layers", "Library", "Board edge", "Silkscreen", "Copper"] {
        assert!(
            !aside.iter().any(|t| t == word),
            "the left aside has no {word}: {aside:?}"
        );
    }
    // A layout saved before this change pointed the Files aside at Layers.
    let mut saved = std::collections::BTreeMap::new();
    for old in ["layers", "library"] {
        let mut layout = app.region().clone();
        layout.left_tab = old.to_owned();
        saved.insert("files".to_owned(), layout);
        workspace::migrate_layouts(&mut saved);
        assert_eq!(
            saved.get("files").map(|l| l.left_tab.as_str()),
            Some("files"),
            "a saved {old} tab becomes the files tree"
        );
    }
    Ok(())
}

#[test]
fn a_newcomer_reads_the_design_and_the_project_library_in_the_aside_tree() -> Result<(), String> {
    let (ctx, mut app) = blank_files()?;
    let left = left_aside();
    let shown: Vec<(String, f32)> = texts(&ctx, &mut app)
        .into_iter()
        .filter(|(_, x)| *x < left)
        .collect();
    let has = |word: &str| shown.iter().any(|(t, _)| t.eq_ignore_ascii_case(word));
    for word in [
        "Design",
        "design.pcbcraft",
        "Project library",
        "Attached files",
        "Made files",
        "Nothing made yet",
        "History",
    ] {
        assert!(has(word), "the aside tree shows {word}: {shown:?}");
    }
    let controls = control_names(&ctx, &mut app);
    assert!(
        controls.iter().any(|c| c.ends_with("Collapse Design")),
        "the tree can fold Design: {controls:?}"
    );
    // Every count on the tree is a number of something the project holds.
    let symbols = app.project.library.symbols.len();
    assert!(
        shown.iter().any(|(t, _)| *t == symbols.to_string()),
        "the Project library count {symbols} is painted"
    );
    for row in [
        "Nothing made yet",
        "Nothing saved yet",
        "Made files",
        "History",
        "Project library",
        "Attached files",
    ] {
        tap_id(&ctx, &mut app, &format!("button:{row}"))?;
        assert_eq!(app.tab, Tab::Files, "{row} only reads, it goes nowhere");
    }
    tap_id(&ctx, &mut app, "button:Collapse Design")?;
    let in_aside = |ctx: &egui::Context, app: &mut App| {
        texts(ctx, app)
            .into_iter()
            .any(|(t, x)| t == "design.pcbcraft" && x > 40.0 && x < left)
    };
    assert!(
        !in_aside(&ctx, &mut app),
        "folding Design hides the design row"
    );
    tap_id(&ctx, &mut app, "button:Expand Design")?;
    assert!(in_aside(&ctx, &mut app), "opening Design shows it again");
    tap_id(&ctx, &mut app, "tab:History")?;
    assert_eq!(app.region().left_tab, "history", "the History tab opens");
    assert!(
        paints_text(&ctx, &mut app, "Nothing saved yet")?,
        "History says what is recorded"
    );
    tap_id(&ctx, &mut app, "tab:Files")?;
    assert_eq!(app.region().left_tab, "files", "Files brings the tree back");
    tap_id(&ctx, &mut app, "button:design.pcbcraft")?;
    assert_eq!(app.tab, Tab::Schematic, "the design row opens the drawing");
    Ok(())
}

#[test]
fn a_newcomer_filters_the_project_files_tree() -> Result<(), String> {
    let (ctx, mut app) = blank_files()?;
    type_into(&ctx, &mut app, "textinput:Filter project files", "zzz")?;
    assert_eq!(
        app.files_home.filter, "zzz",
        "the filter keeps what was typed"
    );
    let left = left_aside();
    let shown: Vec<String> = texts(&ctx, &mut app)
        .into_iter()
        .filter(|(_, x)| *x < left)
        .map(|(t, _)| t)
        .collect();
    assert!(
        !shown
            .iter()
            .any(|t| t.eq_ignore_ascii_case("Project library")),
        "sections with no match are hidden: {shown:?}"
    );
    Ok(())
}

#[test]
fn a_newcomer_finds_no_width_height_or_library_sources_on_the_files_page() -> Result<(), String> {
    let (ctx, mut app) = l_shaped_files()?;
    let controls = control_names(&ctx, &mut app);
    assert!(!controls.is_empty(), "the page has controls");
    for name in &controls {
        let lower = name.to_lowercase();
        for banned in ["width", "height", "library sources"] {
            assert!(
                !lower.contains(banned),
                "no control named {banned} on the Files page: {name}"
            );
        }
    }
    for word in ["Width", "Height", "Library sources"] {
        assert!(
            !paints_text(&ctx, &mut app, word)?,
            "the Files page does not paint {word}"
        );
    }
    Ok(())
}

#[test]
fn a_newcomer_sees_the_l_shaped_outline_drawn_with_six_corners() -> Result<(), String> {
    let (ctx, mut app) = l_shaped_files()?;
    assert!(
        paints_text(&ctx, &mut app, "Free-form shape")?,
        "an L shaped outline is a free-form shape"
    );
    assert!(
        paints_text(&ctx, &mut app, "6 corners")?,
        "the card counts the real corners, not four"
    );
    assert!(
        !paints_text(&ctx, &mut app, "4 corners")?,
        "an L shape is not a four corner rectangle"
    );
    assert!(
        paints_text(&ctx, &mut app, "Reaches 60 x 40 mm")?,
        "the reach is the box around the shape"
    );
    // The painter drew the cut path: one closed line with seven points per outline pass.
    let output = frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let mut paths: Vec<usize> = Vec::new();
    for clipped in &output.shapes {
        count_paths(&clipped.shape, &mut paths);
    }
    assert!(
        paths.contains(&7),
        "a six corner outline paints a closed path of seven points: {paths:?}"
    );
    Ok(())
}

/// Lengths of every line shape inside `shape`.
fn count_paths(shape: &egui::Shape, out: &mut Vec<usize>) {
    match shape {
        egui::Shape::Path(p) => out.push(p.points.len()),
        egui::Shape::Vec(list) => list.iter().for_each(|s| count_paths(s, out)),
        _ => {}
    }
}

#[test]
fn a_newcomer_with_a_plain_rectangle_reads_rectangle_four_corners() -> Result<(), String> {
    let (ctx, mut app) = blank_files()?;
    assert!(
        paints_text(&ctx, &mut app, "Rectangle")?,
        "the name of the shape"
    );
    assert!(paints_text(&ctx, &mut app, "4 corners")?, "its corners");
    Ok(())
}

#[test]
fn a_newcomer_reads_the_shape_of_each_copper_layer() -> Result<(), String> {
    let (ctx, mut app) = l_shaped_files()?;
    for word in [
        "Shape of each layer",
        "Top copper",
        "Bottom copper",
        "Follows the board",
    ] {
        assert!(
            paints_text_tall(&ctx, &mut app, word)?,
            "the row shows {word}"
        );
    }
    for sub in ["Silkscreen", "Solder mask", "Paste"] {
        assert!(
            !paints_text_tall(&ctx, &mut app, sub)?,
            "{sub} has no shape of its own and is not listed"
        );
    }
    Ok(())
}

#[test]
fn a_newcomer_edits_the_shape_in_layout_from_the_card_and_the_aside() -> Result<(), String> {
    for _ in 0..2 {
        let (ctx, mut app) = l_shaped_files()?;
        tap_id(&ctx, &mut app, "button:Edit shape in Layout")?;
        assert_eq!(
            app.tab,
            Tab::Pcb,
            "Edit shape in Layout opens the Layout tab"
        );
        assert_eq!(
            app.outline_ui.segment,
            Some(0),
            "and selects the board shape: {}",
            app.message
        );
    }
    let (ctx, mut app) = l_shaped_files()?;
    authoring::tests::tap(&ctx, &mut app, "Edit shape in Layout")?;
    assert_eq!(app.tab, Tab::Pcb, "the aside button goes to Layout as well");
    Ok(())
}

#[test]
fn a_newcomer_follows_each_card_link_to_its_tab() -> Result<(), String> {
    for (control, tab) in [
        ("button:Open in Schematic", Tab::Schematic),
        ("button:Open Findings", Tab::Checks),
        ("button:Open Manufacture", Tab::PlotFiles),
        ("button:Stackup and finish", Tab::Files),
        ("button:Export", Tab::Files),
    ] {
        let (ctx, mut app) = l_shaped_files()?;
        tap_id(&ctx, &mut app, control)?;
        match tab {
            Tab::Checks => assert!(
                app.region().bottom_open,
                "{control} opens the Findings sheet"
            ),
            Tab::Files if control == "button:Stackup and finish" => assert!(
                app.project_settings.open,
                "{control} opens Project settings"
            ),
            Tab::Files => assert_eq!(
                app.files_home.page,
                files_home::Page::Export,
                "{control} opens the Export page"
            ),
            other => assert_eq!(app.tab, other, "{control} goes to its tab"),
        }
    }
    Ok(())
}

#[test]
fn a_newcomer_starts_from_a_rectangle_in_one_undo_step() -> Result<(), String> {
    let (ctx, mut app) = l_shaped_files()?;
    assert!(app.project.board.outline.is_some(), "the L outline is set");
    let controls_before = control_names(&ctx, &mut app);
    assert!(
        !controls_before
            .iter()
            .any(|c| c.contains("Rectangle width")),
        "the two numbers are not on the page until the dialog opens"
    );
    tap_id(&ctx, &mut app, "button:Start from a rectangle")?;
    assert!(app.files_home.rect_draft.is_some(), "the dialog opens");
    for word in ["Width", "Height"] {
        assert!(
            paints_text(&ctx, &mut app, word)?,
            "the dialog asks for {word}"
        );
    }
    type_into(&ctx, &mut app, "spinbutton:Rectangle width", "80")?;
    type_into(&ctx, &mut app, "spinbutton:Rectangle height", "50")?;
    tap_id(&ctx, &mut app, "button:Use rectangle")?;
    assert!(app.files_home.rect_draft.is_none(), "the dialog closes");
    assert_eq!(
        app.project.board.outline.as_ref().map(Vec::len),
        Some(4),
        "the free-form outline is replaced by a four corner rectangle: {}",
        app.message
    );
    assert_eq!(
        (app.project.board.width, app.project.board.height),
        (80.0, 50.0),
        "the template has the typed size"
    );
    assert!(
        paints_text(&ctx, &mut app, "Reaches 80 x 50 mm")?,
        "the reach follows"
    );
    app.tool_call("undo", json!({}));
    assert!(
        app.project.board.outline.is_some(),
        "one undo brings the L shape back: {}",
        app.message
    );
    Ok(())
}

#[test]
fn a_newcomer_edits_name_and_units_through_the_same_commands() -> Result<(), String> {
    let (ctx, mut app) = blank_files()?;
    type_into(&ctx, &mut app, "textinput:Project name", "Bench supply")?;
    assert_eq!(app.project.name, "Bench supply", "the name is stored");
    authoring::tests::tap(&ctx, &mut app, "mil")?;
    assert_eq!(app.project.unit, "mil", "the units are stored");
    Ok(())
}
