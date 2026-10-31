//! Stories for the right-click menus: one designed component, driven by the keyboard.
//!
//! Context: ticket #836. The menu itself is `../ui_kit/menu.rs`; these stories open
//! it on the real window and use only the keyboard, as a person who has no mouse
//! at hand would.
use super::super::*;
use super::board::right_click;
use super::support::*;

fn press(key: egui::Key) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }
}

fn sample_board() -> App {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.tool_call("sample_design", json!({}));
    app.change_tab(Tab::Pcb);
    app.needs_fit = true;
    app
}

/// Zoom the board in, open the menu on empty space and return the context and app.
fn menu_over_empty_space() -> Result<(egui::Context, App, f32), String> {
    let mut app = sample_board();
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    app.pcb.zoom *= 4.0;
    let zoomed = app.pcb.zoom;
    let screen = pcb_screen(&app, Point::new(-500.0, -500.0));
    let screen = egui::pos2(
        screen
            .x
            .clamp(app.canvas_rect.min.x + 5.0, app.canvas_rect.max.x - 5.0),
        screen
            .y
            .clamp(app.canvas_rect.min.y + 5.0, app.canvas_rect.max.y - 5.0),
    );
    right_click(&ctx, &mut app, screen);
    Ok((ctx, app, zoomed))
}

#[test]
fn a_newcomer_uses_the_keyboard_in_a_right_click_menu() -> Result<(), String> {
    let (ctx, mut app, zoomed) = menu_over_empty_space()?;
    assert!(
        paints_text(&ctx, &mut app, "Fit view")?,
        "a right click on empty space opens the menu: {}",
        app.message
    );
    // Paste is greyed out until something is copied; the arrow key skips it and
    // lands on Fit view, so one Down and Enter fits the view.
    frame(
        &ctx,
        &mut app,
        vec![press(egui::Key::ArrowDown)],
        egui::Modifiers::NONE,
    );
    frame(
        &ctx,
        &mut app,
        vec![press(egui::Key::Enter)],
        egui::Modifiers::NONE,
    );
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    assert!(
        (app.pcb.zoom - zoomed).abs() > f32::EPSILON,
        "Down then Enter picked Fit view and changed the zoom"
    );
    assert!(
        !paints_text(&ctx, &mut app, "Fit view")?,
        "picking a row closes the menu"
    );

    // Type-ahead: the first letters of a row are enough.
    let (ctx, mut app, zoomed) = menu_over_empty_space()?;
    frame(
        &ctx,
        &mut app,
        vec![egui::Event::Text("f".into())],
        egui::Modifiers::NONE,
    );
    frame(
        &ctx,
        &mut app,
        vec![press(egui::Key::Enter)],
        egui::Modifiers::NONE,
    );
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    assert!(
        (app.pcb.zoom - zoomed).abs() > f32::EPSILON,
        "typing F then Enter picked Fit view"
    );

    // Up from nothing lands on the last row, Clear selection, and Escape leaves.
    let (ctx, mut app, zoomed) = menu_over_empty_space()?;
    frame(
        &ctx,
        &mut app,
        vec![press(egui::Key::ArrowUp)],
        egui::Modifiers::NONE,
    );
    frame(
        &ctx,
        &mut app,
        vec![press(egui::Key::ArrowUp)],
        egui::Modifiers::NONE,
    );
    frame(
        &ctx,
        &mut app,
        vec![press(egui::Key::Escape)],
        egui::Modifiers::NONE,
    );
    for _ in 0..3 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    assert!(
        !paints_text(&ctx, &mut app, "Clear selection")?,
        "Escape closes the menu"
    );
    assert!(
        (app.pcb.zoom - zoomed).abs() < f32::EPSILON,
        "Escape picked nothing"
    );
    Ok(())
}

#[test]
fn a_newcomer_sees_a_part_menu_in_the_same_order_every_time() -> Result<(), String> {
    let mut app = sample_board();
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let at = app
        .project
        .board
        .footprints
        .first()
        .and_then(|fp| app.project.object_position(fp.id))
        .ok_or("the sample board has no part")?;
    for _ in 0..4 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    let screen = pcb_screen(&app, at);
    right_click(&ctx, &mut app, screen);
    let output = frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    let mut seen: Vec<(String, f32)> = Vec::new();
    for shape in &output.shapes {
        if let egui::Shape::Text(text) = &shape.shape {
            let word = text.galley.text().to_string();
            if [
                "Properties",
                "Rotate",
                "Lock in place",
                "Copy",
                "Duplicate",
                "Delete",
            ]
            .contains(&word.as_str())
            {
                seen.push((word, text.pos.y));
            }
        }
    }
    // The menu's labels share one left edge; words of the page behind it do not.
    let edge = output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.text() == "Delete" => Some(text.pos.x),
            _ => None,
        })
        .ok_or("the menu has no Delete row")?;
    seen.clear();
    for shape in &output.shapes {
        if let egui::Shape::Text(text) = &shape.shape {
            let word = text.galley.text().to_string();
            if (text.pos.x - edge).abs() < 0.5
                && [
                    "Properties",
                    "Rotate",
                    "Lock in place",
                    "Copy",
                    "Duplicate",
                    "Delete",
                ]
                .contains(&word.as_str())
            {
                seen.push((word, text.pos.y));
            }
        }
    }
    seen.sort_by(|a, b| a.1.total_cmp(&b.1));
    let order: Vec<&str> = seen.iter().map(|(w, _)| w.as_str()).collect();
    assert_eq!(
        order,
        [
            "Rotate",
            "Lock in place",
            "Properties",
            "Copy",
            "Duplicate",
            "Delete"
        ],
        "object actions, then edit, then Delete last: {}",
        app.message
    );
    Ok(())
}
