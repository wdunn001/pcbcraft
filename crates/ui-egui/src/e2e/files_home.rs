//! Stories for the Files workspace: its own left aside, recent projects, and the pages
//! behind Open, Export and Save.
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
fn a_newcomer_opens_files_and_sees_recent_projects() -> Result<(), String> {
    let recents = IsolatedRecents::new()?;
    let (ctx, mut app, _) = files_tab(&recents)?;
    for words in ["Recent projects", "alpha", "beta", "OPEN NOW"] {
        assert!(
            paints_text(&ctx, &mut app, words)?,
            "the page shows {words}"
        );
    }
    for command in ["New project", "Open…", "Save", "Save as…", "Close project"] {
        assert!(
            paints_text(&ctx, &mut app, command)?,
            "the left aside lists {command}"
        );
    }
    assert_eq!(app.files_home.page, files_home::Page::Recent);
    Ok(())
}

#[test]
fn a_newcomer_never_sees_the_library_on_the_files_tab() -> Result<(), String> {
    let recents = IsolatedRecents::new()?;
    let (ctx, mut app, _) = files_tab(&recents)?;
    // The Library is one of the five main tabs, so its name is always on the tab row; what
    // the Files tab must not have is a Library tab (or Layers) in its left aside.
    for words in ["Layers", "Search parts"] {
        assert!(
            !paints_text(&ctx, &mut app, words)?,
            "the Files tab does not show {words}"
        );
    }
    let aside_tabs = |app: &App| -> Vec<&'static str> {
        app.workspace().left_tabs().iter().map(|t| t.id).collect()
    };
    assert_eq!(
        aside_tabs(&app),
        ["files"],
        "the Files left aside lists no Library tab"
    );
    app.change_tab(Tab::Schematic);
    assert!(
        paints_text(&ctx, &mut app, "Library")?,
        "the drawing tabs still have it"
    );
    assert!(
        aside_tabs(&app).contains(&"library"),
        "the drawing tabs keep a Library tab in the left aside"
    );
    Ok(())
}

#[test]
fn a_newcomer_reopens_a_recent_project() -> Result<(), String> {
    let recents = IsolatedRecents::new()?;
    let (ctx, mut app, files) = files_tab(&recents)?;
    authoring::tests::tap(&ctx, &mut app, "alpha")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        paints_text(&ctx, &mut app, "alpha.pcbcraft")?,
        "the right aside shows the full path of the picked project"
    );
    assert_eq!(app.tab, Tab::Files, "one click only picks it");
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
    authoring::tests::tap(&ctx, &mut app, "Save as…")?;
    assert!(app.edit.dialog.open, "Save as opens its dialog");
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
    authoring::tests::tap(&ctx, &mut app, "Open")?;
    assert_eq!(app.path, files[1], "the typed project opens");
    Ok(())
}
