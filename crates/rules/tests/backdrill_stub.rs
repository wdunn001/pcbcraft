//! The back-drill checks (issue #801): a stub longer than the limit and a
//! drill that cuts a layer it must not.

use pcbcraft_doc::{BackDrill, BackDrillFrom, Id, Layer, LayerRecord, Net, Project, Track, Via};
use pcbcraft_geom::Point;

type T = Result<(), String>;

const COPPER: f64 = 0.035;

fn board(n: u8, gaps: &[f64]) -> Result<Project, String> {
    let mut p = Project::default();
    p.nets.push(Net {
        id: 1,
        name: "N".into(),
        ..Default::default()
    });
    let mut records = vec![];
    for i in 0..n {
        records.push(LayerRecord {
            name: format!("L{}", i + 1),
            kind: "copper".into(),
            thickness: COPPER,
            ..Default::default()
        });
        if i + 1 < n {
            records.push(LayerRecord {
                name: format!("D{}", i + 1),
                kind: "prepreg".into(),
                thickness: gaps.get(i as usize).copied().unwrap_or(0.2),
                ..Default::default()
            });
        }
    }
    p.board.layer_records = records;
    Ok(p)
}

fn via(id: Id, span: Option<(u8, u8)>, spec: BackDrill) -> Result<Via, String> {
    Ok(Via {
        id,
        net: 1,
        position: Point::new(10.0, 10.0),
        diameter: 0.6,
        drill: 0.3,
        from_layer: span.map(|s| Layer::from_index(s.0)).transpose()?,
        to_layer: span.map(|s| Layer::from_index(s.1)).transpose()?,
        covering: None,
        backdrill: Some(spec),
    })
}

fn bottom(keep: u8, max_stub: f64) -> Result<BackDrill, String> {
    Ok(BackDrill {
        from: BackDrillFrom::Bottom,
        must_not_cut: Layer::from_index(keep)?,
        max_stub,
        diameter: 0.5,
    })
}

fn codes(p: &Project, code: &str) -> Vec<Id> {
    pcbcraft_rules::drc(p)
        .into_iter()
        .filter(|f| f.code == code)
        .flat_map(|f| f.objects)
        .collect()
}

#[test]
fn a_remaining_stub_over_the_limit_is_reported() -> T {
    // Layers 3 and 4 are 0.2 mm apart plus a 0.035 mm copper layer: 0.235 mm.
    let mut p = board(6, &[])?;
    p.board.vias.push(via(7, None, bottom(2, 0.2)?)?);
    assert_eq!(codes(&p, "backdrill_stub_too_long"), vec![7]);
    let message = pcbcraft_rules::drc(&p)
        .into_iter()
        .find(|f| f.code == "backdrill_stub_too_long")
        .map(|f| f.message)
        .ok_or("finding")?;
    assert!(
        message.contains("0.235 mm") && message.contains("0.200 mm"),
        "{message}"
    );
    p.board.vias[0].backdrill = Some(bottom(2, 0.25)?);
    assert!(
        codes(&p, "backdrill_stub_too_long").is_empty(),
        "clears when the limit is met"
    );
    p.board.vias[0].backdrill = None;
    assert!(
        codes(&p, "backdrill_stub_too_long").is_empty(),
        "no setting, no finding"
    );
    Ok(())
}

#[test]
fn a_backdrill_through_a_layer_the_net_uses_is_reported() -> T {
    let mut p = board(6, &[])?;
    p.board.vias.push(via(7, None, bottom(2, 1.0)?)?);
    assert!(codes(&p, "backdrill_must_not_cut").is_empty());
    p.board.tracks.push(Track {
        id: 8,
        net: 1,
        start: Point::new(10.0, 10.0),
        end: Point::new(15.0, 10.0),
        width: 0.2,
        layer: Layer::from_index(4)?,
        arc: None,
    });
    assert_eq!(codes(&p, "backdrill_must_not_cut"), vec![7, 8]);
    p.board.tracks[0].layer = Layer::from_index(1)?;
    assert!(
        codes(&p, "backdrill_must_not_cut").is_empty(),
        "a kept layer is fine"
    );
    // A span edited after the fact leaves a setting that no longer fits.
    p.board.vias[0].to_layer = Some(Layer::from_index(2)?);
    assert_eq!(codes(&p, "backdrill_must_not_cut"), vec![7]);
    Ok(())
}

#[test]
fn the_check_agrees_with_the_stackup_on_4_6_and_8_layers() -> T {
    for n in [4u8, 6, 8] {
        let gaps: Vec<f64> = (0..n - 1).map(|i| 0.08 + 0.05 * i as f64).collect();
        for keep in 0..n - 1 {
            let want = COPPER + gaps[keep as usize];
            for (limit, reported) in [(want - 0.001, true), (want, false), (want + 0.1, false)] {
                let mut p = board(n, &gaps)?;
                p.board.vias.push(via(7, None, bottom(keep, limit)?)?);
                assert_eq!(
                    !codes(&p, "backdrill_stub_too_long").is_empty(),
                    reported,
                    "{n} layers, keep {keep}, limit {limit}"
                );
            }
        }
    }
    Ok(())
}
