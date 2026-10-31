//! Export validation for fabrication packages (issue #415).

use pcbcraft_doc::Project;
use pcbcraft_geom::Point;
use pcbcraft_io::{
    ipc2581_text, odb_package, validate_fabrication_package_file, validate_ipc2581_document,
    validate_odb_package,
};

const BROKEN_IPC2581: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<IPC-2581 revision="C" xmlns="http://webstds.ipc.org/2581">
  <Content roleRef="Owner">
    <FunctionMode mode="NOT_A_MODE"/>
  </Content>
</IPC-2581>"#;

fn sample_odb_project() -> Result<Project, String> {
    let mut project = Project::default();
    project.name = "sample_board".into();
    project.nets.push(pcbcraft_doc::Net {
        id: 500,
        name: "GND".into(),
        ..Default::default()
    });
    project.board.tracks.push(pcbcraft_doc::Track {
        id: 501,
        net: 500,
        start: Point::new(5.0, 5.0),
        end: Point::new(40.0, 30.0),
        width: 0.3,
        layer: pcbcraft_doc::Layer::Front,
        arc: None,
    });
    project.board.vias.push(pcbcraft_doc::Via {
        id: 502,
        net: 500,
        position: Point::new(12.0, 12.0),
        diameter: 0.6,
        drill: 0.3,
        from_layer: None,
        to_layer: None,
        covering: None,
    });
    project.validate()?;
    Ok(project)
}

#[test]
fn ipc2581_schema_rejects_a_deliberately_broken_document() {
    let err = ipc2581::validate(BROKEN_IPC2581)
        .expect_err("invalid mode must fail schema validation")
        .to_string();
    assert!(
        err.to_ascii_lowercase().contains("schema"),
        "schema errors are reported: {err}"
    );
}

#[test]
fn sample_board_odb_package_passes_layout_validation() -> Result<(), String> {
    let bytes = odb_package(&sample_odb_project()?)?;
    validate_odb_package(&bytes)?;
    validate_fabrication_package_file("odb", "board.odb.zip", &bytes)?;
    Ok(())
}

#[test]
fn odb_validation_rejects_an_archive_without_the_job_tree() {
    let err = validate_odb_package(b"PK\x03\x04not-a-tree")
        .expect_err("random bytes are not an ODB++ package");
    assert!(
        err.contains("ZIP") || err.contains("corrupt") || err.contains("directory"),
        "zip problems are reported: {err}"
    );
}

#[test]
fn legacy_ipc2581_export_is_rejected_for_fabrication_packages() -> Result<(), String> {
    let text = ipc2581_text(&Project::default())?;
    assert!(
        validate_ipc2581_document(&text).is_err(),
        "the legacy writer is not schema-valid yet"
    );
    assert!(
        validate_fabrication_package_file("ipc2581", "board.xml", text.as_bytes()).is_err(),
        "packages refuse legacy IPC-2581 output"
    );
    Ok(())
}
