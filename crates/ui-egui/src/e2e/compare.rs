use super::super::*;
use super::support::{frame, paints_text};
use serde_json::json;

#[test]
fn a_newcomer_compares_their_board_with_yesterdays() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    let baseline = std::env::temp_dir().join(format!(
        "pcbcraft-compare-yesterday-{}.pcbcraft",
        std::process::id()
    ));
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

#[test]
fn a_newcomer_compares_two_parts_and_sees_the_differences() -> Result<(), String> {
    let session = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let listing = |name: &str, package: &str| {
        serde_json::json!({
            "label": name,
            "pins": 2,
            "listing": {
                "source": "fixture",
                "source_id": "sample",
                "source_ref": name,
                "manufacturer": "",
                "mpn": name,
                "description": "",
                "category": "resistor",
                "source_category": "",
                "package_key": "",
                "package_text": package,
                "price_breaks": [{"quantity": 1, "unit_price": 0.1, "currency": "USD"}],
                "lifecycle": "unknown",
                "lifecycle_raw": "",
                "stock": 5,
            }
        })
    };
    let result = session
        .lock()
        .map_err(|_| "session unavailable")?
        .execute(
            "library.compare",
            serde_json::json!({
                "kind": "parts",
                "parts": [listing("A", "1206"), listing("B", "0603")],
                "quantity": 1
            }),
            None,
        )
        .map_err(|e| e.to_string())?;
    let value: serde_json::Value = serde_json::from_str(&result).map_err(|e| e.to_string())?;
    assert_eq!(
        value.pointer("/kind"),
        Some(&serde_json::json!("parts")),
        "the command returns a parts comparison table"
    );
    assert_eq!(
        value.pointer("/table/rows"),
        Some(&serde_json::json!(1)),
        "two parts with different packages show one differing row"
    );
    Ok(())
}
