use pcbcraft_engine::{Error, Session};
use serde_json::json;

fn missing() -> Error {
    Error::Invalid("missing field".into())
}

fn library_fixture(name: &str) -> String {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../library/tests/fixtures")
        .join(name)
        .to_string_lossy()
        .into_owned()
}

#[test]
fn place_library_resistor_gets_resistor_kind() -> Result<(), Error> {
    let mut s = Session::default();
    s.execute(
        "component.place_library",
        json!({"path":library_fixture("device.kicad_sym"),"symbol":"R","position":{"x":20.0,"y":15.0}}),
        None,
    )?;
    assert_eq!(s.project.components[0].kind, "resistor");
    Ok(())
}

#[test]
fn set_kind_changes_missing_decoupling_on_checks() -> Result<(), Error> {
    let mut s = Session::default();
    let sym_path = library_fixture("lm358.kicad_sym");
    let chip = s.execute(
        "component.place_library",
        json!({"path":sym_path,"symbol":"LM358","position":{"x":20.0,"y":15.0}}),
        None,
    )?["result"]["id"]
        .as_u64()
        .ok_or_else(missing)?;
    let rail = s.execute("net.create", json!({"name":"VCC"}), None)?["result"]["id"]
        .as_u64()
        .ok_or_else(missing)?;
    s.execute(
        "net.connect",
        json!({"net":rail,"pins":[{"component":chip,"pin":"8"}]}),
        None,
    )?;
    let before = s.checks();
    let erc_before = before["erc"].as_array().ok_or_else(missing)?;
    assert!(
        erc_before
            .iter()
            .any(|f| f["code"] == "missing_decoupling"),
        "ic-like part with power_in should need decoupling"
    );
    s.execute(
        "component.set_kind",
        json!({"id":chip,"kind":"capacitor"}),
        None,
    )?;
    let after = s.checks();
    let erc_after = after["erc"].as_array().ok_or_else(missing)?;
    assert!(
        !erc_after
            .iter()
            .any(|f| f["code"] == "missing_decoupling"),
        "capacitor kind should not trigger ic decoupling rule"
    );
    Ok(())
}

#[test]
fn set_kind_rejects_unknown_slug() -> Result<(), Error> {
    let mut s = Session::default();
    s.execute(
        "component.add",
        json!({"kind":"resistor","reference":"R1","value":"1k","position":{"x":10.0,"y":10.0}}),
        None,
    )?;
    let id = s.project.components[0].id;
    assert!(
        s.execute(
            "component.set_kind",
            json!({"id":id,"kind":"not_a_real_kind"}),
            None,
        )
        .is_err()
    );
    Ok(())
}
