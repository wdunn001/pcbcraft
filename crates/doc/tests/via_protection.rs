use pcbcraft_doc::{Net, Point, Project, Via, ViaCovering};

#[test]
fn a_file_saved_before_the_side_setting_loads_with_the_same_covering() -> Result<(), String> {
    let mut p = Project::default();
    p.nets.push(Net {
        id: 1,
        name: "GND".into(),
        ..Default::default()
    });
    p.board.via_covering_default = ViaCovering::Tented;
    p.board.vias.push(Via {
        id: 2,
        net: 1,
        position: Point::new(5.0, 5.0),
        diameter: 0.6,
        drill: 0.3,
        from_layer: None,
        to_layer: None,
        covering: Some(ViaCovering::Plugged),
        covering_scope: None,
    });
    let without_scope = serde_json::to_string(&p).map_err(|e| e.to_string())?;
    assert!(
        !without_scope.contains("covering_scope"),
        "legacy files omit the side field"
    );
    let loaded: Project = serde_json::from_str(&without_scope).map_err(|e| e.to_string())?;
    assert_eq!(
        loaded.board.vias[0].covering,
        Some(ViaCovering::Plugged)
    );
    assert_eq!(loaded.board.vias[0].covering_scope, None);
    assert_eq!(
        loaded.board.vias[0].gerber_via_drill(&loaded.board),
        ViaCovering::Plugged.gerber_via_drill(pcbcraft_doc::ViaCoveringScope::Both)
    );
    Ok(())
}
