//! Shared fixtures for the via protection and drill table tests.
#![allow(dead_code)]

use pcbcraft_doc::{
    Component, Footprint, Hole, Id, Layer, LayerRecord, LibraryFootprint, MachiningNotes, Net, Pad,
    Pin, Project, Via, ViaCovering,
};
use pcbcraft_geom::Point;

pub type T = Result<(), String>;

/// An empty board with the given number of copper layers and one net.
pub fn board(copper_layers: u8) -> Project {
    let mut p = Project::default();
    p.board.width = 40.0;
    p.board.height = 30.0;
    p.nets.push(Net {
        id: 1,
        name: "N".into(),
        ..Default::default()
    });
    if copper_layers > 2 {
        p.board.layer_records = (0..copper_layers)
            .map(|i| LayerRecord {
                name: format!("L{}", i + 1),
                kind: "copper".into(),
                thickness: 0.035,
                copper_role: None,
                ..Default::default()
            })
            .collect();
    }
    p
}

/// One via at `(x, 10)` with the given span (copper layer indices, zero is the top).
pub fn via(
    id: Id,
    x: f64,
    drill: f64,
    span: Option<(u8, u8)>,
    top: Option<ViaCovering>,
    bottom: Option<ViaCovering>,
) -> Result<Via, String> {
    Ok(Via {
        id,
        net: 1,
        position: Point::new(x, 10.0),
        diameter: drill + 0.3,
        drill,
        from_layer: span.map(|s| Layer::from_index(s.0)).transpose()?,
        to_layer: span.map(|s| Layer::from_index(s.1)).transpose()?,
        covering: top,
        covering_bottom: bottom,
        backdrill: None,
    })
}

/// The library pad of a through-hole part, drilled `drill` wide.
pub fn thru_geometry(drill: f64) -> pcbcraft_library::FootprintPad {
    pcbcraft_library::FootprintPad {
        number: "1".into(),
        pad_type: "thru_hole".into(),
        shape: "circle".into(),
        position: Point::default(),
        rotation: 0.0,
        size: Point::new(1.6, 1.6),
        layers: vec!["*.Cu".into()],
        drill: Some(pcbcraft_library::Drill {
            size: Point::new(drill, drill),
            offset: Point::default(),
        }),
        roundrect_ratio: None,
        chamfer: None,
        polygon: vec![],
        raw_source: String::new(),
    }
}

/// A through-hole part with one plated pad drilled `drill` wide.
pub fn thru_part(id: Id, x: f64, y: f64, drill: f64) -> Footprint {
    let geometry = thru_geometry(drill);
    Footprint {
        id,
        component: id + 10,
        position: Point::new(x, y),
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
    }
}

/// A mounting hole.
pub fn hole(id: Id, x: f64, y: f64, diameter: f64) -> Hole {
    Hole {
        id,
        position: Point::new(x, y),
        diameter,
        end: None,
    }
}

/// The board the golden files were made from: four copper layers; through
/// vias of four protections, a blind and a buried via, one mounting hole, one
/// component pad hole and a recorded drill tolerance.
pub fn golden_board() -> Result<Project, String> {
    use ViaCovering::*;
    let mut p = board(4);
    p.board.vias = vec![
        via(10, 10.0, 0.3, None, Some(Tented), None)?,
        via(11, 12.0, 0.3, None, Some(Tented), Some(Open))?,
        via(12, 14.0, 0.3, None, Some(FilledCapped), None)?,
        via(13, 16.0, 0.2, None, None, None)?,
        via(14, 18.0, 0.15, Some((0, 1)), Some(Tented), None)?,
        via(15, 20.0, 0.15, Some((1, 2)), None, None)?,
    ];
    p.board.holes = vec![hole(40, 5.0, 5.0, 3.2)];
    p.components = vec![Component {
        id: 60,
        reference: "J1".into(),
        value: "header".into(),
        kind: "connector".into(),
        position: Point::default(),
        rotation: 0.0,
        pins: vec![Pin {
            number: "1".into(),
            name: "1".into(),
            offset: Point::default(),
            net: Some(1),
            electrical_type: "passive".into(),
            direction: None,
            unit: 1,
        }],
        units: vec![],
        library_symbol: None,
        library_footprint: Some(LibraryFootprint {
            library: "Test".into(),
            definition: pcbcraft_library::FootprintDefinition {
                name: "HEADER_ONE".into(),
                description: String::new(),
                keywords: String::new(),
                pads: vec![thru_geometry(0.8)],
                graphics: vec![],
                model: None,
                rules: None,
                raw_source: String::new(),
                unsupported: vec![],
            },
        }),
        fields: vec![],
        sheet: 0,
    }];
    p.board.footprints = vec![thru_part(50, 30.0, 20.0, 0.8)];
    p.board.machining = Some(MachiningNotes {
        drill_tolerance: Some(0.05),
        ..Default::default()
    });
    p.validate()?;
    Ok(p)
}

/// Compare `actual` with `tests/golden/<name>`. Set `PCBCRAFT_UPDATE_GOLDEN=1`
/// to rewrite the file after a change has been read and accepted.
pub fn golden(name: &str, actual: &str) -> T {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join(name);
    if std::env::var_os("PCBCRAFT_UPDATE_GOLDEN").is_some() {
        std::fs::write(&path, actual).map_err(|e| format!("{}: {e}", path.display()))?;
        return Ok(());
    }
    let want = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    if want == actual {
        Ok(())
    } else {
        Err(format!(
            "{name} differs from its golden file.\n--- golden ---\n{want}\n--- actual ---\n{actual}"
        ))
    }
}

/// A six-layer board for the back-drill tests: two vias back-drilled from the
/// bottom (keeping layer 4), one from the top (keeping layer 3, a larger
/// drill) and one plain through via.
pub fn backdrill_board() -> Result<Project, String> {
    use pcbcraft_doc::{BackDrill, BackDrillFrom};
    let mut p = board(6);
    p.board.vias = vec![
        via(20, 10.0, 0.3, None, None, None)?,
        via(21, 12.0, 0.3, None, None, None)?,
        via(22, 14.0, 0.3, None, None, None)?,
        via(23, 16.0, 0.3, None, None, None)?,
    ];
    let bottom = BackDrill {
        from: BackDrillFrom::Bottom,
        must_not_cut: Layer::from_index(3)?,
        max_stub: 0.4,
        diameter: 0.5,
    };
    p.board.vias[0].backdrill = Some(bottom);
    p.board.vias[1].backdrill = Some(bottom);
    p.board.vias[2].backdrill = Some(BackDrill {
        from: BackDrillFrom::Top,
        must_not_cut: Layer::from_index(2)?,
        max_stub: 0.4,
        diameter: 0.6,
    });
    Ok(p)
}
