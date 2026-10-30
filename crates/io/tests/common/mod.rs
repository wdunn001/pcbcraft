//! Shared fixtures for back-drill export tests and the via protection and drill table tests.
#![allow(dead_code)]

use pcbcraft_doc::{
    BackDrill, BackDrillFrom, Component, Footprint, Hole, Id, Layer, LayerRecord, LibraryFootprint, MachiningNotes, Net, Pad, Pin, Project, Via, ViaCovering,
};
use pcbcraft_geom::Point;

/// An empty board with the given number of copper layers and one net.
pub fn board(copper_layers: u8) -> Project {
    let mut p = Project::default();
    p.board.width = 40.0;
};
use pcbcraft_geom::Point;

pub type T = Result<(), String>;

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
    covering: Option<ViaCovering>,
    top: Option<ViaCovering>,
    bottom: Option<ViaCovering>,
) -> Result<Via, String> {
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
        top,
        bottom,
        backdrill: None,
) -> Result<Via, String> {
    Ok(Via {
        id,
        net: 1,
        position: Point::new(x, 10.0),
        diameter: drill + 0.3,
        drill,
        from_layer: span.map(|s| Layer::from_index(s.0)).transpose()?,
        to_layer: span.map(|s| Layer::from_index(s.1)).transpose()?,
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
pub fn backdrill_board() -> Result<Project, String> {
    let mut p = board(6);
    p.board.vias = vec![
        via(20, 10.0, 0.3, None, None)?,
        via(21, 12.0, 0.3, None, None)?,
        via(22, 14.0, 0.3, None, None)?,
        via(23, 16.0, 0.3, None, None)?,
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
    p.validate()?;
    Ok(p)
}
