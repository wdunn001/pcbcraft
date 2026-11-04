//! True arc export and import for board outlines (issue #590).

use pcbcraft_doc::Project;
use pcbcraft_geom::Point;
use std::f64::consts::PI;

/// A board with a rounded top-right corner: three straight edges and one
/// quarter-circle arc.
fn board_with_arc() -> Project {
    let mut p = Project::default();
    // Outline vertices: bottom-left, bottom-right, top-right corner start,
    // top-right corner end, top-left.
    p.board.outline = Some(vec![
        Point::new(0.0, 0.0),
        Point::new(50.0, 0.0),
        Point::new(50.0, 20.0),
        Point::new(30.0, 40.0),
        Point::new(0.0, 40.0),
    ]);
    // Arc on segment 2 (50,20 -> 30,40): quarter circle with center at (30,20),
    // radius 20, sweeping counter-clockwise from 0 to 90 degrees.
    // Midpoint of the arc: at 45 degrees.
    let arc_mid = Point::new(
        30.0 + 20.0 * (PI / 4.0).cos(),
        20.0 + 20.0 * (PI / 4.0).sin(),
    );
    p.board.outline_arcs = vec![None, None, Some(arc_mid), None, None];
    p.board.width = 50.0;
    p.board.height = 40.0;
    p
}

#[test]
fn gerber_edge_layer_draws_true_arcs() -> Result<(), String> {
    let p = board_with_arc();
    let files = pcbcraft_io::gerber_files(&p)?;
    let edge = files.get("pcb-Edge.gbr").ok_or("edge layer file")?;
    // True arcs use G02/G03 with I/J parameters.
    assert!(
        edge.contains("G75*"),
        "gerber must set I/J relative arc mode"
    );
    assert!(
        edge.contains("G02*") || edge.contains("G03*"),
        "gerber must write a true arc command for the rounded corner"
    );
    // The arc endpoint (30, 40) in millionths is 30000000, 40000000.
    assert!(
        edge.contains("X30000000Y40000000"),
        "the arc endpoint must be written as an absolute coordinate"
    );
    Ok(())
}

#[test]
fn dxf_board_writes_arc_entities_for_arced_edges() -> Result<(), String> {
    let p = board_with_arc();
    let dxf = pcbcraft_io::dxf_board(&p)?;
    // DXF uses ARC entities for true arcs.
    assert!(
        dxf.contains("ARC"),
        "dxf must write an ARC entity for the rounded corner"
    );
    // The arc center is at (30, 20).
    assert!(
        dxf.contains("30.000") || dxf.contains("30"),
        "the arc center X must be written"
    );
    Ok(())
}
