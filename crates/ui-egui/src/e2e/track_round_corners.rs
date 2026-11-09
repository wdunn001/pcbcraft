//! Acceptance story: a_newcomer_rounds_every_corner_of_a_board_in_one_step
//!
//! A newcomer routes a track with a sharp right-angle corner, opens
//! Edit > Round corners, types a radius, applies it and sees the corner turn
//! into an arc. One Undo brings the sharp corner back.

use super::super::*;
use super::support::*;

/// A board with two pads on one net, one high and left, one low and right,
/// so the orthogonal route between them has a right-angle corner.
fn board() -> Result<App, String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.run("board.resize", json!({"width":40.0,"height":30.0}));
    app.run("net.create", json!({"name":"N0"}));
    let net = app.project.nets.last().ok_or("net id")?.id;
    app.run(
        "component.add",
        json!({"kind":"resistor","reference":"R1","position":{"x":10,"y":10}}),
    );
    let part = app.project.components.last().ok_or("part id")?.id;
    for pin in ["1", "2"] {
        app.run(
            "net.connect",
            json!({"net":net,"pins":[{"component":part,"pin":pin}]}),
        );
    }
    app.run("board.sync", json!({}));
    {
        let mut s = app.session.lock().map_err(|_| "session lock")?;
        for f in s.project.board.footprints.iter_mut() {
            f.position = pcbcraft_geom::Point::new(0.0, 0.0);
            for (k, pad) in f.pads.iter_mut().enumerate() {
                pad.offset = if k == 0 {
                    pcbcraft_geom::Point::new(5.0, 10.0)
                } else {
                    pcbcraft_geom::Point::new(30.0, 22.0)
                };
                pad.diameter = 1.0;
            }
        }
    }
    app.refresh();
    app.change_tab(Tab::Pcb);
    Ok(app)
}

fn arcs(app: &App) -> usize {
    app.project
        .board
        .tracks
        .iter()
        .filter(|t| t.arc.is_some())
        .count()
}

#[test]
fn a_newcomer_rounds_every_corner_of_a_board_in_one_step() -> Result<(), String> {
    let mut app = board()?;
    let pad = |app: &App, k: usize| -> Result<pcbcraft_geom::Point, String> {
        let f = app.project.board.footprints.first().ok_or("footprint")?;
        let n = f.pads.get(k).ok_or("pad")?.number.clone();
        f.pad_position(&n).ok_or_else(|| "pad place".to_string())
    };
    let (a, b) = (pad(&app, 0)?, pad(&app, 1)?);
    app.tool = Tool::Route(None);
    assert!(app.begin_route_drag(a, 20.0), "the press took a pad");
    app.end_route_drag(b, 20.0);
    let sharp = app.project.board.tracks.clone();
    assert!(sharp.len() >= 2, "the route has a corner: {}", app.message);
    assert_eq!(arcs(&app), 0, "the route starts out sharp");

    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Edit")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Round corners")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(app.edit.round.open, "the Round corners window opens");
    app.edit.round.radius = 2.0;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    // The window title and the apply button share a label; the button is the
    // last one painted, which is the one a click lands on.
    authoring::tests::tap(&ctx, &mut app, "Round corners")?;
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(arcs(&app) >= 1, "the corner became an arc: {}", app.message);
    assert!(
        app.message.starts_with("Rounded 1 corners"),
        "the strip reports it: {}",
        app.message
    );

    app.tool_call("undo", json!({}));
    assert_eq!(arcs(&app), 0, "one undo removes every arc");
    assert_eq!(
        app.project.board.tracks, sharp,
        "one undo restores the sharp corners"
    );
    Ok(())
}
