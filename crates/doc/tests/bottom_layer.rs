//! Bottom always means the last copper layer, on boards of 2 to 8 layers (#821).
//!
//! A track on "B.Cu" of a four-layer board was reported as "In3.Cu", and
//! checks compared copper against `Layer::Back` (index 1), which is an inner
//! layer once the stackup has more than two copper rows.

use pcbcraft_doc::{Footprint, Layer, LayerRecord, Pad, Project};
use pcbcraft_geom::Point;

fn stackup(copper: u8) -> Project {
    let mut p = Project::default();
    if copper > 2 {
        for i in 0..copper {
            let name = match i {
                0 => "F.Cu".to_string(),
                i if i + 1 == copper => "B.Cu".to_string(),
                i => format!("In{i}.Cu"),
            };
            p.board.layer_records.push(LayerRecord {
                name,
                kind: "copper".into(),
                thickness: 0.035,
                ..LayerRecord::default()
            });
        }
    }
    p
}

fn part(thru: bool, side: Option<Layer>) -> Footprint {
    let geometry = pcbcraft_library::FootprintPad {
        number: "1".into(),
        pad_type: if thru { "thru_hole" } else { "smd" }.into(),
        shape: "circle".into(),
        position: Point::default(),
        rotation: 0.0,
        size: Point::new(1.6, 1.6),
        layers: vec![if thru { "*.Cu" } else { "F.Cu" }.into()],
        drill: None,
        roundrect_ratio: None,
        chamfer: None,
        polygon: vec![],
        raw_source: String::new(),
    };
    Footprint {
        id: 1,
        component: 0,
        position: Point::default(),
        rotation: 0.0,
        locked: None,
        side,
        pads: vec![Pad {
            number: "1".into(),
            offset: Point::default(),
            diameter: 1.6,
            net: Some(1),
            castellated: None,
            geometry: Some(geometry),
        }],
    }
}

#[test]
fn bottom_maps_to_the_last_copper_layer_on_2_4_6_and_8_layers() -> Result<(), String> {
    for count in [2u8, 4, 6, 8] {
        let p = stackup(count);
        assert_eq!(p.copper_layer_count(), count);
        let last = Layer::from_index(count - 1)?;
        assert_eq!(p.resolve_copper_layer("B.Cu")?, last, "{count} layers");
        assert_eq!(p.bottom_layer(), last, "{count} layers");
        assert!(p.is_bottom(last), "{count} layers");
        assert_eq!(p.layer_name(last), "B.Cu", "{count} layers");
        assert_eq!(p.face_layer(Layer::Back), last, "{count} layers");
        assert_eq!(p.face_layer(Layer::Front), Layer::Front);
        assert_eq!(p.resolve_copper_layer("F.Cu")?, Layer::Front);
        // Nothing but the last layer is the bottom, the first inner layer included.
        for i in 0..count - 1 {
            assert!(!p.is_bottom(Layer::from_index(i)?), "{count} layers, {i}");
        }
        // A via with no stated end runs down to the bottom.
        let via = pcbcraft_doc::Via {
            id: 9,
            net: 1,
            position: Point::default(),
            diameter: 0.6,
            drill: 0.3,
            from_layer: None,
            to_layer: None,
            covering: None,
            covering_bottom: None,
        };
        assert_eq!(p.via_span(&via), (Layer::Front, last), "{count} layers");
        assert_eq!(p.via_span_layers(&via).len(), count as usize);
    }
    Ok(())
}

#[test]
fn pads_reach_the_bottom_layer_by_index_on_every_stack() -> Result<(), String> {
    for count in [2u8, 4, 6, 8] {
        let p = stackup(count);
        let last = Layer::from_index(count - 1)?;
        let thru = part(true, None);
        let top_smd = part(false, None);
        let bottom_smd = part(false, Some(Layer::Back));
        for layer in p.copper_layers() {
            assert!(p.pad_on_copper(&thru, &thru.pads[0], layer));
            assert_eq!(
                p.pad_on_copper(&top_smd, &top_smd.pads[0], layer),
                layer == Layer::Front
            );
            assert_eq!(
                p.pad_on_copper(&bottom_smd, &bottom_smd.pads[0], layer),
                layer == last,
                "{count} layers, layer {}",
                layer.index()
            );
        }
    }
    Ok(())
}
