//! Everyday editing stories: pick several objects, copy a corner, measure, clean up tracks.

use super::super::*;
use super::support::*;
use pcbcraft_doc::Component;

fn sample() -> App {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    app
}

fn two_parts(app: &App) -> Result<(Id, Id), String> {
    let mut parts: Vec<&Component> = app.project.components.iter().collect();
    parts.sort_by(|a, b| a.position.x.total_cmp(&b.position.x));
    match parts.as_slice() {
        [a, b, ..] => Ok((a.id, b.id)),
        _ => Err("the sample needs two parts".into()),
    }
}

#[test]
fn a_newcomer_copies_a_corner_of_the_board() -> Result<(), String> {
    let mut app = sample();
    app.tab = Tab::Schematic;
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let (first, second) = two_parts(&app)?;
    let (a, _) = app.bounds_of(first).ok_or("first part has no box")?;
    let (_, b) = app.bounds_of(second).ok_or("second part has no box")?;
    let before = app.project.components.len();
    // Drag a box around the corner holding two parts.
    app.begin_box(a, false);
    app.edit.box_to = Some(b);
    app.finish_box();
    let picked = app.selection();
    assert!(
        picked.contains(&first) && picked.contains(&second),
        "the box picks both parts: {}",
        app.message
    );
    frame(
        &ctx,
        &mut app,
        vec![egui::Event::Copy],
        egui::Modifiers::NONE,
    );
    let text = app.edit.clip.clone().ok_or("Ctrl+C should copy text")?;
    frame(
        &ctx,
        &mut app,
        vec![egui::Event::Paste(text)],
        egui::Modifiers::NONE,
    );
    assert_eq!(
        app.project.components.len(),
        before + picked.len(),
        "Ctrl+V adds one copy per picked part: {}",
        app.message
    );
    let copies = app.selection();
    assert_eq!(copies.len(), picked.len(), "the copies are the selection");
    assert!(copies.iter().all(|c| !picked.contains(c)));
    let first_copy = copies[0];
    let from = app
        .project
        .component(first_copy)
        .ok_or("copy missing")?
        .position;
    app.move_selection(3.0, 0.0);
    let to = app
        .project
        .component(first_copy)
        .ok_or("copy missing")?
        .position;
    assert!(
        (to.x - from.x - 3.0).abs() < 1e-9,
        "the copies move together"
    );
    assert_eq!(app.project.components.len(), before + picked.len());
    Ok(())
}

#[test]
fn a_newcomer_measures_between_two_holes() -> Result<(), String> {
    let mut app = sample();
    app.tab = Tab::Pcb;
    app.run(
        "board.hole_place",
        json!({"position":{"x":10.0,"y":10.0},"diameter":3.2}),
    );
    app.run(
        "board.hole_place",
        json!({"position":{"x":16.0,"y":18.0},"diameter":3.2}),
    );
    assert_eq!(app.project.board.holes.len(), 2, "{}", app.message);
    let ctx = egui::Context::default();
    let key = egui::Event::Key {
        key: egui::Key::U,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    };
    frame(&ctx, &mut app, vec![key], egui::Modifiers::NONE);
    assert!(matches!(app.tool, Tool::Measure), "U arms the measure tool");
    // Clicks land near the holes and snap onto their centres.
    app.canvas_click(Point::new(10.2, 9.9), 10.0);
    app.canvas_click(Point::new(15.9, 18.1), 10.0);
    let reading = app.measure_readout().ok_or("no reading")?;
    assert!(reading.contains("Distance 10.00 mm"), "{reading}");
    assert!(
        reading.contains("dx 6.00") && reading.contains("dy 8.00"),
        "{reading}"
    );
    assert_eq!(app.message, reading, "the status strip carries the reading");
    frame(
        &ctx,
        &mut app,
        vec![egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
        egui::Modifiers::NONE,
    );
    assert!(app.measure_readout().is_none(), "Esc clears the reading");
    assert!(matches!(app.tool, Tool::Select));
    Ok(())
}

#[test]
fn a_newcomer_extends_selection_with_shift_click() -> Result<(), String> {
    let mut app = sample();
    app.tab = Tab::Schematic;
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let (first, second) = two_parts(&app)?;
    let at = |app: &App, id: Id| app.project.component(id).map(|c| c.position);
    let p1 = at(&app, first).ok_or("first part missing")?;
    let p2 = at(&app, second).ok_or("second part missing")?;
    app.canvas_click(p1, 10.0);
    assert_eq!(app.selection(), vec![first]);
    app.shift_click(p2, 10.0);
    assert_eq!(app.selection(), vec![first, second], "{}", app.message);
    let before = app.project.components.len();
    app.duplicate_selection();
    assert_eq!(app.project.components.len(), before + 2, "{}", app.message);
    app.set_selection(vec![first, second]);
    app.shift_click(p2, 10.0);
    assert_eq!(
        app.selection(),
        vec![first],
        "shift-click again lets one go"
    );
    Ok(())
}

#[test]
fn a_newcomer_turns_off_parts_in_the_pick_chips() -> Result<(), String> {
    let mut app = sample();
    app.tab = Tab::Schematic;
    let (first, _) = two_parts(&app)?;
    let at = app.project.component(first).ok_or("part missing")?.position;
    app.edit.filter.parts = false;
    app.canvas_click(at, 10.0);
    assert!(app.selection().is_empty(), "parts are filtered out");
    app.edit.filter.parts = true;
    app.canvas_click(at, 10.0);
    assert_eq!(app.selection(), vec![first]);
    Ok(())
}

#[test]
fn a_newcomer_runs_track_cleanup_from_the_menu() -> Result<(), String> {
    let mut app = sample();
    app.tab = Tab::Pcb;
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Edit")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Clean up tracks")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        app.message.starts_with("Track cleanup"),
        "the menu reports a count: {}",
        app.message
    );
    Ok(())
}
