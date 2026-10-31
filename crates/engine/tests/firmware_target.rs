//! Issue #846: peripheral checks and firmware target export.
use pcbcraft_doc::{Component, EmbeddedFile, Net, Pin, Project};
use pcbcraft_engine::Session;
use pcbcraft_geom::Point;

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
            },
            Pin {
                number: "PB2".into(),
                name: "BOOT1".into(),
                offset: Point { x: 0.0, y: 0.0 },
                net: Some(11),
                electrical_type: "input".into(),
                direction: None,
                unit: 1,
            },
        ],
        units: vec![],
        library_symbol: None,
        library_footprint: None,
        fields: vec![],
        sheet: 0,
    });
    project.nets.push(Net {
        id: 10,
        name: "+5V".into(),
        ..Net::default()
    });
    project.nets.push(Net {
        id: 11,
        name: "+3V3".into(),
        ..Net::default()
    });
    project
}

#[test]
fn a_newcomer_is_told_a_pin_cannot_do_the_function_it_was_given() -> Result<(), String> {
    let mut project = firmware_fixture_project();
    project.files[0].data = project.files[0]
        .data
        .replace("function = \"SPI1_SCK\"", "function = \"CAN1_TX\"");
    let mut session = Session::default();
    session.project = project;
    let peripheral = session.checks()["peripheral"]
        .as_array()
        .ok_or("peripheral findings missing")?;
    assert!(
        peripheral.iter().any(|f| {
            f.get("code")
                .and_then(|c| c.as_str())
                .is_some_and(|c| c == "peripheral_invalid_function")
        }),
        "expected invalid function finding"
    );
    Ok(())
}

#[test]
fn a_newcomer_exports_a_board_config_for_the_flight_controller_firmware() -> Result<(), String> {
    let project = firmware_fixture_project();
    let files = pcbcraft_io::firmware_target_files(&project)?;
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
        pcbcraft_io::firmware_config_compare(&project, "#define GYRO_1_CS_PIN PA5\n")?;
    assert!(
        mismatches.iter().any(|m| m.contains("GYRO_1_CS_PIN")),
        "reverse check should report a swapped gyro chip-select pin"
    );
    Ok(())
}
