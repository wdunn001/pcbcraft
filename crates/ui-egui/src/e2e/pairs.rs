//! Differential pair routing: one drag lays both sides, bending at 45 degrees
//! unless "Chamfer corners" is off.

use super::super::*;
use super::support::*;

/// Two parts joined by a declared pair, pads stacked so the sides run
/// side by side. Returns the app plus both footprints' ids.
fn pair_board(right_angles: bool) -> Result<(App, Id, Id), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.run("project.set_auto_sync", json!({"auto_sync": false}));
    for (reference, y) in [("R1", 20.0), ("R2", 34.0)] {
        let x = if reference == "R1" { 10.0 } else { 50.0 };
        app.run(
            "component.add",
            json!({"kind":"resistor","reference":reference,"value":"1 k","position":{"x":x,"y":y}}),
        );
    }
    for pin in ["1", "2"] {
        app.run(
            "wire.connect",
            json!({"from":{"component":1,"pin":pin},"to":{"component":2,"pin":pin}}),
        );
    }
    let nets: Vec<Id> = app
        .project
        .components
        .first()
        .ok_or("the first part exists")?
        .pins
        .iter()
        .filter_map(|p| p.net)
        .collect();
    let (net_p, net_n) = (
        *nets.first().ok_or("pin 1 has a net")?,
        *nets.get(1).ok_or("pin 2 has a net")?,
    );
    app.run(
        "board.set_diffpair",
        json!({"name":"USB","net_p":net_p,"net_n":net_n,"gap":0.3}),
    );
    app.run("board.sync", json!({}));
    let ids: Vec<Id> = app.project.board.footprints.iter().map(|f| f.id).collect();
    // Right-angle pairs nest cleanly when the start pads stack vertically
    // and the end pads sit side by side, running upward; 45 degree pairs use
    // stacked pads at both ends running downward.
    let spots = if right_angles {
        [(10.0, 34.0), (50.0, 20.0)]
    } else {
        [(10.0, 20.0), (50.0, 34.0)]
    };
    for (k, (id, (x, y))) in ids.iter().zip(spots).enumerate() {
        let stacked = !right_angles || k == 0;
        let turns: &[f64] = if stacked {
            &[90.0, 270.0]
        } else {
            &[0.0, 180.0]
        };
        for degrees in turns {
            app.run("footprint.rotate", json!({"id":id,"degrees":degrees}));
            app.run("footprint.move", json!({"id":id,"position":{"x":x,"y":y}}));
            app.refresh();
            let at = |n: &str| app.project.footprint(*id).and_then(|f| f.pad_position(n));
            let (one, two) = (
                at("1").ok_or("pad 1 exists")?,
                at("2").ok_or("pad 2 exists")?,
            );
            let wanted = if stacked {
                two.y > one.y
            } else {
                two.x > one.x
            };
            if wanted {
                break;
            }
        }
    }
    app.refresh();
    let first = *ids.first().ok_or("a footprint was placed")?;
    let second = *ids.get(1).ok_or("two footprints were placed")?;
    Ok((app, first, second))
}

fn drag_pair(app: &mut App, from: Id, to: Id) -> Result<(), String> {
    app.tab = Tab::Pcb;
    app.tool = Tool::Route(None);
    let start = app
        .project
        .footprint(from)
        .and_then(|f| f.pad_position("1"))
        .ok_or("the first part has pad 1")?;
    let end = app
        .project
        .footprint(to)
        .and_then(|f| f.pad_position("1"))
        .ok_or("the second part has pad 1")?;
    app.canvas_click(start, 1.0);
    app.canvas_click(end, 1.0);
    Ok(())
}

fn leans(t: &pcbcraft_doc::Track) -> bool {
    let (dx, dy) = ((t.end.x - t.start.x).abs(), (t.end.y - t.start.y).abs());
    dx > 1e-6 && dy > 1e-6
}

#[test]
fn a_newcomer_routes_a_pair_in_one_drag() -> Result<(), String> {
    let (mut app, from, to) = pair_board(false)?;
    drag_pair(&mut app, from, to)?;
    let pair = app
        .project
        .board
        .diff_pairs
        .first()
        .ok_or("a declared pair")?
        .clone();
    for net in [pair.net_p, pair.net_n] {
        assert!(
            app.project.board.tracks.iter().any(|t| t.net == net),
            "both sides get copper from one drag: {}",
            app.message
        );
    }
    let rows = pair_rows(&app.project, pair.net_p);
    assert!(
        rows.iter().any(|(k, v)| k == "Pair" && v.contains("USB")),
        "the properties name the pair"
    );
    assert!(
        rows.iter().any(|(k, _)| k == "Pair skew"),
        "and show the skew"
    );
    Ok(())
}

#[test]
fn a_newcomer_sees_a_pair_bend_at_45_degrees_by_default() -> Result<(), String> {
    let (mut app, from, to) = pair_board(false)?;
    assert!(app.preferences.chamfer_corners, "Chamfer corners starts on");
    drag_pair(&mut app, from, to)?;
    let tracks = &app.project.board.tracks;
    assert!(!tracks.is_empty(), "the pair was routed: {}", app.message);
    for t in tracks.iter().filter(|t| leans(t)) {
        let (dx, dy) = ((t.end.x - t.start.x).abs(), (t.end.y - t.start.y).abs());
        assert!(
            (dx - dy).abs() < 1e-6,
            "slanted runs are exactly 45 degrees"
        );
    }
    assert!(
        tracks.iter().any(leans),
        "the pair bends through a slanted run: {} {:?}",
        app.message,
        tracks
            .iter()
            .map(|t| (t.net, t.start, t.end))
            .collect::<Vec<_>>()
    );
    Ok(())
}

#[test]
fn a_newcomer_turns_chamfer_corners_off_for_right_angles() -> Result<(), String> {
    let (mut app, from, to) = pair_board(true)?;
    app.change_tab(Tab::Settings);
    app.refresh();
    let ctx = egui::Context::default();
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    assert!(
        scrolled_to_at(&ctx, &mut app, "Chamfer corners", 700.0)?,
        "the setting is on screen"
    );
    authoring::tests::tap(&ctx, &mut app, "Chamfer corners")?;
    assert!(!app.preferences.chamfer_corners, "one tap switches it off");
    drag_pair(&mut app, from, to)?;
    let tracks = &app.project.board.tracks;
    assert!(!tracks.is_empty(), "the pair was routed: {}", app.message);
    assert!(
        !tracks.iter().any(leans),
        "with the setting off every corner is a right angle"
    );
    Ok(())
}
