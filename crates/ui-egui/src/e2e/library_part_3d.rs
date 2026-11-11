//! Story: a newcomer opens a library part and reads what its 3D picture is made of, then
//! imports a SolidCraft part as the model on the footprint.
//!
//! Context: issues #359 (part of #357) and #374. The part shows in the 3D page's own viewport, and the
//! plain sentence under it names the source, the triangle count, the bounding box and the
//! licence line. The built-in resistor has no model file, so its generated body is shown
//! and the sentence says so. The second story types the path of a SolidCraft part into the
//! model file field, uses it, waits for the loaded sentence, and undoes the change.
use super::super::*;
use super::support::*;
use crate::library_workspace::Page;

/// The browser card of the built-in resistor on the Library tab.
const RESISTOR_CARD: &str = "button:Resistor, Built in \u{b7} MIT OR Apache-#.#";

#[test]
fn a_newcomer_opens_a_part_and_reads_its_3d_model() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    app.change_tab(Tab::Library);
    let ctx = egui::Context::default();
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    tap_id(&ctx, &mut app, RESISTOR_CARD)?;
    assert_eq!(app.libws.parts.len(), 1, "a pick opens one part tab");
    app.libws.page = Page::Model;
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    assert!(
        paints_text(&ctx, &mut app, "Generated body shown")?,
        "a built-in part with no model file reports its generated body"
    );
    assert!(
        paints_text(&ctx, &mut app, "triangles")?,
        "the load sentence counts triangles"
    );
    assert!(
        paints_text(&ctx, &mut app, "Bounding box")?,
        "the load sentence gives the bounding box"
    );
    assert!(
        paints_text(&ctx, &mut app, "Licence: none stated.")?,
        "the load sentence carries a licence line"
    );
    let output = frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        output
            .shapes
            .iter()
            .any(|s| matches!(s.shape, egui::Shape::Mesh(_))),
        "the part is drawn as triangles in its own viewport"
    );
    Ok(())
}

/// A copy of the two-body SolidCraft part fixture at a path the 3D model reader can open.
fn solidcraft_scratch_file() -> Result<String, String> {
    let dir = std::env::temp_dir().join(format!("pcbcraft-lib-part-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let file = dir.join("cap-two-bodies.solidcraft");
    let bytes = include_bytes!("../../../io/tests/fixtures/solidcraft/part_two_bodies.solidcraft");
    std::fs::write(&file, bytes).map_err(|e| e.to_string())?;
    file.to_str()
        .map(str::to_string)
        .ok_or("scratch path".to_string())
}

/// A newcomer points a built-in part at a SolidCraft part file, sees the model loaded on
/// the footprint page, and undo brings the generated body back.
#[test]
fn a_newcomer_imports_a_solidcraft_part() -> Result<(), String> {
    let path = solidcraft_scratch_file()?;
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    app.change_tab(Tab::Library);
    let ctx = egui::Context::default();
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    tap_id(&ctx, &mut app, RESISTOR_CARD)?;
    assert_eq!(app.libws.parts.len(), 1, "a pick opens one part tab");
    // A model file lives on the project's own copy of the footprint, so the built-in
    // part joins the project library first, one command, like Add to project does.
    let card = app
        .libws
        .parts
        .first()
        .ok_or("a pick opens one part tab")?
        .card
        .clone();
    let def = app
        .dock_card_footprint(&card)
        .ok_or("the resistor card carries a footprint")?;
    let definition = serde_json::to_value(&def).map_err(|e| e.to_string())?;
    assert!(
        app.tool_call(
            "library.footprint.upsert",
            json!({"definition": definition})
        ),
        "the footprint is saved in the project library"
    );
    app.refresh();
    app.libws.page = Page::Model;
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    // The part is now in the project library, so the file field and buttons are live.
    if on_screen(
        &ctx,
        &mut app,
        "checkbox:Allow editing library footprint transforms",
    ) {
        tap_id(
            &ctx,
            &mut app,
            "checkbox:Allow editing library footprint transforms",
        )?;
        for _ in 0..2 {
            frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
        }
    }
    type_into(&ctx, &mut app, "textinput:Model file path", &path)?;
    tap_id(&ctx, &mut app, "button:Use file")?;
    assert!(
        waits_for_text(&ctx, &mut app, "Model loaded")?,
        "the SolidCraft part loads as the model on the footprint"
    );
    // Undo the model file the same way the Edit menu does: one Ctrl+Z.
    let ctrl = egui::Modifiers {
        ctrl: true,
        command: true,
        ..Default::default()
    };
    frame(
        &ctx,
        &mut app,
        vec![egui::Event::Key {
            key: egui::Key::Z,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: ctrl,
        }],
        ctrl,
    );
    for _ in 0..2 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    assert!(
        paints_text(&ctx, &mut app, "No model file")?,
        "undo drops the model file and the generated body returns"
    );
    std::fs::remove_file(&path).map_err(|e| e.to_string())?;
    Ok(())
}
