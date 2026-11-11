//! Stories for what the browser build promises and what it hides (issue #752).
//!
//! A newcomer in the browser is told that edits are kept in this browser. The page storage
//! is a test double here: it records what the app saves, and the web entry point reads the
//! same key back at start. The Manufacture page hides the plot folder box because a
//! browser has no folder to read.
//!
//! # Dependencies
//! - crates/ui-egui/src/browser.rs
//! - crates/ui-egui/src/workspace.rs
//! - apps/pcbcraft-web/src/main.rs
use super::super::*;
use super::support::*;
use eframe::Storage as _;
use std::collections::BTreeMap;

/// Page storage kept in memory, standing in for the browser's local storage.
#[derive(Default)]
struct MemoryStorage(BTreeMap<String, String>);

impl eframe::Storage for MemoryStorage {
    fn get_string(&self, key: &str) -> Option<String> {
        self.0.get(key).cloned()
    }
    fn set_string(&mut self, key: &str, value: String) {
        self.0.insert(key.to_owned(), value);
    }
    fn remove_string(&mut self, key: &str) {
        self.0.remove(key);
    }
    fn flush(&mut self) {}
}

fn app_in(browser: bool) -> App {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.browser = browser;
    app.refresh();
    app
}

#[test]
fn a_newcomer_in_the_browser_is_told_the_truth_about_where_edits_go() -> Result<(), String> {
    let said = browser::start_message();
    assert!(
        said.contains("edits are kept in this browser until the storage is cleared"),
        "the opening line says where edits stay: {said}"
    );
    let mut app = app_in(true);
    app.tool_call(
        "component.add",
        json!({"kind":"resistor","position":{"x":99,"y":88}}),
    );
    assert!(
        app.project
            .components
            .iter()
            .any(|c| (c.position.x - 99.0).abs() < 0.01),
        "the edit is made: {}",
        app.message
    );
    let mut storage = MemoryStorage::default();
    eframe::App::save(&mut app, &mut storage);
    let saved = storage
        .get_string("recovery")
        .ok_or("the browser build saves a copy of the project in page storage")?;
    let project: pcbcraft_doc::Project =
        serde_json::from_str(&saved).map_err(|e| format!("the saved copy reads back: {e}"))?;
    assert!(
        project
            .components
            .iter()
            .any(|c| (c.position.x - 99.0).abs() < 0.01),
        "the saved copy holds the edit"
    );
    Ok(())
}

#[test]
fn the_desktop_build_does_not_write_a_project_copy_into_page_storage() -> Result<(), String> {
    let mut app = app_in(false);
    app.tool_call(
        "component.add",
        json!({"kind":"resistor","position":{"x":99,"y":88}}),
    );
    let mut storage = MemoryStorage::default();
    eframe::App::save(&mut app, &mut storage);
    assert!(
        storage.get_string("recovery").is_none(),
        "the desktop build keeps its copy in the recovery folder"
    );
    Ok(())
}

#[test]
fn a_newcomer_in_the_browser_is_not_offered_a_plot_folder_that_cannot_open() -> Result<(), String> {
    for (browser, shown) in [(false, true), (true, false)] {
        let mut app = app_in(browser);
        app.tool_call("sample_design", json!({}));
        app.change_tab(Tab::PlotFiles);
        let ctx = egui::Context::default();
        for _ in 0..3 {
            frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        }
        assert_eq!(
            paints_text(&ctx, &mut app, "Plot folder")?,
            shown,
            "the plot folder box on the Manufacture page, browser build {browser}"
        );
    }
    Ok(())
}

/// A newcomer edits in the browser, the page saves to its storage, and after a reload the
/// edit is back. The reload is a new app whose session is filled from the stored copy, the
/// same read the web entry point makes at start (`opening_session` in `apps/pcbcraft-web`).
#[test]
fn a_newcomer_reloads_the_browser_page_and_finds_the_edit() -> Result<(), String> {
    let mut before = app_in(true);
    before.tool_call("sample_design", json!({}));
    before.tool_call(
        "component.add",
        json!({"kind":"resistor","position":{"x":99,"y":88}}),
    );
    let mut storage = MemoryStorage::default();
    eframe::App::save(&mut before, &mut storage);
    drop(before);

    let text = storage
        .get_string("recovery")
        .ok_or("the first visit saved a copy of the project")?;
    let project =
        pcbcraft_io::read_project(&text).map_err(|e| format!("the saved copy reads back: {e}"))?;
    let mut session = pcbcraft_engine::Session::default();
    session
        .replace(project)
        .map_err(|e| format!("the saved copy opens: {e}"))?;
    let mut after = App::create(
        pcbcraft_mcp::shared(session),
        Preferences::default(),
        None,
        None,
    );
    after.browser = true;
    after.refresh();
    assert!(
        after
            .project
            .components
            .iter()
            .any(|c| (c.position.x - 99.0).abs() < 0.01 && (c.position.y - 88.0).abs() < 0.01),
        "the part added before the reload is where it was left"
    );
    Ok(())
}
