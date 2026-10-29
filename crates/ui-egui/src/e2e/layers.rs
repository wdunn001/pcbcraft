use super::super::*;
use crate::test_support::{frame, paints_text, scrolled_to, scrolled_to_at, self_names, tap};
use serde_json::json;

#[test]
fn a_newcomer_finds_anything_through_one_tree() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    app.change_tab(Tab::Pcb);
    app.refresh();
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    tap(&ctx, &mut app, "Layers")?;
    assert!(
        paints_text(&ctx, &mut app, "This board")?,
        "the tree starts at the board itself: {}",
        app.message
    );
    let net = app.project.nets[0].name.clone();
    app.tree_query = net.clone();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        scrolled_to(&ctx, &mut app, &net)?,
        "a search keeps the matching connection in the tree: {}",
        app.message
    );
    app.tree_query.clear();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        scrolled_to(&ctx, &mut app, "Top copper wires")?,
        "the copper layers are rows of the tree: {}",
        app.message
    );
    tap(&ctx, &mut app, "Top copper wires")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        scrolled_to(&ctx, &mut app, "Fade others")?,
        "a copper layer offers a way to fade the rest: {}",
        app.message
    );
    tap(&ctx, &mut app, "Fade others")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert_eq!(
        app.focus_layer.as_deref(),
        Some("copper:front"),
        "fading starts from the top copper layer: {}",
        app.message
    );
    assert!(
        paints_text(&ctx, &mut app, "FOCUSED ON Top copper")?,
        "the drawing says what is focused: {}",
        app.message
    );
    tap(&ctx, &mut app, "Stop fading")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert_eq!(
        app.focus_layer, None,
        "one more tap brings everything back: {}",
        app.message
    );
    Ok(())
}
