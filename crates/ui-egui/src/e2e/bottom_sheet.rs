//! Stories for the bottom sheet: a person drags its top edge to the height they want.

use super::super::bottom_sheet::SheetTab;
use super::super::*;
use super::layers::pcb_app;
use super::support::*;

/// The window the stories draw in (`frame` uses 1440 by 940).
const WINDOW_HEIGHT: f32 = 940.0;

fn sheet_rect(ctx: &egui::Context, id: &str) -> Result<Rect, String> {
    egui::containers::panel::PanelState::load(ctx, egui::Id::new(id))
        .map(|s| s.outer_rect)
        .ok_or_else(|| format!("the sheet panel {id} never showed"))
}

fn settle(ctx: &egui::Context, app: &mut App) {
    for _ in 0..4 {
        frame(ctx, app, vec![], egui::Modifiers::NONE);
    }
}

/// Presses on the top edge of the sheet, drags it by `dy` (negative is up) and lets go.
fn drag_top_edge(ctx: &egui::Context, app: &mut App, id: &str, dy: f32) -> Result<(), String> {
    let rect = sheet_rect(ctx, id)?;
    // Just inside the edge: the canvas above shares the outer pixel of the grab strip.
    let start = egui::pos2(rect.center().x, rect.min.y + 2.0);
    let button = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    frame(
        ctx,
        app,
        vec![egui::Event::PointerMoved(start)],
        egui::Modifiers::NONE,
    );
    frame(ctx, app, vec![button(start, true)], egui::Modifiers::NONE);
    let mut at = start;
    for step in 1..=4 {
        at = egui::pos2(start.x, start.y + dy * (step as f32) / 4.0);
        frame(
            ctx,
            app,
            vec![egui::Event::PointerMoved(at)],
            egui::Modifiers::NONE,
        );
    }
    frame(ctx, app, vec![button(at, false)], egui::Modifiers::NONE);
    settle(ctx, app);
    Ok(())
}

fn open_findings() -> Result<(egui::Context, App), String> {
    let (ctx, mut app) = pcb_app();
    app.change_tab(Tab::Pcb);
    app.open_sheet(SheetTab::Findings);
    settle(&ctx, &mut app);
    Ok((ctx, app))
}

#[test]
fn a_newcomer_drags_the_bottom_sheet_taller_and_shorter() -> Result<(), String> {
    let (ctx, mut app) = open_findings()?;
    let id = "bottom-sheet-open";
    let before = sheet_rect(&ctx, id)?.height();
    drag_top_edge(&ctx, &mut app, id, -80.0)?;
    let taller = sheet_rect(&ctx, id)?.height();
    assert!(
        (taller - (before + 80.0)).abs() < 3.0,
        "dragging the edge up 80 grows the sheet from {before} to about {}, got {taller}",
        before + 80.0
    );
    drag_top_edge(&ctx, &mut app, id, 50.0)?;
    let shorter = sheet_rect(&ctx, id)?.height();
    assert!(
        (shorter - (taller - 50.0)).abs() < 3.0,
        "dragging the edge down 50 shrinks the sheet from {taller} to about {}, got {shorter}",
        taller - 50.0
    );
    Ok(())
}

#[test]
fn a_newcomer_cannot_drag_the_bottom_sheet_over_the_whole_canvas() -> Result<(), String> {
    let (ctx, mut app) = open_findings()?;
    let id = "bottom-sheet-open";
    drag_top_edge(&ctx, &mut app, id, -2000.0)?;
    let tallest = sheet_rect(&ctx, id)?.height();
    assert!(
        (tallest - WINDOW_HEIGHT * 0.7).abs() < 2.0,
        "the sheet stops at 70 percent of the window, got {tallest} of {WINDOW_HEIGHT}"
    );
    drag_top_edge(&ctx, &mut app, id, 2000.0)?;
    let shortest = sheet_rect(&ctx, id)?.height();
    assert!(
        (shortest - 120.0).abs() < 5.0,
        "the sheet keeps its tab strip and a few rows (about 120, plus its border), got {shortest}"
    );
    Ok(())
}

#[test]
fn a_newcomer_gets_the_dragged_height_back_after_folding_the_sheet() -> Result<(), String> {
    let (ctx, mut app) = open_findings()?;
    let id = "bottom-sheet-open";
    let before = sheet_rect(&ctx, id)?.height();
    drag_top_edge(&ctx, &mut app, id, -60.0)?;
    let dragged = sheet_rect(&ctx, id)?.height();
    assert!(
        dragged > before + 50.0,
        "the drag grew the sheet: {before} to {dragged}"
    );
    app.region().bottom_open = false;
    settle(&ctx, &mut app);
    let strip = sheet_rect(&ctx, "bottom-sheet")?.height();
    assert!(
        strip < 60.0,
        "folded, the sheet is only the tab strip, got {strip}"
    );
    app.region().bottom_open = true;
    settle(&ctx, &mut app);
    let again = sheet_rect(&ctx, id)?.height();
    assert!(
        (again - dragged).abs() < 1.0,
        "opened again, the sheet is {again} high, not the {dragged} it was dragged to"
    );
    Ok(())
}

#[test]
fn a_newcomer_opens_the_simulation_tab() -> Result<(), String> {
    let (ctx, mut app) = pcb_app();
    app.change_tab(Tab::Pcb);
    settle(&ctx, &mut app);
    tap_id(&ctx, &mut app, "tab:Simulation")?;
    settle(&ctx, &mut app);
    assert_eq!(
        app.region().bottom_tab,
        "simulation",
        "tapping the Simulation tab opens the Simulation page in the bottom sheet"
    );
    Ok(())
}
