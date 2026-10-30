use pcbcraft_doc::{
    Footprint, Net, Pad, Point, Project, Via, ViaCovering, component_template,
};
use pcbcraft_library::FootprintPad;
use pcbcraft_rules::drc;

fn via_in_pad_board(covering: ViaCovering) -> Result<Project, String> {
    let mut p = Project::default();
    p.nets.push(Net {
        id: 1,
        name: "SIG".into(),
        ..Default::default()
    });
    let component = component_template("resistor", 1, "U1".into(), Point::new(20.0, 20.0))
        .ok_or("component template")?;
    p.components.push(component);
    p.board.footprints.push(Footprint {
        id: 10,
        component: 1,
        position: Point::new(20.0, 20.0),
        rotation: 0.0,
        locked: None,
        side: None,
        pads: vec![Pad {
            number: "1".into(),
            offset: Point::default(),
            net: Some(1),
            diameter: 1.0,
            castellated: None,
            geometry: Some(FootprintPad {
                number: "1".into(),
                shape: "circle".into(),
                pad_type: "smd".into(),
                position: Point::default(),
                rotation: 0.0,
                size: Point::new(1.2, 1.2),
                layers: vec!["F.Cu".into(), "F.Mask".into()],
                drill: None,
                roundrect_ratio: None,
                chamfer: None,
                polygon: vec![],
                raw_source: String::new(),
            }),
        }],
    });
    p.board.vias.push(Via {
        id: 12,
        net: 1,
        position: Point::new(20.0, 20.0),
        diameter: 0.5,
        drill: 0.2,
        from_layer: None,
        to_layer: None,
        covering: Some(covering),
        covering_scope: None,
    });
    p.validate()?;
    Ok(p)
}

#[test]
fn a_via_inside_a_pad_must_be_filled_and_capped() -> Result<(), String> {
    let bad = via_in_pad_board(ViaCovering::Tented)?;
    assert!(
        drc(&bad)
            .iter()
            .any(|f| f.code == "via_in_pad_not_capped"),
        "tented via in pad is flagged"
    );
    let good = via_in_pad_board(ViaCovering::FilledCapped)?;
    assert!(
        !drc(&good)
            .iter()
            .any(|f| f.code == "via_in_pad_not_capped"),
        "filled and capped clears the finding"
    );
    Ok(())
}
