//! Tests for the physical layer record and the built-in material table.
use pcbcraft_doc::{LayerRecord, MATERIALS, Project, material};
use serde_json::Value;

type T = Result<(), String>;

fn row(name: &str, kind: &str, thickness: f64) -> LayerRecord {
    LayerRecord {
        name: name.into(),
        kind: kind.into(),
        thickness,
        ..Default::default()
    }
}

#[test]
fn an_old_project_without_the_new_fields_loads_and_saves_unchanged() -> T {
    let mut p = Project::default();
    p.board.layer_records = vec![row("F.Cu", "copper", 0.035), row("Core", "core", 1.5)];
    p.board.layer_records[0].copper_role = Some("signal".into());
    let first = serde_json::to_value(&p).map_err(|e| e.to_string())?;
    let rows = first["board"]["layer_records"]
        .as_array()
        .ok_or("no layer_records")?;
    for r in rows {
        let keys: Vec<&String> = r.as_object().ok_or("row")?.keys().collect();
        assert!(
            keys.iter()
                .all(|k| matches!(k.as_str(), "name" | "kind" | "thickness" | "copper_role")),
            "new fields leaked into an old-shape row: {keys:?}"
        );
    }
    let text = serde_json::to_string(&first).map_err(|e| e.to_string())?;
    let back: Project = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let second: Value = serde_json::to_value(&back).map_err(|e| e.to_string())?;
    assert_eq!(first, second);
    assert_eq!(back.board.layer_records[1].function(), "dielectric_core");
    Ok(())
}

#[test]
fn a_layer_record_rejects_a_dk_below_one_or_a_negative_df() {
    for bad in [
        |r: &mut LayerRecord| r.dk = Some(0.5),
        |r: &mut LayerRecord| r.dk = Some(21.0),
        |r: &mut LayerRecord| r.df = Some(-0.01),
        |r: &mut LayerRecord| r.df = Some(1.5),
        |r: &mut LayerRecord| r.thickness_tol_pct = Some(60.0),
        |r: &mut LayerRecord| r.copper_weight_oz = Some(0.0),
        |r: &mut LayerRecord| r.foil = Some("shiny".into()),
    ] {
        let mut p = Project::default();
        let mut r = row("Core", "core", 1.0);
        bad(&mut r);
        p.board.layer_records = vec![r];
        assert!(p.validate().is_err());
    }
    let mut p = Project::default();
    let mut r = row("Core", "core", 1.0);
    r.dk = Some(4.2);
    r.df = Some(0.02);
    p.board.layer_records = vec![r];
    assert!(p.validate().is_ok());
}

#[test]
fn the_built_in_material_table_has_a_source_note_on_every_entry() {
    assert!(!MATERIALS.is_empty());
    for m in MATERIALS {
        assert!(!m.source.trim().is_empty(), "{} has no source note", m.name);
        assert!(m.frequency_mhz > 0.0, "{} has no frequency", m.name);
        assert!((1.0..=20.0).contains(&m.dk) && (0.0..=1.0).contains(&m.df));
    }
}

#[test]
fn a_named_material_fills_dk_and_df_but_an_explicit_value_wins() -> T {
    let table = material("FR-4 standard").ok_or("missing FR-4 standard")?;
    let mut r = row("Core", "core", 1.0);
    r.material = Some("FR-4 standard".into());
    assert_eq!(r.dk(), Some(table.dk));
    assert_eq!(r.df(), Some(table.df));
    r.dk = Some(3.9);
    assert_eq!(r.dk(), Some(3.9));
    assert_eq!(r.df(), Some(table.df));
    assert_eq!(row("Core", "core", 1.0).dk(), None);
    Ok(())
}

#[test]
fn mask_silk_and_paste_rows_link_to_the_copper_they_sit_on() {
    let mut p = Project::default();
    p.board.layer_records = vec![
        row("F.Mask", "soldermask", 0.02),
        row("F.Cu", "copper", 0.035),
        row("Core", "core", 1.5),
        row("B.Cu", "copper", 0.035),
        row("B.Mask", "soldermask", 0.02),
    ];
    assert_eq!(p.linked_copper("F.Mask").as_deref(), Some("F.Cu"));
    assert_eq!(p.linked_copper("B.Mask").as_deref(), Some("B.Cu"));
    assert_eq!(p.layer_side("F.Mask").as_deref(), Some("top"));
    assert_eq!(p.layer_side("Core").as_deref(), Some("internal"));
    assert_eq!(p.layer_side("B.Mask").as_deref(), Some("bottom"));
    p.board.layer_records[0].linked_copper = Some("Nope".into());
    assert!(p.validate().is_err());
}
