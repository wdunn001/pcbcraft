//! Stories for Import, Save as and Export: one dialog fed by the format registry.
//!
//! Context: issue #194 and `crates/io/src/formats.rs`. A newcomer finds a bill of
//! materials under File > Export, in a part's right-click menu and in Manufacture;
//! the File menu itself names no format.
use super::super::*;
use super::board::right_click;
use super::support::*;
use pcbcraft_io::formats::{self, Direction, Scope};

/// The sample board on the Layout tab with a fresh make folder.
fn board(dir: &std::path::Path) -> (egui::Context, App) {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    app.change_tab(Tab::Pcb);
    app.needs_fit = true;
    app.make_dir = dir.to_string_lossy().into_owned();
    let ctx = egui::Context::default();
    for _ in 0..4 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    (ctx, app)
}

/// Right-click the first part of the board.
fn right_click_first_part(ctx: &egui::Context, app: &mut App) -> Result<Id, String> {
    let fp = app
        .project
        .board
        .footprints
        .first()
        .map(|f| f.id)
        .ok_or("the sample board has no part")?;
    let at = app
        .project
        .object_position(fp)
        .ok_or("the part has no position")?;
    for _ in 0..4 {
        frame(ctx, app, vec![], egui::Modifiers::NONE);
    }
    let screen = pcb_screen(app, at);
    right_click(ctx, app, screen);
    Ok(fp)
}

fn file_export(ctx: &egui::Context, app: &mut App, entry: &str) -> Result<(), String> {
    authoring::tests::tap(ctx, app, "File")?;
    authoring::tests::tap(ctx, app, "Export")?;
    authoring::tests::tap(ctx, app, entry)
}

#[test]
fn a_newcomer_exports_a_bom_from_the_file_menu() -> Result<(), String> {
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let (ctx, mut app) = board(dir.path());
    file_export(&ctx, &mut app, "Bill of materials")?;
    assert!(
        paints_text(&ctx, &mut app, "Group identical parts")?
            && paints_text(&ctx, &mut app, "Preview")?
            && paints_text(&ctx, &mut app, "Generic CSV")?,
        "the dialog shows the options and a preview"
    );
    authoring::tests::tap(&ctx, &mut app, "Export")?;
    let text = std::fs::read_to_string(dir.path().join("bom.csv")).map_err(|e| e.to_string())?;
    assert!(
        text.starts_with("Reference,Value,Footprint,MPN,Manufacturer,Quantity"),
        "the file has the columns the dialog showed: {text}"
    );
    assert!(text.lines().count() > 1, "and at least one part row");
    Ok(())
}

#[test]
fn a_newcomer_exports_a_bom_from_a_part_right_click() -> Result<(), String> {
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let (ctx, mut app) = board(dir.path());
    right_click_first_part(&ctx, &mut app)?;
    assert!(
        paints_text(&ctx, &mut app, "Export BOM…")?,
        "the part menu offers a bill of materials"
    );
    assert!(
        !paints_text(&ctx, &mut app, "Export BOM for selection…")?,
        "one part selected: no selection entry"
    );
    authoring::tests::tap(&ctx, &mut app, "Export BOM…")?;
    assert!(app.edit.dialog.open, "the Export dialog opens");
    assert_eq!(app.edit.dialog.format, "bom", "on the bill of materials");
    assert!(
        app.edit.dialog.only.is_empty(),
        "with every part, not a selection"
    );
    Ok(())
}

#[test]
fn a_newcomer_excludes_a_part_from_the_bom() -> Result<(), String> {
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let (ctx, mut app) = board(dir.path());
    let before = app.bom_row_count();
    let fp = right_click_first_part(&ctx, &mut app)?;
    let reference = app
        .project
        .footprint(fp)
        .and_then(|f| app.project.component(f.component))
        .map(|c| c.reference.clone())
        .ok_or("no reference")?;
    authoring::tests::tap(&ctx, &mut app, "Exclude from BOM")?;
    assert!(app.part_excluded(fp), "the part now carries the flag");
    let after = app.bom_row_count();
    assert!(
        after < before || before == 0,
        "the list shrank: {before} to {after}"
    );
    file_export(&ctx, &mut app, "Bill of materials")?;
    authoring::tests::tap(&ctx, &mut app, "Export")?;
    let text = std::fs::read_to_string(dir.path().join("bom.csv")).map_err(|e| e.to_string())?;
    assert!(
        !text.lines().skip(1).any(|l| l
            .split(',')
            .any(|c| c.trim_matches('"').split(", ").any(|r| r == reference))),
        "{reference} is not in the file: {text}"
    );
    right_click_first_part(&ctx, &mut app)?;
    assert!(
        paints_text(&ctx, &mut app, "Include in BOM")?,
        "the menu now offers to put it back"
    );
    authoring::tests::tap(&ctx, &mut app, "Include in BOM")?;
    assert!(!app.part_excluded(fp), "and it comes back");
    Ok(())
}

#[test]
fn a_newcomer_sees_the_bom_in_manufacture() -> Result<(), String> {
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let (ctx, mut app) = board(dir.path());
    app.change_tab(Tab::PlotFiles);
    for _ in 0..4 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    assert!(
        paints_text(&ctx, &mut app, "Bill of materials")? && paints_text(&ctx, &mut app, " rows")?,
        "Manufacture lists the bill with its row count"
    );
    authoring::tests::tap(&ctx, &mut app, "Preview")?;
    assert!(
        paints_text(&ctx, &mut app, "Bill of materials preview")?,
        "Preview opens the whole list"
    );
    // The output jobs that include `bom` write the same list, leaving out excluded parts.
    let component = app
        .project
        .components
        .first()
        .map(|c| (c.id, c.reference.clone()))
        .ok_or("no part")?;
    app.set_bom_exclusion(&[component.0], true);
    app.tool_call(
        "output_job.add",
        json!({"name": "assembly", "kinds": ["bom"]}),
    );
    let out = dir.path().join("job");
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    app.tool_call(
        "output_job.run",
        json!({"name": "assembly", "directory": out.to_string_lossy()}),
    );
    let text = std::fs::read_to_string(out.join("bom.csv")).map_err(|e| e.to_string())?;
    assert!(
        !text.contains(&component.1),
        "the job's list leaves {} out: {text}",
        component.1
    );
    Ok(())
}

#[test]
fn a_newcomer_finds_every_export_format_in_export_as() -> Result<(), String> {
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let (ctx, mut app) = board(dir.path());
    file_export(&ctx, &mut app, "More…")?;
    for (group, _) in formats::grouped(Direction::Export, Scope::Output) {
        assert!(
            paints_text(&ctx, &mut app, group.label())?,
            "heading {}",
            group.label()
        );
    }
    for f in formats::listed(Direction::Export, Scope::Output) {
        assert!(
            paints_text(&ctx, &mut app, f.label)?,
            "{} is listed",
            f.label
        );
    }
    for project_format in formats::listed(Direction::Export, Scope::Project) {
        assert!(
            !paints_text(&ctx, &mut app, project_format.label)?,
            "{} belongs to Save as, not Export",
            project_format.label
        );
    }
    assert!(
        paints_text(&ctx, &mut app, "Packages")?
            && paints_text(&ctx, &mut app, "No packages yet.")?,
        "the Packages group is there, empty"
    );
    Ok(())
}

#[test]
fn a_newcomer_opens_more_and_sees_every_format() -> Result<(), String> {
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let (ctx, mut app) = board(dir.path());
    file_export(&ctx, &mut app, "More…")?;
    assert!(
        app.edit.dialog.open && app.edit.dialog.format.is_empty(),
        "nothing preselected"
    );
    assert!(paints_text(&ctx, &mut app, "Pick a format on the left.")?);
    app.edit.dialog.open = false;
    authoring::tests::tap(&ctx, &mut app, "File")?;
    authoring::tests::tap(&ctx, &mut app, "Import")?;
    authoring::tests::tap(&ctx, &mut app, "More…")?;
    for f in formats::listed(Direction::Import, Scope::Parts) {
        assert!(
            paints_text(&ctx, &mut app, f.label)?,
            "{} can be imported",
            f.label
        );
    }
    Ok(())
}

#[test]
fn a_newcomer_exports_from_a_common_item_with_the_dialog_preconfigured() -> Result<(), String> {
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let (ctx, mut app) = board(dir.path());
    authoring::tests::tap(&ctx, &mut app, "File")?;
    authoring::tests::tap(&ctx, &mut app, "Export")?;
    for f in formats::listed(Direction::Export, Scope::Output)
        .into_iter()
        .filter(|f| f.common)
    {
        assert!(
            paints_text(&ctx, &mut app, f.label)?,
            "{} is a common entry",
            f.label
        );
    }
    authoring::tests::tap(&ctx, &mut app, "Pick and place")?;
    assert_eq!(
        app.edit.dialog.format, "placement",
        "the dialog opens on that format"
    );
    authoring::tests::tap(&ctx, &mut app, "Export")?;
    assert!(
        std::fs::read_dir(dir.path())
            .map_err(|e| e.to_string())?
            .any(|e| e.is_ok_and(|e| e.file_name().to_string_lossy().contains("placement"))),
        "one press writes the file: {}",
        app.message
    );
    Ok(())
}

#[test]
fn a_newcomer_repeats_a_recent_export_with_its_options() -> Result<(), String> {
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let (ctx, mut app) = board(dir.path());
    app.open_dialog(file_dialogs::Mode::Export, Some("bom"), None, vec![]);
    app.edit
        .dialog
        .values
        .insert("File type".into(), "tsv".into());
    app.edit
        .dialog
        .values
        .insert("Group identical parts".into(), "false".into());
    authoring::tests::tap(&ctx, &mut app, "Export")?;
    assert!(
        dir.path().join("bom.tsv").exists(),
        "first export: {}",
        app.message
    );
    std::fs::remove_file(dir.path().join("bom.tsv")).map_err(|e| e.to_string())?;
    authoring::tests::tap(&ctx, &mut app, "File")?;
    authoring::tests::tap(&ctx, &mut app, "Export")?;
    assert!(paints_text(&ctx, &mut app, "Recent")?, "a Recent heading");
    authoring::tests::tap(&ctx, &mut app, "Bill of materials (TSV, one row per part)")?;
    assert_eq!(
        app.edit.dialog.values.get("File type").map(String::as_str),
        Some("tsv"),
        "its options come back"
    );
    authoring::tests::tap(&ctx, &mut app, "Export")?;
    assert!(
        dir.path().join("bom.tsv").exists(),
        "one press repeats it: {}",
        app.message
    );
    Ok(())
}

#[test]
fn a_newcomer_saves_the_project_and_exports_a_bom() -> Result<(), String> {
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let (ctx, mut app) = board(dir.path());
    authoring::tests::tap(&ctx, &mut app, "File")?;
    authoring::tests::tap(&ctx, &mut app, "Save as…")?;
    assert!(
        paints_text(&ctx, &mut app, "PCBCraft project")?
            && paints_text(&ctx, &mut app, "Project archive")?
            && !paints_text(&ctx, &mut app, "Bill of materials")?,
        "Save as lists whole-project formats only"
    );
    let target = dir.path().join("mine.pcbcraft");
    app.edit.dialog.path = target.to_string_lossy().into_owned();
    authoring::tests::tap(&ctx, &mut app, "Save")?;
    assert!(target.exists(), "the project is saved: {}", app.message);
    file_export(&ctx, &mut app, "Bill of materials")?;
    assert!(
        !paints_text(&ctx, &mut app, "PCBCraft project")?,
        "Export lists made-from-the-project formats only"
    );
    authoring::tests::tap(&ctx, &mut app, "Export")?;
    assert!(dir.path().join("bom.csv").exists(), "{}", app.message);
    Ok(())
}

#[test]
fn a_newcomer_gets_a_bring_up_checklist_for_the_board() -> Result<(), String> {
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let (ctx, mut app) = board(dir.path());
    file_export(&ctx, &mut app, "More…")?;
    authoring::tests::tap(&ctx, &mut app, "Bring-up test plan")?;
    authoring::tests::tap(&ctx, &mut app, "Export")?;
    let md = std::fs::read_to_string(dir.path().join("bring-up-plan.md")).map_err(|e| e.to_string())?;
    assert!(
        md.contains("Bring-up plan") && md.contains("Power rails"),
        "the markdown checklist covers rails: {md}"
    );
    let csv = std::fs::read_to_string(dir.path().join("bring-up-plan.csv")).map_err(|e| e.to_string())?;
    assert!(
        csv.contains("SECTION") && csv.contains("rail"),
        "the csv mirrors the plan: {csv}"
    );
    Ok(())
}

#[test]
fn a_newcomer_sees_no_format_in_the_file_menu() -> Result<(), String> {
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let (ctx, mut app) = board(dir.path());
    authoring::tests::tap(&ctx, &mut app, "File")?;
    for word in ["Open…", "Import", "Save", "Save as…", "Export"] {
        assert!(paints_text(&ctx, &mut app, word)?, "File has {word}");
    }
    for f in formats::FORMATS {
        assert!(
            !paints_text(&ctx, &mut app, f.label)?,
            "the File menu names no format, but shows {}",
            f.label
        );
    }
    Ok(())
}
