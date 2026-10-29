//! NeoDen YY1 pick-and-place placement file and feeder sheet tests.
use pcbcraft_doc::Project;
use pcbcraft_doc::Footprint;
use pcbcraft_doc::Pad;
use pcbcraft_doc::Component;
use pcbcraft_doc::Variant;
use pcbcraft_geom::Point;

fn make_project() -> Project {
    let mut p = Project::default();
    p.name = "test-board".into();
    p.board.width = 100.0;
    p.board.height = 50.0;
    p
}

fn make_top_footprint(id: u64, component_id: u64, x: f64, y: f64, rot: f64) -> Footprint {
    Footprint {
        id,
        component: component_id,
        position: Point::new(x, y),
        rotation: rot,
        locked: None,
        pads: vec![Pad {
            number: "1".into(),
            offset: Point::new(0.0, 0.0),
            diameter: 1.0,
            net: None,
            castellated: None,
            geometry: None,
        }],
    }
}

fn make_bottom_footprint(id: u64, component_id: u64, x: f64, y: f64, rot: f64) -> Footprint {
    Footprint {
        id,
        component: component_id,
        position: Point::new(x, y),
        rotation: rot,
        locked: None,
        pads: vec![Pad {
            number: "1".into(),
            offset: Point::new(0.0, 0.0),
            diameter: 1.0,
            net: None,
            castellated: None,
            geometry: None,
        }],
    }
}

fn make_tht_footprint(id: u64, component_id: u64, x: f64, y: f64, rot: f64) -> Footprint {
    Footprint {
        id,
        component: component_id,
        position: Point::new(x, y),
        rotation: rot,
        locked: None,
        pads: vec![Pad {
            number: "1".into(),
            offset: Point::new(0.0, 0.0),
            diameter: 1.0,
            net: None,
            castellated: None,
            geometry: None,
        }],
    }
}

fn make_component(id: u64, ref_name: &str, kind: &str) -> Component {
    Component {
        id,
        reference: ref_name.into(),
        value: kind.into(),
        kind: kind.into(),
        position: Point::new(0.0, 0.0),
        rotation: 0.0,
        pins: vec![],
        units: vec![],
        library_symbol: None,
        library_footprint: None,
        fields: vec![],
        sheet: 0,
    }
}

#[test]
fn native_layout_has_preamble_and_placement_rows() -> Result<(), String> {
    let mut p = make_project();
    p.components.push(make_component(1, "U1", "IC"));
    p.board.footprints.push(make_top_footprint(100, 1, 10.0, 10.0, 0.0));
    p.validate()?;

    let result = pcbcraft_io::placement_neoden_yy1(&p, pcbcraft_io::PlacementLayout::Native, false)?;

    // Check preamble
    assert!(result.starts_with("NEODEN,YY1,P&P FILE\n"));
    assert!(result.contains("ProjectName,\"test-board\""));
    assert!(result.contains("BoardSize,\"100.0x50.0\""));
    assert!(result.contains("NozzleChange,1,0"));
    assert!(result.contains("NozzleChange,2,0"));
    assert!(result.contains("NozzleChange,3,0"));
    assert!(result.contains("NozzleChange,4,0"));
    assert!(result.contains("PlacementStart"));
    assert!(result.ends_with("PlacementEnd\n"));

    // Check data row
    assert!(result.contains("U1,IC,IC,1,10.00,10.00,0.00,T,0,10.00,10.00,10.00,10.00"));

    Ok(())
}

#[test]
fn import_layout_has_11_columns() -> Result<(), String> {
    let mut p = make_project();
    p.components.push(make_component(1, "U1", "IC"));
    p.board.footprints.push(make_top_footprint(100, 1, 10.0, 10.0, 0.0));
    p.validate()?;

    let result = pcbcraft_io::placement_neoden_yy1(&p, pcbcraft_io::PlacementLayout::Import, false)?;

    // Check header
    assert!(result.starts_with("Ref,Comment,Footprint,Qty,X,Y,Rotation,Side,NozzleChange,RefX,RefY\n"));

    // Check data row
    assert!(result.contains("U1,IC,IC,1,10.00,10.00,0.00,T,0,10.00,10.00"));

    Ok(())
}

#[test]
fn bottom_side_x_mirrored() -> Result<(), String> {
    let mut p = make_project();
    p.components.push(make_component(1, "U1", "IC"));
    p.board.footprints.push(make_bottom_footprint(100, 1, 20.0, 15.0, 0.0));
    p.validate()?;

    let result = pcbcraft_io::placement_neoden_yy1(&p, pcbcraft_io::PlacementLayout::Import, false)?;

    // Bottom-side X should be mirrored: 100.0 - 20.0 = 80.0
    assert!(result.contains("U1,IC,IC,1,80.00,15.00,0.00,B,0,80.00,15.00"));

    Ok(())
}

#[test]
fn bottom_side_rotation_negated() -> Result<(), String> {
    let mut p = make_project();
    p.components.push(make_component(1, "U1", "IC"));
    p.board.footprints.push(make_bottom_footprint(100, 1, 10.0, 10.0, 90.0));
    p.validate()?;

    let result = pcbcraft_io::placement_neoden_yy1(&p, pcbcraft_io::PlacementLayout::Import, false)?;

    // Bottom-side rotation should be negated: 360 - 90 = 270
    assert!(result.contains("U1,IC,IC,1,90.00,10.00,270.00,B,0,90.00,10.00"));

    Ok(())
}

#[test]
fn rotation_normalized_to_0_360() -> Result<(), String> {
    let mut p = make_project();
    p.components.push(make_component(1, "U1", "IC"));
    p.board.footprints.push(make_top_footprint(100, 1, 10.0, 10.0, 450.0));
    p.validate()?;

    let result = pcbcraft_io::placement_neoden_yy1(&p, pcbcraft_io::PlacementLayout::Import, false)?;

    // 450 degrees should normalize to 90
    assert!(result.contains("U1,IC,IC,1,10.00,10.00,90.00,T,0,10.00,10.00"));

    Ok(())
}

#[test]
fn through_hole_parts_omitted() -> Result<(), String> {
    let mut p = make_project();
    p.components.push(make_component(1, "U1", "IC"));
    p.components.push(make_component(2, "J1", "Connector"));
    p.board.footprints.push(make_top_footprint(100, 1, 10.0, 10.0, 0.0));
    p.board.footprints.push(make_tht_footprint(101, 2, 20.0, 20.0, 0.0));
    p.validate()?;

    let result = pcbcraft_io::placement_neoden_yy1(&p, pcbcraft_io::PlacementLayout::Import, false)?;

    // SMT part should be present
    assert!(result.contains("U1"));
    // THT part should be omitted
    assert!(!result.contains("J1"));

    Ok(())
}

#[test]
fn dnp_parts_skipped_by_default() -> Result<(), String> {
    let mut p = make_project();
    p.components.push(make_component(1, "U1", "IC"));
    p.components.push(make_component(2, "U2", "IC"));
    p.board.footprints.push(make_top_footprint(100, 1, 10.0, 10.0, 0.0));
    p.board.footprints.push(make_top_footprint(101, 2, 20.0, 20.0, 0.0));

    // Create variant with U2 as DNP
    p.variants.push(Variant {
        name: "production".into(),
        note: None,
        dnp: vec![2],
        footprints: vec![],
    });
    p.validate()?;

    let result = pcbcraft_io::placement_neoden_yy1(&p, pcbcraft_io::PlacementLayout::Import, false)?;

    // U1 should be present
    assert!(result.contains("U1"));
    // U2 should be skipped (DNP)
    assert!(!result.contains("U2"));

    Ok(())
}

#[test]
fn dnp_parts_included_when_requested() -> Result<(), String> {
    let mut p = make_project();
    p.components.push(make_component(1, "U1", "IC"));
    p.components.push(make_component(2, "U2", "IC"));
    p.board.footprints.push(make_top_footprint(100, 1, 10.0, 10.0, 0.0));
    p.board.footprints.push(make_top_footprint(101, 2, 20.0, 20.0, 0.0));

    // Create variant with U2 as DNP
    p.variants.push(Variant {
        name: "production".into(),
        note: None,
        dnp: vec![2],
        footprints: vec![],
    });
    p.validate()?;

    let result = pcbcraft_io::placement_neoden_yy1(&p, pcbcraft_io::PlacementLayout::Import, true)?;

    // Both parts should be present when include_dnp is true
    assert!(result.contains("U1"));
    assert!(result.contains("U2"));

    Ok(())
}

#[test]
fn board_size_validation_width() -> Result<(), String> {
    let mut p = make_project();
    p.board.width = 400.0; // Exceeds 350 mm limit
    p.components.push(make_component(1, "U1", "IC"));
    p.board.footprints.push(make_top_footprint(100, 1, 10.0, 10.0, 0.0));
    p.validate()?;

    let result = pcbcraft_io::placement_neoden_yy1(&p, pcbcraft_io::PlacementLayout::Import, false);

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.contains("board width"));
    assert!(err.contains("350.0"));

    Ok(())
}

#[test]
fn board_size_validation_height() -> Result<(), String> {
    let mut p = make_project();
    p.board.height = 300.0; // Exceeds 249 mm limit
    p.components.push(make_component(1, "U1", "IC"));
    p.board.footprints.push(make_top_footprint(100, 1, 10.0, 10.0, 0.0));
    p.validate()?;

    let result = pcbcraft_io::placement_neoden_yy1(&p, pcbcraft_io::PlacementLayout::Import, false);

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.contains("board height"));
    assert!(err.contains("249.0"));

    Ok(())
}

#[test]
fn feeder_sheet_groups_by_footprint() -> Result<(), String> {
    let mut p = make_project();
    p.components.push(make_component(1, "R1", "Resistor"));
    p.components.push(make_component(2, "R2", "Resistor"));
    p.components.push(make_component(3, "C1", "Capacitor"));
    p.board.footprints.push(make_top_footprint(100, 1, 10.0, 10.0, 0.0));
    p.board.footprints.push(make_top_footprint(101, 2, 20.0, 20.0, 0.0));
    p.board.footprints.push(make_top_footprint(102, 3, 30.0, 30.0, 0.0));
    p.validate()?;

    let result = pcbcraft_io::feeder_sheet_neoden_yy1(&p)?;

    // Should have header
    assert!(result.starts_with("FeederNo,Comment,Footprint,TapeWidth_mm,Nozzle,Quantity,Load\n"));

    // Should have two groups: Resistors (qty 2) and Capacitor (qty 1)
    assert!(result.contains("Resistor"));
    assert!(result.contains("Capacitor"));
    assert!(result.contains("2")); // quantity of resistors

    Ok(())
}

#[test]
fn feeder_sheet_omits_through_hole() -> Result<(), String> {
    let mut p = make_project();
    p.components.push(make_component(1, "R1", "Resistor"));
    p.components.push(make_component(2, "J1", "Connector"));
    p.board.footprints.push(make_top_footprint(100, 1, 10.0, 10.0, 0.0));
    p.board.footprints.push(make_tht_footprint(101, 2, 20.0, 20.0, 0.0));
    p.validate()?;

    let result = pcbcraft_io::feeder_sheet_neoden_yy1(&p)?;

    // SMT part should be present
    assert!(result.contains("Resistor"));
    // THT part should be omitted
    assert!(!result.contains("Connector"));

    Ok(())
}

#[test]
fn feeder_sheet_omits_dnp() -> Result<(), String> {
    let mut p = make_project();
    p.components.push(make_component(1, "R1", "Resistor"));
    p.components.push(make_component(2, "R2", "Resistor"));
    p.board.footprints.push(make_top_footprint(100, 1, 10.0, 10.0, 0.0));
    p.board.footprints.push(make_top_footprint(101, 2, 20.0, 20.0, 0.0));

    // Create variant with R2 as DNP
    p.variants.push(Variant {
        name: "production".into(),
        note: None,
        dnp: vec![2],
        footprints: vec![],
    });
    p.validate()?;

    let result = pcbcraft_io::feeder_sheet_neoden_yy1(&p)?;

    // Only one resistor should be counted (R1)
    assert!(result.contains("Resistor"));
    // Quantity should be 1, not 2
    assert!(result.contains("1\n"));

    Ok(())
}

#[test]
fn coordinates_are_two_decimal_places() -> Result<(), String> {
    let mut p = make_project();
    p.components.push(make_component(1, "U1", "IC"));
    p.board.footprints.push(make_top_footprint(100, 1, 10.123, 15.456, 0.0));
    p.validate()?;

    let result = pcbcraft_io::placement_neoden_yy1(&p, pcbcraft_io::PlacementLayout::Import, false)?;

    // Should be rounded to 2 decimal places
    assert!(result.contains("10.12"));
    assert!(result.contains("15.46"));

    Ok(())
}

#[test]
fn multiple_parts_sorted_by_reference() -> Result<(), String> {
    let mut p = make_project();
    p.components.push(make_component(1, "U2", "IC"));
    p.components.push(make_component(2, "U1", "IC"));
    p.components.push(make_component(3, "R1", "Resistor"));
    p.board.footprints.push(make_top_footprint(100, 1, 30.0, 30.0, 0.0));
    p.board.footprints.push(make_top_footprint(101, 2, 20.0, 20.0, 0.0));
    p.board.footprints.push(make_top_footprint(102, 3, 10.0, 10.0, 0.0));
    p.validate()?;

    let result = pcbcraft_io::placement_neoden_yy1(&p, pcbcraft_io::PlacementLayout::Import, false)?;

    // Find positions of each reference in the output
    let r1_pos = result.find("R1").unwrap();
    let u1_pos = result.find("U1").unwrap();
    let u2_pos = result.find("U2").unwrap();

    // Should be sorted: R1, U1, U2
    assert!(r1_pos < u1_pos);
    assert!(u1_pos < u2_pos);

    Ok(())
}
