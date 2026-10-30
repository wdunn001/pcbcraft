//! Back-drill model (issue #801): validation, connectivity and the stub,
//! checked against a hand-written stackup oracle.

use pcbcraft_doc::{BackDrill, BackDrillFrom, Id, Layer, LayerRecord, Net, NetClass, Project, Via};
use pcbcraft_geom::Point;

type T = Result<(), String>;

const COPPER: f64 = 0.035;

/// A board with `n` copper layers; `gaps[i]` is the dielectric between copper i and i+1.
fn stack(n: u8, gaps: &[f64]) -> Result<Project, String> {
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
    p.validate()?;
    Ok(p)
}

fn via(id: Id, span: Option<(u8, u8)>) -> Result<Via, String> {
    Ok(Via {
        id,
        net: 1,
        position: Point::new(10.0 + id as f64, 10.0),
        diameter: 0.6,
        drill: 0.3,
        from_layer: span.map(|s| Layer::from_index(s.0)).transpose()?,
        to_layer: span.map(|s| Layer::from_index(s.1)).transpose()?,
        covering: None,
        covering_bottom: None,
        backdrill: None,
    })
}

fn spec(from: BackDrillFrom, keep: u8, max_stub: f64) -> Result<BackDrill, String> {
    Ok(BackDrill {
        from,
        must_not_cut: Layer::from_index(keep)?,
        max_stub,
        diameter: 0.5,
    })
}

#[test]
fn a_backdrill_must_stop_inside_the_via_span() -> T {
    let p = stack(6, &[])?;
    let v = via(5, Some((1, 4)))?;
    let ok = |s: BackDrill| p.validate_backdrill(&v, &s).is_ok();
    assert!(
        ok(spec(BackDrillFrom::Bottom, 3, 0.3)?),
        "one layer drilled"
    );
    assert!(
        ok(spec(BackDrillFrom::Bottom, 1, 0.3)?),
        "three layers drilled"
    );
    assert!(ok(spec(BackDrillFrom::Top, 4, 0.3)?));
    assert!(!ok(spec(BackDrillFrom::Bottom, 0, 0.3)?), "above the span");
    assert!(!ok(spec(BackDrillFrom::Bottom, 5, 0.3)?), "below the span");
    assert!(
        !ok(spec(BackDrillFrom::Bottom, 4, 0.3)?),
        "nothing left to drill"
    );
    assert!(
        !ok(spec(BackDrillFrom::Top, 1, 0.3)?),
        "nothing left to drill"
    );
    let mut wide = spec(BackDrillFrom::Bottom, 2, 0.3)?;
    wide.diameter = 0.3;
    assert!(!ok(wide), "not larger than the via hole");
    wide.diameter = f64::NAN;
    assert!(!ok(wide));
    let mut blind = v.clone();
    blind.drill = 0.0;
    assert!(
        p.validate_backdrill(&blind, &spec(BackDrillFrom::Bottom, 2, 0.3)?)
            .is_err(),
        "a via with no hole cannot be back-drilled"
    );
    Ok(())
}

#[test]
fn a_drilled_out_layer_is_not_connected_through_the_via() -> T {
    let mut p = stack(6, &[])?;
    let mut v = via(5, None)?;
    v.backdrill = Some(spec(BackDrillFrom::Bottom, 2, 0.5)?);
    p.board.vias.push(v.clone());
    let kept: Vec<u8> = p
        .via_conducting_layers(&v)
        .iter()
        .map(|l| l.index())
        .collect();
    assert_eq!(kept, vec![0, 1, 2], "layers 4 to 6 are drilled out");
    let all: Vec<u8> = p.via_span_layers(&v).iter().map(|l| l.index()).collect();
    assert_eq!(
        all,
        vec![0, 1, 2, 3, 4, 5],
        "the copper span itself is unchanged"
    );
    let members = p.net_members(1).ok_or("net")?;
    let on_layer_five = members
        .by_layer
        .iter()
        .find(|l| l.layer == 4)
        .map(|l| l.vias.contains(&5))
        .unwrap_or(false);
    assert_eq!(
        members.vias[0].layers,
        vec![0, 1, 2],
        "the via member lists only the kept layers"
    );
    assert!(
        !on_layer_five,
        "the net does not reach layer 5 through the via"
    );
    // A class default applies to a via with no setting of its own.
    p.board.vias[0].backdrill = None;
    p.net_classes.push(NetClass {
        id: 9,
        name: "fast".into(),
        clearance: 0.2,
        width: 0.2,
        min_width: None,
        max_width: None,
        target_length: None,
        length_tolerance: None,
        max_stub: None,
        via_diameter: None,
        via_drill: None,
        pair_width: None,
        pair_gap: None,
        nets: vec![1],
        profile: None,
        backdrill: Some(spec(BackDrillFrom::Bottom, 3, 0.5)?),
    });
    let v = p.board.vias[0].clone();
    assert_eq!(
        p.via_conducting_layers(&v).len(),
        4,
        "the class default drills layers 5 and 6"
    );
    Ok(())
}

#[test]
fn a_file_without_backdrills_saves_the_same_keys() -> T {
    let mut p = stack(4, &[])?;
    p.board.vias.push(via(5, None)?);
    let text = serde_json::to_string(&p).map_err(|e| e.to_string())?;
    assert!(!text.contains("backdrill"), "no key is written when unset");
    p.board.vias[0].backdrill = Some(spec(BackDrillFrom::Bottom, 2, 0.5)?);
    let text = serde_json::to_string(&p).map_err(|e| e.to_string())?;
    let back: Project = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    assert_eq!(back.board.vias[0].backdrill, p.board.vias[0].backdrill);
    Ok(())
}

/// Stub oracle: layers a..b apart cost b-a copper thicknesses plus the gaps between them.
fn oracle(gaps: &[f64], a: u8, b: u8) -> f64 {
    let n = (b - a) as f64 * COPPER;
    n + (a..b).map(|i| gaps[i as usize]).sum::<f64>()
}

/// Deterministic pseudo-random gap thicknesses, 0.05 to 0.45 mm.
fn gaps_for(n: u8, seed: u64) -> Vec<f64> {
    let mut x = seed
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    (0..n.saturating_sub(1))
        .map(|_| {
            x = x
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            0.05 + ((x >> 33) % 400) as f64 / 1000.0
        })
        .collect()
}

#[test]
fn the_stub_of_a_valid_backdrill_never_exceeds_the_limit_the_check_accepts() -> T {
    // Every span, direction and keep layer on 4, 6 and 8 layer boards. The
    // limit is chosen at the oracle stub, just under it and just over it:
    // the stub is within the limit exactly when the oracle says so.
    for n in [4u8, 6, 8] {
        for seed in 0..4 {
            let gaps = gaps_for(n, seed);
            let p = stack(n, &gaps)?;
            for a in 0..n {
                for b in (a + 1)..n {
                    for keep in a..=b {
                        for from in [BackDrillFrom::Top, BackDrillFrom::Bottom] {
                            let v = via(5, Some((a, b)))?;
                            let s = spec(from, keep, 1.0)?;
                            let valid = match from {
                                BackDrillFrom::Top => keep > a,
                                BackDrillFrom::Bottom => keep < b,
                            };
                            assert_eq!(p.validate_backdrill(&v, &s).is_ok(), valid);
                            if !valid {
                                continue;
                            }
                            let near = match from {
                                BackDrillFrom::Top => keep - 1,
                                BackDrillFrom::Bottom => keep + 1,
                            };
                            let want = oracle(&gaps, near.min(keep), near.max(keep));
                            let got = p.backdrill_stub(&v, &s).ok_or("stub")?;
                            assert!(
                                (got - want).abs() < 1e-9,
                                "n={n} {a}-{b} keep {keep}: {got} vs {want}"
                            );
                            for (limit, fits) in
                                [(want, true), (want - 0.01, false), (want + 0.01, true)]
                            {
                                let tighter = BackDrill {
                                    max_stub: limit,
                                    ..s
                                };
                                let stub = p.backdrill_stub(&v, &tighter).ok_or("stub")?;
                                assert_eq!(stub <= tighter.max_stub + 1e-9, fits);
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(())
}
