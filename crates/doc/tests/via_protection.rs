//! Via protection by face (issue #799): the saved shape of the side setting
//! and the twelve protection types, checked against an independent table.

use pcbcraft_doc::{
    Id, Layer, LayerRecord, Net, Project, Via, ViaCovering, ViaProtectionType,
    protection_for_faces, protection_type,
};
use pcbcraft_geom::Point;

type T = Result<(), String>;

fn board(copper_layers: u8) -> Result<Project, String> {
    let mut p = Project::default();
    p.nets.push(Net {
        id: 1,
        name: "N".into(),
        ..Default::default()
    });
    p.board.layer_records = (0..copper_layers)
        .map(|i| LayerRecord {
            name: format!("L{}", i + 1),
            kind: "copper".into(),
            thickness: 0.035,
            copper_role: None,
            ..Default::default()
        })
        .collect();
    p.validate()?;
    Ok(p)
}

fn via(id: Id, from: Option<u8>, to: Option<u8>) -> Result<Via, String> {
    Ok(Via {
        id,
        net: 1,
        position: Point::new(10.0 + id as f64, 10.0),
        diameter: 0.6,
        drill: 0.3,
        from_layer: from.map(Layer::from_index).transpose()?,
        to_layer: to.map(Layer::from_index).transpose()?,
        covering: None,
        covering_bottom: None,
    })
}

#[test]
fn a_file_saved_before_the_side_setting_loads_with_the_same_covering() -> T {
    let mut p = board(2)?;
    let mut v = via(5, None, None)?;
    v.covering = Some(ViaCovering::Tented);
    p.board.vias.push(v);
    let mut text = serde_json::to_value(&p).map_err(|e| e.to_string())?;
    let saved = text["board"]["vias"][0]
        .as_object_mut()
        .ok_or("the via should save as an object")?;
    assert!(
        !saved.contains_key("covering_bottom"),
        "a via with no side setting writes the same keys it always wrote"
    );
    let old = serde_json::to_string(&text).map_err(|e| e.to_string())?;
    let back: Project = serde_json::from_str(&old).map_err(|e| e.to_string())?;
    let via = &back.board.vias[0];
    assert_eq!(via.covering, Some(ViaCovering::Tented));
    assert_eq!(via.covering_bottom, None);
    assert_eq!(via.effective_covering(&back.board), ViaCovering::Tented);
    assert_eq!(
        via.effective_covering_bottom(&back.board),
        ViaCovering::Tented,
        "an old file protects the bottom exactly as the top"
    );
    assert_eq!(back.via_protection_type(via), ViaProtectionType::Ib);
    Ok(())
}

#[test]
fn a_bottom_setting_round_trips_and_a_hole_less_via_cannot_carry_one() -> T {
    let mut p = board(2)?;
    let mut v = via(5, None, None)?;
    v.covering = Some(ViaCovering::Tented);
    v.covering_bottom = Some(ViaCovering::Open);
    p.board.vias.push(v);
    let text = serde_json::to_string(&p).map_err(|e| e.to_string())?;
    let back: Project = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    assert_eq!(back.board.vias[0].covering_bottom, Some(ViaCovering::Open));
    assert_eq!(
        back.via_protection_type(&back.board.vias[0]),
        ViaProtectionType::Ia,
        "tented on top and open underneath is the one-sided type"
    );
    let mut bad = back;
    bad.board.vias[0].drill = 0.0;
    bad.board.vias[0].covering = None;
    bad.board.vias[0].covering_bottom = Some(ViaCovering::Plugged);
    assert!(bad.validate().is_err(), "no hole, nothing to plug");
    Ok(())
}

/// The specification's table, written out by hand: the type for a pair of
/// coverings when both faces are reached. Independent of the code under test.
fn oracle(top: ViaCovering, bottom: ViaCovering) -> &'static str {
    use ViaCovering::*;
    let rank = |c| ViaCovering::ALL.iter().position(|x| *x == c).unwrap_or(0);
    let strong = if rank(top) >= rank(bottom) {
        top
    } else {
        bottom
    };
    let weak = if rank(top) >= rank(bottom) {
        bottom
    } else {
        top
    };
    let one = weak == Open;
    match (strong, one) {
        (Open, _) => "None",
        (Tented, true) => "Ia",
        (Tented, false) => "Ib",
        (TentedCovered, true) => "IIa",
        (TentedCovered, false) => "IIb",
        (Plugged, true) => "IIIa",
        (Plugged, false) => "IIIb",
        (PluggedCovered, true) => "IVa",
        (PluggedCovered, false) => "IVb",
        (Filled, _) => "V",
        (FilledCovered, _) => "VI",
        (FilledCapped, _) => "VII",
    }
}

#[test]
fn every_pair_of_coverings_gives_the_type_the_table_says() {
    for top in ViaCovering::ALL {
        for bottom in ViaCovering::ALL {
            assert_eq!(
                protection_type(top, bottom).gerber_token(),
                oracle(top, bottom),
                "top {top:?}, bottom {bottom:?}"
            );
            assert_eq!(
                protection_type(top, bottom),
                protection_type(bottom, top),
                "the type does not depend on which face is which"
            );
        }
    }
}

#[test]
fn the_twelve_tokens_are_distinct_and_all_reachable() {
    let mut seen: Vec<&str> = ViaProtectionType::ALL
        .iter()
        .map(|t| t.gerber_token())
        .collect();
    seen.sort_unstable();
    seen.dedup();
    assert_eq!(seen.len(), 12);
    let reached: std::collections::BTreeSet<_> = ViaCovering::ALL
        .iter()
        .flat_map(|t| ViaCovering::ALL.iter().map(|b| protection_type(*t, *b)))
        .collect();
    assert_eq!(
        reached.len(),
        12,
        "every type can be produced from the picker"
    );
}

#[test]
fn a_via_that_reaches_one_face_is_judged_on_that_face_alone() {
    for c in ViaCovering::ALL {
        for other in ViaCovering::ALL {
            assert_eq!(
                protection_for_faces(true, false, c, other),
                protection_for_faces(true, false, c, ViaCovering::Open)
            );
            assert_eq!(
                protection_for_faces(false, true, other, c),
                protection_for_faces(true, false, c, other)
            );
            assert_eq!(
                protection_for_faces(false, false, c, other),
                ViaProtectionType::None
            );
        }
    }
}

#[test]
fn the_project_type_follows_the_span_on_two_four_and_six_layers() -> T {
    for layers in [2u8, 4, 6] {
        let last = layers - 1;
        let mut spans: Vec<(Option<u8>, Option<u8>, bool, bool)> = vec![(None, None, true, true)];
        if layers > 2 {
            spans.push((Some(0), Some(1), true, false));
            spans.push((Some(layers - 2), Some(last), false, true));
            spans.push((Some(1), Some(layers - 2), false, false));
        }
        for (from, to, top_reached, bottom_reached) in spans {
            for top in ViaCovering::ALL {
                for bottom in ViaCovering::ALL {
                    let mut p = board(layers)?;
                    let mut v = via(5, from, to)?;
                    v.covering = Some(top);
                    v.covering_bottom = Some(bottom);
                    p.board.vias.push(v);
                    let got = p.via_protection_type(&p.board.vias[0]).gerber_token();
                    let want = match (top_reached, bottom_reached) {
                        (true, true) => oracle(top, bottom),
                        (true, false) => oracle(top, ViaCovering::Open),
                        (false, true) => oracle(bottom, ViaCovering::Open),
                        (false, false) => "None",
                    };
                    assert_eq!(
                        got, want,
                        "{layers} layers, span {from:?}-{to:?}, top {top:?}, bottom {bottom:?}"
                    );
                }
            }
        }
    }
    Ok(())
}
