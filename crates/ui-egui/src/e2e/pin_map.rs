//! Stories for pin map export: a newcomer exports every pin of the MCU as a
//! plain file and can see when a pin cannot take the function it was given.
use super::super::*;
use super::export::{file_export, in_format_list};
use super::support::*;
use pcbcraft_doc::{Component, EmbeddedFile, Net, Pin, Project};
use pcbcraft_geom::Point;

/// A board with an MCU and a firmware design file, like the sample but with
/// pins already assigned to functions.
fn firmware_project() -> Project {
    let design_text = include_str!("../../../library/tests/fixtures/firmware-design.toml");
    let mcu_text = include_str!("../../../library/tests/fixtures/stm32f405-fixture.mcu.toml");
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
    let mut app = App::create(shared.clone(), Preferences::default(), None, None);
    shared
        .lock()
        .map_err(|_| "the session is busy".to_string())?
        .project = firmware_project();
    app.refresh();
    app.change_tab(Tab::Pcb);
    app.needs_fit = true;
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    app.make_dir = dir.path().to_string_lossy().into_owned();
    let ctx = egui::Context::default();
    for _ in 0..4 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    file_export(&ctx, &mut app, "More…")?;
    assert!(
        in_format_list(&ctx, &mut app, "Pin map (CSV)")?,
        "Pin map (CSV) is listed under the formats"
    );
    authoring::tests::tap(&ctx, &mut app, "Pin map (CSV)")?;
    for _ in 0..4 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    authoring::tests::tap(&ctx, &mut app, "Export")?;
    for _ in 0..4 {
        frame(&ctx, &mut app, vec![], egui::Modifiers::NONE);
    }
    assert!(
        !app.message.to_lowercase().contains("error"),
        "the export completes without an error: {}",
        app.message
    );
    let path = dir.path().join("pin-map.csv");
    let written: Vec<String> = std::fs::read_dir(dir.path())
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        path.exists(),
        "the pin map file was written; the folder holds {written:?} and the status says: {}",
        app.message
    );
    let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    assert!(
        text.contains("pin,function,net"),
        "the export has the pin map header: {text}"
    );
    Ok(())
}
