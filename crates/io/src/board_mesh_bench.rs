//! Synthetic board with six hundred identical placed parts for 3D render benchmarks.

use pcbcraft_doc::{Component, Footprint, Id, Net, Pad, Project};
use pcbcraft_geom::Point;
use pcbcraft_library::FootprintDefinition;

const PARTS: usize = 600;
const COLS: usize = 30;
const PITCH_MM: f64 = 6.0;
const ORIGIN_X: f64 = 8.0;
const ORIGIN_Y: f64 = 8.0;

fn chip_pad(number: &str, x: f64, y: f64) -> Pad {
    Pad {
        number: number.into(),
        offset: Point::new(x, y),
        geometry: Some(pcbcraft_library::FootprintPad {
            number: number.into(),
            pad_type: "smd".into(),
            shape: "roundrect".into(),
            position: Point::new(x, y),
            rotation: 0.0,
            size: Point::new(1.025, 1.4),
            layers: vec!["F.Cu".into()],
            drill: None,
            roundrect_ratio: None,
            raw_source: String::new(),
            chamfer: None,
            polygon: Vec::new(),
        }),
    }
}

/// A square board carrying six hundred identical two-pad footprints in a grid.
pub fn bench_project_600_parts() -> Project {
    let mut p = Project::default();
    p.board.width = ORIGIN_X * 2.0 + PITCH_MM * (COLS as f64 - 1.0) + 8.0;
    p.board.height = ORIGIN_Y * 2.0 + PITCH_MM * ((PARTS / COLS) as f64 - 1.0) + 8.0;
    p.board.thickness = 1.6;
    p.nets.push(Net {
        id: 1,
        name: "GND".into(),
        ..Net::default()
    });
    let definition = FootprintDefinition {
        name: "R_0805".into(),
        description: String::new(),
        keywords: String::new(),
        pads: vec![
            chip_pad("1", -0.9, 0.0).geometry.clone().unwrap(),
            chip_pad("2", 0.9, 0.0).geometry.clone().unwrap(),
        ],
        graphics: vec![],
        model: None,
        rules: None,
        raw_source: String::new(),
        unsupported: vec![],
    };
    for index in 0..PARTS {
        let row = index / COLS;
        let col = index % COLS;
        let component_id: Id = 100 + index as u64;
        let footprint_id: Id = 10_000 + index as u64;
        let reference = format!("R{}", index + 1);
        p.components.push(Component {
            id: component_id,
            reference: reference.clone(),
            value: "10k".into(),
            kind: "resistor".into(),
            position: Point::new(0.0, 0.0),
            rotation: 0.0,
            pins: vec![
                pcbcraft_doc::Pin {
                    number: "1".into(),
                    name: "1".into(),
                    offset: Point::new(0.0, 0.0),
                    net: Some(1),
                    electrical_type: "passive".into(),
                    direction: None,
                    unit: 1,
                },
                pcbcraft_doc::Pin {
                    number: "2".into(),
                    name: "2".into(),
                    offset: Point::new(0.0, 0.0),
                    net: None,
                    electrical_type: "passive".into(),
                    direction: None,
                    unit: 1,
                },
            ],
            units: vec![],
            library_symbol: None,
            library_footprint: Some(pcbcraft_doc::LibraryFootprint {
                library: "bench".into(),
                definition: definition.clone(),
            }),
            fields: vec![],
            sheet: 0,
        });
        p.board.footprints.push(Footprint {
            id: footprint_id,
            component: component_id,
            position: Point::new(
                ORIGIN_X + PITCH_MM * col as f64,
                ORIGIN_Y + PITCH_MM * row as f64,
            ),
            rotation: 0.0,
            locked: None,
            side: None,
            pads: vec![chip_pad("1", -0.9, 0.0), chip_pad("2", 0.9, 0.0)],
        });
    }
    p
}
