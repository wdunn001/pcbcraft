use super::super::*;
use super::support::*;

#[test]
fn a_newcomer_copies_a_part_with_one_click() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    let resistor = app
        .project
        .components
        .iter()
        .find(|c| c.reference.starts_with('R'))
        .map(|c| c.id)
        .ok_or("the practice design should carry a resistor")?;
    app.select(Some(resistor));
    app.refresh();
    let refs: Vec<String> = app
        .project
        .components
        .iter()
        .map(|c| c.reference.clone())
        .collect();
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        scrolled_to(&ctx, &mut app, "Duplicate")?,
        "a chosen part offers to be copied: {}",
        app.message
    );
    authoring::tests::tap(&ctx, &mut app, "Duplicate")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.refresh();
    let after: Vec<String> = app
        .project
        .components
        .iter()
        .map(|c| c.reference.clone())
        .collect();
    assert_eq!(
        after.len(),
        refs.len() + 1,
        "one more part sits on the board: {refs:?} -> {after:?}"
    );
    assert_eq!(
        after.iter().filter(|r| r.starts_with('R')).count(),
        refs.iter().filter(|r| r.starts_with('R')).count() + 1,
        "the copy got its own new reference: {after:?}"
    );
    Ok(())
}
#[test]
fn a_newcomer_freezes_a_part_so_nothing_moves_it() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    app.change_tab(Tab::Pcb);
    app.select(Some(10));
    app.refresh();
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    // Lock is a Manufacturing toggle in the Properties of a chosen part.
    tap_properties(&ctx, &mut app, "checkbox:Lock in place")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.refresh();
    let frozen = |app: &App| {
        app.project
            .board
            .footprints
            .iter()
            .find(|f| f.id == 10)
            .and_then(|f| f.locked)
            == Some(true)
    };
    assert!(frozen(&app), "one click freezes the part: {}", app.message);
    assert!(
        !app.tool_call(
            "footprint.move",
            json!({"id":10,"position":{"x":20.0,"y":20.0}})
        ),
        "a frozen part refuses to be shoved: {}",
        app.message
    );
    tap_properties(&ctx, &mut app, "checkbox:Lock in place")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.refresh();
    assert!(
        !frozen(&app),
        "unlocking lets the part move freely again: {}",
        app.message
    );
    Ok(())
}
#[test]
fn a_newcomer_docks_a_part_flush_to_the_board_edge() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    app.select(Some(10));
    app.refresh();
    let before = app
        .project
        .board
        .footprints
        .iter()
        .find(|f| f.id == 10)
        .map(|f| f.position.x)
        .ok_or("the practice board should carry J1's footprint")?;
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        scrolled_to(&ctx, &mut app, "Dock left")?,
        "a chosen footprint offers to sit flush to an edge: {}",
        app.message
    );
    authoring::tests::tap(&ctx, &mut app, "Dock left")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.refresh();
    let after = app
        .project
        .board
        .footprints
        .iter()
        .find(|f| f.id == 10)
        .map(|f| f.position.x)
        .ok_or("the practice board should still carry J1's footprint")?;
    assert!(
        after < before && after > 0.0,
        "the part slid in to kiss the left edge: {before} -> {after}"
    );
    Ok(())
}
#[test]
fn a_newcomer_smooths_a_corner_for_the_etcher() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    app.tool_call(
        "track.route",
        json!({"from":{"footprint":10,"pad":"1"},"to":{"footprint":11,"pad":"1"},"layer":"F.Cu"}),
    );
    let (id, middle, before) = app
        .project
        .board
        .tracks
        .first()
        .map(|t| {
            (
                t.id,
                Point::new((t.start.x + t.end.x) / 2.0, (t.start.y + t.end.y) / 2.0),
                app.project.board.tracks.len(),
            )
        })
        .ok_or("routing should leave a corner to smooth")?;
    app.change_tab(Tab::Pcb);
    app.canvas_click(middle, 15.0);
    assert_eq!(
        app.selected,
        Some(id),
        "the first leg of the route is picked up: {}",
        app.message
    );
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        scrolled_to(&ctx, &mut app, "Cut corner")?,
        "a wire that meets another offers a corner cut: {}",
        app.message
    );
    authoring::tests::tap(&ctx, &mut app, "Cut corner")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.refresh();
    assert!(
        app.project.board.tracks.len() > before,
        "the cut adds a diagonal so the run turns gently: {}",
        app.message
    );
    Ok(())
}
#[test]
fn a_newcomer_lengthens_a_wire_to_match_its_partner() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    app.tool_call(
        "track.route",
        json!({"from":{"footprint":10,"pad":"1"},"to":{"footprint":11,"pad":"1"},"layer":"F.Cu"}),
    );
    app.refresh();
    let (id, middle) = app
        .project
        .board
        .tracks
        .first()
        .map(|t| {
            (
                t.id,
                Point::new((t.start.x + t.end.x) / 2.0, (t.start.y + t.end.y) / 2.0),
            )
        })
        .ok_or("routing should leave a wire to tune")?;
    app.change_tab(Tab::Pcb);
    app.canvas_click(middle, 15.0);
    assert_eq!(
        app.selected,
        Some(id),
        "the wire is picked up: {}",
        app.message
    );
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        scrolled_to_at(&ctx, &mut app, "Suggested width", 1280.0)?,
        "the side panel suggests a width for the target impedance: {}",
        app.message
    );
    assert!(
        scrolled_to_at(&ctx, &mut app, "Tune length", 1280.0)?,
        "the side panel offers Tune length: {}",
        app.message
    );
    let before: f64 = app.project.board.tracks.iter().map(track_length).sum();
    app.impedance_ui.tune_target = before + 6.0;
    scrolled_to_at(&ctx, &mut app, "Tune length", 1280.0)?;
    authoring::tests::tap(&ctx, &mut app, "Tune length")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.refresh();
    let after: f64 = app.project.board.tracks.iter().map(track_length).sum();
    assert!(
        after > before + 5.0,
        "the zigzag adds length: {before} -> {after}: {}",
        app.message
    );
    assert!(
        scrolled_to_at(&ctx, &mut app, "Before", 1280.0)?,
        "the panel reports the length before and after: {}",
        app.message
    );
    Ok(())
}

/// One frame at an explicit window size (`support::frame` is fixed at 1440 by 940).
fn frame_sized(
    ctx: &egui::Context,
    app: &mut App,
    size: Vec2,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, size)),
            events,
            ..Default::default()
        },
        |ui| app.show(ui),
    );
    output.textures_delta.clear();
    output
}

/// Words painted this frame that spill past the window's right edge or past
/// the clip that was meant to contain them, as `"words" right-edge`.
fn spilled_text(output: &egui::FullOutput, width: f32) -> Vec<String> {
    let mut spilled = Vec::new();
    for clipped in &output.shapes {
        if let egui::Shape::Text(text) = &clipped.shape {
            let rect = text.galley.rect.translate(text.pos.to_vec2());
            let edge = width.min(clipped.clip_rect.max.x);
            if rect.max.x > edge + 1.0 && rect.min.x < width {
                let words: String = text.galley.text().chars().take(30).collect();
                spilled.push(format!("\"{words}\" ends at {:.0}", rect.max.x));
            }
        }
    }
    spilled.sort();
    spilled.dedup();
    spilled
}

/// Text that spills past a 320 point window today (issue #647 report); none is left. The
/// walk fails on a new spill and on a listed one that no longer happens, so
/// the list shrinks as the layout is fixed.
const KNOWN_REFLOW_DEFECTS: [&str; 0] = [];

#[test]
fn a_newcomer_zooms_to_four_hundred_percent_and_every_dock_still_fits() -> Result<(), String> {
    let size = Vec2::new(320.0, 640.0);
    let mut walked = Vec::new();
    let mut defects = Vec::new();
    for tab in [
        Tab::Files,
        Tab::Schematic,
        Tab::Pcb,
        Tab::Board3d,
        Tab::Rules,
        Tab::Settings,
    ] {
        let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
        let mut app = App::create(shared, Preferences::default(), None, None);
        app.refresh();
        app.tool_call("sample_design", json!({}));
        app.change_tab(tab);
        let ctx = egui::Context::default();
        let mut out = frame_sized(&ctx, &mut app, size, vec![]);
        for _ in 0..2 {
            out = frame_sized(&ctx, &mut app, size, vec![]);
        }
        walked.push(format!("{tab:?}"));
        for line in spilled_text(&out, size.x) {
            defects.push(format!("{tab:?}: {line}"));
        }
    }
    let (new, fixed): (Vec<&String>, Vec<&str>) = (
        defects
            .iter()
            .filter(|d| !KNOWN_REFLOW_DEFECTS.contains(&d.as_str()))
            .collect(),
        KNOWN_REFLOW_DEFECTS
            .iter()
            .copied()
            .filter(|k| !defects.iter().any(|d| d == k))
            .collect(),
    );
    assert!(
        new.is_empty() && fixed.is_empty(),
        "walked {walked:?}; new spills: {new:?}; fixed, so delete from KNOWN_REFLOW_DEFECTS: {fixed:?}"
    );
    Ok(())
}

/// Every start screen tile keeps its title on one line and the tiles share one
/// width, in a huge browser window and in a small one, at every zoom.
///
/// Why: in a 2000 by 1100 browser window the tile titles wrapped one letter per
/// line because the description sat beside the title in the same button.
#[test]
fn a_newcomer_reads_every_start_tile_on_one_line_at_any_window_size() -> Result<(), String> {
    let titles = [
        "New design",
        "Design setup wizard",
        "Worked example",
        "Showcase board",
        "Open a design file",
    ];
    // Window size in physical pixels, then the zoom it is drawn at.
    let sizes = [
        (2000.0_f32, 1100.0_f32),
        (1280.0, 800.0),
        (800.0, 600.0),
        (480.0, 800.0),
    ];
    for (px_w, px_h) in sizes {
        for scale in [0.75_f32, 1.0, 1.5, 2.0, 3.0] {
            let (w, h) = (px_w / scale, px_h / scale);
            let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
            let mut app = App::create(shared, Preferences::default(), None, None);
            app.refresh();
            app.home_open = true;
            let ctx = egui::Context::default();
            ctx.set_pixels_per_point(scale);
            let mut out = frame_sized(&ctx, &mut app, Vec2::new(w, h), vec![]);
            for _ in 0..3 {
                out = frame_sized(&ctx, &mut app, Vec2::new(w, h), vec![]);
            }
            let at = format!("{px_w}x{px_h} at {scale}x");
            let rects: Vec<Rect> = ctx
                .data(|d| d.get_temp(start_tile_key()))
                .unwrap_or_default();
            assert_eq!(rects.len(), 5, "{at}: five start tiles are drawn");
            for (title, tile) in titles.into_iter().zip(&rects) {
                if tile.min.y >= h {
                    // Scrolled below the fold: the page scrolls to it.
                    continue;
                }
                let galley = out
                    .shapes
                    .iter()
                    .find_map(|s| match &s.shape {
                        egui::Shape::Text(t)
                            if t.galley.text().trim_end_matches(['.', '\u{2026}']) == title =>
                        {
                            Some(t.galley.clone())
                        }
                        _ => None,
                    })
                    .ok_or(format!("{at}: tile title {title:?} is not painted"))?;
                if galley.rows.len() != 1 {
                    // A genuinely narrow window may break between words, never
                    // inside one.
                    assert!(
                        title.split_whitespace().count() > 1,
                        "{at}: {title:?} wraps inside a word"
                    );
                    assert!(w < 420.0, "{at}: {title:?} wraps in a window this wide");
                }
            }
            for r in &rects {
                assert!(
                    (r.width() - rects[0].width()).abs() < 0.5,
                    "{at}: tiles differ in width: {rects:?}"
                );
                assert!(
                    r.min.x >= -0.5 && r.max.x <= w + 0.5,
                    "{at}: a tile leaves the {w} point window: {r:?}"
                );
                assert!(
                    (r.center().x - w / 2.0).abs() < 1.0,
                    "{at}: tiles are centred: {r:?}"
                );
            }
        }
    }
    Ok(())
}

/// R1 and R2 placed far left of a locked connector J1 they are wired to.
/// Returns the session and the footprint ids (r1, r2, j1).
fn arrange_session() -> Result<(pcbcraft_engine::Session, [u64; 3]), String> {
    let mut s = pcbcraft_engine::Session::default();
    s.project.auto_sync = false;
    let id = |v: Value| -> Result<u64, String> {
        v["result"]["id"].as_u64().ok_or_else(|| "no id".to_owned())
    };
    let mut add = |kind: &str, reference: &str| -> Result<u64, String> {
        let v = s
            .execute(
                "component.add",
                json!({"kind":kind,"reference":reference,"value":"v","position":{"x":10,"y":10}}),
                None,
            )
            .map_err(|e| e.to_string())?;
        id(v)
    };
    let (r1, r2, j1) = (
        add("resistor", "R1")?,
        add("resistor", "R2")?,
        add("connector", "J1")?,
    );
    for (name, a, ap, b, bp) in [
        ("N1", r1, "1", j1, "1"),
        ("N2", r2, "1", j1, "2"),
        ("N3", r1, "2", r2, "2"),
    ] {
        let net = s
            .execute("net.create", json!({"name":name}), None)
            .map_err(|e| e.to_string())
            .and_then(id)?;
        s.execute(
            "wire.connect",
            json!({"net":net,"from":{"component":a,"pin":ap},"to":{"component":b,"pin":bp}}),
            None,
        )
        .map_err(|e| e.to_string())?;
    }
    s.execute("board.sync", json!({}), None)
        .map_err(|e| e.to_string())?;
    let mut ids = [0_u64; 3];
    for (slot, component) in [r1, r2, j1].into_iter().enumerate() {
        ids[slot] = s
            .project
            .board
            .footprints
            .iter()
            .find(|f| f.component == component)
            .map(|f| f.id)
            .ok_or("a part has no footprint")?;
    }
    for (fp, x, y) in [
        (ids[0], 14.0, 14.0),
        (ids[1], 14.0, 28.0),
        (ids[2], 44.0, 21.0),
    ] {
        s.execute(
            "footprint.move",
            json!({"id":fp,"position":{"x":x,"y":y}}),
            None,
        )
        .map_err(|e| e.to_string())?;
    }
    s.execute(
        "footprint.set_locked",
        json!({"id":ids[2],"locked":true}),
        None,
    )
    .map_err(|e| e.to_string())?;
    Ok((s, ids))
}

#[test]
fn a_newcomer_lets_the_app_arrange_the_unlocked_parts() -> Result<(), String> {
    let (session, ids) = arrange_session()?;
    let shared = pcbcraft_mcp::shared(session);
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.change_tab(Tab::Pcb);
    let ctx = egui::Context::default();
    let places = |app: &App| -> Vec<(u64, f64, f64, f64)> {
        app.project
            .board
            .footprints
            .iter()
            .map(|f| (f.id, f.position.x, f.position.y, f.rotation))
            .collect()
    };
    let errors = |app: &App| -> usize {
        pcbcraft_rules::drc(&app.project)
            .iter()
            .filter(|f| f.severity == "error")
            .count()
    };
    let at = |list: &[(u64, f64, f64, f64)], id: u64| list.iter().copied().find(|p| p.0 == id);
    let before = places(&app);
    let errors_before = errors(&app);
    tap_strip(&ctx, &mut app, "icon:tool-arrange")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.refresh();
    let after = places(&app);
    assert_eq!(
        at(&before, ids[2]),
        at(&after, ids[2]),
        "the locked connector did not move: {}",
        app.message
    );
    let moved =
        at(&before, ids[0]) != at(&after, ids[0]) || at(&before, ids[1]) != at(&after, ids[1]);
    if moved {
        assert!(
            app.message.starts_with("Arranged"),
            "the status line says how many parts moved: {}",
            app.message
        );
    } else {
        assert!(
            app.message.contains("nothing moved") || app.message.contains("no unlocked"),
            "when nothing moves the status line says why: {}",
            app.message
        );
    }
    assert!(
        moved,
        "the resistors are far from their connector: {}",
        app.message
    );
    assert!(
        errors(&app) <= errors_before,
        "arranging added no new error: {} -> {}",
        errors_before,
        errors(&app)
    );
    assert!(
        app.tool_call("undo", json!({})),
        "one undo is available: {}",
        app.message
    );
    assert_eq!(
        places(&app),
        before,
        "undo puts every part back where it was"
    );
    Ok(())
}
