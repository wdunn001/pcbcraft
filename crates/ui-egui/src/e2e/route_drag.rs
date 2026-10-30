use super::super::*;
use super::support::*;

/// Each spec is (net index, x, y, pad diameter); pads come two to a part.
/// Every pad sits exactly where the spec says.
fn board(specs: &[(usize, f64, f64, f64)]) -> Result<App, String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.run("board.resize", json!({"width":40.0,"height":30.0}));
    let mut nets = vec![];
    for i in 0..specs.iter().map(|s| s.0 + 1).max().unwrap_or(0) {
        app.run("net.create", json!({"name":format!("N{i}")}));
        nets.push(app.project.nets.last().ok_or("net id")?.id);
    }
    for (i, pair) in specs.chunks(2).enumerate() {
        app.run(
            "component.add",
            json!({"kind":"resistor","reference":format!("R{}",i+1),"position":{"x":10+i*5,"y":10}}),
        );
        let part = app.project.components.last().ok_or("part id")?.id;
        for (k, spec) in pair.iter().enumerate() {
            let net = nets.get(spec.0).ok_or("net exists")?;
            let pin = if k == 0 { "1" } else { "2" };
            app.run(
                "net.connect",
                json!({"net":net,"pins":[{"component":part,"pin":pin}]}),
            );
        }
    }
    app.run("board.sync", json!({}));
    {
        let mut s = app.session.lock().map_err(|_| "session lock")?;
        for (i, f) in s.project.board.footprints.iter_mut().enumerate() {
            f.position = pcbcraft_geom::Point::new(0.0, 0.0);
            for (k, pad) in f.pads.iter_mut().enumerate() {
                let spec = specs.get(i * 2 + k).ok_or("spec exists")?;
                pad.offset = pcbcraft_geom::Point::new(spec.1, spec.2);
                pad.diameter = spec.3;
            }
        }
    }
    app.refresh();
    app.change_tab(Tab::Pcb);
    Ok(app)
}

fn pad_at(app: &App, index: usize) -> Result<(Id, String, Point), String> {
    let f = app
        .project
        .board
        .footprints
        .get(index / 2)
        .ok_or("footprint exists")?;
    let pad = f.pads.get(index % 2).ok_or("pad exists")?;
    let at = f.pad_position(&pad.number).ok_or("pad has a place")?;
    Ok((f.id, pad.number.clone(), at))
}

/// Press on pad `from`, drag, and let go at `to`, like the real pointer path.
fn drag_between(app: &mut App, from: Point, to: Point) {
    app.tool = Tool::Route(None);
    assert!(app.begin_route_drag(from, ZOOM), "the press took a pad");
    app.end_route_drag(to, ZOOM);
}

fn legal(t: &pcbcraft_doc::Track) -> bool {
    let angle = (t.end.y - t.start.y)
        .atan2(t.end.x - t.start.x)
        .to_degrees();
    ((angle / 45.0).round() * 45.0 - angle).abs() < 1e-4
}

/// A usual board zoom in pixels per millimetre; hit tests are in pixels.
const ZOOM: f32 = 20.0;

const PLAIN: [(usize, f64, f64, f64); 4] = [
    (0, 5.0, 15.0, 1.0),
    (0, 35.0, 15.0, 1.0),
    (1, 20.0, 15.0, 3.0),
    (1, 5.0, 25.0, 1.0),
];

#[test]
fn a_newcomer_drags_a_route_between_two_pads() -> Result<(), String> {
    let mut app = board(&PLAIN)?;
    let (_, _, a) = pad_at(&app, 0)?;
    let (_, _, b) = pad_at(&app, 1)?;
    let net = app.project.board.footprints[0].pads[0].net.ok_or("net")?;
    drag_between(&mut app, a, b);
    let copper: Vec<_> = app
        .project
        .board
        .tracks
        .iter()
        .filter(|t| t.net == net)
        .collect();
    assert!(!copper.is_empty(), "copper exists: {}", app.message);
    assert!(
        copper.iter().all(|t| legal(t)),
        "every run is 0, 45 or 90 degrees"
    );
    assert!(
        app.message.contains("committed"),
        "the route went through the engine: {}",
        app.message
    );
    Ok(())
}

#[test]
fn a_newcomer_sees_the_route_mode_while_dragging() -> Result<(), String> {
    let mut app = board(&PLAIN)?;
    let (_, _, a) = pad_at(&app, 0)?;
    app.tool = Tool::Route(None);
    assert!(app.begin_route_drag(a, ZOOM));
    assert_eq!(app.message, "Route · 45 degrees");
    app.end_route_drag(Point::new(1.0, 1.0), ZOOM);
    app.preferences.route_shove = true;
    app.tool = Tool::Route(None);
    assert!(app.begin_route_drag(a, ZOOM));
    assert_eq!(app.message, "Route · 45 degrees, pushing tracks aside");
    Ok(())
}

#[test]
fn a_newcomer_pushes_a_track_aside_with_one_checkbox() -> Result<(), String> {
    let specs = [
        (0, 5.0, 15.0, 1.0),
        (0, 35.0, 15.0, 1.0),
        (1, 12.0, 17.2, 1.0),
        (1, 28.0, 17.2, 1.0),
        (2, 20.0, 13.9, 1.0),
        (2, 20.0, 16.3, 1.0),
    ];
    let ctx = egui::Context::default();
    let mut app = board(&specs)?;
    app.change_tab(Tab::Settings);
    frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    authoring::tests::tap(&ctx, &mut app, "Push other tracks aside")?;
    assert!(app.preferences.route_shove, "the box is ticked");
    app.change_tab(Tab::Pcb);
    let net_b = app.project.board.footprints[1].pads[0].net.ok_or("net")?;
    app.run(
        "track.add",
        json!({"net":net_b,"start":{"x":12.0,"y":17.2},"end":{"x":14.1,"y":15.1},"width":0.25}),
    );
    app.run(
        "track.add",
        json!({"net":net_b,"start":{"x":14.1,"y":15.1},"end":{"x":25.9,"y":15.1},"width":0.25}),
    );
    app.run(
        "track.add",
        json!({"net":net_b,"start":{"x":25.9,"y":15.1},"end":{"x":28.0,"y":17.2},"width":0.25}),
    );
    let before: Vec<_> = app.project.board.tracks.clone();
    let (_, _, a) = pad_at(&app, 0)?;
    let (_, _, b) = pad_at(&app, 1)?;
    drag_between(&mut app, a, b);
    let after = &app.project.board.tracks;
    let moved: Vec<_> = before
        .iter()
        .filter(|t| after.iter().find(|u| u.id == t.id) != Some(t))
        .collect();
    assert!(
        !moved.is_empty(),
        "the blocking track moved: {}",
        app.message
    );
    assert!(
        moved.iter().all(|t| t.net == net_b),
        "only that net's copper moved"
    );
    Ok(())
}

#[test]
fn route_mode_follows_the_two_preferences() {
    let mut p = Preferences::default();
    for (chamfer, shove, mode) in [
        (false, false, "orthogonal"),
        (false, true, "orthogonal"),
        (true, false, "octilinear"),
        (true, true, "octilinear_shove"),
    ] {
        p.chamfer_corners = chamfer;
        p.route_shove = shove;
        assert_eq!(super::super::route_drag::route_mode(&p), mode);
    }
}

#[test]
fn a_newcomer_sees_why_a_route_was_refused() -> Result<(), String> {
    let specs = [
        (0, 5.0, 15.0, 1.0),
        (0, 35.0, 15.0, 1.0),
        (1, 5.0, 25.0, 1.0),
        (1, 35.0, 25.0, 1.0),
    ];
    let mut app = board(&specs)?;
    app.preferences.route_shove = true;
    let net_b = app.project.board.footprints[1].pads[0].net.ok_or("net")?;
    app.run(
        "track.add",
        json!({"net":net_b,"start":{"x":10.0,"y":15.2},"end":{"x":20.0,"y":15.2},"width":0.25}),
    );
    let (revision, count) = (app.revision, app.project.board.tracks.len());
    let (_, _, a) = pad_at(&app, 0)?;
    let (_, _, b) = pad_at(&app, 1)?;
    drag_between(&mut app, a, b);
    assert_eq!(app.revision, revision, "nothing changed");
    assert_eq!(app.project.board.tracks.len(), count);
    assert!(
        app.message.contains("N1"),
        "the engine sentence: {}",
        app.message
    );
    Ok(())
}

#[test]
fn route_drag_release_off_a_pad_cancels_without_a_change() -> Result<(), String> {
    let mut app = board(&PLAIN)?;
    let (_, _, a) = pad_at(&app, 0)?;
    let revision = app.revision;
    drag_between(&mut app, a, Point::new(30.0, 5.0));
    assert_eq!(app.revision, revision);
    assert_eq!(app.message, "Route cancelled: release the drag on a pad.");
    Ok(())
}

#[test]
fn a_newcomer_drags_a_track_end() -> Result<(), String> {
    let mut app = board(&PLAIN)?;
    let net = app.project.board.footprints[0].pads[0].net.ok_or("net")?;
    app.run(
        "track.add",
        json!({"net":net,"start":{"x":5.0,"y":15.0},"end":{"x":10.0,"y":15.0},"width":0.25}),
    );
    let id = app.project.board.tracks.first().ok_or("a track")?.id;
    app.tool = Tool::Select;
    app.select(Some(id));
    let width = app.project.board.tracks[0].width;
    assert!(
        app.begin_route_drag(Point::new(10.0, 15.0), ZOOM),
        "the end handle is grabbed"
    );
    app.end_route_drag(Point::new(10.0, 8.0), ZOOM);
    let tracks = &app.project.board.tracks;
    assert!(
        tracks.iter().any(|t| t.start == Point::new(5.0, 15.0)),
        "the other end stayed on its pad"
    );
    assert!(
        tracks.iter().any(|t| t.end == Point::new(10.0, 8.0)),
        "the dragged end arrived"
    );
    assert!(tracks.iter().all(|t| legal(t) && t.width == width));
    app.tool_call("undo", json!({}));
    let t = app.project.board.tracks.first().ok_or("restored")?;
    assert_eq!(app.project.board.tracks.len(), 1);
    assert_eq!(t.end, Point::new(10.0, 15.0), "one undo restores it");
    Ok(())
}
