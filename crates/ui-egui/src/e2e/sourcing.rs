//! Stories for a part's Sourcing section and the bill of materials that reads it.
//!
//! Context: issues #227 and #804. A newcomer opens a part's Properties, types the maker's
//! part number and a supplier's number, and finds both in the exported bill.
use super::super::*;
use super::support::*;

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

/// Choose the first part, as a click on it does, and show its Properties.
fn open_first_part_properties(ctx: &egui::Context, app: &mut App) -> Result<Id, String> {
    let fp = app
        .project
        .board
        .footprints
        .first()
        .ok_or("the sample board has no part")?;
    let (footprint, component) = (fp.id, fp.component);
    app.select(Some(footprint));
    app.refresh();
    for _ in 0..4 {
        frame(ctx, app, vec![], egui::Modifiers::NONE);
    }
    Ok(component)
}

/// Every line of text painted this frame, for failure messages.
fn painted(ctx: &egui::Context, app: &mut App) -> String {
    let output = frame(ctx, app, vec![], egui::Modifiers::NONE);
    output
        .shapes
        .iter()
        .filter_map(|s| match &s.shape {
            egui::Shape::Text(t) => Some(t.galley.text().to_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

#[test]
fn a_newcomer_adds_a_manufacturer_part_number_in_properties() -> Result<(), String> {
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let (ctx, mut app) = board(dir.path());
    let id = open_first_part_properties(&ctx, &mut app)?;
    assert!(
        scrolled_to(&ctx, &mut app, "SOURCING")?,
        "the part's Properties has a Sourcing section: {}",
        painted(&ctx, &mut app)
    );
    assert!(
        scrolled_to(&ctx, &mut app, "Part number")?,
        "with a row for the part number: {}",
        painted(&ctx, &mut app)
    );
    type_into(
        &ctx,
        &mut app,
        "textinput:Manufacturer part number",
        "RC0603FR-0710KL",
    )?;
    type_into(&ctx, &mut app, "textinput:Manufacturer", "Yageo")?;
    app.refresh();
    let sourcing = app
        .project
        .component(id)
        .map(|c| c.sourcing.clone())
        .ok_or("part missing")?;
    assert_eq!(sourcing.mpn, "RC0603FR-0710KL", "{}", app.message);
    assert_eq!(sourcing.manufacturer, "Yageo", "{}", app.message);
    // One Undo takes the maker back off; the part number stays.
    app.tool_call("undo", json!({}));
    app.refresh();
    assert!(
        app.project
            .component(id)
            .is_some_and(|c| c.sourcing.manufacturer.is_empty() && !c.sourcing.mpn.is_empty()),
        "each edit is its own undo step"
    );
    Ok(())
}

#[test]
fn a_newcomer_exports_a_bom_with_supplier_part_numbers() -> Result<(), String> {
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let (ctx, mut app) = board(dir.path());
    let id = open_first_part_properties(&ctx, &mut app)?;
    assert!(
        scrolled_to(&ctx, &mut app, "Add supplier")?,
        "the Sourcing section has a button to add a supplier: {}",
        painted(&ctx, &mut app)
    );
    type_into(&ctx, &mut app, "textinput:Supplier name", "Digi-Key")?;
    type_into(
        &ctx,
        &mut app,
        "textinput:Supplier part number",
        "311-10.0KHRCT-ND",
    )?;
    tap_properties(&ctx, &mut app, "button:Add supplier")?;
    app.refresh();
    assert_eq!(
        app.project
            .component(id)
            .and_then(|c| c.sourcing.supplier_number("Digi-Key").map(str::to_owned))
            .as_deref(),
        Some("311-10.0KHRCT-ND"),
        "the supplier is on the part: {}",
        app.message
    );
    authoring::tests::tap(&ctx, &mut app, "File")?;
    authoring::tests::tap(&ctx, &mut app, "Export")?;
    authoring::tests::tap(&ctx, &mut app, "Bill of materials")?;
    assert!(
        paints_text(&ctx, &mut app, "Supplier part numbers")?,
        "the dialog offers the supplier column"
    );
    authoring::tests::tap(&ctx, &mut app, "Export")?;
    let text = std::fs::read_to_string(dir.path().join("bom.csv")).map_err(|e| e.to_string())?;
    let header = text.lines().next().unwrap_or("");
    assert!(
        header.contains("Supplier part numbers"),
        "the file has the column: {header}"
    );
    assert!(
        text.contains("Digi-Key: 311-10.0KHRCT-ND"),
        "and the number: {text}"
    );
    Ok(())
}
