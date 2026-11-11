//! End-to-end acceptance stories for the export packages epic.
//!
//! Context: issue #64 and `crates/io/data/packages.json`. A newcomer picks a
//! named package from the Packages group of the Export as... dialog, reads the
//! file names the preview lists before anything is written, and checks what
//! the export put in the make folder.
use super::super::*;
use super::export::{file_export, in_format_list};
use super::support::*;

/// The sample board with a fresh make folder.
fn sample_board(dir: &std::path::Path) -> (egui::Context, App) {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    app.change_tab(Tab::Pcb);
    app.make_dir = dir.to_string_lossy().into_owned();
    let ctx = egui::Context::default();
    for _ in 0..4 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    (ctx, app)
}

/// Names of the entries in a folder.
fn names(dir: &std::path::Path) -> Result<Vec<String>, String> {
    Ok(std::fs::read_dir(dir)
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect())
}

/// A newcomer exports the prototype fab package from the File menu and finds
/// a zip named after the project carrying its manifest.
#[test]
fn a_newcomer_exports_a_fab_package_from_the_file_menu() -> Result<(), String> {
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let (ctx, mut app) = sample_board(dir.path());
    file_export(&ctx, &mut app, "More…")?;
    let listed = in_format_list(&ctx, &mut app, "Prototype fab (Gerber + drill)")?;
    assert!(listed, "the fab package is listed under Packages");
    authoring::tests::tap(&ctx, &mut app, "Prototype fab (Gerber + drill)")?;
    for _ in 0..4 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    authoring::tests::tap(&ctx, &mut app, "Export package")?;
    for _ in 0..4 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    let found = names(dir.path())?;
    let zip = found
        .iter()
        .find(|n| n.ends_with("-fab-A.zip"))
        .ok_or_else(|| format!("no fab zip was written: {found:?} / {}", app.message))?;
    let bytes = std::fs::read(dir.path().join(zip)).map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&bytes);
    assert!(
        text.contains("manifest.json"),
        "the zip carries its manifest"
    );
    assert!(
        text.contains(".GTL") && text.contains(".TXT"),
        "Protel-style layer and drill names"
    );
    assert!(!app.edit.dialog.open, "the dialog closes after the export");
    Ok(())
}

/// A newcomer selects the fab package, reads the file names the preview
/// lists, and finds that nothing has been written yet.
#[test]
fn a_newcomer_sees_the_file_names_before_exporting_a_package() -> Result<(), String> {
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let (ctx, mut app) = sample_board(dir.path());
    file_export(&ctx, &mut app, "More…")?;
    let listed = in_format_list(&ctx, &mut app, "Prototype fab (Gerber + drill)")?;
    assert!(listed, "the fab package is listed under Packages");
    authoring::tests::tap(&ctx, &mut app, "Prototype fab (Gerber + drill)")?;
    for _ in 0..4 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    assert_eq!(
        app.edit.dialog.package_pick, "fab_prototype",
        "picking the package sets package_pick"
    );
    assert!(
        paints_text(&ctx, &mut app, ".GTL")?,
        "the preview lists a top copper layer name"
    );
    assert!(
        paints_text(&ctx, &mut app, ".TXT")?,
        "the preview lists a drill file name"
    );
    assert!(
        names(dir.path())?.is_empty(),
        "nothing is written before exporting"
    );
    Ok(())
}

/// A newcomer exports the online prototype assembler package and reads the
/// column names back from the bill of materials it wrote.
#[test]
fn a_newcomer_exports_an_assembly_package_and_reads_the_bom_headers() -> Result<(), String> {
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let (ctx, mut app) = sample_board(dir.path());
    file_export(&ctx, &mut app, "More…")?;
    let listed = in_format_list(&ctx, &mut app, "Assembly, online prototype house")?;
    assert!(listed, "the assembly package is listed under Packages");
    authoring::tests::tap(&ctx, &mut app, "Assembly, online prototype house")?;
    for _ in 0..4 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    authoring::tests::tap(&ctx, &mut app, "Export package")?;
    for _ in 0..4 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    let found = names(dir.path())?;
    let bom = found
        .iter()
        .find(|n| n.ends_with("-bom-A.csv"))
        .ok_or_else(|| format!("no bom csv was written: {found:?} / {}", app.message))?;
    let text = std::fs::read_to_string(dir.path().join(bom)).map_err(|e| e.to_string())?;
    let header = text.lines().next().ok_or("the bom csv is empty")?;
    assert!(
        header.contains("Reference") && header.contains("Footprint"),
        "the bom names its columns: {header}"
    );
    assert!(text.lines().count() > 1, "the bom has part rows");
    Ok(())
}

/// A newcomer exports the IPC-2581 package and finds one xml file named
/// after the project and nothing else in the make folder.
#[test]
fn a_newcomer_exports_a_single_file_ipc2581_package() -> Result<(), String> {
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let (ctx, mut app) = sample_board(dir.path());
    file_export(&ctx, &mut app, "More…")?;
    let listed = in_format_list(&ctx, &mut app, "IPC-2581 single file")?;
    assert!(listed, "the IPC-2581 package is listed under Packages");
    authoring::tests::tap(&ctx, &mut app, "IPC-2581 single file")?;
    for _ in 0..4 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    authoring::tests::tap(&ctx, &mut app, "Export package")?;
    for _ in 0..4 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    let found = names(dir.path())?;
    let xml = found.iter().filter(|n| n.ends_with(".xml")).count();
    assert_eq!(xml, 1, "exactly one xml document was written: {found:?}");
    assert!(
        found.iter().all(|n| n.ends_with(".xml")),
        "the package writes nothing but the single file: {found:?}"
    );
    Ok(())
}

/// A newcomer duplicates the prototype fab package under their own name; the
/// edited package is kept with the project and listed in Export as.
#[test]
fn a_newcomer_edits_a_package_and_saves_it_with_the_project() -> Result<(), String> {
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let (ctx, mut app) = sample_board(dir.path());
    assert!(
        app.tool_call(
            "package.duplicate",
            json!({"id": "fab_prototype", "new_id": "my_fab", "name": "My workshop fab"})
        ),
        "the package was duplicated: {}",
        app.message
    );
    assert!(
        app.project
            .packages
            .iter()
            .any(|k| k.id == "my_fab" && k.name == "My workshop fab"),
        "the edited package is saved with the project"
    );
    file_export(&ctx, &mut app, "More…")?;
    let listed = in_format_list(&ctx, &mut app, "My workshop fab")?;
    assert!(listed, "the edited package is listed under Packages");
    Ok(())
}
