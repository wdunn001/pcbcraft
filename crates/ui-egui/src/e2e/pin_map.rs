//! Stories for pin map export: a newcomer exports every pin of the MCU as a
//! plain file and can see when a pin cannot take the function it was given.
use super::super::*;
use super::export::file_export;
use super::support::*;
use pcbcraft_doc::{Component, EmbeddedFile, Net, Pin, Project};
use pcbcraft_geom::Point;

/// A board with an MCU and a firmware design file, like the sample but with
/// pins already assigned to functions.
fn firmware_project() -> Project {
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
        kind: "mcu".into(),
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
    project
}

#[test]
fn a_newcomer_exports_the_pin_map_of_the_board() -> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    app.project = firmware_project();
    app.change_tab(Tab::Pcb);
    app.needs_fit = true;
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    app.make_dir = dir.path().to_string_lossy().into_owned();
    let ctx = egui::Context::default();
    for _ in 0..4 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    file_export(&ctx, &mut app, "Pin map (CSV)")?;
    authoring::tests::tap(&ctx, &mut app, "Export")?;
    assert!(
        !app.message.to_lowercase().contains("error"),
        "the export completes without an error: {}",
        app.message
    );
    let path = dir.path().join("pin-map.csv");
    assert!(path.exists(), "the pin map file was written");
    let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    assert!(
        text.contains("pin,function,net"),
        "the export has the pin map header: {text}"
    );
    Ok(())
}

#[test]
fn a_newcomer_is_told_a_pin_cannot_do_the_function_it_was_given_in_the_findings()
-> Result<(), String> {
    let shared = pcbcraft_mcp::shared(pcbcraft_engine::Session::default());
    let mut app = App::create(shared, Preferences::default(), None, None);
    app.refresh();
    let mut project = firmware_project();
    // Give PA5 a function it cannot take: CAN1_TX instead of SPI1_SCK.
    project.files[0].data = project.files[0]
        .data
        .replace("function = \"SPI1_SCK\"", "function = \"CAN1_TX\"");
    app.project = project;
    app.change_tab(Tab::Pcb);
    app.needs_fit = true;
    let ctx = egui::Context::default();
    for _ in 0..4 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    // Open the Findings list to see the check results.
    app.change_tab(Tab::Findings);
    for _ in 0..4 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    assert!(
        paints_text(&ctx, &mut app, "cannot take CAN1_TX")?,
        "the findings say the pin cannot take the function: {}",
        app.message
    );
    Ok(())
}
