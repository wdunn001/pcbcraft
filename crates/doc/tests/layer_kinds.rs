//! Physical stack row kinds and copper links (issue #652).
use pcbcraft_doc::{LayerRecord, Project};

fn row(name: &str, kind: &str, thickness: f64) -> LayerRecord {
    LayerRecord {
        name: name.into(),
        kind: kind.into(),
        thickness,
        ..Default::default()
    }
}

#[test]
fn the_new_kinds_are_accepted_and_unknown_kinds_still_rejected() {
    for kind in [
        "adhesive",
        "coverlay",
        "stiffener",
        "peelable_mask",
        "carbon_ink",
        "gold_mask",
        "plating",
        "finish",
    ] {
        let mut p = Project::default();
        p.board.layer_records = vec![row("Row", kind, 0.05)];
        assert!(p.validate().is_ok(), "kind {kind} should validate");
        assert_eq!(p.board.layer_records[0].function(), kind);
    }
    let mut bad = Project::default();
    bad.board.layer_records = vec![row("Row", "mystery", 0.05)];
    assert!(bad.validate().is_err());
}

#[test]
fn a_mask_row_must_name_an_existing_copper_row() {
    let mut p = Project::default();
    p.board.layer_records = vec![
        row("F.Mask", "soldermask", 0.02),
        row("F.Cu", "copper", 0.035),
        row("Core", "core", 1.5),
        row("B.Cu", "copper", 0.035),
    ];
    p.board.layer_records[0].linked_copper = Some("Missing".into());
    assert!(p.validate().is_err());
    p.board.layer_records[0].linked_copper = Some("F.Cu".into());
    assert!(p.validate().is_ok());
}

#[test]
fn the_default_copper_for_a_mask_is_the_nearest_outer_layer() {
    let mut p = Project::default();
    p.board.layer_records = vec![
        row("F.Mask", "peelable_mask", 0.01),
        row("F.Cu", "copper", 0.035),
        row("Core", "core", 1.5),
        row("B.Cu", "copper", 0.035),
        row("B.Mask", "gold_mask", 0.02),
    ];
    assert_eq!(p.linked_copper("F.Mask").as_deref(), Some("F.Cu"));
    assert_eq!(p.linked_copper("B.Mask").as_deref(), Some("B.Cu"));
    assert_eq!(p.linked_copper("Core"), None);
    assert_eq!(p.linked_copper("Stiffener"), None);
}
