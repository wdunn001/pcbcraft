//! The via-in-pad finding (issue #799): a via inside a pad must be filled
//! and capped, type VII.

use pcbcraft_doc::{Footprint, Id, Layer, LayerRecord, Net, Pad, Project, Via, ViaCovering};
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
    let geometry = pcbcraft_library::FootprintPad {
        number: "1".into(),
        pad_type: "smd".into(),
        shape: "circle".into(),
        position: Point::default(),
        rotation: 0.0,
        size: Point::new(1.6, 1.6),
        layers: vec!["F.Cu".into()],
        drill: None,
        roundrect_ratio: None,
        chamfer: None,
        polygon: vec![],
        raw_source: String::new(),
    };
    p.board.footprints.push(Footprint {
        id: 20,
        component: 0,
        position: Point::new(10.0, 10.0),
        rotation: 0.0,
        locked: None,
        side: None,
        pads: vec![Pad {
            number: "1".into(),
            offset: Point::default(),
            diameter: 1.6,
            net: Some(1),
            castellated: None,
            geometry: Some(geometry),
        }],
    });
    Ok(p)
}

fn via(id: Id, x: f64, covering: Option<ViaCovering>, bottom: Option<ViaCovering>) -> Via {
    Via {
        id,
        net: 1,
        position: Point::new(x, 10.0),
        diameter: 0.5,
        drill: 0.2,
        from_layer: None,
        to_layer: None,
        covering,
        covering_bottom: bottom,
    }
}

fn found(p: &Project) -> Vec<Id> {
    pcbcraft_rules::drc(p)
        .into_iter()
        .filter(|f| f.code == "via_in_pad_not_capped")
        .flat_map(|f| f.objects)
        .collect()
}

#[test]
fn a_via_inside_a_pad_must_be_filled_and_capped() -> T {
    let mut p = board(2)?;
    p.board.vias.push(via(30, 10.0, None, None));
    assert_eq!(found(&p), vec![30], "an open via in a pad is reported");
    p.board.vias[0].covering = Some(ViaCovering::Filled);
    assert_eq!(found(&p), vec![30], "filled alone is not enough");
    p.board.vias[0].covering = Some(ViaCovering::FilledCapped);
    assert!(found(&p).is_empty(), "type VII clears it");
    Ok(())
}

#[test]
fn a_via_beside_a_pad_or_buried_under_it_is_not_reported() -> T {
    let mut p = board(4)?;
    p.board.vias.push(via(31, 14.0, None, None));
    let mut buried = via(32, 10.0, None, None);
    buried.from_layer = Some(Layer::from_index(1)?);
    buried.to_layer = Some(Layer::from_index(2)?);
    p.board.vias.push(buried);
    assert!(found(&p).is_empty());
    Ok(())
}

#[test]
fn capping_one_face_with_the_other_left_open_still_counts_as_type_vii() -> T {
    let mut p = board(2)?;
    p.board.vias.push(via(
        33,
        10.0,
        Some(ViaCovering::FilledCapped),
        Some(ViaCovering::Open),
    ));
    assert!(found(&p).is_empty(), "type VII treats the whole hole");
    Ok(())
}
