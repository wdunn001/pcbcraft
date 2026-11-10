//! Stories for the Files workspace: the tool bar, the project summary, the Project files
//! table, Recent projects, the Properties aside and the pages behind Open, Import and Export.
//!
//! Context: the Workspace contract in `workspace.rs` and `files_home.rs`. Every story
//! points the recents file at a folder of its own, so none touches the real list.
use super::super::*;
use super::support::*;

/// Recents kept in a settings folder of their own for the length of a story.
struct IsolatedRecents {
    config: pcbcraft_engine::user_config::UserConfig,
    dir: tempfile::TempDir,
}

impl IsolatedRecents {
    fn new() -> Result<Self, String> {
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let config = pcbcraft_engine::user_config::UserConfig::in_folder(dir.path().join("config"));
        pcbcraft_engine::recents::clear_recent_paths(&config);
        Ok(Self { config, dir })
    }

    fn file(&self, name: &str) -> String {
        self.dir.path().join(name).to_string_lossy().into_owned()
    }
}

/// The sample board saved as two projects, then a new app showing the Files tab.
fn files_tab(recents: &IsolatedRecents) -> Result<(egui::Context, App, [String; 2]), String> {
    let shared = session_with(&recents.config);
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    let files = [
        recents.file("alpha.pcbcraft"),
        recents.file("beta.pcbcraft"),
    ];
    for file in &files {
        app.path = file.clone();
        if !app.tool_call("save", json!({"path": file})) {
            return Err(format!("could not save {file}: {}", app.message));
        }
    }
    app.change_tab(Tab::Files);
    let ctx = egui::Context::default();
    for _ in 0..100 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        if app.files_home.settled(&app.files_listed()) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    Ok((ctx, app, files))
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

#[test]
fn a_newcomer_opens_files_and_sees_the_project_summary() -> Result<(), String> {
    let recents = IsolatedRecents::new()?;
    let (ctx, mut app, files) = files_tab(&recents)?;
    for words in [
        "PROJECT",
        "PARTS",
        "NETS",
        "BOARD SHAPE",
        "CHECKS",
        "Shape of each layer",
        "Next",
        "Recent projects",
        "alpha",
        "beta",
        "beta.pcbcraft",
        "DESIGN",
    ] {
        assert!(
            paints_text_tall(&ctx, &mut app, words)?,
            "the Files page shows {words}"
        );
    }
    assert_eq!(app.files_home.page, files_home::Page::Home);
    assert_eq!(app.path, files[1], "the last saved project is the open one");
    Ok(())
}

#[test]
fn a_newcomer_finds_the_tool_bar_and_the_project_files_on_the_files_tab() -> Result<(), String> {
    let recents = IsolatedRecents::new()?;
    let (ctx, mut app, _) = files_tab(&recents)?;
    for command in [
        "New",
        "Open",
        "Save",
        "Import",
        "Export",
        "Open sample project",
    ] {
        assert!(
            paints_text(&ctx, &mut app, command)?,
            "the tool bar has {command}"
        );
    }
    let aside_tabs = |app: &App| -> Vec<&'static str> {
        app.workspace().left_tabs().iter().map(|t| t.id).collect()
    };
    assert_eq!(
        aside_tabs(&app),
        ["files", "history"],
        "the Files left aside has Files and History, as the mockup draws"
    );
    assert_eq!(
        app.region().left_tab,
        "files",
        "the project files tree is the tab showing"
    );
    assert!(
        paints_text(&ctx, &mut app, "Properties")?,
        "the right aside is Properties"
    );
    Ok(())
}

#[test]
fn a_newcomer_reads_the_project_files_tree_and_opens_the_design() -> Result<(), String> {
    let recents = IsolatedRecents::new()?;
    let (ctx, mut app, _) = files_tab(&recents)?;
    for words in [
        "DESIGN",
        "PROJECT LIBRARY",
        "ATTACHED FILES",
        "MADE FILES",
        "Nothing made yet",
    ] {
        assert!(
            paints_text(&ctx, &mut app, &words.to_lowercase())?
                || paints_text(&ctx, &mut app, words)?
                || paints_text(&ctx, &mut app, &capitalised(words))?,
            "the files tree shows {words}"
        );
    }
    tap_id(&ctx, &mut app, "button:beta.pcbcraft")?;
    assert_eq!(
        app.tab,
        Tab::Schematic,
        "clicking the design file opens the drawing"
    );
    Ok(())
}

/// "PROJECT LIBRARY" as "Project library".
fn capitalised(words: &str) -> String {
    let lower = words.to_lowercase();
    let mut chars = lower.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().collect::<String>() + chars.as_str())
        .unwrap_or_default()
}

#[test]
fn a_newcomer_reopens_a_recent_project() -> Result<(), String> {
    let recents = IsolatedRecents::new()?;
    let (ctx, mut app, files) = files_tab(&recents)?;
    authoring::tests::tap(&ctx, &mut app, "alpha")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert_eq!(app.tab, Tab::Files, "one click only picks it");
    assert_eq!(app.files_home.selected(), Some(files[0].as_str()));
    frame(
        &ctx,
        &mut app,
        vec![key(egui::Key::Enter)],
        egui::Modifiers::NONE,
    );
    assert_eq!(app.path, files[0], "Enter opens the picked project");
    assert_eq!(app.tab, Tab::Schematic, "and takes you to the drawing");
    Ok(())
}

#[test]
fn a_newcomer_moves_through_recent_projects_with_the_arrow_keys() -> Result<(), String> {
    let recents = IsolatedRecents::new()?;
    let (ctx, mut app, files) = files_tab(&recents)?;
    let first = app
        .files_listed()
        .first()
        .cloned()
        .ok_or("no recent project")?;
    for expected in [first.clone(), files[0].clone()] {
        frame(
            &ctx,
            &mut app,
            vec![key(egui::Key::ArrowDown)],
            egui::Modifiers::NONE,
        );
        assert_eq!(app.files_home.selected(), Some(expected.as_str()));
    }
    Ok(())
}

#[test]
fn a_newcomer_saves_and_exports_from_the_files_tab() -> Result<(), String> {
    let recents = IsolatedRecents::new()?;
    let (ctx, mut app, files) = files_tab(&recents)?;
    let out = tempfile::tempdir().map_err(|e| e.to_string())?;
    app.make_dir = out.path().to_string_lossy().into_owned();
    app.path = files[1].clone();
    authoring::tests::tap(&ctx, &mut app, "Save")?;
    assert!(
        std::path::Path::new(&files[1]).exists(),
        "Save writes the project: {}",
        app.message
    );
    authoring::tests::tap(&ctx, &mut app, "Export")?;
    assert_eq!(app.files_home.page, files_home::Page::Export);
    for words in ["COMMON", "Bill of materials"] {
        assert!(paints_text(&ctx, &mut app, words)?, "Export lists {words}");
    }
    authoring::tests::tap(&ctx, &mut app, "Bill of materials")?;
    assert!(
        app.edit.dialog.open,
        "a format opens the same Export dialog"
    );
    authoring::tests::tap(&ctx, &mut app, "Export")?;
    assert!(
        out.path().join("bom.csv").exists(),
        "the dialog writes the bill: {}",
        app.message
    );
    Ok(())
}

#[test]
fn a_newcomer_opens_a_project_from_the_open_page() -> Result<(), String> {
    let recents = IsolatedRecents::new()?;
    let (ctx, mut app, files) = files_tab(&recents)?;
    app.change_tab(Tab::Pcb);
    frame(
        &ctx,
        &mut app,
        vec![egui::Event::Key {
            key: egui::Key::O,
            physical_key: Some(egui::Key::O),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::COMMAND,
        }],
        egui::Modifiers::COMMAND,
    );
    assert_eq!(app.tab, Tab::Files, "Ctrl+O works from any tab");
    assert_eq!(app.files_home.page, files_home::Page::Open);
    app.files_home.set_typed_for_test(&files[1]);
    authoring::tests::tap(&ctx, &mut app, "Open file")?;
    assert_eq!(app.path, files[1], "the typed project opens");
    Ok(())
}

#[test]
fn a_newcomer_walks_every_button_of_the_files_page() -> Result<(), String> {
    let recents = IsolatedRecents::new()?;
    let (ctx, mut app, _) = files_tab(&recents)?;
    for (control, page, in_tool_bar) in [
        ("button:Import", files_home::Page::Import, true),
        ("button:Open", files_home::Page::Open, true),
        (
            "button:Replace from SVG or DXF",
            files_home::Page::Import,
            false,
        ),
        ("button:Export", files_home::Page::Export, true),
    ] {
        if in_tool_bar {
            tap_strip(&ctx, &mut app, control)?;
        } else {
            tap_id(&ctx, &mut app, control)?;
        }
        assert_eq!(app.files_home.page, page, "{control} opens its page");
        tap_id(&ctx, &mut app, "button:Back to project")?;
        assert_eq!(
            app.files_home.page,
            files_home::Page::Home,
            "Back returns to the project"
        );
    }
    tap_id(&ctx, &mut app, "button:Open Manufacture")?;
    assert_eq!(app.tab, Tab::PlotFiles, "Open Manufacture opens that tab");
    app.change_tab(Tab::Files);
    tap_strip(&ctx, &mut app, "button:Open sample project")?;
    assert!(
        !app.project.components.is_empty(),
        "the sample project is loaded: {}",
        app.message
    );
    tap_strip(&ctx, &mut app, "button:New")?;
    assert!(
        app.project.components.is_empty(),
        "New leaves a blank project: {}",
        app.message
    );
    assert_eq!(app.tab, Tab::Schematic, "and takes you to the drawing");
    Ok(())
}

#[test]
fn a_newcomer_closes_a_project_from_the_file_menu() -> Result<(), String> {
    let recents = IsolatedRecents::new()?;
    let (ctx, mut app, _) = files_tab(&recents)?;
    authoring::tests::tap(&ctx, &mut app, "File")?;
    authoring::tests::tap(&ctx, &mut app, "Close project")?;
    assert!(
        app.project.components.is_empty(),
        "a saved project closes at once: {}",
        app.message
    );
    Ok(())
}

#[test]
fn a_newcomer_edits_the_project_properties_on_the_files_tab() -> Result<(), String> {
    let recents = IsolatedRecents::new()?;
    let (ctx, mut app, _) = files_tab(&recents)?;
    type_into(&ctx, &mut app, "textinput:Project name", "Bench supply")?;
    assert_eq!(
        app.project.name, "Bench supply",
        "the name is stored: {}",
        app.message
    );
    type_into(&ctx, &mut app, "textinput:Project author", "Sam")?;
    assert_eq!(
        app.project.meta.get("author").map(String::as_str),
        Some("Sam"),
        "the author is stored: {}",
        app.message
    );
    type_into(&ctx, &mut app, "textinput:Project notes", "Rev B")?;
    assert_eq!(
        app.project.meta.get("notes").map(String::as_str),
        Some("Rev B"),
        "the notes are stored: {}",
        app.message
    );
    authoring::tests::tap(&ctx, &mut app, "mil")?;
    assert_eq!(
        app.project.unit, "mil",
        "the units are stored: {}",
        app.message
    );
    for section in ["button:General", "button:Shape", "button:Layers"] {
        tap_id(&ctx, &mut app, section)?;
        tap_id(&ctx, &mut app, section)?;
    }
    assert!(
        paints_text(&ctx, &mut app, "Copper layers")?,
        "a section folds and opens again: {}",
        app.message
    );
    Ok(())
}

#[test]
fn a_newcomer_opens_the_design_file_of_a_project_that_has_no_file_yet() -> Result<(), String> {
    let recents = IsolatedRecents::new()?;
    let mut app = App::create(
        session_with(&recents.config),
        Preferences::default(),
        None,
        None,
    );
    app.refresh();
    app.tool_call("sample_design", json!({}));
    app.change_tab(Tab::Files);
    let ctx = egui::Context::default();
    for _ in 0..4 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    for words in ["design.pcbcraft", "Unsaved changes"] {
        assert!(
            paints_text(&ctx, &mut app, words)?,
            "the tree shows {words}"
        );
    }
    tap_id(&ctx, &mut app, "button:design.pcbcraft")?;
    assert_eq!(
        app.tab,
        Tab::Schematic,
        "the design file opens the drawing: {}",
        app.message
    );
    Ok(())
}

#[test]
fn a_newcomer_opens_project_settings_from_the_files_page_and_visits_every_tab() -> Result<(), String>
{
    let recents = IsolatedRecents::new()?;
    let (ctx, mut app, _) = files_tab(&recents)?;
    tap_id(&ctx, &mut app, "button:Project settings")?;
    assert!(
        app.project_settings.open,
        "the button opens the dialog: {}",
        app.message
    );
    for (tab, name) in [
        ("tab:Design rules", "Fabrication preset"),
        ("tab:Net classes", "No net classes yet"),
        ("tab:Electrical rules", "Pin connections"),
        ("tab:Machines", "Machines in this project"),
        ("tab:Title block", "Printed on every sheet"),
        ("tab:Builds", "New build"),
        ("tab:Board", "Board shape"),
    ] {
        tap_id(&ctx, &mut app, tab)?;
        assert!(
            paints_text(&ctx, &mut app, name)?,
            "{tab} shows {name}: {}",
            app.message
        );
    }
    tap_id(&ctx, &mut app, "tab:Design rules")?;
    type_into(&ctx, &mut app, "spinbutton:Clearance", "0.45")?;
    assert_eq!(
        app.project.board.clearance, 0.45,
        "an edit in the dialog is stored: {}",
        app.message
    );
    authoring::tests::tap(&ctx, &mut app, "Save")?;
    assert!(!app.project_settings.open, "Save closes the dialog");
    assert_eq!(
        app.project.board.clearance, 0.45,
        "the edit stays after Save"
    );
    Ok(())
}

#[test]
fn a_newcomer_with_no_recent_projects_is_offered_to_open_one() -> Result<(), String> {
    let recents = IsolatedRecents::new()?;
    let mut app = App::create(
        session_with(&recents.config),
        Preferences::default(),
        None,
        None,
    );
    app.refresh();
    app.change_tab(Tab::Files);
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    tap_id(&ctx, &mut app, "button:Open a project")?;
    assert_eq!(
        app.files_home.page,
        files_home::Page::Open,
        "the empty list sends the person to the folder browser"
    );
    Ok(())
}

/// The sorted names inside the settings folder the environment points at,
/// or `None` when the machine has none. Read only: the story compares the
/// listing before and after its work to prove the suite wrote nothing there.
fn real_settings_names() -> Result<Option<Vec<String>>, String> {
    let folder = match pcbcraft_engine::user_config::UserConfig::from_environment().folder() {
        Some(folder) => folder.to_path_buf(),
        None => return Ok(None),
    };
    let entries = match std::fs::read_dir(&folder) {
        Ok(entries) => entries,
        Err(_) => return Ok(None),
    };
    let mut names: Vec<String> = Vec::new();
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        names.push(entry.file_name().to_string_lossy().into_owned());
    }
    names.sort();
    Ok(Some(names))
}

/// A newcomer saves two designs through the Files workspace. Everything the
/// app remembers about them lands in the settings folder that session was
/// given, and the real settings folder on the machine is left exactly as it
/// was, so a full test run can never disturb a person's own settings.
#[test]
fn a_newcomer_saves_designs_and_the_real_settings_folder_is_untouched() -> Result<(), String> {
    // Read only, to compare afterwards; the entry point rule for
    // from_environment keeps this the one sanctioned way to name the folder.
    let before = real_settings_names()?;
    let recents = IsolatedRecents::new()?;
    let (_ctx, app, files) = files_tab(&recents)?;
    assert!(
        app.recent_designs.iter().any(|seen| seen == &files[1]),
        "the design just saved is remembered in the session's own folder"
    );
    assert!(
        recents
            .config
            .file("recent-designs.json")
            .map(|path| path.starts_with(recents.dir.path()))
            .unwrap_or(false),
        "the remembered list lives under the session's own folder"
    );
    assert_eq!(
        real_settings_names()?,
        before,
        "the real settings folder gained nothing while the suite ran"
    );
    Ok(())
}
