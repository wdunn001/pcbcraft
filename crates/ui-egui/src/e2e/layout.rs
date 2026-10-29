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
    assert!(
        scrolled_to(&ctx, &mut app, "Lock in place")?,
        "a chosen footprint offers to be frozen: {}",
        app.message
    );
    authoring::tests::tap(&ctx, &mut app, "Lock in place")?;
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
    authoring::tests::tap(&ctx, &mut app, "Unlock")?;
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
        scrolled_to(&ctx, &mut app, "Suggested width")?,
        "the side panel suggests a width for the target impedance: {}",
        app.message
    );
    assert!(
        scrolled_to(&ctx, &mut app, "Tune length")?,
        "the side panel offers Tune length: {}",
        app.message
    );
    let before: f64 = app.project.board.tracks.iter().map(track_length).sum();
    app.impedance_ui.tune_target = before + 6.0;
    scrolled_to(&ctx, &mut app, "Tune length")?;
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
        paints_text(&ctx, &mut app, "Before")?,
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

/// Text that spills past a 320 point window today (issue #647 report). The
/// walk fails on a new spill and on a listed one that no longer happens, so
/// the list shrinks as the layout is fixed.
const KNOWN_REFLOW_DEFECTS: [&str; 12] = [
    "Files: \"Manufacture\" ends at 355",
    "Schematic: \"Manufacture\" ends at 355",
    "Pcb: \"60 × 40 mm    Top / Bottom    \" ends at 355",
    "Pcb: \"Manufacture\" ends at 355",
    "Board3d: \"Manufacture\" ends at 355",
    "Rules: \"Manufacture\" ends at 355",
    "Rules: \"Rules only you know about: pic\" ends at 327",
    "Rules: \"Stem-shaped copper where wires\" ends at 324",
    "Checks: \"Manufacture\" ends at 355",
    "Checks: \"Whether the wiring makes elect\" ends at 326",
    "Settings: \"Manufacture\" ends at 355",
    "Settings: \"Nothing recorded yet: this boa\" ends at 325",
];

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
        Tab::Checks,
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
