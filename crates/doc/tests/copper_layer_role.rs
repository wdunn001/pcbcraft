use pcbcraft_doc::{Layer, Project};

#[test]
fn copper_layer_roles_round_trip_and_validate() -> Result<(), String> {
    let mut p = Project::default();
    p.board.layer_records = ["F.Cu", "In1.Cu", "In2.Cu", "B.Cu"]
        .into_iter()
        .map(|name| pcbcraft_doc::LayerRecord {
            name: name.into(),
            kind: "copper".into(),
            thickness: 0.035,
            ..Default::default()
        })
        .collect();
    let layer = Layer::from_index(1)?;
    p.set_copper_layer_plane_role(layer, "ground")?;
    p.validate()?;
    let json = serde_json::to_string(&p)?;
    assert!(
        json.contains("\"copper_layer_roles\"") && json.contains("ground"),
        "roles persist in the saved design"
    );
    let back: Project = serde_json::from_str(&json)?;
    assert_eq!(back.copper_layer_plane_role(layer), Some("ground"));
    back.validate()?;
    Ok(())
}
