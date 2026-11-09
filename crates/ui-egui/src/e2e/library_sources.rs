//! Stories for the Sources page of the Library tab (issue #1799).
//!
//! Context: library sources are listed, switched, added and removed in Library > Sources. The
//! old Libraries page and the Libraries tab of Preferences are gone. A switch writes the same
//! saved names the part list reads, so a source that is off hides its parts from the list.
use super::super::*;
use super::support::*;
use crate::library_workspace::Page;

/// The Library tab on a private settings folder, open on the Sources page and settled.
fn sources_open(
    name: &str,
) -> Result<
    (
        String,
        pcbcraft_engine::user_config::UserConfig,
        App,
        egui::Context,
    ),
    String,
> {
    let (config, folder) = private_config(name)?;
    let shared = session_with(&config);
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.region().left_tab = "library".to_owned();
    app.change_tab(Tab::Library);
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    tap_id(&ctx, &mut app, "tab:Sources")?;
    let ready = waits_until(&ctx, &mut app, |app| {
        app.library.sources.is_some() && app.library.pending.is_none()
    });
    if !ready {
        return Err("the folder list never arrived".into());
    }
    Ok((folder.to_string_lossy().into_owned(), config, app, ctx))
}

/// A folder of library files that ships with the tests.
fn shelf() -> Result<String, String> {
    std::fs::canonicalize(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../library/tests/fixtures"),
    )
    .map(|p| p.to_string_lossy().into_owned())
    .map_err(|e| format!("fixture shelf missing: {e}"))
}

/// Add the fixture folder through the Add folder box and button.
fn add_shelf(ctx: &egui::Context, app: &mut App, shelf: &str) -> Result<(), String> {
    type_into(
        ctx,
        app,
        "textinput:Folder to add as a library source",
        shelf,
    )?;
    let listed = waits_until(ctx, app, |app| {
        app.library_source_rows()
            .iter()
            .any(|r| r.folder.as_deref().is_some_and(|f| f.starts_with(shelf)))
    });
    if listed {
        Ok(())
    } else {
        Err(format!(
            "the folder never joined the table: {}",
            app.library.error
        ))
    }
}

#[test]
fn a_newcomer_finds_the_sources_page_in_the_library_tab() -> Result<(), String> {
    let (folder, _config, mut app, ctx) = sources_open("page")?;
    assert_eq!(
        app.tab,
        Tab::Library,
        "Sources is a page of the Library tab"
    );
    assert_eq!(
        app.libws.page,
        Page::Sources,
        "the Sources tab shows the page"
    );
    for words in [
        "Library sources",
        "This project",
        "Built in parts",
        "Installed libraries",
        "Open library",
        "Add a source",
        "A folder",
    ] {
        assert!(
            paints_text_tall(&ctx, &mut app, words)?,
            "{words} is on the page"
        );
    }
    assert!(
        paints_text_tall(&ctx, &mut app, "0 symbols")?,
        "This project lists its symbol count"
    );
    tap_id(&ctx, &mut app, "button:Update all")?;
    tap_id(&ctx, &mut app, "button:Add source")?;
    for name in ["Built in parts", "Installed libraries", "This project"] {
        tap_id(&ctx, &mut app, &format!("button:{name}"))?;
    }
    tap_id(&ctx, &mut app, "button:Open library")?;
    assert_eq!(
        app.libws.sources_selected.as_deref(),
        Some("online"),
        "picking a row shows it in Properties"
    );
    tap_id(&ctx, &mut app, "tab:Collections")?;
    assert_eq!(
        app.libws.page,
        Page::Collections,
        "Collections is the third page"
    );
    tap_id(&ctx, &mut app, "tab:Sources")?;
    assert_eq!(app.libws.page, Page::Sources, "Sources comes back");
    let _ = std::fs::remove_dir_all(folder);
    Ok(())
}

#[test]
fn a_newcomer_switches_a_source_off_and_its_parts_leave_the_list() -> Result<(), String> {
    let (folder, _config, mut app, ctx) = sources_open("switch")?;
    let mut sym = crate::authoring::new_symbol();
    sym.name = "Zebra part".into();
    app.tool_call("library.symbol.upsert", json!({"definition": sym}));
    app.refresh();
    app.part_search = "zebra".into();
    app.preferences.left_tab = 1;
    assert!(
        paints_text_tall(&ctx, &mut app, "Zebra part")?,
        "the project part is listed while This project is on"
    );
    tap_id(&ctx, &mut app, "checkbox:This project on")?;
    assert!(app.dock.hide_project, "the list stops reading the project");
    assert!(
        app.preferences
            .library_sources_off
            .iter()
            .any(|n| n == "project"),
        "the switch is saved"
    );
    assert!(
        !paints_text_tall(&ctx, &mut app, "Zebra part")?,
        "a search no longer finds the part"
    );
    let text = serde_json::to_string(&app.preferences).map_err(|e| e.to_string())?;
    let stored: Preferences = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let again = App::create(shared, stored, None, None);
    assert!(again.dock.hide_project, "off stays off after a restart");
    tap_id(&ctx, &mut app, "checkbox:This project on")?;
    assert!(!app.dock.hide_project, "switching it on again");
    assert!(
        paints_text_tall(&ctx, &mut app, "Zebra part")?,
        "the part is listed again"
    );
    for key in ["Built in parts", "Installed libraries", "Open library"] {
        tap_id(&ctx, &mut app, &format!("checkbox:{key} on"))?;
    }
    assert!(app.dock.hide_built_in && app.dock.hide_installed && app.dock.hide_online);
    tap_id(&ctx, &mut app, "checkbox:Fetch #D models")?;
    assert!(
        app.preferences.fetch_open_library_3d,
        "Fetch 3D models is saved"
    );
    let _ = std::fs::remove_dir_all(folder);
    Ok(())
}

#[test]
fn a_newcomer_adds_a_folder_sees_its_counts_switches_it_and_removes_it() -> Result<(), String> {
    let (folder, config, mut app, ctx) = sources_open("folders")?;
    let shelf = shelf()?;
    add_shelf(&ctx, &mut app, &shelf)?;
    let row = app
        .library_source_rows()
        .into_iter()
        .find(|r| r.folder.as_deref().is_some_and(|f| f.starts_with(&shelf)))
        .ok_or("the folder is not a row")?;
    assert!(
        row.symbols + row.footprints > 0,
        "the row counts the libraries it found"
    );
    assert!(
        paints_text_tall(&ctx, &mut app, "symbol and")?,
        "the counts are painted on the row"
    );
    let path = row.folder.clone().unwrap_or_default();
    assert!(!app.library_folder_off(&path), "a new folder is on");
    tap_id(&ctx, &mut app, &format!("checkbox:{} on", row.name))?;
    assert!(
        app.library_folder_off(&format!("{path}/anything.kicad_sym")),
        "switching the folder off hides the parts inside it"
    );
    assert!(
        !app.library_folder_off("/elsewhere/part.kicad_sym"),
        "other folders stay on"
    );
    assert!(
        app.preferences
            .library_sources_off
            .iter()
            .any(|n| n.starts_with("folder:")),
        "the folder switch is saved with the others"
    );
    tap_id(&ctx, &mut app, &format!("checkbox:{} on", row.name))?;
    assert!(
        !app.library_folder_off(&path),
        "switching it on shows it again"
    );
    tap_id(&ctx, &mut app, "button:Update")?;
    let again = waits_until(&ctx, &mut app, |app| {
        app.library.sources.is_some() && app.library.pending.is_none()
    });
    assert!(again, "Update reads the folders again");
    tap_id(&ctx, &mut app, "button:Remove")?;
    let gone = waits_until(&ctx, &mut app, |app| {
        app.library_source_rows().iter().all(|r| r.folder.is_none())
    });
    assert!(gone, "Remove takes the folder out of the table");
    let saved = std::fs::read_to_string(config.file("library-sources.json")?)
        .map_err(|_| "config vanished")?;
    assert!(!saved.contains(&shelf), "the config forgot the folder");
    let _ = std::fs::remove_dir_all(folder);
    Ok(())
}

#[test]
fn a_newcomer_opens_library_sources_from_the_view_menu() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    crate::authoring::tests::open_page(&ctx, &mut app, "Library sources")?;
    assert!(
        app.tab == Tab::Library && app.libws.page == Page::Sources,
        "View > Library sources opens Library > Sources"
    );
    Ok(())
}

#[test]
fn a_newcomer_finds_no_libraries_page_and_no_library_sources_in_settings() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    let ctx = egui::Context::default();
    for tab in [
        Tab::Schematic,
        Tab::Pcb,
        Tab::Library,
        Tab::Settings,
        Tab::PlotFiles,
    ] {
        app.change_tab(tab);
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        let ids: Vec<String> = super::audit::controls(&ctx, &app)
            .into_iter()
            .filter_map(|c| c.id)
            .collect();
        assert!(
            !ids.iter().any(|i| i == "tab:Libraries"),
            "{tab:?} has no Libraries tab"
        );
        if matches!(tab, Tab::Settings | Tab::Pcb) {
            assert!(
                !ids.iter()
                    .any(|i| i == "button:Library sources" || i == "tab:Library sources"),
                "{tab:?} has no Library sources control"
            );
        }
    }
    app.open_preferences(crate::preferences::TAB_APPEARANCE);
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let ids: Vec<String> = super::audit::controls(&ctx, &app)
        .into_iter()
        .filter_map(|c| c.id)
        .collect();
    assert!(
        ids.iter().any(|i| i == "tab:Keys"),
        "the Preferences dialog is open"
    );
    assert!(
        !ids.iter().any(|i| i == "tab:Libraries"),
        "Preferences has no Libraries tab"
    );
    Ok(())
}

#[test]
fn a_newcomer_with_an_old_saved_preference_lands_on_the_library_tab() -> Result<(), String> {
    let mut value = serde_json::to_value(Preferences::default()).map_err(|e| e.to_string())?;
    if let Some(map) = value.as_object_mut() {
        map.insert("tab".into(), json!("libraries"));
        map.insert(
            "library_sources_off".into(),
            json!(["installed", "folder:/old/shelf"]),
        );
    }
    let stored: Preferences = serde_json::from_value(value).map_err(|e| e.to_string())?;
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, stored, None, Some("libraries"));
    app.refresh();
    assert_eq!(
        app.tab,
        Tab::Library,
        "the old Libraries tab lands on the Library tab"
    );
    assert_eq!(app.libws.page, Page::Sources, "and on its Sources page");
    assert!(app.dock.hide_installed, "the saved switch is kept");
    assert!(
        app.library_folder_off("/old/shelf/a.kicad_sym"),
        "a saved folder switch is kept"
    );
    let ctx = egui::Context::default();
    let out = frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert_eq!(out.viewport_output.len(), 1, "the page paints");
    Ok(())
}

#[test]
fn a_newcomer_chooses_a_folder_to_add_and_sees_what_was_found() -> Result<(), String> {
    let (_folder, _config, mut app, ctx) = sources_open("choose-folder")?;
    let shelf = shelf()?;
    tap_id(&ctx, &mut app, "button:Choose folder")?;
    assert!(app.chooser_open(), "Choose folder opens the chooser");
    type_into(&ctx, &mut app, "textinput:Folder path", &shelf)?;
    tap_id(&ctx, &mut app, "button:Go")?;
    tap_id(&ctx, &mut app, "button:Add this folder")?;
    assert!(
        !app.chooser_open(),
        "the chooser closes once a folder is taken"
    );
    let said = waits_until(&ctx, &mut app, |app| {
        app.library.pending.is_none() && app.library.notice.starts_with("Added")
    });
    assert!(
        said,
        "the card says what was added: {:?}",
        app.library.notice
    );
    assert!(
        app.library_source_rows()
            .iter()
            .any(|r| r.folder.as_deref().is_some_and(|f| f.starts_with(&shelf))),
        "the folder is in the source list"
    );
    Ok(())
}

#[test]
fn a_newcomer_who_types_a_wrong_folder_is_told_so() -> Result<(), String> {
    let (_folder, _config, mut app, ctx) = sources_open("wrong-folder")?;
    tap_id(&ctx, &mut app, "button:Choose folder")?;
    type_into(
        &ctx,
        &mut app,
        "textinput:Folder path",
        "/no/such/folder/here",
    )?;
    tap_id(&ctx, &mut app, "button:Go")?;
    assert!(
        paints_text(&ctx, &mut app, "That is not a folder.")?,
        "the chooser says a missing folder is not a folder"
    );
    tap_id(&ctx, &mut app, "button:Cancel")?;
    assert!(!app.chooser_open());
    Ok(())
}
