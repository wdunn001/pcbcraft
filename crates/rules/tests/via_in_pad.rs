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
        rotation: 0.0,
        locked: None,
        side: None,
        pads: vec![Pad {
            number: "1".into(),
            offset: Point::default(),
net: Some(1),
            diameter: 1.6,
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
        covering: Some(covering),
        covering_scope: None,
    });
    p.validate()?;
    Ok(p)
}
        diameter: 0.5,
        drill: 0.2,
        from_layer: None,
        to_layer: None,
covering: Some(covering),
        covering_bottom: bottom,
        covering_scope: None,
    });
    p.validate()?;
    Ok(p)
}

fn found(p: &Project) -> Vec<Id> {
    pcbcraft_rules::drc(p)
        .into_iter()
        .filter(|f| f.code == "via_in_pad_not_capped")
        .flat_map(|f| f.objects)
        .collect()
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
}

#[test]
fn a_via_beside_a_pad_or_buried_under_it_is_not_reported() -> Result<(), String> {
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
fn capping_one_face_with_the_other_left_open_still_counts_as_type_vii() -> Result<(), String> {
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
    Ok(())
}
