//! Stories for the Samtec manufacturer box on the Library Datasheet page (issue #785).
//!
//! A newcomer opens a part whose manufacturer is Samtec, types the path of a file downloaded
//! from the manufacturer page and presses Import. Nothing is fetched: the part is a project
//! part with a Samtec part number and the downloaded file is a small fixture in a temp folder.
//!
//! # Dependencies
//! - crates/ui-egui/src/library_workspace/datasheet.rs
//! - crates/engine/src/authoring.rs
//! - crates/library/tests/fixtures/R_0805_2012Metric.kicad_mod
use super::super::*;
use super::support::*;

/// The id of the browser card of the built-in resistor.
const RESISTOR_CARD: &str = "button:Resistor, Built in \u{b7} MIT OR Apache-#.#";
/// The id of the path box in the manufacturer card.
const PATH_BOX: &str = "textinput:Path of the downloaded file";
/// The id of the Import button in the manufacturer card.
const IMPORT: &str = "button:Import downloaded file";

/// The Datasheet page of a resistor that is made by Samtec, with the manufacturer card showing.
fn samtec_part() -> Result<(egui::Context, App, TallScope), String> {
    let tall = TallScope::enter();
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    let resistor = app
        .project
        .components
        .iter()
        .find(|c| c.reference.starts_with('R'))
        .map(|c| c.id)
        .ok_or("the practice design carries a resistor")?;
    app.tool_call(
        "component.set_sourcing",
        json!({"id": resistor, "manufacturer": "Samtec", "mpn": "TMM-102-01-L-D"}),
    );
    app.change_tab(Tab::Library);
    let ctx = egui::Context::default();
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    tap_id(&ctx, &mut app, RESISTOR_CARD)?;
    tap_id(&ctx, &mut app, "tab:Datasheet")?;
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    if !app
        .project
        .components
        .iter()
        .any(|c| c.sourcing.manufacturer == "Samtec")
    {
        return Err("the part carries Samtec as its manufacturer".into());
    }
    if !paints_text(&ctx, &mut app, "Manufacturer page")? {
        return Err("a Samtec part shows the manufacturer card".into());
    }
    Ok((ctx, app, tall))
}

#[test]
fn a_newcomer_cannot_press_import_until_a_path_is_typed() -> Result<(), String> {
    let (ctx, mut app, _tall) = samtec_part()?;
    assert!(
        tap_id(&ctx, &mut app, IMPORT).is_err() && app.project.library.notices.is_empty(),
        "Import cannot be pressed while the box is empty"
    );
    type_into(&ctx, &mut app, PATH_BOX, "C:/downloads/part.kicad_mod")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert_eq!(
        app.libws.manufacturer_path, "C:/downloads/part.kicad_mod",
        "the typed path stays in the box from one frame to the next"
    );
    assert!(
        tap_id(&ctx, &mut app, IMPORT).is_ok(),
        "Import can be pressed once a path is typed"
    );
    Ok(())
}

#[test]
fn a_newcomer_importing_a_file_that_is_not_there_reads_a_plain_message() -> Result<(), String> {
    let (ctx, mut app, _tall) = samtec_part()?;
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let missing = dir.path().join("absent.kicad_mod");
    type_into(&ctx, &mut app, PATH_BOX, &missing.display().to_string())?;
    tap_id(&ctx, &mut app, IMPORT)?;
    assert!(
        app.message.contains("no downloaded file was found"),
        "the status line says the file is missing, got: {}",
        app.message
    );
    assert!(
        app.project.library.notices.is_empty(),
        "a refused import stores no notice"
    );
    Ok(())
}

#[test]
fn a_newcomer_imports_a_downloaded_footprint_and_it_keeps_the_notice() -> Result<(), String> {
    let (ctx, mut app, _tall) = samtec_part()?;
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let file = dir.path().join("TMM-102-01-L-D.kicad_mod");
    let fixture = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../library/tests/fixtures/R_0805_2012Metric.kicad_mod"
    );
    std::fs::copy(fixture, &file).map_err(|e| e.to_string())?;
    type_into(&ctx, &mut app, PATH_BOX, &file.display().to_string())?;
    tap_id(&ctx, &mut app, IMPORT)?;
    assert!(
        app.project
            .library
            .footprints
            .iter()
            .any(|f| f.name == "R_0805_2012Metric"),
        "the downloaded footprint is in the project library, status: {}",
        app.message
    );
    assert!(
        app.project
            .library
            .notices
            .iter()
            .any(|n| n.contains("downloaded by the user") && n.contains("not redistributed")),
        "the import stored the not-redistributed notice: {:?}",
        app.project.library.notices
    );
    Ok(())
}
