//! Story from issue #980.
use pcbcraft_doc::{Component, EmbeddedFile, Project};
use pcbcraft_geom::Point;
use pcbcraft_plugin_firmware_target::{firmware_config_compare, firmware_target_files};

fn firmware_fixture_project() -> Project {
    let design_text = include_str!("fixtures/firmware-design.toml");
    let mcu_text = include_str!("fixtures/stm32f405-fixture.mcu.toml");
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
        pins: vec![],
        units: vec![],
        library_symbol: None,
        library_footprint: None,
        fields: vec![],
        sheet: 0,
    });
    project
}

#[test]
fn a_newcomer_exports_a_board_config_for_the_flight_controller_firmware() -> Result<(), String> {
    let project = firmware_fixture_project();
    let files = firmware_target_files(&project)?;
    let config = files
        .iter()
        .find(|(name, _)| name.ends_with("config.h"))
        .ok_or("config.h not exported")?;
    let body = String::from_utf8(config.1.clone()).map_err(|e| e.to_string())?;
    assert!(
        body.contains("GYRO_1_CS_PIN PA4") && body.contains("FLASH_CS_PIN PB12"),
        "config.h should name gyro and flash pins"
    );
    let mismatches =
        firmware_config_compare(&project, "#define GYRO_1_CS_PIN PA5\n")?;
    assert!(
        mismatches.iter().any(|m| m.contains("GYRO_1_CS_PIN")),
        "reverse check should report a swapped gyro chip-select pin"
    );
    Ok(())
}
