//! Newcomer stories for the pour settings panel, the pour report and the
//! keepout tool. The fixture is a ground zone with one plated pad inside it.

use super::super::*;
use super::support::*;

/// A board with a ground pour (5 to 35 mm) holding one plated pad R4.
/// `wall` adds a foreign track that splits the pour; `blocked` cuts holes
/// that starve the pad's spokes. Returns the app, window and zone id.
fn pour_app(wall: bool, blocked: bool) -> Result<(App, egui::Context, Id), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let zone;
    {
        let mut s = shared.lock().map_err(|_| "session lock unavailable")?;
        let id_of = |v: Result<Value, pcbcraft_engine::Error>| -> Result<Id, String> {
            v.map_err(|e| e.to_string())?["result"]["id"]
                .as_u64()
                .ok_or_else(|| "command returned no id".to_string())
        };
        let gnd = id_of(s.execute("net.create", json!({"name": "GND"}), None))?;
        let sig = id_of(s.execute("net.create", json!({"name": "SIG"}), None))?;
        let square = |a: f64, b: f64, c: f64, d: f64| json!([{"x":a,"y":b},{"x":c,"y":b},{"x":c,"y":d},{"x":a,"y":d},{"x":a,"y":b}]);
        zone = id_of(s.execute(
            "zone.place",
            json!({"net": gnd, "outline": square(5.0, 5.0, 35.0, 35.0)}),
            None,
        ))?;
        if wall {
            id_of(s.execute(
                "track.add",
                json!({"net": sig, "start": {"x":25.0,"y":4.0}, "end": {"x":25.0,"y":36.0}, "width": 1.0}),
                None,
            ))?;
        }
        if blocked {
            for outline in [square(20.3, 5.0, 35.0, 35.0), square(5.0, 20.3, 20.3, 35.0)] {
                s.execute(
                    "zone.cutout",
                    json!({"zone": zone, "outline": outline}),
                    None,
                )
                .map_err(|e| e.to_string())?;
            }
        }
        let at = if wall {
            Point::new(12.0, 20.0)
        } else {
            Point::new(20.0, 20.0)
        };
        let mut p = s.project.clone();
        let mut part = pcbcraft_doc::component_template("resistor", 4, "R4".into(), at)
            .ok_or("resistor template")?;
        part.id = p.ids().max().unwrap_or(0) + 1;
        part.pins.truncate(1);
        for pin in &mut part.pins {
            pin.net = Some(gnd);
        }
        let part_id = part.id;
        let pad = pcbcraft_library::FootprintPad {
            number: "1".into(),
            pad_type: "thru_hole".into(),
            shape: "circle".into(),
            position: Point::default(),
            rotation: 0.0,
            size: Point::new(1.6, 1.6),
            layers: vec!["*.Cu".into(), "*.Mask".into()],
            drill: Some(pcbcraft_library::Drill {
                size: Point::new(0.8, 0.8),
                offset: Point::default(),
            }),
            roundrect_ratio: None,
            chamfer: None,
            polygon: vec![],
            raw_source: String::new(),
        };
        part.library_footprint = Some(pcbcraft_doc::LibraryFootprint {
            library: "Test".into(),
            definition: pcbcraft_library::FootprintDefinition {
                name: "PTH".into(),
                description: String::new(),
                keywords: String::new(),
                pads: vec![pad.clone()],
                graphics: vec![],
                model: None,
                rules: None,
                box_body: None,
                raw_source: String::new(),
                unsupported: vec![],
            },
        });
        p.components.push(part);
        p.board.footprints.push(pcbcraft_doc::Footprint {
            id: part_id + 1,
            component: part_id,
            position: at,
            rotation: 0.0,
            locked: None,
            side: None,
            pads: vec![pcbcraft_doc::Pad {
                number: "1".into(),
                offset: Point::default(),
                diameter: 1.6,
                net: Some(gnd),
                castellated: None,
                geometry: Some(pad),
            }],
        });
        s.replace(p).map_err(|e| e.to_string())?;
    }
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.change_tab(Tab::Pcb);
    app.refresh();
    app.select(Some(zone));
    Ok((app, egui::Context::default(), zone))
}

fn settle(ctx: &egui::Context, app: &mut App) {
    frame(ctx, app, vec![], egui::Modifiers::NONE);
    frame(ctx, app, vec![], egui::Modifiers::NONE);
}

/// Pick Solid for pads (the same value the button sets), then press Apply.
fn choose_pads_solid(ctx: &egui::Context, app: &mut App) -> Result<(), String> {
    if let Some((_, _, draft)) = app.pours.draft.as_mut() {
        draft.pads = pcbcraft_doc::ZoneConnect::Solid;
    }
    authoring::tests::tap(ctx, app, "Apply pour settings")?;
    settle(ctx, app);
    Ok(())
}

#[test]
fn a_newcomer_pours_a_ground_plane_with_thermal_spokes() -> Result<(), String> {
    let (mut app, ctx, _) = pour_app(false, false)?;
    settle(&ctx, &mut app);
    let r0 = app.revision;
    for _ in 0..20 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    assert_eq!(
        app.revision, r0,
        "showing the report never edits the design"
    );
    assert!(
        scrolled_to(&ctx, &mut app, "Pour settings")?,
        "the panel shows for a zone"
    );
    assert!(
        scrolled_to(&ctx, &mut app, "Thermal spokes reaching pads: 4")?,
        "four spokes by default: {:?}",
        app.pours.report
    );
    choose_pads_solid(&ctx, &mut app)?;
    assert!(
        scrolled_to(&ctx, &mut app, "No thermal spokes")?,
        "solid pads need no spokes: {} {:?} {:?} {:?}",
        app.message,
        app.pours.report,
        app.project.board.zones.first().map(|z| z.pour_settings()),
        app.pours.draft
    );
    Ok(())
}

#[test]
fn a_newcomer_removes_stray_copper_from_a_pour() -> Result<(), String> {
    let (mut app, ctx, zone) = pour_app(true, false)?;
    app.run("zone.pour", json!({"id": zone, "islands": "keep"}));
    settle(&ctx, &mut app);
    assert!(
        scrolled_to(&ctx, &mut app, "Detached copper found: 1. Removed: 0")?,
        "the report names the stray piece"
    );
    if let Some((_, _, draft)) = app.pours.draft.as_mut() {
        draft.islands = pcbcraft_doc::ZoneIslands::Remove;
    }
    authoring::tests::tap(&ctx, &mut app, "Apply pour settings")?;
    settle(&ctx, &mut app);
    assert!(
        scrolled_to(&ctx, &mut app, "Removed: 1")?,
        "the piece is gone: {} {:?}",
        app.message,
        app.pours.report
    );
    Ok(())
}

#[test]
fn a_newcomer_sees_a_starved_pad_and_how_to_fix_it() -> Result<(), String> {
    let (mut app, ctx, _) = pour_app(false, true)?;
    settle(&ctx, &mut app);
    assert!(
        scrolled_to(&ctx, &mut app, "Pad R4 is starved")?,
        "the report names the pad: {:?}",
        app.pours.report
    );
    assert!(
        scrolled_to(&ctx, &mut app, "Raise the relief gap")?,
        "and says what to change"
    );
    choose_pads_solid(&ctx, &mut app)?;
    assert!(
        !scrolled_to(&ctx, &mut app, "is starved")?,
        "a solid join leaves nothing starved"
    );
    Ok(())
}

#[test]
fn a_newcomer_draws_a_keepout_and_sees_the_track_flagged() -> Result<(), String> {
    let (mut app, ctx, _) = pour_app(false, false)?;
    app.select(None);
    app.change_tab(Tab::Rules);
    settle(&ctx, &mut app);
    tap_id(&ctx, &mut app, "button:Draw keepout")?;
    assert!(
        matches!(app.tool, Tool::Keepout),
        "the button arms the tool"
    );
    assert!(matches!(app.tab, Tab::Pcb), "and takes you to the board");
    settle(&ctx, &mut app);
    let pointer = |p: egui::Pos2, pressed: Option<bool>| {
        let mut events = vec![egui::Event::PointerMoved(p)];
        if let Some(pressed) = pressed {
            events.push(egui::Event::PointerButton {
                pos: p,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
        }
        events
    };
    frame(
        &ctx,
        &mut app,
        pointer(egui::pos2(600.0, 400.0), Some(true)),
        egui::Modifiers::NONE,
    );
    frame(
        &ctx,
        &mut app,
        pointer(egui::pos2(700.0, 460.0), None),
        egui::Modifiers::NONE,
    );
    frame(
        &ctx,
        &mut app,
        pointer(egui::pos2(800.0, 520.0), None),
        egui::Modifiers::NONE,
    );
    frame(
        &ctx,
        &mut app,
        pointer(egui::pos2(800.0, 520.0), Some(false)),
        egui::Modifiers::NONE,
    );
    settle(&ctx, &mut app);
    app.refresh();
    let area = app
        .project
        .board
        .rule_areas
        .first()
        .ok_or_else(|| format!("a drag should add an area: {}", app.message))?
        .clone();
    assert!(area.keepout.is_some(), "the new area is a keepout");
    assert!(
        matches!(app.tool, Tool::Select),
        "the tool returns to Select"
    );
    let net = app
        .project
        .nets
        .iter()
        .find(|n| n.name == "GND")
        .ok_or("net")?
        .id;
    let (cx, cy) = (
        area.region.x + area.region.width / 2.0,
        area.region.y + area.region.height / 2.0,
    );
    app.run(
        "track.add",
        json!({"net": net, "start": {"x": cx - 0.5, "y": cy}, "end": {"x": cx + 0.5, "y": cy}, "width": 0.2}),
    );
    let report = pcbcraft_rules::drc(&app.project);
    assert!(
        report.iter().any(|f| f.code == "keepout"),
        "a track inside the keepout is a finding"
    );
    app.tool_call("undo", json!({}));
    app.tool_call("undo", json!({}));
    app.refresh();
    assert!(
        app.project.board.rule_areas.is_empty(),
        "undo removes the keepout"
    );
    Ok(())
}

#[test]
fn a_newcomer_tries_a_bad_pour_number() -> Result<(), String> {
    let (mut app, _ctx, zone) = pour_app(false, false)?;
    let before = app
        .project
        .board
        .zones
        .iter()
        .find(|z| z.id == zone)
        .map(|z| z.pour_settings());
    app.run("zone.pour", json!({"id": zone, "relief_gap": 9.0}));
    let after = app
        .project
        .board
        .zones
        .iter()
        .find(|z| z.id == zone)
        .map(|z| z.pour_settings());
    assert_eq!(before, after, "a refused number changes nothing");
    assert!(
        app.message.contains("relief gap"),
        "the engine sentence shows: {}",
        app.message
    );
    let mut draft = pour_panel::PourDraft::from_zone(
        app.project
            .board
            .zones
            .iter()
            .find(|z| z.id == zone)
            .ok_or("zone")?,
    );
    draft.gap = 9.0;
    assert_eq!(
        draft.params(zone)["relief_gap"],
        json!(9.0),
        "the panel range clamps before this point"
    );
    Ok(())
}

#[test]
fn pour_report_lines_turns_a_zone_fill_result_into_plain_sentences() {
    let name = |id: Id| format!("P{id}");
    let fill = json!({"area": 12.34, "pieces": 2, "spokes": 4, "starved": [7], "islands": 1, "removed": 1, "unconnected": true});
    let lines = pour_panel::pour_report_lines(&fill, &name);
    let text: Vec<&str> = lines.iter().map(|(t, _)| t.as_str()).collect();
    assert!(text[0].contains("12.3"));
    assert!(text.iter().any(|t| t.contains("spokes reaching pads: 4")));
    assert!(text.iter().any(|t| t.contains("Pad P7 is starved")));
    assert!(
        text.iter()
            .any(|t| t.contains("Detached copper found: 1. Removed: 1"))
    );
    assert!(text.iter().any(|t| t.contains("kept whole")));
    let calm = pour_panel::pour_report_lines(
        &json!({"area": 1.0, "pieces": 1, "spokes": 0, "starved": [], "islands": 0, "removed": 0, "unconnected": false}),
        &name,
    );
    assert!(calm.iter().all(|(_, tone)| *tone == pour_panel::Tone::Note));
    assert!(calm.iter().any(|(t, _)| t.starts_with("No thermal spokes")));
    assert!(pour_panel::pour_report_lines(&Value::Null, &name).is_empty());
}

#[test]
fn a_newcomer_ticks_keep_all_copper_out_and_loses_the_gap_field() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    let ctx = egui::Context::default();
    authoring::tests::open_page(&ctx, &mut app, "Rules")?;
    settle(&ctx, &mut app);
    assert!(
        scrolled_to_at(&ctx, &mut app, "Copper stays apart", 700.0)?,
        "the gap field shows at first"
    );
    authoring::tests::tap(&ctx, &mut app, "Keep all copper out")?;
    settle(&ctx, &mut app);
    assert!(
        scrolled_to_at(&ctx, &mut app, "This layer only", 700.0)?,
        "the layer limit appears"
    );
    assert!(
        !scrolled_to(&ctx, &mut app, "Copper stays apart")?,
        "ticking it hides the gap field"
    );
    Ok(())
}
