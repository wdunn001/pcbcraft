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

#[test]
fn a_newcomer_turns_a_pad_into_an_unplated_locating_peg() -> Result<(), String> {
    let mut pad = authoring::new_pad("3", 2.0);
    pad.pad_type = "unplated".into();
    authoring::apply_unplated_defaults(&mut pad);
    if !pad.number.is_empty() || !pad.layers.is_empty() {
        return Err("an unplated peg must have no number and no copper layers".into());
    }
    let drill = pad.drill.as_ref().ok_or("the peg lost its drill")?;
    if drill.size.x <= 0.0 || pad.size.x > 0.0 {
        return Err("the drill must be positive and the peg must carry no copper".into());
    }
    let definition = pcbcraft_library::FootprintDefinition {
        name: "Peg".into(),
        description: String::new(),
        keywords: String::new(),
        pads: vec![authoring::new_pad("1", 0.0), pad],
        graphics: vec![],
        model: None,
        rules: None,
        raw_source: String::new(),
        unsupported: vec![],
    };
    pcbcraft_doc::validate_footprint_support(&definition)?;

    Ok(())
}

fn peg_connector_project() -> Result<pcbcraft_doc::Project, String> {
    use pcbcraft_doc::{Footprint, Layer, Net, Pad, Track};
    use pcbcraft_geom::Point;
    use pcbcraft_library::{Drill, FootprintDefinition, FootprintPad};
    let pad = |number: &str, kind: &str, x: f64, y: f64, size: f64, drill: f64| FootprintPad {
        number: number.into(),
        pad_type: kind.into(),
        shape: "circle".into(),
        position: Point::new(x, y),
        rotation: 0.0,
        size: Point::new(size, size),
        layers: if kind == "unplated" {
            vec![]
        } else {
            vec!["*.Cu".into(), "*.Mask".into()]
        },
        drill: Some(Drill {
            size: Point::new(drill, drill),
            offset: Point::default(),
        }),
        roundrect_ratio: None,
        chamfer: None,
        polygon: vec![],
        raw_source: String::new(),
    };
    let pads = vec![
        pad("1", "thru_hole", -2.54, 0.0, 1.6, 0.8),
        pad("2", "thru_hole", 2.54, 0.0, 1.6, 0.8),
        pad("", "unplated", 0.0, 0.0, 0.0, 3.0),
    ];
    let definition = FootprintDefinition {
        name: "PegConn".into(),
        description: String::new(),
        keywords: String::new(),
        pads: pads.clone(),
        graphics: vec![],
        model: None,
        rules: None,
        raw_source: String::new(),
        unsupported: vec![],
    };
    let mut p = pcbcraft_doc::Project::default();
    p.nets.push(Net {
        id: 1,
        name: "SIG".into(),
        ..Default::default()
    });
    let mut part = pcbcraft_doc::component_template("resistor", 2, "J1".into(), Point::default())
        .ok_or("no template")?;
    part.library_footprint = Some(pcbcraft_doc::LibraryFootprint {
        library: "test".into(),
        definition,
    });
    p.components.push(part);
    p.board.footprints.push(Footprint {
        id: 4,
        component: 2,
        position: Point::new(30.0, 20.0),
        rotation: 0.0,
        locked: None,
        pads: pads
            .into_iter()
            .map(|g| Pad {
                number: g.number.clone(),
                offset: g.position,
                diameter: g.size.x,
                net: None,
                castellated: None,
                geometry: Some(g),
            })
            .collect(),
    });
    p.board.tracks.push(Track {
        id: 5,
        net: 1,
        start: Point::new(20.0, 20.0),
        end: Point::new(40.0, 20.0),
        width: 0.2,
        layer: Layer::Front,
        arc: None,
    });
    p.validate()?;
    Ok(p)
}

#[test]
fn a_newcomer_places_a_connector_with_locating_pegs() -> Result<(), String> {
    let project = peg_connector_project()?;
    let mut session = pcbcraft_engine::Session::default();
    session.project = project.clone();
    let shared = pcbcraft_mcp::shared(session);
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    // A route across the connector touches the peg drill, so the checks
    // must say so even though the peg has no copper and no net.
    let findings = pcbcraft_rules::drc(&project);
    if !findings
        .iter()
        .any(|f| f.objects.contains(&4) && f.objects.contains(&5))
    {
        return Err(format!(
            "no finding for the track through the peg: {findings:?}"
        ));
    }
    Ok(())
}

#[test]
fn a_newcomer_sees_pegs_drawn_as_holes_and_not_as_copper() -> Result<(), String> {
    let project = peg_connector_project()?;
    let footprint = project.board.footprints.first().ok_or("no footprint")?;
    let mut mechanical = 0;
    for pad in &footprint.pads {
        if canvas::is_mechanical_pad(pad) {
            mechanical += 1;
            let at = footprint.pad_world_position(pad);
            if (at.x - 30.0).abs() > 1e-9 || (at.y - 20.0).abs() > 1e-9 {
                return Err(format!("peg drawn at {at:?}, not at its offset"));
            }
            if pad.drill_outline_points().is_empty() {
                return Err("a peg needs a drill outline to draw".into());
            }
        }
    }
    if mechanical != 1 {
        return Err(format!("expected one peg, found {mechanical}"));
    }
    Ok(())
}
