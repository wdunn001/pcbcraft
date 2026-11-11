//! Story: a newcomer opens a library part and reads what its 3D picture is made of.
//!
//! Context: issue #359, part of #357. The part shows in the 3D page's own viewport, and the
//! plain sentence under it names the source, the triangle count, the bounding box and the
//! licence line. The built-in resistor has no model file, so its generated body is shown and
//! the sentence says so.
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
