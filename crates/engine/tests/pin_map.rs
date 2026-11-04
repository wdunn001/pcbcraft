//! Issue #846: pin map export and reverse check.
use pcbcraft_doc::{Component, EmbeddedFile, Net, Pin, Project};
use pcbcraft_engine::Session;
use pcbcraft_geom::Point;
use serde_json::{Value, json};

fn firmware_fixture_project() -> Project {
    let design_text = include_str!("../../library/tests/fixtures/firmware-design.toml");
    let mcu_text = include_str!("../../library/tests/fixtures/stm32f405-fixture.mcu.toml");
    let mut project = Project::default();
    project.files.push(EmbeddedFile {
        name: "firmware-design.toml".into(),
        data: design_text.into(),
        encoding: String::new(),
    });
    project.files.push(EmbeddedFile {
        name: "stm32f405-fixture.mcu.toml".into(),
        data: mcu_text.into(),
        encoding: String::new(),
    });
    project.components.push(Component {
        id: 1,
        reference: "U1".into(),
        value: "STM32F405".into(),
        kind: "ic_mcu".into(),
        position: Point { x: 0.0, y: 0.0 },
        rotation: 0.0,
        pins: vec![
            Pin {
                number: "PA5".into(),
                name: "PA5".into(),
                offset: Point { x: 0.0, y: 0.0 },
                net: Some(10),
                electrical_type: "bidirectional".into(),
                direction: None,
                unit: 1,
                appearances: Default::default(),
            },
            Pin {
                number: "PA6".into(),
                name: "PA6".into(),
                offset: Point { x: 0.0, y: 0.0 },
                net: Some(10),
                electrical_type: "bidirectional".into(),
                direction: None,
                unit: 1,
                appearances: Default::default(),
            },
            Pin {
                number: "PA4".into(),
                name: "PA4".into(),
                offset: Point { x: 0.0, y: 0.0 },
                net: Some(11),
                electrical_type: "input".into(),
                direction: None,
                unit: 1,
                appearances: Default::default(),
            },
            Pin {
                number: "PB12".into(),
                name: "PB12".into(),
                offset: Point { x: 0.0, y: 0.0 },
                net: Some(12),
                electrical_type: "input".into(),
                direction: None,
                unit: 1,
                appearances: Default::default(),
            },
        ],
        units: vec![],
        library_symbol: None,
        library_footprint: None,
        fields: vec![],
        sheet: 0,
        compliance: Default::default(),
        include_in_bom: true,
        include_in_placement: true,
        instance: Default::default(),
        part: None,
        prototype: Default::default(),
        sourcing: Default::default(),
    });
    project.nets.push(Net {
        id: 10,
        name: "+5V".into(),
        ..Net::default()
    });
    project.nets.push(Net {
        id: 11,
        name: "GYRO_CS".into(),
        ..Net::default()
    });
    project.nets.push(Net {
        id: 12,
        name: "FLASH_CS".into(),
        ..Net::default()
    });
    project
}

#[test]
fn pinmap_export_returns_csv_and_check_reports_a_swap() -> Result<(), String> {
    let project = firmware_fixture_project();
    let mut session = Session::default();
    session.project = project;

    let export = session
        .execute("pinmap.export", json!({"format": "csv"}), None)
        .map_err(|e| format!("export failed: {e}"))?;
    let text = export
        .get("text")
        .and_then(Value::as_str)
        .ok_or("export has no text")?;
    assert!(
        text.starts_with("pin,function,net"),
        "csv must start with header"
    );
    let lines = text.lines().collect::<Vec<_>>();
    assert_eq!(
        lines.len(),
        12,
        "expected 12 lines (header + 10 rows + 1 unmapped)"
    );

    let swapped = text
        .replace("PA5,SPI1_SCK", "PA5,SPI1_MISO")
        .replace("PA6,SPI1_MISO", "PA6,SPI1_SCK");
    let check = session
        .execute("pinmap.check", json!({"text": swapped}), None)
        .map_err(|e| format!("check failed: {e}"))?;
    let count = check
        .get("count")
        .and_then(Value::as_u64)
        .ok_or("check has no count")?;
    assert_eq!(count, 2, "expected 2 mismatches for swapped pins");
    Ok(())
}

#[test]
fn pinmap_commands_refuse_empty_text() -> Result<(), String> {
    let project = firmware_fixture_project();
    let mut session = Session::default();
    session.project = project;
    let before = session.inspect()["revision"].clone();
    let result = session.execute("pinmap.check", json!({"text": ""}), None);
    assert!(result.is_err(), "expected error for empty text");
    let after = session.inspect()["revision"].clone();
    assert_eq!(before, after, "revision must not change");
    Ok(())
}

#[test]
fn a_newcomer_exports_the_pin_map_of_the_board() -> Result<(), String> {
    let project = firmware_fixture_project();
    let mut session = Session::default();
    session.project = project;
    let export = session
        .execute("pinmap.export", json!({"format": "json"}), None)
        .map_err(|e| format!("export failed: {e}"))?;
    let rows = export
        .get("rows")
        .and_then(Value::as_u64)
        .ok_or("export has no rows")?;
    assert_eq!(rows, 10, "expected 10 pin rows");
    let text = export
        .get("text")
        .and_then(Value::as_str)
        .ok_or("export has no text")?;
    let doc: Value = serde_json::from_str(text).map_err(|e| format!("json parse failed: {e}"))?;
    let json_rows = doc
        .get("rows")
        .and_then(Value::as_array)
        .ok_or("rows missing from json")?;
    assert_eq!(json_rows.len(), 10, "expected 10 rows in json");
    Ok(())
}
