//! Differential-pair skew and uncoupled-length rules.
use pcbcraft_doc::{DiffPair, Id, Layer, LayerRecord, Net, Point, Project, Track};
use pcbcraft_rules::drc;

type T = Result<(), String>;

fn net(id: Id, name: &str) -> Net {
    Net {
        id,
        name: name.into(),
        ..Default::default()
    }
}

fn track(id: Id, net: Id, start: Point, end: Point, layer: Layer) -> Track {
    Track {
        id,
        net,
        start,
        end,
        width: 0.2,
        layer,
        arc: None,
    }
}

fn pair(id: Id, net_p: Id, net_n: Id) -> DiffPair {
    DiffPair {
        id,
        name: "PAIR".into(),
        net_p,
        net_n,
        gap: 0.3,
        length_tolerance: None,
        target_impedance: Some(90.0),
        skew_limit_mm: None,
        skew_limit_ps: None,
        max_uncoupled_length: None,
        profile: None,
    }
}

fn dielectric(name: &str, thickness: f64, dk: f64) -> LayerRecord {
    LayerRecord {
        name: name.into(),
        kind: "prepreg".into(),
        thickness,
        dk: Some(dk),
        ..Default::default()
    }
}

fn codes(project: &Project) -> Vec<&'static str> {
    drc(project).into_iter().map(|f| f.code).collect()
}

#[test]
fn skew_over_the_limit_is_reported() -> T {
    let mut p = Project::default();
    p.nets.push(net(1, "P"));
    p.nets.push(net(2, "N"));
    let mut dp = pair(9, 1, 2);
    dp.skew_limit_mm = Some(1.0);
    p.board.diff_pairs.push(dp);
    p.board.tracks.push(track(
        10,
        1,
        Point::new(0.0, 0.0),
        Point::new(10.0, 0.0),
        Layer::Front,
    ));
    p.board.tracks.push(track(
        11,
        2,
        Point::new(0.0, 0.35),
        Point::new(15.0, 0.35),
        Layer::Front,
    ));
    p.validate()?;
    assert!(codes(&p).contains(&"pair_skew_exceeded"));
    Ok(())
}

#[test]
fn time_skew_uses_the_dk_of_the_layer_each_segment_runs_on() -> T {
    let mut p = Project::default();
    p.nets.push(net(1, "P"));
    p.nets.push(net(2, "N"));
    p.board.layer_records = vec![
        LayerRecord {
            name: "F.Cu".into(),
            kind: "copper".into(),
            thickness: 0.035,
            ..Default::default()
        },
        dielectric("prepreg_top", 0.2, 4.0),
        LayerRecord {
            name: "In1.Cu".into(),
            kind: "copper".into(),
            thickness: 0.035,
            ..Default::default()
        },
        dielectric("prepreg_bot", 0.2, 2.0),
        LayerRecord {
            name: "B.Cu".into(),
            kind: "copper".into(),
            thickness: 0.035,
            ..Default::default()
        },
    ];
    let mut dp = pair(9, 1, 2);
    dp.skew_limit_mm = Some(100.0);
    dp.skew_limit_ps = Some(50.0);
    p.board.diff_pairs.push(dp);
    let len = 20.0;
    p.board.tracks.push(track(
        10,
        1,
        Point::new(0.0, 0.0),
        Point::new(len, 0.0),
        Layer::Front,
    ));
    p.board.tracks.push(track(
        11,
        2,
        Point::new(0.0, 0.0),
        Point::new(len, 0.0),
        Layer::Back,
    ));
    p.validate()?;
    let with_time = codes(&p);
    assert!(
        !with_time.contains(&"pair_skew_exceeded"),
        "equal lengths should stay under a loose length limit"
    );
    let mut q = p.clone();
    q.board.diff_pairs[0].skew_limit_ps = Some(1.0);
    assert!(
        codes(&q).contains(&"pair_skew_exceeded"),
        "different layer permittivity should show up in the time limit"
    );
    q.board.diff_pairs[0].skew_limit_ps = None;
    q.board.diff_pairs[0].skew_limit_mm = Some(0.001);
    assert!(
        !codes(&q).contains(&"pair_skew_exceeded"),
        "the length limit should not fire when both sides match in millimetres"
    );
    Ok(())
}

#[test]
fn an_uncoupled_run_over_the_limit_is_reported() -> T {
    let mut p = Project::default();
    p.nets.push(net(1, "P"));
    p.nets.push(net(2, "N"));
    let mut dp = pair(9, 1, 2);
    dp.max_uncoupled_length = Some(5.0);
    p.board.diff_pairs.push(dp);
    p.board.tracks.push(track(
        10,
        1,
        Point::new(0.0, 0.0),
        Point::new(5.0, 0.0),
        Layer::Front,
    ));
    p.board.tracks.push(track(
        11,
        2,
        Point::new(0.0, 0.35),
        Point::new(5.0, 0.35),
        Layer::Front,
    ));
    p.board.tracks.push(track(
        12,
        2,
        Point::new(5.0, 0.35),
        Point::new(5.0, 12.0),
        Layer::Front,
    ));
    p.validate()?;
    assert!(codes(&p).contains(&"pair_uncoupled_too_long"));
    Ok(())
}
