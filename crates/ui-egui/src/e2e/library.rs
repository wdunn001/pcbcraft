use super::super::*;
use super::support::*;
use crate::authoring::Draft;
use pcbcraft_library::Graphic;

#[test]
fn a_newcomer_searches_for_a_part_and_it_is_ready_to_place() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.region().left_tab = "library".to_owned();
    app.tool_call("sample_design", json!({}));
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.change_tab(Tab::Schematic);
    authoring::tests::tap(&ctx, &mut app, "Place part")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        paints_text(&ctx, &mut app, "Keyword or part number")?,
        "the part search opens with a box that says what to type"
    );
    tap_id(
        &ctx,
        &mut app,
        "button:#-pin connector, Built in \u{b7} GPL-#.#-or-later",
    )?;
    assert!(
        matches!(&app.tool, Tool::Place(kind) if kind.contains("connector")),
        "a connector can be armed straight from the list, got {:?}",
        app.tool
    );
    authoring::tests::tap(&ctx, &mut app, "Capacitor")?;
    assert!(
        matches!(&app.tool, Tool::Place(kind) if kind == "capacitor"),
        "a capacitor can be armed from the list, got {:?}",
        app.tool
    );
    app.change_tab(Tab::Schematic);
    authoring::tests::tap(&ctx, &mut app, "Place part")?;
    tap_id(&ctx, &mut app, "textinput:Keyword or part number")?;
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
    let out = frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        !out.shapes.iter().any(|s| matches!(
            &s.shape,
            egui::Shape::Text(t) if t.galley.text() == "Capacitor"
        )),
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
    app.region().left_tab = "library".to_owned();
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
    app.change_tab(Tab::Schematic);
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    // The component list no longer shows a description or source line per row.
    assert!(
        paints_text(&ctx, &mut app, "Power bits (1 part)")?,
        "library parts are grouped under their set name: {}",
        app.message
    );
    assert!(
        paints_text(&ctx, &mut app, "thin film")?,
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
        side: None,
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

#[test]
fn a_newcomer_finds_a_real_part_by_number_and_places_it() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared.clone(), Preferences::default(), None, None);
    app.refresh();
    app.region().left_tab = "library".to_owned();
    let ctx = egui::Context::default();
    let none = egui::Modifiers::NONE;
    frame(&ctx, &mut app, vec![], none);
    app.change_tab(Tab::Schematic);
    authoring::tests::tap(&ctx, &mut app, "Place part")?;
    frame(&ctx, &mut app, vec![], none);
    authoring::tests::tap(&ctx, &mut app, "Keyword or part number")?;
    for letter in "resistor".chars() {
        frame(&ctx, &mut app, vec![egui::Event::Text(letter.into())], none);
    }
    assert!(
        paints_text(&ctx, &mut app, "Part sources are off")?,
        "with no source the list says sources are off"
    );
    assert!(
        !paints_text(&ctx, &mut app, "R_Small")?,
        "no remote parts appear while sources are off"
    );
    // The open library answers from its recordings: no network in a story.
    tap_id(&ctx, &mut app, "button:Turn on Open library")?;
    use_recorded_library(&mut app, "real-part")?;
    app.part_search = "resistor".into();
    assert!(
        waits_for_text(&ctx, &mut app, "R_Small")?,
        "the open library lists the parts that match the words: {}",
        app.message
    );
    let source = app
        .part_sources
        .open()
        .ok_or("the open library should be on")?;
    let found = pcbcraft_library::sources::PartSource::search(source.as_ref(), "resistor")
        .map_err(|e| e.to_string())?;
    let part = found
        .iter()
        .find(|p| p.mpn == "R")
        .ok_or("the library should know the part R")?;
    let before = shared
        .lock()
        .map_err(|_| "session")?
        .project
        .components
        .len();
    app.place_online_for_test(&ctx, part, "schematic", Point::new(20.0, 20.0));
    for _ in 0..100 {
        frame(&ctx, &mut app, vec![], none);
        let now = shared
            .lock()
            .map_err(|_| "session")?
            .project
            .components
            .len();
        if now > before {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let session = shared.lock().map_err(|_| "session")?;
    assert_eq!(
        session.project.components.len(),
        before + 1,
        "picking the found part puts it on the sheet: {}",
        app.message
    );
    Ok(())
}

#[test]
fn a_newcomer_finds_a_chip_package_by_name_and_places_it_on_the_board() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared.clone(), Preferences::default(), None, None);
    app.refresh();
    app.region().left_tab = "library".to_owned();
    let ctx = egui::Context::default();
    let none = egui::Modifiers::NONE;
    frame(&ctx, &mut app, vec![], none);
    app.change_tab(Tab::Schematic);
    // The open library answers from its recordings: no network in a story.
    use_recorded_library(&mut app, "chip-package")?;
    app.dock.view = library_dock::ViewKind::Footprints;
    // Typed the way people type it: dash, capitals, no size.
    app.part_search = "LQFP-64".into();
    assert!(
        waits_for_text(&ctx, &mut app, "LQFP-64")?,
        "the footprint list shows the 64-pin package: {}",
        app.message
    );
    let source = app
        .part_sources
        .open()
        .ok_or("the open library should be on")?;
    let found = source
        .search_cards(
            pcbcraft_library::open_library::CardScope::Footprints,
            "lqfp64",
        )
        .map_err(|e| e.to_string())?;
    let part = found
        .iter()
        .find(|p| p.mpn == "LQFP-64_10x10mm_P0.5mm")
        .ok_or("the library should know the package")?;
    app.place_online_for_test(&ctx, part, "board", Point::new(30.0, 30.0));
    for _ in 0..100 {
        frame(&ctx, &mut app, vec![], none);
        if !shared
            .lock()
            .map_err(|_| "session")?
            .project
            .board
            .footprints
            .is_empty()
        {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let session = shared.lock().map_err(|_| "session")?;
    let placed = session
        .project
        .board
        .footprints
        .first()
        .ok_or_else(|| format!("picking the package puts it on the board: {}", app.message))?;
    assert_eq!(placed.pads.len(), 64, "all 64 pads arrive");
    assert_eq!(placed.position, Point::new(30.0, 30.0));
    let component = session
        .project
        .components
        .iter()
        .find(|c| c.id == placed.component)
        .ok_or("the board part has a schematic part")?;
    let fp = component
        .library_footprint
        .as_ref()
        .ok_or("the part carries the package")?;
    assert_eq!(fp.definition.name, "LQFP-64_10x10mm_P0.5mm");
    assert_eq!(component.pins.len(), 64, "one schematic pin per pad");
    Ok(())
}

/// A footprint whose two pads sit on top of each other, saved into the
/// project library the way an import would.
fn app_with_overlapping_pads() -> Result<(App, egui::Context), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(
        shared,
        Preferences::default(),
        None,
        Some("footprint-editor"),
    );
    app.refresh();
    let Some(authoring::Draft::Footprint(mut def)) = app.library.editor.draft.clone() else {
        return Err("the footprint editor should start with a draft".to_string());
    };
    def.name = "SquashedPair".into();
    for pad in &mut def.pads {
        pad.position = Point::new(0.0, 0.0);
        pad.size = Point::new(2.0, 2.0);
    }
    app.run("library.footprint.upsert", json!({"definition": def}));
    let revision = app.revision;
    app.refresh_footprint_quality();
    assert_eq!(
        app.revision, revision,
        "checking a footprint must not change the design"
    );
    Ok((app, egui::Context::default()))
}

#[test]
fn a_newcomer_sees_a_library_finding_with_a_fix_on_the_checks_tab() -> Result<(), String> {
    let (mut app, ctx) = app_with_overlapping_pads()?;
    app.home_open = false;
    app.change_tab(Tab::Pcb);
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.change_tab(Tab::Checks);
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    for needle in ["SquashedPair: Two pads share the same copper area"] {
        assert!(
            scrolled_to_at(&ctx, &mut app, needle, 720.0)?,
            "the findings sheet should show \"{needle}\""
        );
    }
    Ok(())
}

#[test]
fn a_newcomer_sees_a_quality_badge_beside_each_project_footprint() -> Result<(), String> {
    let (mut app, _ctx) = app_with_overlapping_pads()?;
    assert!(
        app.library
            .quality
            .findings_for("SquashedPair")
            .iter()
            .any(|f| f.error && f.code == "lib_pad_overlap"),
        "the squashed footprint should be marked as having a mistake to fix"
    );
    let ctx = egui::Context::default();
    app.change_tab(Tab::Libraries);
    app.library.editor.mode = 0;
    let mut shown = false;
    for _ in 0..4 {
        shown = paints_text(&ctx, &mut app, "to fix")?;
        if shown {
            break;
        }
    }
    if !shown {
        for _ in 0..10 {
            frame(
                &ctx,
                &mut app,
                vec![
                    egui::Event::PointerMoved(egui::pos2(720.0, 500.0)),
                    egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Line,
                        delta: egui::vec2(0.0, -6.0),
                        phase: egui::TouchPhase::Move,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                egui::Modifiers::NONE,
            );
            if paints_text(&ctx, &mut app, "to fix")? {
                shown = true;
                break;
            }
        }
    }
    assert!(
        shown,
        "the project footprint list should show a badge reading \"to fix\""
    );
    Ok(())
}

#[test]
fn a_newcomer_draws_a_two_pin_symbol_with_the_mouse() -> Result<(), String> {
    let (shared, mut app, ctx) = authoring::tests::editor("symbol-editor")?;
    authoring::tests::clear_symbol_draft(&mut app);
    authoring::tests::tap(&ctx, &mut app, "Fit")?;
    authoring::tests::tap(&ctx, &mut app, "Pin")?;
    authoring::tests::library_world_click(&ctx, &mut app, Point::new(-2.0, 0.0))?;
    authoring::tests::library_world_click(&ctx, &mut app, Point::new(2.0, 0.0))?;
    authoring::tests::tap(&ctx, &mut app, "Draw rectangle")?;
    authoring::tests::library_world_click(&ctx, &mut app, Point::new(-1.0, -1.0))?;
    authoring::tests::library_world_click(&ctx, &mut app, Point::new(1.0, 1.0))?;
    let draft = authoring::tests::draft_symbol(&app)?;
    assert_eq!(draft.pins.len(), 2, "two pin clicks place two pins");
    assert!(
        draft
            .pins
            .iter()
            .any(|p| p.position == Point::new(-2.0, 0.0)),
        "pin one sits on the first grid click"
    );
    assert!(
        draft
            .pins
            .iter()
            .any(|p| p.position == Point::new(2.0, 0.0)),
        "pin two sits on the second grid click"
    );
    let rect = draft
        .graphics
        .iter()
        .find_map(|g| match &g.primitive {
            Graphic::Rectangle { start, end, .. } => Some((*start, *end)),
            _ => None,
        })
        .ok_or("rectangle body missing")?;
    assert_eq!(rect.0, Point::new(-1.0, -1.0));
    assert_eq!(rect.1, Point::new(1.0, 1.0));
    authoring::tests::tap(&ctx, &mut app, "Save symbol to project library")?;
    let session = shared.lock().map_err(|_| "session unavailable")?;
    assert_eq!(session.project.library.symbols.len(), 1);
    assert_eq!(session.project.library.symbols[0].pins.len(), 2);
    Ok(())
}

#[test]
fn a_newcomer_drags_a_pad_into_place() -> Result<(), String> {
    let (shared, mut app, ctx) = authoring::tests::editor("footprint-editor")?;
    authoring::tests::tap(&ctx, &mut app, "Fit")?;
    authoring::tests::tap(&ctx, &mut app, "Select")?;
    authoring::tests::tap(&ctx, &mut app, "Save footprint to project library")?;
    let before = app
        .library
        .editor
        .draft
        .as_ref()
        .and_then(|d| match d {
            Draft::Footprint(f) => f.pads.get(1).map(|p| p.position),
            _ => None,
        })
        .ok_or("pad 2 missing from the default footprint")?;
    authoring::tests::library_world_drag(
        &ctx,
        &mut app,
        before,
        before.offset(Point::new(1.0, 0.0)),
    )?;
    let after_drag = app
        .library
        .editor
        .draft
        .as_ref()
        .and_then(|d| match d {
            Draft::Footprint(f) => f.pads.get(1).map(|p| p.position),
            _ => None,
        })
        .ok_or("draft lost after drag")?;
    assert!(
        (after_drag.x - before.x - 1.0).abs() < 1e-6,
        "pad 2 moves one 1.0 mm step on the 0.5 mm grid"
    );
    authoring::tests::tap(&ctx, &mut app, "Save footprint to project library")?;
    {
        let session = shared.lock().map_err(|_| "session unavailable")?;
        let saved = &session.project.library.footprints[0].pads[1].position;
        assert!((saved.x - after_drag.x).abs() < 1e-6);
    }
    app.tool_call("undo", json!({}));
    let session = shared.lock().map_err(|_| "session unavailable")?;
    assert!(
        (session.project.library.footprints[0].pads[1].position.x - before.x).abs() < 1e-6,
        "undo puts pad 2 back where it started"
    );
    Ok(())
}

#[test]
fn library_editor_canvas_and_form_show_the_same_pin_position_after_drag() -> Result<(), String> {
    let (_shared, mut app, ctx) = authoring::tests::editor("symbol-editor")?;
    authoring::tests::tap(&ctx, &mut app, "Fit")?;
    authoring::tests::tap(&ctx, &mut app, "Select")?;
    let start = authoring::tests::draft_symbol(&app)?.pins[0].position;
    let target = canvas::snap(start.offset(Point::new(1.0, 0.5)));
    authoring::tests::library_world_drag(&ctx, &mut app, start, target)?;
    let draft_pos = authoring::tests::draft_symbol(&app)?.pins[0].position;
    assert_eq!(draft_pos, target, "drag updates the draft pin position");
    authoring::tests::tap(&ctx, &mut app, "Pin 1 · unit 1")?;
    authoring::tests::frame(&ctx, &mut app, vec![]);
    let shown = authoring::tests::draft_symbol(&app)?.pins[0].position;
    assert_eq!(
        shown, draft_pos,
        "the form reads the same coordinates the canvas wrote"
    );
    Ok(())
}

#[test]
fn a_newcomer_makes_a_qfn_from_its_datasheet_numbers() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(
        shared.clone(),
        Preferences::default(),
        None,
        Some("footprint-editor"),
    );
    app.refresh();
    let ctx = egui::Context::default();
    authoring::tests::tap(&ctx, &mut app, "Generate footprint")?;
    assert!(
        paints_text(
            &ctx,
            &mut app,
            "Type the numbers from the package drawing in the datasheet. Dimensions are millimetres."
        )?,
        "the form tells a newcomer what to type"
    );
    // The datasheet says: 0.5 mm pitch, 32 pins, 5 by 5 mm body.
    app.library.editor.wizard.family = "qfn";
    app.library.editor.wizard.name = "QFN32_5X5".into();
    app.library.editor.wizard.pitch = 0.5;
    app.library.editor.wizard.pin_count = 32;
    app.library.editor.wizard.body_length = 5.0;
    app.library.editor.wizard.body_width = 5.0;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        paints_text(&ctx, &mut app, "33 pads, with a thermal pad")?,
        "the preview counts the pads and shows the thermal pad"
    );
    authoring::tests::tap(&ctx, &mut app, "Create footprint")?;
    authoring::tests::tap(&ctx, &mut app, "Save footprint to project library")?;
    let session = shared.lock().map_err(|_| "session unavailable")?;
    let saved = session
        .project
        .library
        .footprints
        .iter()
        .find(|f| f.name == "QFN32_5X5")
        .ok_or("the library does not hold the new footprint")?;
    assert_eq!(saved.pads.len(), 33, "32 pins plus the thermal pad");
    assert!(saved.pads.iter().any(|p| p.number == "EP"));
    Ok(())
}

#[test]
fn a_newcomer_sees_why_impossible_numbers_cannot_make_a_footprint() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(
        shared,
        Preferences::default(),
        None,
        Some("footprint-editor"),
    );
    app.refresh();
    let ctx = egui::Context::default();
    authoring::tests::tap(&ctx, &mut app, "Generate footprint")?;
    app.library.editor.wizard.pitch = 40.0;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        paints_text(
            &ctx,
            &mut app,
            "pitch must be between 0.1 and 25.4 millimetres"
        )?,
        "a pitch that no package has is explained in plain words"
    );
    Ok(())
}

#[test]
fn a_newcomer_makes_a_chamfered_pad_and_then_a_custom_pad() -> Result<(), String> {
    let (shared, mut app, ctx) = authoring::tests::editor("footprint-editor")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Pad 1 · thru_hole")?;
    authoring::tests::tap(&ctx, &mut app, "circle")?;
    authoring::tests::tap(&ctx, &mut app, "Chamfered")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        paints_text(&ctx, &mut app, "Top left")?,
        "a chamfered pad offers its corners by name"
    );
    authoring::tests::tap(&ctx, &mut app, "Top right")?;
    let Some(authoring::Draft::Footprint(draft)) = app.library.editor.draft.clone() else {
        return Err("the footprint editor lost its draft".to_string());
    };
    let chamfer = draft.pads[0].chamfer.ok_or("the chamfer was not created")?;
    assert_eq!(draft.pads[0].shape, "chamfered");
    assert!(chamfer.corners.top_left && chamfer.corners.top_right);
    assert!(!chamfer.corners.bottom_left && !chamfer.corners.bottom_right);
    assert!(
        (chamfer.ratio - 0.25).abs() < 1e-9,
        "default cut is a quarter"
    );
    authoring::tests::tap(&ctx, &mut app, "Save footprint to project library")?;
    {
        let session = shared.lock().map_err(|_| "session unavailable")?;
        let saved = &session.project.library.footprints[0].pads[0];
        assert_eq!(saved.shape, "chamfered", "the cut pad reaches the project");
        assert_eq!(saved.chamfer, Some(chamfer));
    }
    authoring::tests::tap(&ctx, &mut app, "Chamfered")?;
    authoring::tests::tap(&ctx, &mut app, "Custom polygon")?;
    authoring::tests::tap(&ctx, &mut app, "Add point")?;
    let Some(authoring::Draft::Footprint(draft)) = app.library.editor.draft.clone() else {
        return Err("the footprint editor lost its draft".to_string());
    };
    assert_eq!(draft.pads[0].shape, "custom");
    assert!(draft.pads[0].chamfer.is_none(), "the cuts go away");
    assert_eq!(
        draft.pads[0].polygon.len(),
        5,
        "four corners plus the added point"
    );
    Ok(())
}

fn editor_app(page: &str) -> (App, egui::Context) {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, Some(page));
    app.refresh();
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    (app, ctx)
}

fn symbol_draft(app: &App) -> Result<pcbcraft_library::SymbolDefinition, String> {
    authoring::tests::draft_symbol(app)
}

#[test]
fn a_newcomer_describes_a_new_symbol_in_every_box() -> Result<(), String> {
    let (mut app, ctx) = editor_app("symbol-editor");
    type_into(&ctx, &mut app, "textinput:Name", "Amplifier")?;
    type_into(&ctx, &mut app, "textinput:Reference prefix", "U")?;
    type_into(&ctx, &mut app, "textinput:Default value", "LM358")?;
    type_into(&ctx, &mut app, "textinput:Description", "Dual op amp")?;
    type_into(&ctx, &mut app, "textinput:Suggested footprint", "SOIC-8")?;
    type_into(&ctx, &mut app, "textinput:Set name", "Analog")?;
    let draft = symbol_draft(&app)?;
    assert_eq!(draft.name, "Amplifier", "the name is the one typed");
    assert_eq!(
        draft.reference, "U",
        "the reference prefix is the one typed"
    );
    assert_eq!(draft.value, "LM358", "the default value is the one typed");
    assert_eq!(
        draft.description, "Dual op amp",
        "the note is the one typed"
    );
    assert_eq!(draft.footprint, "SOIC-8", "the package is the one typed");
    assert_eq!(draft.set, "Analog", "the set is the one typed");
    Ok(())
}

#[test]
fn a_newcomer_adds_pins_and_shapes_to_a_symbol_and_makes_a_second_unit() -> Result<(), String> {
    let (mut app, ctx) = editor_app("symbol-editor");
    let pins = symbol_draft(&app)?.pins.len();
    tap_id(&ctx, &mut app, "button:Add pin")?;
    assert_eq!(
        symbol_draft(&app)?.pins.len(),
        pins + 1,
        "one click, one pin"
    );
    let shapes = symbol_draft(&app)?.graphics.len();
    for (index, button) in [
        "button:Line",
        "button:Rectangle",
        "button:Circle",
        "button:Arc",
        "button:Polyline",
    ]
    .into_iter()
    .enumerate()
    {
        tap_id(&ctx, &mut app, button)?;
        assert_eq!(
            symbol_draft(&app)?.graphics.len(),
            shapes + index + 1,
            "{button} adds one shape to the drawing"
        );
    }
    // Opening a pin or a shape shows its own fields.
    tap_id(&ctx, &mut app, "button:Pin # · unit #")?;
    tap_id(&ctx, &mut app, "button:Graphic # · unit #")?;
    tap_id(&ctx, &mut app, "button:Duplicate preview unit")?;
    assert!(
        symbol_draft(&app)?.pins.iter().any(|pin| pin.unit == 2),
        "the copy lives in a second unit"
    );
    Ok(())
}

#[test]
fn a_newcomer_picks_a_drawing_tool_in_the_symbol_editor() -> Result<(), String> {
    use crate::authoring::LibraryTool;
    let (mut app, ctx) = editor_app("symbol-editor");
    for (button, tool) in [
        ("button:Draw line", LibraryTool::Line),
        ("button:Draw circle", LibraryTool::Circle),
        ("button:Draw arc", LibraryTool::Arc),
        ("button:Draw polyline", LibraryTool::Polyline),
    ] {
        tap_id(&ctx, &mut app, button)?;
        assert!(
            app.library.editor.tool == tool,
            "{button} arms its drawing tool"
        );
    }
    tap_id(&ctx, &mut app, "button:Fit")?;
    assert!(
        app.library.editor.tool == LibraryTool::Polyline,
        "Fit zooms the canvas and leaves the chosen tool alone"
    );
    let before = symbol_draft(&app)?.graphics.len();
    tap_id(&ctx, &mut app, "button:Text")?;
    assert_eq!(
        symbol_draft(&app)?.graphics.len(),
        before + 1,
        "Text adds one text label to the symbol"
    );
    Ok(())
}

#[test]
fn a_newcomer_lays_out_a_footprint_with_pads_and_a_drawing() -> Result<(), String> {
    use crate::authoring::LibraryTool;
    let (mut app, ctx) = editor_app("footprint-editor");
    let pads = match &app.library.editor.draft {
        Some(Draft::Footprint(draft)) => draft.pads.len(),
        _ => return Err("the footprint editor starts with a draft".into()),
    };
    tap_id(&ctx, &mut app, "button:Add pad")?;
    let after = match &app.library.editor.draft {
        Some(Draft::Footprint(draft)) => draft.pads.len(),
        _ => return Err("the footprint draft vanished".into()),
    };
    assert_eq!(after, pads + 1, "one click, one pad");
    tap_id(&ctx, &mut app, "button:Pad")?;
    assert!(
        app.library.editor.tool == LibraryTool::Pad,
        "Pad arms the pad tool"
    );
    tap_id(&ctx, &mut app, "button:Pad # · thru_hole")?;
    tap_id(&ctx, &mut app, "button:Line")?;
    tap_id(&ctx, &mut app, "button:Graphic #")?;
    let shapes = match &app.library.editor.draft {
        Some(Draft::Footprint(draft)) => draft.graphics.len(),
        _ => 0,
    };
    assert!(shapes > 0, "the footprint now has a drawn shape to open");
    Ok(())
}

#[test]
fn a_newcomer_switches_between_the_library_and_the_editor() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.region().left_tab = "library".to_owned();
    app.tool_call("sample_design", json!({}));
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.change_tab(Tab::Libraries);
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert_eq!(app.tab, Tab::Libraries, "the Libraries page stays open");
    tap_id(&ctx, &mut app, "button:Editor")?;
    assert_eq!(app.library.editor.mode, 2, "Editor opens the editor");
    tap_id(&ctx, &mut app, "button:Library")?;
    assert_eq!(app.library.editor.mode, 0, "Library comes back to browsing");
    tap_id(&ctx, &mut app, "button:New symbol")?;
    assert!(
        matches!(app.library.editor.draft, Some(Draft::Symbol(_))),
        "New symbol starts a symbol draft"
    );
    tap_id(&ctx, &mut app, "button:New footprint")?;
    assert!(
        matches!(app.library.editor.draft, Some(Draft::Footprint(_))),
        "New footprint starts a package draft"
    );
    app.library.editor.mode = 0;
    tap_id(&ctx, &mut app, "button:Original starter symbols")?;
    assert!(
        paints_text(&ctx, &mut app, "Place in schematic >")?,
        "the starter symbols open with a button to place each one"
    );
    Ok(())
}

#[test]
fn a_newcomer_browses_component_libraries_from_the_library_tab() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    let ctx = egui::Context::default();
    app.change_tab(Tab::Schematic);
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Place part")?;
    authoring::tests::tap(
        &ctx,
        &mut app,
        "Add or manage libraries in Settings \u{2192}",
    )?;
    assert_eq!(
        app.tab,
        Tab::Settings,
        "the browse button opens the page that lists library sources"
    );
    Ok(())
}
