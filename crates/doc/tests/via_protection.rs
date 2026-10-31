use pcbcraft_doc::{Net, Point, Project, Via, ViaCovering, ViaProtectionType, protection_for_faces, protection_type};
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
    Ok(())
}
