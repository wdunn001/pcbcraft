//! Stories for the bill of materials flavours: pick the file a recipient wants and be
//! told, before writing, which parts lack what it needs.
//!
//! Context: issue #226 and `crates/io/data/bom_flavours.json`. The flavour picker sits
//! in the Export dialog for the bill of materials; the showcase board has no
//! manufacturer or supplier numbers until the person adds them.
use super::super::*;
use super::support::*;

/// The showcase board opened, on the Layout tab, with a fresh make folder.
fn showcase(dir: &std::path::Path) -> (egui::Context, App) {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.open_showcase();
    app.change_tab(Tab::Pcb);
    app.make_dir = dir.to_string_lossy().into_owned();
    let ctx = egui::Context::default();
    for _ in 0..4 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    (ctx, app)
}

/// Give every part of one value the same manufacturer part number.
fn stamp_part_numbers(app: &mut App) {
    let parts: Vec<(Id, String)> = app
        .project
        .components
        .iter()
        .map(|c| (c.id, c.value.clone()))
        .collect();
    for (id, value) in parts {
        let slug: String = value
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .collect();
        app.tool_call(
            "component.field",
            json!({"id": id, "key": "MPN", "value": format!("MP-{slug}")}),
        );
    }
}

/// File > Export > Bill of materials, then the flavour picked by its label.
fn open_flavour(ctx: &egui::Context, app: &mut App, label: &str) -> Result<(), String> {
    authoring::tests::tap(ctx, app, "File")?;
    authoring::tests::tap(ctx, app, "Export")?;
    authoring::tests::tap(ctx, app, "Bill of materials")?;
    authoring::tests::tap(ctx, app, "Generic CSV")?;
    authoring::tests::tap(ctx, app, label)
}

#[test]
fn a_newcomer_exports_a_distributor_bom() -> Result<(), String> {
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let (ctx, mut app) = showcase(dir.path());
    stamp_part_numbers(&mut app);
    // The picker's list is longer than the window is tall, so this story picks the
    // distributor flavour in the dialog's state; the other story clicks the picker.
    authoring::tests::tap(&ctx, &mut app, "File")?;
    authoring::tests::tap(&ctx, &mut app, "Export")?;
    authoring::tests::tap(&ctx, &mut app, "Bill of materials")?;
    let flavour = pcbcraft_io::formats::bom_flavour("dist_us_b").ok_or("no such flavour")?;
    app.edit.dialog.values = file_dialogs::bom_default_values(&app.project, flavour);
    assert!(
        paints_text(&ctx, &mut app, "Part Number")? && paints_text(&ctx, &mut app, "Quantity")?,
        "the preview shows the distributor's two columns"
    );
    assert!(
        !paints_text(&ctx, &mut app, "Unverified")?,
        "this flavour has a published column list"
    );
    authoring::tests::tap(&ctx, &mut app, "Export")?;
    let text = std::fs::read_to_string(dir.path().join("bom.csv")).map_err(|e| e.to_string())?;
    assert!(
        text.starts_with("Part Number,Quantity\n"),
        "the file has the flavour's headers: {text}"
    );
    assert!(text.lines().count() > 1, "and a row per kind of part");
    assert!(
        text.lines().skip(1).all(|l| l.starts_with("MP-")),
        "each row leads with the manufacturer part number: {text}"
    );
    Ok(())
}

#[test]
fn a_newcomer_is_told_which_parts_lack_a_supplier_number() -> Result<(), String> {
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let (ctx, mut app) = showcase(dir.path());
    open_flavour(&ctx, &mut app, "Prototype assembly, house part numbers")?;
    assert!(
        paints_text(&ctx, &mut app, "Supplier Part #")?,
        "the preview has the flavour's column"
    );
    assert!(
        paints_text(&ctx, &mut app, "55 parts lack a supplier part number")?,
        "Export names the parts that lack a supplier part number before writing"
    );
    assert!(
        !dir.path().join("bom.csv").exists(),
        "nothing is written until Export is pressed"
    );
    Ok(())
}
