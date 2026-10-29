use super::walk::acute;
use crate::{Error, Session, missing};
use pcbcraft_doc::{Id, Layer, Track};
use pcbcraft_geom::{Point, segment_distance};
use serde_json::{Value, json};

struct Fixture {
    s: Session,
    nets: Vec<Id>,
    pads: Vec<(Id, &'static str)>,
}

/// Each spec is (net index, x, y, pad diameter). Pads are paired two per part,
/// so give an even count. Every pad sits at exactly the position given.
fn fixture(width: f64, height: f64, specs: &[(usize, f64, f64, f64)]) -> Result<Fixture, Error> {
    let mut s = Session::default();
    s.execute("board.resize", json!({"width":width,"height":height}), None)?;
    let net_count = specs.iter().map(|p| p.0 + 1).max().unwrap_or(0);
    let mut nets = vec![];
    for i in 0..net_count {
        let r = s.execute("net.create", json!({"name":format!("N{i}")}), None)?;
        nets.push(r["result"]["id"].as_u64().ok_or_else(missing)?);
    }
    let mut parts = vec![];
    for (i, pair) in specs.chunks(2).enumerate() {
        let r = s.execute(
            "component.add",
            json!({"kind":"resistor","reference":format!("R{}",i+1),"position":{"x":10+i*5,"y":10}}),
            None,
        )?;
        let id = r["result"]["id"].as_u64().ok_or_else(missing)?;
        for (k, spec) in pair.iter().enumerate() {
            let net = *nets.get(spec.0).ok_or_else(missing)?;
            let pin = if k == 0 { "1" } else { "2" };
            s.execute(
                "net.connect",
                json!({"net":net,"pins":[{"component":id,"pin":pin}]}),
                None,
            )?;
        }
        parts.push(id);
    }
    s.execute("board.sync", json!({}), None)?;
    let mut pads = vec![];
    for (i, f) in s.project.board.footprints.iter_mut().enumerate() {
        f.position = Point::new(0.0, 0.0);
        for (k, pad) in f.pads.iter_mut().enumerate() {
            let spec = specs.get(i * 2 + k).ok_or_else(missing)?;
            pad.offset = Point::new(spec.1, spec.2);
            pad.diameter = spec.3;
            pads.push((f.id, if pad.number == "1" { "1" } else { "2" }));
        }
    }
    Ok(Fixture { s, nets, pads })
}

impl Fixture {
    fn pad(&self, i: usize) -> Result<Value, Error> {
        let (f, n) = self.pads.get(i).ok_or_else(missing)?;
        Ok(json!({"footprint":f,"pad":n}))
    }
    fn track(&mut self, net: usize, pts: &[(f64, f64)], width: f64) -> Result<Vec<Id>, Error> {
        let net = *self.nets.get(net).ok_or_else(missing)?;
        let mut ids = vec![];
        for w in pts.windows(2) {
            let r = self.s.execute(
                "track.add",
                json!({"net":net,"start":{"x":w[0].0,"y":w[0].1},"end":{"x":w[1].0,"y":w[1].1},"width":width}),
                None,
            )?;
            ids.push(r["result"]["id"].as_u64().ok_or_else(missing)?);
        }
        Ok(ids)
    }
    fn route(&mut self, from: usize, to: usize, mode: &str) -> Result<Value, Error> {
        let (a, b) = (self.pad(from)?, self.pad(to)?);
        self.s.execute(
            "track.route",
            json!({"from":a,"to":b,"width":0.25,"mode":mode}),
            None,
        )
    }
    fn tracks(&self, ids: &[Id]) -> Vec<Track> {
        ids.iter()
            .filter_map(|id| self.s.project.board.tracks.iter().find(|t| t.id == *id))
            .cloned()
            .collect()
    }
    fn errors(&self) -> Vec<String> {
        pcbcraft_rules::drc(&self.s.project)
            .into_iter()
            // Routing is judged on copper; the fixture parts sit close together,
            // which is a placement matter reported as a courtyard overlap.
            .filter(|f| f.severity == "error" && f.code != "courtyard_overlap")
            .map(|f| format!("{} {:?} {}", f.code, f.objects, f.message))
            .collect()
    }
}

fn ids(result: &Value) -> Vec<Id> {
    result["result"]["ids"]
        .as_array()
        .map(|a| a.iter().filter_map(Value::as_u64).collect())
        .unwrap_or_default()
}

fn polyline(tracks: &[Track]) -> Vec<Point> {
    let mut pts: Vec<Point> = tracks.first().map(|t| vec![t.start]).unwrap_or_default();
    pts.extend(tracks.iter().map(|t| t.end));
    pts
}

fn only_legal_angles(tracks: &[Track]) {
    for t in tracks {
        let angle = (t.end.y - t.start.y)
            .atan2(t.end.x - t.start.x)
            .to_degrees();
        let snapped = (angle / 45.0).round() * 45.0;
        assert!((angle - snapped).abs() < 1e-4, "segment at {angle} degrees");
    }
}

fn is_connected_chain(tracks: &[Track]) -> bool {
    tracks
        .windows(2)
        .all(|w| w[0].end.distance(w[1].start) < 1e-9)
}

#[test]
fn octilinear_routes_walk_around_a_foreign_pad() -> Result<(), Error> {
    let mut f = fixture(
        40.0,
        30.0,
        &[
            (0, 5.0, 15.0, 1.0),
            (0, 35.0, 15.0, 1.0),
            (1, 20.0, 15.0, 3.0),
            (1, 5.0, 25.0, 1.0),
        ],
    )?;
    let r = f.route(0, 1, "octilinear")?;
    let tracks = f.tracks(&ids(&r));
    assert!(tracks.len() >= 3, "a detour needs several segments");
    only_legal_angles(&tracks);
    assert!(is_connected_chain(&tracks));
    assert_eq!(acute(&polyline(&tracks)), 0, "no sharp corners");
    let pad = Point::new(20.0, 15.0);
    for t in &tracks {
        let gap = pcbcraft_geom::point_segment(pad, t.start, t.end) - 1.5 - 0.125;
        assert!(gap >= 0.2 - 1e-6, "gap {gap}");
    }
    assert!(f.errors().is_empty(), "{:?}", f.errors());
    Ok(())
}

#[test]
fn octilinear_route_with_a_clear_line_is_one_straight_or_one_bend() -> Result<(), Error> {
    let mut f = fixture(
        40.0,
        30.0,
        &[
            (0, 5.0, 10.0, 1.0),
            (0, 30.0, 20.0, 1.0),
            (1, 5.0, 25.0, 1.0),
            (1, 35.0, 25.0, 1.0),
        ],
    )?;
    let r = f.route(0, 1, "octilinear")?;
    let tracks = f.tracks(&ids(&r));
    assert_eq!(tracks.len(), 2);
    only_legal_angles(&tracks);
    assert!(f.errors().is_empty());
    Ok(())
}

#[test]
fn octilinear_routes_walk_around_a_foreign_track_and_its_pads() -> Result<(), Error> {
    let mut f = fixture(
        40.0,
        30.0,
        &[
            (0, 5.0, 15.0, 1.0),
            (0, 35.0, 15.0, 1.0),
            (1, 20.0, 10.0, 1.0),
            (1, 20.0, 20.0, 1.0),
        ],
    )?;
    f.track(1, &[(20.0, 10.0), (20.0, 20.0)], 0.25)?;
    let r = f.route(0, 1, "octilinear")?;
    let tracks = f.tracks(&ids(&r));
    only_legal_angles(&tracks);
    assert_eq!(acute(&polyline(&tracks)), 0);
    for t in &tracks {
        let gap = segment_distance(
            t.start,
            t.end,
            Point::new(20.0, 10.0),
            Point::new(20.0, 20.0),
        ) - 0.25;
        assert!(gap >= 0.2 - 1e-6, "gap {gap}");
    }
    assert!(f.errors().is_empty(), "{:?}", f.errors());
    Ok(())
}

/// Net B has a three-piece track in the way of net A's straight run. Pads of
/// net C leave a gap so tight that only moving B's track makes room for A.
fn shove_fixture() -> Result<(Fixture, Vec<Id>, Vec<Id>), Error> {
    let mut f = fixture(
        40.0,
        30.0,
        &[
            (0, 5.0, 15.0, 1.0),
            (0, 35.0, 15.0, 1.0),
            (1, 12.0, 17.2, 1.0),
            (1, 28.0, 17.2, 1.0),
            (2, 20.0, 13.9, 1.0),
            (2, 20.0, 16.3, 1.0),
            (3, 5.0, 5.0, 1.0),
            (3, 35.0, 5.0, 1.0),
        ],
    )?;
    let b = f.track(
        1,
        &[(12.0, 17.2), (14.1, 15.1), (25.9, 15.1), (28.0, 17.2)],
        0.25,
    )?;
    let d = f.track(3, &[(5.0, 5.0), (35.0, 5.0)], 0.25)?;
    Ok((f, b, d))
}

#[test]
fn shove_moves_exactly_the_blocking_track() -> Result<(), Error> {
    let (mut f, b, d) = shove_fixture()?;
    let b_before = f.tracks(&b);
    let d_before = f.tracks(&d);
    let revision = f.s.revision;
    let r = f.route(0, 1, "octilinear_shove")?;
    assert!(f.s.revision > revision);
    let a = f.tracks(&ids(&r));
    assert_eq!(a.len(), 1, "net A keeps its straight run");
    only_legal_angles(&a);
    assert_eq!(f.tracks(&d), d_before, "the decoy track must not move");
    let pushed = r["result"]["pushed"].as_array().ok_or_else(missing)?;
    assert_eq!(pushed.len(), 1);
    assert_eq!(pushed[0]["rank"], -1);
    let moved_ids: Vec<Id> = pushed[0]["ids"]
        .as_array()
        .ok_or_else(missing)?
        .iter()
        .filter_map(Value::as_u64)
        .collect();
    assert!(b.iter().all(|id| moved_ids.contains(id)) || moved_ids.len() < b.len());
    let b_after: Vec<Track> =
        f.s.project
            .board
            .tracks
            .iter()
            .filter(|t| moved_ids.contains(&t.id))
            .cloned()
            .collect();
    let line_start = b_before.first().map(|t| t.start).ok_or_else(missing)?;
    let line_end = b_before.last().map(|t| t.end).ok_or_else(missing)?;
    assert_eq!(b_after.first().map(|t| t.start), Some(line_start));
    assert_eq!(b_after.last().map(|t| t.end), Some(line_end));
    only_legal_angles(&b_after);
    assert!(f.errors().is_empty(), "{:?}", f.errors());
    Ok(())
}

#[test]
fn shove_refuses_when_the_pushed_track_cannot_move_and_changes_nothing() -> Result<(), Error> {
    let mut f = fixture(
        40.0,
        30.0,
        &[
            (0, 5.0, 15.0, 1.0),
            (0, 35.0, 15.0, 1.0),
            (1, 5.0, 25.0, 1.0),
            (1, 35.0, 25.0, 1.0),
        ],
    )?;
    f.track(1, &[(10.0, 15.2), (20.0, 15.2)], 0.25)?;
    let (revision, count) = (f.s.revision, f.s.project.board.tracks.len());
    let e = f
        .route(0, 1, "octilinear_shove")
        .err()
        .ok_or_else(missing)?;
    assert!(e.to_string().contains("N1"), "{e}");
    assert_eq!(f.s.revision, revision);
    assert_eq!(f.s.project.board.tracks.len(), count);
    Ok(())
}

#[test]
fn routing_with_no_room_refuses_with_a_reason_and_leaves_the_design() -> Result<(), Error> {
    let mut f = fixture(
        40.0,
        6.0,
        &[
            (0, 5.0, 3.0, 1.0),
            (0, 35.0, 3.0, 1.0),
            (1, 20.0, 3.0, 5.0),
            (1, 38.0, 1.0, 0.8),
        ],
    )?;
    let (revision, count) = (f.s.revision, f.s.project.board.tracks.len());
    for mode in ["octilinear", "octilinear_shove"] {
        let e = f.route(0, 1, mode).err().ok_or_else(missing)?;
        assert!(e.to_string().contains("no clear 45-degree path"), "{e}");
    }
    assert_eq!(f.s.revision, revision);
    assert_eq!(f.s.project.board.tracks.len(), count);
    Ok(())
}

#[test]
fn a_pad_already_too_close_to_another_net_is_named() -> Result<(), Error> {
    let mut f = fixture(
        40.0,
        30.0,
        &[
            (0, 5.0, 15.0, 1.0),
            (0, 35.0, 15.0, 1.0),
            (1, 35.0, 15.7, 1.0),
            (1, 5.0, 25.0, 1.0),
        ],
    )?;
    let e = f.route(0, 1, "octilinear").err().ok_or_else(missing)?;
    assert!(e.to_string().contains("too close to net N1"), "{e}");
    Ok(())
}

#[test]
fn routes_run_from_a_pad_to_a_point_given_a_net() -> Result<(), Error> {
    let mut f = fixture(
        40.0,
        30.0,
        &[
            (0, 5.0, 15.0, 1.0),
            (0, 35.0, 5.0, 1.0),
            (1, 20.0, 15.0, 3.0),
            (1, 5.0, 25.0, 1.0),
        ],
    )?;
    let from = f.pad(0)?;
    let net = *f.nets.first().ok_or_else(missing)?;
    let r = f.s.execute(
        "track.route",
        json!({"from":from,"to":{"x":30.0,"y":15.0},"net":net,"width":0.25,"mode":"octilinear"}),
        None,
    )?;
    let tracks = f.tracks(&ids(&r));
    only_legal_angles(&tracks);
    assert_eq!(tracks.last().map(|t| t.end), Some(Point::new(30.0, 15.0)));
    let e = f.s.execute(
        "track.route",
        json!({"from":{"x":1.0,"y":1.0},"to":{"x":30.0,"y":15.0},"mode":"octilinear"}),
        None,
    );
    assert!(e.is_err());
    Ok(())
}

#[test]
fn default_route_mode_keeps_the_old_l_shape() -> Result<(), Error> {
    let mut f = fixture(
        40.0,
        30.0,
        &[
            (0, 5.0, 10.0, 1.0),
            (0, 30.0, 20.0, 1.0),
            (1, 5.0, 25.0, 1.0),
            (1, 35.0, 25.0, 1.0),
        ],
    )?;
    let (a, b) = (f.pad(0)?, f.pad(1)?);
    let r = f.s.execute("track.route", json!({"from":a,"to":b}), None)?;
    let tracks = f.tracks(&ids(&r));
    assert_eq!(tracks.len(), 2);
    assert_eq!(tracks[0].end, Point::new(30.0, 10.0));
    assert_eq!(tracks[0].layer, Layer::Front);
    Ok(())
}

#[test]
fn routing_is_deterministic() -> Result<(), Error> {
    let run = || -> Result<String, Error> {
        let (mut f, _, _) = shove_fixture()?;
        f.route(0, 1, "octilinear_shove")?;
        Ok(serde_json::to_string(&f.s.project.board.tracks).map_err(|e| e.to_string())?)
    };
    assert_eq!(run()?, run()?);
    Ok(())
}

#[test]
fn walkaround_alone_never_moves_another_nets_track() -> Result<(), Error> {
    let (mut f, b, d) = shove_fixture()?;
    let (b_before, d_before) = (f.tracks(&b), f.tracks(&d));
    let outcome = f.route(0, 1, "octilinear")?;
    let walked = f.tracks(&ids(&outcome));
    let length: f64 = walked.iter().map(|t| t.start.distance(t.end)).sum();
    assert!(
        length > 30.5 && walked.len() > 1,
        "the tight gap forces a detour: {length}"
    );
    assert_eq!(f.tracks(&b), b_before);
    assert_eq!(f.tracks(&d), d_before);
    Ok(())
}
