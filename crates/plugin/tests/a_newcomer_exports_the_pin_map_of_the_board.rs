//! Story from issue #846: export the detailed pin map of the board.
use pcbcraft_doc::{Component, EmbeddedFile, Net, Pin, Project};
use pcbcraft_geom::Point;
use pcbcraft_plugin::pin_map_detail;

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
fn a_newcomer_exports_the_pin_map_of_the_board() -> Result<(), String> {
    let project = firmware_fixture_project();
    let detail = pin_map_detail(&project)?;
    assert_eq!(detail.rows.len(), 10, "expected 10 pin rows");
    let pa5 = detail
        .rows
        .iter()
        .find(|r| r.pin == "PA5")
        .ok_or("PA5 row missing")?;
    assert_eq!(pa5.function, "SPI1_SCK");
    assert_eq!(pa5.net, "+5V");
    assert_eq!(pa5.instance, "SPI1");
    assert_eq!(pa5.bus, "SPI1");
    let pa4 = detail
        .rows
        .iter()
        .find(|r| r.pin == "PA4")
        .ok_or("PA4 row missing")?;
    assert_eq!(pa4.select, "PA4", "gyro cs on SPI1");
    let pb12 = detail
        .rows
        .iter()
        .find(|r| r.pin == "PB12")
        .ok_or("PB12 row missing")?;
    assert_eq!(pb12.select, "PB12", "flash cs on SPI2");
    assert_eq!(detail.unmapped, vec!["UNKNOWN_PART"]);
    Ok(())
}
