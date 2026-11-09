use super::super::*;
use super::support::*;

#[test]
fn a_newcomer_reads_the_drill_table_in_manufacture() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    let net = app
        .project
        .nets
        .first()
        .map(|n| n.id)
        .ok_or("the practice design should have a net")?;
    for x in [12.0, 14.0, 16.0] {
        app.tool_call(
            "via.place",
            json!({"net": net, "position": {"x": x, "y": 12.0}, "diameter": 0.7, "drill": 0.3}),
        );
    }
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    tap_id(&ctx, &mut app, "tab:Manufacture")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    for words in [
        "Drill table",
        "Layers",
        "Plated",
        "Used for",
        "Size",
        "Holes",
        "Tolerance",
    ] {
        assert!(
            paints_text(&ctx, &mut app, words)?,
            "the table has a {words:?} heading: {}",
            app.message
        );
    }
    assert!(
        paints_text(&ctx, &mut app, "L1 to L2 through")?,
        "a through hole reads as running from the first layer to the last: {}",
        app.message
    );
    assert!(
        paints_text(&ctx, &mut app, "0.300 mm")?,
        "the via drill size is listed: {}",
        app.message
    );
    assert!(
        paints_text(&ctx, &mut app, "None recorded")?,
        "with no tolerance on the board the table says so: {}",
        app.message
    );
    let rows = pcbcraft_io::drill_table(&app.project)?;
    let vias: usize = rows
        .iter()
        .filter(|r| r.function == pcbcraft_io::DrillFunction::Via)
        .map(|r| r.count)
        .sum();
    assert!(vias >= 3, "the three new vias are counted: {rows:?}");
    Ok(())
}
