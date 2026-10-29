use super::super::*;
use super::support::{frame, paints_text};
use serde_json::json;

#[test]
fn a_newcomer_compares_their_board_with_yesterdays() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    let baseline = std::env::temp_dir().join("pcbcraft-compare-yesterday.pcbcraft");
    pcbcraft_io::save(&app.project, &baseline)?;
    let r1 = app
        .project
        .components
        .iter()
        .find(|c| c.reference == "R1")
        .ok_or("the worked example should include R1")?;
    let r1_id = r1.id;
    app.run("component.value", json!({"id": r1_id, "value": "470"}));
    let fp = app
        .project
        .board
        .footprints
        .iter()
        .find(|f| f.component == r1_id)
        .ok_or("R1 should have a board footprint after sync")?;
    let moved = Point::new(fp.position.x + 3.0, fp.position.y);
    app.run(
        "footprint.move",
        json!({"id": fp.id, "position": {"x": moved.x, "y": moved.y}}),
    );
    app.refresh();
    app.compare_baseline_path = baseline.to_string_lossy().into_owned();
    app.compare_open = true;
    app.run_design_compare()?;
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        app.message.contains("change") || app.message.contains("Compare"),
        "the status strip should announce compare results: {}",
        app.message
    );
    assert!(
        !app.compare_tint.is_empty(),
        "changed parts should pick up a highlight tint: {}",
        app.message
    );
    assert!(
        paints_text(&ctx, &mut app, "R1")? || paints_text(&ctx, &mut app, "470")?,
        "the compare report or window should name the edited part: {}",
        app.message
    );
    let _ = std::fs::remove_file(&baseline);
    Ok(())
}
