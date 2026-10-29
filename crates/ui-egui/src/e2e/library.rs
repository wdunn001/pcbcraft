use super::super::*;
use super::support::*;

#[test]
fn a_newcomer_searches_for_a_part_and_it_is_ready_to_place() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Add part")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        paints_text(&ctx, &mut app, "Search parts by name")?,
        "the part search opens with a box that says what to type"
    );
    authoring::tests::tap(&ctx, &mut app, "Search parts by name")?;
    for letter in "resistor".chars() {
        frame(
            &ctx,
            &mut app,
            vec![egui::Event::Text(letter.into())],
            egui::Modifiers::NONE,
        );
    }
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert_eq!(
        app.part_search, "resistor",
        "typing in the search box arrives letter by letter"
    );
    assert!(
        paints_text(&ctx, &mut app, "Resistor")?,
        "the matching part stays in the list"
    );
    assert!(
        !paints_text(&ctx, &mut app, "Capacitor")?,
        "parts that do not match the search disappear from the list"
    );
    authoring::tests::tap(&ctx, &mut app, "Resistor")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        matches!(&app.tool, Tool::Place(kind) if kind == "resistor"),
        "picking the part arms the schematic to place it, got {:?}",
        app.tool
    );
    assert_eq!(
        app.tab,
        Tab::Schematic,
        "the sheet comes forward to receive it"
    );
    Ok(())
}
#[test]
fn a_newcomer_sees_what_each_part_is_and_hides_a_whole_set() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    let r1 = app
        .project
        .components
        .iter()
        .find(|c| c.reference == "R1")
        .map(|c| c.id)
        .ok_or("the practice board should have a resistor")?;
    app.tool_call(
        "component.field",
        json!({"id": r1, "key": "Description", "value": "blinky red"}),
    );
    app.tool_call(
        "library.symbol.upsert",
        json!({"definition": {
            "name": "SetR",
            "reference": "R",
            "value": "10k",
            "description": "thin film",
            "keywords": "resistor trim",
            "footprint": "",
            "set": "Power bits",
            "pins": [{"number": "1", "name": "", "position": {"x": -5.0, "y": 0.0},
                "angle": 0.0, "length": 2.5, "electrical_type": "passive",
                "unit": 1, "body_style": 0}],
            "graphics": [],
            "extends": null,
            "raw_source": "",
            "unsupported": []
        }}),
    );
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        paints_text(&ctx, &mut app, "    blinky red · built in")?,
        "a part row says what the part is and where it came from: {}",
        app.message
    );
    authoring::tests::tap(&ctx, &mut app, "Add part")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        paints_text(&ctx, &mut app, "Power bits (1 part)")?,
        "library parts are grouped under their set name: {}",
        app.message
    );
    assert!(
        paints_text(&ctx, &mut app, "thin film · search words: resistor · trim")?,
        "each library part is a card that says what it is: {}",
        app.message
    );
    assert!(
        paints_text(&ctx, &mut app, "SetR")?,
        "the part itself is offered under its set: {}",
        app.message
    );
    authoring::tests::tap(&ctx, &mut app, "Power bits (1 part)")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        !paints_text(&ctx, &mut app, "SetR")?,
        "unticking the set hides every part inside it: {}",
        app.message
    );
    authoring::tests::tap(&ctx, &mut app, "Power bits (1 part)")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        paints_text(&ctx, &mut app, "SetR")?,
        "ticking it again brings the group back: {}",
        app.message
    );
    Ok(())
}
