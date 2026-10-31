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

/// Four resistors placed by hand and synced to the board, at staggered heights.
fn four_board_parts() -> Result<(App, Vec<Id>), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    for (i, y) in [10.0, 14.0, 8.0, 12.0].into_iter().enumerate() {
        app.run(
            "component.add",
            json!({"kind":"resistor","position":{"x":10.0 + 12.0 * i as f64,"y":y}}),
        );
    }
    app.run("board.sync", json!({}));
    app.change_tab(Tab::Pcb);
    let ids: Vec<Id> = app.project.board.footprints.iter().map(|f| f.id).collect();
    if ids.len() < 4 {
        return Err(format!("the board needs four parts: {}", app.message));
    }
    Ok((app, ids))
}

#[test]
fn a_newcomer_lines_up_four_connectors() -> Result<(), String> {
    let (mut app, ids) = four_board_parts()?;
    let ys = |app: &App| -> Vec<f64> {
        app.project
            .board
            .footprints
            .iter()
            .map(|f| f.position.y)
            .collect()
    };
    let before = ys(&app);
    app.set_selection(ids);
    app.align_selection("top");
    let after = ys(&app);
    assert!(
        after.windows(2).all(|w| (w[0] - w[1]).abs() < 1e-9),
        "every part sits at one height: {after:?} ({})",
        app.message
    );
    app.tool_call("undo", json!({}));
    assert_eq!(ys(&app), before, "one undo restores every position");
    Ok(())
}

#[test]
fn a_newcomer_makes_a_row_of_led_footprints_with_an_array() -> Result<(), String> {
    let (mut app, ids) = four_board_parts()?;
    let before = app.project.board.footprints.len();
    app.set_selection(vec![ids[0]]);
    app.open_array_dialog();
    assert!(app.edit.arrange.open, "the array dialog opens");
    app.edit.arrange.rows = 1;
    app.edit.arrange.columns = 5;
    app.edit.arrange.pitch_x = 6.0;
    app.run_array();
    assert_eq!(
        app.project.board.footprints.len(),
        before + 4,
        "one part becomes a row of five: {}",
        app.message
    );
    assert!(!app.edit.arrange.open, "the dialog closes on success");
    app.tool_call("undo", json!({}));
    assert_eq!(
        app.project.board.footprints.len(),
        before,
        "one undo removes the row"
    );
    Ok(())
}

#[test]
fn align_needs_two_selected_items() -> Result<(), String> {
    let (mut app, ids) = four_board_parts()?;
    app.set_selection(vec![ids[0]]);
    let before: Vec<_> = app
        .project
        .board
        .footprints
        .iter()
        .map(|f| f.position)
        .collect();
    app.align_selection("left");
    let after: Vec<_> = app
        .project
        .board
        .footprints
        .iter()
        .map(|f| f.position)
        .collect();
    assert_eq!(before, after, "nothing moves with one part selected");
    assert!(app.message.contains("at least two"), "{}", app.message);
    Ok(())
}

#[test]
fn array_dialog_bounds_match_the_engine() -> Result<(), String> {
    let (mut app, ids) = four_board_parts()?;
    let before = app.project.board.footprints.len();
    app.set_selection(vec![ids[0]]);
    app.open_array_dialog();
    app.edit.arrange.rows = crate::arrange::ARRAY_MAX + 1;
    app.edit.arrange.columns = 2;
    app.run_array();
    assert_eq!(
        app.project.board.footprints.len(),
        before,
        "refused, nothing added"
    );
    assert!(
        app.message.contains("1-64"),
        "the engine reason shows: {}",
        app.message
    );
    assert!(
        app.edit.arrange.open,
        "the dialog stays open after a refusal"
    );
    app.edit.arrange.rows = crate::arrange::ARRAY_MAX;
    app.edit.arrange.columns = 1;
    app.edit.arrange.pitch_x = crate::arrange::PITCH_MAX + 1.0;
    app.edit.arrange.pitch_y = 5.0;
    app.run_array();
    assert_eq!(
        app.project.board.footprints.len(),
        before,
        "pitch over the limit is refused"
    );
    Ok(())
}
